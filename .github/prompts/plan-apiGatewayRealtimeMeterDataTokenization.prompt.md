# Plan: API Gateway for Real-Time Meter Data with 1kWh = 1 GRX Token Minting

Design and implement a comprehensive API Gateway to support automated real-time smart meter data processing with direct 1:1 tokenization (1 kWh = 1 GRX token). This plan extends the existing manual-trigger system to include automated background processing, real-time WebSocket notifications, configurable parameters, and enhanced monitoring capabilities.

## Steps

1. **Create tokenization configuration module** in `src/config/tokenization.rs` - centralized config struct loaded from environment variables (`KWH_TO_TOKEN_RATIO=1.0`, `AUTO_MINT_ENABLED`, `POLLING_INTERVAL_SECS`, `BATCH_SIZE`), eliminating hardcoded values in `blockchain_service.rs:303`

2. **Implement automated meter polling service** in `src/services/meter_polling_service.rs` - background tokio task that periodically queries unminted readings via `MeterService::get_unminted_readings()`, processes in configurable batches (50-100 readings), calls batch minting, updates database, broadcasts WebSocket events, and implements retry logic with exponential backoff. Supports both time-based polling and immediate manual triggers via admin API

2a. **Add auto-trigger capability** - when `TOKENIZATION_AUTO_TRIGGER_ON_SUBMIT=true`, immediately queue meter reading for minting after submission (bypasses polling interval). Implemented via tokio channel (mpsc) that polling service listens to, allowing instant processing while maintaining centralized minting logic

3. **Add batch minting to blockchain service** in `src/services/blockchain_service.rs` - new `mint_energy_tokens_batch()` method that accepts `Vec<MeterReading>`, spawns parallel tokio tasks for concurrent transaction submission (respecting Solana rate limits), aggregates results, and handles partial failures gracefully

4. **Enhance WebSocket service for meter events** in `src/services/websocket_service.rs` - extend `MarketEvent` enum with `MeterReadingReceived` and `TokensMinted` variants, add broadcast methods `broadcast_meter_reading()` and `broadcast_tokens_minted()`, implement user-specific event filtering for privacy

5. **Update API handlers for real-time integration** in `src/handlers/meters.rs` - modify `submit_reading()` handler (line 218) to broadcast WebSocket event immediately after database insert, optionally trigger immediate minting if `auto_trigger_on_submit=true`, add new endpoint `GET /api/meters/minting-status/:reading_id` for polling minting progress, enhance error responses with actionable guidance

5a. **Add admin configuration endpoints** in `src/handlers/admin/auto_mint_config.rs` - new handler module with endpoints: `GET /api/admin/meters/auto-mint-config` (view current settings), `PUT /api/admin/meters/auto-mint-config` (update runtime configuration), `POST /api/admin/meters/auto-mint-trigger` (manually trigger immediate minting cycle). Configuration changes are stored in database and hot-reloaded by polling service

6. **Wire services into application state** in `src/main.rs` - initialize `TokenizationConfig` from env vars, create `MeterPollingService` with Arc-wrapped dependencies (db, blockchain_service, websocket_service), spawn background task in separate tokio runtime, register graceful shutdown handlers, expose config via health endpoint

7. **Add monitoring and observability** - implement Prometheus metrics for minting throughput (`meter_readings_minted_total`), latency (`minting_duration_seconds`), batch sizes, failure rates; add structured logging with tracing spans for correlation; create dashboard queries for Grafana

## Further Considerations

1. **Processing interval tuning**: Start with 60-second polling in production to balance latency vs. blockchain transaction costs. Monitor queue depth and adjust to 30 seconds if backlog exceeds threshold. For near-instant minting (<5s latency), enable `TOKENIZATION_AUTO_TRIGGER_ON_SUBMIT=true` which triggers minting immediately after reading submission. Trade-off: Higher blockchain transaction costs but better UX. Recommended for premium users or high-value transactions.

