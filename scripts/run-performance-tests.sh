#!/bin/bash
# Run performance tests for transaction tracking system
# This script runs performance tests and generates reports

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PERF_TEST_DIR="${SCRIPT_DIR}/performance-tests"
REPORT_DIR="${SCRIPT_DIR}/reports"
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
REPORT_FILE="${REPORT_DIR}/performance-report-${TIMESTAMP}.txt"

# Helper functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function to create reports directory
create_report_dir() {
    if [ ! -d "$REPORT_DIR" ]; then
        mkdir -p "$REPORT_DIR"
    fi
}

# Function to check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."

    # Check if jq is available
    if ! command -v jq &> /dev/null; then
        log_error "jq is required but not installed. Please install jq and try again."
        exit 1
    fi

    # Check if bc is available
    if ! command -v bc &> /dev/null; then
        log_error "bc is required but not installed. Please install bc and try again."
        exit 1
    fi

    # Check if performance test script exists
    if [ ! -f "${PERF_TEST_DIR}/transaction-load-test.sh" ]; then
        log_error "Performance test script not found at ${PERF_TEST_DIR}/transaction-load-test.sh"
        exit 1
    fi

    log_success "Prerequisites check passed"
}

# Function to check API server
check_api_server() {
    log_info "Checking if API server is running..."

    API_URL="${API_URL:-http://localhost:8080}"

    if ! curl -s "${API_URL}/health" > /dev/null; then
        log_error "API server is not running at ${API_URL}. Please start the server and try again."
        exit 1
    fi

    log_success "API server is running at ${API_URL}"
}

# Function to run transaction load test
run_transaction_load_test() {
    log_info "Running transaction load test..."

    cd "${PERF_TEST_DIR}"

    if bash "transaction-load-test.sh"; then
        log_success "Transaction load test completed successfully"
        return 0
    else
        log_error "Transaction load test failed"
        return 1
    fi
}

# Function to generate performance summary
generate_summary() {
    log_info "Generating performance summary..."

    create_report_dir

    cat > "$REPORT_FILE" << EOF
GridTokenX Transaction Tracking Performance Report
Generated on: $(date)
Test Environment: API_URL=${API_URL:-http://localhost:8080}

Performance Tests:
1. Transaction History Performance
2. Transaction Statistics Performance
3. User Transactions Performance
4. Filtered Queries Performance
5. Pagination Performance
6. Transaction Status Performance

(See detailed output above for each test)

Recommendations:
- Monitor response times during peak usage
- Consider implementing response caching for frequently accessed data
- Optimize database queries for filtered operations
- Implement rate limiting to prevent system overload

System Information:
- OS: $(uname -s)
- Kernel: $(uname -r)
- CPU: $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2 | xargs || echo "Unknown")
- Memory: $(free -h | grep '^Mem:' | awk '{print $2}')
EOF

    log_success "Performance report generated: $REPORT_FILE"
}

# Function to run light load test
run_light_load_test() {
    log_info "Running light load test (low stress)..."

    cd "${PERF_TEST_DIR}"
    CONCURRENT_USERS=5 REQUESTS_PER_USER=20 bash "transaction-load-test.sh"
}

# Function to run medium load test
run_medium_load_test() {
    log_info "Running medium load test (moderate stress)..."

    cd "${PERF_TEST_DIR}"
    CONCURRENT_USERS=10 REQUESTS_PER_USER=50 bash "transaction-load-test.sh"
}

# Function to run high load test
run_high_load_test() {
    log_info "Running high load test (high stress)..."

    cd "${PERF_TEST_DIR}"
    CONCURRENT_USERS=20 REQUESTS_PER_USER=100 bash "transaction-load-test.sh"
}

# Function to run sustained load test
run_sustained_load_test() {
    log_info "Running sustained load test (long duration)..."

    cd "${PERF_TEST_DIR}"
    CONCURRENT_USERS=10 REQUESTS_PER_USER=10 TEST_DURATION=180 bash "transaction-load-test.sh"
}

# Function to display usage
display_usage() {
    echo "Usage: $0 [OPTION]"
    echo ""
    echo "Options:"
    echo "  help          Show this help message"
    echo "  default        Run default performance test (medium load)"
    echo "  light          Run light load test"
    echo "  medium         Run medium load test (default)"
    echo "  high           Run high load test"
    echo "  sustained      Run sustained load test (3 minutes)"
    echo "  all            Run all test types in sequence"
    echo ""
    echo "Environment Variables:"
    echo "  API_URL        API URL to test (default: http://localhost:8080)"
    echo ""
    echo "Examples:"
    echo "  $0                    # Run default performance test"
    echo "  API_URL=http://localhost:3000 $0  # Test different API URL"
    echo "  $0 high              # Run high load test"
    echo "  $0 all               # Run all test types"
}

# Main execution
main() {
    local command=${1:-default}

    log_info "GridTokenX Transaction Tracking Performance Test Suite"
    echo ""

    # Check prerequisites
    check_prerequisites

    # Check API server
    check_api_server

    case "$command" in
        help)
            display_usage
            exit 0
            ;;
        light)
            run_light_load_test
            generate_summary
            ;;
        medium|default)
            run_medium_load_test
            generate_summary
            ;;
        high)
            run_high_load_test
            generate_summary
            ;;
        sustained)
            run_sustained_load_test
            generate_summary
            ;;
        all)
            log_info "Running all test types in sequence..."

            echo "======================================"
            log_info "Starting with light load test..."
            run_light_load_test
            echo ""

            echo "======================================"
            log_info "Continuing with medium load test..."
            run_medium_load_test
            echo ""

            echo "======================================"
            log_info "Continuing with high load test..."
            run_high_load_test
            echo ""

            echo "======================================"
            log_info "Finishing with sustained load test..."
            run_sustained_load_test
            echo ""

            generate_summary
            ;;
        *)
            log_error "Unknown command: $command"
            echo ""
            display_usage
            exit 1
            ;;
    esac

    log_success "Performance testing completed!"
}

# Run main function
main "$@"
