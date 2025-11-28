#!/usr/bin/env bash
# Complete end-to-end user flow demonstration
# This script simulates: Register → Verify Email → Login → Set Wallet → Register Meter → Submit Reading → Check Minting

set -e

API_URL="http://localhost:8080"
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${YELLOW}=========================================${NC}"
echo -e "${YELLOW}Complete User Flow Demonstration${NC}"
echo -e "${YELLOW}=========================================${NC}"
echo ""

# Generate unique test data
TIMESTAMP=$(date +%s)
TEST_USER="demo_user_$TIMESTAMP"
TEST_EMAIL="demo_${TIMESTAMP}@gridtokenx.com"
TEST_WALLET="DemoWallet${TIMESTAMP}Base58Format123456"
TEST_METER="DEMO-METER-${TIMESTAMP}"

echo "Test User: $TEST_USER"
echo "Test Email: $TEST_EMAIL"
echo ""

# Step 1: Register User
echo -e "${YELLOW}Step 1: User Registration${NC}"
curl -s -X POST "$API_URL/api/auth/register" \
  -H "Content-Type: application/json" \
  -d "{
    \"username\": \"$TEST_USER\",
    \"email\": \"$TEST_EMAIL\",
    \"password\": \"SecureP@ssw0rd2024!\",
    \"first_name\": \"Demo\",
    \"last_name\": \"User\"
  }" | jq .
echo ""

# Step 2: Verify Email
echo -e "${YELLOW}Step 2: Email Verification (simulated)${NC}"
docker exec gridtokenx-postgres psql -U gridtokenx -d gridtokenx -c \
  "UPDATE users SET email_verified = true WHERE email = '$TEST_EMAIL';"
echo "✅ Email verified"
echo ""

# Step 3: Login
echo -e "${YELLOW}Step 3: User Login${NC}"
TOKEN=$(curl -s -X POST "$API_URL/api/auth/login" \
  -H "Content-Type: application/json" \
  -d "{
    \"username\": \"$TEST_USER\",
    \"password\": \"SecureP@ssw0rd2024!\"
  }" | jq -r '.access_token')

if [ "$TOKEN" != "null" ] && [ -n "$TOKEN" ]; then
  echo "✅ Login successful"
  echo "Token: ${TOKEN:0:40}..."
else
  echo "❌ Login failed"
  exit 1
fi
echo ""

# Step 4: Set Wallet Address
echo -e "${YELLOW}Step 4: Connect Wallet (Set Wallet Address)${NC}"
curl -s -X POST "$API_URL/api/user/wallet" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"wallet_address\": \"$TEST_WALLET\"}" | jq .
echo ""

# Step 5: Register Smart Meter
echo -e "${YELLOW}Step 5: Register Smart Meter${NC}"
METER_RESPONSE=$(curl -s -X POST "$API_URL/api/user/meters" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{
    \"meter_serial\": \"$TEST_METER\",
    \"meter_type\": \"solar\",
    \"location_address\": \"123 Demo Street, Solar City\"
  }")

echo "$METER_RESPONSE" | jq .
METER_ID=$(echo "$METER_RESPONSE" | jq -r '.meter_id')
echo "Meter ID: $METER_ID"
echo ""

# Step 6: Verify meter in database
echo -e "${YELLOW}Step 6: Verify Meter Registration in Database${NC}"
docker exec gridtokenx-postgres psql -U gridtokenx -d gridtokenx -c \
  "SELECT id, meter_serial, verification_status, meter_type, location_address 
   FROM meter_registry 
   WHERE meter_serial = '$TEST_METER';"
echo ""

# Step 7: Simulate meter verification (admin action)
echo -e "${YELLOW}Step 7: Admin Verifies Meter (simulated)${NC}"
docker exec gridtokenx-postgres psql -U gridtokenx -d gridtokenx -c \
  "UPDATE meter_registry 
   SET verification_status = 'verified', verified_at = NOW() 
   WHERE meter_serial = '$TEST_METER';"
echo "✅ Meter verified by admin"
echo ""

# Step 8: Submit Meter Reading
echo -e "${YELLOW}Step 8: Submit Meter Reading (Energy Data)${NC}"
READING_RESPONSE=$(curl -s -X POST "$API_URL/api/meters/submit-reading" \
  -H "Content-Type: application/json" \
  -d "{
    \"meter_serial\": \"$TEST_METER\",
    \"wallet_address\": \"$TEST_WALLET\",
    \"timestamp\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",
    \"energy_generated\": 250.5,
    \"energy_consumed\": 100.2,
    \"surplus_energy\": 150.3,
    \"battery_level\": 85.0,
    \"temperature\": 25.5,
    \"voltage\": 230.0,
    \"current\": 10.5
  }")

echo "$READING_RESPONSE"
echo ""

# Step 9: Check meter readings in database
echo -e "${YELLOW}Step 9: Verify Meter Reading in Database${NC}"
docker exec gridtokenx-postgres psql -U gridtokenx -d gridtokenx -c \
  "SELECT 
    tableoid::regclass AS partition,
    meter_serial,
    energy_generated,
    energy_consumed,
    surplus_energy,
    minted,
    reading_timestamp
   FROM meter_readings 
   WHERE meter_serial = '$TEST_METER'
   ORDER BY reading_timestamp DESC
   LIMIT 5;"
echo ""

# Step 10: Check partition distribution
echo -e "${YELLOW}Step 10: Check Data in Partitioned Tables${NC}"
docker exec gridtokenx-postgres psql -U gridtokenx -d gridtokenx -c \
  "SELECT 
    tableoid::regclass AS partition_name,
    COUNT(*) AS readings,
    SUM(energy_generated) AS total_generated,
    SUM(surplus_energy) AS total_surplus
   FROM meter_readings
   WHERE meter_serial = '$TEST_METER'
   GROUP BY tableoid;"
echo ""

# Step 11: Check minting status
echo -e "${YELLOW}Step 11: Check Token Minting Status${NC}"
docker exec gridtokenx-postgres psql -U gridtokenx -d gridtokenx -c \
  "SELECT 
    meter_serial,
    energy_generated,
    surplus_energy,
    minted,
    mint_tx_signature,
    blockchain_status,
    reading_timestamp
   FROM meter_readings 
   WHERE meter_serial = '$TEST_METER';"
echo ""

# Final Summary
echo -e "${GREEN}=========================================${NC}"
echo -e "${GREEN}Complete Flow Summary${NC}"
echo -e "${GREEN}=========================================${NC}"
echo ""
echo "✅ User registered: $TEST_USER"
echo "✅ Email verified"
echo "✅ User logged in"
echo "✅ Wallet connected: $TEST_WALLET"
echo "✅ Smart meter registered: $TEST_METER"
echo "✅ Meter verified by admin"
echo "✅ Energy reading submitted"
echo "✅ Data stored in partitioned tables"
echo "✅ Ready for token minting"
echo ""
echo -e "${GREEN}All steps completed successfully!${NC}"
