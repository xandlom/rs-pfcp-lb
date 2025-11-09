//! PFCP Proxy/Load Balancer
//!
//! A production-grade PFCP proxy/load balancer that distributes sessions across
//! multiple UPF (User Plane Function) backends while maintaining session affinity
//! and protocol compliance per 3GPP TS 29.244.
//!
//! # Architecture
//!
//! ```text
//! SMF (Client) ←→ [PFCP Proxy/LB] ←→ UPF Pool (Multiple Backends)
//! ```
//!
//! # Features
//!
//! - **Session Affinity**: SEID-based routing ensures all messages for a session go to the same UPF
//! - **Load Balancing**: Configurable strategies (round-robin, least-sessions, weighted)
//! - **Health Monitoring**: Heartbeat broadcasting and health status tracking
//! - **Statistics**: Comprehensive metrics collection and reporting
//! - **High Performance**: Async I/O with Tokio, zero-copy where possible

use clap::Parser;
use rs_pfcp::message::{self, MsgType};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

mod config;
mod health;
mod routing;
mod session;
mod statistics;

use health::{HealthMonitor, HealthStatus};
use routing::{LoadBalancingStrategy, RoutingDecision};
use session::{PendingRequests, SessionTable};
use statistics::Statistics;

/// Command-line arguments
#[derive(Parser, Debug)]
#[command(name = "pfcp-proxy")]
#[command(about = "PFCP Proxy/Load Balancer", long_about = None)]
#[command(version)]
struct Args {
    /// Listen address (e.g., 0.0.0.0:8805)
    #[arg(short, long, default_value = "0.0.0.0:8805")]
    listen: String,

    /// Comma-separated list of UPF backend addresses
    /// Example: 10.0.1.10:8805,10.0.1.11:8805,10.0.1.12:8805
    #[arg(short, long, value_delimiter = ',')]
    backends: Vec<String>,

    /// Load balancing strategy: round-robin, least-sessions, weighted
    #[arg(long, default_value = "round-robin")]
    strategy: String,

    /// Statistics reporting interval in seconds
    #[arg(long, default_value = "10")]
    stats_interval: u64,

    /// Health check interval in seconds
    #[arg(long, default_value = "5")]
    health_check_interval: u64,

    /// Configuration file path (optional)
    #[arg(short, long)]
    config: Option<String>,

    /// Log level: trace, debug, info, warn, error
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Enable JSON logging
    #[arg(long)]
    json_logs: bool,
}

// =============================================================================
// UPF Pool Management
// =============================================================================

/// UPF backend pool with configurable load balancing
/// Supports dynamic addition/removal of backends at runtime (up to 64 UPFs)
struct UpfPool {
    backends: Arc<RwLock<Vec<UpfBackend>>>,
    next_index: AtomicUsize,
    strategy: LoadBalancingStrategy,
    max_backends: usize,
}

#[derive(Clone, Debug)]
struct UpfBackend {
    addr: SocketAddr,
    health: Arc<RwLock<HealthStatus>>,
    weight: u32,
}

impl UpfPool {
    fn new(backends: Vec<SocketAddr>, strategy: LoadBalancingStrategy) -> Self {
        let backends = backends
            .into_iter()
            .map(|addr| UpfBackend {
                addr,
                health: Arc::new(RwLock::new(HealthStatus::Unknown)),
                weight: 1,
            })
            .collect();

        Self {
            backends: Arc::new(RwLock::new(backends)),
            next_index: AtomicUsize::new(0),
            strategy,
            max_backends: 64,
        }
    }

    /// Add a new UPF backend at runtime
    pub async fn add_backend(&self, addr: SocketAddr) -> Result<(), String> {
        let mut backends = self.backends.write().await;

        // Check if we've reached the maximum
        if backends.len() >= self.max_backends {
            return Err(format!("Maximum number of backends ({}) reached", self.max_backends));
        }

        // Check if backend already exists
        if backends.iter().any(|b| b.addr == addr) {
            return Err(format!("Backend {} already exists", addr));
        }

        backends.push(UpfBackend {
            addr,
            health: Arc::new(RwLock::new(HealthStatus::Unknown)),
            weight: 1,
        });

        info!("Added UPF backend: {}", addr);
        Ok(())
    }

