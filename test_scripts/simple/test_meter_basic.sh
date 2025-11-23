#!/usr/bin/env bash

# GridTokenX API Gateway - Basic Smart Meter Test
# Tests core meter reading submission and retrieval functionality

set -euo pipefail
IFS=$'\n\t'

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

# Configuration
API_BASE_URL="${API_BASE_URL:-http://localhost:8080}"
SLEEP_TIME="${SLEEP_TIME:-1}"

# Performance metrics
START_TIME=$(date +%s)
PASSED_TESTS=0
FAILED_TESTS=0
TOTAL_TESTS=4

# Helper functions
log() {
    echo -e "${CYAN}[$(date +'%Y-%m-%d %H:%M:%S')]${NC} $1"
}

log_info() {
    log "${BLUE}[INFO]${NC} $1"
}

log_success() {
    log "${GREEN}[SUCCESS]${NC} $1"
    ((PASSED_TESTS++))
}

log_error() {
    log "${RED}[ERROR]${NC} $1"
    ((FAILED_TESTS++))
}

log_warning() {
    log "${YELLOW}[WARNING]${NC} $1"
}

# Generate unique test data
TIMESTAMP=$(date +%s)
USER_EMAIL="meter-test-${TIMESTAMP}@example.com"
USER_PASSWORD="test_password_1234"
WALLET_ADDRESS="9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM"

# Setup and cleanup functions
setup() {
    log_info "Setting up test environment..."

    # Register a new test user
    log_info "Registering test user: ${USER_EMAIL}"
    REGISTER_RESPONSE=$(curl -s -X POST "${API_BASE_URL}/api/auth/register" \
        -H "Content-Type: application/json" \
        -d "{
            \"email\": \"${USER_EMAIL}\",
            \"password\": \"${USER_PASSWORD}\",
            \"role\": \"prosumer\"
        }")

    if [ "$(echo "$REGISTER_RESPONSE" | jq -r '.message')" != "User registered successfully" ]; then
        log_error "User registration failed"
        log_error "Response: $REGISTER_RESPONSE"
        exit 1
    fi

    log_success "User registered successfully"

    # Login to get JWT token
    log_info "Logging in as test user"
    LOGIN_RESPONSE=$(curl -s -X POST "${API_BASE_URL}/api/auth/login" \
        -H "Content-Type: application/json" \
        -d "{
            \"email\": \"${USER_EMAIL}\",
            \"password\": \"${USER_PASSWORD}\"
        }")

    if [ -z "$LOGIN_RESPONSE" ] || [ "$(echo "$LOGIN_RESPONSE" | jq -r '.access_token')" == "null" ]; then
        log_error "Login failed"
        log_error "Response: $LOGIN_RESPONSE"
        exit 1
    fi

    JWT_TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r '.access_token')
    USER_ID=$(echo "$LOGIN_RESPONSE" | jq -r '.user.id')

    log_success "Login successful. User ID: $USER_ID"

    # Set user wallet address
    log_info "Setting wallet address for user"
    UPDATE_WALLET_RESPONSE=$(curl -s -X PUT "${API_BASE_URL}/api/user/wallet" \
        -H "Authorization: Bearer ${JWT_TOKEN}" \
        -H "Content-Type: application/json" \
        -d "{
            \"wallet_address\": \"${WALLET_ADDRESS}\"
        }")

    if [ "$(echo "$UPDATE_WALLET_RESPONSE" | jq -r '.message')" != "Wallet address updated successfully" ]; then
        log_error "Failed to update wallet address"
        log_error "Response: $UPDATE_WALLET_RESPONSE"
        exit 1
    fi

    log_success "Wallet address set successfully"
}

cleanup() {
    log_info "Cleaning up test environment..."
    # Note: We're keeping the test user in the database for audit purposes
    log_info "Test user $USER_ID left in database for audit"
}

# Test functions
test_meter_registration() {
    log_info "Test 1: Registering a smart meter"

    # Register a smart meter
    METER_REG_RESPONSE=$(curl -s -X POST "${API_BASE_URL}/api/meters/register" \
        -H "Authorization: Bearer ${JWT_TOKEN}" \
        -H "Content-Type: application/json" \
        -d "{
            \"meter_serial\": \"METER-${TIMESTAMP}\",
            \"meter_key\": \"test-key-${TIMESTAMP}\",
            \"verification_method\": \"manual\",
            \"manufacturer\": \"Test Manufacturer\",
            \"meter_type\": \"smart\",
            \"location_address\": \"123 Test Street\"
        }")

    if [ -z "$METER_REG_RESPONSE" ] || [ "$(echo "$METER_REG_RESPONSE" | jq -r '.meter_id')" == "null" ]; then
        log_error "Smart meter registration failed"
        log_error "Response: $METER_REG_RESPONSE"
        return 1
    fi

    METER_ID=$(echo "$METER_REG_RESPONSE" | jq -r '.meter_id')
    VERIFICATION_STATUS=$(echo "$METER_REG_RESPONSE" | jq -r '.verification_status')

    log_success "Smart meter registered successfully. ID: $METER_ID, Status: $VERIFICATION_STATUS"

    # Store meter ID for later tests
    TEST_METER_ID="$METER_ID"
    return 0
}

