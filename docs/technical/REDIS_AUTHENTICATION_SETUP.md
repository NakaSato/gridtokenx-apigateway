# Redis Authentication Setup Guide

This guide provides comprehensive instructions for setting up Redis authentication for the GridTokenX API Gateway to ensure secure cache and session management.

## Overview

Redis authentication is crucial for securing your application's caching layer, session storage, and real-time data management. Without authentication, your Redis instance is vulnerable to unauthorized access and potential data breaches.

## Security Benefits

- **Unauthorized Access Prevention**: Only authenticated clients can connect to Redis
- **Data Protection**: Prevents unauthorized read/write operations on cached data
- **Network Security**: Adds an additional layer of security beyond network-level controls
- **Compliance**: Helps meet security requirements for production deployments

## Quick Start

### Option 1: Automated Setup (Recommended)

Use the provided setup script for automatic Redis authentication configuration:

```bash
# Auto-generate password and configure Redis
./scripts/setup-redis-auth.sh

# Or specify a custom password
./scripts/setup-redis-auth.sh --password your_secure_password

# For remote Redis server
./scripts/setup-redis-auth.sh --host redis.example.com --port 6380

# Dry run to see what would be changed
./scripts/setup-redis-auth.sh --dry-run
```

### Option 2: Manual Setup

Follow these steps for manual configuration:

#### 1. Generate a Secure Password

```bash
# Generate a secure 25-character password
openssl rand -base64 32 | tr -d "=+/" | cut -c1-25
```

#### 2. Configure Redis

Edit your Redis configuration file (`/etc/redis/redis.conf`):

```conf
# Enable password authentication
requirepass your_secure_password_here

# Enable protected mode (recommended)
protected-mode yes

# Optional: Bind to specific interfaces
bind 127.0.0.1  # Local only
# bind 0.0.0.0   # All interfaces (use with firewall)
```

#### 3. Restart Redis

```bash
# Using systemd
sudo systemctl restart redis

# Or manual restart
sudo pkill redis-server
redis-server /etc/redis/redis.conf --daemonize yes
```

#### 4. Test Authentication

```bash
# Test without password (should fail)
redis-cli ping
# Should return: NOAUTH Authentication required

# Test with password (should succeed)
redis-cli -a your_secure_password ping
# Should return: PONG
```

## Configuration

### Environment Variables

Update your environment files with the authenticated Redis URL:

#### Development (`local.env`)
```env
# Redis connection with authentication
REDIS_URL=redis://:redis_dev_password@localhost:6379
REDIS_POOL_SIZE=20
```

#### Production (`.env`)
```env
# Use a strong, unique password
REDIS_URL=redis://:your_production_redis_password@redis:6379
REDIS_POOL_SIZE=10

# For SSL/TLS connections
# REDIS_URL=rediss://:your_production_redis_password@redis:6379
```

### Connection URL Format

```
redis://[:password@]host[:port][/db-number][?option=value]
```

Examples:
- `redis://:password@localhost:6379` - Basic authentication
- `redis://:password@redis.example.com:6380/2` - With database number
- `rediss://:password@redis.example.com:6379` - With SSL/TLS

## Application Integration

### Code Changes

The application code already supports Redis authentication. The enhanced connection logic in `src/main.rs`:

1. **Authentication Detection**: Automatically detects if Redis URL contains authentication
2. **Connection Testing**: Validates Redis connectivity and authentication
3. **Error Handling**: Provides detailed error messages for common issues
4. **Security Warnings**: Alerts when production environments lack authentication

### Monitoring Authentication Status

When you start the application, you'll see authentication status:

```
✅ Redis connection established (authenticated)
```

Or a warning if not authenticated:

```
⚠️  Redis connection established (no authentication - consider adding password)
```

### Production Security Warning

In production environments without authentication, you'll see:

```
🚨 SECURITY WARNING: Redis connection in production is not authenticated!
```

## Testing and Validation

### 1. Application Startup Test

```bash
# Start the application with authentication
cargo run

# Check logs for Redis connection status
```

### 2. Functional Tests

Run the integration tests to ensure all Redis-dependent features work:

```bash
# Basic functionality tests
./scripts/test-market-clearing.sh

# Authenticated tests
./scripts/test-market-clearing-authenticated.sh

# Complete flow tests
./scripts/test-complete-flow.sh
```

### 3. Connection Test Script

Create a simple test to verify Redis connectivity:

```bash
#!/bin/bash
# test-redis-connection.sh

REDIS_URL="${REDIS_URL:-redis://localhost:6379}"

echo "Testing Redis connection: $REDIS_URL"

# Extract password if present
if [[ $REDIS_URL =~ :([^@]+)@ ]]; then
    PASSWORD="${BASH_REMATCH[1]}"
    HOST=$(echo $REDIS_URL | sed 's|.*@\([^:]*\):.*|\1|')
    PORT=$(echo $REDIS_URL | sed 's|.*:\([0-9]*\).*|\1|')
    
    # Test with password
    redis-cli -h "$HOST" -p "$PORT" -a "$PASSWORD" ping
else
    # Test without password
    redis-cli ping
fi
```

