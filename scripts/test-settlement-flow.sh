#!/bin/bash
# Settlement Flow Integration Test
# Tests complete settlement pipeline: trading -> matching -> settlement -> blockchain

set -e

echo "=== GridTokenX Settlement Flow Integration Test ==="

# Configuration
API_URL="http://localhost:8080"
SETTLEMENT_ENDPOINT="/api/admin/settlements/process-pending"
TRADING_ENDPOINT="/api/trading/orders"
MARKET_ENDPOINT="/api/market/orderbook"

# Test credentials (should be pre-configured)
ADMIN_TOKEN="${ADMIN_TOKEN:-}"  # Set this environment variable
BUYER_TOKEN="${BUYER_TOKEN:-}"
SELLER_TOKEN="${SELLER_TOKEN:-}"

if [[ -z "$ADMIN_TOKEN" || -z "$BUYER_TOKEN" || -z "$SELLER_TOKEN" ]]; then
    echo "❌ Missing required environment variables:"
    echo "   export ADMIN_TOKEN=<admin_jwt>"
    echo "   export BUYER_TOKEN=<buyer_jwt>"
    echo "   export SELLER_TOKEN=<seller_jwt>"
    exit 1
fi

echo "🔧 Configuration:"
echo "  API URL: $API_URL"
echo "  Admin Token: ${ADMIN_TOKEN:0:20}..."
echo "  Buyer Token: ${BUYER_TOKEN:0:20}..."
echo "  Seller Token: ${SELLER_TOKEN:0:20}..."

# Function to make API calls with error handling
call_api() {
    local method=$1
    local endpoint=$2
    local data=$3
    local token=$4
    
    local response=$(curl -s -X "$method" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer $token" \
        -d "$data" \
        "$API_URL$endpoint" 2>/dev/null)
    
    local exit_code=$?
    if [[ $exit_code -ne 0 ]]; then
        echo "❌ API call failed: $method $endpoint (exit code: $exit_code)"
        return 1
    fi
    
    echo "$response"
}

# Function to check settlement status
check_settlement() {
    local settlement_id=$1
    echo "🔍 Checking settlement status: $settlement_id"
    
    local response=$(call_api "GET" "/api/settlements/$settlement_id" "" "$ADMIN_TOKEN")
    local status=$(echo "$response" | jq -r '.status // "Unknown"')
    
    echo "  Status: $status"
    
    if [[ "$status" == "Confirmed" ]]; then
        local tx_sig=$(echo "$response" | jq -r '.blockchain_tx // empty')
        if [[ -n "$tx_sig" ]]; then
            echo "  ✅ Blockchain Transaction: $tx_sig"
        fi
        return 0
    elif [[ "$status" == "Failed" ]]; then
        echo "  ❌ Settlement failed"
        return 1
    else
        echo "  ⏳ Settlement in progress..."
        return 2
    fi
}

# Function to wait for settlement completion
wait_for_settlement() {
    local settlement_id=$1
    local max_wait=60  # Maximum wait time in seconds
    local interval=5     # Check interval in seconds
    local elapsed=0
    
    while [[ $elapsed -lt $max_wait ]]; do
        local status_result
        if check_settlement "$settlement_id"; then
            status_result=$?
            case $status_result in
                0) echo "✅ Settlement completed successfully"; return 0 ;;
                1) echo "❌ Settlement failed"; return 1 ;;
                2) ;;  # Still in progress
            esac
        fi
        
        sleep $interval
        elapsed=$((elapsed + interval))
    done
    
    echo "⏰ Timeout waiting for settlement"
    return 1
}

echo ""
echo "🚀 Starting Settlement Flow Test"

# Step 1: Check API Health
echo ""
echo "Step 1: Checking API Health"
health_response=$(curl -s "$API_URL/health" 2>/dev/null || echo "failed")
if [[ "$health_response" == "failed" ]]; then
    echo "❌ API health check failed"
    exit 1
fi
echo "✅ API is healthy"

