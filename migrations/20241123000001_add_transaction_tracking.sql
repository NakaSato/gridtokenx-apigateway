-- Enhanced Transaction Tracking Migration
-- Extends existing tables with blockchain transaction metadata
-- Created: November 23, 2024

-- =========================================================================
-- ENHANCE TRADING ORDERS TABLE
-- =========================================================================

-- Add blockchain transaction tracking columns to trading_orders
ALTER TABLE trading_orders
ADD COLUMN IF NOT EXISTS blockchain_tx_signature VARCHAR(88),
ADD COLUMN IF NOT EXISTS blockchain_tx_type VARCHAR(50),
ADD COLUMN IF NOT EXISTS blockchain_status VARCHAR(20) DEFAULT 'pending',
ADD COLUMN IF NOT EXISTS blockchain_attempts INTEGER DEFAULT 0,
ADD COLUMN IF NOT EXISTS blockchain_last_error TEXT,
ADD COLUMN IF NOT EXISTS blockchain_submitted_at TIMESTAMPTZ,
ADD COLUMN IF NOT EXISTS blockchain_confirmed_at TIMESTAMPTZ;

-- Add indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_trading_orders_tx_signature ON trading_orders(blockchain_tx_signature);
CREATE INDEX IF NOT EXISTS idx_trading_orders_tx_type ON trading_orders(blockchain_tx_type);
CREATE INDEX IF NOT EXISTS idx_trading_orders_blockchain_status ON trading_orders(blockchain_status);
CREATE INDEX IF NOT EXISTS idx_trading_orders_blockchain_submitted ON trading_orders(blockchain_submitted_at);

-- =========================================================================
-- ENHANCE SETTLEMENTS TABLE
-- =========================================================================

-- Add blockchain transaction tracking columns to settlements
ALTER TABLE settlements
ADD COLUMN IF NOT EXISTS blockchain_tx_signature VARCHAR(88),
ADD COLUMN IF NOT EXISTS blockchain_tx_type VARCHAR(50) DEFAULT 'settlement',
ADD COLUMN IF NOT EXISTS blockchain_status VARCHAR(20) DEFAULT 'pending',
ADD COLUMN IF NOT EXISTS blockchain_attempts INTEGER DEFAULT 0,
ADD COLUMN IF NOT EXISTS blockchain_last_error TEXT,
ADD COLUMN IF NOT EXISTS blockchain_submitted_at TIMESTAMPTZ,
ADD COLUMN IF NOT EXISTS blockchain_confirmed_at TIMESTAMPTZ;

-- Add indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_settlements_tx_signature ON settlements(blockchain_tx_signature);
CREATE INDEX IF NOT EXISTS idx_settlements_tx_type ON settlements(blockchain_tx_type);
CREATE INDEX IF NOT EXISTS idx_settlements_blockchain_status ON settlements(blockchain_status);
CREATE INDEX IF NOT EXISTS idx_settlements_blockchain_submitted ON settlements(blockchain_submitted_at);

-- =========================================================================
-- ENHANCE METER READINGS TABLE
-- =========================================================================

-- Add blockchain transaction tracking columns to meter_readings
ALTER TABLE meter_readings
ADD COLUMN IF NOT EXISTS blockchain_tx_signature VARCHAR(88),
ADD COLUMN IF NOT EXISTS blockchain_tx_type VARCHAR(50) DEFAULT 'meter_reading',
ADD COLUMN IF NOT EXISTS blockchain_status VARCHAR(20) DEFAULT 'pending',
ADD COLUMN IF NOT EXISTS blockchain_attempts INTEGER DEFAULT 0,
ADD COLUMN IF NOT EXISTS blockchain_last_error TEXT,
ADD COLUMN IF NOT EXISTS blockchain_submitted_at TIMESTAMPTZ,
ADD COLUMN IF NOT EXISTS blockchain_confirmed_at TIMESTAMPTZ,
ADD COLUMN IF NOT EXISTS blockchain_registered BOOLEAN DEFAULT FALSE;

-- Add indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_meter_readings_tx_signature ON meter_readings(blockchain_tx_signature);
CREATE INDEX IF NOT EXISTS idx_meter_readings_tx_type ON meter_readings(blockchain_tx_type);
CREATE INDEX IF NOT EXISTS idx_meter_readings_blockchain_status ON meter_readings(blockchain_status);
CREATE INDEX IF NOT EXISTS idx_meter_readings_blockchain_submitted ON meter_readings(blockchain_submitted_at);
CREATE INDEX IF NOT EXISTS idx_meter_readings_registered ON meter_readings(blockchain_registered);

