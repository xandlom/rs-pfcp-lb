# Docker Deployment Guide

This guide covers deploying the PFCP Proxy/Load Balancer using Docker and Docker Compose.

## Quick Start with Docker Compose

The easiest way to run the complete setup is using Docker Compose, which includes:
- 1x PFCP Proxy/Load Balancer
- 3x Test UPF backends (upf1, upf2, upf3)
- 1x Test SMF client (optional, for testing)

### Start the stack

```bash
# Build and start the proxy and UPF backends
docker-compose up -d

# View logs
docker-compose logs -f pfcp-proxy
docker-compose logs -f upf1 upf2 upf3

# Check status
docker-compose ps
```

### Run tests

```bash
# Run heartbeat test
docker-compose run --rm test-smf --target pfcp-proxy:8805 heartbeat --count 5

# Run session establishment test
docker-compose run --rm test-smf --target pfcp-proxy:8805 sessions --count 10

# Run full test scenario
docker-compose run --rm test-smf --target pfcp-proxy:8805 full --sessions 20
```

### Stop the stack

```bash
docker-compose down
```

## Architecture

```
┌─────────────────────────────────────────────┐
│         External SMF (172.20.0.0/16)        │
│                     │                        │
│                     ▼                        │
│            ┌──────────────────┐             │
│            │   pfcp-proxy     │             │
│            │   (8805/udp)     │             │
│            └────────┬─────────┘             │
│                     │                        │
│        ┌────────────┼────────────┐          │
│        │            │            │          │
│        ▼            ▼            ▼          │
│   ┌────────┐  ┌────────┐  ┌────────┐       │
│   │  upf1  │  │  upf2  │  │  upf3  │       │
│   │ 172.   │  │ 172.   │  │ 172.   │       │
│   │ 20.0.11│  │ 20.0.12│  │ 20.0.13│       │
│   └────────┘  └────────┘  └────────┘       │
└─────────────────────────────────────────────┘
```

## Configuration

### Proxy Configuration

Edit the `pfcp-proxy` service in `docker-compose.yaml`:

```yaml
command:
  - --listen
  - "0.0.0.0:8805"
  - --backends
  - "upf1:8805,upf2:8805,upf3:8805"
  - --strategy
  - "round-robin"              # Options: round-robin, least-sessions, weighted
  - --log-level
  - "info"                     # Options: trace, debug, info, warn, error
  - --stats-interval
  - "10"                       # Statistics reporting interval (seconds)
```

### Adding/Removing UPF Backends

To add more UPF backends:

1. Add a new service in `docker-compose.yaml`:
```yaml
  upf4:
    build:
      context: .
      dockerfile: Dockerfile
    container_name: upf4
    user: pfcp
    entrypoint: ["test-upf"]
    command:
      - --listen
      - "0.0.0.0:8805"
      - --name
      - "upf4"
    networks:
      pfcp-network:
        ipv4_address: 172.20.0.14
    restart: unless-stopped
```

2. Update the proxy's `--backends` argument:
```yaml
- --backends
- "upf1:8805,upf2:8805,upf3:8805,upf4:8805"
```

3. Restart the stack:
```bash
docker-compose up -d
```

## Running Individual Containers

### PFCP Proxy

```bash
docker run -d \
  --name pfcp-proxy \
  -p 8805:8805/udp \
  pfcp-proxy \
  --listen 0.0.0.0:8805 \
  --backends 10.0.1.10:8805,10.0.1.11:8805,10.0.1.12:8805
```

### Test UPF

```bash
docker run -d \
  --name test-upf \
  -p 8805:8805/udp \
  pfcp-proxy \
  test-upf --listen 0.0.0.0:8805 --name upf1
```

### Test SMF Client

```bash
# Heartbeat test
docker run --rm pfcp-proxy \
  test-smf --target 10.0.1.1:8805 heartbeat --count 5

# Session test
docker run --rm pfcp-proxy \
  test-smf --target 10.0.1.1:8805 sessions --count 10 --delete
```

## Production Deployment

For production use, consider:

### 1. Use External UPF Backends

Update the `--backends` argument to point to your actual UPF instances:

```yaml
command:
  - --backends
  - "upf1.example.com:8805,upf2.example.com:8805,upf3.example.com:8805"
```

### 2. Enable Health Monitoring

The proxy includes built-in health checks. Monitor using:

```bash
docker-compose exec pfcp-proxy pfcp-proxy --help
```

### 3. Persistent Logging

Mount a volume for logs:

```yaml
volumes:
  - ./logs:/var/log/pfcp-proxy
```

### 4. Resource Limits

Add resource constraints:

```yaml
deploy:
  resources:
    limits:
      cpus: '2'
      memory: 2G
    reservations:
      cpus: '1'
      memory: 1G
```

### 5. Network Configuration

For production, use host networking or macvlan for better performance:

```yaml
network_mode: "host"
```

Or use a dedicated network interface:

```yaml
networks:
  pfcp-network:
    driver: macvlan
    driver_opts:
      parent: eth0
    ipam:
      config:
        - subnet: 10.0.1.0/24
          gateway: 10.0.1.1
```

## Monitoring and Statistics

### View Real-time Statistics

The proxy logs statistics every 10 seconds (configurable):

```bash
docker-compose logs -f pfcp-proxy | grep "Statistics"
```

### Check UPF Backend Health

```bash
# View UPF logs
docker-compose logs upf1 upf2 upf3

# Check message counts
docker-compose exec upf1 pgrep test-upf
```

## Troubleshooting

### Proxy won't start

Check if backends are specified:
```bash
docker-compose logs pfcp-proxy
```

Expected error if backends missing:
```
Use --backends flag to specify UPF addresses
Example: --backends 10.0.1.10:8805,10.0.1.11:8805
```

### No responses from UPF

Verify UPF containers are running:
```bash
docker-compose ps
```

Check network connectivity:
```bash
docker-compose exec pfcp-proxy ping upf1
```

### High latency

Check network mode and consider using host networking for production.

## Load Balancing Strategies

### Round Robin (Default)
Distributes sessions evenly across all backends:
```yaml
- --strategy
- "round-robin"
```

### Least Sessions
Routes to the UPF with fewest active sessions:
```yaml
- --strategy
- "least-sessions"
```

### Weighted
Distributes based on UPF weights (configure in code):
```yaml
- --strategy
- "weighted"
```

## Testing Load Distribution

Run a load test to verify distribution:

```bash
# Establish 100 sessions
docker-compose run --rm test-smf \
  --target pfcp-proxy:8805 \
  sessions --count 100 --delete

# View distribution in logs
docker-compose logs pfcp-proxy | grep "Session established"
docker-compose logs upf1 upf2 upf3 | grep "Session SEID"
```

## Upgrading

```bash
# Pull latest changes
git pull

# Rebuild and restart
docker-compose down
docker-compose build --no-cache
docker-compose up -d
```

## Cleanup

```bash
# Stop and remove containers
docker-compose down

# Remove images
docker-compose down --rmi all

# Remove volumes
docker-compose down -v
```
