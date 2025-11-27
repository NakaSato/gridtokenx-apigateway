#!/bin/bash

# Test script for Transaction Tracking functionality
# This script tests the unified transaction tracking system

set -e

# Source common test utilities
if [ -f "../scripts/testing.env" ]; then
    source "../scripts/testing.env"
fi

# Configuration
API_URL="${API_URL:-http://localhost:8080}"
USER_EMAIL="${USER_EMAIL:-test@example.com}"
USER_PASSWORD="${USER_PASSWORD:-password123}"
ADMIN_EMAIL="${ADMIN_EMAIL:-admin@example.com}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-adminpass123}"

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

# Test 1: User login and get token
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

# Test 2: Admin login and get token
test_admin_login() {
    log_info "Testing admin login..."

    response=$(curl -s -X POST "${API_URL}/api/auth/login" \
        -H "Content-Type: application/json" \
        -d "{\"email\":\"${ADMIN_EMAIL}\",\"password\":\"${ADMIN_PASSWORD}\"}")

    token=$(echo "$response" | jq -r '.token')

    if [ "$token" = "null" ] || [ -z "$token" ]; then
        log_error "Admin login failed: $response"
        exit 1
    fi

    log_info "Admin login successful"
    export ADMIN_TOKEN="$token"
}

# Test 3: Get user transactions
test_get_user_transactions() {
    log_info "Testing get user transactions..."

    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/user" \
        -H "Authorization: Bearer ${USER_TOKEN}")

    # Check if response is valid JSON
    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response: $response"
        return 1
    fi

    # Check if response is an array
    if [ "$(echo "$response" | jq -r 'if type=="array" then "true" else "false" end')" != "true" ]; then
        log_error "Response is not an array: $response"
        return 1
    fi

    transaction_count=$(echo "$response" | jq 'length')
    log_info "Found $transaction_count transactions for user"

    # If we have transactions, test filtering
    if [ "$transaction_count" -gt 0 ]; then
        test_transaction_filters
    else
        log_warn "No transactions found for user, skipping filter tests"
    fi

    return 0
}

# Test 4: Test transaction filters
test_transaction_filters() {
    log_info "Testing transaction filters..."

    # Test status filter
    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/user?status=confirmed" \
        -H "Authorization: Bearer ${USER_TOKEN}")

    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response for status filter: $response"
        return 1
    fi

    confirmed_count=$(echo "$response" | jq 'length')
    log_info "Found $confirmed_count confirmed transactions"

    # Test limit filter
    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/user?limit=5" \
        -H "Authorization: Bearer ${USER_TOKEN}")

    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response for limit filter: $response"
        return 1
    fi

    limited_count=$(echo "$response" | jq 'length')
    if [ "$limited_count" -gt 5 ]; then
        log_error "Limit filter not working, got $limited_count transactions with limit=5"
        return 1
    fi

    log_info "Limit filter working correctly"
    return 0
}

# Test 5: Get transaction statistics (admin only)
test_transaction_stats() {
    log_info "Testing transaction statistics (admin only)..."

    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/stats" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}")

    # Check if response is valid JSON
    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response for stats: $response"
        return 1
    fi

    # Check if required fields are present
    required_fields=("total_count" "pending_count" "submitted_count" "confirmed_count" "failed_count" "processing_count" "success_rate")
    for field in "${required_fields[@]}"; do
        if [ "$(echo "$response" | jq "has(\"$field\")")" != "true" ]; then
            log_error "Missing required field '$field' in stats response: $response"
            return 1
        fi
    done

    total_count=$(echo "$response" | jq '.total_count')
    success_rate=$(echo "$response" | jq '.success_rate')

    log_info "Stats retrieved successfully: $total_count total transactions, $success_rate success rate"

    return 0
}

# Test 6: Get transaction history (admin only)
test_transaction_history() {
    log_info "Testing transaction history (admin only)..."

    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/history" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}")

    # Check if response is valid JSON
    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response for history: $response"
        return 1
    fi

    # Check if response is an array
    if [ "$(echo "$response" | jq -r 'if type=="array" then "true" else "false" end')" != "true" ]; then
        log_error "History response is not an array: $response"
        return 1
    fi

    history_count=$(echo "$response" | jq 'length')
    log_info "Retrieved $history_count transactions from history"

    return 0
}

# Test 7: Test transaction status by ID
test_transaction_status_by_id() {
    log_info "Testing transaction status by ID..."

    # First, get some transactions for the user
    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/user?limit=1" \
        -H "Authorization: Bearer ${USER_TOKEN}")

    # Check if we have any transactions
    if [ "$(echo "$response" | jq 'length')" -eq 0 ]; then
        log_warn "No transactions found for user, skipping transaction status test"
        return 0
    fi

    # Get the first transaction ID
    transaction_id=$(echo "$response" | jq -r '.[0].operation_id')

    # Get the transaction status
    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/${transaction_id}/status" \
        -H "Authorization: Bearer ${USER_TOKEN}")

    # Check if response is valid JSON
    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response for transaction status: $response"
        return 1
    fi

    # Check if required fields are present
    required_fields=("operation_id" "status" "operation_type")
    for field in "${required_fields[@]}"; do
        if [ "$(echo "$response" | jq "has(\"$field\")")" != "true" ]; then
            log_error "Missing required field '$field' in transaction status response: $response"
            return 1
        fi
    done

    status=$(echo "$response" | jq -r '.status')
    operation_type=$(echo "$response" | jq -r '.operation_type')

    log_info "Transaction status retrieved successfully: $operation_type ($status)"

    return 0
}

# Test 8: Test unauthorized access
test_unauthorized_access() {
    log_info "Testing unauthorized access..."

    # Try to access user transactions without token
    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/user")

    # Should get 401 Unauthorized
    status_code=$(echo "$response" | jq -r '.status // empty')

    if [ -n "$status_code" ] && [ "$status_code" -eq 401 ]; then
        log_info "Unauthorized access correctly blocked"
    elif echo "$response" | grep -q "Unauthorized\|Authentication required"; then
        log_info "Unauthorized access correctly blocked"
    else
        log_error "Unauthorized access not blocked: $response"
        return 1
    fi

    # Try to access admin endpoints with user token
    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/stats" \
        -H "Authorization: Bearer ${USER_TOKEN}")

    # Should get 403 Forbidden
    status_code=$(echo "$response" | jq -r '.status // empty')

    if [ -n "$status_code" ] && [ "$status_code" -eq 403 ]; then
        log_info "Admin endpoint correctly blocked for user"
    elif echo "$response" | grep -q "Forbidden\|Admin access required"; then
        log_info "Admin endpoint correctly blocked for user"
    else
        log_error "Admin endpoint not blocked for user: $response"
        return 1
    fi

    return 0
}

# Main test execution
main() {
    log_info "Starting Transaction Tracking Tests..."

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
    test_admin_login || exit 1
    test_get_user_transactions || exit 1
    test_transaction_stats || exit 1
    test_transaction_history || exit 1
    test_transaction_status_by_id || exit 1
    test_unauthorized_access || exit 1

    log_info "All Transaction Tracking Tests Passed! ✅"
}

# Run the main function
main
