#!/bin/bash

# Focused P2P/P2C Energy Trading Test
# Tests order creation, matching, and settlement for both P2P and P2C scenarios

set -e

API_URL="${API_URL:-http://localhost:8080}"
DB_CONTAINER="${DB_CONTAINER:-gridtokenx-postgres}"
DB_USER="${DB_USER:-gridtokenx_user}"
DB_NAME="${DB_NAME:-gridtokenx}"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
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

# Get engineering API key
ENGINEERING_API_KEY=$(grep "ENGINEERING_API_KEY" .env 2>/dev/null | cut -d'=' -f2 | tr -d '"' | tr -d ' ')

if [ -z "$ENGINEERING_API_KEY" ]; then
    print_error "No engineering API key found"
    exit 1
fi

print_header "P2P/P2C Energy Trading Test"

# Check existing data
print_info "Checking existing trading data..."
ORDER_COUNT=$(docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -t \
    -c "SELECT COUNT(*) FROM trading_orders;" | tr -d ' ')
MATCH_COUNT=$(docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -t \
    -c "SELECT COUNT(*) FROM order_matches;" | tr -d ' ')
SETTLEMENT_COUNT=$(docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -t \
    -c "SELECT COUNT(*) FROM settlements;" | tr -d ' ')

print_info "Current state:"
echo "  - Orders: $ORDER_COUNT"
echo "  - Matches: $MATCH_COUNT"
echo "  - Settlements: $SETTLEMENT_COUNT"

# Get or create test users
print_header "Setting Up Test Users"

# Get existing prosumer user
PROSUMER_ID=$(docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -t \
    -c "SELECT id FROM users WHERE role = 'user' LIMIT 1;" | tr -d ' ')

if [ -z "$PROSUMER_ID" ]; then
    print_error "No prosumer user found"
    exit 1
fi
print_success "Prosumer ID: $PROSUMER_ID"

# Get or create consumer user
CONSUMER_ID=$(docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -t \
    -c "SELECT id FROM users WHERE username LIKE 'consumer_%' LIMIT 1;" | tr -d ' ')

if [ -z "$CONSUMER_ID" ]; then
    print_info "Creating consumer user..."
    CONSUMER_EMAIL="consumer_test_$(date +%s)@example.com"
    CONSUMER_USERNAME="consumer_test_$(date +%s)"
    
    docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -c "
    INSERT INTO users (username, email, password_hash, email_verified, role, wallet_address, created_at, updated_at)
    VALUES (
        '${CONSUMER_USERNAME}',
        '${CONSUMER_EMAIL}',
        '\$2b\$12\$LQv3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/LewY5GyYzpLhJ2olu',
        true,
        'user',
        'Consumer$(openssl rand -hex 20 | cut -c1-36)',
        NOW(),
        NOW()
    )
    RETURNING id;" > /dev/null
    
    CONSUMER_ID=$(docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -t \
        -c "SELECT id FROM users WHERE email = '${CONSUMER_EMAIL}';" | tr -d ' ')
fi
print_success "Consumer ID: $CONSUMER_ID"

# Get or create corporate user
CORPORATE_ID=$(docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -t \
    -c "SELECT id FROM users WHERE role = 'corporate' LIMIT 1;" | tr -d ' ')

if [ -z "$CORPORATE_ID" ]; then
    print_info "Creating corporate user..."
    CORPORATE_EMAIL="corporate_test_$(date +%s)@example.com"
    CORPORATE_USERNAME="corporate_test_$(date +%s)"
    
    docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -c "
    INSERT INTO users (username, email, password_hash, email_verified, role, wallet_address, created_at, updated_at)
    VALUES (
        '${CORPORATE_USERNAME}',
        '${CORPORATE_EMAIL}',
        '\$2b\$12\$LQv3c1yqBWVHxkd0LHAkCOYz6TtxMQJqhN8/LewY5GyYzpLhJ2olu',
        true,
        'corporate',
        'Corporate$(openssl rand -hex 20 | cut -c1-34)',
        NOW(),
        NOW()
    )
    RETURNING id;" > /dev/null
    
    CORPORATE_ID=$(docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -t \
        -c "SELECT id FROM users WHERE email = '${CORPORATE_EMAIL}';" | tr -d ' ')
fi
print_success "Corporate ID: $CORPORATE_ID"

# Test P2P Trading
print_header "Test P2P Trading (Prosumer → Consumer)"

print_info "Creating P2P sell order (Prosumer: 10 kWh @ 0.18)..."
P2P_SELL_RESPONSE=$(curl -s -X POST "${API_URL}/api/trading/orders" \
    -H "X-API-Key: ${ENGINEERING_API_KEY}" \
    -H "X-Impersonate-User: ${PROSUMER_ID}" \
    -H "Content-Type: application/json" \
    -d '{
        "order_type": "sell",
        "energy_amount": 10.0,
        "price_per_kwh": 0.18
    }')