1a. **Auto-trigger vs. Polling trade-offs**: 
   - **Polling mode** (auto_trigger=false): Batches readings efficiently, lower tx costs, 1-2 min latency
   - **Auto-trigger mode** (auto_trigger=true): Near-instant minting, higher tx costs (1 tx per reading), <10s latency
   - **Hybrid mode**: Use auto-trigger for readings >50 kWh, polling for smaller readings (implement threshold logic)

2. **Batch size optimization**: Recommend starting with batch size of 50 readings per cycle. Each Solana transaction is ~1232 bytes; parallel submission of 50 transactions should complete in <10 seconds. Increase to 100 after confirming RPC endpoint can handle load without rate limiting.

3. **Error recovery strategy**: Implement 3-tier retry: (1) Immediate retry for transient RPC errors, (2) Exponential backoff (1min, 5min, 15min) for recoverable failures, (3) Dead letter queue for readings that fail after 3 attempts with admin alert. Store retry count and last error in database for debugging.

4. **Real-time streaming vs. polling**: Current plan uses database polling for simplicity. For true real-time (<1s latency), consider: (a) Postgres LISTEN/NOTIFY triggers on `meter_readings` inserts, (b) Redis pub/sub channel, or (c) Dedicated message queue (RabbitMQ/NATS). Evaluate based on expected meter submission rate (>100/sec warrants streaming).

5. **API rate limiting per user**: Add per-user submission limits to prevent abuse - recommend max 1 meter reading per 5 minutes per user, implemented via Redis sliding window counter. Return HTTP 429 with `Retry-After` header when exceeded.

6. **Wallet account initialization**: Auto-mint requires users to have Associated Token Account (ATA) created. Add endpoint `POST /api/meters/initialize-token-account` that creates ATA if not exists, or automatically create during first mint attempt with retry on "AccountNotFound" error.

7. **Historical backfill**: For existing unminted readings in database, create admin endpoint `POST /api/admin/meters/backfill-mint` with date range filter and progress tracking. Process in larger batches (200-500) since not time-sensitive.

8. **Compliance and audit trail**: Ensure all minting operations are logged with: user_id, reading_id, kwh_amount, tokens_minted, tx_signature, timestamp. Consider immutable append-only audit log table for regulatory compliance.

## API Endpoint Specification

### New Endpoints

#### Real-Time Status Endpoint
```
GET /api/meters/minting-status/:reading_id
Response: {
  "reading_id": "uuid",
  "status": "pending" | "processing" | "minted" | "failed",
  "kwh_amount": "25.5",
  "tx_signature": "5J7K8L9M...",
  "minted_at": "2025-11-18T10:30:00Z",
  "estimated_completion": "2025-11-18T10:32:00Z",
  "retry_count": 0,
  "last_error": null
}
```

#### Wallet Initialization
```
POST /api/meters/initialize-token-account
Response: {
  "wallet_address": "DYw8j...xyz",
  "token_account": "5yW8R...abc",
  "tx_signature": "2Hd9k...",
  "status": "created" | "exists"
}
```

#### Auto-Mint Configuration Control (Admin)
```
GET /api/admin/meters/auto-mint-config
Response: {
  "auto_mint_enabled": true,
  "polling_interval_secs": 60,
  "batch_size": 50,
  "max_retries": 3,
  "last_poll_at": "2025-11-18T10:30:00Z",
  "queue_depth": 23,
  "active": true
}

PUT /api/admin/meters/auto-mint-config
Request: {
  "auto_mint_enabled": true,
  "polling_interval_secs": 30,
  "batch_size": 100
}
Response: {
  "success": true,
  "config": { ...updated config },
  "message": "Auto-mint configuration updated. Changes will take effect on next polling cycle."
}

POST /api/admin/meters/auto-mint-trigger
Description: Manually trigger an immediate minting cycle (bypasses polling interval)
Response: {
  "success": true,
  "readings_processed": 45,
  "readings_minted": 43,
  "readings_failed": 2,
  "duration_seconds": 8.5
}
```

