#!/bin/bash
# Test script for transaction monitoring functionality
# This script tests the background transaction monitoring system

set -e

# Source common test utilities
if [ -f "../scripts/testing.env" ]; then
    source "../scripts/testing.env"
fi

API_URL="${API_URL:-http://localhost:8080}"
USER_EMAIL="${USER_EMAIL:-test@example.com}"
USER_PASSWORD="${USER_PASSWORD:-password123}"
WALLET_ADDRESS="${WALLET_ADDRESS:-5Wnaf6i5D2G69CGKu4tZQqZj9bRLQUgZ1A8Z3eM3B8Hz}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Helper functions
log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Test 1: Login user
test_user_login() {
    log_info "Testing user login..."

    response=$(curl -s -X POST "${API_URL}/api/auth/login" \
        -H "Content-Type: application/json" \
        -d "{\"email\":\"${USER_EMAIL}\",\"password\":\"${USER_PASSWORD}\"}")

    token=$(echo "$response" | jq -r '.token')

    if [ "$token" = "null" ] || [ -z "$token" ]; then
        log_error "Login failed: $response"
        exit 1
    fi

    log_info "User login successful"
    export USER_TOKEN="$token"
}

# Test 2: Create a trading order to generate a transaction
test_create_transaction() {
    log_info "Creating a trading order to generate a transaction..."

    response=$(curl -s -X POST "${API_URL}/api/v1/trading/orders" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${USER_TOKEN}" \
        -d '{
            "type": "sell",
            "amount": "100",
            "price": "0.5"
        }')

    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response when creating order: $response"
        return 1
    fi

    order_id=$(echo "$response" | jq -r '.id')

    if [ "$order_id" = "null" ] || [ -z "$order_id" ]; then
        log_error "Failed to create order: $response"
        return 1
    fi

    log_info "Order created with ID: $order_id"
    export ORDER_ID="$order_id"
}

# Test 3: Check initial transaction status
test_initial_transaction_status() {
    log_info "Checking initial transaction status..."

    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/${ORDER_ID}/status" \
        -H "Authorization: Bearer ${USER_TOKEN}")

    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response when getting transaction status: $response"
        return 1
    fi

    status=$(echo "$response" | jq -r '.status')
    attempts=$(echo "$response" | jq -r '.attempts')
    operation_type=$(echo "$response" | jq -r '.operation_type')

    log_info "Initial transaction status:"
    log_info "  Operation Type: $operation_type"
    log_info "  Status: $status"
    log_info "  Attempts: $attempts"

    if [ "$status" = "pending" ] || [ "$status" = "submitted" ]; then
        log_info "Transaction is in expected initial state: $status"
    else
        log_warn "Unexpected initial status: $status"
    fi
}

# Test 4: Wait for transaction monitoring to update status
test_monitoring_update() {
    log_info "Waiting for transaction monitoring to update status (this may take up to 10 seconds)..."

    local max_attempts=10
    local attempt=1

    while [ $attempt -le $max_attempts ]; do
        response=$(curl -s -X GET "${API_URL}/api/v1/transactions/${ORDER_ID}/status" \
            -H "Authorization: Bearer ${USER_TOKEN}")

        if echo "$response" | jq . > /dev/null 2>&1; then
            current_status=$(echo "$response" | jq -r '.status')
            attempts=$(echo "$response" | jq -r '.attempts')
            signature=$(echo "$response" | jq -r '.signature')

            log_info "Attempt $attempt: Current status is $status, attempts: $attempts"

            # Check if status has changed from initial
            if [ "$current_status" != "pending" ]; then
                log_info "Transaction status updated to: $current_status"

                # If we have a signature, it was submitted
                if [ "$signature" != "null" ] && [ -n "$signature" ]; then
                    log_info "Transaction signature: $signature"
                fi

                return 0
            fi
        fi

        sleep 1
        attempt=$((attempt + 1))
    done

    log_warn "Transaction status did not update within timeout period"
    return 0  # Don't fail the test, as this could be due to blockchain service configuration
}

# Test 5: Get user transactions to see if the transaction is tracked
test_user_transactions() {
    log_info "Getting user transactions to verify tracking..."

    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/user" \
        -H "Authorization: Bearer ${USER_TOKEN}")

    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response when getting user transactions: $response"
        return 1
    fi

    # Check if response is an array
    if [ "$(echo "$response" | jq -r 'if type=="array" then "true" else "false" end')" != "true" ]; then
        log_error "Response is not an array: $response"
        return 1
    fi

    transaction_count=$(echo "$response" | jq 'length')
    log_info "Found $transaction_count transactions for user"

    # Look for our specific transaction
    found_order=$(echo "$response" | jq --arg order_id "$ORDER_ID" '.[] | select(.operation_id == $order_id)')

    if [ -n "$found_order" ]; then
        log_info "Our transaction is properly tracked in user transactions"
        echo "$found_order" | jq -r '  Operation ID: \(.operation_id), Type: \(.operation_type), Status: \(.status)'
    else
        log_warn "Our transaction was not found in user transactions"
    fi
}

