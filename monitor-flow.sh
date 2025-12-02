#!/bin/bash
# Monitor the complete smart meter to token minting flow

echo "==================================="
echo "Smart Meter Flow Monitoring Script"
echo "==================================="
echo ""

# Check services
echo "📊 Service Status:"
echo "-------------------"

# Check Validator
if pgrep -f "solana-test-validator" > /dev/null; then
    echo "✅ Solana Validator: Running"
    solana cluster-version 2>/dev/null || echo "   (localhost:8899)"
else
    echo "❌ Solana Validator: Not running"
fi

# Check API Gateway
if pgrep -f "gridtokenx-apigateway" > /dev/null; then
    echo "✅ API Gateway: Running"
    curl -s http://localhost:8080/health | grep -q "healthy" && echo "   Status: Healthy" || echo "   Status: Unknown"
else
    echo "❌ API Gateway: Not running"
fi

echo ""
echo "📝 Recent Activity:"
echo "-------------------"

# Check API Gateway logs for meter activity
echo ""
echo "API Gateway - Last 10 log entries:"
tail -n 10 apigateway.log 2>/dev/null || echo "No logs found"

echo ""
echo "🔍 Next Steps:"
echo "-------------------"
echo "1. Wait 60 seconds for meter polling service"
echo "2. Check database for minted readings"
echo "3. Run the test to verify on-chain minting"
echo "4. Monitor logs in real-time: tail -f apigateway.log"