#### Batch Minting Admin Endpoint
```
POST /api/admin/meters/backfill-mint
Request: {
  "start_date": "2025-11-01T00:00:00Z",
  "end_date": "2025-11-18T23:59:59Z",
  "batch_size": 200
}
Response: {
  "job_id": "uuid",
  "total_readings": 1523,
  "estimated_duration_seconds": 180
}

GET /api/admin/meters/backfill-status/:job_id
Response: {
  "job_id": "uuid",
  "status": "in_progress",
  "processed": 850,
  "total": 1523,
  "success": 842,
  "failed": 8,
  "progress_percent": 55.8
}
```

### Enhanced Existing Endpoints

#### Submit Reading (Modified)
```
POST /api/meters/submit-reading
Request: {
  "kwh_amount": "25.5",
  "reading_timestamp": "2025-11-18T10:00:00Z",
  "meter_signature": "optional"
}
Response: {
  "id": "uuid",
  "user_id": "uuid",
  "kwh_amount": "25.5",
  "status": "queued_for_minting",  // NEW
  "estimated_minting_time": "2025-11-18T10:02:00Z",  // NEW
  "position_in_queue": 5,  // NEW
  "minted": false,
  "submitted_at": "2025-11-18T10:00:45Z"
}
```

### WebSocket Events

#### Connection
```
ws://localhost:8080/ws?token=<jwt>

// Client receives meter events filtered by user_id
```

#### Event: MeterReadingReceived
```json
{
  "event_type": "MeterReadingReceived",
  "data": {
    "reading_id": "uuid",
    "user_id": "uuid",
    "wallet_address": "DYw8j...xyz",
    "kwh_amount": "25.5",
    "timestamp": "2025-11-18T10:00:00Z",
    "status": "queued"
  }
}
```

#### Event: TokensMinted
```json
{
  "event_type": "TokensMinted",
  "data": {
    "reading_id": "uuid",
    "user_id": "uuid",
    "wallet_address": "DYw8j...xyz",
    "kwh_amount": "25.5",
    "tokens_minted": "25500000000",  // lamports (25.5 * 1B)
    "tx_signature": "5J7K8L9M...xyz",
    "minted_at": "2025-11-18T10:01:23Z",
    "duration_seconds": 8.2
  }
}
```

#### Event: MintingFailed
```json
{
  "event_type": "MintingFailed",
  "data": {
    "reading_id": "uuid",
    "user_id": "uuid",
    "kwh_amount": "25.5",
    "error": "RPC timeout",
    "retry_count": 1,
    "next_retry_at": "2025-11-18T10:05:00Z"
  }
}
```

## Environment Configuration

```bash
# Tokenization Config (new)
TOKENIZATION_KWH_TO_TOKEN_RATIO=1.0
TOKENIZATION_DECIMALS=9
TOKENIZATION_AUTO_MINT_ENABLED=true
TOKENIZATION_POLLING_INTERVAL_SECS=60
TOKENIZATION_BATCH_SIZE=50
TOKENIZATION_MAX_READING_KWH=100.0
TOKENIZATION_READING_MAX_AGE_DAYS=7
TOKENIZATION_AUTO_TRIGGER_ON_SUBMIT=false  # NEW: Auto-trigger minting immediately after reading submission

# Retry Policy (new)
TOKENIZATION_MAX_RETRIES=3
TOKENIZATION_RETRY_BASE_DELAY_SECS=60
TOKENIZATION_RETRY_MAX_DELAY_SECS=900

# Rate Limiting (new)
METER_SUBMISSION_RATE_LIMIT_PER_USER=1
METER_SUBMISSION_RATE_WINDOW_MINUTES=5

# Existing (reference)
DATABASE_URL="postgresql://..."
REDIS_URL="redis://localhost:6379"
SOLANA_RPC_URL="http://localhost:8899"
GRID_TOKEN_MINT="<GRX token mint address>"
AUTHORITY_WALLET_PATH="./authority-wallet.json"
JWT_SECRET="<secret>"
```

## Implementation Priority

**Phase 1: Foundation (Week 1)**
- Create `TokenizationConfig` module
- Update `BlockchainService` to use config
- Add unit tests for configuration loading

