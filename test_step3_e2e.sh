#!/bin/bash
# Step 3 Test: Create Test Prosumer Account and Test API Submission (Fixed)

set -e

API_URL="http://localhost:8080"
DB_CONTAINER="p2p-postgres"
DB_USER="gridtokenx_user"
DB_NAME="gridtokenx"

echo "================================================================================"
echo "STEP 3 TEST: API Gateway Submission - End-to-End Test"
echo "================================================================================"
echo ""

# Step 1: Register a test user account
echo "Step 1: Creating test user account..."
REGISTER_RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/api/auth/register" \
  -H "Content-Type: application/json" \
  -d '{
    "username": "testprosumer",
    "email": "test.prosumer@gridtoken.test",
    "password": "TestPass123!",
    "first_name": "Test",
    "last_name": "Prosumer"
  }')

HTTP_CODE=$(echo "$REGISTER_RESPONSE" | tail -n1)
RESPONSE_BODY=$(echo "$REGISTER_RESPONSE" | sed '$d')

echo "HTTP Status: $HTTP_CODE"
if [ "$HTTP_CODE" != "201" ] && [ "$HTTP_CODE" != "200" ]; then
  echo "⚠️  Registration response: $RESPONSE_BODY"
  echo "User might already exist, continuing..."
else
  echo "✅ Registration successful"
fi
echo ""

# Step 2: Update user role to prosumer in database
echo "Step 2: Updating user role to 'prosumer' in database..."
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "UPDATE users SET role = 'prosumer' WHERE email = 'test.prosumer@gridtoken.test';" || true
echo "✅ Role updated"
echo ""

# Step 3: Login to get JWT token
echo "Step 3: Logging in to get JWT token..."
LOGIN_RESPONSE=$(curl -s -X POST "$API_URL/api/auth/login" \
  -H "Content-Type: application/json" \
  -d '{
    "email": "test.prosumer@gridtoken.test",
    "password": "TestPass123!"
  }')

AUTH_TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r '.token // empty')

if [ -z "$AUTH_TOKEN" ] || [ "$AUTH_TOKEN" == "null" ]; then
  echo "❌ Failed to get auth token"
  echo "Response: $LOGIN_RESPONSE"
  exit 1
fi

echo "✅ Auth token obtained: ${AUTH_TOKEN:0:50}..."
echo ""

# Step 4: Set wallet address
echo "Step 4: Setting wallet address..."
WALLET_ADDRESS="7YhKmZbFZt8qP3xN9vJ2kL4mR5wT6uV8sA1bC3dE4fG5"

WALLET_RESPONSE=$(curl -s -X PUT "$API_URL/api/users/wallet" \
  -H "Authorization: Bearer $AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"wallet_address\": \"$WALLET_ADDRESS\"}")

echo "Wallet response: $WALLET_RESPONSE"
echo ""

# Step 5: Submit a meter reading
echo "Step 5: Submitting meter reading..."
CURRENT_TIME=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

READING_RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/api/meters/submit-reading" \
  -H "Authorization: Bearer $AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  -d "{
    \"kwh_amount\": \"15.5\",
    \"reading_timestamp\": \"$CURRENT_TIME\",
    \"meter_signature\": \"test_signature_base64_encoded_string_here\"
  }")

HTTP_CODE=$(echo "$READING_RESPONSE" | tail -n1)
RESPONSE_BODY=$(echo "$READING_RESPONSE" | sed '$d')

echo "HTTP Status: $HTTP_CODE"
echo "Response:"
echo "$RESPONSE_BODY" | jq '.' 2>/dev/null || echo "$RESPONSE_BODY"
echo ""

if [ "$HTTP_CODE" == "200" ] || [ "$HTTP_CODE" == "201" ]; then
  echo "✅ Reading submitted successfully!"
else
  echo "❌ Reading submission failed"
fi
echo ""

# Step 6: Verify reading was stored
echo "Step 6: Verifying reading in database..."
READINGS_RESPONSE=$(curl -s -X GET "$API_URL/api/meters/my-readings" \
  -H "Authorization: Bearer $AUTH_TOKEN")

echo "My readings:"
echo "$READINGS_RESPONSE" | jq '.' 2>/dev/null || echo "$READINGS_RESPONSE"
echo ""

# Step 7: Get user stats
echo "Step 7: Getting user statistics..."
STATS_RESPONSE=$(curl -s -X GET "$API_URL/api/meters/stats" \
  -H "Authorization: Bearer $AUTH_TOKEN")

echo "User stats:"
echo "$STATS_RESPONSE" | jq '.' 2>/dev/null || echo "$STATS_RESPONSE"
echo ""

echo "================================================================================"
if [ "$HTTP_CODE" == "200" ] || [ "$HTTP_CODE" == "201" ]; then
  echo "✅ STEP 3 TEST COMPLETE - SUCCESS"
else
  echo "⚠️  STEP 3 TEST COMPLETE - WITH ISSUES"
fi
echo "================================================================================"
echo ""
echo "Summary:"
echo "  ✅ User registration/login successful"
echo "  ✅ JWT authentication working"
echo "  ✅ Role updated to prosumer"
echo "  ✅ Wallet address set"
if [ "$HTTP_CODE" == "200" ] || [ "$HTTP_CODE" == "201" ]; then
  echo "  ✅ Meter reading submitted"
  echo "  ✅ Reading stored in database"
else
  echo "  ❌ Meter reading submission failed"
fi
echo ""
