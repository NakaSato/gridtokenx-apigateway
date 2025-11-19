#!/usr/bin/env bash

# GridTokenX API Gateway - Enhanced Integration Test with Performance Metrics
# Improved version with better error handling, timing, and validation

set -uo pipefail
IFS=$'\n\t'

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m'

# Configuration
API_BASE_URL="${API_BASE_URL:-http://localhost:8080}"
DATABASE_URL="${DATABASE_URL:-postgresql://gridtokenx_user:gridtokenx_password@localhost:5432/gridtokenx}"
SLEEP_TIME="${SLEEP_TIME:-1}"
VERBOSE="${VERBOSE:-false}"
STRICT_MODE="${STRICT_MODE:-false}"  # Exit on any failure
SAVE_RESPONSES="${SAVE_RESPONSES:-false}"  # Save all responses to files
SKIP_DB_VERIFICATION="${SKIP_DB_VERIFICATION:-false}" # Skip DB updates if true

# Portable lowercase helper (macOS bash doesn't support ${var,,})
to_lower() { echo "$1" | tr '[:upper:]' '[:lower:]'; }

# Only enable strict (exit on first non-zero) behavior if STRICT_MODE=true
if [ "$(to_lower "$STRICT_MODE")" = "true" ]; then
    set -e
fi

# Performance metrics
START_TIME=$(date +%s)
PASSED_TESTS=0
FAILED_TESTS=0
WARNING_TESTS=0
TOTAL_TESTS=15

# Response storage
RESPONSE_DIR="/tmp/gridtokenx-test-$$"
if [ "$(to_lower "$SAVE_RESPONSES")" = "true" ]; then
    mkdir -p "$RESPONSE_DIR"
    echo "Saving responses to: $RESPONSE_DIR"
fi

# Generate unique test data
TIMESTAMP=$(date +%s)
BUYER_EMAIL="buyer_${TIMESTAMP}@test.com"
SELLER_EMAIL="seller_${TIMESTAMP}@test.com"
PASSWORD="Test123!@#"
BUYER_WALLET="DYw8jZ9RfRfQqPkZHvPWqL5F7yKqWqfH8xKxCxJxQxXx"
SELLER_WALLET="5Xj7hWqKqV9YGJ8r3nPqM8K4dYwZxNfR2tBpLmCvHgE3"

# Test results storage
# Using simple arrays instead of associative arrays for compatibility
TEST_TIMES=()
TEST_RESULTS=()

# Required command checks
REQUIRED_CMDS=("curl" "jq")
for cmd in "${REQUIRED_CMDS[@]}"; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo -e "${YELLOW}⚠ Warning: required command '$cmd' not found. Some checks may be skipped.${NC}"
        if [ "$cmd" = "jq" ]; then
            echo -e "${YELLOW}⚠ jq is required for JSON parsing; install it or set VERBOSE=true to bypass parsing output.${NC}"
        fi
    fi
done

print_header() {
    echo -e "\n${BLUE}════════════════════════════════════════${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}════════════════════════════════════════${NC}\n"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
    ((PASSED_TESTS++))
}

print_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
    ((WARNING_TESTS++))
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
    ((FAILED_TESTS++))
    if [ "$(to_lower "$STRICT_MODE")" = "true" ]; then
        print_final_summary
        exit 1
    fi
}

print_verbose() {
    if [ "$(to_lower "$VERBOSE")" = "true" ]; then
        echo -e "${CYAN}[DEBUG] $1${NC}"
    fi
}

print_info() {
    echo -e "${CYAN}→ $1${NC}"
}

measure_time() {
    local start=$1
    local end=$(date +%s)
    echo $((end - start))
}

validate_json() {
    local json="$1"
    if command -v jq >/dev/null 2>&1; then
        if echo "$json" | jq empty 2>/dev/null; then
            return 0
        else
            return 1
        fi
    fi
    return 1
}

save_response() {
    if [ "$(to_lower "$SAVE_RESPONSES")" = "true" ]; then
        local step="$1"
        local body="$2"
        echo "$body" > "$RESPONSE_DIR/step_${step}_response.json"
    fi
}