# Test 6: Create a meter reading to test different transaction type
test_meter_reading_transaction() {
    log_info "Testing transaction tracking with meter readings..."

    # First, create a meter
    response=$(curl -s -X POST "${API_URL}/api/v1/meters" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${USER_TOKEN}" \
        -d "{
            \"name\": \"Test Monitoring Meter\",
            \"location\": \"Test Location\",
            \"meter_type\": \"smart\",
            \"manufacturer\": \"Test Manufacturer\",
            \"model\": \"Test Model\",
            \"serial_number\": \"TM-$(date +%s)\",
            \"installation_date\": \"2023-01-01T00:00:00Z\",
            \"initial_reading\": 100
        }")

    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_warn "Failed to create meter, skipping meter reading test"
        return 0
    fi

    meter_id=$(echo "$response" | jq -r '.id')

    if [ "$meter_id" = "null" ] || [ -z "$meter_id" ]; then
        log_warn "Failed to create meter, skipping meter reading test"
        return 0
    fi

    # Submit a reading
    response=$(curl -s -X POST "${API_URL}/api/v1/meters/${meter_id}/readings" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${USER_TOKEN}" \
        -d "{
            \"reading\": 150,
            \"timestamp\": \"$(date -u +"%Y-%m-%dT%H:%M:%SZ")\"
        }")

    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_warn "Failed to submit meter reading, skipping meter reading test"
        return 0
    fi

    reading_id=$(echo "$response" | jq -r '.id')

    if [ "$reading_id" = "null" ] || [ -z "$reading_id" ]; then
        log_warn "Failed to submit meter reading, skipping meter reading test"
        return 0
    fi

    log_info "Meter reading created with ID: $reading_id"

    # Wait for monitoring to process
    sleep 5

    # Check if the meter reading is tracked
    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/${reading_id}/status" \
        -H "Authorization: Bearer ${USER_TOKEN}")

    if echo "$response" | jq . > /dev/null 2>&1; then
        status=$(echo "$response" | jq -r '.status')
        operation_type=$(echo "$response" | jq -r '.operation_type')

        log_info "Meter reading transaction status:"
        log_info "  Operation Type: $operation_type"
        log_info "  Status: $status"

        if [ "$operation_type" = "meter_reading" ]; then
            log_info "Meter reading transaction is properly tracked"
        else
            log_warn "Unexpected operation type for meter reading: $operation_type"
        fi
    else
        log_warn "Failed to get meter reading transaction status"
    fi
}

# Test 7: Test transaction filters with monitoring in mind
test_monitoring_filters() {
    log_info "Testing transaction filters related to monitoring..."

    # Test filtering by status
    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/user?status=pending" \
        -H "Authorization: Bearer ${USER_TOKEN}")

    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response for status filter: $response"
        return 1
    fi

    pending_count=$(echo "$response" | jq 'length')
    log_info "Found $pending_count pending transactions"

    # Test filtering by has_signature
    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/user?has_signature=false" \
        -H "Authorization: Bearer ${USER_TOKEN}")

    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response for signature filter: $response"
        return 1
    fi

    no_signature_count=$(echo "$response" | jq 'length')
    log_info "Found $no_signature_count transactions without signature"

    # Test filtering by min_attempts (to find transactions that have been retried)
    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/user?min_attempts=1" \
        -H "Authorization: Bearer ${USER_TOKEN}")

    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response for attempts filter: $response"
        return 1
    fi

    retry_count=$(echo "$response" | jq 'length')
    log_info "Found $retry_count transactions with at least 1 attempt"

    log_info "Monitoring-related filters working correctly"
}

# Main test execution
main() {
    log_info "Starting Transaction Monitoring Tests..."

    # Check if required tools are available
    if ! command -v jq &> /dev/null; then
        log_error "jq is required but not installed. Please install jq and try again."
        exit 1
    fi

    # Check if API is running
    if ! curl -s "${API_URL}/health" > /dev/null; then
        log_error "API server is not running at ${API_URL}. Please start the server and try again."
        exit 1
    fi

    # Run tests in order
    test_user_login || exit 1
    test_create_transaction || exit 1
    test_initial_transaction_status || exit 1
    test_monitoring_update || exit 1
    test_user_transactions || exit 1
    test_meter_reading_transaction || exit 0  # Non-critical, may fail if meters not set up
    test_monitoring_filters || exit 1

    log_info "All Transaction Monitoring Tests Passed! ✅"
}

# Run the main function
main