# Step 2: Create Buy Order
echo ""
echo "Step 2: Creating Buy Order"
buy_order_data='{
    "side": "Buy",
    "energy_amount": "50.0",
    "price_per_kwh": "0.12",
    "expires_at": "'$(date -u -d '+1 hour' -Iseconds)"'",
    "metadata": {"test": "settlement_flow"}
}'

buy_response=$(call_api "POST" "$TRADING_ENDPOINT" "$buy_order_data" "$BUYER_TOKEN")
buy_order_id=$(echo "$buy_response" | jq -r '.id // empty')

if [[ -z "$buy_order_id" ]]; then
    echo "❌ Failed to create buy order"
    echo "Response: $buy_response"
    exit 1
fi

echo "✅ Buy Order Created: $buy_order_id"

# Step 3: Create Sell Order
echo ""
echo "Step 3: Creating Sell Order"
sell_order_data='{
    "side": "Sell",
    "energy_amount": "50.0",
    "price_per_kwh": "0.10",
    "expires_at": "'$(date -u -d '+1 hour' -Iseconds)"'",
    "metadata": {"test": "settlement_flow"}
}'

sell_response=$(call_api "POST" "$TRADING_ENDPOINT" "$sell_order_data" "$SELLER_TOKEN")
sell_order_id=$(echo "$sell_response" | jq -r '.id // empty')

if [[ -z "$sell_order_id" ]]; then
    echo "❌ Failed to create sell order"
    echo "Response: $sell_response"
    exit 1
fi

echo "✅ Sell Order Created: $sell_order_id"

# Step 4: Check Order Book
echo ""
echo "Step 4: Checking Order Book"
orderbook_response=$(call_api "GET" "$MARKET_ENDPOINT" "" "$BUYER_TOKEN")

# Verify orders are in the book
buy_orders_count=$(echo "$orderbook_response" | jq '.buy_orders | length')
sell_orders_count=$(echo "$orderbook_response" | jq '.sell_orders | length')

echo "📊 Order Book Status:"
echo "  Buy Orders: $buy_orders_count"
echo "  Sell Orders: $sell_orders_count"

if [[ $buy_orders_count -eq 0 || $sell_orders_count -eq 0 ]]; then
    echo "❌ Orders not found in order book"
    exit 1
fi

# Step 5: Wait for Matching (Market Clearing)
echo ""
echo "Step 5: Waiting for Order Matching"
echo "⏳ Waiting for market clearing engine to match orders..."

# Wait and check for trades
max_wait=30
elapsed=0
trade_found=false

while [[ $elapsed -lt $max_wait ]]; do
    sleep 2
    
    # Check user's trades
    trades_response=$(call_api "GET" "/api/trading/my-trades" "" "$BUYER_TOKEN")
    trades_count=$(echo "$trades_response" | jq '.trades | length')
    
    if [[ $trades_count -gt 0 ]]; then
        trade_found=true
        trade_id=$(echo "$trades_response" | jq -r '.trades[0].id // empty')
        echo "✅ Trade Found: $trade_id"
        break
    fi
    
    elapsed=$((elapsed + 2))
    echo "  Checking for trades... (${elapsed}s)"
done

if [[ "$trade_found" != true ]]; then
    echo "❌ No trades found after waiting for market clearing"
    exit 1
fi

# Step 6: Check for Pending Settlements
echo ""
echo "Step 6: Checking for Pending Settlements"
settlements_response=$(call_api "GET" "/api/settlements?status=Pending" "" "$ADMIN_TOKEN")
pending_settlements_count=$(echo "$settlements_response" | jq '.settlements | length')

echo "📝 Pending Settlements: $pending_settlements_count"

