-- Migration: Financial Settlement Hardening
-- Adds escrow and refund tracking support

-- Add balance and locked columns to users
ALTER TABLE users
ADD COLUMN IF NOT EXISTS balance NUMERIC(20, 8) DEFAULT 0;

ALTER TABLE users
ADD COLUMN IF NOT EXISTS locked_amount NUMERIC(20, 8) DEFAULT 0;

ALTER TABLE users
ADD COLUMN IF NOT EXISTS locked_energy NUMERIC(20, 8) DEFAULT 0;

-- Track refund transactions
ALTER TABLE trading_orders
ADD COLUMN IF NOT EXISTS refund_tx_signature VARCHAR(88);

-- Create escrow_records for auditing
CREATE TABLE IF NOT EXISTS escrow_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    order_id UUID REFERENCES trading_orders (id) ON DELETE CASCADE,
    amount NUMERIC(20, 8) NOT NULL,
    asset_type VARCHAR(20) NOT NULL, -- 'currency', 'energy'
    escrow_type VARCHAR(20) NOT NULL, -- 'buy_lock', 'sell_lock'
    status VARCHAR(20) NOT NULL DEFAULT 'locked', -- 'locked', 'released', 'refunded'
    description TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW (),
    updated_at TIMESTAMPTZ DEFAULT NOW (),
    CONSTRAINT chk_escrow_status CHECK (
        status IN (
            'locked',
            'released',
            'refunded'
        )
    ),
    CONSTRAINT chk_escrow_asset CHECK (
        asset_type IN ('currency', 'energy')
    )
);

CREATE INDEX IF NOT EXISTS idx_escrow_records_user ON escrow_records (user_id);

CREATE INDEX IF NOT EXISTS idx_escrow_records_order ON escrow_records (order_id);

CREATE INDEX IF NOT EXISTS idx_escrow_records_status ON escrow_records (status);

-- Update trigger for escrow_records
DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'update_escrow_records_updated_at') THEN
        CREATE TRIGGER update_escrow_records_updated_at BEFORE UPDATE ON escrow_records
            FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

END IF;

END $$;