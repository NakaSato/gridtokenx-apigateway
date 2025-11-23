# GridTokenX Smart Meter Feature Test Guide

## Overview
This guide provides instructions for testing the smart meter feature in GridTokenX API Gateway. The smart meter functionality allows users to register meters and submit energy readings that can be tokenized on the Solana blockchain.

## Current Status
The smart meter feature is partially implemented with working components:
- ✅ Meter registration API
- ✅ Meter reading submission API
- ✅ Meter verification service
- ❌ Meter polling service (has compilation errors)
- ❌ Automated token minting (depends on polling service)

## Prerequisites
1. PostgreSQL database running (default: port 5432)
2. Redis server running (default: port 6379)
3. Solana RPC endpoint configured
4. Rust toolchain installed

## Testing the Smart Meter Feature

### Method 1: Using Basic Test Scripts

We've created two basic test scripts that test core functionality without requiring the problematic polling service:

1. **Meter Registration Test**
   ```bash
   chmod +x test_scripts/basic/test_meter_registration.sh
   ./test_scripts/basic/test_meter_registration.sh
   ```

2. **Meter Reading Test**
   ```bash
   chmod +x test_scripts/basic/test_meter_reading.sh
   ./test_scripts/basic/test_meter_reading.sh
   ```

### Method 2: Manual API Testing

1. **Start API Gateway**
   ```bash
   # Note: You'll need to fix compilation errors first (see "Known Issues" below)
   cargo run
   ```

2. **Register a User**
   ```bash
   curl -X POST "http://localhost:8080/api/auth/register" \
     -H "Content-Type: application/json" \
     -d '{
       "email": "test@example.com",
       "password": "password123",
       "first_name": "John",
       "last_name": "Doe"
     }'
   ```

3. **Login and Get JWT Token**
   ```bash
   curl -X POST "http://localhost:8080/api/auth/login" \
     -H "Content-Type: application/json" \
     -d '{
       "email": "test@example.com",
       "password": "password123"
     }'
   ```

4. **Set User Wallet Address**
   ```bash
   # Replace JWT_TOKEN with actual token from login response
   curl -X PUT "http://localhost:8080/api/user/wallet" \
     -H "Authorization: Bearer JWT_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{
       "wallet_address": "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM"
     }'
   ```

5. **Register a Smart Meter**
   ```bash
   curl -X POST "http://localhost:8080/api/meters/register" \
     -H "Authorization: Bearer JWT_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{
       "meter_serial": "METER-001",
       "meter_key": "test-key-12345",
       "verification_method": "manual",
       "manufacturer": "Test Manufacturer",
       "meter_type": "smart",
       "location_address": "123 Test Street"
     }'
   ```

6. **Submit a Meter Reading**
   ```bash
   curl -X POST "http://localhost:8080/api/meters/submit-reading" \
     -H "Authorization: Bearer JWT_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{
       "kwh_amount": "10.5",
       "reading_timestamp": "2024-01-01T12:00:00.000Z",
       "meter_signature": "test-signature-12345"
     }'
   ```

7. **Retrieve User's Meter Readings**
   ```bash
   curl -X GET "http://localhost:8080/api/meters/my-readings" \
     -H "Authorization: Bearer JWT_TOKEN"
   ```

8. **Retrieve User's Meters**
   ```bash
   curl -X GET "http://localhost:8080/api/meters/my-meters" \
     -H "Authorization: Bearer JWT_TOKEN"
   ```

### Method 3: Running Unit Tests

```bash
cargo test meter_verification --lib
```

## Known Issues

1. **Meter Polling Service Compilation Errors**
   - The `meter_polling_service.rs` has multiple compilation errors
   - It's trying to use a database table `minting_retry_queue` that doesn't exist
   - It's trying to call a method `mint_tokens_direct` that doesn't exist in `BlockchainService`

2. **Missing Database Migration**
   - The migration for `minting_retry_queue` table exists but cannot be applied due to a checksum mismatch

3. **WebSocket Integration Issues**
   - Some handlers are trying to use WebSocket broadcasts with incorrect signatures

## Expected Test Results

### Successful Meter Registration
- Response should include a `meter_id` and `verification_status` set to `pending`
- The meter should appear in user's meter list

### Successful Meter Reading Submission
- Response should include a reading `id`, `kwh_amount`, `reading_timestamp`, etc.
- `minted` should be `false` (since automated minting is not working)
- The reading should appear in user's readings list

### Validation Errors
- Negative kWh amounts should be rejected
- Invalid timestamps should be rejected
- Excessively large kWh amounts should be rejected

## Next Steps

To fully implement the smart meter feature:

1. Fix the `minting_retry_queue` table migration
2. Fix compilation errors in `meter_polling_service.rs`
3. Implement the `mint_tokens_direct` method in `BlockchainService`
4. Fix WebSocket broadcasting signatures
5. Test the complete flow from meter reading to token minting

## Architecture Overview

The smart meter feature consists of:

1. **Meter Registration**: Users can register their smart meters with the system
2. **Meter Verification**: System verifies ownership and authenticity of meters
3. **Reading Submission**: Users submit energy readings from their meters
4. **Tokenization**: Energy readings are converted to blockchain tokens
5. **Trading**: Tokens can be traded on the P2P energy marketplace

The current implementation focuses on steps 1-3, with partial implementation of step 4.
