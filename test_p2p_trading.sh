#!/bin/bash
# =============================================================================
# P2P Trading End-to-End Test Script
# =============================================================================
# This script tests the complete P2P trading flow:
# 1. Setup test users with wallets
# 2. Mint tokens to seller
# 3. Create sell order (prosumer)
# 4. Create buy order (consumer)
# 5. Verify order matching
# 6. Check token transfer and settlement
# =============================================================================

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
API_URL="${API_URL:-http://localhost:8080}"
DB_CONTAINER="${DB_CONTAINER:-gridtokenx-postgres}"
DB_USER="${DB_USER:-gridtokenx}"
DB_NAME="${DB_NAME:-gridtokenx}"

# Test users
SELLER_EMAIL="seller@test.com"
SELLER_PASSWORD="seller123456"
BUYER_EMAIL="buyer@test.com"
BUYER_PASSWORD="buyer123456"

echo -e "${BLUE}==============================================================================${NC}"
echo -e "${BLUE}  P2P Trading End-to-End Test${NC}"
echo -e "${BLUE}==============================================================================${NC}"
echo ""

# Helper functions
print_step() {
    echo -e "\n${YELLOW}▶ $1${NC}"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

print_info() {
    echo -e "${BLUE}ℹ $1${NC}"
}

db_query() {
    docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -t -c "$1" 2>/dev/null | tr -d ' '
}

# =============================================================================
# STEP 1: Setup Test Users
# =============================================================================
print_step "Step 1: Setting up test users"

# Check if seller exists
SELLER_ID=$(db_query "SELECT id FROM users WHERE email='${SELLER_EMAIL}' LIMIT 1;")
if [ -z "$SELLER_ID" ]; then
    print_info "Creating seller user..."
    SELLER_RESPONSE=$(curl -s -X POST "${API_URL}/api/auth/register-with-wallet" \
        -H "Content-Type: application/json" \
        -d '{
            "username": "seller_test",
            "email": "'${SELLER_EMAIL}'",
            "password": "'${SELLER_PASSWORD}'",
            "role": "prosumer",
            "first_name": "Seller",
            "last_name": "Test",
            "create_wallet": true,
            "airdrop_amount": 2.0
        }')
    SELLER_ID=$(db_query "SELECT id FROM users WHERE email='${SELLER_EMAIL}' LIMIT 1;")
    print_success "Seller created: ${SELLER_ID}"
else
    print_info "Seller already exists: ${SELLER_ID}"
fi

# Check if buyer exists
BUYER_ID=$(db_query "SELECT id FROM users WHERE email='${BUYER_EMAIL}' LIMIT 1;")
if [ -z "$BUYER_ID" ]; then
    print_info "Creating buyer user..."
    BUYER_RESPONSE=$(curl -s -X POST "${API_URL}/api/auth/register-with-wallet" \
        -H "Content-Type: application/json" \
        -d '{
            "username": "buyer_test",
            "email": "'${BUYER_EMAIL}'",
            "password": "'${BUYER_PASSWORD}'",
            "role": "consumer",
            "first_name": "Buyer",
            "last_name": "Test",
            "create_wallet": true,
            "airdrop_amount": 2.0
        }')
    BUYER_ID=$(db_query "SELECT id FROM users WHERE email='${BUYER_EMAIL}' LIMIT 1;")
    print_success "Buyer created: ${BUYER_ID}"
else
    print_info "Buyer already exists: ${BUYER_ID}"
fi

# Get wallet addresses
SELLER_WALLET=$(db_query "SELECT wallet_address FROM users WHERE id='${SELLER_ID}';")
BUYER_WALLET=$(db_query "SELECT wallet_address FROM users WHERE id='${BUYER_ID}';")

print_success "Seller wallet: ${SELLER_WALLET}"
print_success "Buyer wallet: ${BUYER_WALLET}"

# =============================================================================
# STEP 2: Login and get tokens
# =============================================================================
print_step "Step 2: Logging in users"

# Login seller
SELLER_LOGIN=$(curl -s -X POST "${API_URL}/api/auth/login-with-wallet" \
    -H "Content-Type: application/json" \
    -d '{
        "username": "seller_test",
        "password": "'${SELLER_PASSWORD}'"
    }')
SELLER_TOKEN=$(echo $SELLER_LOGIN | jq -r '.access_token // empty')
if [ -z "$SELLER_TOKEN" ]; then
    # Try with email as username
    SELLER_LOGIN=$(curl -s -X POST "${API_URL}/api/auth/login" \
        -H "Content-Type: application/json" \
        -d '{
            "username": "'${SELLER_EMAIL}'",
            "password": "'${SELLER_PASSWORD}'"
        }')
    SELLER_TOKEN=$(echo $SELLER_LOGIN | jq -r '.access_token // empty')
fi

if [ -n "$SELLER_TOKEN" ] && [ "$SELLER_TOKEN" != "null" ]; then
    print_success "Seller logged in"
else
    print_error "Failed to login seller: $SELLER_LOGIN"
    exit 1
fi

# Login buyer
BUYER_LOGIN=$(curl -s -X POST "${API_URL}/api/auth/login-with-wallet" \
    -H "Content-Type: application/json" \
    -d '{
        "username": "buyer_test",
        "password": "'${BUYER_PASSWORD}'"
    }')
BUYER_TOKEN=$(echo $BUYER_LOGIN | jq -r '.access_token // empty')
if [ -z "$BUYER_TOKEN" ]; then
    BUYER_LOGIN=$(curl -s -X POST "${API_URL}/api/auth/login" \
        -H "Content-Type: application/json" \
        -d '{
            "username": "'${BUYER_EMAIL}'",
            "password": "'${BUYER_PASSWORD}'"
        }')
    BUYER_TOKEN=$(echo $BUYER_LOGIN | jq -r '.access_token // empty')
fi

if [ -n "$BUYER_TOKEN" ] && [ "$BUYER_TOKEN" != "null" ]; then
    print_success "Buyer logged in"
else
    print_error "Failed to login buyer: $BUYER_LOGIN"
    exit 1
fi

# =============================================================================
# STEP 3: Check/Fix Seller's Wallet and Mint Tokens
# =============================================================================
print_step "Step 3: Ensuring seller has tokens to sell"

# Check seller's token balance
print_info "Checking seller's token balance..."
BALANCE_RESPONSE=$(curl -s -X GET "${API_URL}/api/tokens/balance/${SELLER_WALLET}" \
    -H "Authorization: Bearer ${SELLER_TOKEN}")
SELLER_BALANCE=$(echo $BALANCE_RESPONSE | jq -r '.token_balance_raw // 0')
print_info "Current seller token balance: ${SELLER_BALANCE}"

# If balance is 0 or low, we need to mint tokens
if [ "$SELLER_BALANCE" -lt "10000000000" ]; then
    print_info "Seller needs tokens. Simulating meter reading to mint tokens..."
    
    # First, register a meter for the seller if not exists
    METER_ID="METER-SELLER-001"
    print_info "Checking/registering meter..."
    
    METER_RESPONSE=$(curl -s -X POST "${API_URL}/api/user/meters" \
        -H "Authorization: Bearer ${SELLER_TOKEN}" \
        -H "Content-Type: application/json" \
        -d '{
            "meter_id": "'${METER_ID}'",
            "meter_type": "solar",
            "location": "Test Location",
            "capacity_kw": 10.0
        }')
    print_info "Meter registration response: $(echo $METER_RESPONSE | jq -c '.')"
    
    # Submit a meter reading (this should trigger minting)
    print_info "Submitting meter reading (10 kWh surplus)..."
    READING_RESPONSE=$(curl -s -X POST "${API_URL}/api/meters/submit-reading" \
        -H "Authorization: Bearer ${SELLER_TOKEN}" \
        -H "Content-Type: application/json" \
        -d '{
            "meter_id": "'${METER_ID}'",
            "reading_value": 10.5,
            "reading_type": "solar_generation",
            "timestamp": "'$(date -u +%Y-%m-%dT%H:%M:%SZ)'"
        }')
    print_info "Reading submission response: $(echo $READING_RESPONSE | jq -c '.')"
    
    # Wait for minting to complete
    print_info "Waiting for token minting..."
    sleep 5
    
    # Check balance again
    BALANCE_RESPONSE=$(curl -s -X GET "${API_URL}/api/tokens/balance/${SELLER_WALLET}" \
        -H "Authorization: Bearer ${SELLER_TOKEN}")
    SELLER_BALANCE=$(echo $BALANCE_RESPONSE | jq -r '.token_balance_raw // 0')
    print_info "Seller token balance after mint attempt: ${SELLER_BALANCE}"
    
    if [ "$SELLER_BALANCE" -lt "1000000000" ]; then
        print_error "Token minting may have failed. Attempting direct admin mint..."
        
        # Try admin mint endpoint
        ADMIN_TOKEN="${SELLER_TOKEN}" # Use seller's token for now, should use admin
        MINT_RESPONSE=$(curl -s -X POST "${API_URL}/api/admin/tokens/mint" \
            -H "Authorization: Bearer ${ADMIN_TOKEN}" \
            -H "Content-Type: application/json" \
            -d '{
                "wallet_address": "'${SELLER_WALLET}'",
                "amount": 100
            }')
        print_info "Admin mint response: $(echo $MINT_RESPONSE | jq -c '.')"
        sleep 3
        
        # Final balance check
        BALANCE_RESPONSE=$(curl -s -X GET "${API_URL}/api/tokens/balance/${SELLER_WALLET}" \
            -H "Authorization: Bearer ${SELLER_TOKEN}")
        SELLER_BALANCE=$(echo $BALANCE_RESPONSE | jq -r '.token_balance_raw // 0')
        print_info "Final seller token balance: ${SELLER_BALANCE}"
    fi
fi

if [ "$SELLER_BALANCE" -gt "0" ]; then
    print_success "Seller has tokens: ${SELLER_BALANCE} raw ($(echo "scale=2; $SELLER_BALANCE / 1000000000" | bc) tokens)"
else
    print_error "Seller has no tokens - sell order will fail"
    print_info "Continuing anyway to test error handling..."
fi

# =============================================================================
# STEP 4: Create Sell Order (Prosumer)
# =============================================================================
print_step "Step 4: Creating sell order"

SELL_ORDER_RESPONSE=$(curl -s -X POST "${API_URL}/api/trading/orders" \
    -H "Authorization: Bearer ${SELLER_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{
        "order_type": "sell",
        "energy_amount": 5.0,
        "price_per_kwh": 0.15
    }')

SELL_ORDER_ID=$(echo $SELL_ORDER_RESPONSE | jq -r '.id // empty')
SELL_ORDER_ERROR=$(echo $SELL_ORDER_RESPONSE | jq -r '.error.message // empty')

if [ -n "$SELL_ORDER_ID" ] && [ "$SELL_ORDER_ID" != "null" ]; then
    print_success "Sell order created: ${SELL_ORDER_ID}"
    print_info "Sell order details: $(echo $SELL_ORDER_RESPONSE | jq -c '{id, order_side, energy_amount, price_per_kwh, status}')"
else
    print_error "Failed to create sell order: ${SELL_ORDER_ERROR}"
    print_info "Full response: $(echo $SELL_ORDER_RESPONSE | jq -c '.')"
fi

# =============================================================================
# STEP 5: Create Buy Order (Consumer)
# =============================================================================
print_step "Step 5: Creating buy order"

BUY_ORDER_RESPONSE=$(curl -s -X POST "${API_URL}/api/trading/orders" \
    -H "Authorization: Bearer ${BUYER_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{
        "order_type": "buy",
        "energy_amount": 5.0,
        "price_per_kwh": 0.15
    }')

BUY_ORDER_ID=$(echo $BUY_ORDER_RESPONSE | jq -r '.id // empty')
BUY_ORDER_ERROR=$(echo $BUY_ORDER_RESPONSE | jq -r '.error.message // empty')

if [ -n "$BUY_ORDER_ID" ] && [ "$BUY_ORDER_ID" != "null" ]; then
    print_success "Buy order created: ${BUY_ORDER_ID}"
    print_info "Buy order details: $(echo $BUY_ORDER_RESPONSE | jq -c '{id, order_side, energy_amount, price_per_kwh, status}')"
else
    print_error "Failed to create buy order: ${BUY_ORDER_ERROR}"
    print_info "Full response: $(echo $BUY_ORDER_RESPONSE | jq -c '.')"
fi

# =============================================================================
# STEP 6: Verify Orders in Database
# =============================================================================
print_step "Step 6: Verifying orders in database"

ORDER_COUNT=$(db_query "SELECT COUNT(*) FROM trading_orders WHERE status IN ('open', 'pending', 'active');")
print_info "Active orders in database: ${ORDER_COUNT}"

# Show recent orders
print_info "Recent orders:"
docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -c "
SELECT id, user_id, order_side, energy_amount, price_per_kwh, status, created_at 
FROM trading_orders 
ORDER BY created_at DESC 
LIMIT 5;
" 2>/dev/null

# =============================================================================
# STEP 7: Trigger Order Matching (if not automatic)
# =============================================================================
print_step "Step 7: Waiting for order matching"
print_info "Order matching runs automatically every 5 seconds..."
sleep 10

# Check for matches
MATCH_COUNT=$(db_query "SELECT COUNT(*) FROM order_matches;")
print_info "Total matches in database: ${MATCH_COUNT}"

# Show recent matches
print_info "Recent matches:"
docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -c "
SELECT id, buy_order_id, sell_order_id, matched_amount, match_price, settlement_status, created_at 
FROM order_matches 
ORDER BY created_at DESC 
LIMIT 5;
" 2>/dev/null

# =============================================================================
# STEP 8: Check Settlement Status
# =============================================================================
print_step "Step 8: Checking settlement status"

# Check pending settlements
PENDING_SETTLEMENTS=$(db_query "SELECT COUNT(*) FROM order_matches WHERE settlement_status = 'pending';")
CONFIRMED_SETTLEMENTS=$(db_query "SELECT COUNT(*) FROM order_matches WHERE settlement_status = 'confirmed';")
FAILED_SETTLEMENTS=$(db_query "SELECT COUNT(*) FROM order_matches WHERE settlement_status = 'failed';")

print_info "Pending settlements: ${PENDING_SETTLEMENTS}"
print_info "Confirmed settlements: ${CONFIRMED_SETTLEMENTS}"
print_info "Failed settlements: ${FAILED_SETTLEMENTS}"

# =============================================================================
# STEP 9: Final Token Balance Check
# =============================================================================
print_step "Step 9: Final token balance check"

# Seller balance
SELLER_FINAL_BALANCE=$(curl -s -X GET "${API_URL}/api/tokens/balance/${SELLER_WALLET}" \
    -H "Authorization: Bearer ${SELLER_TOKEN}" | jq -r '.token_balance_raw // 0')
print_info "Seller final balance: ${SELLER_FINAL_BALANCE} raw"

# Buyer balance
BUYER_FINAL_BALANCE=$(curl -s -X GET "${API_URL}/api/tokens/balance/${BUYER_WALLET}" \
    -H "Authorization: Bearer ${BUYER_TOKEN}" | jq -r '.token_balance_raw // 0')
print_info "Buyer final balance: ${BUYER_FINAL_BALANCE} raw"

# =============================================================================
# Summary
# =============================================================================
echo ""
echo -e "${BLUE}==============================================================================${NC}"
echo -e "${BLUE}  Test Summary${NC}"
echo -e "${BLUE}==============================================================================${NC}"
echo ""
echo "Test Users:"
echo "  Seller: ${SELLER_EMAIL} (${SELLER_WALLET})"
echo "  Buyer: ${BUYER_EMAIL} (${BUYER_WALLET})"
echo ""
echo "Orders:"
echo "  Sell Order: ${SELL_ORDER_ID:-FAILED}"
echo "  Buy Order: ${BUY_ORDER_ID:-FAILED}"
echo ""
echo "Balances:"
echo "  Seller: ${SELLER_FINAL_BALANCE} raw tokens"
echo "  Buyer: ${BUYER_FINAL_BALANCE} raw tokens"
echo ""
echo "Settlements:"
echo "  Pending: ${PENDING_SETTLEMENTS}"
echo "  Confirmed: ${CONFIRMED_SETTLEMENTS}"
echo "  Failed: ${FAILED_SETTLEMENTS}"
echo ""

if [ -n "$SELL_ORDER_ID" ] && [ -n "$BUY_ORDER_ID" ]; then
    print_success "P2P Trading test completed successfully!"
else
    print_error "P2P Trading test completed with issues - check logs above"
fi
