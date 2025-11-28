-- Migration: Autovacuum Tuning for High-Churn Tables
-- Created: 2024-11-28
-- Description: Configure autovacuum settings for tables with frequent updates

-- =========================================================================
-- TRADING ORDERS - High update frequency during active trading
-- =========================================================================

ALTER TABLE trading_orders SET (
    -- Trigger vacuum when 5% of rows are dead (default is 20%)
    autovacuum_vacuum_scale_factor = 0.05,
    
    -- Trigger analyze when 2% of rows change (default is 10%)
    autovacuum_analyze_scale_factor = 0.02,
    
    -- Minimum number of updated rows before vacuum
    autovacuum_vacuum_threshold = 50,
    
    -- Minimum number of updated rows before analyze
    autovacuum_analyze_threshold = 25,
    
    -- Cost delay for autovacuum (lower = faster, more aggressive)
    autovacuum_vacuum_cost_delay = 10
);

-- =========================================================================
-- ORDER MATCHES - Frequent updates during order matching
-- =========================================================================

ALTER TABLE order_matches SET (
    autovacuum_vacuum_scale_factor = 0.05,
    autovacuum_analyze_scale_factor = 0.02,
    autovacuum_vacuum_threshold = 50,
    autovacuum_analyze_threshold = 25,
    autovacuum_vacuum_cost_delay = 10
);

-- =========================================================================
-- SETTLEMENTS - Updated during settlement processing
-- =========================================================================

ALTER TABLE settlements SET (
    autovacuum_vacuum_scale_factor = 0.05,
    autovacuum_analyze_scale_factor = 0.02,
    autovacuum_vacuum_threshold = 50,
    autovacuum_analyze_threshold = 25,
    autovacuum_vacuum_cost_delay = 10
);

-- =========================================================================
-- MARKET EPOCHS - Updated as epochs progress
-- =========================================================================

ALTER TABLE market_epochs SET (
    autovacuum_vacuum_scale_factor = 0.1,
    autovacuum_analyze_scale_factor = 0.05,
    autovacuum_vacuum_threshold = 25,
    autovacuum_analyze_threshold = 10
);

-- =========================================================================
-- TABLE BLOAT MONITORING VIEW
-- =========================================================================

CREATE OR REPLACE VIEW v_table_bloat AS
SELECT
    schemaname,
    tablename,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS total_size,
    pg_size_pretty(pg_relation_size(schemaname||'.'||tablename)) AS table_size,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename) - pg_relation_size(schemaname||'.'||tablename)) AS indexes_size,
    n_live_tup AS live_tuples,
    n_dead_tup AS dead_tuples,
    ROUND(100.0 * n_dead_tup / NULLIF(n_live_tup + n_dead_tup, 0), 2) AS dead_tuple_percent,
    last_vacuum,
    last_autovacuum,
    last_analyze,
    last_autoanalyze
FROM pg_stat_user_tables
WHERE schemaname = 'public'
ORDER BY n_dead_tup DESC;

-- =========================================================================
-- AUTOVACUUM SETTINGS VIEW
-- =========================================================================

CREATE OR REPLACE VIEW v_autovacuum_settings AS
SELECT
    c.relname AS table_name,
    COALESCE(
        (SELECT option_value FROM pg_options_to_table(c.reloptions) WHERE option_name = 'autovacuum_vacuum_scale_factor'),
        current_setting('autovacuum_vacuum_scale_factor')
    ) AS vacuum_scale_factor,
    COALESCE(
        (SELECT option_value FROM pg_options_to_table(c.reloptions) WHERE option_name = 'autovacuum_analyze_scale_factor'),
        current_setting('autovacuum_analyze_scale_factor')
    ) AS analyze_scale_factor,
    COALESCE(
        (SELECT option_value FROM pg_options_to_table(c.reloptions) WHERE option_name = 'autovacuum_vacuum_threshold'),
        current_setting('autovacuum_vacuum_threshold')
    ) AS vacuum_threshold,
    COALESCE(
        (SELECT option_value FROM pg_options_to_table(c.reloptions) WHERE option_name = 'autovacuum_analyze_threshold'),
        current_setting('autovacuum_analyze_threshold')
    ) AS analyze_threshold
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = 'public'
AND c.relkind = 'r'
ORDER BY c.relname;

-- =========================================================================
-- VACUUM MONITORING FUNCTION
-- =========================================================================

CREATE OR REPLACE FUNCTION check_vacuum_needed()
RETURNS TABLE(
    table_name TEXT,
    live_tuples BIGINT,
    dead_tuples BIGINT,
    dead_percent NUMERIC,
    vacuum_recommended BOOLEAN,
    last_vacuum TIMESTAMPTZ,
    last_autovacuum TIMESTAMPTZ
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        schemaname || '.' || tablename AS table_name,
        n_live_tup,
        n_dead_tup,
        ROUND(100.0 * n_dead_tup / NULLIF(n_live_tup + n_dead_tup, 0), 2),
        (n_dead_tup > 1000 AND 
         100.0 * n_dead_tup / NULLIF(n_live_tup + n_dead_tup, 0) > 10) AS vacuum_recommended,
        pg_stat_user_tables.last_vacuum,
        pg_stat_user_tables.last_autovacuum
    FROM pg_stat_user_tables
    WHERE schemaname = 'public'
    ORDER BY n_dead_tup DESC;
END;
$$ LANGUAGE plpgsql;

-- =========================================================================
-- MANUAL VACUUM HELPER FUNCTION
-- =========================================================================

CREATE OR REPLACE FUNCTION vacuum_high_churn_tables()
RETURNS void AS $$
BEGIN
    -- Vacuum and analyze high-churn tables
    RAISE NOTICE 'Vacuuming trading_orders...';
    VACUUM (VERBOSE, ANALYZE) trading_orders;
    
    RAISE NOTICE 'Vacuuming order_matches...';
    VACUUM (VERBOSE, ANALYZE) order_matches;
    
    RAISE NOTICE 'Vacuuming settlements...';
    VACUUM (VERBOSE, ANALYZE) settlements;
    
    RAISE NOTICE 'Vacuuming market_epochs...';
    VACUUM (VERBOSE, ANALYZE) market_epochs;
    
    RAISE NOTICE 'Vacuum complete!';
END;
$$ LANGUAGE plpgsql;

-- =========================================================================
-- VERIFICATION QUERIES
-- =========================================================================

-- Check autovacuum settings for all tables
-- SELECT * FROM v_autovacuum_settings;

-- Check table bloat
-- SELECT * FROM v_table_bloat WHERE dead_tuple_percent > 10;

-- Check if vacuum is needed
-- SELECT * FROM check_vacuum_needed() WHERE vacuum_recommended = true;

-- Manually vacuum high-churn tables if needed
-- SELECT vacuum_high_churn_tables();

-- Check autovacuum activity
-- SELECT
--     schemaname,
--     tablename,
--     last_autovacuum,
--     last_autoanalyze,
--     autovacuum_count,
--     autoanalyze_count
-- FROM pg_stat_user_tables
-- WHERE schemaname = 'public'
-- ORDER BY last_autovacuum DESC NULLS LAST;
