#!/bin/bash
# Test script for transaction history and unified queries
# This script tests the unified transaction query functionality

set -e

# Source common test utilities
if [ -f "../scripts/testing.env" ]; then
    source "../scripts/testing.env"
fi

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

# Test 1: Login as admin
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

# Test 2: Login as regular user
test_user_login() {
    log_info "Testing user login..."

    response=$(curl -s -X POST "${API_URL}/api/auth/login" \
        -H "Content-Type: application/json" \
        -d "{\"email\":\"${USER_EMAIL}\",\"password\":\"${USER_PASSWORD}\"}")

    token=$(echo "$response" | jq -r '.token')

    if [ "$token" = "null" ] || [ -z "$token" ]; then
        log_error "User login failed: $response"
        exit 1
    fi

    log_info "User login successful"
    export USER_TOKEN="$token"
}

# Test 3: Get all transaction history (admin only)
test_transaction_history() {
    log_info "Testing transaction history endpoint (admin only)..."

    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/history" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}")

    # Check if response is valid JSON
    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response for transaction history: $response"
        return 1
    fi

    # Check if response is an array
    if [ "$(echo "$response" | jq -r 'if type=="array" then "true" else "false" end')" != "true" ]; then
        log_error "Transaction history response is not an array: $response"
        return 1
    fi

    history_count=$(echo "$response" | jq 'length')
    log_info "Retrieved $history_count transactions from history"

    # Check if transactions have the expected fields
    if [ "$history_count" -gt 0 ]; then
        # Check the first transaction for required fields
        first_transaction=$(echo "$response" | jq -r '.[0]')
        required_fields=("operation_id" "operation_type" "status")

        for field in "${required_fields[@]}"; do
            if [ "$(echo "$first_transaction" | jq "has(\"$field\")")" != "true" ]; then
                log_error "Missing required field '$field' in transaction history"
                return 1
            fi
        done

        log_info "Transaction history has the expected fields"
    fi

    return 0
}

# Test 4: Test history filters
test_history_filters() {
    log_info "Testing transaction history filters..."

    # Test filtering by operation type
    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/history?operation_type=trading_order" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}")

    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response for operation type filter: $response"
        return 1
    fi

    trading_orders_count=$(echo "$response" | jq 'length')
    log_info "Found $trading_orders_count trading_order transactions"

    # Verify all results are of the correct type
    if [ "$trading_orders_count" -gt 0 ]; then
        wrong_types=$(echo "$response" | jq -r '.[] | select(.operation_type != "trading_order") | .operation_type')
        if [ -n "$wrong_types" ]; then
            log_error "Found wrong operation types in filtered results: $wrong_types"
            return 1
        fi
    fi

    # Test filtering by status
    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/history?status=confirmed" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}")

    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response for status filter: $response"
        return 1
    fi

    confirmed_count=$(echo "$response" | jq 'length')
    log_info "Found $confirmed_count confirmed transactions"

    # Verify all results have the correct status
    if [ "$confirmed_count" -gt 0 ]; then
        wrong_statuses=$(echo "$response" | jq -r '.[] | select(.status != "confirmed") | .status')
        if [ -n "$wrong_statuses" ]; then
            log_error "Found wrong statuses in filtered results: $wrong_statuses"
            return 1
        fi
    fi

    # Test pagination
    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/history?limit=5" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}")

    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response for pagination: $response"
        return 1
    fi

    paginated_count=$(echo "$response" | jq 'length')
    if [ "$paginated_count" -gt 5 ]; then
        log_error "Pagination not working correctly, got $paginated_count transactions with limit=5"
        return 1
    fi

    log_info "Pagination filter working correctly"

    # Test date range filtering
    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/history?date_from=2023-01-01T00:00:00Z" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}")

    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response for date range filter: $response"
        return 1
    fi

    date_filtered_count=$(echo "$response" | jq 'length')
    log_info "Found $date_filtered_count transactions since 2023-01-01"

    log_info "All history filters working correctly"
    return 0
}

# Test 5: Test unified query across different operation types
test_unified_query() {
    log_info "Testing unified query across different operation types..."

    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/history?limit=20" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}")

    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response for unified query: $response"
        return 1
    fi

    # Count different operation types
    trading_orders=$(echo "$response" | jq '[.[] | select(.operation_type == "trading_order")] | length')
    settlements=$(echo "$response" | jq '[.[] | select(.operation_type == "settlement")] | length')
    meter_readings=$(echo "$response" | jq '[.[] | select(.operation_type == "meter_reading")] | length')
    user_registrations=$(echo "$response" | jq '[.[] | select(.operation_type == "user_registration")] | length')

    log_info "Unified query results:"
    log_info "  Trading Orders: $trading_orders"
    log_info "  Settlements: $settlements"
    log_info "  Meter Readings: $meter_readings"
    log_info "  User Registrations: $user_registrations"

    total=$(echo "$response" | jq 'length')
    calculated_total=$((trading_orders + settlements + meter_readings + user_registrations))

    if [ "$total" -ne "$calculated_total" ]; then
        log_error "Counts don't match: total $total vs calculated $calculated_total"
        return 1
    fi

    log_info "Unified query correctly aggregates different operation types"
    return 0
}