# API call wrapper with timing and validation
api_call() {
    local method="$1"
    local endpoint="$2"
    local auth_token="$3"
    local data="$4"
    local step_num="$5"
    
    local start_time=$(date +%s)
    
    if [ -z "$data" ]; then
        if [ -z "$auth_token" ]; then
            RESPONSE=$(curl -s -w "\n%{http_code}" -X "$method" "$API_BASE_URL$endpoint")
        else
            RESPONSE=$(curl -s -w "\n%{http_code}" -X "$method" "$API_BASE_URL$endpoint" \
                -H "Authorization: Bearer $auth_token")
        fi
    else
        if [ "$(to_lower "$VERBOSE")" = "true" ]; then
            print_verbose "Payload (raw): $data"
        fi
        # Convert multi-line JSON to a single-line payload to avoid server JSON parse issues
        local payload
        # Replace literal "\n" sequences and carriage returns with spaces, producing a single-line payload
        payload=$(printf '%s' "$data" | sed -E 's/\\n/ /g; s/\\r/ /g; s/\\t/ /g')
        if [ "$(to_lower "$VERBOSE")" = "true" ]; then
            print_verbose "Payload (single-line): $payload"
        fi
        if [ -z "$auth_token" ]; then
            RESPONSE=$(curl -s -w "\n%{http_code}" -X "$method" "$API_BASE_URL$endpoint" \
                -H "Content-Type: application/json" \
                -d "$payload")
        else
            RESPONSE=$(curl -s -w "\n%{http_code}" -X "$method" "$API_BASE_URL$endpoint" \
                -H "Authorization: Bearer $auth_token" \
                -H "Content-Type: application/json" \
                -d "$payload")
        fi
    fi
    
    HTTP_CODE=$(echo "$RESPONSE" | tail -n 1)
    BODY=$(echo "$RESPONSE" | sed '$d')
    
    local elapsed=$(measure_time $start_time)
    
    # Store timing using indexed arrays
    eval "TEST_TIME_${step_num}=${elapsed}"
    
    save_response "$step_num" "$BODY"
    
    print_verbose "$method $endpoint → HTTP $HTTP_CODE (${elapsed}s)"
}

# Verify email in database
verify_email_in_db() {
    local email="$1"
    
    print_verbose "Verifying email in database: $email"
    
    if [ "$(to_lower "$SKIP_DB_VERIFICATION")" = "true" ]; then
        print_warning "Skipping DB verification because SKIP_DB_VERIFICATION=true"
        return 0
    fi
    
    if [ -z "$DATABASE_URL" ]; then
        print_warning "DATABASE_URL not set - cannot verify email in DB"
        return 1
    fi
    
    PGPASSWORD=$(echo "$DATABASE_URL" | sed -n 's/.*:\/\/[^:]*:\([^@]*\)@.*/\1/p') \
    psql "$DATABASE_URL" -t -c "UPDATE users SET email_verified = true, email_verified_at = NOW() WHERE email = '$email';" > /dev/null 2>&1
    
    if [ $? -eq 0 ]; then
        print_verbose "Email verified successfully in database"
        return 0
    else
        print_warning "Failed to verify email in database"
        return 1
    fi
}

# Check database connectivity
check_database() {
    if [ ! -z "$DATABASE_URL" ]; then
        if command -v psql >/dev/null 2>&1; then
            if PGPASSWORD=$(echo "$DATABASE_URL" | sed -n 's/.*:\/\/[^:]*:\([^@]*\)@.*/\1/p') \
               psql "$DATABASE_URL" -c "SELECT 1;" > /dev/null 2>&1; then
                return 0
            fi
        fi
    fi
    return 1
}

# ============================================================================
# TEST EXECUTION
# ============================================================================

print_header "GridTokenX API Gateway - Enhanced Integration Test"
echo -e "${CYAN}Configuration:${NC}"
echo -e "  API URL:      $API_BASE_URL"
echo -e "  Verbose:      $VERBOSE"
echo -e "  Strict Mode:  $STRICT_MODE"
echo -e "  Sleep Time:   ${SLEEP_TIME}s"
echo -e "  Timestamp:    $TIMESTAMP"

# Test 1: Health Check
print_header "1. Health Check"
api_call "GET" "/health" "" "" "1"

