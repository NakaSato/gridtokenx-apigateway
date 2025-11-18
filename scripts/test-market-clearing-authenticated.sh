#!/bin/bash

# GridTokenX API Gateway - Market Clearing Engine Full Integration Test
# This script tests both public and admin endpoints with authentication

set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
API_BASE_URL="${API_BASE_URL:-http://localhost:8080}"
ADMIN_EMAIL="${ADMIN_EMAIL:-admin@gridtokenx.com}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-Admin123!@#}"
SLEEP_TIME=2

# Helper functions
print_header() {
    echo -e "\n${BLUE}========================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}========================================${NC}\n"
}

print_result() {
    if [ $1 -eq 0 ]; then
        echo -e "${GREEN}✓ $2${NC}"
    else
        echo -e "${RED}✗ $2${NC}"
    fi
}

# Check if server is running
print_header "1. Health Check"
echo "Testing server availability at $API_BASE_URL..."
if ! curl -s "$API_BASE_URL/health" > /dev/null 2>&1; then
    echo -e "${RED}✗ Server is not running at $API_BASE_URL${NC}"
    echo "Please start the server first: cargo run"
    exit 1
fi
echo -e "${GREEN}✓ Server is running${NC}"

# Test 2: Admin Login
print_header "2. Admin Authentication"
echo "Logging in as admin..."
LOGIN_RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_BASE_URL/api/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$ADMIN_USERNAME\",\"password\":\"$ADMIN_PASSWORD\"}")

HTTP_CODE=$(echo "$LOGIN_RESPONSE" | tail -n 1)
BODY=$(echo "$LOGIN_RESPONSE" | sed '$d')

if [ "$HTTP_CODE" -eq 200 ]; then
    echo -e "${GREEN}✓ Admin login successful${NC}"
    TOKEN=$(echo "$BODY" | jq -r '.token')
    
    if [ "$TOKEN" == "null" ] || [ -z "$TOKEN" ]; then
        echo -e "${RED}✗ No token received${NC}"
        echo "$BODY"
        exit 1
    fi
    
    echo -e "${YELLOW}Token: ${TOKEN:0:20}...${NC}"
else
    echo -e "${RED}✗ Admin login failed (HTTP $HTTP_CODE)${NC}"
    echo "$BODY"
    echo -e "\n${YELLOW}Note: Make sure you have an admin user created${NC}"
    echo "You can create one using the registration endpoint or database migration"
    exit 1
fi

sleep $SLEEP_TIME

# Test 3: Get current epoch (admin)
print_header "3. Get Current Epoch (Admin)"
echo "GET /api/admin/epochs/current"
RESPONSE=$(curl -s -w "\n%{http_code}" "$API_BASE_URL/api/admin/epochs/current" \
    -H "Authorization: Bearer $TOKEN")
HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" -eq 200 ]; then
    echo -e "${GREEN}✓ Successfully retrieved current epoch${NC}"
    echo "$BODY" | jq '.'
    
    EPOCH_ID=$(echo "$BODY" | jq -r '.id')
    EPOCH_NUMBER=$(echo "$BODY" | jq -r '.epoch_number')
    EPOCH_STATUS=$(echo "$BODY" | jq -r '.status')
    
    echo -e "\n${YELLOW}Epoch ID: $EPOCH_ID${NC}"
    echo -e "${YELLOW}Epoch Number: $EPOCH_NUMBER${NC}"
    echo -e "${YELLOW}Status: $EPOCH_STATUS${NC}"
else
    echo -e "${RED}✗ Failed to retrieve current epoch (HTTP $HTTP_CODE)${NC}"
    echo "$BODY"
fi

sleep $SLEEP_TIME

# Test 4: Get epoch history
print_header "4. Get Epoch History (Admin)"
echo "GET /api/admin/epochs/history?page=1&limit=5"
RESPONSE=$(curl -s -w "\n%{http_code}" "$API_BASE_URL/api/admin/epochs/history?page=1&limit=5" \
    -H "Authorization: Bearer $TOKEN")
HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" -eq 200 ]; then
    echo -e "${GREEN}✓ Successfully retrieved epoch history${NC}"
    echo "$BODY" | jq '.'
    
    TOTAL_EPOCHS=$(echo "$BODY" | jq '.pagination.total')
    echo -e "\n${YELLOW}Total Epochs: $TOTAL_EPOCHS${NC}"
else
    echo -e "${RED}✗ Failed to retrieve epoch history (HTTP $HTTP_CODE)${NC}"
    echo "$BODY"
fi

sleep $SLEEP_TIME

# Test 5: Get specific epoch details
if [ ! -z "$EPOCH_ID" ] && [ "$EPOCH_ID" != "null" ]; then
    print_header "5. Get Specific Epoch Details (Admin)"
    echo "GET /api/admin/epochs/$EPOCH_ID"
    RESPONSE=$(curl -s -w "\n%{http_code}" "$API_BASE_URL/api/admin/epochs/$EPOCH_ID" \
        -H "Authorization: Bearer $TOKEN")
    HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
    BODY=$(echo "$RESPONSE" | sed '$d')
    
    if [ "$HTTP_CODE" -eq 200 ]; then
        echo -e "${GREEN}✓ Successfully retrieved epoch details${NC}"
        echo "$BODY" | jq '.'
    else
        echo -e "${RED}✗ Failed to retrieve epoch details (HTTP $HTTP_CODE)${NC}"
        echo "$BODY"
    fi
    
    sleep $SLEEP_TIME
fi

