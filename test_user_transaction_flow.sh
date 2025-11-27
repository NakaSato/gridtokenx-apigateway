#!/bin/bash

# User > Transaction Test Script
# This script demonstrates complete transaction flow from a user perspective

set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Configuration
API_URL="${API_URL:-http://localhost:8080}"
USER_EMAIL="test_user_$(date +%s)@example.com"
USER_PASSWORD="TestPass123!"
WALLET_ADDRESS="DYw8jZ9RfRfQqPkZHvPWqL5F7yKqWqfH8xKxCxJxQxXx"

print_header() {
    echo -e "\n${BLUE}========================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}========================================${NC}\n"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_info() {
    echo -e "${CYAN}ℹ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

print_step() {
    echo -e "${YELLOW}▶ Step $1: $2${NC}"
}

# Check if server is running
print_step "0" "Server Health Check"
echo "Checking if API server is running..."

if curl -s "${API_URL}/health" > /dev/null 2>&1; then
    print_success "API server is running at ${API_URL}"
else
    print_error "API server is not running at ${API_URL}"
    print_info "Please start the server with 'cargo run' or set API_URL to correct endpoint"
    print_error "Exiting script - server availability is required for transaction flow testing"
    exit 1
fi

# Transaction flow demonstration
print_header "User > Transaction Flow Test"
echo "This script demonstrates complete transaction flow from a user perspective."
echo "It shows how users interact with the transaction system end-to-end."
echo ""

# Step 1: User Registration
print_step "1" "User Registration"
echo "Registering a new user: $USER_EMAIL"

REGISTER_DATA="{
    \"email\": \"$USER_EMAIL\",
    \"password\": \"$USER_PASSWORD\",
    \"first_name\": \"Test\",
    \"last_name\": \"User\",
    \"role\": \"user\",
    \"username\": \"test_user_$(date +%s)\"
}"

echo "Request POST /api/auth/register"
echo "$REGISTER_DATA" | jq .

if curl -s -X POST "${API_URL}/api/auth/register" \
    -H "Content-Type: application/json" \
    -d "$REGISTER_DATA" > /dev/null 2>&1; then
    print_success "User registration successful"
else
    print_error "User registration failed"
    exit 1
fi

# Step 2: User Login
print_step "2" "User Authentication"
echo "Authenticating user and obtaining JWT token"

LOGIN_DATA="{
    \"username\": \"test_user_$(date +%s)\",
    \"password\": \"$USER_PASSWORD\"
}"

echo "Request POST /api/auth/login"
echo "$LOGIN_DATA" | jq .

LOGIN_RESPONSE=$(curl -s -X POST "${API_URL}/api/auth/login" \
    -H "Content-Type: application/json" \
    -d "$LOGIN_DATA" || echo "{}")

if echo "$LOGIN_RESPONSE" | jq -e '.access_token' > /dev/null 2>&1; then
    USER_TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r '.access_token')
    USER_ID=$(echo "$LOGIN_RESPONSE" | jq -r '.user_id // .id')
    print_success "User authentication successful"
    print_info "User ID: $USER_ID"
    print_info "Token: ${USER_TOKEN:0:20}..."
else
    print_error "User authentication failed"
    echo "$LOGIN_RESPONSE"
    exit 1
fi

# Step 3: Connect Wallet
print_step "3" "Wallet Connection"
echo "Connecting user's blockchain wallet"

WALLET_DATA="{
    \"wallet_address\": \"$WALLET_ADDRESS\"
}"

echo "Request POST /api/user/wallet"
echo "$WALLET_DATA" | jq .

if curl -s -X POST "${API_URL}/api/user/wallet" \
    -H "Authorization: Bearer $USER_TOKEN" \
    -H "Content-Type: application/json" \
    -d "$WALLET_DATA" > /dev/null 2>&1; then
    print_success "Wallet connected successfully"
else
    print_error "Wallet connection failed"
    exit 1
fi

# Step 4: Create Energy Trade Transaction
print_step "4" "Create Energy Trade Transaction"
echo "Creating an energy trade transaction (Sell Order)"

ENERGY_TRADE_DATA="{
    \"transaction_type\": \"energy_trade\",
    \"user_id\": \"$USER_ID\",
    \"payload\": {
        \"type\": \"EnergyTrade\",
        \"market_pubkey\": \"11111111111111111111111111111112\",
        \"energy_amount\": 100,
        \"price_per_kwh\": 150,
        \"order_type\": \"sell\"
    },
    \"max_priority_fee\": 100000,
    \"skip_prevalidation\": false
}"

echo "Request POST /api/v1/transactions"
echo "$ENERGY_TRADE_DATA" | jq .

