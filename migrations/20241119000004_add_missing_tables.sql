-- Add trades table
-- Created: November 19, 2024

-- =========================================================================
-- Add missing columns to trading_orders
-- =========================================================================
ALTER TABLE trading_orders ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ;

-- =========================================================================
-- Add missing columns to meter_readings
-- =========================================================================
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS kwh_amount NUMERIC(12, 4);
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS minted BOOLEAN DEFAULT FALSE;
ALTER TABLE meter_readings ADD COLUMN IF NOT EXISTS mint_signature VARCHAR(88);

-- =========================================================================
-- BLOCKCHAIN TRANSACTIONS TABLE
-- =========================================================================
CREATE TABLE IF NOT EXISTS blockchain_transactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    signature VARCHAR(88) UNIQUE NOT NULL,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    program_id VARCHAR(44) NOT NULL,
    instruction_name VARCHAR(100),
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    fee BIGINT,
    compute_units_consumed INTEGER,
    submitted_at TIMESTAMPTZ DEFAULT NOW(),
    confirmed_at TIMESTAMPTZ,
    error_message TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    CONSTRAINT chk_blockchain_status CHECK (status IN ('pending', 'confirmed', 'failed'))
);

CREATE INDEX IF NOT EXISTS idx_blockchain_transactions_signature ON blockchain_transactions(signature);
CREATE INDEX IF NOT EXISTS idx_blockchain_transactions_user ON blockchain_transactions(user_id);
CREATE INDEX IF NOT EXISTS idx_blockchain_transactions_status ON blockchain_transactions(status);
CREATE INDEX IF NOT EXISTS idx_blockchain_transactions_submitted ON blockchain_transactions(submitted_at);

-- =========================================================================
-- AUDIT LOGS TABLE
-- =========================================================================
CREATE TABLE IF NOT EXISTS audit_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type VARCHAR(50) NOT NULL,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    ip_address INET,
    event_data JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_audit_logs_event_type ON audit_logs(event_type);
CREATE INDEX IF NOT EXISTS idx_audit_logs_user ON audit_logs(user_id);
CREATE INDEX IF NOT EXISTS idx_audit_logs_created ON audit_logs(created_at);

-- =========================================================================
-- Add trigger for blockchain_transactions updated_at
-- =========================================================================
CREATE TRIGGER update_blockchain_transactions_updated_at BEFORE UPDATE ON blockchain_transactions
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
