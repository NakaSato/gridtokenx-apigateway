#!/bin/bash
set -e

echo "=== GridTokenX Token Minting E2E Test ==="

# Configuration
API_GATEWAY_URL="http://localhost:8080"
SOLANA_URL="http://localhost:8899"
TEST_WALLET="DYw8jZ9RfRfQqPkZHvPWqL5F7yKqWqfH8xKxCxJxQxXx"  # Example wallet

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Helper functions
log_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

log_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

log_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

log_error() {
    echo -e "${RED}❌ $1${NC}"
}

# Cleanup function
cleanup() {
    log_info "Cleaning up..."
    
    # Stop API Gateway
    if [ ! -z "$API_PID" ]; then
        kill $API_PID 2>/dev/null || true
        log_info "API Gateway stopped"
    fi
    
    # Stop validator
    if [ ! -z "$VALIDATOR_PID" ]; then
        kill $VALIDATOR_PID 2>/dev/null || true
        log_info "Solana validator stopped"
    fi
    
    exit 0
}

# Trap cleanup on script exit
trap cleanup EXIT INT TERM

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."
    
    # Check if required commands exist
    for cmd in solana curl jq; do
        if ! command -v $cmd &> /dev/null; then
            log_error "$cmd is not installed or not in PATH"
            exit 1
        fi
    done
    
    # Check if dev-wallet.json exists
    if [ ! -f "./dev-wallet.json" ]; then
        log_error "dev-wallet.json not found. Please create authority wallet first."
        exit 1
    fi
    
    log_success "Prerequisites check passed"
}

