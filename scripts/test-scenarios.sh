#!/bin/bash
# PFCP Proxy Test Scenarios
#
# Helper script to run various test scenarios using the existing rs-pfcp examples

set -e

PROXY_ADDRESS="${PROXY_ADDRESS:-pfcp-proxy}"
PROXY_PORT="${PROXY_PORT:-8805}"

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}╔════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  PFCP Proxy Test Scenarios            ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════╝${NC}"
echo ""

show_menu() {
    echo -e "${GREEN}Available Test Scenarios:${NC}"
    echo ""
    echo "  1) Basic Test          - 10 sessions (establish & delete)"
    echo "  2) Load Balance Test   - 30 sessions (perfect 3-way split)"
    echo "  3) Heavy Load Test     - 100 sessions"
    echo "  4) Stress Test         - 1000 sessions"
    echo "  5) Session Affinity    - 10 sessions with modifications"
    echo "  6) Custom              - Specify custom parameters"
    echo "  7) Continuous Load     - Keep creating sessions (Ctrl+C to stop)"
    echo ""
    echo "  q) Quit"
    echo ""
}

run_test() {
    local sessions=$1
    local name=$2

    echo -e "\n${YELLOW}═══════════════════════════════════════${NC}"
    echo -e "${YELLOW}  Running: $name${NC}"
    echo -e "${YELLOW}  Sessions: $sessions${NC}"
    echo -e "${YELLOW}═══════════════════════════════════════${NC}\n"

    session-client \
        --interface eth0 \
        --address "$PROXY_ADDRESS" \
        --port "$PROXY_PORT" \
        --sessions "$sessions"

    echo -e "\n${GREEN}✓ Test completed${NC}"
    echo -e "${BLUE}Check 'docker compose logs pfcp-proxy' for statistics${NC}\n"
}

# Main menu loop
while true; do
    show_menu
    read -p "Select scenario (1-7, q to quit): " choice

    case $choice in
        1)
            run_test 10 "Basic Test"
            ;;
        2)
            run_test 30 "Load Balance Test (30 sessions ÷ 3 UPFs = 10 each)"
            ;;
        3)
            run_test 100 "Heavy Load Test"
            ;;
        4)
            run_test 1000 "Stress Test"
            ;;
        5)
            echo -e "\n${YELLOW}Session Affinity Test${NC}"
            echo "This test creates sessions and modifies them multiple times"
            echo "All modifications for a session should go to the same UPF"
            echo ""
            run_test 10 "Session Affinity Test - Phase 1 (Establish)"
            echo "TODO: Modifications require session-modify command"
            ;;
        6)
            read -p "Enter number of sessions: " custom_sessions
            run_test "$custom_sessions" "Custom Test"
            ;;
        7)
            echo -e "\n${YELLOW}Continuous Load Test${NC}"
            echo "Creating sessions in batches... Press Ctrl+C to stop"
            echo ""
            batch=1
            while true; do
                echo -e "${BLUE}Batch $batch${NC}"
                run_test 20 "Continuous Load - Batch $batch"
                ((batch++))
                sleep 2
            done
            ;;
        q|Q)
            echo -e "\n${GREEN}Goodbye!${NC}\n"
            exit 0
            ;;
        *)
            echo -e "\n${YELLOW}Invalid choice. Please try again.${NC}\n"
            ;;
    esac

    echo ""
    read -p "Press Enter to continue..."
    clear
done
