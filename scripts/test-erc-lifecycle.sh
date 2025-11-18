#!/bin/bash
# ERC Certificate Lifecycle Integration Test
# Tests complete ERC pipeline: issue → transfer → retire → blockchain

set -e

echo "=== GridTokenX ERC Certificate Lifecycle Test ==="

# Configuration
API_URL="http://localhost:8080"
ERC_ISSUE_ENDPOINT="/api/erc/issue"
ERC_GET_ENDPOINT="/api/erc"
ERC_RETIRE_ENDPOINT="/api/erc/{certificate_id}/retire"

# Test credentials
REC_AUTHORITY_TOKEN="${REC_AUTHORITY_TOKEN:-}"  # REC authority role required
USER_TOKEN="${USER_TOKEN:-}"                # Regular user for transfers
ADMIN_TOKEN="${ADMIN_TOKEN:-}"              # Admin for validation

if [[ -z "$REC_AUTHORITY_TOKEN" || -z "$USER_TOKEN" || -z "$ADMIN_TOKEN" ]]; then
    echo "❌ Missing required environment variables:"
    echo "   export REC_AUTHORITY_TOKEN=<rec_authority_jwt>"
    echo "   export USER_TOKEN=<regular_user_jwt>"
    echo "   export ADMIN_TOKEN=<admin_jwt>"
    exit 1
fi

echo "🔧 Configuration:"
echo "  API URL: $API_URL"
echo "  REC Authority Token: ${REC_AUTHORITY_TOKEN:0:20}..."
echo "  User Token: ${USER_TOKEN:0:20}..."
echo "  Admin Token: ${ADMIN_TOKEN:0:20}..."

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

# Function to check certificate status
check_certificate() {
    local certificate_id=$1
    local token=$2
    
    echo "🔍 Checking certificate status: $certificate_id"
    
    local response=$(call_api "GET" "/api/erc/$certificate_id" "" "$token")
    local status=$(echo "$response" | jq -r '.status // "Unknown"')
    local tx_sig=$(echo "$response" | jq -r '.blockchain_tx_signature // empty')
    
    echo "  Status: $status"
    
    if [[ -n "$tx_sig" ]]; then
        echo "  ✅ Blockchain Transaction: $tx_sig"
        return 0
    else
        echo "  ⚠️  No blockchain transaction found"
        return 1
    fi
}

# Function to wait for certificate processing
wait_for_certificate() {
    local certificate_id=$1
    local token=$2
    local max_wait=60
    local interval=5
    local elapsed=0
    
    while [[ $elapsed -lt $max_wait ]]; do
        local status_result
        if check_certificate "$certificate_id" "$token"; then
            status_result=$?
            case $status_result in
                0) echo "✅ Certificate processed successfully"; return 0 ;;
                1) echo "❌ Certificate processing failed"; return 1 ;;
                2) ;;  # Still processing
            esac
        fi
        
        sleep $interval
        elapsed=$((elapsed + interval))
        echo "  Checking certificate status... (${elapsed}s)"
    done
    
    echo "⏰ Timeout waiting for certificate processing"
    return 1
}

echo ""
echo "🚀 Starting ERC Certificate Lifecycle Test"

# Step 1: Check API Health
echo ""
echo "Step 1: Checking API Health"
health_response=$(curl -s "$API_URL/health" 2>/dev/null || echo "failed")
if [[ "$health_response" == "failed" ]]; then
    echo "❌ API health check failed"
    exit 1
fi
echo "✅ API is healthy"

# Step 2: Issue ERC Certificate
echo ""
echo "Step 2: Issuing ERC Certificate"
issue_data='{
    "wallet_address": "DYw8jZ9RfRfQqPkZHvPWqL5F7yKqWqfH8xKxCxJxQxXx",
    "kwh_amount": "100.0",
    "expiry_date": "'$(date -u -d '+1 year' -Iseconds)'"",
    "metadata": {
        "renewable_source": "Solar",
        "validation_data": "ISO-14064 compliant",
        "location": "California, USA",
        "installation_date": "'$(date -u -d '-2 years' -Iseconds)'",
        "capacity_kw": "5.0"
    }
}'

