//! Simple PFCP UPF Server for Testing
//!
//! A minimal UPF simulator that responds to PFCP messages for testing the proxy.

use clap::Parser;
use rs_pfcp::message;
use rs_pfcp::message::association_setup_response::AssociationSetupResponseBuilder;
use rs_pfcp::message::heartbeat_response::HeartbeatResponseBuilder;
use rs_pfcp::message::session_deletion_response::SessionDeletionResponseBuilder;
use rs_pfcp::message::session_establishment_response::SessionEstablishmentResponseBuilder;
use rs_pfcp::message::Message;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::net::UdpSocket;

#[derive(Parser, Debug)]
#[command(name = "test-upf")]
#[command(about = "Simple PFCP UPF Server for testing")]
struct Args {
    /// Listen address
    #[arg(short, long, default_value = "0.0.0.0:8805")]
    listen: String,

    /// UPF name/identifier
    #[arg(short, long, default_value = "test-upf-1")]
    name: String,
}

struct UpfStats {
    total_messages: AtomicU64,
    heartbeats: AtomicU64,
    sessions: AtomicU64,
}

impl UpfStats {
    fn new() -> Self {
        Self {
            total_messages: AtomicU64::new(0),
            heartbeats: AtomicU64::new(0),
            sessions: AtomicU64::new(0),
        }
    }

    fn print(&self, upf_name: &str) {
        println!("\n[{}] Statistics:", upf_name);
        println!("  Total Messages:      {}", self.total_messages.load(Ordering::Relaxed));
        println!("  Heartbeats:          {}", self.heartbeats.load(Ordering::Relaxed));
        println!("  Session Messages:    {}", self.sessions.load(Ordering::Relaxed));
    }
}

async fn handle_message(
    data: Vec<u8>,
    src: SocketAddr,
    socket: Arc<UdpSocket>,
    upf_name: String,
    stats: Arc<UpfStats>,
    local_ip: Ipv4Addr,
) -> Result<(), Box<dyn std::error::Error>> {
    // Parse message to get type and sequence, then drop it to avoid Send issues
    let (msg_type, sequence, seid) = {
        let msg = message::parse(&data)?;
        let msg_type = msg.msg_type();
        let sequence = msg.sequence();
        let seid = msg.seid();
        (msg_type, sequence, seid)
        // msg is dropped here
    };

    stats.total_messages.fetch_add(1, Ordering::Relaxed);

    println!(
        "[{}] 📥 Received {:?} from {} (seq: {}, SEID: {:?})",
        upf_name, msg_type, src, sequence, seid
    );

    // Build and send appropriate response
    let response_data: Option<Vec<u8>> = match msg_type {
        message::MsgType::HeartbeatRequest => {
            stats.heartbeats.fetch_add(1, Ordering::Relaxed);

            let response = HeartbeatResponseBuilder::new(sequence)
                .recovery_time_stamp(SystemTime::now())
                .build();

            println!("[{}]    📤 Sending HeartbeatResponse", upf_name);
            Some(response.marshal())
        }

        message::MsgType::AssociationSetupRequest => {
            let response = AssociationSetupResponseBuilder::new(sequence)
                .cause_accepted()
                .node_id(local_ip)
                .recovery_time_stamp(SystemTime::now())
                .build();

            println!("[{}]    📤 Sending AssociationSetupResponse (accepted)", upf_name);
            Some(response.marshal())
        }

        message::MsgType::SessionEstablishmentRequest => {
            stats.sessions.fetch_add(1, Ordering::Relaxed);
            if let Some(s) = seid {
                println!("[{}]    ✓ Session SEID: {:#x}", upf_name, s);

                match SessionEstablishmentResponseBuilder::accepted(s, sequence)
                    .fseid(s, local_ip)
                    .build()
                {
                    Ok(response) => {
                        println!("[{}]    📤 Sending SessionEstablishmentResponse (accepted)", upf_name);
                        Some(response.marshal())
                    }
                    Err(e) => {
                        println!("[{}]    ❌ Failed to build SessionEstablishmentResponse: {}", upf_name, e);
                        None
                    }
                }
            } else {
                println!("[{}]    ⚠️  SessionEstablishmentRequest missing SEID", upf_name);
                None
            }
        }

        message::MsgType::SessionDeletionRequest => {
            stats.sessions.fetch_add(1, Ordering::Relaxed);
            if let Some(s) = seid {
                println!("[{}]    ✓ Deleting Session SEID: {:#x}", upf_name, s);

                let response = SessionDeletionResponseBuilder::new(s, sequence)
                    .cause_accepted()
                    .build();

                println!("[{}]    📤 Sending SessionDeletionResponse (accepted)", upf_name);
                Some(response.marshal())
            } else {
                println!("[{}]    ⚠️  SessionDeletionRequest missing SEID", upf_name);
                None
            }
        }

        message::MsgType::SessionModificationRequest => {
            stats.sessions.fetch_add(1, Ordering::Relaxed);
            if let Some(s) = seid {
                println!("[{}]    ✓ Session SEID: {:#x}", upf_name, s);
            }
            println!("[{}]    ⚠️  SessionModificationResponse not implemented yet", upf_name);
            None
        }

        _ => {
            println!("[{}]    ⚠️  No response handler for {:?}", upf_name, msg_type);
            None
        }
    };

    // Send response if we built one
    if let Some(response) = response_data {
        socket.send_to(&response, src).await?;
        println!("[{}]    ✓ Response sent", upf_name);
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    println!("\n🔵 Starting Test UPF Server: {}", args.name);
    println!("   Listen address: {}", args.listen);

    let socket = Arc::new(UdpSocket::bind(&args.listen).await?);
    let local_addr = socket.local_addr()?;
    println!("   ✓ Listening on {}\n", local_addr);

    let stats = Arc::new(UpfStats::new());

    // Spawn stats reporter
    {
        let stats = stats.clone();
        let name = args.name.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                stats.print(&name);
            }
        });
    }

    // Extract local IP for responses (use first octet-based IP or fallback)
    let local_ip = match local_addr.ip() {
        std::net::IpAddr::V4(ipv4) => ipv4,
        std::net::IpAddr::V6(_) => Ipv4Addr::new(127, 0, 0, 1), // Fallback for IPv6
    };

    // Main message loop
    let mut buf = vec![0u8; 65536];
    loop {
        let (len, src) = socket.recv_from(&mut buf).await?;
        let data = buf[..len].to_vec();

        let socket_clone = socket.clone();
        let upf_name = args.name.clone();
        let stats_clone = stats.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_message(data, src, socket_clone, upf_name, stats_clone, local_ip).await {
                eprintln!("Error handling message: {}", e);
            }
        });
    }
}
