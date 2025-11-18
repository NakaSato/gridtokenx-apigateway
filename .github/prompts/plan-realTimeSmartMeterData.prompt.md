# Plan: Real-Time Smart Meter Data Handling for GridTokenX API Gateway

## Current State Analysis

The GridTokenX API gateway has a mature implementation of smart meter data handling with:

**✅ Existing Capabilities:**
- Comprehensive database schema (`meter_readings` table with time-series indexes)
- RESTful API endpoints (6 endpoints: submit, list, stats, admin minting)
- JWT-based user authentication with role-based access control
- WebSocket broadcasting via in-memory `WebSocketService`
- Validation logic (0 < kWh ≤ 100, timestamp within 7 days, no duplicates within ±15 minutes)
- Blockchain integration for energy token minting
- Pagination, filtering, and sorting support
- OpenAPI/Swagger documentation

**⚠️ Identified Gaps:**
- No device-level authentication (only user JWT authentication)
- No MQTT streaming for direct meter-to-gateway communication
- No Redis pub/sub for distributed WebSocket broadcasting (limits horizontal scaling)
- No batch reading submission endpoints
- No Redis caching for frequently accessed meter data
- No background job queue for async processing
- No TimescaleDB optimization for time-series queries
- Limited monitoring/alerting for meter-specific metrics

## Architecture Design

### Real-Time Data Flow

```
Smart Meters → MQTT Broker → API Gateway → Validation → Database
                    ↓                           ↓
              Device Auth              WebSocket Broadcast
                                              ↓
                                       Redis Pub/Sub
                                              ↓
                                    All Gateway Instances
                                              ↓
                                      Connected Clients
```

### Components to Implement

1. **MQTT Service** (`src/services/mqtt_service.rs`)
   - Subscribe to topics: `meters/{meter_id}/readings`
   - Parse incoming JSON/Protobuf messages
   - Validate device signatures
   - Bridge to existing `MeterService::submit_reading()`

2. **Device Authentication** (`src/services/device_auth_service.rs`)
   - Register device public keys
   - Verify HMAC signatures or validate mTLS certificates
   - Manage device lifecycle (provision, revoke, rotate keys)

3. **Distributed WebSocket** (enhance `src/services/websocket_service.rs`)
   - Publish events to Redis channels
   - Subscribe to Redis channels
   - Forward Redis messages to local WebSocket clients

4. **Batch Processing** (`src/services/batch_processing_service.rs`)
   - Accept bulk reading submissions
   - Queue jobs in Redis via `apalis` crate
   - Process batches with SQLx multi-row INSERT
   - Implement retry logic for failed validations

5. **Cache Layer** (`src/services/cache_service.rs`)
   - Cache user statistics (5-minute TTL)
   - Cache recent readings (1-minute TTL)
   - Invalidate cache on new submissions

## Implementation Steps

### Step 1: Implement MQTT Broker Integration

**Goal:** Enable direct meter-to-gateway streaming via MQTT protocol

**Tasks:**
- Add dependencies to `Cargo.toml`:
  ```toml
  rumqttc = "0.24"  # MQTT client
  rumqttd = "0.19"  # Optional: embedded broker
  ```
- Create `src/services/mqtt_service.rs`:
  ```rust
  pub struct MqttService {
      client: AsyncClient,
      meter_service: Arc<MeterService>,
      device_auth_service: Arc<DeviceAuthService>,
  }
  ```
- Implement topic subscription pattern: `meters/{meter_id}/readings`
- Parse incoming messages (JSON format initially)
- Validate device authentication (signature in payload)
- Call `meter_service.submit_reading()` with validated data
- Add MQTT configuration to `src/config/mqtt.rs`:
  ```rust
  pub struct MqttConfig {
      pub broker_host: String,
      pub broker_port: u16,
      pub client_id: String,
      pub use_tls: bool,
  }
  ```
- Update `AppState` in `src/main.rs` to include `mqtt_service`
- Add environment variables: `MQTT_BROKER_HOST`, `MQTT_BROKER_PORT`, `MQTT_USE_TLS`