**Phase 2: Core Automation (Week 2)**
- Implement `MeterPollingService` with basic polling
- Add `mint_energy_tokens_batch()` method
- Wire into `AppState` and spawn background task
- Integration tests for automated minting

**Phase 3: Real-Time Features (Week 3)**
- Enhance `WebSocketService` with meter events
- Update `submit_reading()` handler to broadcast
- Add `minting-status` endpoint
- WebSocket client testing

**Phase 4: Robustness (Week 4)**
- Implement retry logic with exponential backoff
- Add Prometheus metrics
- Create admin backfill endpoint
- Load testing and optimization

**Phase 5: Production Readiness (Week 5)**
- Add rate limiting
- Implement graceful shutdown
- Security audit
- Documentation and runbooks

## Success Metrics

- **Minting Latency**: p95 < 2 minutes from submission to tokens in wallet
- **Throughput**: Process 500+ readings/hour (8.3/min sustained)
- **Reliability**: 99.5% success rate for minting operations
- **WebSocket Delivery**: <100ms event broadcast latency
- **Zero Data Loss**: All readings eventually minted or flagged for manual review

## Current Architecture Reference

### Existing Database Schema
```sql
CREATE TABLE meter_readings (
    id UUID PRIMARY KEY,
    user_id UUID REFERENCES users(id),
    wallet_address VARCHAR(88) NOT NULL,
    kwh_amount DECIMAL(10, 2) NOT NULL,
    reading_timestamp TIMESTAMPTZ NOT NULL,
    submitted_at TIMESTAMPTZ DEFAULT NOW(),
    minted BOOLEAN DEFAULT FALSE,
    mint_tx_signature VARCHAR(88),
    meter_signature TEXT
);
```

### Current Endpoints (Implemented)
- `POST /api/meters/submit-reading` - Prosumer submits reading
- `GET /api/meters/my-readings` - User's reading history with pagination
- `GET /api/meters/readings/:wallet_address` - Query by wallet
- `GET /api/meters/stats` - User statistics (total/minted/unminted kWh)
- `GET /api/admin/meters/unminted` - Admin view of pending readings
- `POST /api/admin/meters/mint-from-reading` - Manual minting trigger

### Current Services
- `MeterService` - Database operations, validation, duplicate detection
- `BlockchainService` - Solana RPC interaction, transaction signing
- `WalletService` - Authority wallet management
- `WebSocketService` - Real-time trading events (needs extension for meters)

### Current Conversion Logic
```rust
// blockchain_service.rs:303-376
pub async fn mint_energy_tokens(
    &self,
    authority: &Keypair,
    user_token_account: &Pubkey,
    mint: &Pubkey,
    amount_kwh: f64,
) -> Result<Signature> {
    // HARDCODED: 1 kWh = 1 token with 9 decimals
    let amount_lamports = (amount_kwh * 1_000_000_000.0) as u64;
    
    // Calls anchor program mint_tokens_direct instruction
    // ...
}
```

## Database Schema Extensions (Proposed)

### Add Retry Tracking
```sql
ALTER TABLE meter_readings
ADD COLUMN retry_count INT DEFAULT 0,
ADD COLUMN last_retry_at TIMESTAMPTZ,
ADD COLUMN last_error TEXT,
ADD COLUMN minting_started_at TIMESTAMPTZ,
ADD COLUMN minting_completed_at TIMESTAMPTZ,
ADD COLUMN auto_triggered BOOLEAN DEFAULT false;  -- NEW: Track if minting was auto-triggered vs. polling

CREATE INDEX idx_meter_readings_retry ON meter_readings(retry_count) WHERE minted = false;
CREATE INDEX idx_meter_readings_started ON meter_readings(minting_started_at) WHERE minted = false;
```

