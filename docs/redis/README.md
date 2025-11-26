# Redis Implementation for GridTokenX API Gateway

## 🎯 **Overview**

This document provides a comprehensive overview of Redis implementation within the GridTokenX P2P Energy Trading Platform. Redis serves as the primary caching layer, real-time data store, and message broker for the application.

## 📁 **File Structure**

```
docs/redis/
├── README.md                           # This overview document
├── redis_implementation_checklist.md    # Detailed implementation checklist
└── planning/                          # Future enhancement plans

docker/redis/
├── redis.conf                         # Production-ready Redis configuration

docker/grafana/provisioning/dashboards/
└── redis-dashboard.json               # Grafana dashboard for Redis monitoring

scripts/
├── setup_redis.sh                     # Redis setup and configuration script
├── redis_health_check.sh              # Redis health monitoring script
└── redis_backup.sh                   # Redis backup automation script
```

## 🚀 **Quick Start**

### **1. Setup Redis**
```bash
# Run the setup script
./scripts/setup_redis.sh

# Or manually start Redis with Docker
docker-compose up -d redis
```

### **2. Verify Installation**
```bash
# Check Redis status
docker-compose ps redis

# Test connection
redis-cli -a $REDIS_PASSWORD ping

# View logs
docker-compose logs -f redis
```

### **3. Access Monitoring**
- **Grafana Dashboard**: http://localhost:3001
- **Prometheus Metrics**: http://localhost:9090
- **Redis CLI**: `redis-cli -a $REDIS_PASSWORD`

## 🔧 **Current Implementation**

### **Core Features**
- ✅ **Redis 7-Alpine** container with security hardening
- ✅ **Connection pooling** with multiplexed async connections
- ✅ **Cache Service** with TTL support and JSON serialization
- ✅ **Market Data** persistence with sorted sets
- ✅ **Rate Limiting** with Redis counters
- ✅ **Health Monitoring** with Prometheus integration
- ✅ **Graceful Degradation** when Redis is unavailable

### **Usage Patterns**

#### **Cache Service**
```rust
// Initialize cache service
let cache_service = CacheService::new(&config.redis_url).await?;

// Set cache with TTL
cache_service.set_with_ttl("user:123", &user_data, 300).await?;

// Get cached data
let cached_user: Option<User> = cache_service.get("user:123").await?;
```

#### **Market Data**
```rust
// Order book snapshots stored in Redis sorted sets
// Buy orders: orderbook:buy (sorted by price descending)
// Sell orders: orderbook:sell (sorted by price ascending)
// Metadata: orderbook:metadata (best bid/ask, spread)
```

#### **Rate Limiting**
```rust
// API rate limiting with Redis counters
let requests = cache_service.increment(&rate_limit_key).await?;
if requests > max_requests {
    return Err(ApiError::RateLimitExceeded);
}
```

## 📊 **Monitoring & Metrics**

### **Key Metrics**
- **Cache Hit Rate**: Target >80%
- **Memory Usage**: Monitor Redis memory consumption
- **Operations/sec**: Track Redis command throughput
- **Connected Clients**: Monitor connection pool usage
- **Slow Queries**: Identify performance bottlenecks

### **Grafana Dashboard**
Access the pre-configured Redis dashboard at:
- **URL**: http://localhost:3001
- **Dashboard**: "GridTokenX Redis Monitoring"
- **Panels**: Memory, CPU, Cache Performance, Network I/O

### **Prometheus Metrics**
Redis metrics are automatically collected and exposed at:
- **Endpoint**: http://localhost:9090/metrics
- **Scrape Interval**: 15 seconds

## 🔐 **Security Configuration**

### **Authentication**
```bash
# Current setup (development)
REDIS_URL=redis://localhost:6379

# Recommended production setup
REDIS_URL=redis://:strong_password@redis:6379
```

