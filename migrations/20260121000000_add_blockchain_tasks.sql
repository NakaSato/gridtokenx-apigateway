-- Create ENUMs for task types and status
DO $$ BEGIN
    CREATE TYPE blockchain_task_type AS ENUM ('escrow_refund', 'settlement', 'minting');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

DO $$ BEGIN
    CREATE TYPE task_status AS ENUM ('pending', 'processing', 'completed', 'failed', 'max_retries');
EXCEPTION
    WHEN duplicate_object THEN null;
END $$;

-- Create the blockchain_tasks table
CREATE TABLE IF NOT EXISTS blockchain_tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_type blockchain_task_type NOT NULL,
    payload JSONB NOT NULL,
    status task_status NOT NULL DEFAULT 'pending',
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 5,
    last_error TEXT,
    next_retry_at TIMESTAMPTZ DEFAULT NOW(),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Index for finding pending tasks efficiently
CREATE INDEX IF NOT EXISTS idx_blockchain_tasks_processing ON blockchain_tasks(status, next_retry_at);

-- Trigger to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

DROP TRIGGER IF EXISTS update_blockchain_tasks_updated_at ON blockchain_tasks;
CREATE TRIGGER update_blockchain_tasks_updated_at
    BEFORE UPDATE ON blockchain_tasks
    FOR EACH ROW
    EXECUTE PROCEDURE update_updated_at_column();
