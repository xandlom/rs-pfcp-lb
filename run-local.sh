#!/bin/bash
# run-local.sh - Run PFCP Proxy/Load Balancer and UPF backends on loopback interface
#
# This script runs all components locally using the loopback interface (127.0.0.1)
# for testing without Docker or complex network setup.
#
# Usage:
#   ./run-local.sh start      - Start all services
#   ./run-local.sh stop       - Stop all services
#   ./run-local.sh restart    - Restart all services
#   ./run-local.sh test       - Run test scenarios
#   ./run-local.sh status     - Show status of services
#   ./run-local.sh logs       - Tail logs from all services
#   ./run-local.sh clean      - Clean logs and build artifacts

set -e

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PID_DIR="${SCRIPT_DIR}/.pids"
LOG_DIR="${SCRIPT_DIR}/.logs"
BUILD_DIR="${SCRIPT_DIR}/target/release"

# Service configuration
PROXY_ADDR="127.0.0.1:8805"
UPF1_ADDR="127.0.0.1:8806"
UPF2_ADDR="127.0.0.1:8807"
UPF3_ADDR="127.0.0.1:8808"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Print colored output
print_info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

# Initialize directories
init_dirs() {
    mkdir -p "$PID_DIR" "$LOG_DIR"
}

# Build the project
build_project() {
    print_info "Building project..."
    cargo build --release
    print_success "Build complete"
}

# Check if a service is running
is_running() {
    local pid_file="$1"
    if [ -f "$pid_file" ]; then
        local pid=$(cat "$pid_file")
        if ps -p "$pid" > /dev/null 2>&1; then
            return 0
        fi
    fi
    return 1
}

# Start a UPF backend
start_upf() {
    local name="$1"
    local port="$2"
    local pid_file="${PID_DIR}/${name}.pid"
    local log_file="${LOG_DIR}/${name}.log"

    if is_running "$pid_file"; then
        print_warning "${name} is already running (PID: $(cat $pid_file))"
        return
    fi

    print_info "Starting ${name} on 127.0.0.1:${port}..."

    "${BUILD_DIR}/test-upf" \
        --listen "127.0.0.1:${port}" \
        --name "${name}" \
        > "$log_file" 2>&1 &

    local pid=$!
    echo "$pid" > "$pid_file"

    sleep 0.5

    if is_running "$pid_file"; then
        print_success "${name} started (PID: ${pid})"
    else
        print_error "Failed to start ${name}"
        rm -f "$pid_file"
    fi
}

# Start the PFCP proxy
start_proxy() {
    local pid_file="${PID_DIR}/pfcp-proxy.pid"
    local log_file="${LOG_DIR}/pfcp-proxy.log"

    if is_running "$pid_file"; then
        print_warning "PFCP Proxy is already running (PID: $(cat $pid_file))"
        return
    fi

    print_info "Starting PFCP Proxy on ${PROXY_ADDR}..."

    "${BUILD_DIR}/pfcp-proxy" \
        --listen "${PROXY_ADDR}" \
        --backends "${UPF1_ADDR},${UPF2_ADDR},${UPF3_ADDR}" \
        --strategy "round-robin" \
        --stats-interval 10 \
        --health-check-interval 5 \
        --log-level "info" \
        > "$log_file" 2>&1 &

    local pid=$!
    echo "$pid" > "$pid_file"

    sleep 1

    if is_running "$pid_file"; then
        print_success "PFCP Proxy started (PID: ${pid})"
    else
        print_error "Failed to start PFCP Proxy"
        rm -f "$pid_file"
    fi
}

# Stop a service
stop_service() {
    local name="$1"
    local pid_file="${PID_DIR}/${name}.pid"

    if ! is_running "$pid_file"; then
        print_warning "${name} is not running"
        return
    fi

    local pid=$(cat "$pid_file")
    print_info "Stopping ${name} (PID: ${pid})..."

    kill "$pid" 2>/dev/null || true

    # Wait up to 5 seconds for graceful shutdown
    for i in {1..10}; do
        if ! ps -p "$pid" > /dev/null 2>&1; then
            break
        fi
        sleep 0.5
    done

    # Force kill if still running
    if ps -p "$pid" > /dev/null 2>&1; then
        print_warning "Force killing ${name}..."
        kill -9 "$pid" 2>/dev/null || true
    fi

    rm -f "$pid_file"
    print_success "${name} stopped"
}

# Start all services
start_all() {
    init_dirs

    # Check if binaries exist
    if [ ! -f "${BUILD_DIR}/pfcp-proxy" ] || [ ! -f "${BUILD_DIR}/test-upf" ]; then
        print_warning "Binaries not found. Building project..."
        build_project
    fi

    echo ""
    print_info "Starting PFCP Proxy/Load Balancer Test Environment"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    # Start UPF backends
    start_upf "upf1" "8806"
    start_upf "upf2" "8807"
    start_upf "upf3" "8808"

    # Wait for UPFs to be ready
    sleep 1

    # Start proxy
    start_proxy

    echo ""
    print_success "All services started successfully!"
    echo ""
    print_info "Service URLs:"
    echo "  • PFCP Proxy:  ${PROXY_ADDR}"
    echo "  • UPF Backend 1: ${UPF1_ADDR}"
    echo "  • UPF Backend 2: ${UPF2_ADDR}"
    echo "  • UPF Backend 3: ${UPF3_ADDR}"
    echo ""
    print_info "Logs are available in: ${LOG_DIR}"
    echo ""
    print_info "Run './run-local.sh test' to run test scenarios"
    print_info "Run './run-local.sh logs' to tail logs"
    print_info "Run './run-local.sh stop' to stop all services"
    echo ""
}