    /// Remove a UPF backend at runtime
    pub async fn remove_backend(&self, addr: SocketAddr) -> Result<(), String> {
        let mut backends = self.backends.write().await;

        let original_len = backends.len();
        backends.retain(|b| b.addr != addr);

        if backends.len() == original_len {
            return Err(format!("Backend {} not found", addr));
        }

        info!("Removed UPF backend: {}", addr);
        Ok(())
    }

    /// Get count of current backends
    pub async fn backend_count(&self) -> usize {
        self.backends.read().await.len()
    }

    /// Get list of all backend addresses
    pub async fn list_backends(&self) -> Vec<SocketAddr> {
        self.backends.read().await.iter().map(|b| b.addr).collect()
    }

    /// Select next UPF based on configured strategy
    async fn select_upf(&self, session_table: &SessionTable) -> Option<SocketAddr> {
        match self.strategy {
            LoadBalancingStrategy::RoundRobin => self.select_round_robin().await,
            LoadBalancingStrategy::LeastSessions => {
                self.select_least_sessions(session_table).await
            }
            LoadBalancingStrategy::Weighted => self.select_weighted().await,
        }
    }

    async fn select_round_robin(&self) -> Option<SocketAddr> {
        let healthy: Vec<_> = self.healthy_backends().await;
        if healthy.is_empty() {
            return None;
        }

        let idx = self.next_index.fetch_add(1, Ordering::Relaxed) % healthy.len();
        Some(healthy[idx].addr)
    }

    async fn select_least_sessions(&self, session_table: &SessionTable) -> Option<SocketAddr> {
        let healthy = self.healthy_backends().await;
        if healthy.is_empty() {
            return None;
        }

        let mut min_sessions = usize::MAX;
        let mut selected = None;

        for backend in healthy {
            let count = session_table.count_by_upf(backend.addr).await;
            if count < min_sessions {
                min_sessions = count;
                selected = Some(backend.addr);
            }
        }

        selected
    }

    async fn select_weighted(&self) -> Option<SocketAddr> {
        // Weighted round-robin implementation
        // For simplicity, treating weight as probability multiplier
        let healthy = self.healthy_backends().await;
        if healthy.is_empty() {
            return None;
        }

        let total_weight: u32 = healthy.iter().map(|b| b.weight).sum();
        if total_weight == 0 {
            return self.select_round_robin().await;
        }

        let idx = self.next_index.fetch_add(1, Ordering::Relaxed);
        let weighted_idx = idx % total_weight as usize;

        let mut cumulative = 0;
        for backend in healthy {
            cumulative += backend.weight as usize;
            if weighted_idx < cumulative {
                return Some(backend.addr);
            }
        }

        None
    }

    async fn healthy_backends(&self) -> Vec<UpfBackend> {
        let backends = self.backends.read().await;
        let mut result = Vec::new();
        for backend in backends.iter() {
            let health = backend.health.read().await;
            if matches!(*health, HealthStatus::Healthy | HealthStatus::Unknown) {
                result.push(backend.clone());
            }
        }
        result
    }

    async fn all_backends(&self) -> Vec<UpfBackend> {
        self.backends.read().await.clone()
    }

    async fn update_health(&self, addr: SocketAddr, status: HealthStatus) {
        let backends = self.backends.read().await;
        for backend in backends.iter() {
            if backend.addr == addr {
                *backend.health.write().await = status;
                break;
            }
        }
    }
}

// =============================================================================
// Message Routing Logic
// =============================================================================

/// Determine if a message type is a request from SMF (vs response from UPF)
fn is_smf_request(msg_type: MsgType) -> bool {
    matches!(
        msg_type,
        MsgType::HeartbeatRequest
            | MsgType::AssociationSetupRequest
            | MsgType::AssociationUpdateRequest
            | MsgType::AssociationReleaseRequest
            | MsgType::PfdManagementRequest
            | MsgType::NodeReportRequest
            | MsgType::SessionEstablishmentRequest
            | MsgType::SessionModificationRequest
            | MsgType::SessionDeletionRequest
            | MsgType::SessionSetDeletionRequest
            | MsgType::SessionSetModificationRequest
    )
}

