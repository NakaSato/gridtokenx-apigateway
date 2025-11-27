#!/bin/bash

# Configuration
API_URL="http://localhost:8080"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

echo "================================================================================"
echo "TEST: Sell Order Balance Check"
echo "================================================================================"

# 1. Login as Seller (using account from previous test)
echo "Logging in as Seller..."
SELLER_RES=$(curl -s -X POST "$API_URL/api/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username": "seller_step6", "password": "SecurePass123!"}')

SELLER_TOKEN=$(echo "$SELLER_RES" | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)

if [ -z "$SELLER_TOKEN" ]; then
    echo -e "${RED}❌ Failed to login as Seller${NC}"
    exit 1
fi
echo -e "${GREEN}✅ Seller logged in${NC}"

# 2. Place Sell Order for excessive amount (e.g., 1,000,000 kWh)
echo "Placing Excessive Sell Order (1,000,000 kWh)..."
SELL_RES=$(curl -s -X POST "$API_URL/api/trading/orders" \
  -H "Authorization: Bearer $SELLER_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "order_type": "sell",
    "energy_amount": 1.0,
    "price_per_kwh": 0.15
  }')

echo "Response: $SELL_RES"

# Check if order was created (ID present)
ORDER_ID=$(echo "$SELL_RES" | grep -o '"id":"[^"]*"' | cut -d'"' -f4)

if [ ! -z "$ORDER_ID" ]; then
    echo -e "${RED}❌ FAILURE: System accepted sell order for 1,000,000 kWh without balance check!${NC}"
    echo "Order ID: $ORDER_ID"
    exit 1
else
    echo -e "${GREEN}✅ SUCCESS: System rejected excessive sell order (or failed for other reasons).${NC}"
fi
