#!/bin/bash
# Combined test runner for all transaction tracking tests
# This script runs all transaction-related test scripts in order

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SUCCESS_COUNT=0
FAILURE_COUNT=0

# Function to print colored output
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

# Function to run a test script
run_test() {
    local test_script=$1
    local test_name=$2

    log_info "Running test: $test_name"
    log_info "Script: $test_script"

    echo "========================================================="

    if bash "$test_script"; then
        log_success "Test passed: $test_name"
        SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
    else
        log_error "Test failed: $test_name"
        FAILURE_COUNT=$((FAILURE_COUNT + 1))
    fi

    echo "========================================================="
    echo ""
}

# Main execution
main() {
    log_info "Starting Transaction Tracking Test Suite"
    log_info "This suite tests all transaction tracking functionality"

    # Check if required tools are available
    if ! command -v jq &> /dev/null; then
        log_error "jq is required but not installed. Please install jq and try again."
        exit 1
    fi

    # Check if API is running
    API_URL="${API_URL:-http://localhost:8080}"
    if ! curl -s "${API_URL}/health" > /dev/null; then
        log_error "API server is not running at ${API_URL}. Please start the server and try again."
        exit 1
    fi

    log_info "API is running at ${API_URL}"
    log_info "Starting test suite..."
    echo ""

    # Run test scripts in order
    run_test "$SCRIPT_DIR/test-transaction-tracking.sh" "Transaction Tracking"
    run_test "$SCRIPT_DIR/test-transaction-monitoring.sh" "Transaction Monitoring"
    run_test "$SCRIPT_DIR/test-transaction-history.sh" "Transaction History"

    # Print summary
    log_info "Test Suite Summary:"
    log_success "Passed: $SUCCESS_COUNT"
    log_error "Failed: $FAILURE_COUNT"

    if [ $FAILURE_COUNT -eq 0 ]; then
        log_success "All tests passed! 🎉"
        exit 0
    else
        log_error "Some tests failed. Please check the output above for details."
        exit 1
    fi
}

# Run main function
main "$@"
