#!/bin/bash

# Test Script for Event Processor Service
# This script tests the blockchain event synchronization functionality

set -e

# Configuration
DB_CONTAINER="p2p-postgres"
DB_USER="gridtokenx_user"
DB_NAME="gridtokenx"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo "================================================================================"
echo "EVENT PROCESSOR SERVICE TEST"
echo "================================================================================"
echo ""

echo "--------------------------------------------------------------------------------"
echo "1. Checking Database Schema"
echo "--------------------------------------------------------------------------------"

# Check if blockchain_events table exists
EVENTS_TABLE=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c \
  "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = 'blockchain_events';" | tr -d ' ')

if [ "$EVENTS_TABLE" -eq "1" ]; then
    echo -e "${GREEN}✅ blockchain_events table exists${NC}"
else
    echo -e "${RED}❌ blockchain_events table not found${NC}"
    exit 1
fi

# Check if event_processing_state table exists
STATE_TABLE=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c \
  "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = 'event_processing_state';" | tr -d ' ')

if [ "$STATE_TABLE" -eq "1" ]; then
    echo -e "${GREEN}✅ event_processing_state table exists${NC}"
else
    echo -e "${RED}❌ event_processing_state table not found${NC}"
    exit 1
fi

# Check if on_chain_confirmed column exists in meter_readings
COLUMN_EXISTS=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c \
  "SELECT COUNT(*) FROM information_schema.columns WHERE table_name = 'meter_readings' AND column_name = 'on_chain_confirmed';" | tr -d ' ')

if [ "$COLUMN_EXISTS" -eq "1" ]; then
    echo -e "${GREEN}✅ on_chain_confirmed column exists in meter_readings${NC}"
else
    echo -e "${RED}❌ on_chain_confirmed column not found${NC}"
    exit 1
fi

echo ""

echo "--------------------------------------------------------------------------------"
echo "2. Checking Event Processing State"
echo "--------------------------------------------------------------------------------"

docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "SELECT service_name, last_processed_slot, last_processed_at FROM event_processing_state;"

echo ""

echo "--------------------------------------------------------------------------------"
echo "3. Checking Pending Confirmations"
echo "--------------------------------------------------------------------------------"

PENDING_COUNT=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c \
  "SELECT COUNT(*) FROM meter_readings 
   WHERE minted = true 
     AND on_chain_confirmed = false
     AND mint_tx_signature IS NOT NULL
     AND mint_tx_signature != 'mock_signature';" | tr -d ' ')

echo -e "${BLUE}Pending confirmations: $PENDING_COUNT${NC}"

if [ "$PENDING_COUNT" -gt "0" ]; then
    echo ""
    echo "Pending readings:"
    docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
      "SELECT id, mint_tx_signature, submitted_at 
       FROM meter_readings 
       WHERE minted = true 
         AND on_chain_confirmed = false
         AND mint_tx_signature IS NOT NULL
         AND mint_tx_signature != 'mock_signature'
       ORDER BY submitted_at DESC 
       LIMIT 5;"
fi

echo ""

echo "--------------------------------------------------------------------------------"
echo "4. Checking Confirmed Readings"
echo "--------------------------------------------------------------------------------"

CONFIRMED_COUNT=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c \
  "SELECT COUNT(*) FROM meter_readings WHERE on_chain_confirmed = true;" | tr -d ' ')

echo -e "${BLUE}On-chain confirmed readings: $CONFIRMED_COUNT${NC}"

if [ "$CONFIRMED_COUNT" -gt "0" ]; then
    echo ""
    echo "Recent confirmed readings:"
    docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
      "SELECT id, mint_tx_signature, on_chain_slot, on_chain_confirmed_at 
       FROM meter_readings 
       WHERE on_chain_confirmed = true 
       ORDER BY on_chain_confirmed_at DESC 
       LIMIT 5;"
fi

echo ""

echo "--------------------------------------------------------------------------------"
echo "5. Checking Blockchain Events"
echo "--------------------------------------------------------------------------------"

EVENTS_COUNT=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c \
  "SELECT COUNT(*) FROM blockchain_events;" | tr -d ' ')

echo -e "${BLUE}Total blockchain events: $EVENTS_COUNT${NC}"

if [ "$EVENTS_COUNT" -gt "0" ]; then
    echo ""
    echo "Recent events:"
    docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
      "SELECT event_type, transaction_signature, slot, block_time, created_at 
       FROM blockchain_events 
       ORDER BY created_at DESC 
       LIMIT 5;"
fi

echo ""

echo "--------------------------------------------------------------------------------"
echo "6. Statistics Summary"
echo "--------------------------------------------------------------------------------"

docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -c \
  "SELECT 
    (SELECT COUNT(*) FROM meter_readings WHERE minted = true) as total_minted,
    (SELECT COUNT(*) FROM meter_readings WHERE on_chain_confirmed = true) as confirmed,
    (SELECT COUNT(*) FROM meter_readings WHERE minted = true AND on_chain_confirmed = false AND mint_tx_signature != 'mock_signature') as pending,
    (SELECT COUNT(*) FROM blockchain_events) as total_events;"

echo ""
echo "================================================================================"
echo -e "${GREEN}EVENT PROCESSOR SERVICE TEST COMPLETED${NC}"
echo "================================================================================"
echo ""
echo "Next steps:"
echo "1. Enable real blockchain mode: TOKENIZATION_ENABLE_REAL_BLOCKCHAIN=true"
echo "2. Submit a meter reading and wait for minting"
echo "3. Wait 10-20 seconds for event processor to confirm"
echo "4. Run this script again to see on-chain confirmations"
echo ""
