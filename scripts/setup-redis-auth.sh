#!/bin/bash

# GridTokenX - Redis Authentication Setup Script
# This script helps configure Redis with password authentication

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
REDIS_PASSWORD=""
REDIS_HOST="localhost"
REDIS_PORT="6379"
REDIS_CONF_PATH="/etc/redis/redis.conf"

# Function to print colored output
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

# Function to check if Redis is installed
check_redis_installation() {
    if ! command -v redis-server &> /dev/null; then
        print_error "Redis is not installed"
        echo "Please install Redis first:"
        echo "  Ubuntu/Debian: sudo apt-get install redis-server"
        echo "  macOS: brew install redis"
        echo "  CentOS/RHEL: sudo yum install redis"
        exit 1
    fi
    
    print_success "Redis is installed"
}

# Function to check if Redis is running
check_redis_running() {
    if ! pgrep redis-server > /dev/null; then
        print_warning "Redis is not running"
        echo "Starting Redis..."
        if command -v systemctl &> /dev/null; then
            sudo systemctl start redis
        else
            redis-server --daemonize yes
        fi
        sleep 2
    fi
    
    if pgrep redis-server > /dev/null; then
        print_success "Redis is running"
    else
        print_error "Failed to start Redis"
        exit 1
    fi
}

# Function to generate a secure password
generate_password() {
    if [ -z "$REDIS_PASSWORD" ]; then
        print_status "Generating secure Redis password..."
        REDIS_PASSWORD=$(openssl rand -base64 32 | tr -d "=+/" | cut -c1-25)
        print_success "Generated password: $REDIS_PASSWORD"
    fi
}

# Function to backup existing Redis configuration
backup_config() {
    if [ -f "$REDIS_CONF_PATH" ]; then
        print_status "Backing up existing Redis configuration..."
        sudo cp "$REDIS_CONF_PATH" "$REDIS_CONF_PATH.backup.$(date +%Y%m%d_%H%M%S)"
        print_success "Configuration backed up"
    fi
}

# Function to configure Redis with authentication
configure_redis_auth() {
    print_status "Configuring Redis with password authentication..."
    
    # Check if Redis config exists
    if [ ! -f "$REDIS_CONF_PATH" ]; then
        print_error "Redis configuration file not found at $REDIS_CONF_PATH"
        print_status "You may need to specify the correct path with --config-path"
        exit 1
    fi
    
    # Add or update requirepass in Redis config
    if grep -q "^requirepass" "$REDIS_CONF_PATH"; then
        print_status "Updating existing password in Redis configuration..."
        sudo sed -i "s/^requirepass.*/requirepass $REDIS_PASSWORD/" "$REDIS_CONF_PATH"
    else
        print_status "Adding password to Redis configuration..."
        echo "requirepass $REDIS_PASSWORD" | sudo tee -a "$REDIS_CONF_PATH" > /dev/null
    fi
    
    # Enable protected mode (if not already enabled)
    if ! grep -q "^protected-mode yes" "$REDIS_CONF_PATH"; then
        print_status "Enabling Redis protected mode..."
        if grep -q "^protected-mode" "$REDIS_CONF_PATH"; then
            sudo sed -i "s/^protected-mode.*/protected-mode yes/" "$REDIS_CONF_PATH"
        else
            echo "protected-mode yes" | sudo tee -a "$REDIS_CONF_PATH" > /dev/null
        fi
    fi
    
    # Restart Redis to apply changes
    print_status "Restarting Redis to apply authentication..."
    if command -v systemctl &> /dev/null; then
        sudo systemctl restart redis
    else
        sudo pkill redis-server || true
        sleep 2
        redis-server "$REDIS_CONF_PATH" --daemonize yes
    fi
    
    sleep 3
    print_success "Redis restarted with authentication enabled"
}

# Function to test Redis authentication
test_redis_auth() {
    print_status "Testing Redis authentication..."
    
    # Test connection without password (should fail)
    if redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" ping 2>/dev/null | grep -q "PONG"; then
        print_warning "Redis is still accessible without password"
        print_error "Authentication may not be properly configured"
        return 1
    fi
    
    # Test connection with password (should succeed)
    if redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" -a "$REDIS_PASSWORD" ping 2>/dev/null | grep -q "PONG"; then
        print_success "Redis authentication is working correctly"
        return 0
    else
        print_error "Redis authentication test failed"
        return 1
    fi
}

# Function to update environment files
update_env_files() {
    print_status "Updating environment configuration files..."
    
    REDIS_URL="redis://:$REDIS_PASSWORD@$REDIS_HOST:$REDIS_PORT"
    
    # Update local.env if it exists
    if [ -f "local.env" ]; then
        print_status "Updating local.env..."
        if grep -q "REDIS_URL=" local.env; then
            sed -i.bak "s|^REDIS_URL=.*|REDIS_URL=$REDIS_URL|" local.env
        else
            echo "REDIS_URL=$REDIS_URL" >> local.env
        fi
        print_success "Updated local.env"
    fi
    
    # Show the new Redis URL
    echo
    print_status "Your new Redis URL:"
    echo -e "${GREEN}$REDIS_URL${NC}"
    echo
}

# Function to show usage
show_usage() {
    echo "GridTokenX Redis Authentication Setup Script"
    echo
    echo "Usage: $0 [OPTIONS]"
    echo
    echo "Options:"
    echo "  --password PASSWORD    Specify Redis password (auto-generated if not provided)"
    echo "  --host HOST           Redis host (default: localhost)"
    echo "  --port PORT           Redis port (default: 6379)"
    echo "  --config-path PATH     Redis configuration file path (default: /etc/redis/redis.conf)"
    echo "  --dry-run             Show what would be done without making changes"
    echo "  --help                Show this help message"
    echo
    echo "Examples:"
    echo "  $0                                    # Auto-generate password and configure"
    echo "  $0 --password mysecretpass           # Use specific password"
    echo "  $0 --host redis.example.com --port 6380  # Connect to remote Redis"
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --password)
            REDIS_PASSWORD="$2"
            shift 2
            ;;
        --host)
            REDIS_HOST="$2"
            shift 2
            ;;
        --port)
            REDIS_PORT="$2"
            shift 2
            ;;
        --config-path)
            REDIS_CONF_PATH="$2"
            shift 2
            ;;
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --help)
            show_usage
            exit 0
            ;;
        *)
            print_error "Unknown option: $1"
            show_usage
            exit 1
            ;;
    esac
done

# Main execution
main() {
    echo "GridTokenX - Redis Authentication Setup"
    echo "======================================="
    echo
    
    check_redis_installation
    
    if [ "$DRY_RUN" = true ]; then
        print_warning "DRY RUN MODE - No changes will be made"
        echo
        generate_password
        print_status "Would configure Redis with password: $REDIS_PASSWORD"
        print_status "Would update Redis configuration at: $REDIS_CONF_PATH"
        print_status "Would create Redis URL: redis://:$REDIS_PASSWORD@$REDIS_HOST:$REDIS_PORT"
        exit 0
    fi
    
    check_redis_running
    generate_password
    backup_config
    configure_redis_auth
    
    if test_redis_auth; then
        update_env_files
        print_success "Redis authentication setup completed successfully!"
        echo
        print_status "Next steps:"
        echo "1. Update your application to use the new Redis URL"
        echo "2. Restart your application"
        echo "3. Test your application functionality"
        echo
        print_status "Important: Save your Redis password in a secure location:"
        echo -e "${YELLOW}$REDIS_PASSWORD${NC}"
    else
        print_error "Redis authentication setup failed"
        exit 1
    fi
}

# Run main function
main "$@"
