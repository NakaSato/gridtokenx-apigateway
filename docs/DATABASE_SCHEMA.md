# GridTokenX Database Schema Diagram

```mermaid
erDiagram
    users ||--o{ meter_registry : "owns"
    users ||--o{ meter_readings : "submits"
    users ||--o{ trading_orders : "creates"
    users ||--o{ erc_certificates : "owns"
    users ||--o{ settlements : "buyer/seller"
    users ||--o{ user_activities : "logs"
    users ||--o{ audit_logs : "tracked"
    users ||--o{ blockchain_transactions : "initiates"
    
    meter_registry ||--o{ meter_readings : "records"
    meter_registry }o--|| users : "verified_by"
    
    meter_readings ||--o{ minting_retry_queue : "retries"
    
    market_epochs ||--o{ trading_orders : "contains"
    market_epochs ||--o{ order_matches : "contains"
    market_epochs ||--o{ settlements : "settles"
    
    trading_orders ||--o{ order_matches : "buy_order"
    trading_orders ||--o{ order_matches : "sell_order"
    
    order_matches }o--|| settlements : "settled_by"
    
    erc_certificates ||--o{ erc_certificate_transfers : "transferred"
    erc_certificate_transfers }o--|| users : "from_user"
    erc_certificate_transfers }o--|| users : "to_user"
    
    users {
        uuid id PK
        varchar email UK
        varchar username UK
        varchar wallet_address UK
        varchar role
        boolean email_verified
        boolean blockchain_registered
        timestamptz created_at
    }
    
    meter_registry {
        uuid id PK
        uuid user_id FK
        varchar meter_serial UK
        varchar verification_status
        varchar meter_type
        uuid verified_by FK
        timestamptz created_at
    }
    
    meter_readings {
        uuid id PK
        uuid meter_id FK
        uuid user_id FK
        varchar wallet_address
        numeric energy_generated
        numeric energy_consumed
        boolean minted
        varchar blockchain_status
        timestamptz timestamp
    }
    
    market_epochs {
        uuid id PK
        bigint epoch_number UK
        timestamptz start_time
        timestamptz end_time
        varchar status
        numeric clearing_price
    }
    
    trading_orders {
        uuid id PK
        uuid user_id FK
        uuid epoch_id FK
        varchar order_type
        numeric energy_amount
        numeric price_per_kwh
        varchar status
    }
    
    order_matches {
        uuid id PK
        uuid epoch_id FK
        uuid buy_order_id FK
        uuid sell_order_id FK
        numeric matched_amount
        numeric match_price
        varchar status
    }
    
    settlements {
        uuid id PK
        uuid epoch_id FK
        uuid buyer_id FK
        uuid seller_id FK
        numeric energy_amount
        numeric total_amount
        varchar status
    }
    
    erc_certificates {
        uuid id PK
        uuid user_id FK
        varchar certificate_id UK
        numeric energy_amount
        varchar status
        timestamptz issue_date
    }
    
    blockchain_events {
        uuid id PK
        bigint slot
        varchar event_type
        varchar signature
        jsonb data
        timestamptz processed_at
    }
```

## Table Relationships Summary

### Core User Flow
1. **User Registration** → `users` table
2. **Email Verification** → `users.email_verified = true`
3. **Wallet Setup** → `users.wallet_address`
4. **Meter Registration** → `meter_registry` (pending)
5. **Admin Verification** → `meter_registry.verification_status = verified`
6. **Submit Readings** → `meter_readings`
7. **Token Minting** → `meter_readings.minted = true`

### Trading Flow
1. **Create Order** → `trading_orders`
2. **Epoch Processing** → `market_epochs`
3. **Order Matching** → `order_matches`
4. **Settlement** → `settlements`
5. **Blockchain Tx** → `blockchain_transactions`

### Certificate Flow
1. **Issue Certificate** → `erc_certificates`
2. **Transfer** → `erc_certificate_transfers`
3. **Retire** → `erc_certificates.status = retired`

## Key Indexes

### Performance-Critical Indexes

**users**:
- `idx_users_email` (login)
- `idx_users_wallet` (blockchain lookup)
- `idx_users_email_verified` (verification check)

**meter_readings**:
- `idx_meter_readings_wallet` (user queries)
- `idx_meter_readings_timestamp` (time-based queries)
- `idx_meter_readings_blockchain_status` (processing)

**trading_orders**:
- `idx_trading_orders_epoch` (epoch queries)
- `idx_trading_orders_status` (active orders)
- `idx_trading_orders_user` (user orders)

## Foreign Key Cascade Rules

| Relationship | On Delete | Reason |
|--------------|-----------|--------|
| meter_registry → users | CASCADE | Remove meters when user deleted |
| meter_readings → users | SET NULL | Keep readings for audit |
| meter_readings → meter_registry | SET NULL | Support legacy readings |
| trading_orders → users | CASCADE | Remove orders when user deleted |
| settlements → users | CASCADE | Remove settlements when user deleted |

## Schema Version Control

All migrations tracked in `_sqlx_migrations` table:
- 21 migrations applied
- Sequential versioning
- Checksum validation
- Rollback support (manual)
