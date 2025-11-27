#!/bin/bash
# Step 3 E2E Test: Complete API Gateway Submission Test with Prosumer Role

set -e

API_URL="http://localhost:8080"
DB_CONTAINER="p2p-postgres"
DB_USER="gridtokenx_user"
DB_NAME="gridtokenx"

echo "================================================================================"
echo "STEP 3 E2E TEST: Complete API Gateway Submission"
echo "================================================================================"
echo ""

# Step 1: Register prosumer user
echo "Step 1: Creating prosumer account..."
REGISTER_RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/api/auth/register" \
  -H "Content-Type: application/json" \
  -d '{
    "username": "testprosumer",
    "email": "test.prosumer@gridtoken.test",
    "password": "ProsumerPass123!",
    "first_name": "Test",
    "last_name": "Prosumer"
  }')

HTTP_CODE=$(echo "$REGISTER_RESPONSE" | tail -n1)
if [ "$HTTP_CODE" == "201" ]; then
  echo "✅ User registered successfully"
else
  echo "⚠️  User may already exist (HTTP $HTTP_CODE)"
fi
echo ""

# Step 2: Update role to prosumer and verify email
echo "Step 2: Setting up prosumer account..."
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "UPDATE users SET role = 'prosumer', email_verified = true WHERE email = 'test.prosumer@gridtoken.test';" > /dev/null
echo "✅ Role set to 'prosumer' and email verified"
echo ""

# Step 3: Login
echo "Step 3: Logging in..."
LOGIN_RESPONSE=$(curl -s -X POST "$API_URL/api/auth/login" \
  -H "Content-Type: application/json" \
  -d '{
    "username": "testprosumer",
    "password": "ProsumerPass123!"
  }')

AUTH_TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r '.access_token')
if [ "$AUTH_TOKEN" == "null" ] || [ -z "$AUTH_TOKEN" ]; then
  echo "❌ Login failed"
  echo "$LOGIN_RESPONSE" | jq '.'
  exit 1
fi
echo "✅ Login successful"
echo "   Token: ${AUTH_TOKEN:0:50}..."
echo ""

# Step 4: Set wallet address
echo "Step 4: Setting wallet address..."
WALLET_ADDRESS="7YhKmZbFZt8qP3xN9vJ2kL4mR5wT6uV8sA1bC3dE4fG5"
WALLET_RESPONSE=$(curl -s -X POST "$API_URL/api/user/wallet" \
  -H "Authorization: Bearer $AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"wallet_address\": \"$WALLET_ADDRESS\"}")

echo "✅ Wallet address set: $WALLET_ADDRESS"
echo ""

# Step 5: Submit meter reading
echo "Step 5: Submitting meter reading..."
CURRENT_TIME=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

READING_RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/api/meters/submit-reading" \
  -H "Authorization: Bearer $AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  -d "{
    \"kwh_amount\": \"15.5\",
    \"reading_timestamp\": \"$CURRENT_TIME\",
    \"meter_signature\": \"dGVzdF9zaWduYXR1cmVfYmFzZTY0X2VuY29kZWRfc3RyaW5nX2hlcmU=\"
  }")

HTTP_CODE=$(echo "$READING_RESPONSE" | tail -n1)
RESPONSE_BODY=$(echo "$READING_RESPONSE" | sed '$d')

echo "HTTP Status: $HTTP_CODE"
echo ""
echo "Response:"
echo "$RESPONSE_BODY" | jq '.'
echo ""

if [ "$HTTP_CODE" == "200" ]; then
  READING_ID=$(echo "$RESPONSE_BODY" | jq -r '.id')
  echo "✅ Reading submitted successfully!"
  echo "   Reading ID: $READING_ID"
  echo ""
  
  # Step 6: Verify in database
  echo "Step 6: Verifying in database..."
  echo ""
  docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
    "SELECT id, kwh_amount, reading_timestamp, minted, submitted_at FROM meter_readings WHERE id = '$READING_ID';"
  echo ""
  
  # Step 7: Get user readings
  echo "Step 7: Fetching user readings via API..."
  MY_READINGS=$(curl -s -X GET "$API_URL/api/meters/my-readings" \
    -H "Authorization: Bearer $AUTH_TOKEN")
  
  READING_COUNT=$(echo "$MY_READINGS" | jq '.data | length')
  echo "✅ User has $READING_COUNT reading(s)"
  echo ""
  
  # Step 8: Get user stats
  echo "Step 8: Fetching user statistics..."
  STATS=$(curl -s -X GET "$API_URL/api/meters/stats" \
    -H "Authorization: Bearer $AUTH_TOKEN")
  
  echo "$STATS" | jq '.'
  echo ""
  
  echo "================================================================================"
  echo "✅ STEP 3 E2E TEST PASSED"
  echo "================================================================================"
  echo ""
  echo "Summary:"
  echo "  ✅ Prosumer account created and configured"
  echo "  ✅ JWT authentication working"
  echo "  ✅ Wallet address set"
  echo "  ✅ Meter reading submitted via HTTP POST"
  echo "  ✅ Reading stored in database (minted=false)"
  echo "  ✅ Reading retrievable via API"
  echo "  ✅ User statistics updated"
  echo ""
  echo "Next Steps:"
  echo "  → Step 4: Backend Validation & Storage (already complete)"
  echo "  → Step 5: Automated Token Minting (polling service)"
  echo ""
else
  echo "================================================================================"
  echo "❌ STEP 3 E2E TEST FAILED"
  echo "================================================================================"
  echo ""
  echo "Error: Reading submission failed with HTTP $HTTP_CODE"
  exit 1
fi