**Expected Outcome:**
- Smart meters can publish readings to MQTT topics
- API gateway subscribes and processes messages in real-time
- Reduced HTTP overhead for high-frequency meter data

### Step 2: Add Device Authentication

**Goal:** Secure device-to-gateway communication with cryptographic verification

**Tasks:**
- Create migration `migrations/xxx_add_device_keys.sql`:
  ```sql
  CREATE TABLE device_keys (
      id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
      meter_id VARCHAR(50) NOT NULL UNIQUE,
      user_id UUID NOT NULL REFERENCES users(id),
      public_key TEXT NOT NULL,
      device_certificate TEXT,
      provisioned_at TIMESTAMPTZ DEFAULT NOW(),
      revoked_at TIMESTAMPTZ,
      last_used_at TIMESTAMPTZ,
      created_at TIMESTAMPTZ DEFAULT NOW()
  );
  
  CREATE INDEX idx_device_keys_meter ON device_keys(meter_id);
  CREATE INDEX idx_device_keys_user ON device_keys(user_id);
  ```
- Create `src/models/device.rs`:
  ```rust
  pub struct DeviceKey {
      pub id: Uuid,
      pub meter_id: String,
      pub user_id: Uuid,
      pub public_key: String,
      pub device_certificate: Option<String>,
      pub provisioned_at: DateTime<Utc>,
      pub revoked_at: Option<DateTime<Utc>>,
      pub last_used_at: Option<DateTime<Utc>>,
  }
  ```
- Create `src/services/device_auth_service.rs`:
  ```rust
  pub struct DeviceAuthService {
      db: PgPool,
  }
  
  impl DeviceAuthService {
      pub async fn register_device(&self, meter_id: &str, user_id: Uuid, public_key: &str) -> Result<DeviceKey>;
      pub async fn verify_signature(&self, meter_id: &str, message: &[u8], signature: &[u8]) -> Result<bool>;
      pub async fn revoke_device(&self, meter_id: &str) -> Result<()>;
      pub async fn update_last_used(&self, meter_id: &str) -> Result<()>;
  }
  ```
- Extend `SubmitReadingRequest` in `src/handlers/meter_reading.rs`:
  ```rust
  pub struct SubmitReadingRequest {
      pub kwh_amount: BigDecimal,
      pub reading_timestamp: DateTime<Utc>,
      pub meter_signature: Option<String>,
      pub device_signature: Option<String>,  // NEW: HMAC or Ed25519 signature
  }
  ```
- Implement signature verification using `ed25519-dalek` or `hmac` + `sha256`
- Add device registration endpoint: `POST /api/admin/devices/register`
- Add device revocation endpoint: `POST /api/admin/devices/{meter_id}/revoke`

**Expected Outcome:**
- Each smart meter has a registered public key
- All reading submissions include cryptographic signatures
- Invalid signatures are rejected before database insertion
- Device lifecycle management (provision, revoke, rotate)

### Step 3: Integrate Redis Pub/Sub for Distributed WebSocket

**Goal:** Enable horizontal scaling of WebSocket servers via Redis message broker

**Tasks:**
- Modify `src/services/websocket_service.rs`:
  ```rust
  pub struct WebSocketService {
      clients: Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<MarketEvent>>>>,
      redis: Arc<redis::aio::MultiplexedConnection>,  // NEW
      redis_subscriber: Arc<RwLock<redis::aio::PubSub>>,  // NEW
  }
  ```
- Implement Redis pub/sub methods:
  ```rust
  async fn publish_to_redis(&self, channel: &str, event: &MarketEvent) -> Result<()>;
  async fn subscribe_to_redis(&self, channel: &str) -> Result<()>;
  async fn handle_redis_message(&self, msg: redis::Msg) -> Result<()>;
  ```