-- =========================================================================
-- ENHANCE USERS TABLE
-- =========================================================================

-- Add blockchain transaction tracking columns to users
ALTER TABLE users
ADD COLUMN IF NOT EXISTS blockchain_tx_signature VARCHAR(88),
ADD COLUMN IF NOT EXISTS blockchain_tx_type VARCHAR(50) DEFAULT 'user_registration',
ADD COLUMN IF NOT EXISTS blockchain_status VARCHAR(20) DEFAULT 'pending',
ADD COLUMN IF NOT EXISTS blockchain_attempts INTEGER DEFAULT 0,
ADD COLUMN IF NOT EXISTS blockchain_last_error TEXT,
ADD COLUMN IF NOT EXISTS blockchain_submitted_at TIMESTAMPTZ,
ADD COLUMN IF NOT EXISTS blockchain_confirmed_at TIMESTAMPTZ,
ADD COLUMN IF NOT EXISTS blockchain_registered BOOLEAN DEFAULT FALSE;

-- Add indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_users_tx_signature ON users(blockchain_tx_signature);
CREATE INDEX IF NOT EXISTS idx_users_tx_type ON users(blockchain_tx_type);
CREATE INDEX IF NOT EXISTS idx_users_blockchain_status ON users(blockchain_status);
CREATE INDEX IF NOT EXISTS idx_users_registered ON users(blockchain_registered);

-- =========================================================================
-- CREATE BLOCKCHAIN_OPERATIONS VIEW
-- =========================================================================

-- Create unified view for querying all blockchain operations across tables
CREATE OR REPLACE VIEW blockchain_operations AS
SELECT
    'trading_order' AS operation_type,
    id AS operation_id,
    user_id,
    blockchain_tx_signature AS signature,
    blockchain_tx_type AS tx_type,
    blockchain_status AS operation_status,
    blockchain_attempts AS attempts,
    blockchain_last_error AS last_error,
    blockchain_submitted_at AS submitted_at,
    blockchain_confirmed_at AS confirmed_at,
    created_at,
    updated_at
FROM trading_orders
WHERE blockchain_tx_signature IS NOT NULL

UNION ALL

SELECT
    'settlement' AS operation_type,
    id AS operation_id,
    buyer_id AS user_id,
    blockchain_tx_signature AS signature,
    blockchain_tx_type AS tx_type,
    blockchain_status AS operation_status,
    blockchain_attempts AS attempts,
    blockchain_last_error AS last_error,
    blockchain_submitted_at AS submitted_at,
    blockchain_confirmed_at AS confirmed_at,
    created_at,
    updated_at
FROM settlements
WHERE blockchain_tx_signature IS NOT NULL

UNION ALL

SELECT
    'meter_reading' AS operation_type,
    id AS operation_id,
    NULL AS user_id, -- meter readings don't have direct user_id in the schema
    blockchain_tx_signature AS signature,
    blockchain_tx_type AS tx_type,
    blockchain_status AS operation_status,
    blockchain_attempts AS attempts,
    blockchain_last_error AS last_error,
    blockchain_submitted_at AS submitted_at,
    blockchain_confirmed_at AS confirmed_at,
    created_at,
    updated_at
FROM meter_readings
WHERE blockchain_tx_signature IS NOT NULL

UNION ALL

SELECT
    'user_registration' AS operation_type,
    id AS operation_id,
    id AS user_id,
    blockchain_tx_signature AS signature,
    blockchain_tx_type AS tx_type,
    blockchain_status AS operation_status,
    blockchain_attempts AS attempts,
    blockchain_last_error AS last_error,
    blockchain_submitted_at AS submitted_at,
    blockchain_confirmed_at AS confirmed_at,
    created_at,
    updated_at
FROM users
WHERE blockchain_tx_signature IS NOT NULL;

-- Note: Indexes cannot be created on views
-- The underlying tables have indexes that will be used when querying the view

-- =========================================================================
-- CREATE HELPER FUNCTIONS
-- =========================================================================

-- Function to increment blockchain attempts and record error
CREATE OR REPLACE FUNCTION increment_blockchain_attempts(
    p_table_name TEXT,
    p_record_id UUID,
    p_error_message TEXT DEFAULT NULL
) RETURNS BOOLEAN AS $$
DECLARE
    query TEXT;
BEGIN
    -- Build the dynamic query based on table name
    query := format(
        'UPDATE %I SET blockchain_attempts = blockchain_attempts + 1, blockchain_last_error = $2, updated_at = NOW() WHERE id = $1',
        p_table_name
    );

    -- Execute the query
    EXECUTE query USING p_record_id, p_error_message;

    RETURN TRUE;
