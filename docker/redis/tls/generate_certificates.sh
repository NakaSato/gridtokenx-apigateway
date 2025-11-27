#!/bin/bash

# Redis TLS Certificate Generation Script
# Generates self-signed certificates for Redis TLS encryption

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
CERT_DIR="$(dirname "$0")"
COUNTRY="US"
STATE="California"
CITY="San Francisco"
ORGANIZATION="GridTokenX"
ORGANIZATIONAL_UNIT="Engineering"
COMMON_NAME="redis.gridtokenx.local"
DAYS_VALID=365

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

# Check if OpenSSL is available
check_openssl() {
    if ! command -v openssl &> /dev/null; then
        print_error "OpenSSL is required but not installed."
        print_status "Please install OpenSSL:"
        print_status "  Ubuntu/Debian: sudo apt-get install openssl"
        print_status "  CentOS/RHEL: sudo yum install openssl"
        print_status "  macOS: brew install openssl"
        exit 1
    fi
}

# Generate CA certificate
generate_ca() {
    print_status "Generating Certificate Authority (CA)..."
    
    # Generate CA private key
    openssl genrsa -out "$CERT_DIR/ca-key.pem" 4096
    
    # Generate CA certificate
    openssl req -new -x509 -days $DAYS_VALID -key "$CERT_DIR/ca-key.pem" -sha256 -out "$CERT_DIR/ca-cert.pem" -subj "/C=$COUNTRY/ST=$STATE/L=$CITY/O=$ORGANIZATION/OU=$ORGANIZATIONAL_UNIT/CN=GridTokenX Redis CA"
    
    print_success "CA certificate generated"
}

# Generate server certificate
generate_server_cert() {
    print_status "Generating Redis server certificate..."
    
    # Generate server private key
    openssl genrsa -out "$CERT_DIR/redis-key.pem" 4096
    
    # Create server certificate signing request (CSR)
    openssl req -new -key "$CERT_DIR/redis-key.pem" -out "$CERT_DIR/redis.csr" -subj "/C=$COUNTRY/ST=$STATE/L=$CITY/O=$ORGANIZATION/OU=$ORGANIZATIONAL_UNIT/CN=$COMMON_NAME"
    
    # Create server certificate extension file
    cat > "$CERT_DIR/redis.ext" << EOF
authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth
subjectAltName = @alt_names

[alt_names]
DNS.1 = localhost
DNS.2 = redis
DNS.3 = redis.gridtokenx.local
DNS.4 = *.redis.gridtokenx.local
IP.1 = 127.0.0.1
IP.2 = ::1
EOF
    
    # Generate server certificate signed by CA
    openssl x509 -req -in "$CERT_DIR/redis.csr" -CA "$CERT_DIR/ca-cert.pem" -CAkey "$CERT_DIR/ca-key.pem" -CAcreateserial -out "$CERT_DIR/redis-cert.pem" -days $DAYS_VALID -sha256 -extfile "$CERT_DIR/redis.ext"
    
    # Remove CSR and extension files
    rm "$CERT_DIR/redis.csr" "$CERT_DIR/redis.ext"
    
    print_success "Server certificate generated"
}

# Generate client certificate
generate_client_cert() {
    print_status "Generating Redis client certificate..."
    
    # Generate client private key
    openssl genrsa -out "$CERT_DIR/client-key.pem" 4096
    
    # Create client certificate signing request (CSR)
    openssl req -new -key "$CERT_DIR/client-key.pem" -out "$CERT_DIR/client.csr" -subj "/C=$COUNTRY/ST=$STATE/L=$CITY/O=$ORGANIZATION/OU=$ORGANIZATIONAL_UNIT/CN=GridTokenX Redis Client"
    
    # Create client certificate extension file
    cat > "$CERT_DIR/client.ext" << EOF
authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = clientAuth
EOF
    
    # Generate client certificate signed by CA
    openssl x509 -req -in "$CERT_DIR/client.csr" -CA "$CERT_DIR/ca-cert.pem" -CAkey "$CERT_DIR/ca-key.pem" -CAcreateserial -out "$CERT_DIR/client-cert.pem" -days $DAYS_VALID -sha256 -extfile "$CERT_DIR/client.ext"
    
    # Remove CSR and extension files
    rm "$CERT_DIR/client.csr" "$CERT_DIR/client.ext"
    
    print_success "Client certificate generated"
}

