//! Statistics collection and reporting

use crate::{SessionTable, UpfPool};
use dashmap::DashMap;
use rs_pfcp::message::MsgType;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Default)]
pub struct Statistics {
    // Global counters
    pub total_messages_received: AtomicU64,
    pub total_messages_sent: AtomicU64,
    pub total_responses_forwarded: AtomicU64,

    // Per-message-type counters
    pub msg_type_counts: Arc<DashMap<MsgType, u64>>,

    // Per-UPF counters
    pub upf_message_counts: Arc<DashMap<SocketAddr, u64>>,

    // Session counters
    pub sessions_established: AtomicU64,
    pub sessions_deleted: AtomicU64,

    // Routing decisions
    pub routed_by_seid: AtomicU64,
    pub routed_by_load_balance: AtomicU64,
    pub broadcasts: AtomicU64,

    // Response tracking
    pub responses_dropped: AtomicU64,
}

impl Statistics {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn record_message_received(&self, msg_type: MsgType) {
        self.total_messages_received.fetch_add(1, Ordering::Relaxed);
        self.msg_type_counts
            .entry(msg_type)
            .and_modify(|c| *c += 1)
            .or_insert(1);
    }

    pub async fn record_message_sent(&self, upf_addr: SocketAddr) {
        self.total_messages_sent.fetch_add(1, Ordering::Relaxed);
        self.upf_message_counts
            .entry(upf_addr)
            .and_modify(|c| *c += 1)
            .or_insert(1);
    }

