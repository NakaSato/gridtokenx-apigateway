#!/bin/bash
# Test script for Zone-Based Economic Insights

API_BASE="http://localhost:3000"
USER_EMAIL="admin@gridtokenx.com"
USER_PASS="Admin123!"

echo "Logging in as admin..."
LOGIN_RESPONSE=$(curl -s -X POST "$API_BASE/api/v1/auth/login" \
  -H "Content-Type: application/json" \
  -d "{
    \"email\": \"$USER_EMAIL\",
    \"password\": \"$USER_PASS\"
  }")

TOKEN=$(echo "$LOGIN_RESPONSE" | jq -r .token)

if [ "$TOKEN" == "null" ] || [ -z "$TOKEN" ]; then
  echo "Login failed!"
  echo "$LOGIN_RESPONSE"
  exit 1
fi

echo "Token acquired."
echo ""

# 1. Test Admin Stats (Fixed)
echo "1. Getting Admin Stats..."
curl -s -X GET "$API_BASE/api/v1/analytics/admin/stats" \
  -H "Authorization: Bearer $TOKEN" | jq .
echo ""

# 2. Test Zone Economic Insights
echo "2. Getting Zone Economic Insights (24h)..."
curl -s -X GET "$API_BASE/api/v1/analytics/admin/zones/economic?timeframe=24h" \
  -H "Authorization: Bearer $TOKEN" | jq .
echo ""

echo "3. Getting Zone Economic Insights (30d)..."
curl -s -X GET "$API_BASE/api/v1/analytics/admin/zones/economic?timeframe=30d" \
  -H "Authorization: Bearer $TOKEN" | jq .
echo ""

echo "Done."
