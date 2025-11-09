//! Routing decision logic and load balancing strategies

use std::net::SocketAddr;

#[derive(Debug, Clone, Copy)]
pub enum LoadBalancingStrategy {
    RoundRobin,
    LeastSessions,
    Weighted,
}

#[derive(Debug)]
pub enum RoutingDecision {
    Single(SocketAddr),
    Broadcast(Vec<SocketAddr>),
    ResponseToClient,
    SessionNotFound,
    NoSeid,
    NoBackend,
}