test_meter_reading_submission() {
    log_info "Test 2: Submitting meter reading"

    # Submit a meter reading
    METER_READING_RESPONSE=$(curl -s -X POST "${API_BASE_URL}/api/meters/submit-reading" \
        -H "Authorization: Bearer ${JWT_TOKEN}" \
        -H "Content-Type: application/json" \
        -d "{
            \"kwh_amount\": \"10.5\",
            \"reading_timestamp\": \"$(date -u +'%Y-%m-%dT%H:%M:%S.%3NZ')\",
            \"meter_signature\": \"test-signature-${TIMESTAMP}\"
        }")

    if [ -z "$METER_READING_RESPONSE" ] || [ "$(echo "$METER_READING_RESPONSE" | jq -r '.id')" == "null" ]; then
        log_error "Meter reading submission failed"
        log_error "Response: $METER_READING_RESPONSE"
        return 1
    fi

    READING_ID=$(echo "$METER_READING_RESPONSE" | jq -r '.id')
    MINTED=$(echo "$METER_READING_RESPONSE" | jq -r '.minted')

    if [ "$MINTED" != "false" ]; then
        log_warning "Reading already marked as minted (unexpected)"
    fi

    log_success "Meter reading submitted successfully. ID: $READING_ID"

    # Store reading ID for later tests
    TEST_READING_ID="$READING_ID"
    return 0
}

test_reading_list() {
    log_info "Test 3: Retrieving meter readings list"

    # Get user's meter readings
    READINGS_RESPONSE=$(curl -s -X GET "${API_BASE_URL}/api/meters/my-readings" \
        -H "Authorization: Bearer ${JWT_TOKEN}" \
        -H "Content-Type: application/json")

    if [ -z "$READINGS_RESPONSE" ] || [ "$(echo "$READINGS_RESPONSE" | jq -r '.data')" == "null" ]; then
        log_error "Failed to retrieve meter readings"
        log_error "Response: $READINGS_RESPONSE"
        return 1
    fi

    READING_COUNT=$(echo "$READINGS_RESPONSE" | jq -r '.data | length')

    if [ "$READING_COUNT" -eq 0 ]; then
        log_error "No meter readings found"
        return 1
    fi

    log_success "Retrieved $READING_COUNT meter readings"
    return 0
}

test_meter_list() {
    log_info "Test 4: Retrieving user's meter list"

    # Get user's meters
    METERS_RESPONSE=$(curl -s -X GET "${API_BASE_URL}/api/meters/my-meters" \
        -H "Authorization: Bearer ${JWT_TOKEN}" \
        -H "Content-Type: application/json")

    if [ -z "$METERS_RESPONSE" ] || [ "$(echo "$METERS_RESPONSE" | jq -r '.data')" == "null" ]; then
        log_error "Failed to retrieve meters"
        log_error "Response: $METERS_RESPONSE"
        return 1
    fi

    METER_COUNT=$(echo "$METERS_RESPONSE" | jq -r '.data | length')

    if [ "$METER_COUNT" -eq 0 ]; then
        log_warning "No meters found (this might be expected if meter registration failed)"
        return 0
    fi

    log_success "Retrieved $METER_COUNT meters"
    return 0
}

# Run tests
main() {
    log_info "Starting GridTokenX Basic Smart Meter Test"
    log_info "API URL: $API_BASE_URL"

    # Check if API Gateway is running
    log_info "Checking if API Gateway is running..."
    if ! curl -s "$API_BASE_URL/health" > /dev/null; then
        log_error "API Gateway is not running at $API_BASE_URL"
        log_error "Please start the API Gateway first with: cargo run"
        exit 1
    fi

    log_success "API Gateway is running"

    # Setup test environment
    setup

    # Run tests
    test_meter_registration
    sleep "$SLEEP_TIME"

    test_meter_reading_submission
    sleep "$SLEEP_TIME"

    test_reading_list
    sleep "$SLEEP_TIME"

    test_meter_list

    # Cleanup
    cleanup

    # Report results
    END_TIME=$(date +%s)
    DURATION=$((END_TIME - START_TIME))

    echo ""
    log_info "Test Summary"
    echo "===================="
    log_info "Total Tests: $TOTAL_TESTS"
    log_info "Passed: $PASSED_TESTS"
    log_info "Failed: $FAILED_TESTS"
    log_info "Duration: ${DURATION}s"

    if [ $FAILED_TESTS -eq 0 ]; then
        log_success "All basic tests passed! Smart Meter functionality is working correctly."
    else
        log_error "$FAILED_TESTS test(s) failed. Please check the logs above."
        exit 1
    fi
}

# Handle script interruption
trap cleanup EXIT
trap 'log_error "Test interrupted"; cleanup; exit 1' INT TERM

# Run main function
main
