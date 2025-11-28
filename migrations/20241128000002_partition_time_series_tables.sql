-- Migration: Partition Time-Series Tables
-- Created: 2024-11-28
-- Description: Partition meter_readings and user_activities by month for better performance

-- =========================================================================
-- PARTITION METER READINGS TABLE
-- =========================================================================

-- Step 1: Rename existing table
ALTER TABLE meter_readings RENAME TO meter_readings_old;

-- Step 2: Create partitioned table
CREATE TABLE meter_readings (
    id UUID DEFAULT gen_random_uuid(),
    meter_id VARCHAR(50) NOT NULL,
    wallet_address VARCHAR(88) NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    energy_generated NUMERIC(12, 4),
    energy_consumed NUMERIC(12, 4),
    surplus_energy NUMERIC(12, 4),
    deficit_energy NUMERIC(12, 4),
    battery_level NUMERIC(5, 2),
    temperature NUMERIC(5, 2),
    voltage NUMERIC(8, 2),
    current NUMERIC(8, 2),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (id, timestamp)
) PARTITION BY RANGE (timestamp);

-- Step 3: Create partitions for current and next 3 months
CREATE TABLE meter_readings_2024_11 PARTITION OF meter_readings
    FOR VALUES FROM ('2024-11-01') TO ('2024-12-01');

CREATE TABLE meter_readings_2024_12 PARTITION OF meter_readings
    FOR VALUES FROM ('2024-12-01') TO ('2025-01-01');

CREATE TABLE meter_readings_2025_01 PARTITION OF meter_readings
    FOR VALUES FROM ('2025-01-01') TO ('2025-02-01');

CREATE TABLE meter_readings_2025_02 PARTITION OF meter_readings
    FOR VALUES FROM ('2025-02-01') TO ('2025-03-01');

-- Step 4: Create indexes on partitioned table
CREATE INDEX idx_meter_readings_timestamp ON meter_readings USING BRIN(timestamp);
CREATE INDEX idx_meter_readings_meter_timestamp ON meter_readings(meter_id, timestamp DESC);
CREATE INDEX idx_meter_readings_wallet_timestamp ON meter_readings(wallet_address, timestamp DESC);

-- Step 5: Migrate existing data (if any)
INSERT INTO meter_readings 
SELECT * FROM meter_readings_old
WHERE timestamp >= '2024-11-01';

-- Step 6: Drop old table (after verifying data migration)
-- DROP TABLE meter_readings_old;
-- Note: Commented out for safety. Run manually after verification.

-- =========================================================================
-- PARTITION USER ACTIVITIES TABLE
-- =========================================================================

-- Step 1: Rename existing table
ALTER TABLE user_activities RENAME TO user_activities_old;

-- Step 2: Create partitioned table
CREATE TABLE user_activities (
    id UUID DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    activity_type VARCHAR(50) NOT NULL,
    description TEXT,
    ip_address INET,
    user_agent TEXT,
    metadata JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (id, created_at)
) PARTITION BY RANGE (created_at);

-- Step 3: Create partitions for current and next 3 months
CREATE TABLE user_activities_2024_11 PARTITION OF user_activities
    FOR VALUES FROM ('2024-11-01') TO ('2024-12-01');

CREATE TABLE user_activities_2024_12 PARTITION OF user_activities
    FOR VALUES FROM ('2024-12-01') TO ('2025-01-01');

CREATE TABLE user_activities_2025_01 PARTITION OF user_activities
    FOR VALUES FROM ('2025-01-01') TO ('2025-02-01');

CREATE TABLE user_activities_2025_02 PARTITION OF user_activities
    FOR VALUES FROM ('2025-02-01') TO ('2025-03-01');

-- Step 4: Create indexes on partitioned table
CREATE INDEX idx_user_activities_created ON user_activities USING BRIN(created_at);
CREATE INDEX idx_user_activities_user_created ON user_activities(user_id, created_at DESC);
CREATE INDEX idx_user_activities_type_created ON user_activities(activity_type, created_at DESC);
CREATE INDEX idx_user_activities_metadata ON user_activities USING GIN(metadata jsonb_path_ops)
WHERE metadata IS NOT NULL;

-- Step 5: Migrate existing data (if any)
INSERT INTO user_activities 
SELECT * FROM user_activities_old
WHERE created_at >= '2024-11-01';

-- Step 6: Drop old table (after verifying data migration)
-- DROP TABLE user_activities_old;
-- Note: Commented out for safety. Run manually after verification.

-- =========================================================================
-- AUTO-CREATE FUTURE PARTITIONS FUNCTION
-- =========================================================================

CREATE OR REPLACE FUNCTION create_monthly_partitions()
RETURNS void AS $$
DECLARE
    start_date DATE;
    end_date DATE;
    partition_name TEXT;