/// Determine if a message type is a request from UPF (special case - reversed flow)
fn is_upf_request(msg_type: MsgType) -> bool {
    matches!(
        msg_type,
        MsgType::SessionReportRequest // UPF-initiated: UPF reports usage/events to SMF
    )
}

/// Route a PFCP message to appropriate backend(s)
async fn route_message(
    msg_type: MsgType,
    seid: Option<u64>,
    smf_addr: SocketAddr,
    session_table: &SessionTable,
    upf_pool: &UpfPool,
    stats: &Statistics,
) -> RoutingDecision {
    match msg_type {
        // Broadcast heartbeats to all UPFs for health monitoring
        MsgType::HeartbeatRequest => {
            stats.record_routing_decision(false, true);
            let backends: Vec<_> = upf_pool
                .all_backends()
                .await
                .iter()
                .map(|b| b.addr)
                .collect();
            RoutingDecision::Broadcast(backends)
        }

        // Node-level messages: send to all backends
        MsgType::AssociationSetupRequest
        | MsgType::AssociationUpdateRequest
        | MsgType::PfdManagementRequest => {
            stats.record_routing_decision(false, true);
            let backends: Vec<_> = upf_pool
                .all_backends()
                .await
                .iter()
                .map(|b| b.addr)
                .collect();
            RoutingDecision::Broadcast(backends)
        }

        // Session establishment: load balance to select new UPF
        MsgType::SessionEstablishmentRequest => {
            if let Some(upf_addr) = upf_pool.select_upf(session_table).await {
                stats.record_routing_decision(false, false);
                stats.record_session_established();

                // Record SEID → (UPF, SMF) mapping
                if let Some(s) = seid {
                    session_table.insert(s, upf_addr, smf_addr).await;
                }

                RoutingDecision::Single(upf_addr)
            } else {
                RoutingDecision::NoBackend
            }
        }

        // Session-level messages: route by SEID affinity
        MsgType::SessionModificationRequest | MsgType::SessionDeletionRequest => {
            if let Some(s) = seid {
                if let Some(upf_addr) = session_table.lookup(s).await {
                    stats.record_routing_decision(true, false);

                    // Track session deletion
                    if msg_type == MsgType::SessionDeletionRequest {
                        stats.record_session_deleted();
                        session_table.remove(s).await;
                    }

                    RoutingDecision::Single(upf_addr)
                } else {
                    warn!(
                        "Session not found for SEID {:#x}, message type: {:?}",
                        s, msg_type
                    );
                    RoutingDecision::SessionNotFound
                }
            } else {
                warn!("Expected SEID for {:?} but none found", msg_type);
                RoutingDecision::NoSeid
            }
        }

        // Responses - handled separately
        MsgType::HeartbeatResponse
        | MsgType::SessionEstablishmentResponse
        | MsgType::SessionModificationResponse
        | MsgType::SessionDeletionResponse
        | MsgType::SessionReportResponse
        | MsgType::AssociationSetupResponse
        | MsgType::AssociationUpdateResponse
        | MsgType::AssociationReleaseResponse
        | MsgType::PfdManagementResponse
        | MsgType::NodeReportResponse
        | MsgType::SessionSetDeletionResponse
        | MsgType::SessionSetModificationResponse
        | MsgType::VersionNotSupportedResponse => RoutingDecision::ResponseToClient,

        // Other messages: try SEID routing or load balance
        _ => {
            if let Some(s) = seid {
                if let Some(upf_addr) = session_table.lookup(s).await {
                    stats.record_routing_decision(true, false);
                    RoutingDecision::Single(upf_addr)
                } else {
                    // SEID not found, load balance and record
                    if let Some(upf_addr) = upf_pool.select_upf(session_table).await {
                        stats.record_routing_decision(false, false);
                        session_table.insert(s, upf_addr, smf_addr).await;
                        RoutingDecision::Single(upf_addr)
                    } else {
                        RoutingDecision::NoBackend
                    }
                }
            } else {
                // No SEID, load balance
                if let Some(upf_addr) = upf_pool.select_upf(session_table).await {
                    stats.record_routing_decision(false, false);
                    RoutingDecision::Single(upf_addr)
                } else {
                    RoutingDecision::NoBackend
                }
            }
        }
    }
}