    pub fn record_session_established(&self) {
        self.sessions_established.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_session_deleted(&self) {
        self.sessions_deleted.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_routing_decision(&self, by_seid: bool, broadcast: bool) {
        if broadcast {
            self.broadcasts.fetch_add(1, Ordering::Relaxed);
        } else if by_seid {
            self.routed_by_seid.fetch_add(1, Ordering::Relaxed);
        } else {
            self.routed_by_load_balance.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_response_forwarded(&self) {
        self.total_responses_forwarded
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_response_dropped(&self) {
        self.responses_dropped.fetch_add(1, Ordering::Relaxed);
    }

    pub async fn print_report(&self, session_table: &SessionTable, upf_pool: &UpfPool) {
        println!("\n{}", "=".repeat(80));
        println!("PFCP Proxy Statistics Report");
        println!("{}", "=".repeat(80));

        // Global metrics
        let total_rx = self.total_messages_received.load(Ordering::Relaxed);
        let total_tx = self.total_messages_sent.load(Ordering::Relaxed);
        let active_sessions = session_table.count().await;

        println!("\nGLOBAL METRICS:");
        println!("  Total Messages Received:   {}", total_rx);
        println!("  Total Messages Sent:       {}", total_tx);
        println!(
            "  Total Responses Forwarded: {}",
            self.total_responses_forwarded.load(Ordering::Relaxed)
        );
        println!("  Active Sessions:           {}", active_sessions);
        println!(
            "  Sessions Established:      {}",
            self.sessions_established.load(Ordering::Relaxed)
        );
        println!(
            "  Sessions Deleted:          {}",
            self.sessions_deleted.load(Ordering::Relaxed)
        );

        let dropped = self.responses_dropped.load(Ordering::Relaxed);
        if dropped > 0 {
            println!(
                "  ⚠️  Responses Dropped:       {} (no matching request)",
                dropped
            );
        }

        // Routing decisions
        println!("\nROUTING DECISIONS:");
        println!(
            "  Routed by SEID (affinity): {}",
            self.routed_by_seid.load(Ordering::Relaxed)
        );
        println!(
            "  Load balanced (new):       {}",
            self.routed_by_load_balance.load(Ordering::Relaxed)
        );
        println!(
            "  Broadcast (heartbeat):     {}",
            self.broadcasts.load(Ordering::Relaxed)
        );

        // Message type distribution
        println!("\nMESSAGE TYPE DISTRIBUTION:");
        let mut msg_counts: Vec<_> = self.msg_type_counts.iter().map(|e| (*e.key(), *e.value())).collect();
        msg_counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));

        for (msg_type, count) in msg_counts.iter().take(10) {
            println!("  {:40} {:>8}", format!("{:?}", msg_type), count);
        }

        // Per-UPF distribution
        println!("\nPER-UPF DISTRIBUTION:");
        println!(
            "{:<25} {:>14} {:>16}",
            "Backend Address", "Messages Sent", "Active Sessions"
        );
        println!("{}", "-".repeat(56));

        for backend in upf_pool.all_backends() {
            let msg_count = self
                .upf_message_counts
                .get(&backend.addr)
                .map(|e| *e.value())
                .unwrap_or(0);
            let session_count = session_table.count_by_upf(backend.addr).await;

            println!("{:<25} {:>14} {:>16}", backend.addr, msg_count, session_count);
        }

        println!("{}", "=".repeat(80));
    }

    /// Export statistics to a JSON file for external consumption (e.g., TUI)
    pub async fn export_to_json(
        &self,
        path: impl AsRef<Path>,
        session_table: &SessionTable,
        upf_pool: &UpfPool,
    ) -> std::io::Result<()> {
        let snapshot = self.create_snapshot(session_table, upf_pool).await;
        let json = serde_json::to_string_pretty(&snapshot)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Create a serializable snapshot of current statistics
    async fn create_snapshot(
        &self,
        session_table: &SessionTable,
        upf_pool: &UpfPool,
    ) -> StatisticsSnapshot {
        // Collect message type counts
        let mut message_types = HashMap::new();
        for entry in self.msg_type_counts.iter() {
            message_types.insert(format!("{:?}", entry.key()), *entry.value());
        }

        // Collect per-UPF statistics
        let mut upf_stats = Vec::new();
        for backend in upf_pool.all_backends() {
            let messages_sent = self
                .upf_message_counts
                .get(&backend.addr)
                .map(|e| *e.value())
                .unwrap_or(0);
            let active_sessions = session_table.count_by_upf(backend.addr).await;

            upf_stats.push(UpfStatSnapshot {
                address: backend.addr.to_string(),
                messages_sent,
                active_sessions,
                health: format!("{:?}", *backend.health.read().await),
            });
        }

        StatisticsSnapshot {
            timestamp: chrono::Utc::now().to_rfc3339(),
            total_messages_received: self.total_messages_received.load(Ordering::Relaxed),
            total_messages_sent: self.total_messages_sent.load(Ordering::Relaxed),
            total_responses_forwarded: self.total_responses_forwarded.load(Ordering::Relaxed),
            responses_dropped: self.responses_dropped.load(Ordering::Relaxed),
            active_sessions: session_table.count().await,
            sessions_established: self.sessions_established.load(Ordering::Relaxed),
            sessions_deleted: self.sessions_deleted.load(Ordering::Relaxed),
            routed_by_seid: self.routed_by_seid.load(Ordering::Relaxed),
            routed_by_load_balance: self.routed_by_load_balance.load(Ordering::Relaxed),
            broadcasts: self.broadcasts.load(Ordering::Relaxed),
            message_types,
            upf_stats,
        }
    }
}

/// Serializable snapshot of statistics
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StatisticsSnapshot {
    pub timestamp: String,
    pub total_messages_received: u64,
    pub total_messages_sent: u64,
    pub total_responses_forwarded: u64,
    pub responses_dropped: u64,
    pub active_sessions: usize,
    pub sessions_established: u64,
    pub sessions_deleted: u64,
    pub routed_by_seid: u64,
    pub routed_by_load_balance: u64,
    pub broadcasts: u64,
    pub message_types: HashMap<String, u64>,
    pub upf_stats: Vec<UpfStatSnapshot>,
}

/// Per-UPF statistics snapshot
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpfStatSnapshot {
    pub address: String,
    pub messages_sent: u64,
    pub active_sessions: usize,
    pub health: String,
}