BEGIN
    -- Create partitions for next 3 months
    FOR i IN 0..2 LOOP
        start_date := DATE_TRUNC('month', CURRENT_DATE + (i || ' months')::INTERVAL);
        end_date := start_date + INTERVAL '1 month';
        
        -- Meter readings partition
        partition_name := 'meter_readings_' || TO_CHAR(start_date, 'YYYY_MM');
        IF NOT EXISTS (
            SELECT 1 FROM pg_tables 
            WHERE tablename = partition_name
        ) THEN
            EXECUTE format(
                'CREATE TABLE %I PARTITION OF meter_readings FOR VALUES FROM (%L) TO (%L)',
                partition_name, start_date, end_date
            );
            RAISE NOTICE 'Created partition: %', partition_name;
        END IF;
        
        -- User activities partition
        partition_name := 'user_activities_' || TO_CHAR(start_date, 'YYYY_MM');
        IF NOT EXISTS (
            SELECT 1 FROM pg_tables 
            WHERE tablename = partition_name
        ) THEN
            EXECUTE format(
                'CREATE TABLE %I PARTITION OF user_activities FOR VALUES FROM (%L) TO (%L)',
                partition_name, start_date, end_date
            );
            RAISE NOTICE 'Created partition: %', partition_name;
        END IF;
    END LOOP;
END;
$$ LANGUAGE plpgsql;

-- Run the function to ensure partitions exist
SELECT create_monthly_partitions();

-- =========================================================================
-- PARTITION MAINTENANCE VIEW
-- =========================================================================

CREATE OR REPLACE VIEW v_partition_info AS
SELECT
    nmsp_parent.nspname AS schema_name,
    parent.relname AS parent_table,
    child.relname AS partition_name,
    pg_get_expr(child.relpartbound, child.oid) AS partition_bounds,
    pg_size_pretty(pg_total_relation_size(child.oid)) AS partition_size,
    (SELECT COUNT(*) FROM pg_inherits WHERE inhparent = parent.oid) AS total_partitions
FROM pg_inherits
JOIN pg_class parent ON pg_inherits.inhparent = parent.oid
JOIN pg_class child ON pg_inherits.inhrelid = child.oid
JOIN pg_namespace nmsp_parent ON nmsp_parent.oid = parent.relnamespace
WHERE parent.relname IN ('meter_readings', 'user_activities')
ORDER BY parent.relname, child.relname;

-- =========================================================================
-- PARTITION CLEANUP FUNCTION (Archive old partitions)
-- =========================================================================

CREATE OR REPLACE FUNCTION archive_old_partitions(months_to_keep INTEGER DEFAULT 6)
RETURNS void AS $$
DECLARE
    partition_record RECORD;
    cutoff_date DATE;
BEGIN
    cutoff_date := DATE_TRUNC('month', CURRENT_DATE - (months_to_keep || ' months')::INTERVAL);
    
    FOR partition_record IN
        SELECT
            child.relname AS partition_name,
            parent.relname AS parent_table
        FROM pg_inherits
        JOIN pg_class parent ON pg_inherits.inhparent = parent.oid
        JOIN pg_class child ON pg_inherits.inhrelid = child.oid
        WHERE parent.relname IN ('meter_readings', 'user_activities')
        AND child.relname ~ '\d{4}_\d{2}$'
    LOOP
        -- Extract date from partition name (e.g., meter_readings_2024_01)
        DECLARE
            partition_date DATE;
        BEGIN
            partition_date := TO_DATE(
                SUBSTRING(partition_record.partition_name FROM '\d{4}_\d{2}$'),
                'YYYY_MM'
            );
            
            IF partition_date < cutoff_date THEN
                -- Detach partition (keeps data but removes from parent)
                EXECUTE format(
                    'ALTER TABLE %I DETACH PARTITION %I',
                    partition_record.parent_table,
                    partition_record.partition_name
                );
                RAISE NOTICE 'Detached old partition: %', partition_record.partition_name;
                
                -- Optionally: Drop the partition
                -- EXECUTE format('DROP TABLE %I', partition_record.partition_name);
            END IF;
        END;
    END LOOP;
END;
$$ LANGUAGE plpgsql;

-- =========================================================================
-- VERIFICATION QUERIES
-- =========================================================================

-- List all partitions
-- SELECT * FROM v_partition_info;

-- Check partition sizes
-- SELECT partition_name, partition_size FROM v_partition_info ORDER BY partition_name;

-- Verify data distribution
-- SELECT 
--     DATE_TRUNC('month', timestamp) as month,
--     COUNT(*) as readings_count
-- FROM meter_readings
-- GROUP BY month
-- ORDER BY month;

-- Test partition pruning (EXPLAIN should show only relevant partition)
-- EXPLAIN SELECT * FROM meter_readings 
-- WHERE timestamp >= '2024-11-01' AND timestamp < '2024-12-01';