# Test 6: Get epoch statistics
if [ ! -z "$EPOCH_ID" ] && [ "$EPOCH_ID" != "null" ]; then
    print_header "6. Get Epoch Statistics (Admin)"
    echo "GET /api/admin/epochs/$EPOCH_ID/stats"
    RESPONSE=$(curl -s -w "\n%{http_code}" "$API_BASE_URL/api/admin/epochs/$EPOCH_ID/stats" \
        -H "Authorization: Bearer $TOKEN")
    HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
    BODY=$(echo "$RESPONSE" | sed '$d')
    
    if [ "$HTTP_CODE" -eq 200 ]; then
        echo -e "${GREEN}✓ Successfully retrieved epoch statistics${NC}"
        echo "$BODY" | jq '.'
        
        TOTAL_ORDERS=$(echo "$BODY" | jq -r '.total_orders // 0')
        MATCHED_ORDERS=$(echo "$BODY" | jq -r '.matched_orders // 0')
        TOTAL_VOLUME=$(echo "$BODY" | jq -r '.total_volume // "0"')
        
        echo -e "\n${YELLOW}Total Orders: $TOTAL_ORDERS${NC}"
        echo -e "${YELLOW}Matched Orders: $MATCHED_ORDERS${NC}"
        echo -e "${YELLOW}Total Volume: $TOTAL_VOLUME${NC}"
    else
        echo -e "${RED}✗ Failed to retrieve epoch statistics (HTTP $HTTP_CODE)${NC}"
        echo "$BODY"
    fi
    
    sleep $SLEEP_TIME
fi

# Test 7: Test public market endpoints
print_header "7. Public Market Endpoints"

echo "Testing GET /api/market/epoch..."
RESPONSE=$(curl -s -w "\n%{http_code}" "$API_BASE_URL/api/market/epoch")
HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
if [ "$HTTP_CODE" -eq 200 ]; then
    echo -e "${GREEN}✓ Market epoch endpoint working${NC}"
else
    echo -e "${RED}✗ Market epoch endpoint failed (HTTP $HTTP_CODE)${NC}"
fi

sleep 1

echo "Testing GET /api/market/epoch/status..."
RESPONSE=$(curl -s -w "\n%{http_code}" "$API_BASE_URL/api/market/epoch/status")
HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
if [ "$HTTP_CODE" -eq 200 ]; then
    echo -e "${GREEN}✓ Epoch status endpoint working${NC}"
else
    echo -e "${RED}✗ Epoch status endpoint failed (HTTP $HTTP_CODE)${NC}"
fi

sleep 1

echo "Testing GET /api/market/orderbook..."
RESPONSE=$(curl -s -w "\n%{http_code}" "$API_BASE_URL/api/market/orderbook")
HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
if [ "$HTTP_CODE" -eq 200 ]; then
    echo -e "${GREEN}✓ Order book endpoint working${NC}"
else
    echo -e "${RED}✗ Order book endpoint failed (HTTP $HTTP_CODE)${NC}"
fi

sleep 1

echo "Testing GET /api/market/stats..."
RESPONSE=$(curl -s -w "\n%{http_code}" "$API_BASE_URL/api/market/stats")
HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
if [ "$HTTP_CODE" -eq 200 ]; then
    echo -e "${GREEN}✓ Market stats endpoint working${NC}"
else
    echo -e "${RED}✗ Market stats endpoint failed (HTTP $HTTP_CODE)${NC}"
fi

# Test 8: Manual epoch trigger (if epoch is pending)
if [ "$EPOCH_STATUS" == "pending" ] || [ "$EPOCH_STATUS" == "active" ]; then
    print_header "8. Manual Epoch Trigger (Admin)"
    echo -e "${YELLOW}Warning: This will manually trigger epoch clearing${NC}"
    echo -e "${YELLOW}Current status: $EPOCH_STATUS${NC}"
    echo -e "${YELLOW}Skipping automatic trigger to avoid disruption${NC}"
    echo ""
    echo "To manually trigger epoch clearing, run:"
    echo "curl -X POST \"$API_BASE_URL/api/admin/epochs/$EPOCH_ID/trigger\" \\"
    echo "  -H \"Authorization: Bearer \$TOKEN\""
else
    print_header "8. Manual Epoch Trigger (Skipped)"
    echo -e "${YELLOW}Epoch is already in '$EPOCH_STATUS' state${NC}"
fi

# Summary
print_header "Test Summary"
echo -e "${GREEN}✓ Authentication Test: PASSED${NC}"
echo -e "${GREEN}✓ Admin Epoch Endpoints: TESTED${NC}"
echo -e "${GREEN}✓ Public Market Endpoints: TESTED${NC}"

echo -e "\n${BLUE}Key Findings:${NC}"
echo -e "- Current Epoch: ${YELLOW}$EPOCH_NUMBER${NC}"
echo -e "- Epoch Status: ${YELLOW}$EPOCH_STATUS${NC}"
if [ ! -z "$TOTAL_ORDERS" ]; then
    echo -e "- Total Orders: ${YELLOW}$TOTAL_ORDERS${NC}"
    echo -e "- Matched Orders: ${YELLOW}$MATCHED_ORDERS${NC}"
fi

echo -e "\n${BLUE}Next Steps:${NC}"
echo "1. Create test orders using POST /api/trading/orders"
echo "2. Wait for epoch transition or manually trigger"
echo "3. Verify order matching and settlements"
echo "4. Test WebSocket real-time updates"

echo -e "\n${BLUE}Testing complete!${NC}\n"