- Update `broadcast()` method to publish to Redis channel:
  ```rust
  pub async fn broadcast(&self, event: MarketEvent) {
      // Publish to Redis (all instances receive)
      self.publish_to_redis("market:events", &event).await;
      
      // Local broadcast (backward compatibility)
      let clients = self.clients.read().await;
      for (_, tx) in clients.iter() {
          let _ = tx.send(event.clone());
      }
  }
  ```
- Create background task to subscribe to Redis channels:
  ```rust
  tokio::spawn(async move {
      let mut pubsub = redis_subscriber.write().await;
      pubsub.subscribe("market:events").await?;
      pubsub.subscribe("meter:updates").await?;
      
      loop {
          let msg = pubsub.on_message().next().await?;
          websocket_service.handle_redis_message(msg).await?;
      }
  });
  ```
- Add Redis pub/sub configuration to `src/config/redis.rs`
- Add meter-specific events to `MarketEvent` enum:
  ```rust
  pub enum MarketEvent {
      // Existing events...
      MeterReadingSubmitted { meter_id, user_id, kwh_amount, timestamp },
      MeterReadingMinted { reading_id, tx_signature, amount },
  }
  ```

**Expected Outcome:**
- Multiple API gateway instances can run behind load balancer
- WebSocket events broadcast to all instances via Redis
- Clients connected to any instance receive real-time updates
- Horizontal scaling without session affinity requirements

### Step 4: Create Batch Reading Endpoints

**Goal:** Support high-volume meter data ingestion via bulk API

**Tasks:**
- Add `apalis` to `Cargo.toml` for background job processing:
  ```toml
  apalis = { version = "0.5", features = ["redis"] }
  apalis-redis = "0.5"
  ```
- Create `src/models/batch.rs`:
  ```rust
  pub struct BatchReadingRequest {
      pub readings: Vec<SubmitReadingRequest>,
  }
  
  pub struct BatchReadingResponse {
      pub total_submitted: usize,
      pub successful: usize,
      pub failed: usize,
      pub errors: Vec<BatchError>,
  }
  
  pub struct BatchError {
      pub index: usize,
      pub meter_id: String,
      pub error: String,
  }
  ```
- Create `src/services/batch_processing_service.rs`:
  ```rust
  pub struct BatchProcessingService {
      db: PgPool,
      meter_service: Arc<MeterService>,
      job_queue: Storage<RedisStorage<BatchReadingJob>>,
  }
  
  impl BatchProcessingService {
      pub async fn submit_batch(&self, readings: Vec<SubmitReadingRequest>) -> Result<BatchReadingResponse>;
      pub async fn process_batch_job(&self, job: BatchReadingJob) -> Result<()>;
  }
  ```
- Implement SQLx batch insert:
  ```rust
  async fn insert_readings_batch(&self, readings: Vec<MeterReading>) -> Result<()> {
      let mut query_builder = QueryBuilder::new(
          "INSERT INTO meter_readings (user_id, wallet_address, meter_id, kwh_amount, reading_timestamp, submitted_at)"
      );
      
      query_builder.push_values(readings, |mut b, reading| {
          b.push_bind(reading.user_id)
           .push_bind(&reading.wallet_address)
           .push_bind(&reading.meter_id)
           .push_bind(&reading.kwh_amount)
           .push_bind(reading.reading_timestamp)
           .push_bind(Utc::now());
      });
      
      query_builder.build().execute(&self.db).await?;
      Ok(())
  }
  ```
- Add handler `src/handlers/meter_reading.rs`:
  ```rust
  #[utoipa::path(
      post,
      path = "/api/meters/submit-readings-batch",
      tag = "meters",
      request_body = BatchReadingRequest,
      security(("bearer_auth" = []))
  )]
  pub async fn submit_readings_batch(
      State(state): State<AppState>,
      AuthenticatedUser(user): AuthenticatedUser,
      Json(request): Json<BatchReadingRequest>,
  ) -> Result<Json<BatchReadingResponse>, ApiError>
  ```