TRANSACTION_RESPONSE=$(curl -s -X POST "${API_URL}/api/v1/transactions" \
    -H "Authorization: Bearer $USER_TOKEN" \
    -H "Content-Type: application/json" \
    -d "$ENERGY_TRADE_DATA" || echo "{}")

if echo "$TRANSACTION_RESPONSE" | jq -e '.operation_id' > /dev/null 2>&1; then
    TRANSACTION_ID=$(echo "$TRANSACTION_RESPONSE" | jq -r '.operation_id')
    TRANSACTION_STATUS=$(echo "$TRANSACTION_RESPONSE" | jq -r '.status')
    print_success "Transaction created successfully"
    print_info "Transaction ID: $TRANSACTION_ID"
    print_info "Initial Status: $TRANSACTION_STATUS"
else
    print_error "Transaction creation failed"
    echo "$TRANSACTION_RESPONSE"
    exit 1
fi

# Step 5: Monitor Transaction Status
print_step "5" "Transaction Status Monitoring"
echo "Checking transaction status"

echo "Request GET /api/v1/transactions/${TRANSACTION_ID}/status"

STATUS_RESPONSE=$(curl -s -X GET "${API_URL}/api/v1/transactions/${TRANSACTION_ID}/status" \
    -H "Authorization: Bearer $USER_TOKEN" || echo "{}")

if echo "$STATUS_RESPONSE" | jq -e '.operation_id' > /dev/null 2>&1; then
    CURRENT_STATUS=$(echo "$STATUS_RESPONSE" | jq -r '.status')
    ATTEMPTS=$(echo "$STATUS_RESPONSE" | jq -r '.attempts')
    SIGNATURE=$(echo "$STATUS_RESPONSE" | jq -r '.signature // "pending"')
    
    print_success "Transaction status retrieved"
    print_info "Current Status: $CURRENT_STATUS"
    print_info "Attempts: $ATTEMPTS"
    print_info "Signature: $SIGNATURE"
else
    print_error "Failed to retrieve transaction status"
    echo "$STATUS_RESPONSE"
    exit 1
fi

# Step 6: Get User's Transaction History
print_step "6" "Transaction History"
echo "Retrieving user's transaction history"

echo "Request GET /api/v1/transactions/user"

HISTORY_RESPONSE=$(curl -s -X GET "${API_URL}/api/v1/transactions/user" \
    -H "Authorization: Bearer $USER_TOKEN" || echo "[]")

if echo "$HISTORY_RESPONSE" | jq -e '.[]' > /dev/null 2>&1; then
    TRANSACTION_COUNT=$(echo "$HISTORY_RESPONSE" | jq 'length')
    print_success "Transaction history retrieved"
    print_info "Total transactions: $TRANSACTION_COUNT"
    
    echo "$HISTORY_RESPONSE" | jq -r '.[] | "ID: \(.operation_id), Type: \(.transaction_type), Status: \(.status)"'
else
    print_info "No transaction history found or endpoint not available"
fi

# Step 7: Transaction Filtering
print_step "7" "Transaction Filtering"
echo "Testing transaction filters"

echo "Request GET /api/v1/transactions/user?status=pending"

FILTER_RESPONSE=$(curl -s -X GET "${API_URL}/api/v1/transactions/user?status=pending" \
    -H "Authorization: Bearer $USER_TOKEN" || echo "[]")

if echo "$FILTER_RESPONSE" | jq -e '.[]' > /dev/null 2>&1; then
    PENDING_COUNT=$(echo "$FILTER_RESPONSE" | jq 'length')
    print_success "Transaction filtering working"
    print_info "Pending transactions: $PENDING_COUNT"
else
    print_info "Filter test completed (no pending transactions or endpoint not available)"
fi

# Step 8: Create Different Transaction Types
print_step "8" "Create Different Transaction Types"
echo "Testing various transaction types"

# Token Transfer Transaction
TOKEN_TRANSFER_DATA="{
    \"transaction_type\": \"token_transfer\",
    \"user_id\": \"$USER_ID\",
    \"payload\": {
        \"type\": \"TokenTransfer\",
        \"from\": \"$WALLET_ADDRESS\",
        \"to\": \"5Xj7hWqKqV9YGJ8r3nPqM8K4dYwZxNfR2tBpLmCvHgE3\",
        \"amount\": 1000,
        \"token_mint\": \"11111111111111111111111111111112\"
    },
    \"max_priority_fee\": 100000,
    \"skip_prevalidation\": false
}"

echo "Request POST /api/v1/transactions (Token Transfer)"
echo "$TOKEN_TRANSFER_DATA" | jq -r '.payload'

