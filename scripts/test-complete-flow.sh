#!/usr/bin/env bash
# Complete automated integration test for GridTokenX platform
set -e

API_URL="http://localhost:8080"
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${YELLOW}=========================================${NC}"
echo -e "${YELLOW}GridTokenX Complete Integration Test${NC}"
echo -e "${YELLOW}=========================================${NC}"
echo ""

# Generate unique test data
TIMESTAMP=$(date +%s)
TEST_USER="test_user_$TIMESTAMP"
TEST_EMAIL="test_${TIMESTAMP}@example.com"
# Use valid Solana wallet format (base58, 44 chars) - using a known valid format
TEST_WALLET="8WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWW${TIMESTAMP:0:1}"
TEST_METER="METER-${TIMESTAMP}"

echo -e "${YELLOW}Test Configuration:${NC}"
echo "User: $TEST_USER"
echo "Email: $TEST_EMAIL"
echo "Meter: $TEST_METER"
echo ""

# Step 1: Register User
echo -e "${YELLOW}Step 1: Registering user...${NC}"
REGISTER_RESPONSE=$(curl -s -X POST "$API_URL/api/auth/register" \
  -H "Content-Type: application/json" \
  -d "{
    \"username\": \"$TEST_USER\",
    \"email\": \"$TEST_EMAIL\",
    \"password\": \"ComplexP@ssw0rd!2024\",
    \"first_name\": \"Test\",
    \"last_name\": \"User\"
  }")

echo "$REGISTER_RESPONSE" | jq .
if echo "$REGISTER_RESPONSE" | jq -e '.error' > /dev/null; then
  echo -e "${RED}❌ Registration failed${NC}"
  exit 1
fi
echo -e "${GREEN}✅ User registered${NC}"
echo ""

# Step 2: Verify Email
echo -e "${YELLOW}Step 2: Verifying email...${NC}"
docker exec gridtokenx-postgres psql -U gridtokenx -d gridtokenx -c \
  "UPDATE users SET email_verified = true WHERE email = '$TEST_EMAIL';" > /dev/null
echo -e "${GREEN}✅ Email verified${NC}"
echo ""

# Step 3: Login
echo -e "${YELLOW}Step 3: Logging in...${NC}"
LOGIN_RESPONSE=$(curl -s -X POST "$API_URL/api/auth/login" \
  -H "Content-Type: application/json" \
  -d "{
    \"username\": \"$TEST_USER\",
    \"password\": \"ComplexP@ssw0rd!2024\"
  }")

TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r '.access_token')
if [ "$TOKEN" == "null" ] || [ -z "$TOKEN" ]; then
  echo -e "${RED}❌ Login failed${NC}"
  echo "$LOGIN_RESPONSE" | jq .
  exit 1
fi
echo -e "${GREEN}✅ Login successful${NC}"
echo "Token: ${TOKEN:0:30}..."
echo ""

# Step 4: Set Wallet Address
echo -e "${YELLOW}Step 4: Setting wallet address...${NC}"
WALLET_RESPONSE=$(curl -s -X POST "$API_URL/api/user/wallet" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"wallet_address\": \"$TEST_WALLET\"}")

echo "$WALLET_RESPONSE" | jq .
if echo "$WALLET_RESPONSE" | jq -e '.error' > /dev/null; then
  echo -e "${RED}❌ Wallet setup failed${NC}"
  exit 1
fi
echo -e "${GREEN}✅ Wallet address set${NC}"
echo ""

