#!/bin/bash
# verification_settlement_revenue.sh

# 1. Setup participants
BUYER_ID=$(docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -t -c "SELECT id FROM users LIMIT 1 OFFSET 0" | xargs)
SELLER_ID=$(docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -t -c "SELECT id FROM users LIMIT 1 OFFSET 1" | xargs)

if [ -z "$SELLER_ID" ]; then
    echo "Need at least 2 users for settlement test."
    SELLER_ID=$(uuidgen | tr '[:upper:]' '[:lower:]')
    docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c "INSERT INTO users (id, email, username, role, status) VALUES ('$SELLER_ID', 'seller@example.com', 'selleruser', 'user', 'active')"
fi

echo "Buyer: $BUYER_ID, Seller: $SELLER_ID"

# 2. Setup Initial Balances
docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c "UPDATE users SET balance = 1000, locked_amount = 0, locked_energy = 0 WHERE id = '$BUYER_ID'"
docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c "UPDATE users SET balance = 0, locked_amount = 0, locked_energy = 0 WHERE id = '$SELLER_ID'"

# 3. Create a Mock Epoch if none exists
EPOCH_ID=$(docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -t -c "SELECT id FROM market_epochs WHERE status = 'active' LIMIT 1" | xargs)
if [ -z "$EPOCH_ID" ]; then
    EPOCH_ID=$(uuidgen | tr '[:upper:]' '[:lower:]')
    docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c "INSERT INTO market_epochs (id, epoch_number, start_time, end_time, status) VALUES ('$EPOCH_ID', 1, NOW(), NOW() + INTERVAL '1 hour', 'active')"
fi

# 4. Create Mock Trading Orders
BUY_ORDER_ID=$(uuidgen | tr '[:upper:]' '[:lower:]')
SELL_ORDER_ID=$(uuidgen | tr '[:upper:]' '[:lower:]')
docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c "
INSERT INTO trading_orders (id, user_id, order_type, side, energy_amount, price_per_kwh, status, epoch_id)
VALUES ('$BUY_ORDER_ID', '$BUY_ORDER_ID', 'limit', 'buy', 100.0, 2.0, 'pending', '$EPOCH_ID');
"
docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c "
INSERT INTO trading_orders (id, user_id, order_type, side, energy_amount, price_per_kwh, status, epoch_id)
VALUES ('$SELL_ORDER_ID', '$SELL_ORDER_ID', 'limit', 'sell', 100.0, 2.0, 'pending', '$EPOCH_ID');
"

# 5. Simulate Escrow Lock
docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c "UPDATE users SET balance = balance - 200, locked_amount = 200 WHERE id = '$BUYER_ID'"
docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c "UPDATE users SET locked_energy = 100 WHERE id = '$SELLER_ID'"

# 6. Create Mock Settlement
SETTLEMENT_ID=$(uuidgen | tr '[:upper:]' '[:lower:]')
# Total: 200. Fee (1%): 2. Wheeling: 5. Net: 193.
docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c "
INSERT INTO settlements (
    id, buyer_id, seller_id, buy_order_id, sell_order_id, energy_amount, 
    price_per_kwh, total_amount, fee_amount, wheeling_charge, net_amount, status, epoch_id
) VALUES (
    '$SETTLEMENT_ID', '$BUYER_ID', '$SELLER_ID', '$BUY_ORDER_ID', '$SELL_ORDER_ID', 
    100.0, 2.0, 200.0, 2.0, 5.0, 193.0, 'pending', '$EPOCH_ID'
);
"

# 7. Insert Locked Escrow Records
docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c "
INSERT INTO escrow_records (user_id, order_id, amount, asset_type, escrow_type, status)
VALUES ('$BUYER_ID', '$BUY_ORDER_ID', 200.0, 'currency', 'buy_lock', 'locked');
"
docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c "
INSERT INTO escrow_records (user_id, order_id, amount, asset_type, escrow_type, status)
VALUES ('$SELLER_ID', '$SELL_ORDER_ID', 100.0, 'energy', 'sell_lock', 'locked');
"

echo "--- State before Finalization ---"
echo "Buyer Balance/Locked:"
docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c "SELECT balance, locked_amount FROM users WHERE id = '$BUYER_ID'"
echo "Seller Balance/Locked Energy:"
docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c "SELECT balance, locked_energy FROM users WHERE id = '$SELLER_ID'"

# 6. Trigger Settlement Processing (via background task or manual psql simulation of the logic)
# Since I want to verify the RUST code, I can't easily trigger the background loop on demand without waiting.
# But I can wait for the background settlement loop (every 5s).
# Note: The background loop calls 'process_pending_settlements' which calls 'finalize_escrow'.

echo "Waiting for background settlement processor..."
sleep 10

echo "--- State after Finalization ---"
echo "Buyer Balance/Locked (Locked should decrease by 200, balance stays 800):"
docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c "SELECT balance, locked_amount FROM users WHERE id = '$BUYER_ID'"
echo "Seller Balance/Locked Energy (Balance should be 193, locked_energy should be 0):"
docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c "SELECT balance, locked_energy FROM users WHERE id = '$SELLER_ID'"

echo "--- Platform Revenue ---"
docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c "SELECT revenue_type, amount FROM platform_revenue WHERE settlement_id = '$SETTLEMENT_ID'"

echo "--- Escrow Records (Status should be released) ---"
docker exec gridtokenx-postgres psql -U gridtokenx_user -d gridtokenx -c "SELECT user_id, status FROM escrow_records WHERE order_id IN ('$BUY_ORDER_ID', '$SELL_ORDER_ID')"

echo "Verification complete."