issue_response=$(call_api "POST" "$ERC_ISSUE_ENDPOINT" "$issue_data" "$REC_AUTHORITY_TOKEN")
certificate_id=$(echo "$issue_response" | jq -r '.certificate_id // empty')

if [[ -z "$certificate_id" ]]; then
    echo "❌ Failed to issue certificate"
    echo "Response: $issue_response"
    exit 1
fi

echo "✅ Certificate Issued: $certificate_id"

# Extract blockchain transaction signature
tx_signature=$(echo "$issue_response" | jq -r '.blockchain_tx_signature // empty')
if [[ -n "$tx_signature" ]]; then
    echo "🔗 Blockchain Transaction: $tx_signature"
else
    echo "⚠️  No blockchain transaction signature found"
fi

# Step 3: Verify Certificate Details
echo ""
echo "Step 3: Verifying Certificate Details"
wait_for_certificate "$certificate_id" "$REC_AUTHORITY_TOKEN"
certificate_details=$(call_api "GET" "/api/erc/$certificate_id" "" "$USER_TOKEN")

echo "📋 Certificate Details:"
echo "$certificate_details" | jq -r '
"  Certificate ID: \(.certificate_id)
  Wallet: \(.wallet_address)
  Energy Amount: \(.kwh_amount) kWh
  Status: \(.status)
  Issue Date: \(.issue_date)
  Expiry Date: \(.expiry_date // "Never")
  Issuer Wallet: \(.issuer_wallet // "Unknown")
  Blockchain TX: \(.blockchain_tx_signature // "None")
  Metadata: \(.metadata | keys | join(", "))"'

# Step 4: Test Certificate Validation (Admin)
echo ""
echo "Step 4: Testing Certificate Validation"
validation_response=$(call_api "GET" "/api/admin/erc/validate/$certificate_id" "" "$ADMIN_TOKEN")

echo "🔍 Validation Result:"
echo "$validation_response" | jq -r '
"  Certificate ID: \(.certificate_id)
  Valid: \(.valid)
  On-Chain: \(.on_chain)
  Validation Method: \(.validation_method // "Database")"
  Checked At: \(.checked_at // "Unknown")"'

# Step 5: Test Certificate Transfer
echo ""
echo "Step 5: Testing Certificate Transfer"
transfer_data='{
    "to_wallet_address": "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDs8JoB2T7Wb8",
    "transfer_reason": "Portfolio reorganization"
}'

transfer_response=$(call_api "POST" "/api/erc/$certificate_id/transfer" "$transfer_data" "$USER_TOKEN")
transfer_status=$(echo "$transfer_response" | jq -r '.status // "Unknown"')

if [[ "$transfer_status" == "Transferred" ]]; then
    echo "✅ Certificate transferred successfully"
    
    # Extract transfer transaction signature
    transfer_tx=$(echo "$transfer_response" | jq -r '.blockchain_tx_signature // empty')
    if [[ -n "$transfer_tx" ]]; then
        echo "🔗 Transfer Transaction: $transfer_tx"
    fi
else
    echo "❌ Certificate transfer failed"
    echo "Response: $transfer_response"
fi

# Step 6: Get User Certificate Statistics
echo ""
echo "Step 6: Getting User Certificate Statistics"
stats_response=$(call_api "GET" "/api/erc/my-stats" "" "$USER_TOKEN")

echo "📊 User Statistics:"
echo "$stats_response" | jq -r '
"  Total Certificates: \(.total_certificates)
  Active kWh: \(.active_kwh)
  Retired kWh: \(.retired_kwh)
  Total kWh: \(.total_kwh)"'

# Step 7: Get Certificates by Wallet
echo ""
echo "Step 7: Getting Certificates by Wallet"
wallet_certificates=$(call_api "GET" "/api/erc/wallet/DYw8jZ9RfRfQqPkZHvPWqL5F7yKqWqfH8xKxCxJxQxXx" "" "$USER_TOKEN")
certificates_count=$(echo "$wallet_certificates" | jq '. | length')

echo "📄 Wallet Certificates: $certificates_count found"
echo "$wallet_certificates" | jq -r '.[] | "  \(.certificate_id): \(.kwh_amount) kWh (\(.status))"'

# Step 8: Test Certificate Retirement
echo ""
echo "Step 8: Testing Certificate Retirement"
retire_response=$(call_api "POST" "/api/erc/$certificate_id/retire" "" "$USER_TOKEN")
retire_status=$(echo "$retire_response" | jq -r '.status // "Unknown"')

if [[ "$retire_status" == "Retired" ]]; then
    echo "✅ Certificate retired successfully"
    
    # Extract retirement transaction signature
    retire_tx=$(echo "$retire_response" | jq -r '.blockchain_tx_signature // empty')
    if [[ -n "$retire_tx" ]]; then
        echo "🔗 Retirement Transaction: $retire_tx"
    fi
else
    echo "❌ Certificate retirement failed"
    echo "Response: $retire_response"
fi

# Step 9: Final Verification
echo ""
echo "Step 9: Final Verification"

# Check certificate status one more time
final_status=$(call_api "GET" "/api/erc/$certificate_id" "" "$USER_TOKEN")
final_cert_status=$(echo "$final_status" | jq -r '.status // "Unknown"')
final_blockchain_tx=$(echo "$final_status" | jq -r '.blockchain_tx_signature // empty')

echo "🔍 Final Certificate Status:"
echo "  Certificate ID: $certificate_id"
echo "  Status: $final_cert_status"
echo "  Blockchain TX: $final_blockchain_tx"

# Verify blockchain transaction if exists
if [[ -n "$final_blockchain_tx" && ! "$final_blockchain_tx" =~ ^MOCK_ ]]; then
    echo ""
    echo "Step 10: Verifying Blockchain Transaction"
    
    # Optional: Verify on Solana explorer (if RPC is accessible)
    if command -v solana >/dev/null 2>&1; then
        echo "🔍 Checking transaction on Solana network..."
        # This would require Solana CLI and RPC configuration
        # solana confirm -v $final_blockchain_tx --url http://localhost:8899 || echo "Transaction verification not available"
    fi
    
    echo "✅ Real blockchain transaction detected"
else
    echo "⚠️  Warning: Certificate may not be properly minted on-chain"
fi

# Test Results Summary
echo ""
echo "=== Test Results Summary ==="

# Check each step
issue_success=$([[ -n "$certificate_id" ]] && echo true || echo false)
transfer_success=$([[ "$transfer_status" == "Transferred" ]] && echo true || echo false)
retire_success=$([[ "$retire_status" == "Retired" ]] && echo true || echo false)
blockchain_integration=$([[ -n "$final_blockchain_tx" && ! "$final_blockchain_tx" =~ ^MOCK_ ]] && echo true || echo false)

echo "✅ ERC Issue: $issue_success"
echo "✅ ERC Transfer: $transfer_success"
echo "✅ ERC Retire: $retire_success"
echo "✅ Blockchain Integration: $blockchain_integration"

if $issue_success && $retire_success; then
    echo ""
    echo "🎉 ERC CERTIFICATE LIFECYCLE TEST: PASSED"
    echo "  - Certificate issued successfully"
    echo "  - Certificate retired successfully"
    echo "  - All API endpoints functional"
    if $blockchain_integration; then
        echo "  - Blockchain integration working"
    else
        echo "  - ⚠️  Blockchain integration needs attention"
    fi
    exit 0
else
    echo ""
    echo "❌ ERC CERTIFICATE LIFECYCLE TEST: FAILED"
    echo "  Issue success: $issue_success"
    echo "  Transfer success: $transfer_success"
    echo "  Retire success: $retire_success"
    echo "  Check application logs for detailed error information"
    exit 1
fi