## Troubleshooting

### Common Issues

#### 1. "NOAUTH Authentication required"
**Cause**: Redis requires password but none provided  
**Solution**: Update `REDIS_URL` to include password: `redis://:password@host:port`

#### 2. "Connection refused"
**Cause**: Redis not running or wrong host/port  
**Solution**: 
```bash
# Check if Redis is running
pgrep redis-server

# Start Redis
sudo systemctl start redis
```

#### 3. "WRONGPASS invalid username-password pair"
**Cause**: Incorrect password in connection URL  
**Solution**: Verify password and update `REDIS_URL`

#### 4. "Redis is still accessible without password"
**Cause**: Authentication not properly enabled  
**Solution**: 
1. Check Redis config: `grep requirepass /etc/redis/redis.conf`
2. Restart Redis: `sudo systemctl restart redis`
3. Verify with test: `redis-cli ping` (should fail)

### Debug Commands

```bash
# Check Redis configuration
redis-cli CONFIG GET requirepass
redis-cli CONFIG GET protected-mode

# Test connection with explicit password
redis-cli -a your_password ping

# Check Redis server info
redis-cli INFO server

# Monitor Redis commands (with auth)
redis-cli -a your_password MONITOR
```

## Best Practices

### Security

1. **Strong Passwords**: Use at least 25 characters with mixed case, numbers, and symbols
2. **Environment Variables**: Store passwords in environment variables, not code
3. **Network Security**: Combine authentication with firewall rules and network segmentation
4. **Regular Rotation**: Change Redis passwords periodically
5. **SSL/TLS**: Use `rediss://` for encrypted connections in production

### Performance

1. **Connection Pooling**: Configure appropriate `REDIS_POOL_SIZE` (10-50)
2. **Timeout Settings**: Set reasonable connection and command timeouts
3. **Monitoring**: Monitor Redis memory usage and connection counts
4. **Persistence**: Configure appropriate persistence settings for your use case

### Operations

1. **Backup Configuration**: Always backup Redis config before making changes
2. **Testing**: Test authentication in staging before production
3. **Monitoring**: Set up alerts for Redis authentication failures
4. **Documentation**: Keep password and configuration documentation secure

## Production Deployment Checklist

- [ ] Redis server is configured with `requirepass`
- [ ] `protected-mode` is enabled in Redis config
- [ ] `REDIS_URL` in production environment includes password
- [ ] Application successfully connects to authenticated Redis
- [ ] All integration tests pass with authentication
- [ ] Redis password is stored securely (password manager, vault)
- [ ] Firewall rules restrict Redis access to application servers only
- [ ] SSL/TLS is enabled for remote Redis connections
- [ ] Monitoring is configured for Redis authentication failures
- [ ] Backup and recovery procedures include Redis password rotation

## Advanced Configuration

### Redis Cluster with Authentication

For Redis clusters, each node must have the same password:

```conf
# On each node in the cluster
requirepass your_cluster_password
masterauth your_cluster_password
```

### Docker Deployment

```yaml
# docker-compose.yml
version: '3.8'
services:
  redis:
    image: redis:7-alpine
    command: redis-server --requirepass your_secure_password
    ports:
      - "6379:6379"
    environment:
      REDIS_PASSWORD: your_secure_password
```

### Kubernetes Deployment

```yaml
# redis-secret.yaml
apiVersion: v1
kind: Secret
metadata:
  name: redis-secret
type: Opaque
data:
  password: <base64-encoded-password>

---
# redis-deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: redis
spec:
  template:
    spec:
      containers:
      - name: redis
        image: redis:7-alpine
        command:
        - redis-server
        - --requirepass
        - $(REDIS_PASSWORD)
        env:
        - name: REDIS_PASSWORD
          valueFrom:
            secretKeyRef:
              name: redis-secret
              key: password
```

## Migration from Unauthenticated Redis

If you're migrating from an unauthenticated Redis setup:

1. **Schedule Maintenance Window**: Plan for application downtime
2. **Backup Data**: Ensure Redis data is backed up
3. **Update Configuration**: Add password to Redis config
4. **Update Environment**: Update `REDIS_URL` in all environments
5. **Restart Services**: Restart Redis and application
6. **Validate**: Test all Redis-dependent functionality
7. **Monitor**: Watch for authentication errors

## Support

For issues with Redis authentication:

1. Check the troubleshooting section above
2. Review Redis logs: `sudo journalctl -u redis`
3. Consult Redis documentation: https://redis.io/documentation
4. Check application logs for detailed error messages
5. Use the setup script with `--dry-run` to validate configuration

---

**Last Updated**: November 18, 2025  
**Version**: 1.0  
**Maintainer**: GridTokenX Development Team
