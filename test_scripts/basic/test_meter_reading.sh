#!/usr/bin/env bash

# GridTokenX API Gateway - Smart Meter Reading Test
# Tests smart meter reading submission and retrieval

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

# Helper functions
log() {
    echo -e "${CYAN}[$(date +'%Y-%m-%d %H:%M:%S')]${NC} $1"
}

log_info() {
    log "${BLUE}[INFO]${NC} $1"
}

log_success() {
    log "${GREEN}[SUCCESS]${NC} $1"
}

log_error() {
    log "${RED}[ERROR]${NC} $1"
}

log_warning() {
    log "${YELLOW}[WARNING]${NC} $1"
}

# Generate unique test data
TIMESTAMP=$(date +%s)
USER_EMAIL="meter-reading-test-${TIMESTAMP}@example.com"
USER_PASSWORD="test_password_1234"
WALLET_ADDRESS="9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM"

# Main test function
main() {
    log_info "Testing Smart Meter Reading Submission"
    log_info "API URL: $API_BASE_URL"
    log_info "User: $USER_EMAIL"

    # Check if API Gateway is running
    if ! curl -s "$API_BASE_URL/health" > /dev/null; then
        log_error "API Gateway is not running at $API_BASE_URL"
        log_error "Please start API Gateway first"
        exit 1
    fi

    log_success "API Gateway is running"

    # Register a test user
    log_info "Registering test user"
    REGISTER_RESPONSE=$(curl -s -X POST "${API_BASE_URL}/api/auth/register" \
        -H "Content-Type: application/json" \
        -d "{
            \"email\": \"${USER_EMAIL}\",
            \"password\": \"${USER_PASSWORD}\",
            \"role\": \"prosumer\"
        }")

    if [ "$(echo "$REGISTER_RESPONSE" | jq -r '.message')" != "User registered successfully" ]; then
        log_error "User registration failed"
        echo "$REGISTER_RESPONSE"
        exit 1
    fi

    log_success "User registered successfully"

    # Login to get JWT token
    log_info "Logging in"
    LOGIN_RESPONSE=$(curl -s -X POST "${API_BASE_URL}/api/auth/login" \
        -H "Content-Type: application/json" \
        -d "{
            \"email\": \"${USER_EMAIL}\",
            \"password\": \"${USER_PASSWORD}\"
        }")

    if [ -z "$LOGIN_RESPONSE" ] || [ "$(echo "$LOGIN_RESPONSE" | jq -r '.access_token')" == "null" ]; then
        log_error "Login failed"
        echo "$LOGIN_RESPONSE"
        exit 1
    fi

    JWT_TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r '.access_token')
    USER_ID=$(echo "$LOGIN_RESPONSE" | jq -r '.user.id')
    log_success "Login successful. User ID: $USER_ID"

    # Set user wallet address
    log_info "Setting wallet address"
    UPDATE_WALLET_RESPONSE=$(curl -s -X PUT "${API_BASE_URL}/api/user/wallet" \
        -H "Authorization: Bearer ${JWT_TOKEN}" \
        -H "Content-Type: application/json" \
        -d "{
            \"wallet_address\": \"${WALLET_ADDRESS}\"
        }")

    if [ "$(echo "$UPDATE_WALLET_RESPONSE" | jq -r '.message')" != "Wallet address updated successfully" ]; then
        log_error "Failed to update wallet address"
        echo "$UPDATE_WALLET_RESPONSE"
        exit 1
    fi

    log_success "Wallet address set successfully"

    # Register a smart meter (optional, but recommended for better tracking)
    log_info "Registering smart meter"
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

    METER_ID=$(echo "$METER_REG_RESPONSE" | jq -r '.meter_id // empty')

    if [ -n "$METER_ID" ] && [ "$METER_ID" != "null" ]; then
        log_success "Smart meter registered with ID: $METER_ID"
    else
        log_warning "Smart meter registration failed, continuing without meter ID"
        METER_ID=""
    fi

    # Submit a meter reading
    log_info "Submitting meter reading"
    READING_TIMESTAMP=$(date -u +'%Y-%m-%dT%H:%M:%S.%3NZ')
    READING_KWH="10.5"
    READING_SIGNATURE="test-signature-${TIMESTAMP}"

    METER_READING_RESPONSE=$(curl -s -X POST "${API_BASE_URL}/api/meters/submit-reading" \
        -H "Authorization: Bearer ${JWT_TOKEN}" \
        -H "Content-Type: application/json" \
        -d "{
            \"kwh_amount\": \"${READING_KWH}\",
            \"reading_timestamp\": \"${READING_TIMESTAMP}\",
            \"meter_signature\": \"${READING_SIGNATURE}\"
        }")

    echo "Meter reading submission response:"
    echo "$METER_READING_RESPONSE" | jq .

    # Get reading ID and status
    READING_ID=$(echo "$METER_READING_RESPONSE" | jq -r '.id // empty')
    MINTED=$(echo "$METER_READING_RESPONSE" | jq -r '.minted // false')

    if [ -n "$READING_ID" ] && [ "$READING_ID" != "null" ]; then
        log_success "Meter reading submitted successfully"
        log_info "Reading ID: $READING_ID"
        log_info "Minted: $MINTED"
    else
        log_error "Meter reading submission failed"
        exit 1
    fi

    # Get user's meter readings
    log_info "Retrieving user's meter readings"
    READINGS_RESPONSE=$(curl -s -X GET "${API_BASE_URL}/api/meters/my-readings" \
        -H "Authorization: Bearer ${JWT_TOKEN}" \
        -H "Content-Type: application/json")

    echo "Meter readings list response:"
    echo "$READINGS_RESPONSE" | jq .

    # Check if submitted reading is in list
    READING_FOUND=$(echo "$READINGS_RESPONSE" | jq -r ".data[] | select(.id == \"$READING_ID\") | .id")

    if [ "$READING_FOUND" == "$READING_ID" ]; then
        log_success "Reading found in user's meter readings list"
    else
        log_warning "Reading not found in user's meter readings list"
    fi

    # Submit multiple readings to test pagination
    log_info "Submitting multiple readings for pagination test"
    for i in {1..3}; do
        MULTI_TIMESTAMP=$(date -u +'%Y-%m-%dT%H:%M:%S.%3NZ')
        MULTI_KWH="$((5 + i)).2"
        MULTI_SIGNATURE="multi-test-$i-${TIMESTAMP}"

        MULTI_RESPONSE=$(curl -s -X POST "${API_BASE_URL}/api/meters/submit-reading" \
            -H "Authorization: Bearer ${JWT_TOKEN}" \
            -H "Content-Type: application/json" \
            -d "{
                \"kwh_amount\": \"${MULTI_KWH}\",
                \"reading_timestamp\": \"${MULTI_TIMESTAMP}\",
                \"meter_signature\": \"${MULTI_SIGNATURE}\"
            }")

        MULTI_READING_ID=$(echo "$MULTI_RESPONSE" | jq -r '.id // empty')
        if [ -n "$MULTI_READING_ID" ] && [ "$MULTI_READING_ID" != "null" ]; then
            log_success "Additional reading $i submitted successfully (ID: $MULTI_READING_ID)"
        else
            log_warning "Additional reading $i submission failed"
        fi

        # Small delay between submissions
        sleep 1
    done

    # Test pagination with limit
    log_info "Testing pagination with limit=2"
    PAGINATED_RESPONSE=$(curl -s -X GET "${API_BASE_URL}/api/meters/my-readings?limit=2" \
        -H "Authorization: Bearer ${JWT_TOKEN}" \
        -H "Content-Type: application/json")

    echo "Paginated response (limit=2):"
    echo "$PAGINATED_RESPONSE" | jq .

    # Check total count
    TOTAL_COUNT=$(echo "$READINGS_RESPONSE" | jq -r '.total_count // 0')
    log_info "Total readings count: $TOTAL_COUNT"

    log_info "Smart Meter Reading Test completed successfully!"
}

# Run main function
main
