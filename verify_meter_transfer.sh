#!/bin/bash
set -e

# Configuration
API_GATEWAY_DIR="/Users/chanthawat/Developments/gridtokenx-platform/gridtokenx-apigateway"
SIMULATOR_DIR="/Users/chanthawat/Developments/gridtokenx-platform/gridtokenx-smartmeter-simulator"
API_URL="http://localhost:8080"
SIMULATOR_URL="http://localhost:8000"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
CYAN='\033[0;36m'
NC='\033[0m'

echo -e "${CYAN}Starting Meter Data Transfer Verification (Full Flow with Clean Start)...${NC}"

# Helper to kill port
kill_port() {
    PORT=$1
    PID=$(lsof -t -i:$PORT || true)
    if [ -n "$PID" ]; then
        echo "Killing process $PID on port $PORT"
        kill -9 $PID || true
    fi
}

# 1. Check/Start API Gateway
echo -e "${CYAN}Checking API Gateway...${NC}"
cd "$API_GATEWAY_DIR"
if curl -s "${API_URL}/health" > /dev/null; then
    echo -e "${GREEN}API Gateway is running.${NC}"
else
    echo -e "${RED}API Gateway is NOT running. Starting it...${NC}"
    kill_port 8080
    nohup ./start-apigateway.sh > apigateway.log 2>&1 &
    API_PID=$!
    echo "API Gateway started with PID $API_PID. Waiting for startup..."
    
    # Wait loop up to 60s
    MAX_RETRIES=30
    COUNT=0
    while ! curl -s "${API_URL}/health" > /dev/null; do
        sleep 2
        COUNT=$((COUNT+1))
        echo -ne "Waiting for API Gateway ($COUNT/$MAX_RETRIES)...\r"
        if [ $COUNT -ge $MAX_RETRIES ]; then
             echo ""
             echo -e "${RED}Timeout waiting for API Gateway.${NC}"
             tail -n 20 apigateway.log
             exit 1
        fi
    done
    echo ""
    echo -e "${GREEN}API Gateway is up!${NC}"
fi

# 2. Start Simulator (Force Restart)
echo -e "${CYAN}Starting Smart Meter Simulator (Clean)...${NC}"
cd "$SIMULATOR_DIR"
kill_port 8000
pkill -f "python3 -m smart_meter_simulator.main" || true

# Append to log instead of overwriting to keep history
nohup ./start-simulator.sh >> simulator.log 2>&1 &
echo "Simulator started. Waiting..."
sleep 5
if ! curl -s "${SIMULATOR_URL}/health" > /dev/null; then
     echo -e "${RED}Failed to start Simulator.${NC}"
     # Check log
     tail -n 20 simulator.log
     exit 1
fi

# 3. Run Registration Loop to Seed Valid Data
echo -e "${CYAN}Running Registration Loop to seed valid meter/user data...${NC}"
cd "$API_GATEWAY_DIR"
if ./verify_registration_loop.sh > registration.log 2>&1; then
    echo -e "${GREEN}Registration Loop Completed Successfully.${NC}"
else
    echo -e "${RED}Registration Loop Failed.${NC}"
    tail -n 20 registration.log
fi

# 4. RESTART Simulator to pick up new meter (This is still needed if dynamic update is flaky, 
# although fixing persistence + clean start might make dynamic update work?)
# Let's try WITHOUT restart first. If dynamic update works (engine.meters.append), it should trigger.
# If it fails, I'll add restart back. 
# Actually, since I fixed DB persistence, restart is SAFER.
echo -e "${CYAN}Restarting Simulator to load new meter...${NC}"
cd "$SIMULATOR_DIR"
kill_port 8000
nohup ./start-simulator.sh >> simulator.log 2>&1 &
echo "Simulator restarted. Waiting for availability..."
sleep 5
if curl -s "${SIMULATOR_URL}/health" > /dev/null; then
     echo -e "${GREEN}Simulator is back online.${NC}"
else
     echo -e "${RED}Failed to restart Simulator.${NC}"
     exit 1
fi

# 5. Monitor Logs for Data Transfer
echo -e "${CYAN}Monitoring logs for success...${NC}"

TIMEOUT=60
ELAPSED=0
SUCCESS_SIM=0
SUCCESS_GATEWAY=0

while [ $ELAPSED -lt $TIMEOUT ]; do
    # Check Simulator Log
    if grep -q "Reading sent successfully" "$SIMULATOR_DIR/simulator.log"; then
        SUCCESS_SIM=1
    fi
    
    # Check Gateway Log (New entries)
    if grep -q "Meter reading submitted successfully" "$API_GATEWAY_DIR/apigateway.log"; then
        SUCCESS_GATEWAY=1
    fi
    
    if [ $SUCCESS_SIM -eq 1 ]; then
        echo -e "${GREEN}SUCCESS: Simulator sent reading!${NC}"
        break
    fi
    
    sleep 2
    ELAPSED=$((ELAPSED+2))
    echo -ne "Waiting... ${ELAPSED}/${TIMEOUT}s (Sim: $SUCCESS_SIM)\r"
done

echo "" # Newline

if [ $SUCCESS_SIM -eq 0 ]; then
    echo -e "${RED}FAILURE: Simulator did not report successful send.${NC}"
    echo "Tail of simulator log:"
    tail -n 50 "$SIMULATOR_DIR/simulator.log"
    exit 1
else
    exit 0
fi
