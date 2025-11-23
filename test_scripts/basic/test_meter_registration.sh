#!/usr/bin/env bash

# GridTokenX API Gateway - Smart Meter Registration Test
# Tests smart meter registration functionality

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

# Generate unique test data
TIMESTAMP=$(date +%s)
USER_EMAIL="meter-reg-test-${TIMESTAMP}@example.com"
USER_PASSWORD="test_password_1234"
WALLET_ADDRESS="9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM"

# Main test function
main() {
    log_info "Testing Smart Meter Registration"
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
    log_success "Login successful"

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

    # Register a smart meter
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

    echo "Meter registration response:"
    echo "$METER_REG_RESPONSE" | jq .

    # Get meter ID and status
    METER_ID=$(echo "$METER_REG_RESPONSE" | jq -r '.meter_id // empty')
    VERIFICATION_STATUS=$(echo "$METER_REG_RESPONSE" | jq -r '.verification_status // empty')

    if [ -n "$METER_ID" ] && [ "$METER_ID" != "null" ]; then
        log_success "Smart meter registered successfully"
        log_info "Meter ID: $METER_ID"
        log_info "Verification Status: $VERIFICATION_STATUS"
    else
        log_error "Smart meter registration failed"
        exit 1
    fi

    # Get user's meters
    log_info "Retrieving user's meters"
    METERS_RESPONSE=$(curl -s -X GET "${API_BASE_URL}/api/meters/my-meters" \
        -H "Authorization: Bearer ${JWT_TOKEN}" \
        -H "Content-Type: application/json")

    echo "Meters list response:"
    echo "$METERS_RESPONSE" | jq .

    # Check if registered meter is in the list
    METER_FOUND=$(echo "$METERS_RESPONSE" | jq -r ".data[] | select(.id == \"$METER_ID\") | .id")

    if [ "$METER_FOUND" == "$METER_ID" ]; then
        log_success "Meter found in user's meter list"
    else
        log_warning "Meter not found in user's meter list"
    fi

    log_info "Smart Meter Registration Test completed successfully!"
}

# Run main function
main
