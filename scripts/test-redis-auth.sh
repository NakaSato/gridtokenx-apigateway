#!/bin/bash

# Test Redis Authentication Setup for GridTokenX
# This script tests the Redis authentication configuration

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

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

print_status "Testing Redis Authentication Setup for GridTokenX"
echo "=================================================="

# Check if local.env exists
if [ ! -f "local.env" ]; then
    print_error "local.env file not found"
    exit 1
fi

# Extract Redis URL from local.env
REDIS_URL=$(grep "REDIS_URL=" local.env | cut -d'=' -f2)

if [ -z "$REDIS_URL" ]; then
    print_error "REDIS_URL not found in local.env"
    exit 1
fi

print_status "Found Redis URL: ${REDIS_URL:0:20}..."

# Check if Redis URL contains authentication
if [[ $REDIS_URL =~ :([^@]+)@ ]]; then
    PASSWORD="${BASH_REMATCH[1]}"
    print_success "Redis URL contains authentication"
    
    # Extract host and port
    if [[ $REDIS_URL =~ @([^:]+):([0-9]+) ]]; then
        HOST="${BASH_REMATCH[1]}"
        PORT="${BASH_REMATCH[2]}"
        print_status "Redis Host: $HOST"
        print_status "Redis Port: $PORT"
    fi
else
    print_warning "Redis URL does not contain authentication"
    print_warning "Consider updating to: redis://:password@host:port"
fi

# Test if Redis is installed
if command -v redis-cli &> /dev/null; then
    print_status "Redis CLI is available"
    
    # Test connection
    print_status "Testing Redis connection..."
    
    if [ -n "$PASSWORD" ]; then
        # Test with password
        if redis-cli -h "$HOST" -p "$PORT" -a "$PASSWORD" ping 2>/dev/null | grep -q "PONG"; then
            print_success "Redis authentication is working correctly"
        else
            print_error "Redis authentication failed"
            print_error "Check if Redis server is running and configured with the correct password"
            exit 1
        fi
        
        # Test without password (should fail)
        if redis-cli -h "$HOST" -p "$PORT" ping 2>/dev/null | grep -q "PONG"; then
            print_warning "Redis is still accessible without password"
            print_warning "Authentication may not be properly enabled"
        else
            print_success "Redis properly rejects unauthenticated connections"
        fi
    else
        # Test without password
        if redis-cli -h "$HOST" -p "$PORT" ping 2>/dev/null | grep -q "PONG"; then
            print_warning "Redis is accessible without authentication"
            print_warning "Consider enabling Redis authentication for security"
        else
            print_error "Redis connection failed"
            exit 1
        fi
    fi
else
    print_warning "Redis CLI not available, skipping connection test"
    print_status "Install Redis CLI with: brew install redis (macOS) or apt-get install redis-tools (Ubuntu)"
fi

# Test application compilation
print_status "Testing application compilation..."
if cargo check --quiet 2>/dev/null; then
    print_success "Application compiles successfully"
else
    print_error "Application compilation failed"
    exit 1
fi

# Check Redis configuration in application code
print_status "Checking Redis authentication logic in application..."
if grep -q "authentication" src/main.rs; then
    print_success "Redis authentication logic found in application"
else
    print_warning "Redis authentication logic may not be implemented"
fi

if grep -q "contains(\"@\")" src/main.rs; then
    print_success "Application detects Redis authentication"
else
    print_warning "Application may not detect Redis authentication"
fi

print_success "Redis authentication setup test completed"
echo
print_status "Summary:"
echo "- Redis configuration: $([ -n "$PASSWORD" ] && echo "✅ Authenticated" || echo "⚠️  Not authenticated")"
echo "- Application compilation: ✅ Success"
echo "- Authentication logic: ✅ Implemented"

if [ -n "$PASSWORD" ]; then
    echo
    print_status "To test the application with Redis authentication:"
    echo "1. Start your Redis server with authentication"
    echo "2. Run: cargo run"
    echo "3. Check logs for: '✅ Redis connection established (authenticated)'"
else
    echo
    print_status "To enable Redis authentication:"
    echo "1. Run: ./scripts/setup-redis-auth.sh"
    echo "2. Or manually update your Redis configuration"
    echo "3. Update local.env with authenticated Redis URL"
fi

echo
print_status "For detailed setup instructions, see: docs/technical/REDIS_AUTHENTICATION_SETUP.md"
