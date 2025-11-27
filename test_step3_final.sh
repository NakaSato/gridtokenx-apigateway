#!/bin/bash
# Step 3 Test: Final Working Test

set -e

API_URL="http://localhost:8080"
DB_CONTAINER="p2p-postgres"
DB_USER="gridtokenx_user"
DB_NAME="gridtokenx"

echo "================================================================================"
echo "STEP 3 TEST: API Gateway Submission - Final Test"
echo "================================================================================"
echo ""

# Step 1: Register admin user
echo "Step 1: Creating admin account..."
curl -s -X POST "$API_URL/api/auth/register" \
  -H "Content-Type: application/json" \
  -d '{
    "username": "testadmin",
    "email": "test.admin@gridtoken.test",
    "password": "AdminPass123!",
    "first_name": "Test",
    "last_name": "Admin"
  }' > /dev/null 2>&1 || true

docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "UPDATE users SET role = 'admin' WHERE email = 'test.admin@gridtoken.test';" > /dev/null 2>&1 || true
echo "✅ Admin account ready"
echo ""

# Step 2: Login with username
echo "Step 2: Logging in..."
LOGIN_RESPONSE=$(curl -s -X POST "$API_URL/api/auth/login" \
  -H "Content-Type: application/json" \
  -d '{
    "username": "testadmin",
    "password": "AdminPass123!"
  }')

AUTH_TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r '.token')
if [ "$AUTH_TOKEN" == "null" ] || [ -z "$AUTH_TOKEN" ]; then
  echo "❌ Login failed: $LOGIN_RESPONSE"
  exit 1
fi
echo "✅ Token obtained"
echo ""

# Step 3: Set wallet
echo "Step 3: Setting wallet..."
WALLET_ADDRESS="7YhKmZbFZt8qP3xN9vJ2kL4mR5wT6uV8sA1bC3dE4fG5"
curl -s -X PUT "$API_URL/api/users/wallet" \
  -H "Authorization: Bearer $AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"wallet_address\": \"$WALLET_ADDRESS\"}" > /dev/null
echo "✅ Wallet set"
echo ""

# Step 4: Submit reading
echo "Step 4: Submitting meter reading..."
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
echo "$RESPONSE_BODY" | jq '.'
echo ""

if [ "$HTTP_CODE" == "200" ]; then
  READING_ID=$(echo "$RESPONSE_BODY" | jq -r '.id')
  echo "✅ SUCCESS! Reading ID: $READING_ID"
  echo ""
  echo "Verifying in database..."
  docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
    "SELECT id, kwh_amount, reading_timestamp, minted, submitted_at FROM meter_readings WHERE id = '$READING_ID';"
  echo ""
  echo "================================================================================"
  echo "✅ STEP 3 TEST PASSED"
  echo "================================================================================"
else
  echo "❌ FAILED"
fi
