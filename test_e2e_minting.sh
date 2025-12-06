#!/bin/bash
# Test End-to-End Meter Reading → Token Minting Flow

set -e

echo "🧪 Testing End-to-End Token Minting Flow"
echo "=========================================="
echo ""

# Configuration
API_URL="http://localhost:8080"
MINT_ADDRESS="7WPEWFhy7V1nW1eqcSCX6mtchnhmSzp2VKYypZiDTnYR"
RPC_URL="http://localhost:8899"

# Test user from database
USER_EMAIL="gap_test_1765007551@example.com"
USER_PASSWORD="testpassword123"  # You may need to update this
USER_WALLET="FUSUaU44p57WSanw5vM7X5RSZJKVhXowhC3pEbBg13BC"

echo "📝 Step 1: Login to get JWT token"
echo "-----------------------------------"
LOGIN_RESPONSE=$(curl -s -X POST "$API_URL/api/v1/auth/login" \
  -H "Content-Type: application/json" \
  -d "{\"email\":\"$USER_EMAIL\",\"password\":\"$USER_PASSWORD\"}")

TOKEN=$(echo $LOGIN_RESPONSE | jq -r '.token // empty')

if [ -z "$TOKEN" ]; then
  echo "❌ Login failed. Response:"
  echo "$LOGIN_RESPONSE" | jq '.'
  echo ""
  echo "💡 You may need to register a user first or update credentials in the script"
  exit 1
fi

echo "✅ Login successful"
echo "Token: ${TOKEN:0:20}..."
echo ""

echo "💰 Step 2: Check initial token balance"
echo "---------------------------------------"
INITIAL_BALANCE=$(spl-token balance $MINT_ADDRESS --owner $USER_WALLET --url $RPC_URL 2>/dev/null || echo "0")
echo "Initial balance: $INITIAL_BALANCE tokens"
echo ""

echo "📊 Step 3: Submit meter reading"
echo "--------------------------------"
READING_KWH=15.5
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

READING_RESPONSE=$(curl -s -X POST "$API_URL/api/v1/meters/submit-reading" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d "{
    \"meter_id\": \"TEST_METER_001\",
    \"energy_produced\": $READING_KWH,
    \"energy_consumed\": 0,
    \"timestamp\": \"$TIMESTAMP\"
  }")

READING_ID=$(echo $READING_RESPONSE | jq -r '.reading_id // .id // empty')

if [ -z "$READING_ID" ]; then
  echo "❌ Failed to submit reading. Response:"
  echo "$READING_RESPONSE" | jq '.'
  exit 1
fi

echo "✅ Reading submitted successfully"
echo "Reading ID: $READING_ID"
echo "Energy: $READING_KWH kWh"
echo "Expected tokens: $(echo "$READING_KWH * 1000000000" | bc) lamports"
echo ""

echo "⏳ Step 4: Wait for minting (polling interval: 60s)"
echo "----------------------------------------------------"
echo "Waiting for meter polling service to process..."

for i in {1..12}; do
  sleep 10
  
  # Check if reading is minted
  MINTED=$(docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -t -c \
    "SELECT minted FROM meter_readings WHERE id = '$READING_ID';" 2>/dev/null | tr -d ' ')
  
  if [ "$MINTED" = "t" ]; then
    echo "✅ Reading marked as minted!"
    break
  fi
  
  echo "⏱️  Waiting... ($((i*10))s elapsed)"
done

echo ""

echo "🔍 Step 5: Verify minting in database"
echo "--------------------------------------"
docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c \
  "SELECT id, energy_produced, minted, mint_tx_signature FROM meter_readings WHERE id = '$READING_ID';"

TX_SIG=$(docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -t -c \
  "SELECT mint_tx_signature FROM meter_readings WHERE id = '$READING_ID';" | tr -d ' ')

echo ""

if [ -n "$TX_SIG" ] && [ "$TX_SIG" != "" ]; then
  echo "✅ Transaction signature found: $TX_SIG"
  
  echo ""
  echo "🔗 Step 6: Confirm transaction on blockchain"
  echo "---------------------------------------------"
  solana confirm $TX_SIG --url $RPC_URL || echo "⚠️  Transaction not found (may have been pruned)"
  
  echo ""
  echo "💰 Step 7: Check final token balance"
  echo "-------------------------------------"
  FINAL_BALANCE=$(spl-token balance $MINT_ADDRESS --owner $USER_WALLET --url $RPC_URL 2>/dev/null || echo "0")
  echo "Final balance: $FINAL_BALANCE tokens"
  echo "Initial balance: $INITIAL_BALANCE tokens"
  echo "Difference: $(echo "$FINAL_BALANCE - $INITIAL_BALANCE" | bc) tokens"
  
  echo ""
  echo "✅ END-TO-END TEST COMPLETE!"
  echo "============================"
else
  echo "❌ No transaction signature found - minting may have failed"
  echo ""
  echo "📋 Check service logs:"
  echo "tail -50 gateway.log | grep -E 'ERROR|mint'"
fi
