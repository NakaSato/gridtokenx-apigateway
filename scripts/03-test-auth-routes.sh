#!/bin/bash

# GridTokenX API Gateway - Auth Routes Test
# Tests public API routes for authentication and saves token to testing.env

set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

# Configuration
API_BASE_URL="${API_BASE_URL:-http://localhost:8080}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_FILE="$SCRIPT_DIR/testing.env"
SLEEP_TIME=1

# Function to restart the API server
restart_server() {
    echo -e "\n${BLUE}Restarting API Gateway...${NC}"
    
    # Find and kill existing server process on port 8080
    PID=$(lsof -t -i:8080)
    if [ ! -z "$PID" ]; then
        echo "Killing existing server (PID $PID)..."
        kill -9 $PID
    fi
    
    # Start server in background
    echo "Starting server..."
    cd "$SCRIPT_DIR/.." && cargo run > server.log 2>&1 &
    
    # Wait for server to be ready
    echo "Waiting for server to be ready..."
    for i in {1..60}; do
        if curl -s http://localhost:8080/health > /dev/null; then
            echo -e "${GREEN}Server is ready!${NC}"
            return 0
        fi
        sleep 2
        echo -n "."
    done
    
    echo -e "${RED}Server failed to start. Check server.log for details.${NC}"
    exit 1
}

# Restart server before running tests
restart_server

# Generate unique test data
TIMESTAMP=$(date +%s)
EMAIL="auth_test_${TIMESTAMP}@test.com"
PASSWORD="Test123!@#"
USERNAME="auth_test_${TIMESTAMP}"
# Generate a random-looking wallet address (base58-like chars)
WALLET_ADDRESS="AuthTestWallet${TIMESTAMP}xXxXxXxXxXxXxXxXxXx"

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

# 1. Register User
print_header "1. Register User"
echo "Registering user: $EMAIL"
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_BASE_URL/api/auth/register" \
    -H "Content-Type: application/json" \
    -d "{
        \"email\": \"$EMAIL\",
        \"password\": \"$PASSWORD\",
        \"first_name\": \"Auth\",
        \"last_name\": \"Test\",
        \"username\": \"$USERNAME\"
    }")

HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" -eq 201 ] || [ "$HTTP_CODE" -eq 200 ]; then
    print_success "User registered successfully"
    echo "$BODY" | jq '.'
    
    # Extract verification token if present (for later use)
    VERIFICATION_TOKEN=$(echo "$BODY" | jq -r '.email_verification_token // empty')
else
    print_error "Registration failed (HTTP $HTTP_CODE)"
    echo "$BODY"
    exit 1
fi

sleep $SLEEP_TIME

# 2. Login User
print_header "2. Login User"
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_BASE_URL/api/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\"}")

HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" -eq 200 ]; then
    print_success "Login successful"
    TOKEN=$(echo "$BODY" | jq -r '.access_token')
    
    if [ ! -z "$TOKEN" ] && [ "$TOKEN" != "null" ]; then
        echo "Token received: ${TOKEN:0:20}..."
        
        # Save to testing.env
        echo "JWT_TOKEN=$TOKEN" > "$ENV_FILE"
        echo "TEST_USER_EMAIL=$EMAIL" >> "$ENV_FILE"
        echo "TEST_USER_ID=$(echo "$BODY" | jq -r '.user.id // empty')" >> "$ENV_FILE"
        
        print_success "Token saved to $ENV_FILE"
    else
        print_error "No token in response"
        exit 1
    fi
elif [ "$HTTP_CODE" -eq 401 ]; then
    # If login failed due to unverified email, we expect that
    if echo "$BODY" | grep -q "Email not verified"; then
        print_success "Login correctly rejected unverified email"
    else
        print_error "Login failed (HTTP $HTTP_CODE)"
        echo "$BODY"
        exit 1
    fi
else
    print_error "Login failed (HTTP $HTTP_CODE)"
    echo "$BODY"
    exit 1
fi

sleep $SLEEP_TIME

# 3. Verify Email
print_header "3. Verify Email"
if [ ! -z "$VERIFICATION_TOKEN" ]; then
    echo "Verifying with token from registration..."
    RESPONSE=$(curl -s -w "\n%{http_code}" -X GET "$API_BASE_URL/api/auth/verify-email?token=$VERIFICATION_TOKEN")
    
    HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
    BODY=$(echo "$RESPONSE" | sed '$d')
    
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Email verified successfully"
        
        # Retry login to get token if we didn't get it before
        if [ -z "$TOKEN" ]; then
            echo "Retrying login after verification..."
            RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_BASE_URL/api/auth/login" \
                -H "Content-Type: application/json" \
                -d "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\"}")
                
            HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
            BODY=$(echo "$RESPONSE" | sed '$d')
            
            if [ "$HTTP_CODE" -eq 200 ]; then
                TOKEN=$(echo "$BODY" | jq -r '.access_token')
                echo "JWT_TOKEN=$TOKEN" > "$ENV_FILE"
                print_success "Token saved to $ENV_FILE"
            fi
        fi
    else
        print_error "Email verification failed (HTTP $HTTP_CODE)"
        echo "$BODY"
    fi
else
    echo "No verification token available from registration response. Skipping automatic verification."
    # In a real scenario, we might need to check the DB or email service
fi

sleep $SLEEP_TIME

# 4. Resend Verification
print_header "4. Resend Verification"
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_BASE_URL/api/auth/resend-verification" \
    -H "Content-Type: application/json" \
    -d "{\"email\":\"$EMAIL\"}")

HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" -eq 200 ]; then
    print_success "Verification email resent successfully"
    echo "$BODY" | jq '.'
else
    # It might fail if already verified, which is fine
    echo "Resend response (HTTP $HTTP_CODE):"
    echo "$BODY"
fi

sleep $SLEEP_TIME

# 5. Wallet Register
print_header "5. Wallet Register"
# The API expects a full registration payload with wallet options, not just a signature.
WALLET_USER_EMAIL="wallet_test_${TIMESTAMP}@test.com"
WALLET_USERNAME="wallet_test_${TIMESTAMP}"

echo "Registering user with wallet creation: $WALLET_USERNAME"
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_BASE_URL/api/auth/wallet/register" \
    -H "Content-Type: application/json" \
    -d "{
        \"email\": \"$WALLET_USER_EMAIL\",
        \"password\": \"$PASSWORD\",
        \"first_name\": \"Wallet\",
        \"last_name\": \"Test\",
        \"username\": \"$WALLET_USERNAME\",
        \"role\": \"user\",
        \"create_wallet\": true
    }")

HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" -eq 200 ] || [ "$HTTP_CODE" -eq 201 ]; then
    print_success "Wallet registered successfully"
    echo "$BODY" | jq '.'
else
    print_error "Wallet registration failed (HTTP $HTTP_CODE)"
    echo "$BODY"
fi

sleep $SLEEP_TIME

# 6. Wallet Login
print_header "6. Wallet Login"
# The API expects username/password for this endpoint, returning wallet info.
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_BASE_URL/api/auth/wallet/login" \
    -H "Content-Type: application/json" \
    -d "{
        \"username\": \"$WALLET_USERNAME\",
        \"password\": \"$PASSWORD\"
    }")

HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_CODE" -eq 200 ]; then
    print_success "Wallet login successful"
    echo "$BODY" | jq '.'
else
    print_error "Wallet login failed (HTTP $HTTP_CODE)"
    echo "$BODY"
fi

echo -e "\n${BLUE}Test sequence completed.${NC}"
