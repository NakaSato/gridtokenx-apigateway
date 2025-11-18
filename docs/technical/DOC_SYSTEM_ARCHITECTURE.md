# GridTokenX Platform - System Architecture

**Document Version**: 1.0  
**Last Updated**: November 15, 2025  
**Status**: Production Architecture  
**Author**: GridTokenX Engineering Team

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Architecture Overview](#architecture-overview)
3. [System Components](#system-components)
4. [Technology Stack](#technology-stack)
5. [Data Flow Architecture](#data-flow-architecture)
6. [Security Architecture](#security-architecture)
7. [Scalability & Performance](#scalability--performance)
8. [Deployment Architecture](#deployment-architecture)

---

## Executive Summary

GridTokenX is a decentralized peer-to-peer (P2P) energy trading platform built on Solana blockchain. The system enables renewable energy producers (prosumers) to trade excess energy directly with consumers through a transparent, automated marketplace with 15-minute trading epochs.

### Key Design Principles

- **Decentralization**: Blockchain-based energy certificates and settlements
- **Transparency**: All trades recorded on-chain with immutable audit trail
- **Efficiency**: Automated market clearing with price-time priority matching
- **Scalability**: Designed for 10,000+ users and 100,000+ daily transactions
- **Security**: Multi-layer security with JWT authentication, rate limiting, and audit logging

### System Capabilities

| Feature | Capability | Performance Target |
|---------|-----------|-------------------|
| Trading Epochs | 15-minute intervals | 96 epochs/day |
| Order Matching | Price-time priority | < 1s for 1,000 orders |
| Settlement | Blockchain confirmation | < 30s average |
| User Capacity | Concurrent traders | 10,000+ users |
| Transaction Volume | Daily settlements | 100,000+ trades |

---

## Architecture Overview

### Three-Tier Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     PRESENTATION LAYER                       │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐     │
│  │   Web App    │  │  Mobile App  │  │   Admin UI   │     │
│  │ (React/Vite) │  │  (Future)    │  │   (React)    │     │
│  └──────────────┘  └──────────────┘  └──────────────┘     │
└─────────────────────────────────────────────────────────────┘
                            │
                    ┌───────┴───────┐
                    │   REST API    │
                    │   WebSocket   │
                    └───────┬───────┘
┌─────────────────────────────────────────────────────────────┐
│                     APPLICATION LAYER                        │
│  ┌──────────────────────────────────────────────────────┐  │
│  │           API Gateway (Rust/Axum)                     │  │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐    │  │
│  │  │  Trading   │  │   Market   │  │ Settlement │    │  │
│  │  │  Service   │  │  Clearing  │  │  Service   │    │  │
│  │  └────────────┘  └────────────┘  └────────────┘    │  │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐    │  │
│  │  │   Epoch    │  │    User    │  │  Analytics │    │  │
│  │  │ Scheduler  │  │   Auth     │  │  Service   │    │  │
│  │  └────────────┘  └────────────┘  └────────────┘    │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        │                   │                   │
┌───────▼───────┐  ┌────────▼────────┐  ┌──────▼──────┐
│   PostgreSQL  │  │   Redis Cache   │  │   Solana    │
│   Database    │  │   Session Store │  │  Blockchain │
└───────────────┘  └─────────────────┘  └─────────────┘
```

### Layer Responsibilities

#### 1. Presentation Layer
- **Purpose**: User interface for traders, admins, and monitoring
- **Components**: React web app, future mobile applications
- **Communication**: REST API (read/write), WebSocket (real-time updates)

#### 2. Application Layer
- **Purpose**: Business logic, order matching, and blockchain integration
- **Framework**: Rust with Axum web framework
- **Services**: Trading, market clearing, settlement, authentication, analytics

#### 3. Data Layer
- **PostgreSQL**: Primary data store (users, orders, settlements)
- **Redis**: Caching layer (sessions, order books, market data)
- **Solana**: Blockchain ledger (energy certificates, settlements)

---

## System Components

### 1. API Gateway (Rust/Axum)

**Location**: `api-gateway/`  
**Language**: Rust  
**Framework**: Axum 0.7  
**Purpose**: Central backend service handling all business logic

#### Core Services

##### Trading Service
```rust
// Location: src/services/trading_service.rs
Features:
- Order placement (buy/sell energy)
- Order cancellation
- Order validation
- Portfolio management
- Trade history queries
```

##### Market Clearing Service
```rust
// Location: src/services/market_clearing_service.rs
Features:
- Epoch management
- Order matching algorithm
- Clearing price calculation
- Settlement generation
- Market statistics
```

##### Epoch Scheduler
```rust
// Location: src/services/epoch_scheduler.rs
Features:
- 15-minute epoch intervals
- Automatic state transitions
- Recovery from server restarts
- Event broadcasting
```

##### Settlement Service
```rust
// Location: src/services/settlement_service.rs
Features:
- Blockchain transaction creation
- Settlement confirmation
- Fee calculation (1% platform fee)
- Transaction history
```

##### Authentication Service
```rust
// Location: src/services/auth.rs
Features:
- JWT token generation/validation
- Role-based access control (RBAC)
- Password hashing (Argon2)
- Session management
```

##### Analytics Service
```rust
// Location: src/services/analytics_service.rs
Features:
- User trading patterns
- Market depth analysis
- Price trend analysis
- Performance metrics
```

#### Middleware Stack

```rust
// Request Flow
Request → Rate Limiter → CORS → Auth → Logging → Handler → Response
```

1. **Rate Limiting**: 100 requests/minute per IP
2. **CORS**: Configured origins for frontend
3. **Authentication**: JWT token validation
4. **Audit Logging**: All requests logged to database
5. **Error Handling**: Standardized ApiError responses

### 2. Blockchain Layer (Solana Programs)

**Location**: `anchor/programs/`  
**Framework**: Anchor 0.30.1  
**Language**: Rust (on-chain), TypeScript (client)

#### Five Solana Programs

##### 1. Energy Token Program
```rust
// Location: programs/energy-token/
Purpose: Tokenization of energy certificates
Features:
- Mint energy tokens (1 token = 1 kWh)
- Transfer tokens between accounts
- Burn tokens (energy consumption)
- Query token balances
```

##### 2. Trading Program
```rust
// Location: programs/trading/
Purpose: On-chain order book and trade execution
Features:
- Create trade orders
- Execute matched trades
- Cancel orders
- Query order history
```

##### 3. Oracle Program
```rust
// Location: programs/oracle/
Purpose: External data feeds (energy prices, grid data)
Features:
- Submit price data
- Query oracle feeds
- Oracle authority management
- Data validation
```

##### 4. Registry Program
```rust
// Location: programs/registry/
Purpose: User and device registration
Features:
- Register prosumers/consumers
- Register smart meters
- Verify certificates (ERC)
- Update user profiles
```

##### 5. Governance Program
```rust
// Location: programs/governance/
Purpose: Platform governance and PoA consensus
Features:
- Validator management
- Proposal creation/voting
- Emergency pause mechanism
- Authority rotation
```

### 3. Database Layer

#### PostgreSQL Schema

**Version**: 16.x  
**Location**: `api-gateway/migrations/`

##### Core Tables

```sql
-- Users and authentication
users (id, wallet_address, role, created_at)
user_profiles (user_id, energy_capacity, location)
sessions (token, user_id, expires_at)

-- Trading and orders
trading_orders (id, user_id, epoch_id, energy_amount, price_per_kwh)
market_epochs (id, epoch_number, start_time, end_time, status)
order_matches (id, buy_order_id, sell_order_id, matched_amount)

-- Settlement and blockchain
settlements (id, buyer_id, seller_id, transaction_hash, status)
blockchain_transactions (id, tx_hash, status, confirmation_time)

-- Analytics and monitoring
audit_logs (id, user_id, action, timestamp, metadata)
performance_metrics (id, endpoint, response_time, timestamp)
```

##### Indexing Strategy
```sql
-- Time-based queries (epoch transitions)
CREATE INDEX idx_epochs_timing ON market_epochs(start_time, end_time);

-- Order matching queries
CREATE INDEX idx_orders_epoch ON trading_orders(epoch_id, status);

-- User history queries
CREATE INDEX idx_settlements_user ON settlements(buyer_id, seller_id);

-- Performance monitoring
CREATE INDEX idx_audit_logs_time ON audit_logs(timestamp DESC);
```

#### Redis Cache Layer

**Version**: 7.x  
**Purpose**: High-performance caching and session store

##### Cache Keys Pattern
```redis
# User sessions (30 min TTL)
session:{token} → {user_id, role, expires_at}

# Order book snapshots (1 min TTL)
orderbook:{epoch_id} → {buy_orders[], sell_orders[]}

# Market statistics (5 min TTL)
market:stats → {clearing_price, volume, orders_count}

# User rate limiting (1 min TTL)
ratelimit:{user_id} → {request_count, window_start}
```

### 4. WebSocket Service

**Purpose**: Real-time market updates  
**Protocol**: WebSocket (ws://)  
**Location**: `src/services/websocket_service.rs`

#### Event Types

```typescript
// Epoch transitions
{
  type: "epoch_transition",
  epoch_id: "uuid",
  old_status: "active",
  new_status: "cleared"
}

// Order matches
{
  type: "order_matched",
  match_id: "uuid",
  buyer_id: "uuid",
  seller_id: "uuid",
  amount: 100.0,
  price: 0.15
}

// Market updates
{
  type: "market_update",
  epoch_id: "uuid",
  clearing_price: 0.145,
  total_volume: 5000.0
}
```

---

## Technology Stack

### Backend Technologies

| Component | Technology | Version | Purpose |
|-----------|-----------|---------|---------|
| API Framework | Axum | 0.7 | HTTP/WebSocket server |
| Language | Rust | 1.82+ | System programming |
| Database | PostgreSQL | 16 | Primary data store |
| Cache | Redis | 7 | Session/caching layer |
| Blockchain | Solana | 1.18 | Settlement ledger |
| Smart Contracts | Anchor | 0.30.1 | On-chain programs |

### Frontend Technologies

| Component | Technology | Version | Purpose |
|-----------|-----------|---------|---------|
| Framework | React | 18 | UI library |
| Build Tool | Vite | 5 | Development server |
| Language | TypeScript | 5 | Type safety |
| Wallet | Solana Wallet Adapter | Latest | Blockchain integration |
| Charts | Chart.js | 4 | Data visualization |

### Development Tools

| Tool | Purpose |
|------|---------|
| Docker | Containerization |
| Docker Compose | Local development |
| GitHub Actions | CI/CD pipeline |
| SQLx | Database ORM (compile-time checked) |
| Vitest | JavaScript testing |
| Cargo | Rust package manager |
| pnpm | JavaScript package manager |

---

## Data Flow Architecture

### 1. Order Placement Flow

```
User → Frontend → API Gateway → Database
                      ↓
               Epoch Scheduler
                      ↓
            Market Clearing Service
                      ↓
         [Order Matching Algorithm]
                      ↓
              Settlement Service
                      ↓
            Solana Blockchain
                      ↓
         WebSocket Notification
                      ↓
              Frontend Update
```

**Steps**:
1. User submits order via web interface
2. API Gateway validates order (auth, balance, limits)
3. Trading service creates order record in database
4. Order assigned to current/next active epoch
5. Epoch scheduler triggers matching at epoch end
6. Market clearing service executes matching algorithm
7. Settlement service creates blockchain transactions
8. WebSocket broadcasts updates to connected clients
9. Frontend displays trade confirmation

### 2. Epoch Lifecycle Flow

```
Time: 00:00 → Epoch Created (pending)
Time: 00:00 → Epoch Activated (accepting orders)
Time: 00:15 → Epoch Expired (order collection ends)
         ↓
   Order Matching
         ↓
   Settlement Creation
         ↓
   Blockchain Submission
         ↓
Time: 00:15:30 → Epoch Cleared
Time: 00:16:00 → Epoch Settled (blockchain confirmed)
```

### 3. Market Clearing Algorithm Flow

```sql
1. Fetch pending orders for epoch
   SELECT * FROM trading_orders 
   WHERE epoch_id = $1 AND status = 'pending'

2. Sort orders (price-time priority)
   Buy Orders:  DESC price, ASC created_at
   Sell Orders: ASC price, ASC created_at

3. Match orders
   WHILE buy_price >= sell_price:
     matched_amount = MIN(buy_qty, sell_qty)
     CREATE order_match
     UPDATE order quantities

4. Create settlements
   FOR EACH match:
     CREATE settlement
     CALCULATE fees (1%)
     PREPARE blockchain transaction

5. Update epoch statistics
   UPDATE market_epochs SET
     clearing_price = $1,
     total_volume = $2,
     matched_orders = $3,
     status = 'cleared'
```

---

## Security Architecture

### 1. Authentication & Authorization

#### JWT Token Structure
```json
{
  "sub": "user_uuid",
  "role": "prosumer|consumer|admin",
  "iat": 1700000000,
  "exp": 1700001800
}
```

#### Role-Based Access Control (RBAC)

| Role | Permissions |
|------|------------|
| **Consumer** | Place buy orders, view portfolio, cancel own orders |
| **Prosumer** | Place buy/sell orders, view production, manage devices |
| **Admin** | Trigger epochs, view all orders, system configuration |
| **Oracle** | Submit price data, update market feeds |

### 2. Rate Limiting

```rust
// Per-user limits
const MAX_REQUESTS_PER_MINUTE: u32 = 100;
const MAX_ORDERS_PER_EPOCH: u32 = 50;

// Per-IP limits (DDoS protection)
const MAX_REQUESTS_PER_IP: u32 = 300;
```

### 3. Audit Logging

All security-sensitive actions logged:
- User authentication (login/logout)
- Order placement/cancellation
- Admin actions (epoch triggers)
- Failed authentication attempts
- Rate limit violations

### 4. Data Encryption

- **In Transit**: TLS 1.3 for all API requests
- **At Rest**: PostgreSQL encryption for sensitive data
- **Passwords**: Argon2id hashing with salt
- **Private Keys**: Hardware wallet integration (Ledger/Trezor)

---

## Scalability & Performance

### Horizontal Scaling Strategy

```
┌─────────────────────────────────────────────┐
│           Load Balancer (NGINX)             │
└─────────────────┬───────────────────────────┘
                  │
      ┌───────────┼───────────┐
      │           │           │
┌─────▼────┐ ┌────▼─────┐ ┌──▼──────┐
│ API GW 1 │ │ API GW 2 │ │ API GW 3│
└──────────┘ └──────────┘ └─────────┘
      │           │           │
      └───────────┼───────────┘
                  │
      ┌───────────┴───────────┐
      │                       │
┌─────▼────────┐    ┌─────────▼──────┐
│ PostgreSQL   │    │ Redis Cluster  │
│ (Primary +   │    │ (3 nodes)      │
│  Replicas)   │    │                │
└──────────────┘    └────────────────┘
```

### Performance Targets

| Metric | Target | Current |
|--------|--------|---------|
| API Response Time (p95) | < 200ms | 150ms |
| Order Matching Time | < 1s (1000 orders) | 800ms |
| Settlement Time | < 30s | 25s avg |
| WebSocket Latency | < 50ms | 35ms |
| Database Queries (p95) | < 50ms | 40ms |

### Caching Strategy

```
┌─────────────────────────────────────┐
│      Redis Cache Layers             │
├─────────────────────────────────────┤
│ L1: User Sessions (30 min TTL)     │
│ L2: Order Books (1 min TTL)        │
│ L3: Market Stats (5 min TTL)       │
│ L4: User Profiles (15 min TTL)     │
└─────────────────────────────────────┘
```

### Database Optimization

1. **Connection Pooling**: 20 connections per API instance
2. **Prepared Statements**: SQLx compile-time query validation
3. **Indexes**: Strategic indexing on hot paths
4. **Partitioning**: Time-based partitioning for audit logs
5. **Materialized Views**: Pre-computed analytics tables

---

## Deployment Architecture

### Production Environment

```
┌─────────────────────────────────────────────────────┐
│                   AWS/GCP Cloud                      │
├─────────────────────────────────────────────────────┤
│  ┌────────────────────────────────────────────┐    │
│  │      Kubernetes Cluster                     │    │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐ │    │
│  │  │ API GW   │  │ API GW   │  │ API GW   │ │    │
│  │  │  Pod 1   │  │  Pod 2   │  │  Pod 3   │ │    │
│  │  └──────────┘  └──────────┘  └──────────┘ │    │
│  └────────────────────────────────────────────┘    │
│                                                      │
│  ┌──────────────┐  ┌──────────────┐               │
│  │ PostgreSQL   │  │ Redis        │               │
│  │ (RDS/Cloud   │  │ (ElastiCache/│               │
│  │  SQL)        │  │  MemoryStore)│               │
│  └──────────────┘  └──────────────┘               │
└─────────────────────────────────────────────────────┘
```

### Monitoring Stack

```
┌─────────────────────────────────────────┐
│         Monitoring & Observability       │
├─────────────────────────────────────────┤
│ Prometheus  → Metrics collection        │
│ Grafana     → Visualization dashboards  │
│ Loki        → Log aggregation           │
│ Jaeger      → Distributed tracing       │
└─────────────────────────────────────────┘
```

### Backup Strategy

- **Database**: Daily full backup + hourly incremental
- **Redis**: AOF + RDB snapshots
- **Blockchain**: Not backed up (immutable ledger)
- **Retention**: 90 days for production data

---

## Appendix

### A. Performance Benchmarks

```
Load Test Results (1000 concurrent users):
- Orders/second: 2,500
- Average latency: 150ms
- p95 latency: 300ms
- p99 latency: 500ms
- Error rate: 0.01%
```

### B. System Requirements

**API Gateway Instance**:
- CPU: 4 cores
- RAM: 8 GB
- Disk: 100 GB SSD
- Network: 1 Gbps

**Database Instance**:
- CPU: 8 cores
- RAM: 32 GB
- Disk: 500 GB SSD
- IOPS: 10,000+

### C. Related Documents

- [API Reference](./API_REFERENCE.md)
- [Database Schema](./specifications/DATABASE_SCHEMA.md)
- [Deployment Guide](./DEPLOYMENT_GUIDE.md)
- [Market Clearing Engine Design](../plan/MARKET_CLEARING_ENGINE_DESIGN.md)

---

**Document Status**: ✅ Complete  
**Next Review**: January 2026  
**Maintainer**: GridTokenX Engineering Team
