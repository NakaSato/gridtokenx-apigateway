-- Add migration script here
CREATE TABLE IF NOT EXISTS liquidity_pools (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    token_a VARCHAR(255) NOT NULL,
    token_b VARCHAR(255) NOT NULL,
    reserve_a NUMERIC(20, 9) NOT NULL DEFAULT 0,
    reserve_b NUMERIC(20, 9) NOT NULL DEFAULT 0,
    fee_rate NUMERIC(5, 4) NOT NULL DEFAULT 0.0030, -- 0.3% default
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS swap_transactions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id),
    pool_id UUID NOT NULL REFERENCES liquidity_pools(id),
    input_token VARCHAR(255) NOT NULL,
    input_amount NUMERIC(20, 9) NOT NULL,
    output_token VARCHAR(255) NOT NULL,
    output_amount NUMERIC(20, 9) NOT NULL,
    fee_amount NUMERIC(20, 9) NOT NULL,
    slippage_tolerance NUMERIC(5, 4),
    status VARCHAR(50) NOT NULL, -- 'pending', 'completed', 'failed'
    tx_hash VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for faster lookups
CREATE INDEX idx_swap_user_id ON swap_transactions(user_id);
CREATE INDEX idx_swap_pool_id ON swap_transactions(pool_id);
