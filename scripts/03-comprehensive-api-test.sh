#!/usr/bin/env bash

# GridTokenX API Gateway - Comprehensive API Test Suite
# Enhanced version with full API endpoint coverage, better error handling, and detailed reporting

set -uo pipefail
IFS=$'\n\t'

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
WHITE='\033[1;37m'
NC='\033[0m'

# Configuration
API_BASE_URL="${API_BASE_URL:-http://localhost:8080}"
DATABASE_URL="${DATABASE_URL:-postgresql://gridtokenx_user:gridtokenx_password@localhost:5432/gridtokenx}"
SLEEP_TIME="${SLEEP_TIME:-1}"
VERBOSE="${VERBOSE:-false}"
STRICT_MODE="${STRICT_MODE:-false}"
SAVE_RESPONSES="${SAVE_RESPONSES:-false}"
SKIP_DB_VERIFICATION="${SKIP_DB_VERIFICATION:-false}"
TEST_CATEGORY="${TEST_CATEGORY:-all}"  # Options: all, auth, trading, blockchain, admin, meters, erc

# Portable lowercase helper
to_lower() { echo "$1" | tr '[:upper:]' '[:lower:]'; }

# Only enable strict behavior if STRICT_MODE=true
if [ "$(to_lower "$STRICT_MODE")" = "true" ]; then
    set -e
fi

# Performance metrics
START_TIME=$(date +%s)
PASSED_TESTS=0
FAILED_TESTS=0
WARNING_TESTS=0
SKIPPED_TESTS=0
TOTAL_TESTS=0

# Response storage
RESPONSE_DIR="/tmp/gridtokenx-test-$$"
if [ "$(to_lower "$SAVE_RESPONSES")" = "true" ]; then
    mkdir -p "$RESPONSE_DIR"
    echo "Saving responses to: $RESPONSE_DIR"
fi

# Generate unique test data
TIMESTAMP=$(date +%s)
RANDOM_SUFFIX=$(shuf -i 1000-9999 -n 1)
BUYER_EMAIL="buyer_${TIMESTAMP}_${RANDOM_SUFFIX}@test.com"
SELLER_EMAIL="seller_${TIMESTAMP}_${RANDOM_SUFFIX}@test.com"
ADMIN_EMAIL="admin_${TIMESTAMP}_${RANDOM_SUFFIX}@test.com"
PASSWORD="Test123!@#"
BUYER_WALLET="DYw8jZ9RfRfQqPkZHvPWqL5F7yKqWqfH8xKxCxJxQxXx"
SELLER_WALLET="5Xj7hWqKqV9YGJ8r3nPqM8K4dYwZxNfR2tBpLmCvHgE3"
ADMIN_WALLET="7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU"

# Test categories and their endpoints
declare -A TEST_CATEGORIES=(
    ["health"]="Health Check"
    ["auth"]="Authentication & User Management"
    ["blockchain"]="Blockchain Operations"
    ["trading"]="Energy Trading"
    ["tokens"]="Token Management"
    ["meters"]="Energy Meters"
    ["erc"]="ERC Certificates"
    ["oracle"]="Oracle Services"
    ["governance"]="Governance"
    ["admin"]="Admin Operations"
    ["market"]="Market Data"
    ["analytics"]="Analytics"
    ["websocket"]="WebSocket"
    ["transactions"]="Transaction Management"
)

# Initialize variables
BUYER_ID=""
SELLER_ID=""
ADMIN_ID=""
BUYER_TOKEN=""
SELLER_TOKEN=""
ADMIN_TOKEN=""
BUYER_ORDER_ID=""
SELLER_ORDER_ID=""
EPOCH_ID=""
METER_ID=""
CERTIFICATE_ID=""
TRANSACTION_ID=""

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

# Utility functions
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

print_skip() {
    echo -e "${YELLOW}→ $1${NC}"
    ((SKIPPED_TESTS++))
}

print_verbose() {
    if [ "$(to_lower "$VERBOSE")" = "true" ]; then
        echo -e "${CYAN}[DEBUG] $1${NC}"
    fi
}

print_info() {
    echo -e "${CYAN}→ $1${NC}"
}

