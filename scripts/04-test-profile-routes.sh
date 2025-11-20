#!/bin/bash

# GridTokenX API Gateway - Profile Routes Test
# Tests protected profile management routes

set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Configuration
API_BASE_URL="${API_BASE_URL:-http://localhost:8080}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SLEEP_TIME=1

print_header() {
    echo -e "\n${BLUE}========================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}========================================${NC}\n"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

# Check if server is running
echo "Checking server status..."
if ! curl -s "$API_BASE_URL/health" > /dev/null; then
    echo -e "${RED}Server is not running at $API_BASE_URL${NC}"
    echo "Please start the server or run 03-test-auth-routes.sh first."
    exit 1
else
    echo -e "${GREEN}Server is running${NC}"
fi

# Generate unique test data
TIMESTAMP=$(date +%s)
EMAIL="profile_test_${TIMESTAMP}@test.com"
PASSWORD="Test123!@#"
NEW_PASSWORD="Str0ng!P@ssw0rd_${TIMESTAMP}"
USERNAME="profile_test_${TIMESTAMP}"

# 1. Setup: Register and Login to get Token
print_header "1. Setup: Register and Login"

echo "Registering user: $EMAIL"
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_BASE_URL/api/auth/register" \
    -H "Content-Type: application/json" \
    -d "{
        \"email\": \"$EMAIL\",
        \"password\": \"$PASSWORD\",
        \"first_name\": \"Profile\",
        \"last_name\": \"Test\",
        \"username\": \"$USERNAME\"
    }")

HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" -ne 201 ] && [ "$HTTP_CODE" -ne 200 ]; then
    print_error "Registration failed (HTTP $HTTP_CODE)"
    echo "$BODY"
    exit 1
fi

# Extract verification token if needed (some setups might require verification before login)
# Assuming for now we can login or the system allows unverified login for testing, 
# OR we need to verify. 03 script verifies. Let's verify just in case.
VERIFICATION_TOKEN=$(echo "$BODY" | jq -r '.email_verification_token // empty')

if [ ! -z "$VERIFICATION_TOKEN" ] && [ "$VERIFICATION_TOKEN" != "null" ]; then
    echo "Verifying email..."
    curl -s "$API_BASE_URL/api/auth/verify-email?token=$VERIFICATION_TOKEN" > /dev/null
fi

echo "Logging in..."
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_BASE_URL/api/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\"}")

HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" -ne 200 ]; then
    print_error "Login failed (HTTP $HTTP_CODE)"
    echo "$BODY"
    exit 1
fi

TOKEN=$(echo "$BODY" | jq -r '.access_token')
if [ -z "$TOKEN" ] || [ "$TOKEN" == "null" ]; then
    print_error "No token received"
    exit 1
fi

print_success "Got JWT Token"
sleep $SLEEP_TIME

# 2. Get Profile
print_header "2. Get Profile"
RESPONSE=$(curl -s -w "\n%{http_code}" -X GET "$API_BASE_URL/api/auth/profile" \
    -H "Authorization: Bearer $TOKEN")

HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" -eq 200 ]; then
    print_success "Profile retrieved successfully"
    echo "$BODY" | jq '.'
else
    print_error "Failed to get profile (HTTP $HTTP_CODE)"
    echo "$BODY"
    exit 1
fi

sleep $SLEEP_TIME

# 3. Update Profile
print_header "3. Update Profile"
echo "Updating profile..."
# Note: The API only accepts email, first_name, last_name, wallet_address
# And the response UserInfo only contains id, username, email, role, wallet_address
# So we can't verify first_name/last_name update via response, but we can verify success status.
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_BASE_URL/api/auth/profile/update" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{
        \"first_name\": \"Updated\",
        \"last_name\": \"Name\"
    }")

HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" -eq 200 ]; then
    print_success "Profile updated successfully"
    echo "$BODY" | jq '.'
else
    print_error "Failed to update profile (HTTP $HTTP_CODE)"
    echo "$BODY"
    exit 1
fi

sleep $SLEEP_TIME

# 4. Change Password
print_header "4. Change Password"
echo "Changing password..."
PAYLOAD="{
    \"current_password\": \"$PASSWORD\",
    \"new_password\": \"$NEW_PASSWORD\"
}"
echo "Payload: $PAYLOAD"

RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_BASE_URL/api/auth/password" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "$PAYLOAD")

HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" -eq 204 ]; then
    print_success "Password changed successfully"
elif [ "$HTTP_CODE" -eq 200 ]; then
    # Some APIs return 200 for success even if 204 is documented/expected
    print_success "Password changed successfully"
    echo "$BODY" | jq '.'
else
    print_error "Failed to change password (HTTP $HTTP_CODE)"
    echo "$BODY"
    exit 1
fi

sleep $SLEEP_TIME

# 5. Verify New Password (Login again)
print_header "5. Verify New Password"
echo "Logging in with new password..."
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_BASE_URL/api/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$USERNAME\",\"password\":\"$NEW_PASSWORD\"}")

HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" -eq 200 ]; then
    print_success "Login with new password successful"
else
    print_error "Login with new password failed (HTTP $HTTP_CODE)"
    echo "$BODY"
    exit 1
fi

echo -e "\n${BLUE}Profile routes test completed successfully.${NC}"
