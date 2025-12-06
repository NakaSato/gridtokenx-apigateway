#!/bin/bash

# Configuration
API_URL="${API_URL:-http://localhost:8080}"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

echo "========================================================"
echo "Verifying Registry Gap Implementation"
echo "========================================================"

# 1. Register User with Wallet Creation (Triggers RegisterUser on-chain)
echo -e "\nStep 1: Register User with Create Wallet (expecting on-chain tx)"
USERNAME="gap_test_$(date +%s)"
EMAIL="${USERNAME}@example.com"
PASSWORD="StrongP@ssw0rd!${RANDOM}"

RESPONSE=$(curl -s -X POST "${API_URL}/api/auth/wallet/register" \
  -H "Content-Type: application/json" \
  -d "{
    \"username\": \"${USERNAME}\",
    \"email\": \"${EMAIL}\",
    \"password\": \"${PASSWORD}\",
    \"role\": \"consumer\",
    \"first_name\": \"Gap\",
    \"last_name\": \"Tester\",
    \"create_wallet\": true,
    \"airdrop_amount\": 1.0
  }")

ACCESS_TOKEN=$(echo $RESPONSE | jq -r '.access_token')
WALLET_ADDRESS=$(echo $RESPONSE | jq -r '.wallet_info.address')

if [ "$ACCESS_TOKEN" != "null" ] && [ "$ACCESS_TOKEN" != "" ]; then
    echo -e "${GREEN}✓ User registration successful${NC}"
    echo "  Wallet: $WALLET_ADDRESS"
    echo "  Token: ${ACCESS_TOKEN:0:10}..."
else
    echo -e "${RED}✗ User registration failed${NC}"
    echo "Response: $RESPONSE"
    exit 1
fi

# 2. Register Meter (Triggers RegisterMeter on-chain by Gateway)
echo -e "\nStep 2: Register Meter (expecting Gateway to register on-chain)"
METER_SERIAL="GAP-METER-$(date +%s)"
# Use a random public key for test
# Generate unique meter public key
solana-keygen new --no-passphrase --outfile /tmp/gap_test_meter.json > /dev/null 2>&1
METER_PUBKEY=$(solana-keygen pubkey /tmp/gap_test_meter.json)
rm /tmp/gap_test_meter.json

RESPONSE=$(curl -s -X POST "${API_URL}/api/user/meters" \
  -H "Authorization: Bearer ${ACCESS_TOKEN}" \
  -H "Content-Type: application/json" \
  -d "{
    \"meter_serial\": \"${METER_SERIAL}\",
    \"meter_public_key\": \"${METER_PUBKEY}\",
    \"meter_type\": \"solar\",
    \"location_address\": \"Test Location\",
    \"manufacturer\": \"TestMfg\",
    \"installation_date\": \"2023-01-01\"
  }")

METER_ID=$(echo $RESPONSE | jq -r '.meter_id')

if [ "$METER_ID" != "null" ] && [ "$METER_ID" != "" ]; then
    echo -e "${GREEN}✓ Meter registration successful${NC}"
    echo "  Meter ID: $METER_ID"
else
    echo -e "${RED}✗ Meter registration failed${NC}"
    echo "Response: $RESPONSE"
    exit 1
fi

echo -e "\n${GREEN}Tests Completed.${NC} Please check API Gateway logs for 'User registered on-chain successfully' and 'Meter registered on-chain successfully'."