if [ "$HTTP_CODE" -eq 200 ]; then
    print_success "Server is running (${TEST_TIME_1}s)"
    if validate_json "$BODY"; then
        SERVER_VERSION=$(echo "$BODY" | jq -r '.version // "unknown"')
        SERVER_STATUS=$(echo "$BODY" | jq -r '.status // "unknown"')
        print_info "Version: $SERVER_VERSION, Status: $SERVER_STATUS"
    fi
else
    print_error "Server not running (HTTP $HTTP_CODE)"
    exit 1
fi

# Test 1.5: Configuration Check
print_header "1.5. Configuration & Database Check"
if check_database; then
    print_success "Database connection: OK"
else
    print_warning "Database connection: Failed"
fi

CONFIG_RESPONSE=$(curl -s "$API_BASE_URL/health")
if command -v jq >/dev/null 2>&1 && echo "$CONFIG_RESPONSE" | jq -e '.test_mode == true' >/dev/null 2>&1; then
    print_warning "Server in TEST MODE - email verification may be bypassed"
    TEST_MODE=true
else
    print_success "Server in normal mode - email verification required"
    TEST_MODE=false
fi

sleep $SLEEP_TIME

# Test 2: Register Buyer
print_header "2. Register Buyer"
REGISTER_DATA="{\n    \"email\": \"$BUYER_EMAIL\",\n    \"password\": \"$PASSWORD\",\n    \"first_name\": \"Test\",\n    \"last_name\": \"Buyer\",\n    \"username\": \"buyer_$TIMESTAMP\",\n    \"role\": \"user\"\n}"

api_call "POST" "/api/auth/register" "" "$REGISTER_DATA" "2"

if [ "$HTTP_CODE" -eq 201 ] || [ "$HTTP_CODE" -eq 200 ]; then
    if validate_json "$BODY"; then
        BUYER_ID=$(echo "$BODY" | jq -r '.user_id // .id')
        BUYER_USERNAME=$(echo "$BODY" | jq -r '.username // "buyer_'$TIMESTAMP'"')
        print_success "Buyer registered - ID: $BUYER_ID (${TEST_TIME_2}s)"
        
        # Verify email in database if needed
        if [ "$TEST_MODE" != true ]; then
            verify_email_in_db "$BUYER_EMAIL" || print_warning "DB email verification failed"
        fi
    else
        print_warning "Registration succeeded but invalid JSON response"
    fi
else
    if echo "$BODY" | grep -q "already exists"; then
        print_warning "User already exists - continuing anyway"
    else
        print_error "Buyer registration failed (HTTP $HTTP_CODE)"
        if [ "$(to_lower "$VERBOSE")" = "true" ]; then echo "$BODY"; fi
    fi
fi

sleep $SLEEP_TIME

# Test 3: Login Buyer
print_header "3. Login Buyer"
LOGIN_DATA="{\"username\":\"buyer_$TIMESTAMP\",\"password\":\"$PASSWORD\"}"
api_call "POST" "/api/auth/login" "" "$LOGIN_DATA" "3"

if [ "$HTTP_CODE" -eq 200 ]; then
    BUYER_TOKEN=$(echo "$BODY" | jq -r '.access_token')
    
    if [ ! -z "$BUYER_TOKEN" ] && [ "$BUYER_TOKEN" != "null" ]; then
        TOKEN_PARTS=$(echo "$BUYER_TOKEN" | tr '.' '\n' | wc -l | xargs)
        if [ "$TOKEN_PARTS" -eq 3 ]; then
            print_success "Buyer logged in - JWT valid (${TEST_TIME_3}s)"
            print_verbose "Token length: ${#BUYER_TOKEN} chars"
        else
            print_warning "Login succeeded but invalid JWT structure"
        fi
    else
        print_error "Login succeeded but no token received"
    fi
else
    print_error "Buyer login failed (HTTP $HTTP_CODE)"
    if [ "$(to_lower "$VERBOSE")" = "true" ]; then echo "$BODY"; fi
fi

sleep $SLEEP_TIME