# Set proper file permissions
set_permissions() {
    print_status "Setting proper file permissions..."
    
    # Set restrictive permissions for private keys
    chmod 600 "$CERT_DIR"/*-key.pem
    chmod 644 "$CERT_DIR"/*-cert.pem
    chmod 644 "$CERT_DIR"/ca-cert.pem
    
    print_success "File permissions set"
}

# Verify certificates
verify_certificates() {
    print_status "Verifying certificates..."
    
    # Verify server certificate
    if openssl verify -CAfile "$CERT_DIR/ca-cert.pem" "$CERT_DIR/redis-cert.pem" &> /dev/null; then
        print_success "Server certificate verification passed"
    else
        print_error "Server certificate verification failed"
        return 1
    fi
    
    # Verify client certificate
    if openssl verify -CAfile "$CERT_DIR/ca-cert.pem" "$CERT_DIR/client-cert.pem" &> /dev/null; then
        print_success "Client certificate verification passed"
    else
        print_error "Client certificate verification failed"
        return 1
    fi
}

# Display certificate information
display_info() {
    print_status "Certificate Information:"
    echo "=================================="
    
    # Server certificate details
    echo "Server Certificate:"
    openssl x509 -in "$CERT_DIR/redis-cert.pem" -text -noout | grep -E "(Subject:|Issuer:|Not Before:|Not After:)"
    echo ""
    
    # Client certificate details
    echo "Client Certificate:"
    openssl x509 -in "$CERT_DIR/client-cert.pem" -text -noout | grep -E "(Subject:|Issuer:|Not Before:|Not After:)"
    echo ""
    
    # CA certificate details
    echo "CA Certificate:"
    openssl x509 -in "$CERT_DIR/ca-cert.pem" -text -noout | grep -E "(Subject:|Issuer:|Not Before:|Not After:)"
    echo ""
    
    # File locations
    echo "Generated Files:"
    echo "- CA Certificate: $CERT_DIR/ca-cert.pem"
    echo "- CA Private Key: $CERT_DIR/ca-key.pem"
    echo "- Server Certificate: $CERT_DIR/redis-cert.pem"
    echo "- Server Private Key: $CERT_DIR/redis-key.pem"
    echo "- Client Certificate: $CERT_DIR/client-cert.pem"
    echo "- Client Private Key: $CERT_DIR/client-key.pem"
    echo "=================================="
}

# Generate DH parameters for perfect forward secrecy
generate_dh_params() {
    print_status "Generating Diffie-Hellman parameters (this may take a while)..."
    
    openssl dhparam -out "$CERT_DIR/dhparam.pem" 2048
    
    print_success "DH parameters generated"
}

# Clean up existing certificates
cleanup() {
    print_status "Cleaning up existing certificates..."
    rm -f "$CERT_DIR"/*.pem "$CERT_DIR"/*.csr "$CERT_DIR"/*.ext "$CERT_DIR"/.srl
}

# Main execution
main() {
    print_status "Starting Redis TLS certificate generation..."
    
    # Check prerequisites
    check_openssl
    
    # Clean up existing certificates
    if [ "$1" = "--force" ] || [ "$1" = "-f" ]; then
        cleanup
    fi
    
    # Check if certificates already exist
    if [ -f "$CERT_DIR/redis-cert.pem" ] && [ "$1" != "--force" ] && [ "$1" != "-f" ]; then
        print_warning "Certificates already exist. Use --force to regenerate."
        display_info
        exit 0
    fi
    
    # Generate certificates
    generate_ca
    generate_server_cert
    generate_client_cert
    generate_dh_params
    
    # Set permissions
    set_permissions
    
    # Verify certificates
    verify_certificates
    
    # Display information
    display_info
    
    print_success "TLS certificates generated successfully!"
    
    print_status "Next steps:"
    echo "1. Copy certificates to appropriate locations"
    echo "2. Update Redis configuration to use TLS"
    echo "3. Update application connection strings to use rediss://"
    echo "4. Restart Redis with TLS enabled"
    echo ""
    print_status "Example Redis TLS configuration:"
    echo "tls-port 6380"
    echo "port 0"
    echo "tls-cert-file /etc/redis/tls/redis-cert.pem"
    echo "tls-key-file /etc/redis/tls/redis-key.pem"
    echo "tls-ca-cert-file /etc/redis/tls/ca-cert.pem"
    echo "tls-dh-params-file /etc/redis/tls/dhparam.pem"
    echo "tls-auth-clients yes"
    echo "tls-replication yes"
}

# Run main function with all arguments
main "$@"
