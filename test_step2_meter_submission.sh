#!/bin/bash

# Test Script for Step 2: Meter Reading Submission and Automated Minting
# This script tests the end-to-end flow of meter reading submission and token minting

set -e

# Configuration
API_URL="http://localhost:8080"
DB_CONTAINER="p2p-postgres"
DB_USER="gridtokenx_user"
DB_NAME="gridtokenx"

# Test user credentials
TEST_EMAIL="prosumer_meter@test.com"
TEST_USERNAME="prosumer_meter"
TEST_PASSWORD="SecurePass123!"
TEST_WALLET="5Xj7hWqKqV9YGJ8r3nPqM8K4dYwZxNfR2tBpLmCvHgE3"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "================================================================================"
echo "STEP 2 TEST: Meter Reading Submission and Automated Minting"
echo "================================================================================"

# Check if API Gateway is running
if ! curl -s "$API_URL/health" > /dev/null; then
    echo -e "${RED}❌ API Gateway is not running on $API_URL${NC}"
    echo "Please start the API Gateway first: cargo run --bin api-gateway"
    exit 1
fi
echo -e "${GREEN}✅ API Gateway is running${NC}"
echo ""

echo "--------------------------------------------------------------------------------"
echo "1. Setting up Test User"
echo "--------------------------------------------------------------------------------"

# Check if user exists
USER_EXISTS=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c "SELECT COUNT(*) FROM users WHERE email = '$TEST_EMAIL';" | tr -d ' ')

if [ "$USER_EXISTS" -eq "0" ]; then
    echo "Creating test user..."
    REGISTER_RESPONSE=$(curl -s -X POST "$API_URL/api/auth/register" \
        -H "Content-Type: application/json" \
        -d "{
            \"email\": \"$TEST_EMAIL\",
            \"username\": \"$TEST_USERNAME\",
            \"password\": \"$TEST_PASSWORD\",
            \"first_name\": \"Test\",
            \"last_name\": \"Prosumer\"
        }")
    echo "Registration Response: $REGISTER_RESPONSE"
else
    echo "User already exists."
fi

# Manually verify email and set role to prosumer
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c "UPDATE users SET email_verified = true, role = 'prosumer', wallet_address = '$TEST_WALLET' WHERE email = '$TEST_EMAIL';" > /dev/null
echo -e "${GREEN}✅ User configured as prosumer with wallet${NC}"

# Login
LOGIN_RESPONSE=$(curl -s -X POST "$API_URL/api/auth/login" \
    -H "Content-Type: application/json" \
    -d "{
        \"username\": \"$TEST_USERNAME\",
        \"password\": \"$TEST_PASSWORD\"
    }")

TOKEN=$(echo $LOGIN_RESPONSE | jq -r '.access_token')
if [ "$TOKEN" == "null" ] || [ -z "$TOKEN" ]; then
    echo -e "${RED}❌ Failed to login${NC}"
    echo "Response: $LOGIN_RESPONSE"
    exit 1
fi
echo -e "${GREEN}✅ User logged in${NC}"

# Clear old meter readings for this user to avoid duplicate detection
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c "DELETE FROM meter_readings WHERE wallet_address = '$TEST_WALLET';" > /dev/null
echo "Cleared old meter readings for test user"
echo ""

echo "--------------------------------------------------------------------------------"
echo "2. Submitting Meter Reading (Legacy - No meter_id)"
echo "--------------------------------------------------------------------------------"

# Submit a meter reading without meter_id (legacy support)
READING_RESPONSE=$(curl -s -X POST "$API_URL/api/meters/submit-reading" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d "{
        \"kwh_amount\": \"25.5\",
        \"reading_timestamp\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",
        \"meter_signature\": \"mock_signature_legacy_test\"
    }")

READING_ID=$(echo $READING_RESPONSE | jq -r '.id')
if [ "$READING_ID" == "null" ] || [ -z "$READING_ID" ]; then
    echo -e "${RED}❌ Failed to submit reading${NC}"
    echo "Response: $READING_RESPONSE"
    exit 1
fi
echo -e "${GREEN}✅ Reading submitted: $READING_ID${NC}"

# Check verification status in database
VERIFICATION_STATUS=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c "SELECT verification_status FROM meter_readings WHERE id = '$READING_ID';" | tr -d ' ')
echo "Verification Status: $VERIFICATION_STATUS"

if [ "$VERIFICATION_STATUS" != "legacy_unverified" ]; then
    echo -e "${YELLOW}⚠️  Expected 'legacy_unverified', got '$VERIFICATION_STATUS'${NC}"
fi
echo ""

echo "--------------------------------------------------------------------------------"
echo "3. Waiting for Automated Minting (60 seconds)"
echo "--------------------------------------------------------------------------------"
echo "The meter polling service runs every 60 seconds..."
sleep 65

echo "--------------------------------------------------------------------------------"
echo "4. Verifying Minting Results"
echo "--------------------------------------------------------------------------------"

# Check if reading was minted
MINTED_STATUS=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c "SELECT minted FROM meter_readings WHERE id = '$READING_ID';" | tr -d ' ')

if [ "$MINTED_STATUS" == "t" ]; then
    echo -e "${GREEN}✅ Reading was minted successfully${NC}"
else
    echo -e "${RED}❌ Reading was not minted (minted = $MINTED_STATUS)${NC}"
    echo "Checking for errors..."
    docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c "SELECT id, kwh_amount, minted, verification_status, submitted_at FROM meter_readings WHERE id = '$READING_ID';"
    exit 1
fi

# Get minting details
echo "Minting Details:"
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c "SELECT id, kwh_amount, minted, mint_tx_signature, verification_status FROM meter_readings WHERE id = '$READING_ID';"

echo ""
echo "================================================================================"
echo -e "${GREEN}STEP 2 TEST PASSED SUCCESSFULLY!${NC}"
echo "================================================================================"
echo ""