- Configure job queue in `src/main.rs`:
  ```rust
  let redis_storage = RedisStorage::new(redis_client);
  let job_queue = WorkerBuilder::new("meter-batch-worker")
      .backend(redis_storage)
      .build_fn(|job: BatchReadingJob| async move {
          batch_service.process_batch_job(job).await
      });
  ```
- Add validation: max 1000 readings per batch
- Implement retry logic with exponential backoff (3 attempts)

**Expected Outcome:**
- Single API call can submit hundreds/thousands of readings
- Background processing prevents HTTP timeout on large batches
- Failed readings tracked with detailed error messages
- Retry mechanism handles transient failures

### Step 5: Add TimescaleDB Hypertable

**Goal:** Optimize time-series queries and enable automatic data retention

**Tasks:**
- Ensure TimescaleDB extension installed in PostgreSQL:
  ```sql
  CREATE EXTENSION IF NOT EXISTS timescaledb CASCADE;
  ```
- Create migration `migrations/xxx_convert_to_timescaledb.sql`:
  ```sql
  -- Convert meter_readings to hypertable
  SELECT create_hypertable(
      'meter_readings',
      'reading_timestamp',
      if_not_exists => TRUE,
      migrate_data => TRUE
  );
  
  -- Create continuous aggregate for hourly stats
  CREATE MATERIALIZED VIEW meter_hourly_stats
  WITH (timescaledb.continuous) AS
  SELECT 
      time_bucket('1 hour', reading_timestamp) AS hour_bucket,
      meter_id,
      user_id,
      COUNT(*) as reading_count,
      SUM(kwh_amount) as total_kwh,
      AVG(kwh_amount) as avg_kwh,
      MIN(kwh_amount) as min_kwh,
      MAX(kwh_amount) as max_kwh
  FROM meter_readings
  GROUP BY hour_bucket, meter_id, user_id;
  
  -- Refresh policy (every 30 minutes)
  SELECT add_continuous_aggregate_policy('meter_hourly_stats',
      start_offset => INTERVAL '3 hours',
      end_offset => INTERVAL '1 hour',
      schedule_interval => INTERVAL '30 minutes');
  
  -- Retention policy (compress after 30 days, drop after 1 year)
  SELECT add_compression_policy('meter_readings', INTERVAL '30 days');
  SELECT add_retention_policy('meter_readings', INTERVAL '1 year');
  ```
- Create service method to query aggregates:
  ```rust
  pub async fn get_hourly_stats(
      &self,
      meter_id: &str,
      start: DateTime<Utc>,
      end: DateTime<Utc>,
  ) -> Result<Vec<HourlyStats>> {
      sqlx::query_as!(
          HourlyStats,
          r#"SELECT hour_bucket, meter_id, reading_count, total_kwh, avg_kwh
             FROM meter_hourly_stats
             WHERE meter_id = $1 AND hour_bucket BETWEEN $2 AND $3
             ORDER BY hour_bucket ASC"#,
          meter_id, start, end
      ).fetch_all(&self.db).await
  }
  ```
- Add endpoint: `GET /api/meters/{meter_id}/stats/hourly`
- Update documentation with TimescaleDB benefits

**Expected Outcome:**
- Fast time-range queries with automatic partitioning
- Pre-computed hourly/daily statistics
- Automatic data compression after 30 days
- Automatic data deletion after 1 year
- Reduced storage costs for historical data

### Step 6: Implement Distributed Caching

**Goal:** Reduce database load with Redis caching for frequently accessed data

**Tasks:**
- Create `src/services/cache_service.rs`:
  ```rust
  pub struct CacheService {
      redis: Arc<redis::aio::MultiplexedConnection>,
  }
  
  impl CacheService {
      pub async fn get_user_stats(&self, user_id: Uuid) -> Result<Option<UserStatsResponse>>;
      pub async fn set_user_stats(&self, user_id: Uuid, stats: &UserStatsResponse, ttl: u64) -> Result<()>;
      pub async fn invalidate_user_stats(&self, user_id: Uuid) -> Result<()>;
      
      pub async fn get_recent_readings(&self, meter_id: &str) -> Result<Option<Vec<MeterReading>>>;
      pub async fn set_recent_readings(&self, meter_id: &str, readings: &[MeterReading], ttl: u64) -> Result<()>;
      pub async fn invalidate_readings(&self, meter_id: &str) -> Result<()>;
  }
  ```