# Test 4: Register Seller
print_header "4. Register Seller"
REGISTER_DATA="{\n    \"email\": \"$SELLER_EMAIL\",\n    \"password\": \"$PASSWORD\",\n    \"first_name\": \"Test\",\n    \"last_name\": \"Seller\",\n    \"username\": \"seller_$TIMESTAMP\",\n    \"role\": \"user\"\n}"

api_call "POST" "/api/auth/register" "" "$REGISTER_DATA" "4"

if [ "$HTTP_CODE" -eq 201 ] || [ "$HTTP_CODE" -eq 200 ]; then
    if validate_json "$BODY"; then
        SELLER_ID=$(echo "$BODY" | jq -r '.user_id // .id')
        print_success "Seller registered - ID: $SELLER_ID (${TEST_TIME_4}s)"
        
        if [ "$TEST_MODE" != true ]; then
            verify_email_in_db "$SELLER_EMAIL" || print_warning "DB email verification failed"
        fi
    fi
else
    print_error "Seller registration failed (HTTP $HTTP_CODE)"
fi

sleep $SLEEP_TIME

# Test 5: Login Seller
print_header "5. Login Seller"
LOGIN_DATA="{\"username\":\"seller_$TIMESTAMP\",\"password\":\"$PASSWORD\"}"
api_call "POST" "/api/auth/login" "" "$LOGIN_DATA" "5"

if [ "$HTTP_CODE" -eq 200 ]; then
    SELLER_TOKEN=$(echo "$BODY" | jq -r '.access_token')
    if [ ! -z "$SELLER_TOKEN" ] && [ "$SELLER_TOKEN" != "null" ]; then
        print_success "Seller logged in successfully (${TEST_TIME_5}s)"
    else
        print_error "Login succeeded but no token"
    fi
else
    print_error "Seller login failed (HTTP $HTTP_CODE)"
fi

sleep $SLEEP_TIME

# Test 6: Connect Buyer Wallet
print_header "6. Connect Buyer Wallet"
WALLET_DATA="{\"wallet_address\": \"$BUYER_WALLET\"}"
api_call "POST" "/api/user/wallet" "$BUYER_TOKEN" "$WALLET_DATA" "6"

if [ "$HTTP_CODE" -eq 200 ]; then
    print_success "Buyer wallet connected (${TEST_TIME_6}s)"
else
    print_warning "Wallet connection failed (HTTP $HTTP_CODE)"
fi

sleep $SLEEP_TIME

# Test 7: Connect Seller Wallet
print_header "7. Connect Seller Wallet"
WALLET_DATA="{\"wallet_address\": \"$SELLER_WALLET\"}"
api_call "POST" "/api/user/wallet" "$SELLER_TOKEN" "$WALLET_DATA" "7"

if [ "$HTTP_CODE" -eq 200 ]; then
    print_success "Seller wallet connected (${TEST_TIME_7}s)"
else
    print_warning "Wallet connection failed (HTTP $HTTP_CODE)"
fi

sleep $SLEEP_TIME

# Test 8: Get Current Epoch
print_header "8. Get Current Market Epoch"
api_call "GET" "/api/market/epoch" "$BUYER_TOKEN" "" "8"

if [ "$HTTP_CODE" -eq 200 ]; then
    if validate_json "$BODY"; then
        EPOCH_ID=$(echo "$BODY" | jq -r '.id // .epoch_id')
        EPOCH_STATUS=$(echo "$BODY" | jq -r '.status')
        print_success "Epoch retrieved - ID: $EPOCH_ID, Status: $EPOCH_STATUS (${TEST_TIME_8}s)"
    fi
else
    print_warning "Could not get epoch (HTTP $HTTP_CODE)"
    EPOCH_ID=""
fi

sleep $SLEEP_TIME

# Test 9: List Orders (Before Creation)
print_header "9. List Trading Orders (Initial)"
api_call "GET" "/api/trading/orders" "$BUYER_TOKEN" "" "9"

if [ "$HTTP_CODE" -eq 200 ]; then
    if validate_json "$BODY"; then
        INITIAL_ORDER_COUNT=$(echo "$BODY" | jq 'length // 0')
        print_success "Orders listed: $INITIAL_ORDER_COUNT orders (${TEST_TIME_9}s)"
    fi