### Auto-Mint Configuration Table
```sql
CREATE TABLE auto_mint_config (
    id INT PRIMARY KEY DEFAULT 1,
    auto_mint_enabled BOOLEAN DEFAULT true,
    polling_interval_secs INT DEFAULT 60,
    batch_size INT DEFAULT 50,
    auto_trigger_on_submit BOOLEAN DEFAULT false,
    max_retries INT DEFAULT 3,
    retry_base_delay_secs INT DEFAULT 60,
    retry_max_delay_secs INT DEFAULT 900,
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    updated_by UUID REFERENCES users(id),
    CONSTRAINT single_config CHECK (id = 1)  -- Ensure only one config row
);

-- Insert default configuration
INSERT INTO auto_mint_config (id) VALUES (1);
```

### Audit Log Table
```sql
CREATE TABLE meter_minting_audit (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reading_id UUID REFERENCES meter_readings(id),
    user_id UUID REFERENCES users(id),
    wallet_address VARCHAR(88) NOT NULL,
    kwh_amount DECIMAL(10, 2) NOT NULL,
    tokens_minted BIGINT NOT NULL,
    tx_signature VARCHAR(88) NOT NULL,
    authority_wallet VARCHAR(88) NOT NULL,
    minted_at TIMESTAMPTZ DEFAULT NOW(),
    duration_ms INT,
    metadata JSONB
);

CREATE INDEX idx_audit_user_id ON meter_minting_audit(user_id);
CREATE INDEX idx_audit_minted_at ON meter_minting_audit(minted_at);
CREATE INDEX idx_audit_tx_sig ON meter_minting_audit(tx_signature);
```

### Backfill Job Tracking
```sql
CREATE TABLE meter_backfill_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    admin_user_id UUID REFERENCES users(id),
    start_date TIMESTAMPTZ NOT NULL,
    end_date TIMESTAMPTZ NOT NULL,
    batch_size INT NOT NULL,
    status VARCHAR(20) NOT NULL, -- pending, in_progress, completed, failed
    total_readings INT DEFAULT 0,
    processed_readings INT DEFAULT 0,
    success_count INT DEFAULT 0,
    failure_count INT DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    error_message TEXT
);
```

## Testing Strategy

### Unit Tests
- `TokenizationConfig::from_env()` - Parse all env vars correctly
- `MeterPollingService::calculate_batch()` - Batch size logic
- `BlockchainService::mint_energy_tokens_batch()` - Parallel execution
- Conversion formula validation (1 kWh = 1e9 lamports)

### Integration Tests
1. **Happy Path (Polling Mode)**: Submit reading → wait for polling cycle → auto-mint (mock Solana) → verify DB update → WebSocket event
2. **Happy Path (Auto-Trigger Mode)**: Enable auto_trigger → submit reading → verify immediate minting within 5s → WebSocket event
3. **Batch Processing**: Submit 100 readings → verify all minted in 2 batches
4. **Manual Admin Trigger**: Submit readings → admin calls POST /auto-mint-trigger → verify immediate processing
5. **Config Hot-Reload**: Update polling_interval via admin API → verify next cycle uses new interval
6. **Retry Logic**: Inject RPC failure → verify exponential backoff → eventual success
7. **Rate Limiting**: Submit 10 readings in 1 minute → verify 429 response after limit
8. **WebSocket Filtering**: User A submits reading → verify User B doesn't receive event

### Load Tests (Apache Bench + Custom Scripts)
```bash
# 1000 concurrent meter submissions
ab -n 1000 -c 100 -p meter_reading.json \
   -H "Authorization: Bearer $TOKEN" \
   http://localhost:8080/api/meters/submit-reading

# Monitor queue depth during load
watch -n 1 'psql -c "SELECT COUNT(*) FROM meter_readings WHERE minted=false"'

# Verify throughput
curl http://localhost:8080/metrics | grep meter_readings_minted_total
```

### Failure Scenarios
- Solana RPC timeout (10s+) during minting
- Database connection pool exhaustion (max connections reached)
- Authority wallet file missing/corrupted
- WebSocket server crash during active connections
- Server restart with 500 unminted readings in queue

## Monitoring & Alerting