// =============================================================================
// Main Message Handler
// =============================================================================

async fn handle_message(
    data: Vec<u8>,
    src: SocketAddr,
    socket: Arc<UdpSocket>,
    session_table: SessionTable,
    upf_pool: Arc<UpfPool>,
    pending_requests: PendingRequests,
    stats: Arc<Statistics>,
) {
    // Parse message header
    let (msg_type, seid, sequence) = match message::parse(&data) {
        Ok(msg) => (msg.msg_type(), msg.seid(), msg.sequence()),
        Err(e) => {
            error!("Failed to parse PFCP message from {}: {}", src, e);
            return;
        }
    };

    // Record statistics
    stats.record_message_received(msg_type).await;

    debug!(
        "Received {:?} from {} (SEID: {:?}, seq: {})",
        msg_type, src, seid, sequence
    );

    // Handle UPF-initiated requests (reversed flow)
    if is_upf_request(msg_type) {
        handle_upf_request(
            data,
            src,
            seid,
            sequence,
            socket,
            session_table,
            pending_requests,
        )
        .await;
        return;
    }

    // Handle responses
    if !is_smf_request(msg_type) {
        handle_response(data, src, sequence, socket, upf_pool, pending_requests, stats).await;
        return;
    }

    // Handle SMF requests
    handle_smf_request(
        data,
        src,
        msg_type,
        seid,
        sequence,
        socket,
        session_table,
        upf_pool,
        pending_requests,
        stats,
    )
    .await;
}

async fn handle_upf_request(
    data: Vec<u8>,
    src: SocketAddr,
    seid: Option<u64>,
    sequence: u32,
    socket: Arc<UdpSocket>,
    session_table: SessionTable,
    pending_requests: PendingRequests,
) {
    if let Some(s) = seid {
        if let Some((upf_addr, smf_addr)) = session_table.lookup_full(s).await {
            if upf_addr == src {
                pending_requests.insert(sequence, src, false, true).await;

                if let Err(e) = socket.send_to(&data, smf_addr).await {
                    error!("Failed to forward to SMF {}: {}", smf_addr, e);
                    pending_requests.remove(sequence).await;
                } else {
                    debug!("Forwarded UPF request to SMF {}", smf_addr);
                }
            } else {
                warn!("Request from wrong UPF (expected {}, got {})", upf_addr, src);
            }
        } else {
            warn!("Session not found for SEID {:#x}", s);
        }
    } else {
        warn!("UPF request without SEID");
    }
}

async fn handle_response(
    data: Vec<u8>,
    src: SocketAddr,
    sequence: u32,
    socket: Arc<UdpSocket>,
    upf_pool: Arc<UpfPool>,
    pending_requests: PendingRequests,
    stats: Arc<Statistics>,
) {
    match pending_requests.lookup_and_increment(sequence).await {
        Some((origin_addr, is_broadcast, response_count, _from_upf)) => {
            if let Err(e) = socket.send_to(&data, origin_addr).await {
                error!("Failed to forward response to {}: {}", origin_addr, e);
            } else {
                let backend_count = if is_broadcast {
                    upf_pool.all_backends().await.len()
                } else {
                    1
                };

                debug!(
                    "Forwarded response to {} (response {}/{})",
                    origin_addr,
                    response_count,
                    backend_count
                );
                stats.record_response_forwarded();

                if !is_broadcast || response_count >= backend_count {
                    pending_requests.remove(sequence).await;
                }
            }
        }
        None => {
            debug!(
                "Received response from {} (seq: {}) - no pending request found",
                src, sequence
            );
            stats.record_response_dropped();
        }
    }
}

