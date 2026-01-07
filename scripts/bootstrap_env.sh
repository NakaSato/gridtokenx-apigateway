#!/bin/bash
set -e

# Configuration
RPC_URL="http://localhost:8899"
WALLET="dev-wallet.json"
ANCHOR_DIR="../gridtokenx-anchor"

echo "🛑 Stopping existing services..."
pkill -9 api-gateway || true
pkill -9 solana-test-validator || true
sleep 2

echo "🚀 Starting Solana Test Validator with pre-loaded programs..."
rm -rf test-ledger
solana-test-validator --reset --quiet \
  --bpf-program HaT3koMseafcCB9aUQUCrSLMDfN1km7Xik9UhZSG9UV6 $ANCHOR_DIR/target/deploy/energy_token.so \
  --bpf-program 8gHn9oeYcUQgNrMi8fNYGyMCKJTMwM6K413f41AANFt4 $ANCHOR_DIR/target/deploy/trading.so \
  --bpf-program CVS6pz2qdEmjusHCmiwe2R21KVrSoGubdEy5d766KooN $ANCHOR_DIR/target/deploy/registry.so \
  --bpf-program GAZQm4bHUyNhSYrAq5noBohXcTaf6dKZNDKju8499e6w $ANCHOR_DIR/target/deploy/governance.so \
  --bpf-program 3hSEt5vVzbiMCegFnhdMpFGkXEDY8BinrPb8egJoS7C7 $ANCHOR_DIR/target/deploy/oracle.so > /dev/null 2>&1 &

echo "⏳ Waiting for validator to start (20s)..."
for i in {1..20}; do
    if curl -s -X POST -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' $RPC_URL | grep -q 'ok'; then
        echo "✅ Validator is healthy!"
        break
    fi
    sleep 1
    if [ $i -eq 20 ]; then
        echo "❌ Validator failed to start in 20s"
        exit 1
    fi
done

echo "💰 Funding Authority Wallet..."
AUTH_PUB=$(solana-keygen pubkey $WALLET)
# Airdrop and wait for confirmation
solana airdrop 10 $AUTH_PUB --url $RPC_URL --commitment confirmed
# Verify balance before proceeding
BALANCE=$(solana balance $AUTH_PUB --url $RPC_URL)
echo "✅ Authority Balance: $BALANCE"
if [[ "$BALANCE" == "0 SOL" ]]; then
    echo "❌ Airdrop failed to reflect!"
    exit 1
fi

echo "🛠️ Initializing Programs (Market & Token)..."
cargo run --example init_programs

echo "🪙 Creating SPL Token Mint (Token-2022)..."
# create-token will fail if it can't find a config, so we explicitly provide fee-payer
NEW_MINT=$(spl-token create-token --program-id TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb --decimals 9 --fee-payer $WALLET --mint-authority $WALLET --url $RPC_URL | grep "Address:" | awk '{print $NF}')

if [ -z "$NEW_MINT" ]; then
    echo "❌ Failed to create token mint!"
    exit 1
fi
echo "✅ New Mint Created: $NEW_MINT"

echo "📝 Updating .env with new mint..."
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS sed requires empty string for -i or different syntax
    sed -i '' "s/ENERGY_TOKEN_MINT=.*/ENERGY_TOKEN_MINT=$NEW_MINT/" .env
else
    sed -i "s/ENERGY_TOKEN_MINT=.*/ENERGY_TOKEN_MINT=$NEW_MINT/" .env
fi

echo "💵 Creating Currency Token Mint (USDC Mock)..."
CURRENCY_MINT=$(spl-token create-token --program-id TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb --decimals 6 --fee-payer $WALLET --mint-authority $WALLET --url $RPC_URL | grep "Address:" | awk '{print $NF}')
echo "✅ Currency Mint: $CURRENCY_MINT"

echo "📝 Updating .env with currency mint..."
if [[ "$OSTYPE" == "darwin"* ]]; then
    # Check if CURRENCY_TOKEN_MINT exists in .env, if not append it
    if grep -q "CURRENCY_TOKEN_MINT=" .env; then
        sed -i '' "s/CURRENCY_TOKEN_MINT=.*/CURRENCY_TOKEN_MINT=$CURRENCY_MINT/" .env
    else
        echo "CURRENCY_TOKEN_MINT=$CURRENCY_MINT" >> .env
    fi
else
    if grep -q "CURRENCY_TOKEN_MINT=" .env; then
        sed -i "s/CURRENCY_TOKEN_MINT=.*/CURRENCY_TOKEN_MINT=$CURRENCY_MINT/" .env
    else
        echo "CURRENCY_TOKEN_MINT=$CURRENCY_MINT" >> .env
    fi
fi

echo "🧹 Clearing Database Trading Data..."
cargo run --example clear_trading_data

echo "⚡ Starting API Gateway..."
nohup cargo run --bin api-gateway > api.log 2>&1 &

echo "⏳ Waiting for API Gateway to boot (15s)..."
sleep 15

echo "🎯 Environment is ready for P2P test!"
echo "Run: python3 scripts/test_p2p_flow.py"
