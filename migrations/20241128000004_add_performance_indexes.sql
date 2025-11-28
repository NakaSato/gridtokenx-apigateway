-- Migration: Add Performance Indexes
-- Created: 2024-11-28
-- Description: Add optimized indexes for common query patterns

-- =========================================================================
-- USERS TABLE - Optimized Indexes
-- =========================================================================

-- Case-insensitive email lookup (replace existing index)
DROP INDEX IF EXISTS idx_users_email;
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email ON users(lower(email));

-- Wallet address lookup is already indexed

-- =========================================================================
-- METER READINGS TABLE - Time-Series Optimized Indexes
-- =========================================================================

-- BRIN index for timestamp (efficient for time-series data)
DROP INDEX IF EXISTS idx_meter_readings_timestamp;
CREATE INDEX IF NOT EXISTS idx_meter_readings_timestamp ON meter_readings USING BRIN(timestamp);

-- Composite index for meter + timestamp queries (most common pattern)
CREATE INDEX IF NOT EXISTS idx_meter_readings_meter_timestamp ON meter_readings(meter_id, timestamp DESC);

-- Wallet address with timestamp
CREATE INDEX IF NOT EXISTS idx_meter_readings_wallet_timestamp ON meter_readings(wallet_address, timestamp DESC);

-- =========================================================================
-- TRADING ORDERS TABLE - Active Orders Optimization
-- =========================================================================

-- Partial index for active orders (most frequently queried)
CREATE INDEX IF NOT EXISTS idx_trading_orders_active ON trading_orders(status, price_per_kwh)
WHERE status IN ('pending', 'active', 'partially_filled');

-- Composite index for user's orders by date
CREATE INDEX IF NOT EXISTS idx_trading_orders_user_created ON trading_orders(user_id, created_at DESC);

-- Index for order matching queries
CREATE INDEX IF NOT EXISTS idx_trading_orders_type_status_price ON trading_orders(order_type, status, price_per_kwh)
WHERE status IN ('active', 'partially_filled');

-- =========================================================================
-- ORDER MATCHES TABLE - Settlement Queries
-- =========================================================================

-- Composite index for epoch settlement queries
CREATE INDEX IF NOT EXISTS idx_order_matches_epoch_status ON order_matches(epoch_id, status);

-- Index for buyer/seller match lookups
CREATE INDEX IF NOT EXISTS idx_order_matches_orders ON order_matches(buy_order_id, sell_order_id);

-- =========================================================================
-- SETTLEMENTS TABLE - Transaction Lookups
-- =========================================================================

-- Composite index for user settlement history
CREATE INDEX IF NOT EXISTS idx_settlements_buyer_created ON settlements(buyer_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_settlements_seller_created ON settlements(seller_id, created_at DESC);

-- Index for pending settlements
CREATE INDEX IF NOT EXISTS idx_settlements_pending ON settlements(status, created_at)
WHERE status IN ('pending', 'processing');

-- =========================================================================
-- USER ACTIVITIES TABLE - Audit Log Optimization
-- =========================================================================

-- BRIN index for timestamp (efficient for append-only audit logs)
DROP INDEX IF EXISTS idx_user_activities_created;
CREATE INDEX IF NOT EXISTS idx_user_activities_created ON user_activities USING BRIN(created_at);

-- Composite index for user activity history
CREATE INDEX IF NOT EXISTS idx_user_activities_user_created ON user_activities(user_id, created_at DESC);

-- Index for activity type filtering
CREATE INDEX IF NOT EXISTS idx_user_activities_type_created ON user_activities(activity_type, created_at DESC);

-- =========================================================================
-- ENERGY CERTIFICATES TABLE - Certificate Lookups
-- =========================================================================

-- Composite index for active certificates by wallet
CREATE INDEX IF NOT EXISTS idx_certificates_wallet_status ON erc_certificates(wallet_address, status)
WHERE status = 'active';

-- Index for expiring certificates
CREATE INDEX IF NOT EXISTS idx_certificates_expiry ON erc_certificates(expiry_date)
WHERE status = 'active' AND expiry_date IS NOT NULL;

-- =========================================================================
-- MARKET EPOCHS TABLE - Epoch Queries
-- =========================================================================

-- Composite index for active/pending epochs
CREATE INDEX IF NOT EXISTS idx_market_epochs_status_time ON market_epochs(status, start_time DESC);

-- =========================================================================
-- JSONB INDEXES (if metadata columns are used)
-- =========================================================================

-- GIN index for energy certificate metadata searches
CREATE INDEX IF NOT EXISTS idx_certificates_metadata ON erc_certificates USING GIN(metadata jsonb_path_ops)
WHERE metadata IS NOT NULL;

-- GIN index for user activity metadata
CREATE INDEX IF NOT EXISTS idx_user_activities_metadata ON user_activities USING GIN(metadata jsonb_path_ops)
WHERE metadata IS NOT NULL;

-- =========================================================================
-- COVERING INDEXES (Include frequently accessed columns)
-- =========================================================================

-- Trading orders with commonly accessed fields
CREATE INDEX IF NOT EXISTS idx_trading_orders_user_covering ON trading_orders(user_id, status)
INCLUDE (order_type, energy_amount, price_per_kwh, created_at);

-- =========================================================================
-- INDEX USAGE MONITORING VIEW
-- =========================================================================

-- Create view to monitor index usage
CREATE OR REPLACE VIEW v_index_usage AS
SELECT
    schemaname,
    relname as tablename,
    indexrelname as indexname,
    idx_scan as scans,
    idx_tup_read as tuples_read,
    idx_tup_fetch as tuples_fetched,
    pg_size_pretty(pg_relation_size(indexrelid)) as index_size
FROM pg_stat_user_indexes
WHERE schemaname = 'public'
ORDER BY idx_scan DESC;

-- =========================================================================
-- VERIFICATION QUERIES
-- =========================================================================

-- List all indexes on meter_readings
-- SELECT indexname, indexdef FROM pg_indexes WHERE tablename = 'meter_readings';

-- Check index usage statistics
-- SELECT * FROM v_index_usage WHERE tablename = 'meter_readings';

-- Verify BRIN index efficiency
-- SELECT * FROM pg_stat_user_indexes WHERE indexrelname LIKE '%brin%';
