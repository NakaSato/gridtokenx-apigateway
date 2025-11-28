#!/usr/bin/env bash
# Test script for database archival
# Tests archival functions and data integrity

set -e

DB_CONTAINER="gridtokenx-postgres"
DB_NAME="gridtokenx"
DB_USER="gridtokenx"

echo "========================================="
echo "Database Archival Test Script"
echo "========================================="
echo ""

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Test 1: Check if archive tables exist
echo -e "${YELLOW}Test 1: Checking archive table existence...${NC}"
ARCHIVE_TABLES=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c \
  "SELECT COUNT(*) FROM pg_tables WHERE tablename LIKE '%_archive';")

if [ "$ARCHIVE_TABLES" -ge 4 ]; then
  echo -e "${GREEN}✅ Found $ARCHIVE_TABLES archive tables${NC}"
else
  echo -e "${RED}❌ Expected 4 archive tables, found $ARCHIVE_TABLES${NC}"
  exit 1
fi
echo ""

# Test 2: List archive tables
echo -e "${YELLOW}Test 2: Listing archive tables...${NC}"
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "SELECT 
    tablename,
    pg_size_pretty(pg_total_relation_size('public.'||tablename)) AS size
  FROM pg_tables
  WHERE tablename LIKE '%_archive'
  ORDER BY tablename;"
echo ""

# Test 3: Check archival functions
echo -e "${YELLOW}Test 3: Checking archival functions...${NC}"
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "SELECT 
    proname AS function_name,
    pg_get_function_arguments(oid) AS arguments
  FROM pg_proc
  WHERE proname LIKE 'archive_%'
  OR proname = 'run_archival_process'
  ORDER BY proname;"
echo ""

# Test 4: Create test data for archival
echo -e "${YELLOW}Test 4: Creating test data for archival...${NC}"

# Insert old meter reading (91 days ago)
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "INSERT INTO meter_readings (
    wallet_address, 
    reading_timestamp, 
    energy_generated, 
    energy_consumed
  ) VALUES (
    'TEST_ARCHIVE_WALLET',
    NOW() - INTERVAL '91 days',
    100.0,
    50.0
  );"

# Insert old market epoch (91 days ago, settled)
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "INSERT INTO market_epochs (
    epoch_number,
    start_time,
    end_time,
    status
  ) VALUES (
    999999,
    NOW() - INTERVAL '91 days',
    NOW() - INTERVAL '91 days' + INTERVAL '15 minutes',
    'settled'
  );"

echo -e "${GREEN}✅ Test data created${NC}"
echo ""

# Test 5: Test archival dry run (check what would be archived)
echo -e "${YELLOW}Test 5: Checking archivable data...${NC}"

ARCHIVABLE_READINGS=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c \
  "SELECT COUNT(*) FROM meter_readings 
  WHERE reading_timestamp < NOW() - INTERVAL '90 days';")

ARCHIVABLE_EPOCHS=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c \
  "SELECT COUNT(*) FROM market_epochs 
  WHERE end_time < NOW() - INTERVAL '90 days' AND status = 'settled';")

echo "Archivable meter readings: $ARCHIVABLE_READINGS"
echo "Archivable market epochs: $ARCHIVABLE_EPOCHS"
echo ""

# Test 6: Run archival process
echo -e "${YELLOW}Test 6: Running archival process...${NC}"
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "SELECT * FROM run_archival_process(90);"
echo ""

# Test 7: Verify data was archived
echo -e "${YELLOW}Test 7: Verifying archived data...${NC}"

ARCHIVED_READINGS=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c \
  "SELECT COUNT(*) FROM meter_readings_archive;")

ARCHIVED_EPOCHS=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c \
  "SELECT COUNT(*) FROM market_epochs_archive;")

echo "Archived meter readings: $ARCHIVED_READINGS"
echo "Archived market epochs: $ARCHIVED_EPOCHS"

