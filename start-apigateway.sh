#!/bin/bash
# Start API Gateway in background
cd /Users/chanthawat/Developments/gridtokenx-platform/gridtokenx-apigateway
cargo run > apigateway.log 2>&1 &
echo $! > apigateway.pid
echo "API Gateway started with PID: $(cat apigateway.pid)"
