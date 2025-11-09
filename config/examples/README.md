# Configuration Examples

This directory contains example configuration files for the PFCP Proxy/Load Balancer.

## Files

- **pfcp-proxy.toml** - Full configuration with all options documented
- **pfcp-proxy-simple.toml** - Minimal configuration for testing
- **free5gc-smf-config-notes.md** - Notes on configuring free5GC SMF to use the proxy

## Usage

### Using Configuration File

```bash
pfcp-proxy --config /path/to/pfcp-proxy.toml
```

### Configuration Priority

Command-line arguments override configuration file settings:

```bash
# Config file specifies round-robin, but command line overrides to least-sessions
pfcp-proxy --config config.toml --strategy least-sessions
```

## Configuration Sections

### [proxy]

Core proxy settings:
- `listen_address`: PFCP listening address
- `threads`: Number of worker threads (optional)
- `buffer_size`: UDP receive buffer size

### [load_balancing]

Load balancing behavior:
- `strategy`: Algorithm for distributing sessions
  - `round-robin`: Even distribution
  - `least-sessions`: Route to UPF with fewest sessions
  - `weighted`: Route based on backend weights
- `session_timeout`: Idle session cleanup timeout
- `health_check_interval`: How often to check UPF health
- `heartbeat_timeout`: Timeout for heartbeat responses

### [[backends]]

UPF backend configuration (can have multiple):
- `address`: UPF PFCP address (IP:port)
- `weight`: Relative capacity (used with weighted strategy)
- `zone`: Geographic zone (for future zone-aware routing)
- `max_sessions`: Maximum sessions this UPF can handle

### [health]

Health monitoring thresholds:
- `failure_threshold`: Failures before marking unhealthy
- `recovery_threshold`: Successes before marking healthy
- `degraded_latency_ms`: Latency threshold for degraded status
- `unhealthy_latency_ms`: Latency threshold for unhealthy status

### [metrics]

Metrics and monitoring:
- `enabled`: Enable metrics collection
- `prometheus_port`: Port for Prometheus scraping
- `export_interval`: How often to export metrics
- `retention_days`: How long to keep historical data

### [logging]

Logging configuration:
- `level`: Log verbosity (trace, debug, info, warn, error)
- `format`: Log format (json, text)
- `output`: Log file path (empty for stdout)

## Examples by Use Case

### Development/Testing

Use `pfcp-proxy-simple.toml` with local UPF simulators:

```bash
# Start UPF simulators
cargo run --example session-server -- --port 8806 &
cargo run --example session-server -- --port 8807 &
cargo run --example session-server -- --port 8808 &

# Start proxy
pfcp-proxy --config config/examples/pfcp-proxy-simple.toml
```

### Production Deployment

Use `pfcp-proxy.toml` with actual UPF addresses:

1. Edit backend addresses to match your UPF IPs
2. Configure appropriate weights based on UPF capacity
3. Set proper health check intervals
4. Enable JSON logging for structured logs
5. Configure Prometheus metrics

```bash
pfcp-proxy --config /etc/pfcp-proxy/config.toml
```

### Docker Deployment

Configuration can be mounted as a volume:

```yaml
services:
  pfcp-proxy:
    image: pfcp-proxy:latest
    volumes:
      - ./config/pfcp-proxy.toml:/etc/pfcp-proxy/config.toml:ro
    command: ["--config", "/etc/pfcp-proxy/config.toml"]
```

### Kubernetes Deployment

Store configuration in a ConfigMap:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: pfcp-proxy-config
data:
  config.toml: |
    [proxy]
    listen_address = "0.0.0.0:8805"
    # ... rest of configuration
```

Then mount in the pod:

```yaml
volumeMounts:
  - name: config
    mountPath: /etc/pfcp-proxy
volumes:
  - name: config
    configMap:
      name: pfcp-proxy-config
```

## Validation

Validate your configuration before deployment:

```bash
# Test configuration syntax
pfcp-proxy --config config.toml --help

# Dry-run mode (future feature)
# pfcp-proxy --config config.toml --dry-run
```

## Common Configurations

### High Availability

Multiple UPFs with least-sessions strategy:

```toml
[load_balancing]
strategy = "least-sessions"

[[backends]]
address = "10.0.1.10:8805"
weight = 1.0

[[backends]]
address = "10.0.1.11:8805"
weight = 1.0

[[backends]]
address = "10.0.2.10:8805"
weight = 1.0
```

### Heterogeneous UPF Pool

Different capacity UPFs with weighted strategy:

```toml
[load_balancing]
strategy = "weighted"

# High-capacity UPF (2x weight)
[[backends]]
address = "10.0.1.10:8805"
weight = 2.0
max_sessions = 20000

# Standard capacity UPF
[[backends]]
address = "10.0.1.11:8805"
weight = 1.0
max_sessions = 10000

# Low-capacity UPF (50% weight)
[[backends]]
address = "10.0.1.12:8805"
weight = 0.5
max_sessions = 5000
```

### Development Environment

Local testing with verbose logging:

```toml
[proxy]
listen_address = "127.0.0.1:8805"

[load_balancing]
strategy = "round-robin"

[[backends]]
address = "127.0.0.1:8806"
weight = 1.0

[logging]
level = "debug"
format = "text"
output = ""  # stdout
```