else
    print_warning "Could not list orders (HTTP $HTTP_CODE)"
    INITIAL_ORDER_COUNT=0
fi

sleep $SLEEP_TIME

# Test 10: Create Sell Order
print_header "10. Create Sell Order"
ORDER_DATA="{\n    \"energy_amount\": \"100.0\",\n    \"price_per_kwh\": \"0.15\",\n    \"order_type\": \"Limit\",\n    \"side\": \"Sell\"\n}"

api_call "POST" "/api/trading/orders" "$SELLER_TOKEN" "$ORDER_DATA" "10"

if [ "$HTTP_CODE" -eq 201 ] || [ "$HTTP_CODE" -eq 200 ]; then
    if validate_json "$BODY"; then
        SELL_ORDER_ID=$(echo "$BODY" | jq -r '.id // .order_id')
        if [ -z "$EPOCH_ID" ]; then
            EPOCH_ID=$(echo "$BODY" | jq -r '.epoch_id')
        fi
        ORDER_STATUS=$(echo "$BODY" | jq -r '.status // "unknown"')
        print_success "Sell order created - ID: $SELL_ORDER_ID (${TEST_TIME_10}s)"
        print_info "Status: $ORDER_STATUS"
    fi
else
    print_error "Sell order creation failed (HTTP $HTTP_CODE)"
fi

sleep $SLEEP_TIME

# Test 11: Create Buy Order
print_header "11. Create Buy Order"
ORDER_DATA="{\n    \"energy_amount\": \"100.0\",\n    \"price_per_kwh\": \"0.15\",\n    \"order_type\": \"Limit\",\n    \"side\": \"Buy\"\n}"

api_call "POST" "/api/trading/orders" "$BUYER_TOKEN" "$ORDER_DATA" "11"

if [ "$HTTP_CODE" -eq 201 ] || [ "$HTTP_CODE" -eq 200 ]; then
    if validate_json "$BODY"; then
        BUY_ORDER_ID=$(echo "$BODY" | jq -r '.id // .order_id')
        ORDER_STATUS=$(echo "$BODY" | jq -r '.status // "unknown"')
        print_success "Buy order created - ID: $BUY_ORDER_ID (${TEST_TIME_11}s)"
        print_info "Status: $ORDER_STATUS"
    fi
else
    print_error "Buy order creation failed (HTTP $HTTP_CODE)"
fi

sleep $SLEEP_TIME

# Test 12: List Orders (After Creation)
print_header "12. List Trading Orders (After Creation)"
api_call "GET" "/api/trading/orders" "$BUYER_TOKEN" "" "12"

if [ "$HTTP_CODE" -eq 200 ]; then
    if validate_json "$BODY"; then
        AFTER_ORDER_COUNT=$(echo "$BODY" | jq 'length // 0')
        NEW_ORDERS=$((AFTER_ORDER_COUNT - INITIAL_ORDER_COUNT))
        print_success "Orders listed: $AFTER_ORDER_COUNT total (+$NEW_ORDERS new) (${TEST_TIME_12}s)"
    fi
else
    print_warning "Could not list orders (HTTP $HTTP_CODE)"
fi

sleep $SLEEP_TIME

# Test 13: Get Order Book
print_header "13. Get Market Order Book"
api_call "GET" "/api/market/orderbook" "$BUYER_TOKEN" "" "13"

if [ "$HTTP_CODE" -eq 200 ]; then
    if validate_json "$BODY"; then
        BUY_COUNT=$(echo "$BODY" | jq '.bids | length')
        SELL_COUNT=$(echo "$BODY" | jq '.asks | length')
        print_success "Order book retrieved - $BUY_COUNT bids, $SELL_COUNT asks (${TEST_TIME_13}s)"
    fi
else
    print_warning "Could not get order book (HTTP $HTTP_CODE)"
fi

sleep $SLEEP_TIME

# Test 14: Get User Profile
print_header "14. Get User Profile"
api_call "GET" "/api/user/profile" "$BUYER_TOKEN" "" "14"

if [ "$HTTP_CODE" -eq 200 ]; then
    print_success "Profile retrieved (${TEST_TIME_14}s)"
else
    print_warning "Could not get profile (HTTP $HTTP_CODE)"
