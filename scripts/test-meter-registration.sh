#!/usr/bin/env bash
# Test script for meter registration feature
# This tests the simplified meter registration flow

set -e

API_URL="http://localhost:8080"
echo "Testing Meter Registration Feature"
echo "===================================="
echo ""

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Step 1: Register a new user
echo -e "${YELLOW}Step 1: Registering new user...${NC}"
REGISTER_RESPONSE=$(curl -s -X POST "$API_URL/api/auth/register" \
  -H "Content-Type: application/json" \
  -d '{
    "username": "meter_test_user",
    "email": "meter_test@example.com",
    "password": "SecurePassword123!",
    "first_name": "Test",
    "last_name": "User"
  }')

echo "$REGISTER_RESPONSE" | jq .
echo ""

# Step 2: Verify email (manual DB update required)
echo -e "${YELLOW}Step 2: Verifying email (manual step)...${NC}"
echo "Run this SQL command to verify the user:"
echo ""
echo "docker exec gridtokenx-postgres psql -U gridtokenx -d gridtokenx -c \\"
echo "  UPDATE users SET email_verified = true WHERE email = 'meter_test@example.com';\\"
echo ""
read -p "Press Enter after verifying email..."

# Step 3: Login
echo -e "${YELLOW}Step 3: Logging in...${NC}"
LOGIN_RESPONSE=$(curl -s -X POST "$API_URL/api/auth/login" \
  -H "Content-Type: application/json" \
  -d '{
    "username": "meter_test_user",
    "password": "SecurePassword123!"
  }')

TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r '.access_token')

if [ "$TOKEN" == "null" ] || [ -z "$TOKEN" ]; then
  echo -e "${RED}❌ Login failed${NC}"
  echo "$LOGIN_RESPONSE" | jq .
  exit 1
fi

echo -e "${GREEN}✅ Login successful${NC}"
echo "Token: ${TOKEN:0:20}..."
echo ""

# Step 4: Set wallet address
echo -e "${YELLOW}Step 4: Setting wallet address...${NC}"
WALLET_RESPONSE=$(curl -s -X POST "$API_URL/api/user/wallet" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "wallet_address": "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM"
  }')

echo "$WALLET_RESPONSE" | jq .
echo ""

# Step 5: Register a meter
echo -e "${YELLOW}Step 5: Registering meter...${NC}"
METER_RESPONSE=$(curl -s -X POST "$API_URL/api/user/meters" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "meter_serial": "TEST-METER-001",
    "meter_type": "solar",
    "location_address": "123 Test Street"
  }')

echo "$METER_RESPONSE" | jq .

METER_ID=$(echo "$METER_RESPONSE" | jq -r '.meter_id')

if [ "$METER_ID" == "null" ] || [ -z "$METER_ID" ]; then
  echo -e "${RED}❌ Meter registration failed${NC}"
  exit 1
fi

echo -e "${GREEN}✅ Meter registered successfully${NC}"
echo "Meter ID: $METER_ID"
echo ""

# Step 6: List user's meters
echo -e "${YELLOW}Step 6: Listing user's meters...${NC}"
METERS_LIST=$(curl -s -X GET "$API_URL/api/user/meters" \
  -H "Authorization: Bearer $TOKEN")

echo "$METERS_LIST" | jq .
echo ""

# Step 7: Try to register duplicate meter (should fail)
echo -e "${YELLOW}Step 7: Testing duplicate meter registration (should fail)...${NC}"
DUPLICATE_RESPONSE=$(curl -s -X POST "$API_URL/api/user/meters" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "meter_serial": "TEST-METER-001",
    "meter_type": "solar"
  }')

if echo "$DUPLICATE_RESPONSE" | jq -e '.error' > /dev/null; then
  echo -e "${GREEN}✅ Duplicate prevention working${NC}"
else
  echo -e "${RED}❌ Duplicate meter was allowed${NC}"
fi

echo "$DUPLICATE_RESPONSE" | jq .
echo ""

# Step 8: Verify in database
echo -e "${YELLOW}Step 8: Verifying in database...${NC}"
echo "Run this SQL command to verify:"
echo ""
echo "docker exec gridtokenx-postgres psql -U gridtokenx -d gridtokenx -c \\"
echo "  SELECT id, user_id, meter_serial, verification_status, meter_type FROM meter_registry WHERE meter_serial = 'TEST-METER-001';\\"
echo ""

# Step 9: Delete the meter
echo -e "${YELLOW}Step 9: Deleting meter...${NC}"
DELETE_RESPONSE=$(curl -s -X DELETE "$API_URL/api/user/meters/$METER_ID" \
  -H "Authorization: Bearer $TOKEN" \
  -w "\nHTTP_STATUS:%{http_code}")

HTTP_STATUS=$(echo "$DELETE_RESPONSE" | grep "HTTP_STATUS" | cut -d':' -f2)

if [ "$HTTP_STATUS" == "204" ]; then
  echo -e "${GREEN}✅ Meter deleted successfully${NC}"
else
  echo -e "${RED}❌ Meter deletion failed (HTTP $HTTP_STATUS)${NC}"
  echo "$DELETE_RESPONSE"
fi

echo ""

# Step 10: Verify deletion
echo -e "${YELLOW}Step 10: Verifying deletion...${NC}"
METERS_AFTER_DELETE=$(curl -s -X GET "$API_URL/api/user/meters" \
  -H "Authorization: Bearer $TOKEN")

METER_COUNT=$(echo "$METERS_AFTER_DELETE" | jq '.total')

if [ "$METER_COUNT" == "0" ]; then
  echo -e "${GREEN}✅ Meter successfully deleted${NC}"
else
  echo -e "${RED}❌ Meter still exists${NC}"
fi

echo "$METERS_AFTER_DELETE" | jq .
echo ""

echo -e "${GREEN}===================================="
echo "Test completed successfully!"
echo "====================================${NC}"
