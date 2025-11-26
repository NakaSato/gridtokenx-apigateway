#!/bin/bash

# Redis Backup Automation Script for GridTokenX
# Provides automated backup with rotation, compression, and monitoring

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default configuration
REDIS_HOST=${REDIS_HOST:-localhost}
REDIS_PORT=${REDIS_PORT:-6379}
REDIS_PASSWORD=${REDIS_PASSWORD:-}
BACKUP_DIR=${BACKUP_DIR:-./backups/redis}
RETENTION_DAYS=${RETENTION_DAYS:-7}
BACKUP_TYPE=${BACKUP_TYPE:-both}  # rdb, aof, both
COMPRESSION=${COMPRESSION:-gzip}
S3_BUCKET=${S3_BUCKET:-}
S3_REGION=${S3_REGION:-us-east-1}
SLACK_WEBHOOK=${SLACK_WEBHOOK:-}
TELEGRAM_BOT_TOKEN=${TELEGRAM_BOT_TOKEN:-}
TELEGRAM_CHAT_ID=${TELEGRAM_CHAT_ID:-}

# Print colored output
print_status() {
    echo -e "${BLUE}[INFO]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $(date '+%Y-%m-%d %H:%M:%S') - $1"
}

# Logging function
log_message() {
    local level=$1
    local message=$2
    local log_file="$BACKUP_DIR/backup.log"
    
    echo "$(date '+%Y-%m-%d %H:%M:%S') [$level] $message" >> "$log_file"
    
    case $level in
        "INFO") print_status "$message" ;;
        "SUCCESS") print_success "$message" ;;
        "WARNING") print_warning "$message" ;;
        "ERROR") print_error "$message" ;;
    esac
}

# Send notification
send_notification() {
    local status=$1
    local message=$2
    
    # Slack notification
    if [ -n "$SLACK_WEBHOOK" ]; then
        local color="good"
        if [ "$status" = "ERROR" ]; then
            color="danger"
        elif [ "$status" = "WARNING" ]; then
            color="warning"
        fi
        
        curl -X POST -H 'Content-type: application/json' \
            --data "{\"text\":\"Redis Backup $status\",\"attachments\":[{\"color\":\"$color\",\"text\":\"$message\"}]}" \
            "$SLACK_WEBHOOK" 2>/dev/null || true
    fi
    
    # Telegram notification
    if [ -n "$TELEGRAM_BOT_TOKEN" ] && [ -n "$TELEGRAM_CHAT_ID" ]; then
        local emoji="✅"
        if [ "$status" = "ERROR" ]; then
            emoji="❌"
        elif [ "$status" = "WARNING" ]; then
            emoji="⚠️"
        fi
        
        curl -s "https://api.telegram.org/bot$TELEGRAM_BOT_TOKEN/sendMessage" \
            -d parse_mode="Markdown" \
            -d chat_id="$TELEGRAM_CHAT_ID" \
            -d text="$emoji Redis Backup *$status*%0A%0A$message" \
            2>/dev/null || true
    fi
}