### Prometheus Metrics
```rust
// metrics.rs (to be created)
lazy_static! {
    // Counters
    static ref METER_READINGS_SUBMITTED: Counter = register_counter!(...);
    static ref METER_READINGS_MINTED: Counter = register_counter!(...);
    static ref MINTING_FAILURES: Counter = register_counter!(...);
    
    // Histograms
    static ref MINTING_DURATION: Histogram = register_histogram!(...);
    static ref BATCH_SIZE: Histogram = register_histogram!(...);
    
    // Gauges
    static ref UNMINTED_QUEUE_DEPTH: Gauge = register_gauge!(...);
    static ref POLLING_SERVICE_ACTIVE: Gauge = register_gauge!(...);
}
```

### Grafana Dashboard Queries
```promql
# Minting throughput (readings/hour)
rate(meter_readings_minted_total[1h]) * 3600

# P95 latency (submission to minted)
histogram_quantile(0.95, minting_duration_seconds_bucket)

# Failure rate
rate(minting_failures_total[5m]) / rate(meter_readings_submitted_total[5m])

# Queue backlog
unminted_queue_depth > 100
```

### Alerts (PagerDuty/Slack)
1. **Critical**: Minting failure rate > 5% for 5 minutes
2. **Warning**: Queue depth > 200 unminted readings
3. **Warning**: P95 latency > 5 minutes
4. **Critical**: Polling service stopped (gauge = 0 for 2 minutes)
5. **Critical**: Authority wallet balance < 0.1 SOL

## Security Considerations

1. **Authority Wallet Protection**
   - Store keypair in encrypted vault (AWS KMS, HashiCorp Vault)
   - Rotate wallet quarterly
   - Monitor balance and transaction patterns
   - Alert on suspicious activity (>1000 mints/hour)

2. **Input Validation**
   - Sanitize all meter_signature inputs (prevent SQL injection)
   - Validate kWh amounts (0-100 range)
   - Verify wallet address format before blockchain calls
   - Rate limit per wallet address (not just user_id)

3. **WebSocket Security**
   - JWT validation on every connection
   - Filter events by user_id (no cross-user data leakage)
   - Implement connection limits per user (max 5 concurrent)
   - Automatic disconnect on token expiry

4. **Admin Endpoint Protection**
   - Require admin role + IP whitelist
   - Audit all admin actions (who, what, when) - especially config changes
   - Rate limit backfill operations (1 job per 10 minutes)
   - Rate limit manual triggers (max 1 per minute to prevent abuse)
   - Log all auto-mint config changes to audit trail with before/after values

5. **Data Privacy**
   - Hash wallet addresses in logs (GDPR compliance)
   - Implement data retention policy (delete old readings after 2 years)
   - Allow user data export (GDPR right to data portability)

## Rollout Plan

### Stage 1: Development (Week 1-3)
- Implement all features in dev environment
- Unit + integration tests
- Code review + security review

### Stage 2: Staging (Week 4)
- Deploy to staging with `AUTO_MINT_ENABLED=false`
- Load testing with realistic data volumes
- WebSocket stress testing (1000 concurrent connections)
- Manual QA + exploratory testing

### Stage 3: Production Canary (Week 5)
- Deploy to 10% of production traffic
- Enable auto-minting with 5-minute polling interval
- Monitor metrics for 48 hours
- Rollback criteria: failure rate > 2%

### Stage 4: Full Production (Week 6)
- Gradually increase to 100% traffic
- Reduce polling interval to 60 seconds
- Enable all monitoring alerts
- Publish API documentation

### Stage 5: Optimization (Week 7-8)
- Tune batch sizes based on observed load
- Implement connection pooling optimizations
- Add caching for frequently accessed data
- Performance profiling and bottleneck analysis

## Documentation Deliverables

1. **API Reference** - OpenAPI spec update with new endpoints
2. **Integration Guide** - How to consume WebSocket events
3. **Operations Runbook** - Troubleshooting, monitoring, incident response
4. **Architecture Decision Records** - Why polling vs streaming, batch size rationale
5. **Migration Guide** - Transitioning from manual to automated minting