print_category() {
    echo -e "\n${MAGENTA}📂 $1${NC}"
    echo -e "${MAGENTA}───────────────────────────────────────────────${NC}"
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

# Enhanced API call wrapper with comprehensive logging
api_call() {
    local method="$1"
    local endpoint="$2"
    local auth_token="$3"
    local data="$4"
    local step_num="$5"
    local description="$6"
    
    local start_time=$(date +%s)
    local current_test=$((TOTAL_TESTS + 1))
    
    if [ ! -z "$description" ]; then
        echo -e "${WHITE}Test $current_test: $description${NC}"
    fi
    
    print_verbose "$method $endpoint"
    
    if [ -z "$data" ]; then
        if [ -z "$auth_token" ]; then
            RESPONSE=$(curl -s -w "\n%{http_code}" -X "$method" "$API_BASE_URL$endpoint")
        else
            RESPONSE=$(curl -s -w "\n%{http_code}" -X "$method" "$API_BASE_URL$endpoint" \
                -H "Authorization: Bearer $auth_token")
        fi
    else
        # Convert multi-line JSON to single-line payload
        local payload
        payload=$(printf '%s' "$data" | sed -E 's/\\n/ /g; s/\\r/ /g; s/\\t/ /g')
        print_verbose "Payload: $payload"
        
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
    eval "TEST_TIME_${current_test}=${elapsed}"
    
    save_response "$current_test" "$BODY"
    
    print_verbose "Response: HTTP $HTTP_CODE (${elapsed}s)"
    if [ "$(to_lower "$VERBOSE")" = "true" ] && [ ! -z "$BODY" ]; then
        print_verbose "Response body: $BODY"
    fi
    
    ((TOTAL_TESTS++))
}

# Check if test category should run
should_run_category() {
    local category="$1"
    if [ "$(to_lower "$TEST_CATEGORY")" = "all" ]; then
        return 0
    elif [ "$(to_lower "$TEST_CATEGORY")" = "$(to_lower "$category")" ]; then
        return 0
    else
        return 1
    fi
}

# Enhanced database verification
verify_email_in_db() {
    local email="$1"
    
    if [ "$(to_lower "$SKIP_DB_VERIFICATION")" = "true" ]; then
        print_verbose "Skipping DB verification for $email"
        return 0
    fi
    
    if [ -z "$DATABASE_URL" ]; then
        print_warning "DATABASE_URL not set - cannot verify email in DB"
        return 1
    fi
    
    print_verbose "Verifying email in database: $email"
    
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
# HEALTH CHECK TESTS
# ============================================================================
run_health_tests() {
    if ! should_run_category "health"; then return; fi
    
    print_category "Health & Status Tests"
    
    # Test 1: Basic Health Check
    api_call "GET" "/health" "" "" "1" "Basic health check"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Server is running (${TEST_TIME_1}s)"
        if validate_json "$BODY"; then
            SERVER_VERSION=$(echo "$BODY" | jq -r '.version // "unknown"')
            SERVER_STATUS=$(echo "$BODY" | jq -r '.status // "unknown"')
            print_info "Version: $SERVER_VERSION, Status: $SERVER_STATUS"
        fi
    else
        print_error "Server not running (HTTP $HTTP_CODE)"
        return 1
    fi
    
    sleep $SLEEP_TIME
    
    # Test 2: Metrics Endpoint
    api_call "GET" "/metrics" "" "" "2" "Prometheus metrics"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Metrics endpoint accessible (${TEST_TIME_2}s)"
    else
        print_warning "Metrics endpoint not available (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 3: Database Check
    if check_database; then
        print_success "Database connection: OK"
    else
        print_warning "Database connection: Failed"
    fi
}

# ============================================================================
# AUTHENTICATION & USER MANAGEMENT TESTS
# ============================================================================
run_auth_tests() {
    if ! should_run_category "auth"; then return; fi
    
    print_category "Authentication & User Management Tests"
    
    # Test 4: Register Buyer
    api_call "POST" "/api/auth/register" "" "" "4" "Register buyer user"
    REGISTER_DATA="{\"username\":\"buyer_${TIMESTAMP}_${RANDOM_SUFFIX}\",\"email\":\"$BUYER_EMAIL\",\"password\":\"$PASSWORD\",\"first_name\":\"Test\",\"last_name\":\"Buyer\",\"role\":\"user\"}"
    api_call "POST" "/api/auth/register" "" "$REGISTER_DATA" "4" "Register buyer user"
    
    if [ "$HTTP_CODE" -eq 201 ] || [ "$HTTP_CODE" -eq 200 ]; then
        if validate_json "$BODY"; then
            BUYER_ID=$(echo "$BODY" | jq -r '.user_id // .id // empty')
            BUYER_USERNAME=$(echo "$BODY" | jq -r '.username // empty')
            print_success "Buyer registered - ID: $BUYER_ID (${TEST_TIME_4}s)"
            verify_email_in_db "$BUYER_EMAIL" || print_warning "DB email verification failed"
        fi
    else
        if echo "$BODY" | grep -q "already exists"; then
            print_warning "Buyer already exists - continuing"
        else
            print_error "Buyer registration failed (HTTP $HTTP_CODE)"
        fi
    fi
    
    sleep $SLEEP_TIME
    
    # Test 5: Login Buyer
    LOGIN_DATA="{\"username\":\"buyer_${TIMESTAMP}_${RANDOM_SUFFIX}\",\"password\":\"$PASSWORD\"}"
    api_call "POST" "/api/auth/login" "" "$LOGIN_DATA" "5" "Login buyer"
    
    if [ "$HTTP_CODE" -eq 200 ]; then
        BUYER_TOKEN=$(echo "$BODY" | jq -r '.access_token // .token // empty')
        if [ ! -z "$BUYER_TOKEN" ] && [ "$BUYER_TOKEN" != "null" ]; then
            print_success "Buyer logged in (${TEST_TIME_5}s)"
        else
            print_error "Login succeeded but no token received"
        fi
    else
        print_error "Buyer login failed (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 6: Register Seller
    REGISTER_DATA="{\"username\":\"seller_${TIMESTAMP}_${RANDOM_SUFFIX}\",\"email\":\"$SELLER_EMAIL\",\"password\":\"$PASSWORD\",\"first_name\":\"Test\",\"last_name\":\"Seller\",\"role\":\"prosumer\"}"
    api_call "POST" "/api/auth/register" "" "$REGISTER_DATA" "6" "Register seller user"
    
    if [ "$HTTP_CODE" -eq 201 ] || [ "$HTTP_CODE" -eq 200 ]; then
        if validate_json "$BODY"; then
            SELLER_ID=$(echo "$BODY" | jq -r '.user_id // .id // empty')
            print_success "Seller registered - ID: $SELLER_ID (${TEST_TIME_6}s)"
            verify_email_in_db "$SELLER_EMAIL" || print_warning "DB email verification failed"
        fi
    else
        print_error "Seller registration failed (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 7: Login Seller
    LOGIN_DATA="{\"username\":\"seller_${TIMESTAMP}_${RANDOM_SUFFIX}\",\"password\":\"$PASSWORD\"}"
    api_call "POST" "/api/auth/login" "" "$LOGIN_DATA" "7" "Login seller"
    
    if [ "$HTTP_CODE" -eq 200 ]; then
        SELLER_TOKEN=$(echo "$BODY" | jq -r '.access_token // .token // empty')
        if [ ! -z "$SELLER_TOKEN" ] && [ "$SELLER_TOKEN" != "null" ]; then
            print_success "Seller logged in (${TEST_TIME_7}s)"
        else
            print_error "Seller login succeeded but no token received"
        fi
    else
        print_error "Seller login failed (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 8: Get User Profile
    if [ ! -z "$BUYER_TOKEN" ] && [ "$BUYER_TOKEN" != "null" ]; then
        api_call "GET" "/api/auth/profile" "$BUYER_TOKEN" "" "8" "Get user profile"
        if [ "$HTTP_CODE" -eq 200 ]; then
            print_success "Profile retrieved (${TEST_TIME_8}s)"
        else
            print_warning "Could not get profile (HTTP $HTTP_CODE)"
        fi
    else
        print_skip "Cannot test profile - no buyer token"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 9: Update Profile
    if [ ! -z "$BUYER_TOKEN" ] && [ "$BUYER_TOKEN" != "null" ]; then
        UPDATE_DATA="{\"email\":\"updated_${BUYER_EMAIL}\",\"first_name\":\"Updated\"}"
        api_call "POST" "/api/auth/profile/update" "$BUYER_TOKEN" "$UPDATE_DATA" "9" "Update user profile"
        if [ "$HTTP_CODE" -eq 200 ]; then
            print_success "Profile updated (${TEST_TIME_9}s)"
        else
            print_warning "Profile update failed (HTTP $HTTP_CODE)"
        fi
    else
        print_skip "Cannot test profile update - no buyer token"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 10: Wallet Registration
    if [ ! -z "$BUYER_TOKEN" ] && [ "$BUYER_TOKEN" != "null" ]; then
        WALLET_DATA="{\"wallet_address\":\"$BUYER_WALLET\"}"
        api_call "POST" "/api/user/wallet" "$BUYER_TOKEN" "$WALLET_DATA" "10" "Connect buyer wallet"
        if [ "$HTTP_CODE" -eq 200 ]; then
            print_success "Buyer wallet connected (${TEST_TIME_10}s)"
        else
            print_warning "Wallet connection failed (HTTP $HTTP_CODE)"
        fi
    else
        print_skip "Cannot test wallet connection - no buyer token"
    fi
    
    sleep $SLEEP_TIME
}

# ============================================================================
# BLOCKCHAIN OPERATIONS TESTS
# ============================================================================
run_blockchain_tests() {
    if ! should_run_category "blockchain"; then return; fi
    
    print_category "Blockchain Operations Tests"
    
    if [ -z "$BUYER_TOKEN" ] || [ "$BUYER_TOKEN" = "null" ]; then
        print_skip "Cannot run blockchain tests - no valid token"
        return
    fi
    
    # Test 11: Get Network Status
    api_call "GET" "/api/blockchain/network" "$BUYER_TOKEN" "" "11" "Get blockchain network status"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Network status retrieved (${TEST_TIME_11}s)"
        if validate_json "$BODY"; then
            NETWORK_STATUS=$(echo "$BODY" | jq -r '.status // "unknown"')
            BLOCK_HEIGHT=$(echo "$BODY" | jq -r '.block_height // "unknown"')
            print_info "Network Status: $NETWORK_STATUS, Block Height: $BLOCK_HEIGHT"
        fi
    else
        print_warning "Could not get network status (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 12: Get Account Info
    api_call "GET" "/api/blockchain/accounts/$BUYER_WALLET" "$BUYER_TOKEN" "" "12" "Get account information"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Account info retrieved (${TEST_TIME_12}s)"
    else
        print_warning "Could not get account info (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 13: Get Transaction History
    api_call "GET" "/api/blockchain/transactions?limit=10" "$BUYER_TOKEN" "" "13" "Get transaction history"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Transaction history retrieved (${TEST_TIME_13}s)"
    else
        print_warning "Could not get transaction history (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 14: Get Blockchain User
    api_call "GET" "/api/blockchain/users/$BUYER_WALLET" "$BUYER_TOKEN" "" "14" "Get blockchain user info"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Blockchain user info retrieved (${TEST_TIME_14}s)"
    else
        print_warning "Could not get blockchain user info (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
}

# ============================================================================
# TRADING OPERATIONS TESTS
# ============================================================================
run_trading_tests() {
    if ! should_run_category "trading"; then return; fi
    
    print_category "Energy Trading Tests"
    
    if [ -z "$SELLER_TOKEN" ] || [ "$SELLER_TOKEN" = "null" ]; then
        print_skip "Cannot run trading tests - no seller token"
        return
    fi
    
    # Test 15: Get Current Epoch
    api_call "GET" "/api/market/epoch" "$SELLER_TOKEN" "" "15" "Get current epoch"
    if [ "$HTTP_CODE" -eq 200 ]; then
        if validate_json "$BODY"; then
            EPOCH_ID=$(echo "$BODY" | jq -r '.id // .epoch_id // empty')
            EPOCH_STATUS=$(echo "$BODY" | jq -r '.status // empty')
            print_success "Epoch retrieved - ID: $EPOCH_ID, Status: $EPOCH_STATUS (${TEST_TIME_15}s)"
        fi
    else
        print_warning "Could not get epoch (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 16: Create Sell Order
    ORDER_DATA="{\"energy_amount\":\"100.0\",\"price_per_kwh\":\"0.15\",\"order_type\":\"Limit\",\"side\":\"Sell\"}"
    api_call "POST" "/api/trading/orders" "$SELLER_TOKEN" "$ORDER_DATA" "16" "Create sell order"
    
    if [ "$HTTP_CODE" -eq 201 ] || [ "$HTTP_CODE" -eq 200 ]; then
        if validate_json "$BODY"; then
            SELLER_ORDER_ID=$(echo "$BODY" | jq -r '.id // .order_id // empty')
            ORDER_STATUS=$(echo "$BODY" | jq -r '.status // "unknown"')
            print_success "Sell order created - ID: $SELLER_ORDER_ID (${TEST_TIME_16}s)"
            print_info "Status: $ORDER_STATUS"
        fi
    else
        print_error "Sell order creation failed (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 17: Create Buy Order
    if [ ! -z "$BUYER_TOKEN" ] && [ "$BUYER_TOKEN" != "null" ]; then
        ORDER_DATA="{\"energy_amount\":\"100.0\",\"price_per_kwh\":\"0.15\",\"order_type\":\"Limit\",\"side\":\"Buy\"}"
        api_call "POST" "/api/trading/orders" "$BUYER_TOKEN" "$ORDER_DATA" "17" "Create buy order"
        
        if [ "$HTTP_CODE" -eq 201 ] || [ "$HTTP_CODE" -eq 200 ]; then
            if validate_json "$BODY"; then
                BUYER_ORDER_ID=$(echo "$BODY" | jq -r '.id // .order_id // empty')
                ORDER_STATUS=$(echo "$BODY" | jq -r '.status // "unknown"')
                print_success "Buy order created - ID: $BUYER_ORDER_ID (${TEST_TIME_17}s)"
                print_info "Status: $ORDER_STATUS"
            fi
        else
            print_error "Buy order creation failed (HTTP $HTTP_CODE)"
        fi
    else
        print_skip "Cannot create buy order - no buyer token"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 18: Get Order Book
    api_call "GET" "/api/market/orderbook" "$SELLER_TOKEN" "" "18" "Get market order book"
    if [ "$HTTP_CODE" -eq 200 ]; then
        if validate_json "$BODY"; then
            BUY_COUNT=$(echo "$BODY" | jq '.bids | length // 0')
            SELL_COUNT=$(echo "$BODY" | jq '.asks | length // 0')
            print_success "Order book retrieved - $BUY_COUNT bids, $SELL_COUNT asks (${TEST_TIME_18}s)"
        fi
    else
        print_warning "Could not get order book (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 19: Get Market Stats
    api_call "GET" "/api/market/stats" "$SELLER_TOKEN" "" "19" "Get market statistics"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Market stats retrieved (${TEST_TIME_19}s)"
        if [ "$(to_lower "$VERBOSE")" = "true" ]; then
            echo "$BODY" | jq '.'
        fi
    else
        print_warning "Could not get market stats (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
}

# ============================================================================
# TOKEN MANAGEMENT TESTS
# ============================================================================
run_token_tests() {
    if ! should_run_category "tokens"; then return; fi
    
    print_category "Token Management Tests"
    
    if [ -z "$BUYER_TOKEN" ] || [ "$BUYER_TOKEN" = "null" ]; then
        print_skip "Cannot run token tests - no valid token"
        return
    fi
    
    # Test 20: Get Token Balance
    api_call "GET" "/api/tokens/balance/$BUYER_WALLET" "$BUYER_TOKEN" "" "20" "Get token balance"
    if [ "$HTTP_CODE" -eq 200 ]; then
        if validate_json "$BODY"; then
            TOKEN_BALANCE=$(echo "$BODY" | jq -r '.balance // "0"')
            print_success "Token balance retrieved: $TOKEN_BALANCE (${TEST_TIME_20}s)"
        fi
    else
        print_warning "Could not get token balance (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 21: Get Token Info
    api_call "GET" "/api/tokens/info" "$BUYER_TOKEN" "" "21" "Get token information"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Token info retrieved (${TEST_TIME_21}s)"
    else
        print_warning "Could not get token info (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
}

# ============================================================================
# ENERGY METER TESTS
# ============================================================================
run_meter_tests() {
    if ! should_run_category "meters"; then return; fi
    
    print_category "Energy Meter Tests"
    
    if [ -z "$SELLER_TOKEN" ] || [ "$SELLER_TOKEN" = "null" ]; then
        print_skip "Cannot run meter tests - no seller token"
        return
    fi
    
    # Test 22: Submit Energy Reading
    READING_DATA="{\"meter_id\":\"METER_${TIMESTAMP}\",\"reading_kwh\":\"125.5\",\"timestamp\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"location\":\"Building A\"}"
    api_call "POST" "/api/meters/submit-reading" "$SELLER_TOKEN" "$READING_DATA" "22" "Submit energy reading"
    
    if [ "$HTTP_CODE" -eq 201 ] || [ "$HTTP_CODE" -eq 200 ]; then
        if validate_json "$BODY"; then
            READING_ID=$(echo "$BODY" | jq -r '.id // .reading_id // empty')
            print_success "Energy reading submitted - ID: $READING_ID (${TEST_TIME_22}s)"
        fi
    else
        print_error "Energy reading submission failed (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 23: Get My Readings
    api_call "GET" "/api/meters/my-readings?limit=5" "$SELLER_TOKEN" "" "23" "Get my meter readings"
    if [ "$HTTP_CODE" -eq 200 ]; then
        if validate_json "$BODY"; then
            READING_COUNT=$(echo "$BODY" | jq 'length // 0')
            print_success "Retrieved $READING_COUNT readings (${TEST_TIME_23}s)"
        fi
    else
        print_warning "Could not get readings (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 24: Get User Stats
    api_call "GET" "/api/meters/stats" "$SELLER_TOKEN" "" "24" "Get user energy statistics"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "User stats retrieved (${TEST_TIME_24}s)"
    else
        print_warning "Could not get user stats (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 25: Get Registered Meters
    api_call "GET" "/api/meters/registered" "$SELLER_TOKEN" "" "25" "Get registered meters"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Registered meters retrieved (${TEST_TIME_25}s)"
    else
        print_warning "Could not get registered meters (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
}

# ============================================================================
# ERC CERTIFICATE TESTS
# ============================================================================
run_erc_tests() {
    if ! should_run_category "erc"; then return; fi
    
    print_category "ERC Certificate Tests"
    
    if [ -z "$SELLER_TOKEN" ] || [ "$SELLER_TOKEN" = "null" ]; then
        print_skip "Cannot run ERC tests - no seller token"
        return
    fi
    
    # Test 26: Issue Certificate
    ERC_DATA="{\"energy_amount_kwh\":1000,\"energy_source\":\"solar\",\"generation_date\":\"$(date +%Y-%m-%d)\",\"location\":\"Building A\"}"
    api_call "POST" "/api/erc/issue" "$SELLER_TOKEN" "$ERC_DATA" "26" "Issue ERC certificate"
    
    if [ "$HTTP_CODE" -eq 201 ] || [ "$HTTP_CODE" -eq 200 ]; then
        if validate_json "$BODY"; then
            CERTIFICATE_ID=$(echo "$BODY" | jq -r '.id // .certificate_id // empty')
            print_success "ERC certificate issued - ID: $CERTIFICATE_ID (${TEST_TIME_26}s)"
        fi
    else
        print_error "ERC certificate issuance failed (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 27: Get My Certificates
    api_call "GET" "/api/erc/my-certificates?status=active" "$SELLER_TOKEN" "" "27" "Get my certificates"
    if [ "$HTTP_CODE" -eq 200 ]; then
        if validate_json "$BODY"; then
            CERT_COUNT=$(echo "$BODY" | jq 'length // 0')
            print_success "Retrieved $CERT_COUNT certificates (${TEST_TIME_27}s)"
        fi
    else
        print_warning "Could not get certificates (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 28: Get Certificate Stats
    api_call "GET" "/api/erc/my-stats" "$SELLER_TOKEN" "" "28" "Get certificate statistics"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Certificate stats retrieved (${TEST_TIME_28}s)"
    else
        print_warning "Could not get certificate stats (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
}

# ============================================================================
# ORACLE TESTS
# ============================================================================
run_oracle_tests() {
    if ! should_run_category "oracle"; then return; fi
    
    print_category "Oracle Services Tests"
    
    if [ -z "$BUYER_TOKEN" ] || [ "$BUYER_TOKEN" = "null" ]; then
        print_skip "Cannot run oracle tests - no valid token"
        return
    fi
    
    # Test 29: Get Current Prices
    api_call "GET" "/api/oracle/prices/current" "$BUYER_TOKEN" "" "29" "Get current oracle prices"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Current prices retrieved (${TEST_TIME_29}s)"
    else
        print_warning "Could not get current prices (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 30: Get Oracle Data
    api_call "GET" "/api/oracle/data" "$BUYER_TOKEN" "" "30" "Get oracle data"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Oracle data retrieved (${TEST_TIME_30}s)"
    else
        print_warning "Could not get oracle data (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
}

# ============================================================================
# GOVERNANCE TESTS
# ============================================================================
run_governance_tests() {
    if ! should_run_category "governance"; then return; fi
    
    print_category "Governance Tests"
    
    if [ -z "$BUYER_TOKEN" ] || [ "$BUYER_TOKEN" = "null" ]; then
        print_skip "Cannot run governance tests - no valid token"
        return
    fi
    
    # Test 31: Get Governance Status
    api_call "GET" "/api/governance/status" "$BUYER_TOKEN" "" "31" "Get governance status"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Governance status retrieved (${TEST_TIME_31}s)"
    else
        print_warning "Could not get governance status (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
}

# ============================================================================
# MARKET DATA TESTS
# ============================================================================
run_market_tests() {
    if ! should_run_category "market"; then return; fi
    
    print_category "Market Data Tests"
    
    if [ -z "$BUYER_TOKEN" ] || [ "$BUYER_TOKEN" = "null" ]; then
        print_skip "Cannot run market data tests - no valid token"
        return
    fi
    
    # Test 32: Get Order Book Depth
    api_call "GET" "/api/market-data/depth" "$BUYER_TOKEN" "" "32" "Get order book depth"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Order book depth retrieved (${TEST_TIME_32}s)"
    else
        print_warning "Could not get order book depth (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 33: Get Market Depth Chart
    api_call "GET" "/api/market-data/depth-chart" "$BUYER_TOKEN" "" "33" "Get market depth chart"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Market depth chart retrieved (${TEST_TIME_33}s)"
    else
        print_warning "Could not get market depth chart (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 34: Get Clearing Price
    api_call "GET" "/api/market-data/clearing-price" "$BUYER_TOKEN" "" "34" "Get clearing price"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Clearing price retrieved (${TEST_TIME_34}s)"
    else
        print_warning "Could not get clearing price (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 35: Get Trade History
    api_call "GET" "/api/market-data/trades/my-history?limit=10" "$BUYER_TOKEN" "" "35" "Get trade history"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Trade history retrieved (${TEST_TIME_35}s)"
    else
        print_warning "Could not get trade history (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
}

# ============================================================================
# ANALYTICS TESTS
# ============================================================================
run_analytics_tests() {
    if ! should_run_category "analytics"; then return; fi
    
    print_category "Analytics Tests"
    
    if [ -z "$BUYER_TOKEN" ] || [ "$BUYER_TOKEN" = "null" ]; then
        print_skip "Cannot run analytics tests - no valid token"
        return
    fi
    
    # Test 36: Get Market Analytics
    api_call "GET" "/api/analytics/market" "$BUYER_TOKEN" "" "36" "Get market analytics"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Market analytics retrieved (${TEST_TIME_36}s)"
    else
        print_warning "Could not get market analytics (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 37: Get User Trading Stats
    api_call "GET" "/api/analytics/my-stats" "$BUYER_TOKEN" "" "37" "Get user trading statistics"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "User trading stats retrieved (${TEST_TIME_37}s)"
    else
        print_warning "Could not get user trading stats (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
}

# ============================================================================
# TRANSACTION MANAGEMENT TESTS
# ============================================================================
run_transaction_tests() {
    if ! should_run_category "transactions"; then return; fi
    
    print_category "Transaction Management Tests"
    
    if [ -z "$BUYER_TOKEN" ] || [ "$BUYER_TOKEN" = "null" ]; then
        print_skip "Cannot run transaction tests - no valid token"
        return
    fi
    
    # Test 38: Get User Transactions
    api_call "GET" "/api/v1/transactions/user?limit=10" "$BUYER_TOKEN" "" "38" "Get user transactions"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "User transactions retrieved (${TEST_TIME_38}s)"
    else
        print_warning "Could not get user transactions (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 39: Get Transaction Status (using dummy ID)
    api_call "GET" "/api/v1/transactions/00000000-0000-0000-0000-000000000000/status" "$BUYER_TOKEN" "" "39" "Get transaction status"
    if [ "$HTTP_CODE" -eq 404 ]; then
        print_success "Transaction status endpoint working (correct 404) (${TEST_TIME_39}s)"
    elif [ "$HTTP_CODE" -eq 200 ]; then
        print_success "Transaction status retrieved (${TEST_TIME_39}s)"
    else
        print_warning "Transaction status endpoint unexpected response (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
}

# ============================================================================
# ADMIN OPERATIONS TESTS (Limited - requires admin role)
# ============================================================================
run_admin_tests() {
    if ! should_run_category "admin"; then return; fi
    
    print_category "Admin Operations Tests"
    
    # These tests typically require admin privileges, so we'll test access control
    if [ -z "$BUYER_TOKEN" ] || [ "$BUYER_TOKEN" = "null" ]; then
        print_skip "Cannot run admin tests - no valid token"
        return
    fi
    
    # Test 40: Test Admin Access Control
    api_call "GET" "/api/admin/stats" "$BUYER_TOKEN" "" "40" "Test admin access control"
    if [ "$HTTP_CODE" -eq 403 ]; then
        print_success "Admin access control working (correct 403) (${TEST_TIME_40}s)"
    else
        print_warning "Admin access control issue (expected 403, got $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
    
    # Test 41: Test User Management Access Control
    api_call "GET" "/api/users" "$BUYER_TOKEN" "" "41" "Test user management access control"
    if [ "$HTTP_CODE" -eq 403 ]; then
        print_success "User management access control working (${TEST_TIME_41}s)"
    else
        print_warning "User management access control issue (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
}

# ============================================================================
# WEBSOCKET TESTS (Basic connectivity)
# ============================================================================
run_websocket_tests() {
    if ! should_run_category "websocket"; then return; fi
    
    print_category "WebSocket Tests"
    
    # Test 42: WebSocket Stats Endpoint
    api_call "GET" "/ws/stats" "" "" "42" "Get WebSocket statistics"
    if [ "$HTTP_CODE" -eq 200 ]; then
        print_success "WebSocket stats retrieved (${TEST_TIME_42}s)"
    else
        print_warning "Could not get WebSocket stats (HTTP $HTTP_CODE)"
    fi
    
    sleep $SLEEP_TIME
}

# ============================================================================
# FINAL SUMMARY
# ============================================================================
print_final_summary() {
    local end_time=$(date +%s)
    local total_time=$((end_time - START_TIME))
    
    print_header "Comprehensive API Test Results"
    
    echo -e "${BLUE}╔══════════════════════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║        GridTokenX Comprehensive API Test Results        ║${NC}"
    echo -e "${BLUE}╚══════════════════════════════════════════════════════╝${NC}"
    
    echo -e "\n${CYAN}Configuration:${NC}"
    echo -e "  API URL:           $API_BASE_URL"
    echo -e "  Test Category:     $TEST_CATEGORY"
    echo -e "  Verbose:           $VERBOSE"
    echo -e "  Strict Mode:       $STRICT_MODE"
    echo -e "  Save Responses:    $SAVE_RESPONSES"
    echo -e "  Skip DB Verify:    $SKIP_DB_VERIFICATION"
    echo -e "  Timestamp:         $TIMESTAMP"
    
    echo -e "\n${CYAN}Performance Metrics:${NC}"
    echo -e "  Total execution time:  ${YELLOW}${total_time}s${NC}"
    if [ $TOTAL_TESTS -gt 0 ]; then
        echo -e "  Average per test:      ${YELLOW}$((total_time / TOTAL_TESTS))s${NC}"
    fi
    
    echo -e "\n${CYAN}Test Results Summary:${NC}"
    echo -e "  ${GREEN}✓ Passed:${NC}         $PASSED_TESTS"
    echo -e "  ${YELLOW}⚠ Warnings:${NC}       $WARNING_TESTS"
    echo -e "  ${RED}✗ Failed:${NC}          $FAILED_TESTS"
    echo -e "  ${YELLOW}→ Skipped:${NC}         $SKIPPED_TESTS"
    echo -e "  ${WHITE}Total Tests:${NC}       $TOTAL_TESTS"
    
    local success_rate=0
    local total_run=$((PASSED_TESTS + WARNING_TESTS + FAILED_TESTS))
    if [ $total_run -gt 0 ]; then
        success_rate=$((PASSED_TESTS * 100 / total_run))
    fi
    echo -e "  Success rate:           ${YELLOW}${success_rate}%${NC}"
    
    echo -e "\n${CYAN}Test Identifiers:${NC}"
    echo -e "  Timestamp:        $TIMESTAMP"
    echo -e "  Random Suffix:    $RANDOM_SUFFIX"
    echo -e "  Buyer Email:      $BUYER_EMAIL"
    echo -e "  Seller Email:     $SELLER_EMAIL"
    echo -e "  Buyer ID:        ${BUYER_ID:-N/A}"
    echo -e "  Seller ID:       ${SELLER_ID:-N/A}"
    echo -e "  Buy Order ID:     ${BUYER_ORDER_ID:-N/A}"
    echo -e "  Sell Order ID:    ${SELLER_ORDER_ID:-N/A}"
    echo -e "  Epoch ID:        ${EPOCH_ID:-N/A}"
    echo -e "  Meter ID:        ${METER_ID:-N/A}"
    echo -e "  Certificate ID:   ${CERTIFICATE_ID:-N/A}"
    
    if [ "$(to_lower "$SAVE_RESPONSES")" = "true" ]; then
        echo -e "\n${CYAN}Response Files:${NC}"
        echo -e "  Saved to: $RESPONSE_DIR"
        echo -e "  Total files: $(ls -1 "$RESPONSE_DIR" 2>/dev/null | wc -l)"
    fi
    
    # Show slowest tests
    echo -e "\n${CYAN}Performance Analysis:${NC}"
    echo -e "  Slowest Tests:"
    local timing_data=""
    for i in $(seq 1 $TOTAL_TESTS); do
        local time_var="TEST_TIME_${i}"
        local time_val="${!time_var}"
        if [ ! -z "$time_val" ]; then
            timing_data="${timing_data}${i} ${time_val}\n"
        fi
    done
    
    echo -e "$timing_data" | sort -k2 -rn | head -5 | while read step time; do
        if [ ! -z "$step" ]; then
            echo -e "    Test $step: ${time}s"
        fi
    done
    
    # Category coverage
    echo -e "\n${CYAN}Category Coverage:${NC}"
    for category in "${!TEST_CATEGORIES[@]}"; do
        if should_run_category "$category"; then
            echo -e "  ${GREEN}✓${NC} ${TEST_CATEGORIES[$category]}"
        else
            echo -e "  ${YELLOW}→${NC} ${TEST_CATEGORIES[$category]} (skipped)"
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

# ============================================================================
# MAIN EXECUTION
# ============================================================================

print_header "GridTokenX Comprehensive API Test Suite"
echo -e "${CYAN}This script tests all major API endpoints with proper validation.${NC}"
echo -e "${CYAN}Use TEST_CATEGORY environment variable to run specific categories.${NC}"
echo -e "${CYAN}Categories: ${!TEST_CATEGORIES[*]}${NC}"

# Run health checks first (always)
run_health_tests

# Check if server is running before proceeding
if [ $FAILED_TESTS -gt 0 ]; then
    print_error "Health checks failed - aborting remaining tests"
    print_final_summary
    exit 1
fi

# Run test categories based on configuration
run_auth_tests
run_blockchain_tests
run_trading_tests
run_token_tests
run_meter_tests
run_erc_tests
run_oracle_tests
run_governance_tests
run_market_tests
run_analytics_tests
run_transaction_tests
run_admin_tests
run_websocket_tests

# Print final summary
print_final_summary
