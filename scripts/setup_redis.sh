#!/bin/bash

# GridTokenX Redis Setup Script
# This script sets up Redis with proper configuration for GridTokenX

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Print colored output
print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if Redis is installed
check_redis_installation() {
    print_status "Checking Redis installation..."
    
    if command -v redis-cli &> /dev/null; then
        REDIS_VERSION=$(redis-cli --version | cut -d' ' -f2)
        print_success "Redis $REDIS_VERSION is installed"
        return 0
    else
        print_warning "Redis CLI not found. Installing Redis..."
        return 1
    fi
}

# Install Redis on different systems
install_redis() {
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        # Linux
        if command -v apt-get &> /dev/null; then
            # Ubuntu/Debian
            print_status "Installing Redis on Ubuntu/Debian..."
            sudo apt-get update
            sudo apt-get install -y redis-server redis-tools
        elif command -v yum &> /dev/null; then
            # CentOS/RHEL
            print_status "Installing Redis on CentOS/RHEL..."
            sudo yum install -y epel-release
            sudo yum install -y redis
        elif command -v dnf &> /dev/null; then
            # Fedora
            print_status "Installing Redis on Fedora..."
            sudo dnf install -y redis
        fi
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS
        if command -v brew &> /dev/null; then
            print_status "Installing Redis on macOS using Homebrew..."
            brew install redis
        else
            print_error "Homebrew not found. Please install Homebrew first."
            exit 1
        fi
    else
        print_error "Unsupported operating system: $OSTYPE"
        print_status "Please install Redis manually from https://redis.io/download"
        exit 1
    fi
}

# Generate secure Redis password
generate_password() {
    if command -v openssl &> /dev/null; then
        openssl rand -base64 32
    else
        # Fallback method
        LC_ALL=C tr -dc 'A-Za-z0-9!@#$%^&*()_+-=' < /dev/urandom | head -c 32
    fi
}

# Setup Redis configuration
setup_redis_config() {
    print_status "Setting up Redis configuration..."
    
    # Create config directory if it doesn't exist
    mkdir -p docker/redis
    
    # Generate secure password if not provided
    if [ -z "$REDIS_PASSWORD" ]; then
        print_status "Generating secure Redis password..."
        REDIS_PASSWORD=$(generate_password)
        print_success "Generated password: $REDIS_PASSWORD"
        print_warning "Please save this password securely!"
    fi
    
    # Update .env file with Redis password
    if [ -f ".env" ]; then
        if grep -q "REDIS_PASSWORD=" .env; then
            sed -i.bak "s/REDIS_PASSWORD=.*/REDIS_PASSWORD=$REDIS_PASSWORD/" .env
        else
            echo "REDIS_PASSWORD=$REDIS_PASSWORD" >> .env
        fi
        print_success "Updated .env file with Redis password"
    else
        print_warning ".env file not found. Creating one..."
        echo "REDIS_PASSWORD=$REDIS_PASSWORD" > .env
        print_success "Created .env file with Redis password"
    fi
    
    # Copy Redis configuration
    if [ ! -f "docker/redis/redis.conf" ]; then
        print_error "Redis configuration file not found at docker/redis/redis.conf"
        return 1
    fi
    
    print_success "Redis configuration is ready"
}

# Test Redis connection
test_redis_connection() {
    print_status "Testing Redis connection..."
    
    # Start Redis if not running (for local development)
    if ! pgrep -x "redis-server" > /dev/null; then
        print_status "Starting Redis server..."
        redis-server docker/redis/redis.conf --daemonize yes
        sleep 2
    fi
    
    # Test connection
    if redis-cli -a "$REDIS_PASSWORD" ping > /dev/null 2>&1; then
        print_success "Redis connection test passed"
        return 0
    else
        print_error "Redis connection test failed"
        return 1
    fi
}

# Setup Redis monitoring
setup_monitoring() {
    print_status "Setting up Redis monitoring..."
    
    # Create Prometheus configuration for Redis
    mkdir -p docker/prometheus
    
    cat >> docker/prometheus/prometheus.yml << EOF

# Redis Exporter Configuration
  - job_name: 'redis'
    static_configs:
      - targets: ['redis:6379']
    scrape_interval: 15s
    metrics_path: /metrics
EOF
    
    print_success "Redis monitoring configuration added"
}

