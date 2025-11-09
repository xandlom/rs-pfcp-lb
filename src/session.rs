//! Session affinity table and pending request tracking

use dashmap::DashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

// =============================================================================
// Session Table
// =============================================================================

/// Session affinity table: maps SEID to backend UPF
#[derive(Clone)]
pub struct SessionTable {
    sessions: Arc<DashMap<u64, SessionInfo>>,
}

#[derive(Clone, Debug)]
pub struct SessionInfo {
    pub upf_addr: SocketAddr,
    pub smf_addr: SocketAddr,
    pub created_at: Instant,
    pub last_activity: Instant,
}

impl SessionTable {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
        }
    }

    pub async fn insert(&self, seid: u64, upf_addr: SocketAddr, smf_addr: SocketAddr) {
        let now = Instant::now();
        let info = SessionInfo {
            upf_addr,
            smf_addr,
            created_at: now,
            last_activity: now,
        };
        self.sessions.insert(seid, info);
    }

    pub async fn lookup(&self, seid: u64) -> Option<SocketAddr> {
        self.sessions.get(&seid).map(|info| info.upf_addr)
    }

    pub async fn lookup_full(&self, seid: u64) -> Option<(SocketAddr, SocketAddr)> {
        self.sessions
            .get(&seid)
            .map(|info| (info.upf_addr, info.smf_addr))
    }

    pub async fn remove(&self, seid: u64) {
        self.sessions.remove(&seid);
    }

    pub async fn count(&self) -> usize {
        self.sessions.len()
    }

    pub async fn count_by_upf(&self, upf_addr: SocketAddr) -> usize {
        self.sessions
            .iter()
            .filter(|entry| entry.value().upf_addr == upf_addr)
            .count()
    }

    pub async fn cleanup_idle(&self, timeout: Duration) {
        let now = Instant::now();
        self.sessions
            .retain(|_, info| now.duration_since(info.last_activity) < timeout);
    }
}

impl Default for SessionTable {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Pending Requests
// =============================================================================

/// Pending request tracker: maps sequence number to origin address for response forwarding
#[derive(Clone)]
pub struct PendingRequests {
    requests: Arc<DashMap<u32, RequestInfo>>,
}

#[derive(Clone, Debug)]
pub struct RequestInfo {
    pub origin_addr: SocketAddr,
    pub timestamp: Instant,
    pub is_broadcast: bool,
    pub responses_received: usize,
    pub from_upf: bool,
}

impl PendingRequests {
    pub fn new() -> Self {
        Self {
            requests: Arc::new(DashMap::new()),
        }
    }

    pub async fn insert(&self, seq: u32, origin_addr: SocketAddr, is_broadcast: bool, from_upf: bool) {
        let info = RequestInfo {
            origin_addr,
            timestamp: Instant::now(),
            is_broadcast,
            responses_received: 0,
            from_upf,
        };
        self.requests.insert(seq, info);
    }

    pub async fn lookup_and_increment(&self, seq: u32) -> Option<(SocketAddr, bool, usize, bool)> {
        self.requests.get_mut(&seq).map(|mut entry| {
            entry.responses_received += 1;
            (
                entry.origin_addr,
                entry.is_broadcast,
                entry.responses_received,
                entry.from_upf,
            )
        })
    }

    pub async fn remove(&self, seq: u32) {
        self.requests.remove(&seq);
    }

    pub async fn cleanup_stale(&self, max_age: Duration) {
        let now = Instant::now();
        self.requests
            .retain(|_, info| now.duration_since(info.timestamp) < max_age);
    }

    pub async fn count(&self) -> usize {
        self.requests.len()
    }
}

impl Default for PendingRequests {
    fn default() -> Self {
        Self::new()
    }
}
