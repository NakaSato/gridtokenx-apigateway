#!/bin/bash

# Test Token Minting with New Mint Account
# Submits meter reading and verifies tokens are minted

set -e

API_URL="${API_URL:-http://localhost:8080}"
DB_CONTAINER="${DB_CONTAINER:-gridtokenx-postgres}"
DB_USER="${DB_USER:-gridtokenx_user}"
DB_NAME="${DB_NAME:-gridtokenx}"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
CYAN='\033[0;36m'
NC='\033[0m'

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

# Get test data
ENGINEERING_API_KEY=$(grep "ENGINEERING_API_KEY" .env | cut -d'=' -f2 | tr -d '"' | tr -d ' ')
USER_ID=$(docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -t \
    -c "SELECT id FROM users WHERE wallet_address IS NOT NULL LIMIT 1;" | tr -d ' ')
WALLET_ADDRESS=$(docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -t \
    -c "SELECT wallet_address FROM users WHERE id = '${USER_ID}';" | tr -d ' ')
METER_SERIAL=$(docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -t \
    -c "SELECT meter_serial FROM meter_registry LIMIT 1;" | tr -d ' ')

print_header "Token Minting Test"

print_info "Test Configuration:"
echo "  User ID: $USER_ID"
echo "  Wallet: ${WALLET_ADDRESS:0:30}..."
echo "  Meter: $METER_SERIAL"

# Clear previous readings to avoid duplicates
print_info "Clearing previous meter readings..."
docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} \
    -c "DELETE FROM meter_readings WHERE meter_serial = '${METER_SERIAL}';" > /dev/null
print_success "Cleared"

# Submit meter reading
print_header "Submitting Meter Reading"

READING_RESPONSE=$(curl -s -X POST "${API_URL}/api/meters/submit-reading" \
    -H "X-API-Key: ${ENGINEERING_API_KEY}" \
    -H "X-Impersonate-User: ${USER_ID}" \
    -H "Content-Type: application/json" \
    -d "{
        \"meter_serial\": \"${METER_SERIAL}\",
        \"wallet_address\": \"${WALLET_ADDRESS}\",
        \"kwh_amount\": \"10.5\",
        \"reading_timestamp\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",
        \"reading_type\": \"production\"
    }")

echo "$READING_RESPONSE" | jq '.' 2>/dev/null || echo "$READING_RESPONSE"

if echo "$READING_RESPONSE" | grep -q "reading_id\|success"; then
    print_success "Reading submitted successfully"
else
    print_error "Reading submission may have failed"
fi

# Wait for polling service to process
print_info "Waiting 70 seconds for polling service to mint tokens..."
sleep 70

# Check if tokens were minted
print_header "Verification Results"

MINTED_COUNT=$(docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -t \
    -c "SELECT COUNT(*) FROM meter_readings WHERE meter_serial = '${METER_SERIAL}' AND minted = true;" | tr -d ' ')

if [ "$MINTED_COUNT" -gt 0 ]; then
    print_success "Tokens minted! ($MINTED_COUNT reading(s))"
    
    # Get transaction signature
    MINT_TX=$(docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -t \
        -c "SELECT mint_tx_signature FROM meter_readings WHERE meter_serial = '${METER_SERIAL}' AND minted = true LIMIT 1;" | tr -d ' ')
    
    if [ -n "$MINT_TX" ] && [ "$MINT_TX" != "NULL" ]; then
        print_info "Mint transaction: ${MINT_TX:0:40}..."
    fi
    
    # Check token balance
    print_info "Checking token balance..."
    BALANCE=$(spl-token balance 7WPEWFhy7V1nW1eqcSCX6mtchnhmSzp2VKYypZiDTnYR \
        --owner ${WALLET_ADDRESS} \
        --url http://localhost:8899 2>&1 || echo "0")
    
    print_info "Token balance: $BALANCE"
    
else
    print_error "Tokens not yet minted"
    
    # Check for errors
    print_info "Checking for minting errors in database..."
    docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -c \
        "SELECT id, minted, error_message FROM meter_readings WHERE meter_serial = '${METER_SERIAL}' ORDER BY created_at DESC LIMIT 3;"
    
    print_info "Checking gateway logs for errors..."
    tail -30 gateway.log | grep -i "error\|fail" | tail -10
fi

print_header "Test Complete"

if [ "$MINTED_COUNT" -gt 0 ]; then
    print_success "✅ Token minting is working!"
    echo ""
    echo "Next steps:"
    echo "  1. Test P2P/P2C trading with actual token balances"
    echo "  2. Run full test suite: ./test_full_loop.sh"
else
    print_error "❌ Token minting still has issues"
    echo ""
    echo "Troubleshooting:"
    echo "  1. Check gateway logs: tail -f gateway.log"
    echo "  2. Verify mint account: solana account 7WPEWFhy7V1nW1eqcSCX6mtchnhmSzp2VKYypZiDTnYR"
    echo "  3. Check polling service is running"
fi
