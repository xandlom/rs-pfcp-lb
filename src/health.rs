//! Health monitoring for UPF backends

use crate::UpfPool;
use rs_pfcp::message::{HeartbeatRequest, Message};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

pub struct HealthMonitor;

impl HealthMonitor {
    pub async fn run(upf_pool: Arc<UpfPool>, socket: Arc<UdpSocket>, interval: Duration) {
        let mut ticker = tokio::time::interval(interval);
        let mut sequence = 1u32;

        loop {
            ticker.tick().await;
            Self::check_health(&upf_pool, &socket, sequence).await;
            sequence = sequence.wrapping_add(1);
        }
    }

    async fn check_health(upf_pool: &Arc<UpfPool>, socket: &Arc<UdpSocket>, sequence: u32) {
        debug!("Running health check (seq: {})", sequence);

        for backend in upf_pool.all_backends() {
            let request = HeartbeatRequest::builder()
                .sequence(sequence)
                .build()
                .expect("Failed to build heartbeat request");

            match request.marshal() {
                Ok(data) => {
                    if let Err(e) = socket.send_to(&data, backend.addr).await {
                        error!("Failed to send heartbeat to {}: {}", backend.addr, e);
                        upf_pool.update_health(backend.addr, HealthStatus::Unhealthy).await;
                    } else {
                        debug!("Sent heartbeat to {}", backend.addr);
                    }
                }
                Err(e) => {
                    error!("Failed to marshal heartbeat: {}", e);
                }
            }
        }
    }
}
