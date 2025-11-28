-- Create archive tables for historical data
-- Data older than 90 days will be moved to these tables

-- Archive table for meter readings
CREATE TABLE meter_readings_archive (
    LIKE meter_readings INCLUDING ALL
);

-- Add comment
COMMENT ON TABLE meter_readings_archive IS 'Archive for meter readings older than 90 days';

-- Archive table for market epochs
CREATE TABLE market_epochs_archive (
    LIKE market_epochs INCLUDING ALL
);

COMMENT ON TABLE market_epochs_archive IS 'Archive for settled market epochs older than 90 days';

-- Archive table for settlements
CREATE TABLE settlements_archive (
    LIKE settlements INCLUDING ALL
);

COMMENT ON TABLE settlements_archive IS 'Archive for completed settlements older than 90 days';

-- Archive table for trading orders
CREATE TABLE trading_orders_archive (
    LIKE trading_orders INCLUDING ALL
);

COMMENT ON TABLE trading_orders_archive IS 'Archive for settled trading orders older than 90 days';

-- Create indexes on archive tables
CREATE INDEX idx_meter_readings_archive_timestamp ON meter_readings_archive(reading_timestamp);
CREATE INDEX idx_meter_readings_archive_wallet ON meter_readings_archive(wallet_address);
CREATE INDEX idx_meter_readings_archive_user ON meter_readings_archive(user_id);

CREATE INDEX idx_market_epochs_archive_time ON market_epochs_archive(start_time);
CREATE INDEX idx_market_epochs_archive_number ON market_epochs_archive(epoch_number);

CREATE INDEX idx_settlements_archive_epoch ON settlements_archive(epoch_id);
CREATE INDEX idx_settlements_archive_created ON settlements_archive(created_at);

CREATE INDEX idx_trading_orders_archive_user ON trading_orders_archive(user_id);
CREATE INDEX idx_trading_orders_archive_created ON trading_orders_archive(created_at);

-- Create function to archive old meter readings
CREATE OR REPLACE FUNCTION archive_old_meter_readings(retention_days INTEGER DEFAULT 90)
RETURNS TABLE(archived_count BIGINT) AS $$
DECLARE
    cutoff_date TIMESTAMPTZ;
    rows_archived BIGINT;
BEGIN
    cutoff_date := NOW() - (retention_days || ' days')::INTERVAL;
    
    -- Insert old readings into archive
    WITH archived AS (
        INSERT INTO meter_readings_archive
        SELECT * FROM meter_readings
        WHERE reading_timestamp < cutoff_date
        RETURNING *
    )
    SELECT COUNT(*) INTO rows_archived FROM archived;
    
    -- Delete archived readings from main table
    DELETE FROM meter_readings
    WHERE reading_timestamp < cutoff_date;
    
    RAISE NOTICE 'Archived % meter readings older than %', rows_archived, cutoff_date;
    
    RETURN QUERY SELECT rows_archived;
END;
$$ LANGUAGE plpgsql;

-- Create function to archive old market epochs
CREATE OR REPLACE FUNCTION archive_old_epochs(retention_days INTEGER DEFAULT 90)
RETURNS TABLE(archived_count BIGINT) AS $$
DECLARE
    cutoff_date TIMESTAMPTZ;
    rows_archived BIGINT;
BEGIN
    cutoff_date := NOW() - (retention_days || ' days')::INTERVAL;
    
    -- Insert old settled epochs into archive
    WITH archived AS (
        INSERT INTO market_epochs_archive
        SELECT * FROM market_epochs
        WHERE end_time < cutoff_date
        AND status = 'settled'
        RETURNING *
    )
    SELECT COUNT(*) INTO rows_archived FROM archived;
    
    -- Delete archived epochs from main table
    DELETE FROM market_epochs
    WHERE end_time < cutoff_date
    AND status = 'settled';
    
    RAISE NOTICE 'Archived % market epochs older than %', rows_archived, cutoff_date;
    
    RETURN QUERY SELECT rows_archived;
END;
$$ LANGUAGE plpgsql;