# Test 6: Test transaction statistics endpoint
test_transaction_stats() {
    log_info "Testing transaction statistics endpoint..."

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

    # Extract stats
    total_count=$(echo "$response" | jq '.total_count')
    pending_count=$(echo "$response" | jq '.pending_count')
    submitted_count=$(echo "$response" | jq '.submitted_count')
    confirmed_count=$(echo "$response" | jq '.confirmed_count')
    failed_count=$(echo "$response" | jq '.failed_count')
    processing_count=$(echo "$response" | jq '.processing_count')
    success_rate=$(echo "$response" | jq '.success_rate')

    log_info "Transaction statistics:"
    log_info "  Total: $total_count"
    log_info "  Pending: $pending_count"
    log_info "  Submitted: $submitted_count"
    log_info "  Confirmed: $confirmed_count"
    log_info "  Failed: $failed_count"
    log_info "  Processing: $processing_count"
    log_info "  Success Rate: $success_rate"

    # Verify the counts make sense
    calculated_total=$((pending_count + submitted_count + confirmed_count + failed_count + processing_count))

    # Allow some discrepancy as there might be additional transaction statuses
    if [ "$total_count" -lt "$calculated_total" ]; then
        log_error "Total count $total_count is less than the sum of status counts $calculated_total"
        return 1
    fi

    # Check if success rate is between 0 and 1
    if (( $(echo "$success_rate < 0" | bc -l) )) || (( $(echo "$success_rate > 1" | bc -l) )); then
        log_error "Success rate $success_rate is not between 0 and 1"
        return 1
    fi

    log_info "Transaction statistics are valid"
    return 0
}

# Test 7: Test unauthorized access to admin endpoints
test_unauthorized_access() {
    log_info "Testing unauthorized access to admin endpoints..."

    # Try to access history endpoint without admin token
    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/history" \
        -H "Authorization: Bearer ${USER_TOKEN}")

    # Should get 403 Forbidden
    status_code=$(echo "$response" | jq -r '.status // empty')

    if [ -n "$status_code" ] && [ "$status_code" -eq 403 ]; then
        log_info "History endpoint correctly blocked for non-admin user"
    elif echo "$response" | grep -q "Forbidden\|Admin access required"; then
        log_info "History endpoint correctly blocked for non-admin user"
    else
        log_error "History endpoint not blocked for non-admin user: $response"
        return 1
    fi

    # Try to access stats endpoint without admin token
    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/stats" \
        -H "Authorization: Bearer ${USER_TOKEN}")

    # Should get 403 Forbidden
    status_code=$(echo "$response" | jq -r '.status // empty')

    if [ -n "$status_code" ] && [ "$status_code" -eq 403 ]; then
        log_info "Stats endpoint correctly blocked for non-admin user"
    elif echo "$response" | grep -q "Forbidden\|Admin access required"; then
        log_info "Stats endpoint correctly blocked for non-admin user"
    else
        log_error "Stats endpoint not blocked for non-admin user: $response"
        return 1
    fi

    log_info "Unauthorized access tests passed"
    return 0
}

# Test 8: Test complex filtering scenarios
test_complex_filters() {
    log_info "Testing complex filtering scenarios..."

    # Test multiple filters combined
    response=$(curl -s -X GET "${API_URL}/api/v1/transactions/history?operation_type=trading_order&status=confirmed&limit=5" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}")

    if ! echo "$response" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response for complex filters: $response"
        return 1
    fi

    count=$(echo "$response" | jq 'length')
    if [ "$count" -gt 5 ]; then
        log_error "Complex filter not working correctly, got $count transactions with limit=5"
        return 1
    fi

    # Verify all results match all filters
    if [ "$count" -gt 0 ]; then
        wrong_operations=$(echo "$response" | jq -r '.[] | select(.operation_type != "trading_order") | .operation_type')
        if [ -n "$wrong_operations" ]; then
            log_error "Found wrong operation types in complex filter results: $wrong_operations"
            return 1
        fi

        wrong_statuses=$(echo "$response" | jq -r '.[] | select(.status != "confirmed") | .status')
        if [ -n "$wrong_statuses" ]; then
            log_error "Found wrong statuses in complex filter results: $wrong_statuses"
            return 1
        fi
    fi

    # Test offset for pagination
    response1=$(curl -s -X GET "${API_URL}/api/v1/transactions/history?limit=5" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}")

    response2=$(curl -s -X GET "${API_URL}/api/v1/transactions/history?limit=5&offset=5" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}")

    if ! echo "$response1" | jq . > /dev/null 2>&1 || ! echo "$response2" | jq . > /dev/null 2>&1; then
        log_error "Invalid JSON response for pagination test"
        return 1
    fi

    # If we have at least 10 transactions, the sets should be different
    total_count=$(echo "$response1" | jq 'length')
    if [ "$total_count" -eq 5 ]; then
        response1_ids=$(echo "$response1" | jq -r '.[] | .operation_id' | sort | tr '\n' ',')
        response2_ids=$(echo "$response2" | jq -r '.[] | .operation_id' | sort | tr '\n' ',')

        if [ "$response1_ids" = "$response2_ids" ]; then
            log_warn "Offset might not be working correctly (same IDs returned)"
        else
            log_info "Pagination with offset is working correctly"
        fi
    fi

    log_info "Complex filtering scenarios working correctly"
    return 0
}

# Main test execution
main() {
    log_info "Starting Transaction History and Unified Query Tests..."

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
    test_admin_login || exit 1
    test_user_login || exit 1
    test_transaction_history || exit 1
    test_history_filters || exit 1
    test_unified_query || exit 1
    test_transaction_stats || exit 1
    test_unauthorized_access || exit 1
    test_complex_filters || exit 1

    log_info "All Transaction History and Unified Query Tests Passed! ✅"
}

# Run the main function
main
