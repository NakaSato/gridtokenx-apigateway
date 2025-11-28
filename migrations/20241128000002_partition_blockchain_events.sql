-- Partition blockchain_events table by slot (1M slots per partition ≈ 5 days)
-- This improves query performance for event replay and slot-based queries

-- Step 1: Create partitioned table structure
CREATE TABLE blockchain_events_partitioned (
    LIKE blockchain_events INCLUDING DEFAULTS
) PARTITION BY RANGE (slot);

-- Add Primary Key (must include partition key)
ALTER TABLE blockchain_events_partitioned ADD PRIMARY KEY (id, slot);

-- Add Indexes
CREATE INDEX idx_blockchain_events_part_signature ON blockchain_events_partitioned(transaction_signature);
CREATE INDEX idx_blockchain_events_part_type ON blockchain_events_partitioned(event_type);
CREATE INDEX idx_blockchain_events_part_slot ON blockchain_events_partitioned(slot);
CREATE INDEX idx_blockchain_events_part_processed ON blockchain_events_partitioned(processed);
CREATE INDEX idx_blockchain_events_part_created ON blockchain_events_partitioned(created_at);

-- Unique constraint (must include partition key)
CREATE UNIQUE INDEX idx_blockchain_events_part_unique ON blockchain_events_partitioned(transaction_signature, event_type, slot);

-- Step 2: Create partitions for slot ranges
-- Partition 0-1M
CREATE TABLE blockchain_events_slot_0 PARTITION OF blockchain_events_partitioned
    FOR VALUES FROM (0) TO (1000000);

-- Partition 1M-2M
CREATE TABLE blockchain_events_slot_1m PARTITION OF blockchain_events_partitioned
    FOR VALUES FROM (1000000) TO (2000000);

-- Partition 2M-3M
CREATE TABLE blockchain_events_slot_2m PARTITION OF blockchain_events_partitioned
    FOR VALUES FROM (2000000) TO (3000000);

-- Partition 3M-4M
CREATE TABLE blockchain_events_slot_3m PARTITION OF blockchain_events_partitioned
    FOR VALUES FROM (3000000) TO (4000000);

-- Partition 4M-5M
CREATE TABLE blockchain_events_slot_4m PARTITION OF blockchain_events_partitioned
    FOR VALUES FROM (4000000) TO (5000000);

-- Default partition for future slots
CREATE TABLE blockchain_events_default PARTITION OF blockchain_events_partitioned DEFAULT;

-- Step 3: Copy existing data to partitioned table
INSERT INTO blockchain_events_partitioned 
SELECT * FROM blockchain_events;

-- Step 4: Verify data migration
DO $$
DECLARE
    old_count BIGINT;
    new_count BIGINT;
BEGIN
    SELECT COUNT(*) INTO old_count FROM blockchain_events;
    SELECT COUNT(*) INTO new_count FROM blockchain_events_partitioned;
    
    IF old_count != new_count THEN
        RAISE EXCEPTION 'Data migration failed: old_count=%, new_count=%', old_count, new_count;
    END IF;
    
    RAISE NOTICE 'Data migration successful: % rows migrated', new_count;
END $$;

-- Step 5: Rename tables (keep old table as backup)
ALTER TABLE blockchain_events RENAME TO blockchain_events_old_backup;
ALTER TABLE blockchain_events_partitioned RENAME TO blockchain_events;

-- Step 6: Create function to automatically create future slot partitions
CREATE OR REPLACE FUNCTION create_blockchain_events_partition(start_slot BIGINT)
RETURNS void AS $$
DECLARE
    partition_name TEXT;
    end_slot BIGINT;
BEGIN
    partition_name := 'blockchain_events_slot_' || (start_slot / 1000000) || 'm';
    end_slot := start_slot + 1000000;
    
    -- Check if partition already exists
    IF NOT EXISTS (
        SELECT 1 FROM pg_class WHERE relname = partition_name
    ) THEN
        EXECUTE format(
            'CREATE TABLE %I PARTITION OF blockchain_events FOR VALUES FROM (%L) TO (%L)',
            partition_name, start_slot, end_slot
        );
        RAISE NOTICE 'Created partition: % for slots % to %', partition_name, start_slot, end_slot;
    ELSE
        RAISE NOTICE 'Partition already exists: %', partition_name;
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Step 7: Create partitions up to 10M slots
DO $$
DECLARE
    i BIGINT;
BEGIN
    FOR i IN 5..9 LOOP
        PERFORM create_blockchain_events_partition(i * 1000000);
    END LOOP;
END $$;

-- Step 8: Grant permissions
GRANT SELECT, INSERT, UPDATE, DELETE ON blockchain_events TO gridtokenx;

-- Verification queries
-- View partition information
SELECT 
    schemaname,
    tablename,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS size
FROM pg_tables
WHERE tablename LIKE 'blockchain_events_%'
ORDER BY tablename;

-- View data distribution across partitions
SELECT 
    tableoid::regclass AS partition_name,
    COUNT(*) AS row_count,
    MIN(slot) AS min_slot,
    MAX(slot) AS max_slot
FROM blockchain_events
GROUP BY tableoid
ORDER BY partition_name;