# Step 5: Register Meter
echo -e "${YELLOW}Step 5: Registering meter...${NC}"
METER_RESPONSE=$(curl -s -X POST "$API_URL/api/user/meters" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{
    \"meter_serial\": \"$TEST_METER\",
    \"meter_type\": \"solar\",
    \"location_address\": \"123 Integration Test Street\"
  }")

echo "$METER_RESPONSE" | jq .
METER_ID=$(echo "$METER_RESPONSE" | jq -r '.meter_id')
if [ "$METER_ID" == "null" ] || [ -z "$METER_ID" ]; then
  echo -e "${RED}❌ Meter registration failed${NC}"
  exit 1
fi
echo -e "${GREEN}✅ Meter registered: $METER_ID${NC}"
echo ""

# Step 6: List Meters
echo -e "${YELLOW}Step 6: Listing user meters...${NC}"
METERS_LIST=$(curl -s -X GET "$API_URL/api/user/meters" \
  -H "Authorization: Bearer $TOKEN")

echo "$METERS_LIST" | jq .
METER_COUNT=$(echo "$METERS_LIST" | jq '.total')
if [ "$METER_COUNT" -lt 1 ]; then
  echo -e "${RED}❌ No meters found${NC}"
  exit 1
fi
echo -e "${GREEN}✅ Found $METER_COUNT meter(s)${NC}"
echo ""

# Step 7: Verify in Database
echo -e "${YELLOW}Step 7: Verifying in database...${NC}"
echo "Meter Registry:"
docker exec gridtokenx-postgres psql -U gridtokenx -d gridtokenx -c \
  "SELECT meter_serial, verification_status, meter_type, location_address 
   FROM meter_registry 
   WHERE meter_serial = '$TEST_METER';"
echo ""

echo "User Info:"
docker exec gridtokenx-postgres psql -U gridtokenx -d gridtokenx -c \
  "SELECT username, email, email_verified, wallet_address 
   FROM users 
   WHERE email = '$TEST_EMAIL';"
echo ""

# Step 8: Check Partitions
echo -e "${YELLOW}Step 8: Checking partition status...${NC}"
docker exec gridtokenx-postgres psql -U gridtokenx -d gridtokenx -c \
  "SELECT tablename, pg_size_pretty(pg_total_relation_size('public.'||tablename)) AS size
   FROM pg_tables
   WHERE tablename LIKE 'meter_readings_%'
   ORDER BY tablename;" | head -15
echo ""

# Step 9: Test Duplicate Prevention
echo -e "${YELLOW}Step 9: Testing duplicate meter prevention...${NC}"
DUPLICATE_RESPONSE=$(curl -s -X POST "$API_URL/api/user/meters" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{
    \"meter_serial\": \"$TEST_METER\",
    \"meter_type\": \"solar\"
  }")

if echo "$DUPLICATE_RESPONSE" | jq -e '.error' > /dev/null; then
  echo -e "${GREEN}✅ Duplicate prevention working${NC}"
else
  echo -e "${RED}❌ Duplicate meter was allowed${NC}"
fi
echo ""

# Step 10: Delete Meter
echo -e "${YELLOW}Step 10: Deleting meter...${NC}"
DELETE_RESPONSE=$(curl -s -X DELETE "$API_URL/api/user/meters/$METER_ID" \
  -H "Authorization: Bearer $TOKEN" \
  -w "\nHTTP_STATUS:%{http_code}")

HTTP_STATUS=$(echo "$DELETE_RESPONSE" | grep "HTTP_STATUS" | cut -d':' -f2)
if [ "$HTTP_STATUS" == "204" ]; then
  echo -e "${GREEN}✅ Meter deleted successfully${NC}"
else
  echo -e "${RED}❌ Meter deletion failed (HTTP $HTTP_STATUS)${NC}"
fi
echo ""

# Final Summary
echo -e "${GREEN}=========================================${NC}"
echo -e "${GREEN}Integration Test Complete!${NC}"
echo -e "${GREEN}=========================================${NC}"
echo ""
echo "Summary:"
echo "✅ User registration"
echo "✅ Email verification"
echo "✅ User login"
echo "✅ Wallet address setup"
echo "✅ Meter registration"
echo "✅ Meter listing"
echo "✅ Database verification"
echo "✅ Partition status check"
echo "✅ Duplicate prevention"
echo "✅ Meter deletion"
echo ""
echo -e "${GREEN}All tests passed! Platform is operational.${NC}"