- Implement cache keys pattern:
  ```rust
  // User statistics (5-minute TTL)
  let cache_key = format!("meter:stats:user:{}", user_id);
  
  // Recent readings (1-minute TTL)
  let cache_key = format!("meter:recent:{}", meter_id);
  
  // Order book snapshot (10-second TTL)
  let cache_key = "market:orderbook:snapshot";
  ```
- Modify `MeterService::get_user_stats()`:
  ```rust
  pub async fn get_user_stats(&self, user_id: Uuid) -> Result<UserStatsResponse> {
      // Try cache first
      if let Some(cached) = self.cache_service.get_user_stats(user_id).await? {
          return Ok(cached);
      }
      
      // Fetch from database
      let stats = self.calculate_user_stats(user_id).await?;
      
      // Cache for 5 minutes
      self.cache_service.set_user_stats(user_id, &stats, 300).await?;
      
      Ok(stats)
  }
  ```
- Implement cache invalidation on write operations:
  ```rust
  pub async fn submit_reading(&self, request: SubmitReadingRequest, user_id: Uuid) -> Result<MeterReading> {
      // Insert reading
      let reading = self.insert_reading(request, user_id).await?;
      
      // Invalidate caches
      self.cache_service.invalidate_user_stats(user_id).await?;
      self.cache_service.invalidate_readings(&reading.meter_id).await?;
      
      Ok(reading)
  }
  ```
- Add cache hit/miss metrics:
  ```rust
  pub fn track_cache_operation(operation: &str, hit: bool) {
      gauge!("cache.operations", 1.0, "type" => operation, "hit" => hit.to_string());
  }
  ```

**Expected Outcome:**
- User statistics queries served from cache (5-minute TTL)
- Recent readings cached (1-minute TTL)
- Reduced database load by 50-70% for read-heavy endpoints
- Automatic cache invalidation on new submissions
- Cache metrics tracked via Prometheus

## Architecture Decisions

### Decision 1: MQTT Broker Deployment

**Options:**
1. **Embedded Broker** (`rumqttd` within API gateway)
   - ✅ Pros: Simplified deployment, fewer moving parts
   - ❌ Cons: Limited scalability, single point of failure
   
2. **External Broker** (Mosquitto/HiveMQ/AWS IoT Core)
   - ✅ Pros: Independent scaling, high availability, managed options
   - ❌ Cons: Additional infrastructure, operational complexity

**Recommendation:** Start with **embedded broker** for Phase 1 (< 1000 meters), migrate to **external broker** for production scale (> 1000 meters).

### Decision 2: Device Authentication Method

**Options:**
1. **mTLS (Mutual TLS with Client Certificates)**
   - ✅ Pros: Industry standard, strong security, no shared secrets
   - ❌ Cons: Certificate management overhead, PKI infrastructure required
   
2. **HMAC Signatures (Shared Secret per Device)**
   - ✅ Pros: Simple implementation, no PKI, easy key rotation
   - ❌ Cons: Shared secrets must be provisioned securely

3. **JWT Tokens for Devices**
   - ✅ Pros: Reuse existing JWT infrastructure
   - ❌ Cons: Token refresh complexity, storage overhead

**Recommendation:** Use **HMAC signatures** for Phase 1 (rapid development), implement **mTLS** for production (security compliance).

### Decision 3: Message Format

**Options:**
1. **JSON** (current standard)
   - ✅ Pros: Human-readable, debugging ease, API consistency
   - ❌ Cons: Higher bandwidth (30-50% larger than binary)
   
2. **Protocol Buffers**
   - ✅ Pros: Compact size, schema validation, backward compatibility
   - ❌ Cons: Requires `.proto` definitions, binary format (harder to debug)
   
