# Database Optimization Operations Guide

## Overview

This guide covers the operation and maintenance of database partitioning and archival features for the GridTokenX platform.

---

## Table Partitioning

### Overview

Two tables are partitioned for improved performance:
- **`meter_readings`** - Partitioned by `reading_timestamp` (monthly)
- **`blockchain_events`** - Partitioned by `slot` (1M slots per partition)

### Benefits

- ✅ Faster queries on recent data (partition pruning)
- ✅ Easier data archival and maintenance
- ✅ Better index performance
- ✅ Reduced table bloat

---

## Managing Partitions

### View Existing Partitions

```sql
-- List meter_readings partitions
SELECT 
    tablename,
    pg_size_pretty(pg_total_relation_size('public.'||tablename)) AS size
FROM pg_tables
WHERE tablename LIKE 'meter_readings_%'
ORDER BY tablename;

-- List blockchain_events partitions
SELECT 
    tablename,
    pg_size_pretty(pg_total_relation_size('public.'||tablename)) AS size
FROM pg_tables
WHERE tablename LIKE 'blockchain_events_%'
ORDER BY tablename;
```

### Check Data Distribution

```sql
-- Meter readings distribution
SELECT 
    tableoid::regclass AS partition_name,
    COUNT(*) AS row_count,
    MIN(reading_timestamp) AS min_timestamp,
    MAX(reading_timestamp) AS max_timestamp
FROM meter_readings
GROUP BY tableoid
ORDER BY partition_name;

-- Blockchain events distribution
SELECT 
    tableoid::regclass AS partition_name,
    COUNT(*) AS row_count,
    MIN(slot) AS min_slot,
    MAX(slot) AS max_slot
FROM blockchain_events
GROUP BY tableoid
ORDER BY partition_name;
```

### Create New Partitions

#### Meter Readings (Monthly)

```sql
-- Create partition for a specific month
SELECT create_meter_readings_partition(DATE '2025-06-01');

-- Create partitions for next 6 months
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
```

#### Blockchain Events (Slot-based)

```sql
-- Create partition for specific slot range
SELECT create_blockchain_events_partition(10000000);  -- Creates 10M-11M partition
```

### Automated Partition Creation

Set up a monthly cron job to create future partitions:

```bash
# Add to crontab
0 0 1 * * docker exec gridtokenx-postgres psql -U gridtokenx -d gridtokenx -c "SELECT create_meter_readings_partition(DATE_TRUNC('month', CURRENT_DATE + INTERVAL '3 months'));"
```

---

## Data Archival

### Overview

Archive tables store historical data older than 90 days:
- `meter_readings_archive`
- `market_epochs_archive`
- `settlements_archive`
- `trading_orders_archive`

### Manual Archival

#### Archive All Data

```sql
-- Run complete archival process (90 days retention)
SELECT * FROM run_archival_process(90);
```

#### Archive Specific Tables

```sql
-- Archive meter readings only
SELECT * FROM archive_old_meter_readings(90);

-- Archive market epochs only
SELECT * FROM archive_old_epochs(90);

-- Archive settlements only
SELECT * FROM archive_old_settlements(90);

-- Archive trading orders only
SELECT * FROM archive_old_trading_orders(90);
```

#### Custom Retention Period

```sql
-- Archive data older than 180 days
SELECT * FROM run_archival_process(180);
```

### Automated Archival

Set up a daily cron job to run archival:

```bash
# Add to crontab (runs daily at 2 AM)
0 2 * * * docker exec gridtokenx-postgres psql -U gridtokenx -d gridtokenx -c "SELECT * FROM run_archival_process(90);"
```

### Query Archived Data

#### Combined View

```sql
-- Query both current and archived data
SELECT * FROM meter_readings_all
WHERE wallet_address = 'YOUR_WALLET'
ORDER BY reading_timestamp DESC;
```

#### Archive Tables Directly

```sql
-- Query archive directly
SELECT * FROM meter_readings_archive
WHERE reading_timestamp BETWEEN '2024-01-01' AND '2024-03-31';
```

---

## Monitoring

### Partition Health

```sql
-- Check partition sizes
SELECT 
    schemaname,
    tablename,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS size,
    pg_total_relation_size(schemaname||'.'||tablename) AS bytes
FROM pg_tables
WHERE tablename LIKE 'meter_readings_%'
OR tablename LIKE 'blockchain_events_%'
ORDER BY bytes DESC;
```

### Archive Statistics

```sql
-- Archive table statistics
SELECT 
    'meter_readings' AS table_name,
    (SELECT COUNT(*) FROM meter_readings) AS current_count,
    (SELECT COUNT(*) FROM meter_readings_archive) AS archived_count,
    pg_size_pretty(pg_total_relation_size('meter_readings')) AS current_size,
    pg_size_pretty(pg_total_relation_size('meter_readings_archive')) AS archive_size
UNION ALL
SELECT 
    'market_epochs',
    (SELECT COUNT(*) FROM market_epochs),
    (SELECT COUNT(*) FROM market_epochs_archive),
    pg_size_pretty(pg_total_relation_size('market_epochs')),
    pg_size_pretty(pg_total_relation_size('market_epochs_archive'));
```

