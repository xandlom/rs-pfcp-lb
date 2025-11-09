//! Health monitoring for UPF backends

use crate::UpfPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tracing::debug;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

pub struct HealthMonitor;

impl HealthMonitor {
    /// Run passive health monitoring
    ///
    /// Note: Active health checks are performed by the proxy when it forwards
    /// HeartbeatRequest messages from the SMF to all UPF backends. This function
    /// provides a placeholder for future active health monitoring enhancements.
    pub async fn run(_upf_pool: Arc<UpfPool>, _socket: Arc<UdpSocket>, interval: Duration) {
        let mut ticker = tokio::time::interval(interval);

        loop {
            ticker.tick().await;
            debug!("Health monitor tick (passive monitoring via HeartbeatRequest broadcasts)");

            // Future enhancement: Implement active health checks here
            // For now, health is monitored passively through HeartbeatResponse messages
            // that come back from UPF backends when SMF sends HeartbeatRequest
        }
    }
}
