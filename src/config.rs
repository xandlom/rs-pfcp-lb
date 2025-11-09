//! Configuration management for PFCP Proxy

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub proxy: ProxyConfig,
    pub load_balancing: LoadBalancingConfig,
    pub backends: Vec<BackendConfig>,
    pub health: HealthConfig,
    pub metrics: MetricsConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub listen_address: String,
    pub threads: Option<usize>,
    pub buffer_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadBalancingConfig {
    pub strategy: String,
    pub session_timeout: u64,
    pub health_check_interval: u64,
    pub heartbeat_timeout: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    pub address: String,
    pub weight: f32,
    pub zone: Option<String>,
    pub max_sessions: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    pub failure_threshold: u32,
    pub recovery_threshold: u32,
    pub degraded_latency_ms: u64,
    pub unhealthy_latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub prometheus_port: Option<u16>,
    pub export_interval: u64,
    pub retention_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
    pub output: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            proxy: ProxyConfig {
                listen_address: "0.0.0.0:8805".to_string(),
                threads: None,
                buffer_size: 65536,
            },
            load_balancing: LoadBalancingConfig {
                strategy: "round-robin".to_string(),
                session_timeout: 3600,
                health_check_interval: 5,
                heartbeat_timeout: 2,
            },
            backends: vec![],
            health: HealthConfig {
                failure_threshold: 3,
                recovery_threshold: 5,
                degraded_latency_ms: 100,
                unhealthy_latency_ms: 500,
            },
            metrics: MetricsConfig {
                enabled: true,
                prometheus_port: Some(9090),
                export_interval: 10,
                retention_days: 7,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "json".to_string(),
                output: "/var/log/pfcp-proxy/proxy.log".to_string(),
            },
        }
    }
}

impl Config {
    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}
