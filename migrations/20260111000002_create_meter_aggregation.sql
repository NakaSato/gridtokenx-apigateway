-- Migration: Create meter_unminted_balances table for energy aggregation
-- Created: January 11, 2026

CREATE TABLE IF NOT EXISTS meter_unminted_balances (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    meter_serial VARCHAR(100) UNIQUE NOT NULL,
    accumulated_kwh NUMERIC(15,6) DEFAULT 0.0,
    last_mint_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Index for fast lookup by meter serial
CREATE INDEX IF NOT EXISTS idx_meter_unminted_balances_serial ON meter_unminted_balances(meter_serial);

-- Trigger to update updated_at column
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'update_meter_unminted_balances_updated_at') THEN
        CREATE TRIGGER update_meter_unminted_balances_updated_at BEFORE UPDATE ON meter_unminted_balances
            FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
    END IF;
END $$;
