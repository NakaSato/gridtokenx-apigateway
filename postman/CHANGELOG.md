# Postman Collection Changelog

## Version 1.3.0 (2025-11-25)

### 🎉 Major Update - Complete API Coverage

This major update adds comprehensive coverage of all GridTokenX API endpoints, bringing the total from 76+ to **95+ endpoints** across 20 categories.

#### New Endpoint Categories Added

**1. Transaction Management (Category 16)**
- `GET /api/v1/transactions/:id/status` - Get specific transaction status
- `GET /api/v1/transactions/user` - Get user transactions with advanced filtering
- `GET /api/v1/transactions/history` - Get all transactions (admin only)
- `GET /api/v1/transactions/stats` - Get transaction statistics (admin only)
- `POST /api/v1/transactions/:id/retry` - Retry failed transactions

**2. Enhanced Trading v1 API (Category 18)**
- `POST /api/v1/trading/orders` - Create orders with enhanced parameters
- `GET /api/v1/trading/orders` - Get user orders with advanced filtering
- `GET /api/v1/trading/market` - Get market data with order book
- `GET /api/v1/trading/stats` - Get trading statistics

**3. Enhanced Blockchain v1 API (Category 19)**
- `GET /api/v1/blockchain/accounts/:address` - Get account info
- `GET /api/v1/blockchain/network` - Get network status
- `POST /api/v1/blockchain/programs/:name` - Interact with smart contracts
- `POST /api/v1/blockchain/transactions` - Submit enhanced transactions
- `GET /api/v1/blockchain/transactions` - Get transaction history with filtering
- `GET /api/v1/blockchain/transactions/:signature` - Get transaction status

**4. Enhanced Meter Endpoints (Category 20)**
- `POST /api/meters/submit-reading` - Enhanced reading submission with signature support
- `GET /api/meters/my-readings` - Enhanced reading retrieval with advanced filtering
- `GET /api/meters/readings/:wallet_address` - Enhanced wallet-based reading queries

#### Enhanced Admin Operations

**Complete Admin Restructure:**
- **User Management**: Get by ID, update, deactivate, reactivate, get activity, list all
- **Registry Admin**: Update user roles on blockchain
- **Governance Admin**: Emergency pause/unpause controls
- **Token Admin**: Mint tokens with enhanced parameters
- **Trading Admin**: Match orders, market health, analytics, control operations
- **Meter Admin**: Get unminted readings, mint from readings
- **Audit & Security**: User audit logs, type-based logs, security events
- **Epoch Management Admin**: Epoch statistics, list all, manual clearing triggers

#### 🧪 Advanced Test Scripts

**New Test Features:**
- Transaction retry validation with attempt tracking
- Enhanced pagination validation for all list endpoints
- Smart contract interaction testing
- Advanced query parameter validation
- Admin privilege testing in test scripts

#### 📊 Collection Statistics

- **Total Endpoints**: 95+ (increased from 76+)
- **New Categories**: 4 additional categories (16-20)
- **Version**: Updated to 1.3.0
- **Updated Date**: 2025-11-25

#### 🔧 Technical Improvements

**Enhanced Request Bodies:**
- Added `expiry_time` parameter for order creation
- Enhanced `energy_amount` and `price_per_kwh` validation
- Added `meter_signature` support for meter readings
- Smart contract `compute_units` and priority fee parameters

**Advanced Query Parameters:**
- Comprehensive filtering options for all list endpoints
- Date range filtering with ISO 8601 support
- Pagination with configurable page sizes
- Multiple sort fields and directions
- Status and type-based filtering

#### 📝 Documentation Enhancements

- Updated all endpoint descriptions with v1 API details
- Added comprehensive parameter documentation
- Enhanced error response examples
- Added authentication requirement notes
- Updated collection metadata with new endpoint counts

---

## Version 1.2.0 (2025-11-20)

### 🎉 New Features

#### Enhanced User Management Endpoints

**1. Update Wallet Address** (`POST /api/user/wallet`)
- Added comprehensive test scripts for response validation
- Automatically saves wallet address to environment variable
- Validates wallet format (32-44 characters, base58)
- Prevents duplicate wallet addresses
- Tests verify HTTP 200 status and response structure

**2. Remove Wallet Address** (`DELETE /api/user/wallet`)
- Added test scripts for HTTP 204 validation
- Automatically clears wallet_address from environment
- Validates successful deletion

**3. Get User Activity** (`GET /api/user/activity`)
- Added pagination validation tests
- Validates response structure (activities, total, page, per_page, total_pages)
- Added optional `activity_type` filter parameter
- Enhanced query parameter documentation
- Tests verify array types and pagination values

### 📊 Collection Updates

- **Total Endpoints**: 76+ (increased from 75+)
- **Admin Operations**: 16 endpoints (increased from 15)
- **Version**: Updated to 1.2.0
- **Updated Date**: 2025-11-20

### 🧪 Test Automation

All updated endpoints now include:
- Automated response validation
- Environment variable management (auto-save/clear)
- Detailed test assertions
- Query parameter documentation

### 📝 Documentation Improvements

- Enhanced endpoint descriptions with validation details
- Added query parameter descriptions
- Updated collection metadata with recent changes
- Added changelog for tracking updates

---

## Version 1.1.0 (2025-11-19)

### Initial Features

- 75+ endpoints covering all platform functionality
- JWT authentication with automatic token management
- Comprehensive API coverage:
  - Health & Status
  - Authentication & Registration
  - User Management
  - Blockchain Integration
  - Oracle Price Feeds
  - Governance
  - Energy Trading
  - Token Operations
  - Smart Meters
  - ERC Certificates
  - Market Data & Analytics
  - Admin Operations
  - Testing Utilities
  - Audit & Security
  - Epoch Management
  - WebSocket Support
  - Meter Verification

---

## How to Use

### Import Collection
1. Open Postman
2. Click "Import"
3. Select `GridTokenX-API-Gateway.postman_collection.json`
4. Select environment file (Local or Production)

### Run Tests
1. **Authentication**: Login to get JWT token (auto-saved)
2. **Wallet Management**: 
   - Update wallet → Test validates and saves to environment
   - Remove wallet → Test validates and clears from environment
3. **Activity Tracking**: 
   - Get activity → Test validates pagination structure

### Environment Variables

The collection automatically manages:
- `jwt_token` - Saved after login
- `user_id` - Saved after login
- `username` - Saved after login
- `user_role` - Saved after login
- `wallet_address` - Saved after wallet update, cleared after removal

---

## Support

For issues or questions:
- **Email**: wit.chanthawat@gmail.com
- **API Documentation**: `/api/docs` (Swagger UI)
- **Test Script**: `scripts/05-test-user-management-routes.sh`