async fn handle_smf_request(
    data: Vec<u8>,
    src: SocketAddr,
    msg_type: MsgType,
    seid: Option<u64>,
    sequence: u32,
    socket: Arc<UdpSocket>,
    session_table: SessionTable,
    upf_pool: Arc<UpfPool>,
    pending_requests: PendingRequests,
    stats: Arc<Statistics>,
) {
    let decision = route_message(msg_type, seid, src, &session_table, &upf_pool, &stats).await;

    match decision {
        RoutingDecision::Single(upf_addr) => {
            pending_requests.insert(sequence, src, false, false).await;

            if let Err(e) = socket.send_to(&data, upf_addr).await {
                error!("Failed to forward to {}: {}", upf_addr, e);
                pending_requests.remove(sequence).await;
            } else {
                debug!("Forwarded to {}", upf_addr);
                stats.record_message_sent(upf_addr).await;
            }
        }

        RoutingDecision::Broadcast(backends) => {
            pending_requests.insert(sequence, src, true, false).await;

            debug!("Broadcasting to {} backends", backends.len());
            let mut sent_count = 0;
            for upf_addr in backends {
                if let Err(e) = socket.send_to(&data, upf_addr).await {
                    error!("Failed to broadcast to {}: {}", upf_addr, e);
                } else {
                    stats.record_message_sent(upf_addr).await;
                    sent_count += 1;
                }
            }

            if sent_count == 0 {
                pending_requests.remove(sequence).await;
            }
        }

        RoutingDecision::SessionNotFound => {
            warn!("Session not found in table");
        }

        RoutingDecision::NoSeid => {
            warn!("Expected SEID but none found");
        }

        RoutingDecision::NoBackend => {
            error!("No healthy backend available");
        }

        RoutingDecision::ResponseToClient => {
            warn!("Unexpected response classification for request");
        }
    }
}

// =============================================================================
// Main Proxy Server
// =============================================================================