P2P_SELL_ID=$(echo "$P2P_SELL_RESPONSE" | jq -r '.id // empty')
if [ -n "$P2P_SELL_ID" ]; then
    print_success "P2P sell order created: $P2P_SELL_ID"
else
    print_info "Response: $P2P_SELL_RESPONSE"
fi

print_info "Creating P2P buy order (Consumer: 10 kWh @ 0.20)..."
P2P_BUY_RESPONSE=$(curl -s -X POST "${API_URL}/api/trading/orders" \
    -H "X-API-Key: ${ENGINEERING_API_KEY}" \
    -H "X-Impersonate-User: ${CONSUMER_ID}" \
    -H "Content-Type: application/json" \
    -d '{
        "order_type": "buy",
        "energy_amount": 10.0,
        "price_per_kwh": 0.20
    }')

P2P_BUY_ID=$(echo "$P2P_BUY_RESPONSE" | jq -r '.id // empty')
if [ -n "$P2P_BUY_ID" ]; then
    print_success "P2P buy order created: $P2P_BUY_ID"
else
    print_info "Response: $P2P_BUY_RESPONSE"
fi

# Test P2C Trading
print_header "Test P2C Trading (Prosumer → Corporate)"

print_info "Creating P2C sell order (Prosumer: 15 kWh @ 0.22)..."
P2C_SELL_RESPONSE=$(curl -s -X POST "${API_URL}/api/trading/orders" \
    -H "X-API-Key: ${ENGINEERING_API_KEY}" \
    -H "X-Impersonate-User: ${PROSUMER_ID}" \
    -H "Content-Type: application/json" \
    -d '{
        "order_type": "sell",
        "energy_amount": 15.0,
        "price_per_kwh": 0.22
    }')

P2C_SELL_ID=$(echo "$P2C_SELL_RESPONSE" | jq -r '.id // empty')
if [ -n "$P2C_SELL_ID" ]; then
    print_success "P2C sell order created: $P2C_SELL_ID"
else
    print_info "Response: $P2C_SELL_RESPONSE"
fi

print_info "Creating P2C buy order (Corporate: 15 kWh @ 0.25)..."
P2C_BUY_RESPONSE=$(curl -s -X POST "${API_URL}/api/trading/orders" \
    -H "X-API-Key: ${ENGINEERING_API_KEY}" \
    -H "X-Impersonate-User: ${CORPORATE_ID}" \
    -H "Content-Type: application/json" \
    -d '{
        "order_type": "buy",
        "energy_amount": 15.0,
        "price_per_kwh": 0.25
    }')

P2C_BUY_ID=$(echo "$P2C_BUY_RESPONSE" | jq -r '.id // empty')
if [ -n "$P2C_BUY_ID" ]; then
    print_success "P2C buy order created: $P2C_BUY_ID"
else
    print_info "Response: $P2C_BUY_RESPONSE"
fi

# Trigger order matching
print_header "Triggering Order Matching"

print_info "Calling admin match-orders endpoint..."
MATCH_RESPONSE=$(curl -s -X POST "${API_URL}/api/admin/trading/match-orders" \
    -H "X-API-Key: ${ENGINEERING_API_KEY}")

MATCHED_COUNT=$(echo "$MATCH_RESPONSE" | jq -r '.matched_orders // 0')
print_success "Matched $MATCHED_COUNT order pairs"

# Verify results
print_header "Verification Results"

sleep 2

NEW_ORDER_COUNT=$(docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -t \
    -c "SELECT COUNT(*) FROM trading_orders;" | tr -d ' ')
NEW_MATCH_COUNT=$(docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -t \
    -c "SELECT COUNT(*) FROM order_matches;" | tr -d ' ')
NEW_SETTLEMENT_COUNT=$(docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -t \
    -c "SELECT COUNT(*) FROM settlements;" | tr -d ' ')

echo -e "${CYAN}Database State:${NC}"
echo "  Orders: $ORDER_COUNT → $NEW_ORDER_COUNT (+$((NEW_ORDER_COUNT - ORDER_COUNT)))"
echo "  Matches: $MATCH_COUNT → $NEW_MATCH_COUNT (+$((NEW_MATCH_COUNT - MATCH_COUNT)))"
echo "  Settlements: $SETTLEMENT_COUNT → $NEW_SETTLEMENT_COUNT (+$((NEW_SETTLEMENT_COUNT - SETTLEMENT_COUNT)))"

# Show recent matches
print_info "Recent order matches:"
docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -c \
    "SELECT matched_amount, match_price, status, match_time 
     FROM order_matches 
     ORDER BY created_at DESC 
     LIMIT 5;"

# Show recent settlements
if [ "$NEW_SETTLEMENT_COUNT" -gt 0 ]; then
    print_info "Recent settlements:"
    docker exec ${DB_CONTAINER} psql -U ${DB_USER} -d ${DB_NAME} -c \
        "SELECT energy_amount, price_per_kwh, total_amount, status, created_at 
         FROM settlements 
         ORDER BY created_at DESC 
         LIMIT 5;"
fi

print_header "Test Complete"
print_success "P2P/P2C trading test finished successfully!"
