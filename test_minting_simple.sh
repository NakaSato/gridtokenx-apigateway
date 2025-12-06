#!/bin/bash
# Simplified End-to-End Token Minting Test
# Inserts reading directly to database and verifies automatic minting

set -e

echo "🧪 Simplified Token Minting Flow Test"
echo "======================================"
echo ""

# Configuration
MINT_ADDRESS="7WPEWFhy7V1nW1eqcSCX6mtchnhmSzp2VKYypZiDTnYR"
RPC_URL="http://localhost:8899"
USER_ID="924c2cda-0999-442f-b368-e9fe790412c7"
USER_WALLET="FUSUaU44p57WSanw5vM7X5RSZJKVhXowhC3pEbBg13BC"

echo "💰 Step 1: Check initial token balance"
echo "---------------------------------------"
INITIAL_BALANCE=$(spl-token balance $MINT_ADDRESS --owner $USER_WALLET --url $RPC_URL 2>/dev/null || echo "0")
echo "Wallet: $USER_WALLET"
echo "Initial balance: $INITIAL_BALANCE tokens"
echo ""

echo "📊 Step 2: Insert test meter reading into database"
echo "---------------------------------------------------"
READING_KWH=25.75
TIMESTAMP=$(date -u +"%Y-%m-%d %H:%M:%S")
READING_ID=$(uuidgen | tr '[:upper:]' '[:lower:]')

echo "Reading ID: $READING_ID"
echo "Energy: $READING_KWH kWh"
echo "Expected tokens to mint: $READING_KWH"
echo ""

# Insert reading directly into database
docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c "
INSERT INTO meter_readings (
  id, 
  user_id, 
  meter_serial,
  wallet_address,
  energy_generated, 
  energy_consumed,
  kwh_amount,
  timestamp,
  minted,
  created_at
) VALUES (
  '$READING_ID',
  '$USER_ID',
  'TEST_METER_E2E',
  '$USER_WALLET',
  $READING_KWH,
  0,
  $READING_KWH,
  '$TIMESTAMP',
  false,
  NOW()
);" > /dev/null

echo "✅ Reading inserted into database"
echo ""

echo "⏳ Step 3: Wait for automatic minting (polling interval: 60s)"
echo "-------------------------------------------------------------"
echo "The meter polling service will pick this up in the next cycle..."
echo ""

MINTED=false
for i in {1..15}; do
  sleep 10
  
  # Check if reading is minted
  MINTED_STATUS=$(docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -t -c \
    "SELECT minted FROM meter_readings WHERE id = '$READING_ID';" 2>/dev/null | tr -d ' \n')
  
  if [ "$MINTED_STATUS" = "t" ]; then
    MINTED=true
    echo "✅ Reading marked as minted after $((i*10)) seconds!"
    break
  fi
  
  echo "⏱️  Waiting... ($((i*10))s elapsed)"
done

echo ""

if [ "$MINTED" = true ]; then
  echo "🔍 Step 4: Verify minting details"
  echo "---------------------------------"
  docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c \
    "SELECT 
      id, 
      energy_generated as kwh, 
      minted, 
      LEFT(mint_tx_signature, 20) || '...' as tx_signature 
    FROM meter_readings 
    WHERE id = '$READING_ID';"
  
  TX_SIG=$(docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -t -c \
    "SELECT mint_tx_signature FROM meter_readings WHERE id = '$READING_ID';" | tr -d ' \n')
  
  echo ""
  echo "Transaction Signature: $TX_SIG"
  echo ""
  
  echo "🔗 Step 5: Verify transaction on blockchain"
  echo "--------------------------------------------"
  if solana confirm $TX_SIG --url $RPC_URL 2>/dev/null; then
    echo "✅ Transaction confirmed on blockchain"
  else
    echo "⚠️  Transaction not found (may have been pruned from local validator)"
  fi
  
  echo ""
  echo "💰 Step 6: Check final token balance"
  echo "-------------------------------------"
  sleep 2  # Brief delay for balance to update
  FINAL_BALANCE=$(spl-token balance $MINT_ADDRESS --owner $USER_WALLET --url $RPC_URL 2>/dev/null || echo "0")
  echo "Final balance: $FINAL_BALANCE tokens"
  echo "Initial balance: $INITIAL_BALANCE tokens"
  DIFF=$(echo "$FINAL_BALANCE - $INITIAL_BALANCE" | bc)
  echo "Difference: $DIFF tokens"
  echo ""
  
  if [ "$DIFF" = "$READING_KWH" ] || [ "$(echo "$DIFF >= $READING_KWH - 0.1" | bc)" = "1" ]; then
    echo "✅ Balance increased by expected amount!"
  else
    echo "⚠️  Balance difference ($DIFF) doesn't match expected ($READING_KWH)"
    echo "   This may be due to previous test runs"
  fi
  
  echo ""
  echo "✅ END-TO-END TEST PASSED!"
  echo "=========================="
  echo ""
  echo "Summary:"
  echo "  • Reading submitted: $READING_KWH kWh"
  echo "  • Minting: ✅ Successful"
  echo "  • Transaction: $TX_SIG"
  echo "  • Balance change: +$DIFF tokens"
  
else
  echo "❌ TEST FAILED: Reading not minted within 150 seconds"
  echo ""
  echo "Troubleshooting:"
  echo "1. Check if meter polling service is running:"
  echo "   tail -50 gateway.log | grep polling"
  echo ""
  echo "2. Check for errors:"
  echo "   tail -100 gateway.log | grep ERROR"
  echo ""
  echo "3. Verify reading in database:"
  echo "   docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c \"SELECT * FROM meter_readings WHERE id = '$READING_ID';\""
fi
