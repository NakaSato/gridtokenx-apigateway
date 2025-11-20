# Postman Collection Changelog

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