3. **MessagePack**
   - ✅ Pros: JSON-compatible, smaller size, fast serialization
   - ❌ Cons: Less tooling support than Protobuf

**Recommendation:** Use **JSON** for Phase 1 (maintain consistency), evaluate **Protocol Buffers** for Phase 2 if bandwidth becomes bottleneck (>10,000 meters sending every 15 minutes).

### Decision 4: Batch Processing Strategy

**Options:**
1. **Synchronous Processing** (process batch in HTTP handler)
   - ✅ Pros: Simple implementation, immediate feedback
   - ❌ Cons: HTTP timeout risk for large batches, blocks worker thread
   
2. **Asynchronous Job Queue** (`apalis` + Redis)
   - ✅ Pros: No HTTP timeout, retry mechanism, monitoring
   - ❌ Cons: Eventual consistency, additional infrastructure

**Recommendation:** Use **asynchronous job queue** with immediate validation in HTTP handler (quick response with job ID, process in background).

## Performance Targets

### Throughput Requirements
- **Small Scale** (Phase 1): 1,000 meters × 4 readings/hour = 1,111 readings/second peak
- **Medium Scale** (Phase 2): 10,000 meters × 4 readings/hour = 11,111 readings/second peak
- **Large Scale** (Phase 3): 100,000 meters × 4 readings/hour = 111,111 readings/second peak

### Latency Requirements
- **MQTT Message Processing**: < 100ms (meter → database)
- **WebSocket Broadcast**: < 50ms (event → all clients)
- **API Response Time**: p95 < 200ms, p99 < 500ms
- **Batch Processing**: < 5 seconds for 1,000 readings

### Availability Requirements
- **Uptime**: 99.9% (8.76 hours downtime/year)
- **MQTT Broker**: 99.95% (4.38 hours downtime/year)
- **Database**: 99.99% (52 minutes downtime/year)

## Monitoring & Observability

### Metrics to Track (Prometheus)

**Meter Ingestion Metrics:**
- `meter_readings_submitted_total` (counter by meter_id)
- `meter_readings_validation_errors_total` (counter by error_type)
- `meter_readings_processing_duration_seconds` (histogram)
- `mqtt_messages_received_total` (counter by topic)
- `mqtt_connection_errors_total` (counter)

**Device Authentication Metrics:**
- `device_signature_verifications_total` (counter by result: valid/invalid)
- `device_authentication_duration_seconds` (histogram)
- `device_keys_registered_total` (counter)
- `device_keys_revoked_total` (counter)

**WebSocket Metrics:**
- `websocket_clients_connected` (gauge)
- `websocket_messages_sent_total` (counter by event_type)
- `redis_pubsub_messages_total` (counter by channel)
- `websocket_broadcast_duration_seconds` (histogram)

**Batch Processing Metrics:**
- `batch_jobs_submitted_total` (counter)
- `batch_jobs_completed_total` (counter by status: success/failed)
- `batch_processing_duration_seconds` (histogram)
- `batch_readings_per_job` (histogram)

**Cache Metrics:**
- `cache_hits_total` (counter by cache_type)
- `cache_misses_total` (counter by cache_type)
- `cache_invalidations_total` (counter by cache_type)
- `cache_operation_duration_seconds` (histogram)

### Alerting Rules

**Critical Alerts:**
- MQTT broker disconnected for > 1 minute
- Device authentication failure rate > 10%
- Batch job failure rate > 5%
- Database connection pool exhausted
- Redis connection failure

**Warning Alerts:**
- Meter reading validation error rate > 2%
- WebSocket client connection failures > 5%
- Cache hit rate < 50%
- API response time p95 > 300ms
- Batch job queue depth > 100

### Logging Strategy

**Structured Logging** (JSON format via `tracing` crate):
```rust
tracing::info!(
    meter_id = %reading.meter_id,
    user_id = %user_id,
    kwh_amount = %reading.kwh_amount,
    source = "mqtt",
    "Meter reading submitted successfully"
);

tracing::warn!(
    meter_id = %meter_id,
    error = %e,
    "Device signature verification failed"
);
```

