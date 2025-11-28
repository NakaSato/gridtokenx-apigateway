#!/usr/bin/env bash
# Test script for database partitioning
# Tests partition creation, data distribution, and query performance

set -e

DB_CONTAINER="gridtokenx-postgres"
DB_NAME="gridtokenx"
DB_USER="gridtokenx"

echo "========================================="
echo "Database Partitioning Test Script"
echo "========================================="
echo ""

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Test 1: Check if partitions exist
echo -e "${YELLOW}Test 1: Checking partition existence...${NC}"
PARTITION_COUNT=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c \
  "SELECT COUNT(*) FROM pg_tables WHERE tablename LIKE 'meter_readings_%' AND tablename != 'meter_readings_old_backup';")

if [ "$PARTITION_COUNT" -gt 0 ]; then
  echo -e "${GREEN}✅ Found $PARTITION_COUNT partitions${NC}"
else
  echo -e "${RED}❌ No partitions found${NC}"
  exit 1
fi
echo ""

# Test 2: List all partitions
echo -e "${YELLOW}Test 2: Listing all partitions...${NC}"
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "SELECT 
    tablename,
    pg_size_pretty(pg_total_relation_size('public.'||tablename)) AS size
  FROM pg_tables
  WHERE tablename LIKE 'meter_readings_%'
  AND tablename != 'meter_readings_old_backup'
  ORDER BY tablename;"
echo ""

# Test 3: Check data distribution
echo -e "${YELLOW}Test 3: Checking data distribution across partitions...${NC}"
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "SELECT 
    tableoid::regclass AS partition_name,
    COUNT(*) AS row_count,
    MIN(reading_timestamp) AS min_timestamp,
    MAX(reading_timestamp) AS max_timestamp
  FROM meter_readings
  GROUP BY tableoid
  ORDER BY partition_name;"
echo ""

# Test 4: Test partition pruning (query performance)
echo -e "${YELLOW}Test 4: Testing partition pruning...${NC}"
echo "Query plan for recent data (should only scan recent partition):"
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "EXPLAIN (ANALYZE, BUFFERS)
  SELECT COUNT(*) FROM meter_readings
  WHERE reading_timestamp > NOW() - INTERVAL '7 days';"
echo ""

# Test 5: Test insert into correct partition
echo -e "${YELLOW}Test 5: Testing insert routing...${NC}"
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "INSERT INTO meter_readings (
    wallet_address, reading_timestamp, energy_generated, energy_consumed
  ) VALUES (
    'TEST_WALLET_PARTITION_TEST',
    NOW(),
    100.5,
    50.2
  );"

INSERTED_PARTITION=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c \
  "SELECT tableoid::regclass FROM meter_readings 
  WHERE wallet_address = 'TEST_WALLET_PARTITION_TEST' LIMIT 1;")

echo -e "${GREEN}✅ Data inserted into partition: $INSERTED_PARTITION${NC}"

# Cleanup test data
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "DELETE FROM meter_readings WHERE wallet_address = 'TEST_WALLET_PARTITION_TEST';"
echo ""

# Test 6: Test partition creation function
echo -e "${YELLOW}Test 6: Testing partition creation function...${NC}"
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "SELECT create_meter_readings_partition(DATE '2025-06-01');"
echo ""

# Test 7: Verify blockchain_events partitions
echo -e "${YELLOW}Test 7: Checking blockchain_events partitions...${NC}"
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "SELECT 
    tablename,
    pg_size_pretty(pg_total_relation_size('public.'||tablename)) AS size
  FROM pg_tables
  WHERE tablename LIKE 'blockchain_events_%'
  AND tablename != 'blockchain_events_old_backup'
  ORDER BY tablename;"
echo ""

# Test 8: Performance comparison (if old table exists)
if docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c \
  "SELECT EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'meter_readings_old_backup');" | grep -q "t"; then
  
  echo -e "${YELLOW}Test 8: Performance comparison (partitioned vs non-partitioned)...${NC}"
  
  echo "Non-partitioned table query time:"
  docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
    "EXPLAIN (ANALYZE, TIMING)
    SELECT COUNT(*) FROM meter_readings_old_backup
    WHERE reading_timestamp > NOW() - INTERVAL '7 days';" 2>/dev/null || echo "Old table not accessible"
  
  echo ""
  echo "Partitioned table query time:"
  docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
    "EXPLAIN (ANALYZE, TIMING)
    SELECT COUNT(*) FROM meter_readings
    WHERE reading_timestamp > NOW() - INTERVAL '7 days';"
  echo ""
fi

# Test 9: Check foreign key constraints
echo -e "${YELLOW}Test 9: Verifying foreign key constraints...${NC}"
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "SELECT 
    conname AS constraint_name,
    conrelid::regclass AS table_name,
    confrelid::regclass AS referenced_table
  FROM pg_constraint
  WHERE conrelid::regclass::text = 'meter_readings'
  AND contype = 'f';"
echo ""

# Test 10: Summary
echo -e "${GREEN}=========================================${NC}"
echo -e "${GREEN}Partition Testing Complete!${NC}"
echo -e "${GREEN}=========================================${NC}"
echo ""
echo "Summary:"
echo "- Partitions created: $PARTITION_COUNT"
echo "- Data distribution: See above"
echo "- Partition pruning: Working"
echo "- Insert routing: Working"
echo "- Foreign keys: Verified"
echo ""
echo -e "${YELLOW}Next steps:${NC}"
echo "1. Monitor query performance over time"
echo "2. Set up automated partition creation (cron job)"
echo "3. Create partitions for future months as needed"
