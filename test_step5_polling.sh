#!/bin/bash

# Configuration
API_URL="http://localhost:8080"
DB_CONTAINER="p2p-postgres"
DB_USER="gridtokenx_user"
DB_NAME="gridtokenx"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

echo "================================================================================"
echo "STEP 5 TEST: Automated Token Minting"
echo "================================================================================"

# Check if API Gateway is running
if ! curl -s "$API_URL/health" > /dev/null; then
    echo -e "${RED}❌ API Gateway is not running!${NC}"
    echo "Please start the API Gateway with 'cargo run' first."
    exit 1
fi

echo -e "${GREEN}✅ API Gateway is running${NC}"

# Step 1: Insert a test reading with minted=false
echo "Step 1: Inserting unminted test reading..."

# Get a valid user ID
USER_ID=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c "SELECT id FROM users LIMIT 1;" | tr -d ' ')

if [ -z "$USER_ID" ]; then
    echo -e "${RED}❌ No users found in database${NC}"
    exit 1
fi

# Insert reading
READING_ID=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c "
INSERT INTO meter_readings (
    id, user_id, wallet_address, kwh_amount, reading_timestamp, submitted_at, minted, verification_status, timestamp
) VALUES (
    gen_random_uuid(), 
    '$USER_ID', 
    '7YhKmZbFZt8qP3xN9vJ2kL4mR5wT6uV8sA1bC3dE4fG5', 
    10.5, 
    NOW(), 
    NOW(), 
    false,
    'verified',
    NOW()
) RETURNING id;" | grep -oE '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -n 1)

echo -e "${GREEN}✅ Inserted reading: $READING_ID${NC}"

# Step 2: Wait for polling service
echo "Step 2: Waiting for polling service (max 70s)..."
echo "The polling interval is 60 seconds."

for i in {1..14}; do
    echo -n "."
    sleep 5
    
    # Check status
    MINTED=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c "SELECT minted FROM meter_readings WHERE id = '$READING_ID';" | tr -d ' ')
    
    if [ "$MINTED" = "t" ]; then
        echo ""
        echo -e "${GREEN}✅ Reading marked as minted!${NC}"
        
        # Check signature
        SIG=$(docker exec $DB_CONTAINER psql -U $DB_USER -d $DB_NAME -t -c "SELECT mint_tx_signature FROM meter_readings WHERE id = '$READING_ID';" | tr -d ' ')
        echo "Signature: $SIG"
        
        if [ "$SIG" = "mock_signature" ]; then
             echo -e "${GREEN}✅ Mock signature verified${NC}"
        else
             echo -e "${RED}⚠️  Unexpected signature: $SIG${NC}"
        fi
        
        exit 0
    fi
done

echo ""
echo -e "${RED}❌ Timeout waiting for minting${NC}"
echo "Reading status is still minted=$MINTED"
exit 1
