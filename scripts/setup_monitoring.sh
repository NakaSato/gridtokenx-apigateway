#!/bin/bash
# GridTokenX API Gateway - Monitoring Setup Script
# Quick setup script for metrics and monitoring

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}GridTokenX API Gateway - Monitoring Setup${NC}"
echo "============================================"
echo ""

# Check if API Gateway is running
echo "Checking API Gateway..."
if curl -s http://localhost:8080/health | grep -q "healthy"; then
    echo -e "${GREEN}✓ API Gateway is running${NC}"
else
    echo -e "${RED}✗ API Gateway is not running${NC}"
    echo "Please start the API Gateway first: cargo run --bin api-gateway"
    exit 1
fi
echo ""

# Test metrics endpoints
echo "Testing metrics endpoints..."

# Test basic health
echo -n "  Health check: "
if curl -s http://localhost:8080/health | grep -q "healthy"; then
    echo -e "${GREEN}✓ OK${NC}"
else
    echo -e "${RED}✗ Failed${NC}"
fi

# Test metrics endpoint
echo -n "  Metrics endpoint: "
if curl -s http://localhost:8080/metrics | grep -q "gridtokenx_"; then
    echo -e "${GREEN}✓ OK${NC}"
else
    echo -e "${RED}✗ Failed${NC}"
fi

# Test health with metrics (requires JWT)
echo -n "  Health with metrics: "
echo -e "${YELLOW}⚠ Skipped (requires JWT token)${NC}"
echo ""

# Display metrics sample
echo -e "${GREEN}Sample metrics output:${NC}"
curl -s http://localhost:8080/metrics | head -10
echo ""

# Instructions for Prometheus integration
echo -e "${GREEN}Prometheus Integration:${NC}"
echo "1. Add to your prometheus.yml:"
echo "  scrape_configs:"
echo "    - job_name: 'gridtokenx-api'"
echo "      static_configs:"
echo "        - targets: ['localhost:8080']"
echo "        metrics_path: '/metrics'"
echo "        scrape_interval: 15s"
echo ""
echo "2. Restart Prometheus"
echo ""

# Grafana Dashboard URL
echo -e "${GREEN}Grafana Dashboard:${NC}"
echo "1. Add Prometheus data source: http://localhost:9090"
echo "2. Import dashboard configuration from docs/metrics/"
echo ""

echo -e "${GREEN}Setup completed!${NC}"
echo "Your API Gateway is now providing metrics for monitoring."