async fn run_proxy(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    // Parse and resolve backend addresses (supports both IP addresses and hostnames)
    let mut backends = Vec::new();
    for backend_str in &args.backends {
        // Try to parse as SocketAddr first (for IP addresses)
        match backend_str.parse::<SocketAddr>() {
            Ok(addr) => {
                backends.push(addr);
            }
            Err(_) => {
                // If parsing fails, try DNS resolution (for hostnames like "upf1:8805")
                match tokio::net::lookup_host(backend_str).await {
                    Ok(mut addrs) => {
                        if let Some(addr) = addrs.next() {
                            info!("Resolved {} to {}", backend_str, addr);
                            backends.push(addr);
                        } else {
                            error!("Failed to resolve hostname: {}", backend_str);
                            eprintln!("Failed to resolve hostname: {}", backend_str);
                            std::process::exit(1);
                        }
                    }
                    Err(e) => {
                        error!("Failed to resolve {}: {}", backend_str, e);
                        eprintln!("Failed to resolve {}: {}", backend_str, e);
                        std::process::exit(1);
                    }
                }
            }
        }
    }

    if backends.is_empty() {
        error!("No backend UPF addresses specified");
        eprintln!("Use --backends flag to specify UPF addresses");
        eprintln!("Example: --backends 10.0.1.10:8805,10.0.1.11:8805");
        eprintln!("         --backends upf1:8805,upf2:8805,upf3:8805");
        std::process::exit(1);
    }

    // Parse load balancing strategy
    let strategy = match args.strategy.as_str() {
        "round-robin" => LoadBalancingStrategy::RoundRobin,
        "least-sessions" => LoadBalancingStrategy::LeastSessions,
        "weighted" => LoadBalancingStrategy::Weighted,
        _ => {
            error!("Invalid strategy: {}", args.strategy);
            std::process::exit(1);
        }
    };

    info!("Starting PFCP Proxy/Load Balancer");
    info!("  Listen address: {}", args.listen);
    info!("  Backend UPFs: {:?}", backends);
    info!("  Strategy: {:?}", strategy);

    // Initialize components
    let socket = Arc::new(UdpSocket::bind(&args.listen).await?);
    let session_table = SessionTable::new();
    let upf_pool = Arc::new(UpfPool::new(backends, strategy));
    let pending_requests = PendingRequests::new();
    let stats = Arc::new(Statistics::new());

    // Spawn statistics reporting task
    {
        let stats = stats.clone();
        let session_table = session_table.clone();
        let upf_pool = upf_pool.clone();
        let interval = args.stats_interval;

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(interval));
            loop {
                ticker.tick().await;
                stats.print_report(&session_table, &upf_pool).await;

                // Export stats to JSON for TUI consumption
                if let Err(e) = stats.export_to_json("/tmp/pfcp-proxy-stats.json", &session_table, &upf_pool).await {
                    warn!("Failed to export statistics to JSON: {}", e);
                }
            }
        });
    }

    // Spawn cleanup task for stale pending requests
    {
        let pending_requests = pending_requests.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(30));
            loop {
                ticker.tick().await;
                pending_requests
                    .cleanup_stale(Duration::from_secs(60))
                    .await;
            }
        });
    }

    // Spawn health monitor
    {
        let upf_pool = upf_pool.clone();
        let socket = socket.clone();
        let interval = args.health_check_interval;

        tokio::spawn(async move {
            HealthMonitor::run(upf_pool, socket, Duration::from_secs(interval)).await;
        });
    }

    // Spawn control file monitor for dynamic UPF management
    {
        let upf_pool = upf_pool.clone();
        tokio::spawn(async move {
            use tokio::fs;
            use tokio::io::AsyncReadExt;
            use std::path::Path;

            let control_file = Path::new(".upf_control");
            let mut last_size = 0u64;

            let mut ticker = tokio::time::interval(Duration::from_millis(500));
            loop {
                ticker.tick().await;

                // Check if control file exists and has grown
                if let Ok(metadata) = fs::metadata(control_file).await {
                    let current_size = metadata.len();
                    if current_size > last_size {
                        // Read new content
                        if let Ok(mut file) = fs::File::open(control_file).await {
                            let mut contents = String::new();
                            if file.read_to_string(&mut contents).await.is_ok() {
                                // Process commands
                                for line in contents.lines().skip((last_size as usize) / 20) { // Approximate line skip
                                    if let Some((action, addr)) = line.split_once(':') {
                                        match addr.parse::<SocketAddr>() {
                                            Ok(socket_addr) => {
                                                match action {
                                                    "add" => {
                                                        if let Err(e) = upf_pool.add_backend(socket_addr).await {
                                                            warn!("Failed to add UPF {}: {}", socket_addr, e);
                                                        } else {
                                                            info!("Dynamically added UPF backend: {}", socket_addr);
                                                        }
                                                    }
                                                    "remove" => {
                                                        if let Err(e) = upf_pool.remove_backend(socket_addr).await {
                                                            warn!("Failed to remove UPF {}: {}", socket_addr, e);
                                                        } else {
                                                            info!("Dynamically removed UPF backend: {}", socket_addr);
                                                        }
                                                    }
                                                    _ => warn!("Unknown UPF command: {}", action),
                                                }
                                            }
                                            Err(e) => warn!("Invalid socket address in control file '{}': {}", addr, e),
                                        }
                                    }
                                }
                            }
                        }
                        last_size = current_size;
                    }
                }
            }
        });
    }

    info!("Proxy listening on {}", args.listen);

    // Main message processing loop
    let mut buf = vec![0u8; 65536];
    loop {
        let (len, src) = socket.recv_from(&mut buf).await?;
        let data = buf[..len].to_vec();

        // Spawn task to handle message
        let socket = socket.clone();
        let session_table = session_table.clone();
        let upf_pool = upf_pool.clone();
        let pending_requests = pending_requests.clone();
        let stats = stats.clone();

        tokio::spawn(async move {
            handle_message(
                data,
                src,
                socket,
                session_table,
                upf_pool,
                pending_requests,
                stats,
            )
            .await;
        });
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Initialize logging
    let filter = args.log_level.clone();
    if args.json_logs {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }

    run_proxy(args).await
}