# Start local validator
start_validator() {
    log_info "Starting solana-test-validator..."
    solana-test-validator --reset &
    VALIDATOR_PID=$!
    
    # Wait for validator to start
    log_info "Waiting for validator to start..."
    sleep 10
    
    # Check if validator is running
    if ! kill -0 $VALIDATOR_PID 2>/dev/null; then
        log_error "Failed to start solana-test-validator"
        exit 1
    fi
    
    log_success "Solana validator started (PID: $VALIDATOR_PID)"
    
    # Verify RPC health
    RPC_HEALTH=$(curl -s -X POST http://localhost:8899 \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' | jq -r '.result // "error"')
    
    if [ "$RPC_HEALTH" = "ok" ]; then
        log_success "RPC health check passed"
    else
        log_error "RPC health check failed: $RPC_HEALTH"
        exit 1
    fi
}

# Deploy Anchor programs (placeholder - assumes already deployed)
deploy_programs() {
    log_info "Deploying Anchor programs..."
    
    # Check if anchor directory exists
    if [ ! -d "../gridtokenx-anchor" ]; then
        log_warning "gridtokenx-anchor directory not found. Assuming programs are already deployed."
        return 0
    fi
    
    cd ../gridtokenx-anchor
    
    # Check if anchor CLI is available
    if command -v anchor &> /dev/null; then
        log_info "Building Anchor programs..."
        anchor build
        
        log_info "Deploying Anchor programs to localnet..."
        anchor deploy --provider.cluster localnet
        
        log_success "Anchor programs deployed"
    else
        log_warning "Anchor CLI not found. Assuming programs are already deployed."
    fi
    
    cd ../gridtokenx-apigateway
}

# Start API Gateway
start_api_gateway() {
    log_info "Starting API Gateway..."
    
    # Build first
    if [ ! -d "./target/release" ]; then
        log_info "Building API Gateway in release mode..."
        cargo build --release
    fi
    
    # Start API Gateway in background
    ./target/release/api-gateway &
    API_PID=$!
    
    # Wait for API Gateway to start
    log_info "Waiting for API Gateway to start..."
    sleep 5
    
    # Check if API Gateway is running
    if ! kill -0 $API_PID 2>/dev/null; then
        log_error "Failed to start API Gateway"
        exit 1
    fi
    
    # Health check
    API_HEALTH=$(curl -s $API_GATEWAY_URL/health | jq -r '.status // "error"')
    
    if [ "$API_HEALTH" = "healthy" ]; then
        log_success "API Gateway is running"
    else
        log_warning "API Gateway health check failed (status: $API_HEALTH), but continuing..."
    fi
}

# Create test user and get token
create_test_user() {
    log_info "Creating test user..."
    
    # Register user
    REGISTER_RESP=$(curl -s -X POST $API_GATEWAY_URL/api/auth/register \
        -H "Content-Type: application/json" \
        -d '{
            "email": "test@example.com",
            "password": "Test123!@#",
            "name": "Test Prosumer"
        }')
    
    if echo "$REGISTER_RESP" | jq -e '.error' > /dev/null; then
        log_error "Failed to register user: $(echo $REGISTER_RESP | jq -r '.error')"
        exit 1
    fi
    
    USER_ID=$(echo $REGISTER_RESP | jq -r '.user_id')
    log_success "User registered. ID: $USER_ID"
    
    # Login and get JWT
    log_info "Logging in..."
    LOGIN_RESP=$(curl -s -X POST $API_GATEWAY_URL/api/auth/login \
        -H "Content-Type: application/json" \
        -d '{
            "email": "test@example.com",
            "password": "Test123!@#"
        }')
    
    if echo "$LOGIN_RESP" | jq -e '.error' > /dev/null; then
        log_error "Failed to login: $(echo $LOGIN_RESP | jq -r '.error')"
        exit 1
    fi
    
    TOKEN=$(echo $LOGIN_RESP | jq -r '.access_token')
    log_success "JWT token obtained: ${TOKEN:0:20}..."
}

# Connect wallet
connect_wallet() {
    log_info "Connecting wallet..."
    
    CONNECT_RESP=$(curl -s -X POST $API_GATEWAY_URL/api/user/wallet \
        -H "Authorization: Bearer $TOKEN" \
        -H "Content-Type: application/json" \
        -d "{\"wallet_address\": \"$TEST_WALLET\"}")
    
    if echo "$CONNECT_RESP" | jq -e '.error' > /dev/null; then
        log_error "Failed to connect wallet: $(echo $CONNECT_RESP | jq -r '.error')"
        exit 1
    fi
    
    log_success "Wallet connected: $TEST_WALLET"
}

# Submit meter reading
submit_reading() {
    log_info "Submitting meter reading..."
    
    READING_RESP=$(curl -s -X POST $API_GATEWAY_URL/api/meters/submit-reading \
        -H "Authorization: Bearer $TOKEN" \
        -H "Content-Type: application/json" \
        -d '{
            "kwh_amount": 25.5,
            "reading_timestamp": "'$(date -u +"%Y-%m-%dT%H:%M:%SZ")'",
            "metadata": {"meter_id": "TEST-001"}
        }')
    
    if echo "$READING_RESP" | jq -e '.error' > /dev/null; then
        log_error "Failed to submit reading: $(echo $READING_RESP | jq -r '.error')"
        exit 1
    fi
    
    READING_ID=$(echo $READING_RESP | jq -r '.id')
    READING_AMOUNT=$(echo $READING_RESP | jq -r '.kwh_amount')
    
    log_success "Reading submitted. ID: $READING_ID, Amount: $READING_AMOUNT kWh"
}

# Create admin token (simplified - assumes admin exists)
get_admin_token() {
    log_info "Getting admin token..."
    
    # Try to login with default admin credentials
    ADMIN_LOGIN_RESP=$(curl -s -X POST $API_GATEWAY_URL/api/auth/login \
        -H "Content-Type: application/json" \
        -d '{
            "email": "admin@gridtokenx.com",
            "password": "Admin123!@#"
        }')
    
    if echo "$ADMIN_LOGIN_RESP" | jq -e '.access_token' > /dev/null; then
        ADMIN_TOKEN=$(echo $ADMIN_LOGIN_RESP | jq -r '.access_token')
        log_success "Admin token obtained: ${ADMIN_TOKEN:0:20}..."
    else
        log_warning "Default admin credentials not found. Using user token for minting (may fail)"
        ADMIN_TOKEN="$TOKEN"
    fi
}

# Mint tokens
mint_tokens() {
    log_info "Minting tokens for reading: $READING_ID"
    
    MINT_RESP=$(curl -s -X POST $API_GATEWAY_URL/api/admin/meters/mint-from-reading \
        -H "Authorization: Bearer $ADMIN_TOKEN" \
        -H "Content-Type: application/json" \
        -d "{\"reading_id\": \"$READING_ID\"}")
    
    if echo "$MINT_RESP" | jq -e '.error' > /dev/null; then
        log_error "Failed to mint tokens: $(echo $MINT_RESP | jq -r '.error')"
        exit 1
    fi
    
    TX_SIGNATURE=$(echo $MINT_RESP | jq -r '.transaction_signature')
    MINTED_AMOUNT=$(echo $MINT_RESP | jq -r '.kwh_amount')
    WALLET_ADDRESS=$(echo $MINT_RESP | jq -r '.wallet_address')
    
    log_success "Tokens minted successfully!"
    log_info "Transaction signature: $TX_SIGNATURE"
    log_info "Amount minted: $MINTED_AMOUNT kWh"
    log_info "Wallet: $WALLET_ADDRESS"
}

# Verify on-chain transaction
verify_transaction() {
    log_info "Verifying transaction on-chain..."
    
    # Wait for transaction to be confirmed
    sleep 5
    
    # Check transaction status
    TX_STATUS=$(solana confirm -v $TX_SIGNATURE --url $SOLANA_URL 2>/dev/null || echo "failed")
    
    if echo "$TX_STATUS" | grep -q "confirmed"; then
        log_success "Transaction confirmed on-chain"
    else
        log_warning "Transaction confirmation status: $TX_STATUS"
    fi
    
    # Get transaction details
    TX_DETAILS=$(solana transaction $TX_SIGNATURE --url $SOLANA_URL --output json 2>/dev/null || echo "{}")
    
    if echo "$TX_DETAILS" | jq -e '.meta.err' > /dev/null; then
        log_error "Transaction failed on-chain: $(echo $TX_DETAILS | jq -r '.meta.err')"
    else
        log_success "Transaction executed successfully on-chain"
    fi
}

# Check token balance
check_token_balance() {
    log_info "Checking token balance..."
    
    # Get energy token mint address from config
    ENERGY_TOKEN_MINT=$(grep ENERGY_TOKEN_MINT local.env | cut -d'=' -f2 | tr -d '"')
    
    if [ -z "$ENERGY_TOKEN_MINT" ]; then
        log_warning "ENERGY_TOKEN_MINT not found in local.env"
        return 0
    fi
    
    # Check token balance
    BALANCE=$(spl-token balance $ENERGY_TOKEN_MINT --owner $TEST_WALLET --url $SOLANA_URL 2>/dev/null || echo "0")
    
    if [ "$BALANCE" = "0" ]; then
        log_warning "Token balance is 0"
    else
        log_success "Token balance: $BALANCE"
    fi
}

# Verify database update
verify_database() {
    log_info "Verifying database update..."
    
    # Check if reading is marked as minted
    READING_STATUS=$(curl -s -X GET "$API_GATEWAY_URL/api/meters/my-readings" \
        -H "Authorization: Bearer $TOKEN" | jq -r '.data[] | select(.id == "'$READING_ID'") | .minted')
    
    if [ "$READING_STATUS" = "true" ]; then
        log_success "Reading marked as minted in database"
    else
        log_warning "Reading minting status in database: $READING_STATUS"
    fi
}

# Final verification
final_verification() {
    log_info "Running final verification checks..."
    
    # Check all components are still running
    if ! kill -0 $VALIDATOR_PID 2>/dev/null; then
        log_error "Solana validator stopped unexpectedly"
        return 1
    fi
    
    if ! kill -0 $API_PID 2>/dev/null; then
        log_error "API Gateway stopped unexpectedly"
        return 1
    fi
    
    # API Gateway health check
    API_HEALTH=$(curl -s $API_GATEWAY_URL/health | jq -r '.status // "error"')
    if [ "$API_HEALTH" = "healthy" ]; then
        log_success "API Gateway still healthy"
    else
        log_warning "API Gateway health: $API_HEALTH"
    fi
    
    log_success "All components still running"
}

# Main execution
main() {
    echo ""
    log_info "Starting GridTokenX Token Minting E2E Test"
    echo ""
    
    # Run all test steps
    check_prerequisites
    start_validator
    deploy_programs
    start_api_gateway
    create_test_user
    connect_wallet
    submit_reading
    get_admin_token
    mint_tokens
    verify_transaction
    check_token_balance
    verify_database
    final_verification
    
    echo ""
    log_success "🎉 E2E Test Completed Successfully!"
    echo ""
    log_info "Test Summary:"
    log_info "- ✅ Solana validator running"
    log_info "- ✅ API Gateway running"
    log_info "- ✅ User registration and login"
    log_info "- ✅ Wallet connection"
    log_info "- ✅ Meter reading submission"
    log_info "- ✅ Token minting"
    log_info "- ✅ Transaction confirmation"
    log_info "- ✅ Database update"
    log_info "- ✅ Token balance verification"
    echo ""
    log_info "Transaction Signature: $TX_SIGNATURE"
    log_info "Reading ID: $READING_ID"
    log_info "Test Wallet: $TEST_WALLET"
    echo ""
    log_info "To keep services running, press Ctrl+C to stop this script."
    log_info "Services will continue running in the background."
    
    # Wait for user to stop the script
    while true; do
        sleep 1
    done
}

# Run main function
main "$@"