# Create Redis health check script
create_health_check() {
    print_status "Creating Redis health check script..."
    
    cat > scripts/redis_health_check.sh << 'EOF'
#!/bin/bash

# Redis Health Check Script
# Used by Docker health checks and monitoring

REDIS_HOST=${REDIS_HOST:-localhost}
REDIS_PORT=${REDIS_PORT:-6379}
REDIS_PASSWORD=${REDIS_PASSWORD:-}

# Health check function
check_redis_health() {
    local response
    if [ -n "$REDIS_PASSWORD" ]; then
        response=$(redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" -a "$REDIS_PASSWORD" ping 2>/dev/null)
    else
        response=$(redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" ping 2>/dev/null)
    fi
    
    if [ "$response" = "PONG" ]; then
        echo "Redis is healthy"
        return 0
    else
        echo "Redis is unhealthy"
        return 1
    fi
}

# Run health check
if check_redis_health; then
    exit 0
else
    exit 1
fi
EOF
    
    chmod +x scripts/redis_health_check.sh
    print_success "Redis health check script created"
}

# Create Redis backup script
create_backup_script() {
    print_status "Creating Redis backup script..."
    
    cat > scripts/redis_backup.sh << 'EOF'
#!/bin/bash

# Redis Backup Script
# Creates backups of Redis data

BACKUP_DIR=${BACKUP_DIR:-./backups/redis}
REDIS_HOST=${REDIS_HOST:-localhost}
REDIS_PORT=${REDIS_PORT:-6379}
REDIS_PASSWORD=${REDIS_PASSWORD:-}
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Create backup directory
mkdir -p "$BACKUP_DIR"

# Create backup
echo "Creating Redis backup..."
if [ -n "$REDIS_PASSWORD" ]; then
    redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" -a "$REDIS_PASSWORD" --rdb "$BACKUP_DIR/redis_backup_$TIMESTAMP.rdb"
else
    redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" --rdb "$BACKUP_DIR/redis_backup_$TIMESTAMP.rdb"
fi

# Compress backup
gzip "$BACKUP_DIR/redis_backup_$TIMESTAMP.rdb"

# Clean old backups (keep last 7 days)
find "$BACKUP_DIR" -name "redis_backup_*.rdb.gz" -mtime +7 -delete

echo "Redis backup completed: redis_backup_$TIMESTAMP.rdb.gz"
EOF
    
    chmod +x scripts/redis_backup.sh
    print_success "Redis backup script created"
}

# Display Redis information
show_redis_info() {
    print_status "Redis Setup Information:"
    echo "=================================="
    echo "Host: localhost"
    echo "Port: 6379"
    echo "Password: $REDIS_PASSWORD"
    echo "Config: docker/redis/redis.conf"
    echo "Data Directory: ./data/redis"
    echo "=================================="
    
    print_status "Useful Commands:"
    echo "- Connect: redis-cli -a $REDIS_PASSWORD"
    echo "- Test: redis-cli -a $REDIS_PASSWORD ping"
    echo "- Monitor: redis-cli -a $REDIS_PASSWORD monitor"
    echo "- Info: redis-cli -a $REDIS_PASSWORD info"
    
    print_status "Docker Commands:"
    echo "- Start: docker-compose up -d redis"
    echo "- Stop: docker-compose stop redis"
    echo "- Logs: docker-compose logs -f redis"
    echo "- Exec: docker-compose exec redis redis-cli"
}

# Main execution
main() {
    print_status "Starting GridTokenX Redis Setup..."
    
    # Check if running in Docker context
    if [ -f "docker-compose.yml" ]; then
        print_success "Docker Compose configuration found"
    else
        print_error "docker-compose.yml not found. Please run from project root."
        exit 1
    fi
    
    # Check/Install Redis CLI
    if ! check_redis_installation; then
        install_redis
    fi
    
    # Setup configuration
    setup_redis_config
    
    # Create utility scripts
    create_health_check
    create_backup_script
    
    # Setup monitoring
    setup_monitoring
    
    # Test connection (if Redis is running)
    if pgrep -x "redis-server" > /dev/null || docker-compose ps redis | grep -q "Up"; then
        test_redis_connection
    else
        print_warning "Redis is not running. Start with: docker-compose up -d redis"
    fi
    
    # Show information
    show_redis_info
    
    print_success "Redis setup completed!"
    print_status "Next steps:"
    echo "1. Start Redis: docker-compose up -d redis"
    echo "2. Check status: docker-compose ps redis"
    echo "3. View logs: docker-compose logs -f redis"
    echo "4. Access Grafana: http://localhost:3001 (Redis dashboard)"
}

# Run main function
main "$@"
