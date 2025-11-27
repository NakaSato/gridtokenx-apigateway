#!/bin/bash

# Configuration
API_URL="http://localhost:8080"
DB_CONTAINER="p2p-postgres"
DB_USER="gridtokenx_user"
DB_NAME="gridtokenx"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

echo "================================================================================"
echo "STEP 6 TEST: P2P Trading Logic"
echo "================================================================================"

# Check if API Gateway is running
if ! curl -s "$API_URL/health" > /dev/null; then
    echo -e "${RED}❌ API Gateway is not running!${NC}"
    echo "Please start the API Gateway with 'cargo run' first."
    exit 1
fi

echo -e "${GREEN}✅ API Gateway is running${NC}"

# Helper function to extract JSON field
get_json_field() {
    echo "$1" | grep -o "\"$2\":[^,}]*" | cut -d':' -f2 | tr -d '" '
}

# Helper function to extract token
get_token() {
    echo "$1" | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4
}

# ==============================================================================
# 1. Setup Accounts
# ==============================================================================
echo ""
echo "--------------------------------------------------------------------------------"
echo "1. Setting up Test Accounts"
echo "--------------------------------------------------------------------------------"

# Create Admin
echo "Creating Admin..."
ADMIN_EMAIL="admin_step6@test.com"
ADMIN_PASS="SecurePass123!"

# Check if user exists
USER_EXISTS=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c "SELECT COUNT(*) FROM users WHERE email = '$ADMIN_EMAIL';" | tr -d ' ')
if [ "$USER_EXISTS" -eq "0" ]; then
    REG_RES=$(curl -s -X POST "$API_URL/api/auth/register" \
      -H "Content-Type: application/json" \
      -d "{\"username\": \"admin_step6\", \"email\": \"$ADMIN_EMAIL\", \"password\": \"$ADMIN_PASS\", \"first_name\": \"Admin\", \"last_name\": \"User\"}")
    echo "Registration Response: $REG_RES"
else
    echo "Admin already exists."
fi

# Manually verify email and set role to admin
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c "UPDATE users SET email_verified = true, role = 'admin' WHERE email = '$ADMIN_EMAIL';" > /dev/null

# Verify user exists and has correct role
ADMIN_ROLE=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c "SELECT role FROM users WHERE email = '$ADMIN_EMAIL';" | tr -d ' ')
echo "Admin Role in DB: $ADMIN_ROLE"

# Login Admin
ADMIN_RES=$(curl -s -X POST "$API_URL/api/auth/login" \
  -H "Content-Type: application/json" \
  -d "{\"username\": \"admin_step6\", \"password\": \"$ADMIN_PASS\"}")

echo "Admin Login Response: $ADMIN_RES"

ADMIN_TOKEN=$(get_token "$ADMIN_RES")

if [ -z "$ADMIN_TOKEN" ]; then
    echo -e "${RED}❌ Failed to login as Admin${NC}"
    exit 1
fi
echo -e "${GREEN}✅ Admin logged in${NC}"

# Create Seller (Prosumer)
echo "Creating Seller..."
SELLER_EMAIL="seller_step6@test.com"
SELLER_PASS="SecurePass123!"

# Check if user exists
USER_EXISTS=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c "SELECT COUNT(*) FROM users WHERE email = '$SELLER_EMAIL';" | tr -d ' ')
if [ "$USER_EXISTS" -eq "0" ]; then
    curl -s -X POST "$API_URL/api/auth/register" \
      -H "Content-Type: application/json" \
      -d "{\"username\": \"seller_step6\", \"email\": \"$SELLER_EMAIL\", \"password\": \"$SELLER_PASS\", \"first_name\": \"Seller\", \"last_name\": \"User\"}" > /dev/null
    echo "Seller registered."
else
    echo "Seller already exists."
fi

# Clear trading tables to ensure clean state
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c "TRUNCATE TABLE settlements, order_matches, trading_orders CASCADE;" > /dev/null
docker exec p2p-redis redis-cli FLUSHALL > /dev/null

# Manually verify email and set role to prosumer
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c "ALTER TABLE trading_orders ADD COLUMN IF NOT EXISTS filled_at TIMESTAMPTZ;" > /dev/null
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c "UPDATE users SET email_verified = true, role = 'prosumer' WHERE email = '$SELLER_EMAIL';" > /dev/null

# Login Seller
SELLER_RES=$(curl -s -X POST "$API_URL/api/auth/login" \
  -H "Content-Type: application/json" \
  -d "{\"username\": \"seller_step6\", \"password\": \"$SELLER_PASS\"}")
SELLER_TOKEN=$(get_token "$SELLER_RES")

if [ -z "$SELLER_TOKEN" ]; then
    echo -e "${RED}❌ Failed to login as Seller${NC}"
    exit 1
fi
echo -e "${GREEN}✅ Seller logged in${NC}"

# Create Buyer (Consumer)
echo "Creating Buyer..."
BUYER_EMAIL="buyer_step6@test.com"
BUYER_PASS="SecurePass123!"

# Check if user exists
USER_EXISTS=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c "SELECT COUNT(*) FROM users WHERE email = '$BUYER_EMAIL';" | tr -d ' ')
if [ "$USER_EXISTS" -eq "0" ]; then
    curl -s -X POST "$API_URL/api/auth/register" \
      -H "Content-Type: application/json" \
      -d "{\"username\": \"buyer_step6\", \"email\": \"$BUYER_EMAIL\", \"password\": \"$BUYER_PASS\", \"first_name\": \"Buyer\", \"last_name\": \"User\"}" > /dev/null
    echo "Buyer registered."
