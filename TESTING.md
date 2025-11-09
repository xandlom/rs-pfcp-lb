# PFCP Proxy Load Balancer - Testing Guide

This guide explains how to test the PFCP Proxy Load Balancer using various scenarios and tools.

## Table of Contents

- [Quick Start](#quick-start)
- [Test Infrastructure](#test-infrastructure)
- [Test Scenarios](#test-scenarios)
- [Manual Testing](#manual-testing)
- [Monitoring and Verification](#monitoring-and-verification)
- [Troubleshooting](#troubleshooting)

---

## Quick Start

### 1. Start the Infrastructure

```bash
# Start the proxy and UPF backends
docker compose up -d

# Wait for services to be healthy
docker compose ps
```

### 2. Run Interactive Test Scenarios

```bash
# Launch the interactive test menu
docker compose run --rm test-runner
```

This will display a menu with various test scenarios:
- **Basic Test**: 10 sessions (quick validation)
- **Load Balance Test**: 30 sessions (perfect 3-way split)
- **Heavy Load Test**: 100 sessions
- **Stress Test**: 1000 sessions
- **Session Affinity**: Tests that modifications go to same UPF
- **Custom**: Specify your own number of sessions
- **Continuous Load**: Keep creating sessions until stopped

### 3. Monitor the Results

```bash
# Watch proxy statistics in real-time
docker compose logs -f pfcp-proxy

# Check individual UPF logs
docker compose logs upf1
docker compose logs upf2
docker compose logs upf3
```

---

## Test Infrastructure

### Architecture

```
┌─────────────┐
│ test-runner │ ──┐
│ (SMF)       │   │
└─────────────┘   │
                  │ PFCP Messages
┌─────────────┐   │   (UDP 8805)
│ smf-client  │ ──┤
│ (Manual)    │   │
└─────────────┘   │
                  ▼
            ┌──────────────┐
            │ pfcp-proxy   │
            │ (172.20.0.10)│
            └──────────────┘
                    │
        ┌───────────┼───────────┐
        │           │           │
        ▼           ▼           ▼
    ┌──────┐   ┌──────┐   ┌──────┐
    │ upf1 │   │ upf2 │   │ upf3 │
    │ .11  │   │ .12  │   │ .13  │
    └──────┘   └──────┘   └──────┘
```

### Services

| Service | IP Address | Purpose |
|---------|------------|---------|
| pfcp-proxy | 172.20.0.10:8805 | Load balancer |
| upf1 | 172.20.0.11:8805 | Backend UPF #1 |
| upf2 | 172.20.0.12:8805 | Backend UPF #2 |
| upf3 | 172.20.0.13:8805 | Backend UPF #3 |
| smf-client | 172.20.0.20 | Manual test client |
| test-runner | 172.20.0.21 | Scenario runner |

---

## Test Scenarios

### 1. Basic Test (10 Sessions)

**Purpose**: Quick validation that the proxy is working

**Expected Result**:
- 10 session establishment requests sent
- Messages distributed across 3 UPFs
- Distribution: ~4, ~3, ~3 (round-robin with remainder)
- 10 session deletion requests sent
- All sessions cleaned up

**Run**:
```bash
docker compose run --rm test-runner
# Select option 1
```

**Statistics to Check**:
```bash
docker compose logs pfcp-proxy | grep "Statistics"
# Should show:
# - Total messages: ~20 (10 establish + 10 delete)
# - Active sessions: 0 (after cleanup)
# - Messages per UPF: roughly equal distribution
```

### 2. Load Balance Test (30 Sessions)

**Purpose**: Validate perfect load distribution

**Expected Result**:
- 30 session establishments = 10 per UPF (perfect round-robin)
- Each UPF receives exactly 10 sessions
- Followed by 30 deletions

**Run**:
```bash
docker compose run --rm test-runner
# Select option 2
```

**Verification**:
```bash
# Check proxy statistics
docker compose logs pfcp-proxy | tail -20

# Expected distribution:
# UPF1: 20 messages (10 establish + 10 delete)
# UPF2: 20 messages (10 establish + 10 delete)
# UPF3: 20 messages (10 establish + 10 delete)
```

### 3. Heavy Load Test (100 Sessions)

**Purpose**: Test proxy performance under moderate load

**Expected Result**:
- 100 sessions created
- Distribution: 34, 33, 33 (approximately)
- All sessions successfully established and deleted
- No message loss

**Run**:
```bash
docker compose run --rm test-runner
# Select option 3
```

### 4. Stress Test (1000 Sessions)

**Purpose**: Test proxy under high load

**Expected Result**:
- 1000 sessions created
- Even distribution across UPFs (~333 each)
- Proxy remains responsive
- No crashes or errors

**Run**:
```bash
docker compose run --rm test-runner
# Select option 4
```

**Monitor Performance**:
```bash
# Watch CPU and memory usage
docker stats pfcp-proxy

# Check for errors
docker compose logs pfcp-proxy | grep -i error
```

### 5. Session Affinity Test

**Purpose**: Verify that session modifications are routed to the correct UPF

**Expected Result**:
- Session establishment goes to UPF (via load balancer)
- Session modifications go to same UPF (via SEID lookup)
- Session deletion goes to same UPF

**Run**:
```bash
docker compose run --rm test-runner
# Select option 5
```

**Note**: This test currently only establishes sessions. Full affinity testing requires session modification support in the client.

### 6. Custom Test

**Purpose**: Test with a specific number of sessions

**Run**:
```bash
docker compose run --rm test-runner
# Select option 6
# Enter your desired number of sessions
```

### 7. Continuous Load Test

**Purpose**: Long-running test to verify stability

**Expected Result**:
- Sessions created in batches of 20
- Proxy continues to distribute load evenly
- No memory leaks or degradation over time

**Run**:
```bash
docker compose run --rm test-runner
# Select option 7
# Press Ctrl+C to stop when satisfied
```

**Monitor**:
```bash
# In another terminal, watch statistics
docker compose logs -f pfcp-proxy

# Check memory usage over time
docker stats --no-stream pfcp-proxy
```

---

## Manual Testing

### Using session-client Directly

```bash
# Run a specific number of sessions
docker compose run --rm smf-client

# Custom parameters
docker compose run --rm smf-client session-client \
  --interface eth0 \
  --address pfcp-proxy \
  --port 8805 \
  --sessions 50
```

### Testing Individual UPFs

```bash
# Test UPF1 directly (bypass proxy)
docker compose run --rm -e PROXY_ADDRESS=upf1 smf-client

# Test UPF2 directly
docker compose run --rm -e PROXY_ADDRESS=upf2 smf-client

# Test UPF3 directly
docker compose run --rm -e PROXY_ADDRESS=upf3 smf-client
```

### Testing Different Load Balancing Strategies

```bash
# Edit docker-compose.yml to change strategy
# Current default: round-robin
# Options: round-robin, least-sessions, weighted

# Restart proxy with new strategy
docker compose up -d pfcp-proxy

# Run tests
docker compose run --rm test-runner
```

---

## Monitoring and Verification

### Proxy Statistics

The proxy logs statistics every 10 seconds (configurable with `--stats-interval`):

```bash
docker compose logs -f pfcp-proxy
```

**Key Metrics**:
- `Total messages received`: All PFCP messages processed
- `Messages forwarded`: Successfully sent to UPFs
- `Routing decisions`: How messages were routed
- `Active sessions`: Currently tracked sessions
- `Per-UPF message distribution`: Load balance verification

### UPF Logs

Check what each UPF receives:

```bash
# UPF1
docker compose logs upf1 | grep "Received"

# UPF2
docker compose logs upf2 | grep "Received"

# UPF3
docker compose logs upf3 | grep "Received"
```

### Health Checks

```bash
# Check service health
docker compose ps

# All services should show "healthy" status
```

### Network Verification

```bash
# Verify network connectivity
docker compose exec pfcp-proxy ping -c 3 upf1
docker compose exec pfcp-proxy ping -c 3 upf2
docker compose exec pfcp-proxy ping -c 3 upf3
```

---

## Troubleshooting

### Test Client Fails to Connect

**Symptom**: `Error: Os { code: 22, kind: InvalidInput, message: "Invalid argument" }`

**Solution**: Ensure `--interface eth0` is specified:
```bash
session-client --interface eth0 --address pfcp-proxy --port 8805 --sessions 10
```

### Uneven Load Distribution

**Symptom**: One UPF receives significantly more messages than others

**Possible Causes**:
1. **Small sample size**: With 10 sessions and 3 UPFs, distribution will be 4-3-3 (not equal)
   - Solution: Run test with 30+ sessions for better distribution

2. **Session affinity**: Modifications/deletions follow the establishment UPF
   - Expected: This is correct behavior

3. **Health check failures**: One UPF might be marked unhealthy
   - Check: `docker compose logs pfcp-proxy | grep -i health`

### Proxy Not Starting

**Symptom**: `pfcp-proxy` exits immediately with code 0

**Possible Causes**:
1. **Backend DNS resolution failed**
   - Check: `docker compose logs pfcp-proxy | grep "Resolved"`
   - Solution: Ensure UPF services are running first

2. **Port already in use**
   - Check: `netstat -ulnp | grep 8805`
   - Solution: Stop conflicting service or change port

### High Latency

**Symptom**: Messages take longer than expected to process

**Investigation**:
```bash
# Check CPU usage
docker stats pfcp-proxy

# Check network latency
docker compose exec pfcp-proxy ping upf1

# Enable debug logging
# Edit docker-compose.yml: --log-level debug
docker compose up -d pfcp-proxy
```

### Sessions Not Deleted

**Symptom**: Active session count keeps growing

**Check**:
```bash
# View session table size
docker compose logs pfcp-proxy | grep "Active sessions"

# Verify delete requests are being sent
docker compose run --rm test-runner
# Run Basic Test and check if count returns to 0
```

### Test Runner Script Not Found

**Symptom**: `/scripts/test-scenarios.sh: No such file or directory`

**Solution**: Ensure script is executable and volume is mounted:
```bash
chmod +x scripts/test-scenarios.sh
docker compose down
docker compose up -d
```

---

## Performance Benchmarks

Based on testing with the current setup:

| Test Scenario | Sessions | Duration | Messages/sec | Notes |
|---------------|----------|----------|--------------|-------|
| Basic | 10 | ~1s | ~20 | Quick validation |
| Load Balance | 30 | ~2s | ~30 | Perfect distribution |
| Heavy Load | 100 | ~5s | ~40 | Good performance |
| Stress | 1000 | ~30s | ~65 | Sustained throughput |

**Hardware**: Docker Desktop on typical development machine

**Note**: Performance depends on:
- Network latency between containers
- Host machine resources
- Docker networking overhead
- UPF response times

---

## Advanced Testing

### Custom Test Scenarios

Create your own test script:

```bash
#!/bin/bash
# custom-test.sh

# Test with increasing load
for sessions in 10 50 100 500 1000; do
    echo "Testing with $sessions sessions..."
    session-client \
        --interface eth0 \
        --address pfcp-proxy \
        --port 8805 \
        --sessions $sessions

    echo "Sleeping 5s..."
    sleep 5
done
```

Run it:
```bash
docker compose run --rm -v ./custom-test.sh:/custom-test.sh:ro smf-client /bin/bash /custom-test.sh
```

### Load Testing with Concurrent Clients

```bash
# Run multiple clients simultaneously
for i in {1..5}; do
    docker compose run --rm -d smf-client session-client \
        --interface eth0 \
        --address pfcp-proxy \
        --port 8805 \
        --sessions 20 &
done

wait
echo "All clients finished"
```

### Chaos Testing

Test proxy resilience:

```bash
# Start load
docker compose run --rm -d test-runner
# Select option 7 (Continuous Load)

# Kill random UPF
docker compose stop upf2

# Observe proxy behavior
docker compose logs -f pfcp-proxy

# Restart UPF
docker compose start upf2

# Verify recovery
docker compose ps
```

---

## Test Matrix

Recommended test sequence for validation:

- [ ] Basic Test (10 sessions) - Passes
- [ ] Load Balance Test (30 sessions) - Perfect distribution
- [ ] Heavy Load Test (100 sessions) - No errors
- [ ] Stress Test (1000 sessions) - Stable under load
- [ ] Continuous Load (5 minutes) - No memory leaks
- [ ] UPF Failure Test - Graceful degradation
- [ ] UPF Recovery Test - Auto-recovery
- [ ] Multiple Clients Test - Concurrent handling

---

## Contributing Test Scenarios

To add new test scenarios:

1. Edit `scripts/test-scenarios.sh`
2. Add new case to the menu
3. Implement `run_test` call with appropriate parameters
4. Update this documentation
5. Submit PR with test results

Example:
```bash
8)
    echo -e "\n${YELLOW}Burst Test${NC}"
    echo "Creates 100 sessions rapidly without delays"
    run_test 100 "Burst Test"
    ;;
```

---

## Resources

- [rs-pfcp Examples](https://github.com/xandlom/rs-pfcp/tree/main/examples)
- [PFCP Protocol (3GPP TS 29.244)](https://www.3gpp.org/ftp/Specs/archive/29_series/29.244/)
- [Docker Compose Documentation](https://docs.docker.com/compose/)

---

## Support

For issues or questions:
1. Check the [Troubleshooting](#troubleshooting) section
2. Review `docker compose logs` for errors
3. Open an issue on GitHub with:
   - Test scenario details
   - Expected vs actual behavior
   - Relevant log excerpts
   - Environment information