### **Security Features**
- ✅ **Password Authentication** (configured via environment)
- ✅ **Command Renaming** (dangerous commands disabled)
- ✅ **Network Isolation** (private subnet in production)
- ✅ **TLS Support** (rediss:// protocol ready)

### **Production Hardening**
```bash
# Enable Redis authentication
REDIS_PASSWORD=$(openssl rand -base64 32)

# Update environment
echo "REDIS_PASSWORD=$REDIS_PASSWORD" >> .env

# Restart Redis with authentication
docker-compose up -d redis
```

## 🎛️ **Configuration**

### **Redis Configuration** (`docker/redis/redis.conf`)

Key settings for GridTokenX:
```conf
# Memory management
maxmemory 2gb
maxmemory-policy allkeys-lru

# Persistence
save 900 1
save 300 10
save 60 10000
appendonly yes
appendfsync everysec

# Security
requirepass ${REDIS_PASSWORD}
rename-command CONFIG ""
rename-command FLUSHALL ""

# Performance
tcp-keepalive 300
timeout 300
maxclients 10000
```

### **Environment Variables**
```bash
# Redis connection
REDIS_URL=redis://:password@localhost:6379
REDIS_POOL_SIZE=20
REDIS_CONNECTION_TIMEOUT=5
REDIS_COMMAND_TIMEOUT=3

# Monitoring
REDIS_PASSWORD=your_secure_password
```

## 📈 **Performance Optimization**

### **Cache Strategy**
- **TTL Management**: Different TTL for different data types
- **Key Naming**: Structured namespace for efficient key management
- **Memory Eviction**: LRU policy for optimal memory usage
- **Connection Pooling**: Multiplexed connections for high concurrency

### **Best Practices**
1. **Set Appropriate TTLs**: Balance freshness with performance
2. **Use Structured Keys**: Follow naming conventions for consistency
3. **Monitor Memory Usage**: Prevent memory exhaustion
4. **Implement Fallbacks**: Handle Redis unavailability gracefully
5. **Use Pipelining**: Batch operations for better performance

## 🔄 **Data Patterns**

### **Current Key Namespace**
```
# User data
user:profile:{user_id}           - User profiles (TTL: 1h)
user:wallet:{user_id}            - Wallet info (TTL: 30m)

# Market data
orderbook:buy                    - Buy orders (sorted set)
orderbook:sell                   - Sell orders (sorted set)
orderbook:metadata               - Market metadata (TTL: 5m)
market:current_epoch             - Current epoch info

# Rate limiting
rate_limit:api:{ip}:{endpoint}   - API limits (TTL: 60s)
rate_limit:meter_verify:{user_id} - Meter verification (TTL: 300s)

# Token data
token:balance:{wallet}:{mint}    - Token balances (TTL: 5m)
erc:certificate:{cert_id}        - ERC certificates (TTL: 24h)
```

### **Data Types Used**
- **Strings**: Simple key-value storage (user profiles, metadata)
- **Sorted Sets**: Order book with price-based ordering
- **Hashes**: Order metadata and complex objects
- **Counters**: Rate limiting and analytics

## 🚨 **Troubleshooting**

### **Common Issues**

#### **Connection Failed**
```bash
# Check if Redis is running
docker-compose ps redis

# Check Redis logs
docker-compose logs redis

# Test connection manually
redis-cli -a $REDIS_PASSWORD ping
```

#### **Memory Issues**
```bash
# Check memory usage
redis-cli -a $REDIS_PASSWORD info memory

# Monitor memory in real-time
redis-cli -a $REDIS_PASSWORD --latency-monitor

# Check slow queries
redis-cli -a $REDIS_PASSWORD slowlog get 10
```

#### **Performance Issues**
```bash
# Check connection pool
redis-cli -a $REDIS_PASSWORD info clients

# Monitor commands
redis-cli -a $REDIS_PASSWORD monitor

# Check stats
redis-cli -a $REDIS_PASSWORD info stats
```

### **Health Checks**
```bash
# Run health check script
./scripts/redis_health_check.sh

# Or manually check
redis-cli -a $REDIS_PASSWORD ping && echo "Healthy" || echo "Unhealthy"
```

## 📋 **Maintenance Tasks**

### **Daily**
- Monitor Redis metrics in Grafana
- Check memory usage and performance
- Review slow query logs

### **Weekly**
- Review and rotate Redis passwords
- Check backup procedures
- Analyze cache hit rates

### **Monthly**
- Update Redis configuration as needed
- Review and optimize TTL values
- Plan capacity upgrades

## 🚀 **Future Enhancements**

### **Phase 1: Production Hardening (Immediate)**
- [ ] Enable Redis authentication with strong passwords
- [ ] Implement TLS encryption for Redis connections
- [ ] Configure Redis on private network
- [ ] Set up automated backup procedures

### **Phase 2: Advanced Features (Next Sprint)**
- [ ] Redis Pub/Sub for WebSocket scaling
- [ ] Distributed locking with Redlock
- [ ] Advanced rate limiting with sliding windows
- [ ] Cache warming strategies

### **Phase 3: High Availability (Future)**
- [ ] Redis Cluster for horizontal scaling
- [ ] Sentinel for automatic failover
- [ ] Cross-datacenter replication
- [ ] Redis Streams for event sourcing

## 📚 **Resources**

### **Documentation**
- [Redis Official Documentation](https://redis.io/documentation)
- [Redis Configuration Guide](https://redis.io/topics/config)
- [Redis Best Practices](https://redis.io/topics/memory-optimization)

### **Monitoring Tools**
- [Redis Insight](https://redis.com/redis-enterprise/redis-insight/)
- [Prometheus Redis Exporter](https://github.com/oliver006/redis_exporter)
- [Grafana Redis Dashboard](https://grafana.com/grafana/dashboards/763)

### **Community**
- [Redis Community](https://redis.com/community/)
- [Redis University](https://university.redis.com/)
- [Redis Discord](https://discord.gg/redis)

---

## 📞 **Support**

For Redis-related issues in GridTokenX:

1. **Check Logs**: `docker-compose logs redis`
2. **Run Health Check**: `./scripts/redis_health_check.sh`
3. **Review Metrics**: Grafana dashboard at http://localhost:3001
4. **Consult Documentation**: This document and implementation checklist

**Engineering Department**: engineering@gridtokenx.com  
**Documentation Updated**: November 26, 2025  
**Next Review**: December 10, 2025