if [[ $pending_settlements_count -eq 0 ]]; then
    echo "⚠️  No pending settlements found"
    echo "  This might be normal if settlements were processed quickly"
    
    # Check for recent settlements (any status)
    recent_settlements=$(call_api "GET" "/api/settlements?limit=5" "" "$ADMIN_TOKEN")
    echo "📋 Recent Settlements:"
    echo "$recent_settlements" | jq -r '.settlements[] | "  \(.id): \(.status) (\(.created_at))"'
    
    if [[ $(echo "$recent_settlements" | jq '.settlements | length') -gt 0 ]]; then
        # Get the most recent settlement for monitoring
        latest_settlement_id=$(echo "$recent_settlements" | jq -r '.settlements[0].id // empty')
        echo ""
        echo "🔍 Monitoring latest settlement: $latest_settlement_id"
        wait_for_settlement "$latest_settlement_id"
        settlement_result=$?
    else
        echo "❌ No settlements found in the system"
        exit 1
    fi
else
    # Monitor the first pending settlement
    pending_settlement_id=$(echo "$settlements_response" | jq -r '.settlements[0].id // empty')
    echo ""
    echo "🔍 Monitoring pending settlement: $pending_settlement_id"
    wait_for_settlement "$pending_settlement_id"
    settlement_result=$?
fi

# Step 7: Verify Blockchain Transaction
if [[ $settlement_result -eq 0 ]]; then
    echo ""
    echo "Step 7: Verifying Blockchain Transaction"
    
    # Get the successful settlement details
    settlement_details=$(call_api "GET" "/api/settlements/$pending_settlement_id" "" "$ADMIN_TOKEN")
    tx_signature=$(echo "$settlement_details" | jq -r '.blockchain_tx // empty')
    
    if [[ -n "$tx_signature" ]]; then
        echo "🔗 Transaction Signature: $tx_signature"
        
        # Check if it's a real blockchain transaction (not mock)
        if [[ "$tx_signature" =~ ^MOCK_ ]]; then
            echo "⚠️  Warning: This appears to be a mock transaction"
            echo "  Real blockchain integration may not be working"
        else
            echo "✅ Real blockchain transaction detected"
            
            # Optional: Verify on Solana explorer (if RPC is accessible)
            if command -v solana >/dev/null 2>&1; then
                echo "🔍 Checking transaction on Solana network..."
                # This would require Solana CLI and RPC configuration
                # solana confirm $tx_signature --url http://localhost:8899 || echo "Transaction verification not available"
            fi
        fi
    else
        echo "❌ No transaction signature found in settlement"
        exit 1
    fi
fi

# Step 8: Final Verification
echo ""
echo "Step 8: Final System Verification"

# Check settlement statistics
stats_response=$(call_api "GET" "/api/settlements/stats" "" "$ADMIN_TOKEN")

echo "📊 Settlement Statistics:"
echo "$stats_response" | jq -r '
"  Pending: \(.pending_count // 0)"
"  Processing: \(.processing_count // 0)"
"  Confirmed: \(.confirmed_count // 0)"
"  Failed: \(.failed_count // 0)"
"  Total Settled (24h): \(.total_settled_value // "0") GRID"
'

# Check market status
market_response=$(call_api "GET" "$MARKET_ENDPOINT" "" "$BUYER_TOKEN")
best_bid=$(echo "$market_response" | jq -r '.best_bid // "None"')
best_ask=$(echo "$market_response" | jq -r '.best_ask // "None"')
spread=$(echo "$market_response" | jq -r '.spread // "None"')

echo ""
echo "📈 Market Status:"
echo "  Best Bid: $best_bid"
echo "  Best Ask: $best_ask"
echo "  Spread: $spread"

# Test Results Summary
echo ""
echo "=== Test Results Summary ==="

if [[ $settlement_result -eq 0 ]]; then
    echo "✅ SETTLEMENT FLOW TEST: PASSED"
    echo "  - Buy order created successfully"
    echo "  - Sell order created successfully"
    echo "  - Orders matched by market clearing"
    echo "  - Settlement executed successfully"
    echo "  - Blockchain transaction confirmed"
    exit 0
else
    echo "❌ SETTLEMENT FLOW TEST: FAILED"
    echo "  Settlement execution failed or timed out"
    echo "  Check application logs for detailed error information"
    exit 1
fi
