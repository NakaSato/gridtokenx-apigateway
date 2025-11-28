-- Add minting retry queue table to handle failed token minting operations
-- This table will store readings that failed to mint and need to be retried

CREATE TABLE IF NOT EXISTS minting_retry_queue (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reading_id UUID NOT NULL REFERENCES meter_readings(id) ON DELETE CASCADE,
    error_message TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 1,
    next_retry_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Create indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_minting_retry_queue_reading_id ON minting_retry_queue(reading_id);
CREATE INDEX IF NOT EXISTS idx_minting_retry_queue_next_retry_at ON minting_retry_queue(next_retry_at);
CREATE INDEX IF NOT EXISTS idx_minting_retry_queue_attempts ON minting_retry_queue(attempts);

-- Add a comment to explain the purpose of the table
COMMENT ON TABLE minting_retry_queue IS 'Stores failed meter readings that need to be retried for token minting';