# Stop all services
stop_all() {
    echo ""
    print_info "Stopping all services..."
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    stop_service "pfcp-proxy"
    stop_service "upf1"
    stop_service "upf2"
    stop_service "upf3"

    echo ""
    print_success "All services stopped"
    echo ""
}

# Show status of services
show_status() {
    echo ""
    print_info "Service Status"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    local services=("pfcp-proxy" "upf1" "upf2" "upf3")

    for service in "${services[@]}"; do
        local pid_file="${PID_DIR}/${service}.pid"
        if is_running "$pid_file"; then
            local pid=$(cat "$pid_file")
            echo -e "  ${GREEN}●${NC} ${service} - running (PID: ${pid})"
        else
            echo -e "  ${RED}●${NC} ${service} - stopped"
        fi
    done

    echo ""
}

# Run test scenarios
run_tests() {
    if ! is_running "${PID_DIR}/pfcp-proxy.pid"; then
        print_error "PFCP Proxy is not running. Start services first with './run-local.sh start'"
        exit 1
    fi

    if [ ! -f "${BUILD_DIR}/test-smf" ]; then
        print_error "test-smf binary not found. Build the project first."
        exit 1
    fi

    echo ""
    print_info "PFCP Proxy Test Menu"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    echo "Available test scenarios:"
    echo "  1) Heartbeat Test (5 heartbeats)"
    echo "  2) Basic Session Test (10 sessions)"
    echo "  3) Load Balance Test (30 sessions)"
    echo "  4) Session Lifecycle Test (20 sessions with deletion)"
    echo "  5) Full Test (heartbeat + sessions)"
    echo "  6) Custom test"
    echo "  0) Cancel"
    echo ""
    read -p "Select test scenario (0-6): " choice

    case "$choice" in
        1)
            print_info "Running heartbeat test..."
            "${BUILD_DIR}/test-smf" --target "${PROXY_ADDR}" heartbeat --count 5 --interval 1
            ;;
        2)
            print_info "Running basic session test (10 sessions)..."
            "${BUILD_DIR}/test-smf" --target "${PROXY_ADDR}" sessions --count 10
            ;;
        3)
            print_info "Running load balance test (30 sessions)..."
            "${BUILD_DIR}/test-smf" --target "${PROXY_ADDR}" load-balance --sessions 30
            ;;
        4)
            print_info "Running session lifecycle test (20 sessions with deletion)..."
            "${BUILD_DIR}/test-smf" --target "${PROXY_ADDR}" sessions --count 20 --delete
            ;;
        5)
            print_info "Running full test..."
            "${BUILD_DIR}/test-smf" --target "${PROXY_ADDR}" full --sessions 20
            ;;
        6)
            print_info "Custom test - showing help:"
            "${BUILD_DIR}/test-smf" --help
            echo ""
            print_info "Run custom tests with: ${BUILD_DIR}/test-smf --target ${PROXY_ADDR} <command>"
            ;;
        0)
            print_info "Test cancelled"
            ;;
        *)
            print_error "Invalid choice"
            exit 1
            ;;
    esac

    echo ""
}

# Tail logs from all services
tail_logs() {
    if [ ! -d "$LOG_DIR" ]; then
        print_error "Log directory not found. Start services first."
        exit 1
    fi

    print_info "Tailing logs from all services (Ctrl+C to exit)..."
    echo ""

    # Use tail with multiple files
    tail -f "${LOG_DIR}"/*.log
}

# Clean logs and PIDs
clean() {
    print_info "Cleaning logs and PID files..."

    # Stop services first
    if [ -d "$PID_DIR" ]; then
        stop_all
    fi

    # Remove logs and PIDs
    rm -rf "$LOG_DIR" "$PID_DIR"

    print_success "Cleaned logs and PID files"

    # Optionally clean build artifacts
    read -p "Clean build artifacts? (y/N): " clean_build
    if [[ "$clean_build" =~ ^[Yy]$ ]]; then
        print_info "Cleaning build artifacts..."
        cargo clean
        print_success "Build artifacts cleaned"
    fi
}

# Main script logic
main() {
    case "${1:-}" in
        start)
            start_all
            ;;
        stop)
            stop_all
            ;;
        restart)
            stop_all
            sleep 1
            start_all
            ;;
        status)
            show_status
            ;;
        test)
            run_tests
            ;;
        logs)
            tail_logs
            ;;
        build)
            build_project
            ;;
        clean)
            clean
            ;;
        *)
            echo "PFCP Proxy/Load Balancer - Local Runner"
            echo ""
            echo "Usage: $0 {start|stop|restart|status|test|logs|build|clean}"
            echo ""
            echo "Commands:"
            echo "  start      Start all services (proxy + 3 UPF backends)"
            echo "  stop       Stop all services"
            echo "  restart    Restart all services"
            echo "  status     Show status of all services"
            echo "  test       Run test scenarios"
            echo "  logs       Tail logs from all services"
            echo "  build      Build the project"
            echo "  clean      Clean logs, PIDs, and optionally build artifacts"
            echo ""
            echo "Examples:"
            echo "  $0 start          # Start all services"
            echo "  $0 test           # Run interactive test menu"
            echo "  $0 logs           # Watch logs in real-time"
            echo "  $0 stop           # Stop all services"
            echo ""
            exit 1
            ;;
    esac
}

# Trap Ctrl+C to clean up
trap 'echo ""; print_warning "Interrupted. Run \"./run-local.sh stop\" to stop services."; exit 130' INT

# Run main
main "$@"