EXCEPTION
    WHEN OTHERS THEN
        RAISE NOTICE 'Error incrementing attempts for % %: %', p_table_name, p_record_id, SQLERRM;
        RETURN FALSE;
END;
$$ LANGUAGE plpgsql;

-- Function to mark blockchain transaction as confirmed
CREATE OR REPLACE FUNCTION mark_blockchain_confirmed(
    p_table_name TEXT,
    p_record_id UUID,
    p_signature VARCHAR(88),
    p_status TEXT DEFAULT 'confirmed'
) RETURNS BOOLEAN AS $$
DECLARE
    query TEXT;
BEGIN
    -- Build the dynamic query based on table name
    query := format(
        'UPDATE %I SET blockchain_status = $2, blockchain_tx_signature = $3, blockchain_confirmed_at = NOW(), updated_at = NOW() WHERE id = $1',
        p_table_name
    );

    -- Execute the query
    EXECUTE query USING p_record_id, p_status, p_signature;

    RETURN TRUE;
EXCEPTION
    WHEN OTHERS THEN
        RAISE NOTICE 'Error confirming transaction for % %: %', p_table_name, p_record_id, SQLERRM;
        RETURN FALSE;
END;
$$ LANGUAGE plpgsql;

-- Function to mark blockchain transaction as submitted
CREATE OR REPLACE FUNCTION mark_blockchain_submitted(
    p_table_name TEXT,
    p_record_id UUID,
    p_signature VARCHAR(88),
    p_tx_type TEXT DEFAULT NULL
) RETURNS BOOLEAN AS $$
DECLARE
    query TEXT;
BEGIN
    -- Build the dynamic query based on table name
    query := format(
        'UPDATE %I SET blockchain_status = ''submitted'', blockchain_tx_signature = $2, blockchain_submitted_at = NOW(), updated_at = NOW() WHERE id = $1',
        p_table_name
    );

    -- If tx_type is provided, include it in the update
    IF p_tx_type IS NOT NULL THEN
        query := format(
            'UPDATE %I SET blockchain_status = ''submitted'', blockchain_tx_signature = $2, blockchain_tx_type = $3, blockchain_submitted_at = NOW(), updated_at = NOW() WHERE id = $1',
            p_table_name
        );
        EXECUTE query USING p_record_id, p_signature, p_tx_type;
    ELSE
        EXECUTE query USING p_record_id, p_signature;
    END IF;

    RETURN TRUE;
EXCEPTION
    WHEN OTHERS THEN
        RAISE NOTICE 'Error submitting transaction for % %: %', p_table_name, p_record_id, SQLERRM;
        RETURN FALSE;
END;
$$ LANGUAGE plpgsql;

-- =========================================================================
-- CREATE TRIGGERS TO UPDATE BLOCKCHAIN STATUS BASED ON TRANSACTION HASH
-- =========================================================================

-- Function to update blockchain status when transaction hash is set
CREATE OR REPLACE FUNCTION update_blockchain_status_on_hash() RETURNS TRIGGER AS $$
BEGIN
    -- If transaction hash is set and status is still pending, update to submitted
    IF NEW.blockchain_tx_signature IS NOT NULL AND OLD.blockchain_tx_signature IS NULL THEN
        NEW.blockchain_status = 'submitted';
        NEW.blockchain_submitted_at = NOW();
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create triggers for each table
CREATE TRIGGER update_trading_orders_blockchain_status
    BEFORE UPDATE ON trading_orders
    FOR EACH ROW EXECUTE FUNCTION update_blockchain_status_on_hash();

CREATE TRIGGER update_settlements_blockchain_status
    BEFORE UPDATE ON settlements
    FOR EACH ROW EXECUTE FUNCTION update_blockchain_status_on_hash();

CREATE TRIGGER update_meter_readings_blockchain_status
    BEFORE UPDATE ON meter_readings
    FOR EACH ROW EXECUTE FUNCTION update_blockchain_status_on_hash();

CREATE TRIGGER update_users_blockchain_status
    BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION update_blockchain_status_on_hash();

-- =========================================================================
-- MIGRATION NOTES
-- =========================================================================

-- 1. This migration extends existing tables with blockchain transaction tracking
-- 2. Creates a unified view (blockchain_operations) for querying across all tables
-- 3. Adds helper functions for common operations
-- 4. Creates triggers to automatically update status when transaction hash is set
-- 5. All new columns are nullable to ensure backward compatibility
-- 6. Indexes are added for efficient querying of blockchain operations
-- 7. Backfilling existing records may be required in a subsequent step
