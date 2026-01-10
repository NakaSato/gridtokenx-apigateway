#!/bin/bash
# Test script for meter registration endpoint
# Requires: API Gateway running on localhost:4000

set -e

API_BASE="http://localhost:4000"
EMAIL="seller@gridtokenx.com"
PASSWORD="Seller123!"

echo "=== GridTokenX Meter Registration Test ==="
echo ""

# 1. Login to get JWT token
echo "1. Logging in as $EMAIL..."
LOGIN_RESPONSE=$(curl -s -X POST "$API_BASE/api/v1/auth/token" \
  -H "Content-Type: application/json" \
  -d "{\"username\": \"$EMAIL\", \"password\": \"$PASSWORD\"}")

TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r '.access_token')

if [ "$TOKEN" == "null" ] || [ -z "$TOKEN" ]; then
  echo "❌ Login failed. Response:"
  echo "$LOGIN_RESPONSE" | jq .
  exit 1
fi

echo "✅ Login successful. Token: ${TOKEN:0:20}..."
echo ""

# 2. Register a new meter
METER_ID="TEST-METER-$(date +%s)"
WALLET_ADDRESS="9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM"

echo "2. Registering meter: $METER_ID"
REGISTER_RESPONSE=$(curl -s -X POST "$API_BASE/api/v1/simulator/meters/register" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d "{
    \"meter_id\": \"$METER_ID\",
    \"wallet_address\": \"$WALLET_ADDRESS\",
    \"meter_type\": \"solar\",
    \"location\": \"Test Location\",
    \"latitude\": 13.7563,
    \"longitude\": 100.5018,
    \"zone_id\": 1
  }")

echo "Response:"
echo "$REGISTER_RESPONSE" | jq .
echo ""

# 3. Try to register same meter again (should return already exists)
echo "3. Re-registering same meter (should return success - already exists)..."
REREGISTER_RESPONSE=$(curl -s -X POST "$API_BASE/api/v1/simulator/meters/register" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d "{
    \"meter_id\": \"$METER_ID\",
    \"wallet_address\": \"$WALLET_ADDRESS\"
  }")

echo "Response:"
echo "$REREGISTER_RESPONSE" | jq .
echo ""

# 4. Submit reading for registered meter
echo "4. Submitting reading for registered meter: $METER_ID"
READING_RESPONSE=$(curl -s -X POST "$API_BASE/api/meters/submit-reading" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d "{
    \"wallet_address\": \"$WALLET_ADDRESS\",
    \"kwh_amount\": 1.5,
    \"reading_timestamp\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",
    \"meter_serial\": \"$METER_ID\",
    \"voltage\": 230.5,
    \"current\": 6.5,
    \"energy_generated\": 2.0,
    \"energy_consumed\": 0.5,
    \"thd_voltage\": 2.5,
    \"thd_current\": 3.2,
    \"battery_level\": 85.0
  }")

echo "Response:"
echo "$READING_RESPONSE" | jq .
echo ""

# 5. Try submitting reading for unregistered meter (should fail)
echo "5. Submitting reading for UNREGISTERED meter (should fail)..."
UNREGISTERED_RESPONSE=$(curl -s -X POST "$API_BASE/api/meters/submit-reading" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d "{
    \"wallet_address\": \"$WALLET_ADDRESS\",
    \"kwh_amount\": 1.0,
    \"reading_timestamp\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",
    \"meter_serial\": \"UNREGISTERED-FAKE-123\"
  }")

echo "Response:"
echo "$UNREGISTERED_RESPONSE" | jq .
echo ""

# 6. Query meter readings
echo "6. Querying readings for meter: $METER_ID"
QUERY_RESPONSE=$(curl -s -X GET "$API_BASE/api/v1/meters/$METER_ID/readings?limit=5" \
  -H "Authorization: Bearer $TOKEN")

echo "Response (first reading):"
echo "$QUERY_RESPONSE" | jq '.readings[0]'
echo "Total readings: $(echo "$QUERY_RESPONSE" | jq '.total')"
echo ""

# 7. Get meter health
echo "7. Getting health status for meter: $METER_ID"
HEALTH_RESPONSE=$(curl -s -X GET "$API_BASE/api/v1/meters/$METER_ID/health" \
  -H "Authorization: Bearer $TOKEN")

echo "Response:"
echo "$HEALTH_RESPONSE" | jq .
echo ""

# 8. Trigger an alert (Low Voltage)
echo "8. Submitting ABNORMAL reading (low voltage) to trigger alert..."
ALERT_READING_RESPONSE=$(curl -s -X POST "$API_BASE/api/meters/submit-reading" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d "{
    \"wallet_address\": \"$WALLET_ADDRESS\",
    \"kwh_amount\": 0.1,
    \"reading_timestamp\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",
    \"meter_serial\": \"$METER_ID\",
    \"voltage\": 190.0
  }")

echo "Response:"
echo "$ALERT_READING_RESPONSE" | jq .
echo "Health after alert:"
curl -s -X GET "$API_BASE/api/v1/meters/$METER_ID/health" -H "Authorization: Bearer $TOKEN" | jq .
echo ""

# 9. Query historical trends
echo "9. Querying historical trends for meter: $METER_ID"
TREND_RESPONSE=$(curl -s -X GET "$API_BASE/api/v1/meters/$METER_ID/trends?period=hour" \
  -H "Authorization: Bearer $TOKEN")

echo "Response:"
echo "$TREND_RESPONSE" | jq .
echo ""

# 10. Query zone summary
echo "10. Querying zone summary..."
ZONES_RESPONSE=$(curl -s -X GET "$API_BASE/api/v1/meters/zones" \
  -H "Authorization: Bearer $TOKEN")

echo "Response:"
echo "$ZONES_RESPONSE" | jq .
echo ""

# 11. Query specific zone stats
echo "11. Querying stats for Zone 1..."
ZONE_STATS_RESPONSE=$(curl -s -X GET "$API_BASE/api/v1/meters/zones/1/stats" \
  -H "Authorization: Bearer $TOKEN")

echo "Response:"
echo "$ZONE_STATS_RESPONSE" | jq .
echo ""

# 12. Update meter zone
echo "12. Updating zone for meter: $METER_ID to Zone 2..."
UPDATE_ZONE_RESPONSE=$(curl -s -X PATCH "$API_BASE/api/v1/meters/$METER_ID" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d "{
    \"zone_id\": 2
  }")

echo "Response:"
echo "$UPDATE_ZONE_RESPONSE" | jq .
echo "Meters in Zone 2:"
curl -s -X GET "$API_BASE/api/v1/meters/zones" -H "Authorization: Bearer $TOKEN" | jq '.[] | select(.zone_id == 2)'
echo ""

echo "=== Test Complete ==="
