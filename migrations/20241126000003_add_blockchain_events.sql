-- Migration: Add blockchain events tracking
-- Created: 2024-11-26
-- Purpose: Track blockchain events and on-chain confirmations for meter readings

-- =========================================================================
-- BLOCKCHAIN EVENTS TABLE
-- =========================================================================
CREATE TABLE blockchain_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type VARCHAR(50) NOT NULL,
    transaction_signature VARCHAR(88) NOT NULL,
    slot BIGINT NOT NULL,
    block_time TIMESTAMPTZ,
    program_id VARCHAR(44) NOT NULL,
    event_data JSONB NOT NULL,
    processed BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_blockchain_events_signature ON blockchain_events(transaction_signature);
CREATE INDEX idx_blockchain_events_type ON blockchain_events(event_type);
CREATE INDEX idx_blockchain_events_slot ON blockchain_events(slot);
CREATE INDEX idx_blockchain_events_processed ON blockchain_events(processed);
CREATE INDEX idx_blockchain_events_created ON blockchain_events(created_at);

-- Unique constraint to prevent duplicate events
CREATE UNIQUE INDEX idx_blockchain_events_unique ON blockchain_events(transaction_signature, event_type);

-- =========================================================================
-- EVENT PROCESSING STATE TABLE
-- =========================================================================
CREATE TABLE event_processing_state (
    id SERIAL PRIMARY KEY,
    service_name VARCHAR(50) UNIQUE NOT NULL,
    last_processed_slot BIGINT NOT NULL DEFAULT 0,
    last_processed_signature VARCHAR(88),
    last_processed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Insert initial state for event processor
INSERT INTO event_processing_state (service_name, last_processed_slot)
VALUES ('event_processor', 0);

-- =========================================================================
-- METER READINGS ON-CHAIN CONFIRMATION
-- =========================================================================
-- Add on-chain confirmation tracking to meter_readings
ALTER TABLE meter_readings 
ADD COLUMN IF NOT EXISTS on_chain_confirmed BOOLEAN DEFAULT false,
ADD COLUMN IF NOT EXISTS on_chain_slot BIGINT,
ADD COLUMN IF NOT EXISTS on_chain_confirmed_at TIMESTAMPTZ;

CREATE INDEX idx_meter_readings_on_chain_confirmed ON meter_readings(on_chain_confirmed);
CREATE INDEX idx_meter_readings_on_chain_slot ON meter_readings(on_chain_slot);

-- =========================================================================
-- UPDATED_AT TRIGGER
-- =========================================================================
CREATE TRIGGER update_blockchain_events_updated_at BEFORE UPDATE ON blockchain_events
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_event_processing_state_updated_at BEFORE UPDATE ON event_processing_state
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- =========================================================================
-- COMMENTS
-- =========================================================================
COMMENT ON TABLE blockchain_events IS 'Tracks all blockchain events synced from Solana';
COMMENT ON TABLE event_processing_state IS 'Tracks the last processed slot for event synchronization';
COMMENT ON COLUMN meter_readings.on_chain_confirmed IS 'Whether the minting transaction has been confirmed on-chain';
COMMENT ON COLUMN meter_readings.on_chain_slot IS 'The slot number where the transaction was confirmed';
COMMENT ON COLUMN meter_readings.on_chain_confirmed_at IS 'Timestamp when on-chain confirmation was detected';