else
    echo "Buyer already exists."
fi

# Manually verify email and set role to consumer
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c "UPDATE users SET email_verified = true, role = 'consumer' WHERE email = '$BUYER_EMAIL';" > /dev/null

# Login Buyer
BUYER_RES=$(curl -s -X POST "$API_URL/api/auth/login" \
  -H "Content-Type: application/json" \
  -d "{\"username\": \"buyer_step6\", \"password\": \"$BUYER_PASS\"}")
BUYER_TOKEN=$(get_token "$BUYER_RES")

if [ -z "$BUYER_TOKEN" ]; then
    echo -e "${RED}❌ Failed to login as Buyer${NC}"
    exit 1
fi
echo -e "${GREEN}✅ Buyer logged in${NC}"


# ==============================================================================
# 2. Place Orders
# ==============================================================================
echo ""
echo "--------------------------------------------------------------------------------"
echo "2. Placing Orders"
echo "--------------------------------------------------------------------------------"

# Place Sell Order (10 kWh @ $0.15)
echo "Placing Sell Order..."
SELL_RES=$(curl -s -X POST "$API_URL/api/trading/orders" \
  -H "Authorization: Bearer $SELLER_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "order_type": "sell",
    "energy_amount": 10.0,
    "price_per_kwh": 0.15
  }')

SELL_ORDER_ID=$(get_json_field "$SELL_RES" "id")
if [ -z "$SELL_ORDER_ID" ]; then
    echo -e "${RED}❌ Failed to place sell order${NC}"
    echo "Response: $SELL_RES"
    exit 1
fi
echo -e "${GREEN}✅ Sell Order Placed: $SELL_ORDER_ID${NC}"

# Place Buy Order (10 kWh @ $0.15)
echo "Placing Buy Order..."
BUY_RES=$(curl -s -X POST "$API_URL/api/trading/orders" \
  -H "Authorization: Bearer $BUYER_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "order_type": "buy",
    "energy_amount": 10.0,
    "price_per_kwh": 0.15
  }')

BUY_ORDER_ID=$(get_json_field "$BUY_RES" "id")
if [ -z "$BUY_ORDER_ID" ]; then
    echo -e "${RED}❌ Failed to place buy order${NC}"
    echo "Response: $BUY_RES"
    exit 1
fi
echo -e "${GREEN}✅ Buy Order Placed: $BUY_ORDER_ID${NC}"


# ==============================================================================
# 3. Trigger Matching
# ==============================================================================
echo ""
echo "--------------------------------------------------------------------------------"
echo "3. Triggering Matching Engine"
echo "--------------------------------------------------------------------------------"

MATCH_RES=$(curl -s -X POST "$API_URL/api/admin/trading/match-orders" \
  -H "Authorization: Bearer $ADMIN_TOKEN")

MATCHED_COUNT=$(get_json_field "$MATCH_RES" "matched_orders")

echo "Match Response: $MATCH_RES"

if [ "$MATCHED_COUNT" != "1" ]; then
    echo -e "${RED}❌ Expected 1 match, got $MATCHED_COUNT${NC}"
    # Check order status directly from DB to debug
    echo "Debugging: Checking order statuses in DB..."
    docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c "SELECT id, side::text, status, filled_amount, epoch_id, expires_at FROM trading_orders WHERE id IN ('$SELL_ORDER_ID', '$BUY_ORDER_ID');"
    exit 1
fi

echo -e "${GREEN}✅ Matching Successful: 1 match created${NC}"


# ==============================================================================
# 4. Verify Results
# ==============================================================================
echo ""
echo "--------------------------------------------------------------------------------"
echo "4. Verifying Results"
echo "--------------------------------------------------------------------------------"

# Check Order Status (Should be Filled)
echo "Checking Sell Order Status..."
SELL_STATUS=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c "SELECT status FROM trading_orders WHERE id = '$SELL_ORDER_ID';" | tr -d ' ')
if [ "$SELL_STATUS" != "filled" ]; then
    echo -e "${RED}❌ Sell order status is $SELL_STATUS (expected filled)${NC}"
    exit 1
fi
echo -e "${GREEN}✅ Sell Order Filled${NC}"

echo "Checking Buy Order Status..."
BUY_STATUS=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c "SELECT status FROM trading_orders WHERE id = '$BUY_ORDER_ID';" | tr -d ' ')
if [ "$BUY_STATUS" != "filled" ]; then
    echo -e "${RED}❌ Buy order status is $BUY_STATUS (expected filled)${NC}"
    exit 1
fi
echo -e "${GREEN}✅ Buy Order Filled${NC}"

# Check Settlement Creation
echo "Checking Settlement..."
SETTLEMENT_COUNT=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c "SELECT COUNT(*) FROM settlements WHERE buyer_id = (SELECT id FROM users WHERE email = '$BUYER_EMAIL') AND seller_id = (SELECT id FROM users WHERE email = '$SELLER_EMAIL');" | tr -d ' ')

if [ "$SETTLEMENT_COUNT" -lt 1 ]; then
    echo -e "${RED}❌ No settlement found${NC}"
    exit 1
fi
echo -e "${GREEN}✅ Settlement Created${NC}"

echo ""
echo "================================================================================"
echo -e "${GREEN}STEP 6 TEST PASSED SUCCESSFULLY!${NC}"
echo "================================================================================"