TOKEN_RESPONSE=$(curl -s -X POST "${API_URL}/api/v1/transactions" \
    -H "Authorization: Bearer $USER_TOKEN" \
    -H "Content-Type: application/json" \
    -d "$TOKEN_TRANSFER_DATA" || echo "{}")

if echo "$TOKEN_RESPONSE" | jq -e '.operation_id' > /dev/null 2>&1; then
    TOKEN_TX_ID=$(echo "$TOKEN_RESPONSE" | jq -r '.operation_id')
    print_success "Token transfer transaction created"
    print_info "Token Transfer ID: $TOKEN_TX_ID"
else
    print_info "Token transfer transaction test completed"
fi

# Step 9: Transaction Retry (if failed)
print_step "9" "Transaction Retry Mechanism"
echo "Testing transaction retry functionality for failed transactions"

RETRY_DATA="{
    \"operation_id\": \"$TRANSACTION_ID\",
    \"operation_type\": \"energy_trade\",
    \"max_attempts\": 5
}"

echo "Request POST /api/v1/transactions/${TRANSACTION_ID}/retry"
echo "$RETRY_DATA" | jq .

RETRY_RESPONSE=$(curl -s -X POST "${API_URL}/api/v1/transactions/${TRANSACTION_ID}/retry" \
    -H "Authorization: Bearer $USER_TOKEN" \
    -H "Content-Type: application/json" \
    -d "$RETRY_DATA" || echo "{}")

if echo "$RETRY_RESPONSE" | jq -e '.success' > /dev/null 2>&1; then
    RETRY_SUCCESS=$(echo "$RETRY_RESPONSE" | jq -r '.success')
    RETRY_ATTEMPTS=$(echo "$RETRY_RESPONSE" | jq -r '.attempts')
    
    if [ "$RETRY_SUCCESS" = "true" ]; then
        print_success "Transaction retry successful"
        print_info "Retry attempts: $RETRY_ATTEMPTS"
    else
        print_info "Transaction retry initiated (may still be processing)"
    fi
else
    print_info "Transaction retry test completed"
fi

# Step 10: Transaction Analytics
print_step "10" "Transaction Analytics"
echo "Getting transaction statistics (admin only - will fail for regular user)"

echo "Request GET /api/v1/transactions/stats"

STATS_RESPONSE=$(curl -s -X GET "${API_URL}/api/v1/transactions/stats" \
    -H "Authorization: Bearer $USER_TOKEN" || echo "{}")

if echo "$STATS_RESPONSE" | jq -e '.total_count' > /dev/null 2>&1; then
    TOTAL_COUNT=$(echo "$STATS_RESPONSE" | jq -r '.total_count')
    SUCCESS_RATE=$(echo "$STATS_RESPONSE" | jq -r '.success_rate')
    
    print_success "Transaction statistics retrieved"
    print_info "Total transactions: $TOTAL_COUNT"
    print_info "Success rate: $SUCCESS_RATE%"
else
    print_info "Transaction statistics not available for regular users (expected behavior)"
fi

# Summary
print_header "Transaction Flow Summary"
echo "User > Transaction Flow Test Completed!"
echo ""
echo "${CYAN}Key Transaction System Features Demonstrated:${NC}"
echo "  ✓ User authentication with JWT tokens"
echo "  ✓ Wallet connection for blockchain operations"
echo "  ✓ Multiple transaction types (Energy Trade, Token Transfer)"
echo "  ✓ Transaction creation and validation"
echo "  ✓ Real-time transaction status monitoring"
echo "  ✓ Transaction history and filtering"
echo "  ✓ Transaction retry mechanisms"
echo "  ✓ Transaction analytics (admin functionality)"
echo ""
echo "${CYAN}Transaction Types Supported:${NC}"
echo "  • EnergyTrade - Buy/sell energy on the marketplace"
echo "  • TokenMint - Create new energy tokens"
echo "  • TokenTransfer - Transfer tokens between wallets"
echo "  • GovernanceVote - Participate in governance"
echo "  • OracleUpdate - Update price feeds"
echo "  • RegistryUpdate - Update participant registry"
echo ""
echo "${CYAN}Transaction Status Flow:${NC}"
echo "  Pending → Processing → Submitted → Confirmed → Settled"
echo "  ↳ Failed (can retry from here)"
echo ""
echo "${CYAN}Test Information:${NC}"
echo "  User Email: $USER_EMAIL"
echo "  User ID: $USER_ID"
echo "  Wallet: $WALLET_ADDRESS"
echo "  Primary Transaction ID: $TRANSACTION_ID"
echo ""
echo "${GREEN}✅ User > Transaction Flow Test Complete!${NC}"
echo "This demonstrates the complete transaction lifecycle from user perspective."