fi

sleep $SLEEP_TIME

# Test 15: Get Market Stats
print_header "15. Get Market Statistics"
api_call "GET" "/api/market/stats" "$BUYER_TOKEN" "" "15"

if [ "$HTTP_CODE" -eq 200 ]; then
    if validate_json "$BODY"; then
        print_success "Market stats retrieved (${TEST_TIME_15}s)"
        if [ "$(to_lower "$VERBOSE")" = "true" ]; then
            echo "$BODY" | jq '.'
        fi
    fi
elif [ "$HTTP_CODE" -eq 404 ]; then
    print_warning "Market stats endpoint not available"
else
    print_warning "Could not get market stats (HTTP $HTTP_CODE)"
fi

sleep $SLEEP_TIME

# ============================================================================
# FINAL SUMMARY
# ============================================================================

print_final_summary() {
    local end_time=$(date +%s)
    local total_time=$((end_time - START_TIME))
    
    print_header "Test Execution Summary"
    
    echo -e "${BLUE}╔══════════════════════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║        GridTokenX API Test Results                   ║${NC}"
    echo -e "${BLUE}╚══════════════════════════════════════════════════════╝${NC}"
    
    echo -e "\n${CYAN}Performance Metrics:${NC}"
    echo -e "  Total execution time:  ${YELLOW}${total_time}s${NC}"
    echo -e "  Average per test:      ${YELLOW}$((total_time / TOTAL_TESTS))s${NC}"
    
    echo -e "\n${CYAN}Test Results:${NC}"
    echo -e "  ${GREEN}✓ Passed:${NC}  $PASSED_TESTS"
    echo -e "  ${YELLOW}⚠ Warnings:${NC} $WARNING_TESTS"
    echo -e "  ${RED}✗ Failed:${NC}  $FAILED_TESTS"
    
    local success_rate=0
    local total_run=$((PASSED_TESTS + WARNING_TESTS + FAILED_TESTS))
    if [ $total_run -gt 0 ]; then
        success_rate=$((PASSED_TESTS * 100 / total_run))
    fi
    echo -e "  Success rate:          ${YELLOW}${success_rate}%${NC}"
    
    echo -e "\n${CYAN}Test Identifiers:${NC}"
    echo -e "  Timestamp:    $TIMESTAMP"
    echo -e "  Buyer ID:     ${BUYER_ID:-N/A}"
    echo -e "  Seller ID:    ${SELLER_ID:-N/A}"
    echo -e "  Buy Order:    ${BUY_ORDER_ID:-N/A}"
    echo -e "  Sell Order:   ${SELL_ORDER_ID:-N/A}"
    echo -e "  Epoch ID:     ${EPOCH_ID:-N/A}"
    
    if [ "$(to_lower "$SAVE_RESPONSES")" = "true" ]; then
        echo -e "\n${CYAN}Response Files:${NC}"
        echo -e "  Saved to: $RESPONSE_DIR"
    fi
    
    echo -e "\n${CYAN}Slowest Tests:${NC}"
    # Collect all timing data
    local timing_data=""
    for i in {1..15}; do
        local time_var="TEST_TIME_${i}"
        local time_val="${!time_var}"
        if [ ! -z "$time_val" ]; then
            timing_data="${timing_data}${i} ${time_val}\n"
        fi
    done
    
    # Sort and display top 5 slowest
    echo -e "$timing_data" | sort -k2 -rn | head -5 | while read step time; do
        if [ ! -z "$step" ]; then
            echo -e "  Step $step: ${time}s"
        fi
    done
    
    if [ $FAILED_TESTS -gt 0 ]; then
        echo -e "\n${RED}════════════════════════════════════════════════════${NC}"
        echo -e "${RED}✗ Tests completed with $FAILED_TESTS failure(s)${NC}"
        echo -e "${RED}════════════════════════════════════════════════════${NC}\n"
        exit 1
    else
        echo -e "\n${GREEN}════════════════════════════════════════════════════${NC}"
        echo -e "${GREEN}✓ All tests completed successfully!${NC}"
        echo -e "${GREEN}════════════════════════════════════════════════════${NC}\n"
        exit 0
    fi
}

print_final_summary
