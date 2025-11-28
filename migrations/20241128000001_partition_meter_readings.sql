-- Partition meter_readings table by reading_timestamp (monthly partitions)
-- Fixed version: Primary key includes partition key

-- Step 1: Create partitioned table structure (without INCLUDING CONSTRAINTS)
CREATE TABLE meter_readings_partitioned (
    id UUID NOT NULL,
    meter_serial VARCHAR(50),
    meter_id UUID,
    user_id UUID,
    wallet_address VARCHAR(88) NOT NULL,
    
    -- Energy data
    timestamp TIMESTAMPTZ NOT NULL,
    energy_generated NUMERIC(12,4),
    energy_consumed NUMERIC(12,4),
    surplus_energy NUMERIC(12,4),
    deficit_energy NUMERIC(12,4),
    kwh_amount NUMERIC(12,4),
    
    -- Meter telemetry
    battery_level NUMERIC(5,2),
    temperature NUMERIC(5,2),
    voltage NUMERIC(8,2),
    current NUMERIC(8,2),
    
    -- Minting status
    minted BOOLEAN DEFAULT false,
    mint_signature VARCHAR(88),
    mint_tx_signature VARCHAR(88),
    
    -- Blockchain tracking
    blockchain_tx_signature VARCHAR(88),
    blockchain_tx_type VARCHAR(50) DEFAULT 'meter_reading',
    blockchain_status VARCHAR(20) DEFAULT 'pending',
    blockchain_attempts INTEGER DEFAULT 0,
    blockchain_last_error TEXT,
    blockchain_submitted_at TIMESTAMPTZ,
    blockchain_confirmed_at TIMESTAMPTZ,
    blockchain_registered BOOLEAN DEFAULT false,
    
    -- On-chain confirmation
    on_chain_confirmed BOOLEAN DEFAULT false,
    on_chain_slot BIGINT,
    on_chain_confirmed_at TIMESTAMPTZ,
    
    -- Verification
    verification_status VARCHAR(20) DEFAULT 'legacy_unverified',
    
    -- Timestamps
    reading_timestamp TIMESTAMPTZ DEFAULT NOW() NOT NULL,
    submitted_at TIMESTAMPTZ DEFAULT NOW(),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    
    -- Primary key must include partition key
    PRIMARY KEY (id, reading_timestamp)
) PARTITION BY RANGE (reading_timestamp);

-- Step 2: Create partitions for recent months
CREATE TABLE meter_readings_2024_11 PARTITION OF meter_readings_partitioned
    FOR VALUES FROM ('2024-11-01 00:00:00+00') TO ('2024-12-01 00:00:00+00');

CREATE TABLE meter_readings_2024_12 PARTITION OF meter_readings_partitioned
    FOR VALUES FROM ('2024-12-01 00:00:00+00') TO ('2025-01-01 00:00:00+00');

CREATE TABLE meter_readings_2025_01 PARTITION OF meter_readings_partitioned
    FOR VALUES FROM ('2025-01-01 00:00:00+00') TO ('2025-02-01 00:00:00+00');

CREATE TABLE meter_readings_2025_02 PARTITION OF meter_readings_partitioned
    FOR VALUES FROM ('2025-02-01 00:00:00+00') TO ('2025-03-01 00:00:00+00');

CREATE TABLE meter_readings_2025_03 PARTITION OF meter_readings_partitioned
    FOR VALUES FROM ('2025-03-01 00:00:00+00') TO ('2025-04-01 00:00:00+00');

CREATE TABLE meter_readings_default PARTITION OF meter_readings_partitioned DEFAULT;

-- Step 3: Create indexes
CREATE INDEX idx_meter_readings_part_blockchain_status ON meter_readings_partitioned(blockchain_status);
CREATE INDEX idx_meter_readings_part_blockchain_submitted ON meter_readings_partitioned(blockchain_submitted_at);
CREATE INDEX idx_meter_readings_part_meter ON meter_readings_partitioned(meter_serial);
CREATE INDEX idx_meter_readings_part_mint_tx ON meter_readings_partitioned(mint_tx_signature);
CREATE INDEX idx_meter_readings_part_on_chain_confirmed ON meter_readings_partitioned(on_chain_confirmed);
CREATE INDEX idx_meter_readings_part_on_chain_slot ON meter_readings_partitioned(on_chain_slot);
CREATE INDEX idx_meter_readings_part_reading_timestamp ON meter_readings_partitioned(reading_timestamp);
CREATE INDEX idx_meter_readings_part_registered ON meter_readings_partitioned(blockchain_registered);
CREATE INDEX idx_meter_readings_part_timestamp ON meter_readings_partitioned(timestamp);
CREATE INDEX idx_meter_readings_part_tx_signature ON meter_readings_partitioned(blockchain_tx_signature);
CREATE INDEX idx_meter_readings_part_tx_type ON meter_readings_partitioned(blockchain_tx_type);
CREATE INDEX idx_meter_readings_part_user ON meter_readings_partitioned(user_id);
CREATE INDEX idx_meter_readings_part_wallet ON meter_readings_partitioned(wallet_address);