# Check Redis connection
check_redis_connection() {
    log_message "INFO" "Checking Redis connection..."
    
    local response
    if [ -n "$REDIS_PASSWORD" ]; then
        response=$(redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" -a "$REDIS_PASSWORD" ping 2>/dev/null)
    else
        response=$(redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" ping 2>/dev/null)
    fi
    
    if [ "$response" = "PONG" ]; then
        log_message "SUCCESS" "Redis connection successful"
        return 0
    else
        log_message "ERROR" "Redis connection failed: $response"
        return 1
    fi
}

# Get Redis info
get_redis_info() {
    local key=$1
    
    if [ -n "$REDIS_PASSWORD" ]; then
        redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" -a "$REDIS_PASSWORD" info "$key" 2>/dev/null
    else
        redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" info "$key" 2>/dev/null
    fi
}

# Create RDB backup
create_rdb_backup() {
    local timestamp=$(date '+%Y%m%d_%H%M%S')
    local backup_file="$BACKUP_DIR/rdb/redis_rdb_$timestamp.rdb"
    
    log_message "INFO" "Creating RDB backup..."
    
    mkdir -p "$BACKUP_DIR/rdb"
    
    # Create RDB snapshot
    if [ -n "$REDIS_PASSWORD" ]; then
        redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" -a "$REDIS_PASSWORD" --rdb "$backup_file" 2>/dev/null
    else
        redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" --rdb "$backup_file" 2>/dev/null
    fi
    
    if [ -f "$backup_file" ]; then
        local size=$(du -h "$backup_file" | cut -f1)
        log_message "SUCCESS" "RDB backup created: $backup_file ($size)"
        
        # Compress if enabled
        if [ "$COMPRESSION" = "gzip" ]; then
            gzip "$backup_file"
            backup_file="$backup_file.gz"
            size=$(du -h "$backup_file" | cut -f1)
            log_message "SUCCESS" "RDB backup compressed: $backup_file ($size)"
        fi
        
        echo "$backup_file"
        return 0
    else
        log_message "ERROR" "Failed to create RDB backup"
        return 1
    fi
}

# Create AOF backup
create_aof_backup() {
    local timestamp=$(date '+%Y%m%d_%H%M%S')
    local backup_file="$BACKUP_DIR/aof/redis_aof_$timestamp.aof"
    
    log_message "INFO" "Creating AOF backup..."
    
    mkdir -p "$BACKUP_DIR/aof"
    
    # Get AOF file path from Redis info
    local aof_path=$(get_redis_info "persistence" | grep "aof_current_rewrite_time_sec:" | cut -d: -f2 | tr -d '\r')
    if [ -z "$aof_path" ]; then
        aof_path=$(get_redis_info "persistence" | grep "aof_current_size:" | cut -d: -f2 | tr -d '\r')
    fi
    
    # If AOF is not enabled, create one by enabling it temporarily
    if ! get_redis_info "persistence" | grep -q "aof_enabled:1"; then
        log_message "WARNING" "AOF not enabled, enabling temporarily for backup..."
        
        if [ -n "$REDIS_PASSWORD" ]; then
            redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" -a "$REDIS_PASSWORD" config set appendonly yes 2>/dev/null
        else
            redis-cli -h "$REDIS_HOST" -p "$REDIS_PORT" config set appendonly yes 2>/dev/null
        fi
        
        # Wait for AOF to be created
        sleep 5
    fi
    
    # Copy AOF file from Redis data directory
    local redis_data_dir=$(get_redis_info "server" | grep "redis_config_dir:" | cut -d: -f2 | tr -d '\r')
    if [ -z "$redis_data_dir" ]; then
        redis_data_dir="/data"
    fi
    
    local aof_source="$redis_data_dir/appendonly.aof"
    
    if [ -f "$aof_source" ]; then
        cp "$aof_source" "$backup_file"
        
        if [ -f "$backup_file" ]; then
            local size=$(du -h "$backup_file" | cut -f1)
            log_message "SUCCESS" "AOF backup created: $backup_file ($size)"
            
            # Compress if enabled
            if [ "$COMPRESSION" = "gzip" ]; then
                gzip "$backup_file"
                backup_file="$backup_file.gz"
                size=$(du -h "$backup_file" | cut -f1)
                log_message "SUCCESS" "AOF backup compressed: $backup_file ($size)"
            fi
            
            echo "$backup_file"
            return 0
        else
            log_message "ERROR" "Failed to copy AOF file"
            return 1
        fi
    else
        log_message "ERROR" "AOF file not found at $aof_source"
        return 1
    fi
}

# Upload to S3
upload_to_s3() {
    local file=$1
    
    if [ -n "$S3_BUCKET" ]; then
        log_message "INFO" "Uploading $file to S3..."
        
        local s3_key="redis-backups/$(basename "$file")"
        
        if aws s3 cp "$file" "s3://$S3_BUCKET/$s3_key" --region "$S3_REGION" 2>/dev/null; then
            log_message "SUCCESS" "Uploaded to S3: s3://$S3_BUCKET/$s3_key"
            return 0
        else
            log_message "ERROR" "Failed to upload to S3"
            return 1
        fi
    fi
    
    return 0  # S3 upload is optional
}

# Clean old backups
cleanup_old_backups() {
    log_message "INFO" "Cleaning up backups older than $RETENTION_DAYS days..."
    
    # Clean RDB backups
    find "$BACKUP_DIR/rdb" -name "redis_rdb_*.rdb*" -mtime +$RETENTION_DAYS -delete -print 2>/dev/null | while read file; do
        log_message "INFO" "Deleted old RDB backup: $file"
    done
    
    # Clean AOF backups
    find "$BACKUP_DIR/aof" -name "redis_aof_*.aof*" -mtime +$RETENTION_DAYS -delete -print 2>/dev/null | while read file; do
        log_message "INFO" "Deleted old AOF backup: $file"
    done
    
    # Clean S3 backups
    if [ -n "$S3_BUCKET" ]; then
        log_message "INFO" "Cleaning up S3 backups older than $RETENTION_DAYS days..."
        
        local cutoff_date=$(date -d "$RETENTION_DAYS days ago" '+%Y%m%d')
        aws s3 ls "s3://$S3_BUCKET/redis-backups/" --region "$S3_REGION" 2>/dev/null | \
        awk '$1 <= "'$cutoff_date'" {print $4}' | \
        while read key; do
            if [ -n "$key" ]; then
                aws s3 rm "s3://$S3_BUCKET/redis-backups/$key" --region "$S3_REGION" 2>/dev/null || true
                log_message "INFO" "Deleted old S3 backup: $key"
            fi
        done
    fi
    
    log_message "SUCCESS" "Backup cleanup completed"
}

# Generate backup report
generate_report() {
    local rdb_file=$1
    local aof_file=$2
    local start_time=$3
    local end_time=$(date +%s)
    local duration=$((end_time - start_time))
    
    local report_file="$BACKUP_DIR/reports/backup_report_$(date '+%Y%m%d_%H%M%S').json"
    mkdir -p "$BACKUP_DIR/reports"
    
    # Get Redis stats
    local memory_used=$(get_redis_info "memory" | grep "used_memory_human:" | cut -d: -f2 | tr -d '\r')
    local connected_clients=$(get_redis_info "clients" | grep "connected_clients:" | cut -d: -f2 | tr -d '\r')
    local total_commands=$(get_redis_info "stats" | grep "total_commands_processed:" | cut -d: -f2 | tr -d '\r')
    
    # Create JSON report
    cat > "$report_file" << EOF
{
    "backup_timestamp": "$(date -Iseconds)",
    "backup_duration_seconds": $duration,
    "redis_info": {
        "host": "$REDIS_HOST:$REDIS_PORT",
        "memory_used": "$memory_used",
        "connected_clients": $connected_clients,
        "total_commands_processed": $total_commands
    },
    "backups": {
EOF
    
    if [ -n "$rdb_file" ]; then
        local rdb_size=$(du -b "$rdb_file" 2>/dev/null | cut -f1 || echo "0")
        cat >> "$report_file" << EOF
        "rdb": {
            "file": "$(basename "$rdb_file")",
            "size_bytes": $rdb_size,
            "compressed": $([ "$COMPRESSION" = "gzip" ] && echo "true" || echo "false")
        }EOF
    fi
    
    if [ -n "$aof_file" ]; then
        local aof_size=$(du -b "$aof_file" 2>/dev/null | cut -f1 || echo "0")
        if [ -n "$rdb_file" ]; then
            echo "," >> "$report_file"
        fi
        cat >> "$report_file" << EOF
        "aof": {
            "file": "$(basename "$aof_file")",
            "size_bytes": $aof_size,
            "compressed": $([ "$COMPRESSION" = "gzip" ] && echo "true" || echo "false")
        }EOF
    fi
    
    cat >> "$report_file" << EOF
    }
}
EOF
    
    log_message "SUCCESS" "Backup report generated: $report_file"
}

# Main backup function
perform_backup() {
    local start_time=$(date +%s)
    local rdb_file=""
    local aof_file=""
    local backup_status="SUCCESS"
    local error_message=""
    
    log_message "INFO" "Starting Redis backup process..."
    
    # Check Redis connection
    if ! check_redis_connection; then
        backup_status="ERROR"
        error_message="Redis connection failed"
        send_notification "$backup_status" "$error_message"
        return 1
    fi
    
    # Create RDB backup
    if [ "$BACKUP_TYPE" = "rdb" ] || [ "$BACKUP_TYPE" = "both" ]; then
        if ! rdb_file=$(create_rdb_backup); then
            backup_status="ERROR"
            error_message="RDB backup failed"
        fi
    fi
    
    # Create AOF backup
    if [ "$BACKUP_TYPE" = "aof" ] || [ "$BACKUP_TYPE" = "both" ]; then
        if ! aof_file=$(create_aof_backup); then
            backup_status="ERROR"
            error_message="AOF backup failed"
        fi
    fi
    
    # Upload to S3
    if [ "$backup_status" = "SUCCESS" ]; then
        if [ -n "$rdb_file" ]; then
            upload_to_s3 "$rdb_file" || backup_status="WARNING"
        fi
        if [ -n "$aof_file" ]; then
            upload_to_s3 "$aof_file" || backup_status="WARNING"
        fi
    fi
    
    # Generate report
    generate_report "$rdb_file" "$aof_file" "$start_time"
    
    # Clean old backups
    cleanup_old_backups
    
    # Send notification
    if [ "$backup_status" = "SUCCESS" ]; then
        local message="Redis backup completed successfully in $(($(date +%s) - start_time)) seconds"
        send_notification "$backup_status" "$message"
        log_message "SUCCESS" "Backup process completed successfully"
    else
        send_notification "$backup_status" "$error_message"
        log_message "ERROR" "Backup process failed: $error_message"
    fi
    
    return $([ "$backup_status" = "SUCCESS" ] && echo 0 || echo 1)
}

# Show usage
show_usage() {
    cat << EOF
Redis Backup Automation Script for GridTokenX

Usage: $0 [OPTIONS]

Options:
    -h, --help              Show this help message
    -t, --type TYPE         Backup type: rdb, aof, both (default: both)
    -d, --days DAYS         Retention days (default: 7)
    -c, --compression TYPE   Compression: gzip, none (default: gzip)
    -b, --bucket BUCKET     S3 bucket for remote backup
    -r, --region REGION      S3 region (default: us-east-1)
    --host HOST             Redis host (default: localhost)
    --port PORT             Redis port (default: 6379)
    --password PASSWORD      Redis password
    --slack-webhook URL     Slack webhook for notifications
    --telegram-bot TOKEN    Telegram bot token
    --telegram-chat ID      Telegram chat ID

Environment Variables:
    REDIS_HOST              Redis host
    REDIS_PORT              Redis port
    REDIS_PASSWORD          Redis password
    BACKUP_DIR              Backup directory
    RETENTION_DAYS          Retention days
    BACKUP_TYPE             Backup type
    COMPRESSION             Compression type
    S3_BUCKET               S3 bucket
    S3_REGION               S3 region
    SLACK_WEBHOOK           Slack webhook URL
    TELEGRAM_BOT_TOKEN      Telegram bot token
    TELEGRAM_CHAT_ID        Telegram chat ID

Examples:
    $0                                              # Use defaults
    $0 -t rdb -d 30                               # RDB backup, 30 days retention
    $0 -t both -b my-bucket -r us-west-2          # Both backup types, S3 upload
    $0 --host redis.example.com --password secret     # Remote Redis with auth

EOF
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            show_usage
            exit 0
            ;;
        -t|--type)
            BACKUP_TYPE="$2"
            shift 2
            ;;
        -d|--days)
            RETENTION_DAYS="$2"
            shift 2
            ;;
        -c|--compression)
            COMPRESSION="$2"
            shift 2
            ;;
        -b|--bucket)
            S3_BUCKET="$2"
            shift 2
            ;;
        -r|--region)
            S3_REGION="$2"
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
        --password)
            REDIS_PASSWORD="$2"
            shift 2
            ;;
        --slack-webhook)
            SLACK_WEBHOOK="$2"
            shift 2
            ;;
        --telegram-bot)
            TELEGRAM_BOT_TOKEN="$2"
            shift 2
            ;;
        --telegram-chat)
            TELEGRAM_CHAT_ID="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            show_usage
            exit 1
            ;;
    esac
done

# Create backup directory
mkdir -p "$BACKUP_DIR"

# Main execution
main() {
    log_message "INFO" "Starting Redis backup automation"
    log_message "INFO" "Configuration: Type=$BACKUP_TYPE, Retention=$RETENTION_DAYS days, Compression=$COMPRESSION"
    
    if perform_backup; then
        log_message "SUCCESS" "Redis backup automation completed successfully"
        exit 0
    else
        log_message "ERROR" "Redis backup automation failed"
        exit 1
    fi
}

# Run main function
main