**Log Levels by Component:**
- Production: INFO for handlers, WARN for services, ERROR for critical failures
- Development: DEBUG for all components, TRACE for WebSocket/MQTT internals

## Testing Strategy

### Unit Tests
- Device signature verification (valid/invalid/expired)
- Batch validation logic (max size, duplicate detection)
- Cache invalidation triggers
- MQTT message parsing (valid/malformed JSON)

### Integration Tests
- End-to-end meter reading flow (MQTT → DB → WebSocket)
- Batch submission with partial failures
- Redis pub/sub message delivery across instances
- Cache hit/miss scenarios

### Load Tests
- 1,000 concurrent MQTT connections publishing every 15 seconds
- 10,000 readings batch submission
- 100 concurrent WebSocket clients receiving updates
- Redis pub/sub throughput (10,000 events/second)

### Test Scripts to Create
```bash
# scripts/test-mqtt-ingestion.sh
# Publish 1000 readings via MQTT, verify database insertion

# scripts/test-batch-api.sh
# Submit batch of 1000 readings via HTTP, verify async processing

# scripts/test-distributed-websocket.sh
# Connect to multiple gateway instances, verify all receive events

# scripts/test-cache-performance.sh
# Measure cache hit rate under load, verify invalidation
```

## Deployment Considerations

### Docker Compose Enhancement
```yaml
services:
  mqtt-broker:
    image: eclipse-mosquitto:2
    ports:
      - "1883:1883"
      - "8883:8883"
    volumes:
      - ./mosquitto.conf:/mosquitto/config/mosquitto.conf
      - ./certs:/mosquitto/certs
  
  timescaledb:
    image: timescale/timescaledb:latest-pg15
    environment:
      POSTGRES_DB: gridtokenx
      POSTGRES_USER: gridtokenx_user
      POSTGRES_PASSWORD: gridtokenx_password
    ports:
      - "5432:5432"
  
  redis:
    image: redis:7-alpine
    command: redis-server --requirepass ${REDIS_PASSWORD}
    ports:
      - "6379:6379"
  
  api-gateway-1:
    build: .
    environment:
      - MQTT_BROKER_HOST=mqtt-broker
      - REDIS_URL=redis://:${REDIS_PASSWORD}@redis:6379
      - DATABASE_URL=postgresql://...@timescaledb:5432/gridtokenx
    depends_on:
      - mqtt-broker
      - redis
      - timescaledb
  
  api-gateway-2:
    # Second instance for testing horizontal scaling
    build: .
    environment:
      - MQTT_BROKER_HOST=mqtt-broker
      - REDIS_URL=redis://:${REDIS_PASSWORD}@redis:6379
      - DATABASE_URL=postgresql://...@timescaledb:5432/gridtokenx
    depends_on:
      - mqtt-broker
      - redis
      - timescaledb
```

### Environment Variables
```bash
# MQTT Configuration
MQTT_BROKER_HOST="localhost"
MQTT_BROKER_PORT="1883"
MQTT_CLIENT_ID="gridtokenx-gateway"
MQTT_USE_TLS="false"
MQTT_USERNAME=""
MQTT_PASSWORD=""

# Device Authentication
DEVICE_AUTH_METHOD="hmac"  # or "mtls"
DEVICE_KEY_ROTATION_DAYS="90"

# Redis Pub/Sub
REDIS_PUBSUB_CHANNELS="market:events,meter:updates"

# Batch Processing
BATCH_MAX_SIZE="1000"
BATCH_WORKER_CONCURRENCY="4"
BATCH_RETRY_ATTEMPTS="3"

# Caching
CACHE_USER_STATS_TTL="300"  # 5 minutes
CACHE_RECENT_READINGS_TTL="60"  # 1 minute
```