### Query Performance

```sql
-- Check if partition pruning is working
EXPLAIN (ANALYZE, BUFFERS)
SELECT COUNT(*) FROM meter_readings
WHERE reading_timestamp > NOW() - INTERVAL '7 days';

-- Should show: "Partitions scanned: 1" or similar
```

---

## Maintenance

### Vacuum Partitions

```sql
-- Vacuum specific partition
VACUUM ANALYZE meter_readings_2024_11;

-- Vacuum all partitions
VACUUM ANALYZE meter_readings;
```

### Drop Old Partitions

```sql
-- Drop partition (after archiving data)
DROP TABLE meter_readings_2024_01;
```

### Reindex Partitions

```sql
-- Reindex specific partition
REINDEX TABLE meter_readings_2024_11;

-- Reindex all partitions
REINDEX TABLE meter_readings;
```

---

## Testing

### Run Test Scripts

```bash
# Test partitioning
cd /Users/chanthawat/Developments/gridtokenx-platform/gridtokenx-apigateway
./scripts/test-partitioning.sh

# Test archival
./scripts/test-archival.sh
```

### Manual Testing

```sql
-- Test insert routing
INSERT INTO meter_readings (
    wallet_address, reading_timestamp, energy_generated
) VALUES (
    'TEST_WALLET', NOW(), 100.0
);

-- Check which partition it went to
SELECT tableoid::regclass FROM meter_readings 
WHERE wallet_address = 'TEST_WALLET';

-- Cleanup
DELETE FROM meter_readings WHERE wallet_address = 'TEST_WALLET';
```

---

## Troubleshooting

### Partition Not Found Error

**Problem**: Insert fails with "no partition found"

**Solution**: Create missing partition

```sql
SELECT create_meter_readings_partition(DATE_TRUNC('month', CURRENT_DATE));
```

### Slow Queries

**Problem**: Queries are slower than expected

**Solution**: Check if partition pruning is working

```sql
EXPLAIN SELECT * FROM meter_readings
WHERE reading_timestamp > '2024-11-01';
-- Should show limited partitions scanned
```

### Archive Process Fails

**Problem**: Archival function returns error

**Solution**: Check for foreign key constraints or locks

```sql
-- Check for locks
SELECT * FROM pg_locks WHERE relation::regclass::text LIKE 'meter_readings%';

-- Check for active transactions
SELECT * FROM pg_stat_activity WHERE state = 'active';
```

---

## Best Practices

### Partition Management

1. **Create partitions in advance** - Always have 2-3 months of future partitions
2. **Monitor partition sizes** - Alert if partitions grow too large
3. **Regular maintenance** - Vacuum and analyze partitions monthly
4. **Archive before dropping** - Always archive data before dropping old partitions

### Archival

1. **Regular schedule** - Run archival daily during low-traffic hours
2. **Monitor archive growth** - Track archive table sizes
3. **Verify before deletion** - Always verify data in archive before removing from main tables
4. **Backup archives** - Include archive tables in backup strategy

### Performance

1. **Use partition pruning** - Always include partition key in WHERE clauses
2. **Monitor query plans** - Regularly check EXPLAIN output
3. **Index maintenance** - Reindex partitions as needed
4. **Statistics updates** - Run ANALYZE after bulk operations

---

## SQL Function Reference

### Partition Functions

| Function | Purpose | Example |
|----------|---------|---------|
| `create_meter_readings_partition(DATE)` | Create monthly partition | `SELECT create_meter_readings_partition('2025-06-01')` |
| `create_blockchain_events_partition(BIGINT)` | Create slot partition | `SELECT create_blockchain_events_partition(10000000)` |

### Archival Functions

| Function | Purpose | Example |
|----------|---------|---------|
| `run_archival_process(INTEGER)` | Archive all tables | `SELECT * FROM run_archival_process(90)` |
| `archive_old_meter_readings(INTEGER)` | Archive meter readings | `SELECT * FROM archive_old_meter_readings(90)` |
| `archive_old_epochs(INTEGER)` | Archive market epochs | `SELECT * FROM archive_old_epochs(90)` |
| `archive_old_settlements(INTEGER)` | Archive settlements | `SELECT * FROM archive_old_settlements(90)` |
| `archive_old_trading_orders(INTEGER)` | Archive trading orders | `SELECT * FROM archive_old_trading_orders(90)` |

---

## Monitoring Queries

### Daily Health Check

```sql
-- Run this daily to monitor partition and archive health
SELECT 
    'Partitions' AS category,
    COUNT(*) AS count
FROM pg_tables
WHERE tablename LIKE 'meter_readings_%'
UNION ALL
SELECT 
    'Archive Tables',
    COUNT(*)
FROM pg_tables
WHERE tablename LIKE '%_archive'
UNION ALL
SELECT 
    'Current Readings',
    COUNT(*)::TEXT
FROM meter_readings
UNION ALL
SELECT 
    'Archived Readings',
    COUNT(*)::TEXT
FROM meter_readings_archive;
```

---

**Last Updated**: 2025-11-28  
**Version**: 1.0.0