-- Step 4: Copy existing data
INSERT INTO meter_readings_partitioned (
    id, 
    meter_serial, 
    wallet_address, 
    timestamp, 
    energy_generated, 
    energy_consumed, 
    surplus_energy, 
    deficit_energy, 
    battery_level, 
    temperature, 
    voltage, 
    current, 
    created_at,
    reading_timestamp
)
SELECT 
    id, 
    meter_id, 
    wallet_address, 
    timestamp, 
    energy_generated, 
    energy_consumed, 
    surplus_energy, 
    deficit_energy, 
    battery_level, 
    temperature, 
    voltage, 
    current, 
    created_at,
    timestamp
FROM meter_readings;

-- Step 5: Verify data migration
DO $$
DECLARE
    old_count BIGINT;
    new_count BIGINT;
BEGIN
    SELECT COUNT(*) INTO old_count FROM meter_readings;
    SELECT COUNT(*) INTO new_count FROM meter_readings_partitioned;
    
    IF old_count != new_count THEN
        RAISE EXCEPTION 'Data migration failed: old_count=%, new_count=%', old_count, new_count;
    END IF;
    
    RAISE NOTICE 'Data migration successful: % rows migrated', new_count;
END $$;

-- Step 6: Rename tables
ALTER TABLE meter_readings RENAME TO meter_readings_old_backup;
ALTER TABLE meter_readings_partitioned RENAME TO meter_readings;

-- Step 7: Add foreign key constraints
ALTER TABLE meter_readings 
    ADD CONSTRAINT meter_readings_meter_id_fkey 
    FOREIGN KEY (meter_id) REFERENCES meter_registry(id) ON DELETE SET NULL;

ALTER TABLE meter_readings 
    ADD CONSTRAINT meter_readings_user_id_fkey 
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL;

-- Step 8: Recreate triggers
CREATE TRIGGER update_meter_readings_blockchain_status 
    BEFORE UPDATE ON meter_readings 
    FOR EACH ROW EXECUTE FUNCTION update_blockchain_status_on_hash();

CREATE TRIGGER update_meter_readings_updated_at 
    BEFORE UPDATE ON meter_readings 
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Step 9: Update minting_retry_queue foreign key
ALTER TABLE minting_retry_queue 
    DROP CONSTRAINT IF EXISTS minting_retry_queue_reading_id_fkey;

-- ALTER TABLE minting_retry_queue 
--    ADD CONSTRAINT minting_retry_queue_reading_id_fkey 
--    FOREIGN KEY (reading_id) REFERENCES meter_readings(id) ON DELETE CASCADE;

-- Step 10: Create partition creation function
CREATE OR REPLACE FUNCTION create_meter_readings_partition(partition_date DATE)
RETURNS void AS $$
DECLARE
    partition_name TEXT;
    start_date TEXT;
    end_date TEXT;
BEGIN
    partition_name := 'meter_readings_' || TO_CHAR(partition_date, 'YYYY_MM');
    start_date := TO_CHAR(partition_date, 'YYYY-MM-01 00:00:00+00');
    end_date := TO_CHAR(partition_date + INTERVAL '1 month', 'YYYY-MM-01 00:00:00+00');
    
    IF NOT EXISTS (
        SELECT 1 FROM pg_class WHERE relname = partition_name
    ) THEN
        EXECUTE format(
            'CREATE TABLE %I PARTITION OF meter_readings FOR VALUES FROM (%L) TO (%L)',
            partition_name, start_date, end_date
        );
        RAISE NOTICE 'Created partition: %', partition_name;
    ELSE
        RAISE NOTICE 'Partition already exists: %', partition_name;
    END IF;
END;
$$ LANGUAGE plpgsql;

-- Step 11: Create future partitions
DO $$
DECLARE
    i INTEGER;
    partition_date DATE;
BEGIN
    FOR i IN 0..5 LOOP
        partition_date := DATE_TRUNC('month', CURRENT_DATE) + (i || ' months')::INTERVAL;
        PERFORM create_meter_readings_partition(partition_date);
    END LOOP;
END $$;

-- Step 12: Grant permissions
GRANT SELECT, INSERT, UPDATE, DELETE ON meter_readings TO gridtokenx;

-- Verification
SELECT 
    schemaname,
    tablename,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS size
FROM pg_tables
WHERE tablename LIKE 'meter_readings_%'
ORDER BY tablename;