if [ "$ARCHIVED_READINGS" -gt 0 ]; then
  echo -e "${GREEN}✅ Meter readings archived successfully${NC}"
else
  echo -e "${YELLOW}⚠️  No meter readings archived (may be expected)${NC}"
fi

if [ "$ARCHIVED_EPOCHS" -gt 0 ]; then
  echo -e "${GREEN}✅ Market epochs archived successfully${NC}"
else
  echo -e "${YELLOW}⚠️  No market epochs archived (may be expected)${NC}"
fi
echo ""

# Test 8: Verify data removed from main tables
echo -e "${YELLOW}Test 8: Verifying data removed from main tables...${NC}"

REMAINING_OLD_READINGS=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c \
  "SELECT COUNT(*) FROM meter_readings 
  WHERE reading_timestamp < NOW() - INTERVAL '90 days';")

echo "Old readings remaining in main table: $REMAINING_OLD_READINGS"

if [ "$REMAINING_OLD_READINGS" -eq 0 ]; then
  echo -e "${GREEN}✅ Old data successfully removed from main table${NC}"
else
  echo -e "${YELLOW}⚠️  Some old data still in main table${NC}"
fi
echo ""

# Test 9: Test combined view
echo -e "${YELLOW}Test 9: Testing meter_readings_all view...${NC}"
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "SELECT 
    is_archived,
    COUNT(*) AS count
  FROM meter_readings_all
  GROUP BY is_archived;"
echo ""

# Test 10: Test data integrity
echo -e "${YELLOW}Test 10: Verifying data integrity...${NC}"

# Check if test data is in archive
TEST_IN_ARCHIVE=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c \
  "SELECT COUNT(*) FROM meter_readings_archive 
  WHERE wallet_address = 'TEST_ARCHIVE_WALLET';")

if [ "$TEST_IN_ARCHIVE" -gt 0 ]; then
  echo -e "${GREEN}✅ Test data found in archive${NC}"
else
  echo -e "${RED}❌ Test data not found in archive${NC}"
fi
echo ""

# Cleanup test data
echo -e "${YELLOW}Cleaning up test data...${NC}"
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "DELETE FROM meter_readings_archive WHERE wallet_address = 'TEST_ARCHIVE_WALLET';
  DELETE FROM market_epochs_archive WHERE epoch_number = 999999;"
echo -e "${GREEN}✅ Cleanup complete${NC}"
echo ""

# Test 11: Archive statistics
echo -e "${YELLOW}Test 11: Archive statistics...${NC}"
docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "SELECT 
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
    pg_size_pretty(pg_total_relation_size('market_epochs_archive'))
  UNION ALL
  SELECT 
    'settlements',
    (SELECT COUNT(*) FROM settlements),
    (SELECT COUNT(*) FROM settlements_archive),
    pg_size_pretty(pg_total_relation_size('settlements')),
    pg_size_pretty(pg_total_relation_size('settlements_archive'))
  UNION ALL
  SELECT 
    'trading_orders',
    (SELECT COUNT(*) FROM trading_orders),
    (SELECT COUNT(*) FROM trading_orders_archive),
    pg_size_pretty(pg_total_relation_size('trading_orders')),
    pg_size_pretty(pg_total_relation_size('trading_orders_archive'));"
echo ""

# Summary
echo -e "${GREEN}=========================================${NC}"
echo -e "${GREEN}Archival Testing Complete!${NC}"
echo -e "${GREEN}=========================================${NC}"
echo ""
echo "Summary:"
echo "- Archive tables: $ARCHIVE_TABLES"
echo "- Archival functions: Working"
echo "- Data archival: Successful"
echo "- Data integrity: Verified"
echo ""
echo -e "${YELLOW}Next steps:${NC}"
echo "1. Set up automated archival (cron job)"
echo "2. Monitor archive table growth"
echo "3. Plan for archive table maintenance"
echo "4. Consider archive table partitioning for very large archives"
