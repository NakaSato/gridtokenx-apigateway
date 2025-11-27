# GridTokenX API Gateway - Metrics Documentation

## Overview

The GridTokenX API Gateway provides comprehensive metrics collection and monitoring capabilities through dedicated endpoints. These metrics track application performance, blockchain transactions, and system health.

## Quick Start

### 1. Access Metrics

```bash
# Check if service is running
curl -s http://localhost:8080/health | jq .

# View metrics (no authentication required)
curl -s http://localhost:8080/metrics

# View detailed health metrics (requires JWT)
curl -s -H "Authorization: Bearer <token>" \
     http://localhost:8080/health/metrics | jq .
```

### 2. Prometheus Integration

Add to your `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'gridtokenx-api'
    static_configs:
      - targets: ['localhost:8080']
    metrics_path: '/metrics'
    scrape_interval: 15s
```

## Available Metrics

### 1. Health Check

**Endpoint**: `GET /health`

Returns basic service status:
```json
{
  "status": "healthy",
  "timestamp": "2025-11-23T11:05:38.442908Z",
  "version": "0.1.1",
  "environment": "development"
}
```

### 2. Health with Metrics

**Endpoint**: `GET /health/metrics`

**Authentication**: JWT token required

Returns system status with metrics:
```json
{
  "status": "healthy",
  "timestamp": "2025-11-23T11:05:43.195013+00:00",
  "uptime_seconds": 120,
  "metrics": {
    "database": {
      "active_connections": 5,
      "idle_connections": 5,
      "pool_size": 10
    },
    "transactions": {
      "confirmed_count": 50,
      "failed_count": 2,
      "pending_count": 8,
      "total_count": 60
    }
  }
}
```

### 3. Prometheus Metrics Export

**Endpoint**: `GET /metrics`

**Content-Type**: `text/plain; version=0.0.4`

Exports metrics in Prometheus format:
```
# HELP gridtokenx_transaction_pending_count Current number of pending transactions
# TYPE gridtokenx_transaction_pending_count gauge
gridtokenx_transaction_pending_count{tx_type="energy_trade"} 8

# HELP gridtokenx_database_active_connections Current number of active database connections
# TYPE gridtokenx_database_active_connections gauge
gridtokenx_database_active_connections 5
```

## Key Metrics

### Transaction Metrics
- `gridtokenx_transaction_pending_count`: Pending transactions by type
- `gridtokenx_transaction_confirmed_total`: Total confirmed transactions
- `gridtokenx_transaction_failed_total`: Total failed transactions
- `gridtokenx_transaction_confirmation_duration_seconds`: Time to confirmation

### API Performance Metrics
- `gridtokenx_api_requests_total`: Request count by method, endpoint, status
- `gridtokenx_api_request_duration_seconds`: Request latency with histogram buckets
- `gridtokenx_api_response_size_bytes`: Response size tracking

### System Metrics
- `gridtokenx_database_active_connections`: Active database connections
- `gridtokenx_database_pool_size`: Total connection pool size
- `process_cpu_seconds_total`: CPU usage tracking
- `process_resident_memory_bytes`: Memory usage tracking

## Grafana Dashboard

Create a dashboard with panels for:

1. **Transaction Status** (Pie Chart)
   - Metric: `gridtokenx_transaction_pending_count`
   - Group by: `tx_type`

2. **API Request Rate** (Graph)
   - Metric: `rate(gridtokenx_api_requests_total[5m])`
   - Group by: `method`, `endpoint`

3. **Response Time** (Graph)
   - Metric: `histogram_quantile(0.95, rate(gridtokenx_api_request_duration_seconds_bucket[5m]))`

4. **Database Connections** (Stat Panel)
   - Metrics: `gridtokenx_database_active_connections`, `gridtokenx_database_pool_size`

## Alerting

### Critical Alerts

```yaml
# API Gateway Down
- alert: APIGatewayDown
  expr: up{job="gridtokenx-api"} == 0
  for: 30s
  labels:
    severity: critical

# Transaction Backlog
- alert: TransactionBacklog
  expr: gridtokenx_transaction_pending_count > 100
  for: 10m
  labels:
    severity: warning
```

### Performance Alerts

```yaml
# High Latency
- alert: HighAPILatency
  expr: histogram_quantile(0.95, rate(gridtokenx_api_request_duration_seconds_bucket[5m])) > 1.0
  for: 5m
  labels:
    severity: warning

# Database Connection Issues
- alert: DatabaseConnectionIssue
  expr: gridtokenx_database_active_connections / gridtokenx_database_pool_size > 0.9
  for: 5m
  labels:
    severity: critical
```

## Implementation Details

### Metrics Collection

Implemented in `src/services/transaction_metrics.rs`:

- Transaction tracking with timestamps
- API request/response metrics
- Database connection monitoring
- Prometheus-compatible export format

### Middleware Integration

Applied in `src/middleware/metrics_middleware.rs`:

- Automatic request ID generation
- Request duration tracking
- Status code aggregation
- Response header addition

### Service Integration

Used throughout the application:

- Transaction coordinator emits metrics on state changes
- Database service monitors connection pool
- Blockchain service tracks operation latency

## Troubleshooting

### Common Issues

1. **No metrics data**
   - Verify metrics middleware is applied to router
   - Check `RUST_LOG=info` is set
   - Confirm `/metrics` endpoint is accessible

2. **High memory usage**
   - Reduce metrics sampling rate
   - Implement buffering for metric exports
   - Check for metric leaks

3. **Transaction tracking not working**
   - Verify `blockchain_operations` view exists
   - Check database connection health
   - Review transaction coordinator logs

## Production Deployment

### Security

1. **Restrict metrics endpoint**:
   ```nginx
   location /metrics {
       allow 127.0.0.1;  # Monitoring server only
       deny all;
   }
   ```

2. **TLS Configuration**:
   ```yaml
   scheme: https
   tls_config:
     cert_file: /path/to/cert.pem
     key_file: /path/to/key.pem
   ```

3. **Authentication**:
   ```yaml
   basic_auth:
     - username: metrics
       password: secure_password
   ```

### Performance Tuning

1. **Connection Pool**:
   ```rust
   PgPoolOptions::new()
       .max_connections(100)
       .min_connections(20)
       .acquire_timeout(Duration::from_secs(3))
   ```

2. **Metrics Sampling**:
   ```rust
   // Sample high-cardinality metrics
   if fastrand::bool() < 0.1 {
       record_metric("high_cardinality", value);
   }
   ```

3. **Batch Processing**:
   ```rust
   // Process transactions in batches
   let batch = self.get_pending_transactions(batch_size).await?;
   for tx in batch {
       tokio::spawn(self.process_transaction(tx));
   }
   ```

## Next Steps

1. Set up production monitoring stack
2. Configure alerts based on SLA requirements
3. Implement automated testing of metrics endpoints
4. Establish regular performance reviews