-- Create function to archive old settlements
CREATE OR REPLACE FUNCTION archive_old_settlements(retention_days INTEGER DEFAULT 90)
RETURNS TABLE(archived_count BIGINT) AS $$
DECLARE
    cutoff_date TIMESTAMPTZ;
    rows_archived BIGINT;
BEGIN
    cutoff_date := NOW() - (retention_days || ' days')::INTERVAL;
    
    -- Insert old completed settlements into archive
    WITH archived AS (
        INSERT INTO settlements_archive
        SELECT * FROM settlements
        WHERE created_at < cutoff_date
        AND status = 'completed'
        RETURNING *
    )
    SELECT COUNT(*) INTO rows_archived FROM archived;
    
    -- Delete archived settlements from main table
    DELETE FROM settlements
    WHERE created_at < cutoff_date
    AND status = 'completed';
    
    RAISE NOTICE 'Archived % settlements older than %', rows_archived, cutoff_date;
    
    RETURN QUERY SELECT rows_archived;
END;
$$ LANGUAGE plpgsql;

-- Create function to archive old trading orders
CREATE OR REPLACE FUNCTION archive_old_trading_orders(retention_days INTEGER DEFAULT 90)
RETURNS TABLE(archived_count BIGINT) AS $$
DECLARE
    cutoff_date TIMESTAMPTZ;
    rows_archived BIGINT;
BEGIN
    cutoff_date := NOW() - (retention_days || ' days')::INTERVAL;
    
    -- Insert old settled orders into archive
    WITH archived AS (
        INSERT INTO trading_orders_archive
        SELECT * FROM trading_orders
        WHERE created_at < cutoff_date
        AND status IN ('settled', 'cancelled')
        RETURNING *
    )
    SELECT COUNT(*) INTO rows_archived FROM archived;
    
    -- Delete archived orders from main table
    DELETE FROM trading_orders
    WHERE created_at < cutoff_date
    AND status IN ('settled', 'cancelled');
    
    RAISE NOTICE 'Archived % trading orders older than %', rows_archived, cutoff_date;
    
    RETURN QUERY SELECT rows_archived;
END;
$$ LANGUAGE plpgsql;

-- Create master archival function
CREATE OR REPLACE FUNCTION run_archival_process(retention_days INTEGER DEFAULT 90)
RETURNS TABLE(
    table_name TEXT,
    archived_count BIGINT,
    archived_at TIMESTAMPTZ
) AS $$
DECLARE
    readings_count BIGINT;
    epochs_count BIGINT;
    settlements_count BIGINT;
    orders_count BIGINT;
BEGIN
    -- Archive meter readings
    SELECT * INTO readings_count FROM archive_old_meter_readings(retention_days);
    RETURN QUERY SELECT 'meter_readings'::TEXT, readings_count, NOW();
    
    -- Archive market epochs
    SELECT * INTO epochs_count FROM archive_old_epochs(retention_days);
    RETURN QUERY SELECT 'market_epochs'::TEXT, epochs_count, NOW();
    
    -- Archive settlements
    SELECT * INTO settlements_count FROM archive_old_settlements(retention_days);
    RETURN QUERY SELECT 'settlements'::TEXT, settlements_count, NOW();
    
    -- Archive trading orders
    SELECT * INTO orders_count FROM archive_old_trading_orders(retention_days);
    RETURN QUERY SELECT 'trading_orders'::TEXT, orders_count, NOW();
    
    RAISE NOTICE 'Archival process completed';
END;
$$ LANGUAGE plpgsql;

-- Grant permissions
GRANT SELECT ON meter_readings_archive TO gridtokenx;
GRANT SELECT ON market_epochs_archive TO gridtokenx;
GRANT SELECT ON settlements_archive TO gridtokenx;
GRANT SELECT ON trading_orders_archive TO gridtokenx;

-- Create view to query both current and archived meter readings
CREATE OR REPLACE VIEW meter_readings_all AS
SELECT *, false AS is_archived FROM meter_readings
UNION ALL
SELECT *, true AS is_archived FROM meter_readings_archive;

COMMENT ON VIEW meter_readings_all IS 'Combined view of current and archived meter readings';

-- Test archival (dry run - commented out)
-- SELECT * FROM run_archival_process(90);