### Kubernetes Deployment (Future)
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: api-gateway
spec:
  replicas: 3
  selector:
    matchLabels:
      app: api-gateway
  template:
    metadata:
      labels:
        app: api-gateway
    spec:
      containers:
      - name: api-gateway
        image: gridtokenx/api-gateway:latest
        env:
        - name: MQTT_BROKER_HOST
          value: "mqtt-service"
        - name: REDIS_URL
          valueFrom:
            secretKeyRef:
              name: redis-secret
              key: url
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "512Mi"
            cpu: "500m"
---
apiVersion: v1
kind: Service
metadata:
  name: api-gateway-service
spec:
  type: LoadBalancer
  selector:
    app: api-gateway
  ports:
  - port: 8080
    targetPort: 8080
```

## Security Considerations

### Device Key Management
- Store device public keys encrypted at rest (PostgreSQL `pgcrypto`)
- Implement key rotation policy (90-day expiration)
- Audit log all device provisioning/revocation events
- Rate limit device registration endpoint (5 req/min per IP)

### MQTT Security
- Enable TLS/SSL for production (port 8883)
- Use strong authentication (client certificates or password)
- Implement ACL for topic subscriptions (devices can only publish to their own topics)
- Monitor for suspicious activity (rapid topic switching, malformed messages)

### Redis Security
- Enable authentication (`requirepass` in redis.conf)
- Use TLS for Redis connections in production
- Implement network isolation (Redis on private subnet)
- Monitor for command injection attempts

### API Security Enhancements
- Add API key authentication for meter devices (alternative to JWT)
- Implement device-level rate limiting (separate from user rate limits)
- Add CAPTCHA for device registration endpoint
- Log all authentication failures with IP address

## Migration Plan (Phased Rollout)

### Phase 1: Foundation (Week 1-2)
- ✅ Add device authentication (HMAC signatures)
- ✅ Implement basic MQTT service (embedded broker)
- ✅ Create batch reading endpoints
- ✅ Add unit tests and integration tests
- **Rollout:** Internal testing with 10 test meters

### Phase 2: Real-Time Enhancements (Week 3-4)
- ✅ Integrate Redis pub/sub for WebSocket
- ✅ Implement distributed caching
- ✅ Add TimescaleDB hypertables
- ✅ Set up monitoring and alerting
- **Rollout:** Pilot program with 100 prosumer meters

### Phase 3: Production Readiness (Week 5-6)
- ✅ Deploy external MQTT broker (Mosquitto)
- ✅ Implement mTLS device authentication
- ✅ Add load balancing and auto-scaling
- ✅ Conduct load testing (10,000 concurrent meters)
- **Rollout:** Full production launch

### Phase 4: Optimization (Week 7-8)
- ✅ Implement Protocol Buffers for MQTT messages
- ✅ Add edge caching with CDN
- ✅ Optimize database queries and indexes
- ✅ Implement advanced analytics (continuous aggregates)
- **Rollout:** Scale to 100,000 meters

## Success Metrics

### Technical KPIs
- ✅ MQTT message processing latency < 100ms (p95)
- ✅ WebSocket broadcast latency < 50ms (p95)
- ✅ Batch processing throughput > 1,000 readings/second
- ✅ Cache hit rate > 70%
- ✅ API uptime > 99.9%

### Business KPIs
- ✅ Support 10,000+ active meters
- ✅ Process 1M+ readings per day
- ✅ Zero data loss incidents
- ✅ < 0.1% invalid signature rejection rate
- ✅ Real-time market updates latency < 1 second

## Conclusion

This comprehensive plan transforms the GridTokenX API gateway from a HTTP-based meter reading system into a high-performance, real-time data ingestion platform capable of handling thousands of concurrent smart meters. The phased implementation approach minimizes risk while delivering incremental value, and the architecture supports horizontal scaling to handle future growth.

**Next Steps:**
1. Review and approve architecture decisions
2. Create detailed task breakdown in project management tool
3. Set up development environment with MQTT broker and TimescaleDB
4. Begin Phase 1 implementation (device authentication + basic MQTT)
5. Schedule weekly progress reviews and milestone assessments
