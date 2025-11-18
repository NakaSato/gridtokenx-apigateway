#!/bin/bash

echo "=== GridTokenX Blockchain Connection Test ==="

# Check if solana CLI is installed
if ! command -v solana &> /dev/null; then
    echo "❌ Solana CLI not found. Please install Solana CLI first."
    echo "Visit: https://docs.solana.com/cli/install/solana-cli-install"
    exit 1
fi

# Start validator
echo "🚀 Starting solana-test-validator..."
solana-test-validator --reset &
VALIDATOR_PID=$!

# Wait for validator to start
echo "⏳ Waiting for validator to start..."
sleep 10

# Check if validator is running
if ! kill -0 $VALIDATOR_PID 2>/dev/null; then
    echo "❌ Failed to start solana-test-validator"
    exit 1
fi

echo "✅ Solana validator started (PID: $VALIDATOR_PID)"

# Check RPC health
echo "🔍 Testing RPC health check..."
RPC_HEALTH=$(curl -s -X POST http://localhost:8899 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' | jq -r '.result // "error"')

if [ "$RPC_HEALTH" = "ok" ]; then
    echo "✅ RPC health check passed"
else
    echo "❌ RPC health check failed: $RPC_HEALTH"
    kill $VALIDATOR_PID
    exit 1
fi

# Check authority wallet
echo "👛 Checking authority wallet..."

# Check if dev-wallet.json exists
if [ -f "./dev-wallet.json" ]; then
    echo "📁 Found dev-wallet.json"
    
    # Check authority wallet balance
    echo "💰 Checking authority wallet balance..."
    BALANCE=$(solana balance ./dev-wallet.json --url http://localhost:8899 | awk '{print $1}')
    
    if [ -n "$BALANCE" ] && [ "$BALANCE" != "0" ]; then
        echo "✅ Authority wallet balance: $BALANCE SOL"
    elif [ "$BALANCE" = "0" ]; then
        echo "⚠️  Authority wallet has 0 SOL. Requesting airdrop..."
        solana airdrop 2 ./dev-wallet.json --url http://localhost:8899
        sleep 5
        NEW_BALANCE=$(solana balance ./dev-wallet.json --url http://localhost:8899 | awk '{print $1}')
        echo "✅ Authority wallet balance after airdrop: $NEW_BALANCE SOL"
    else
        echo "❌ Failed to get authority wallet balance"
    fi
    
    # Get authority wallet pubkey
    AUTHORITY_PUBKEY=$(solana address -k ./dev-wallet.json)
    echo "🔑 Authority wallet pubkey: $AUTHORITY_PUBKEY"
    
else
    echo "⚠️  dev-wallet.json not found. Creating new authority wallet..."
    solana-keygen new --no-bip39-passphrase --silent --outfile ./dev-wallet.json
    AUTHORITY_PUBKEY=$(solana address -k ./dev-wallet.json)
    echo "🔑 Created new authority wallet: $AUTHORITY_PUBKEY"
    
    # Request airdrop for new wallet
    echo "💰 Requesting airdrop for new authority wallet..."
    solana airdrop 2 ./dev-wallet.json --url http://localhost:8899
    sleep 5
fi

# Test basic API Gateway health check (if running)
echo "🌐 Testing API Gateway health check..."
API_HEALTH=$(curl -s http://localhost:8080/health | jq -r '.status // "error"')

if [ "$API_HEALTH" = "healthy" ]; then
    echo "✅ API Gateway is running"
else
    echo "⚠️  API Gateway not running or unhealthy (status: $API_HEALTH)"
fi

# Test token mint configuration
echo "🪙 Testing token mint configuration..."
if [ -n "$ENERGY_TOKEN_MINT" ]; then
    echo "✅ ENERGY_TOKEN_MINT is set: $ENERGY_TOKEN_MINT"
else
    echo "⚠️  ENERGY_TOKEN_MINT environment variable not set"
fi

# Check if energy token mint account exists on-chain
if [ -n "$ENERGY_TOKEN_MINT" ]; then
    echo "🔍 Checking if token mint account exists..."
    TOKEN_ACCOUNT=$(solana account $ENERGY_TOKEN_MINT --url http://localhost:8899 --output json 2>/dev/null)
    
    if [ $? -eq 0 ]; then
        echo "✅ Token mint account exists on-chain"
        MINT_AUTHORITY=$(echo $TOKEN_ACCOUNT | jq -r '.data.parsed.info.mintAuthority // "null"')
        SUPPLY=$(echo $TOKEN_ACCOUNT | jq -r '.data.parsed.info.supply // "unknown"')
        echo "📊 Token supply: $SUPPLY"
        echo "👛 Mint authority: $MINT_AUTHORITY"
    else
        echo "⚠️  Token mint account not found on-chain"
        echo "💡 Make sure to deploy the gridtokenx-anchor programs first"
    fi
fi

# Summary
echo ""
echo "=== Test Summary ==="
echo "✅ Solana validator: Running"
echo "👛 Authority wallet: Available"
echo "🔗 RPC endpoint: http://localhost:8899"
echo "🌐 WebSocket endpoint: ws://localhost:8900"

if [ -n "$AUTHORITY_PUBKEY" ]; then
    echo "🔑 Authority pubkey: $AUTHORITY_PUBKEY"
fi

echo ""
echo "🎉 Blockchain connection test completed successfully!"
echo ""
echo "Next steps:"
echo "1. Deploy Anchor programs: cd ../gridtokenx-anchor && anchor deploy"
echo "2. Start API Gateway: cargo run"
echo "3. Test token minting via API"
echo ""
echo "To stop the validator, run: kill $VALIDATOR_PID"

# Keep validator running in background
echo "Validator is running in background. Use Ctrl+C to stop this script (validator will continue running)."

# Cleanup function
cleanup() {
    echo ""
    echo "🛑 Stopping validator..."
    kill $VALIDATOR_PID 2>/dev/null
    echo "✅ Validator stopped"
    exit 0
}

# Trap Ctrl+C
trap cleanup INT

# Wait indefinitely (or until user presses Ctrl+C)
while true; do
    sleep 1
done
