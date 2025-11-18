# Plan: Meter Verification After Email Authentication

## Overview
Add a mandatory meter verification step after successful email verification. Users must input their smart meter's unique key to prove ownership before they can submit meter readings or mint energy tokens. This prevents unauthorized users from submitting fraudulent readings.

## Architecture Changes

### Authentication Flow Enhancement
```
Current Flow:
Register → Verify Email → Login → Connect Wallet → Submit Readings

New Flow:
Register → Verify Email → Login → **Verify Meter** → Connect Wallet → Submit Readings
```

### Meter Key Types
Support multiple verification methods based on meter manufacturer:
1. **Meter Serial Number** - Physical label on meter (e.g., `SM-2024-A1B2C3D4`)
2. **API Key** - Provided by utility company for smart meter API access
3. **QR Code** - Scanned from meter display (contains encrypted ownership proof)
4. **Challenge-Response** - Meter displays time-based code that user enters

## Implementation Steps

### 1. Database Schema Extensions

#### Add Meter Registry Table
```sql
// filepath: migrations/20241119000001_add_meter_verification.sql
-- Store registered meters with ownership proof
CREATE TABLE meter_registry (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    
    -- Meter identification
    meter_serial VARCHAR(64) NOT NULL UNIQUE,
    meter_type VARCHAR(32) NOT NULL, -- 'residential', 'commercial', 'solar'
    manufacturer VARCHAR(64), -- 'Landis+Gyr', 'Itron', 'Elster'
    
    -- Verification credentials
    verification_method VARCHAR(32) NOT NULL, -- 'serial', 'api_key', 'qr_code', 'challenge'
    meter_key_hash VARCHAR(128) NOT NULL, -- Hashed meter key (bcrypt)
    api_key_encrypted TEXT, -- Encrypted meter API key (for automated readings)
    
    -- Ownership proof
    verification_proof TEXT, -- QR code data, utility bill upload reference
    verified_at TIMESTAMPTZ,
    verification_status VARCHAR(32) NOT NULL DEFAULT 'pending', -- 'pending', 'verified', 'rejected', 'suspended'
    
    -- Metadata
    location_address TEXT,
    installation_date DATE,
    last_reading_at TIMESTAMPTZ,
    
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    CONSTRAINT unique_user_meter UNIQUE(user_id, meter_serial)
);

-- Add meter_id reference to meter_readings
ALTER TABLE meter_readings 
ADD COLUMN meter_id UUID REFERENCES meter_registry(id),
ADD COLUMN verification_status VARCHAR(32) DEFAULT 'unverified';

CREATE INDEX idx_meter_registry_user_id ON meter_registry(user_id);
CREATE INDEX idx_meter_registry_serial ON meter_registry(meter_serial);
CREATE INDEX idx_meter_registry_status ON meter_registry(verification_status);
CREATE INDEX idx_meter_readings_meter_id ON meter_readings(meter_id);

-- Audit log for verification attempts
CREATE TABLE meter_verification_attempts (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id),
    meter_serial VARCHAR(64) NOT NULL,
    verification_method VARCHAR(32) NOT NULL,
    attempt_status VARCHAR(32) NOT NULL, -- 'success', 'invalid_key', 'meter_claimed', 'rate_limited'
    ip_address INET,
    user_agent TEXT,
    attempted_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_verification_attempts_user ON meter_verification_attempts(user_id, attempted_at);
CREATE INDEX idx_verification_attempts_serial ON meter_verification_attempts(meter_serial, attempted_at);
```

### 2. Add Meter Verification Service

```rust
use anyhow::{Context, Result};
use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum VerificationMethod {
    Serial,          // Simple serial number + key
    ApiKey,          // Utility company API key
    QrCode,          // QR code scan with encrypted data
    Challenge,       // Time-based challenge-response
}

impl VerificationMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Serial => "serial",
            Self::ApiKey => "api_key",
            Self::QrCode => "qr_code",
            Self::Challenge => "challenge",
        }
    }
}

#[derive(Debug)]
pub struct MeterVerificationRequest {
    pub user_id: Uuid,
    pub meter_serial: String,
    pub meter_key: String,
    pub verification_method: VerificationMethod,
    pub manufacturer: Option<String>,
    pub meter_type: String,
    pub location_address: Option<String>,
    pub verification_proof: Option<String>, // QR data, utility bill reference
}

#[derive(Debug, sqlx::FromRow)]
pub struct MeterRegistry {
    pub id: Uuid,
    pub user_id: Uuid,
    pub meter_serial: String,
    pub meter_type: String,
    pub manufacturer: Option<String>,
    pub verification_method: String,
    pub verification_status: String,
    pub verified_at: Option<chrono::DateTime<Utc>>,
    pub created_at: chrono::DateTime<Utc>,
}

pub struct MeterVerificationService {
    db: PgPool,
}

impl MeterVerificationService {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Verify meter ownership and register meter to user account
    pub async fn verify_meter(&self, req: MeterVerificationRequest) -> Result<MeterRegistry> {
        // 1. Rate limiting check (max 5 attempts per hour per user)
        self.check_rate_limit(&req.user_id).await?;

        // 2. Check if meter already claimed by another user
        if let Some(existing) = self.find_meter_by_serial(&req.meter_serial).await? {
            self.log_attempt(
                &req.user_id,
                &req.meter_serial,
                &req.verification_method,
                "meter_claimed",
                None,
            )
            .await?;
            
            anyhow::bail!("Meter {} is already registered to another account", req.meter_serial);
        }

        // 3. Validate meter key format based on verification method
        self.validate_meter_key(&req.meter_key, &req.verification_method)?;

        // 4. Hash meter key for storage (never store plaintext)
        let meter_key_hash = hash(&req.meter_key, DEFAULT_COST)
            .context("Failed to hash meter key")?;

        // 5. Optional: Verify with utility company API (if api_key method)
        if matches!(req.verification_method, VerificationMethod::ApiKey) {
            self.verify_with_utility_api(&req.meter_serial, &req.meter_key).await?;
        }

        // 6. Register meter in database
        let meter = sqlx::query_as!(
            MeterRegistry,
            r#"
            INSERT INTO meter_registry (
                user_id, meter_serial, meter_type, manufacturer,
                verification_method, meter_key_hash, verification_proof,
                verification_status, verified_at, location_address
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'verified', NOW(), $8)
            RETURNING id, user_id, meter_serial, meter_type, manufacturer,
                      verification_method, verification_status,
                      verified_at, created_at
            "#,
            req.user_id,
            req.meter_serial,
            req.meter_type,
            req.manufacturer,
            req.verification_method.as_str(),
            meter_key_hash,
            req.verification_proof,
            req.location_address
        )
        .fetch_one(&self.db)
        .await
        .context("Failed to register meter")?;

        // 7. Log successful verification
        self.log_attempt(
            &req.user_id,
            &req.meter_serial,
            &req.verification_method,
            "success",
            None,
        )
        .await?;

        tracing::info!(
            user_id = %req.user_id,
            meter_serial = %req.meter_serial,
            method = %req.verification_method.as_str(),
            "Meter verified successfully"
        );

        Ok(meter)
    }

    /// Check if user has exceeded verification attempt rate limit
    async fn check_rate_limit(&self, user_id: &Uuid) -> Result<()> {
        let one_hour_ago = Utc::now() - chrono::Duration::hours(1);
        
        let attempt_count: i64 = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) as "count!"
            FROM meter_verification_attempts
            WHERE user_id = $1 AND attempted_at > $2
            "#,
            user_id,
            one_hour_ago
        )
        .fetch_one(&self.db)
        .await?;

        if attempt_count >= 5 {
            anyhow::bail!("Rate limit exceeded. Maximum 5 verification attempts per hour.");
        }

        Ok(())
    }

    /// Find meter by serial number
    async fn find_meter_by_serial(&self, serial: &str) -> Result<Option<MeterRegistry>> {
        let meter = sqlx::query_as!(
            MeterRegistry,
            r#"
            SELECT id, user_id, meter_serial, meter_type, manufacturer,
                   verification_method, verification_status,
                   verified_at, created_at
            FROM meter_registry
            WHERE meter_serial = $1
            "#,
            serial
        )
        .fetch_optional(&self.db)
        .await?;

        Ok(meter)
    }

    /// Validate meter key format
    fn validate_meter_key(&self, key: &str, method: &VerificationMethod) -> Result<()> {
        match method {
            VerificationMethod::Serial => {
                // Serial keys: 16-32 alphanumeric characters
                if key.len() < 16 || key.len() > 32 {
                    anyhow::bail!("Invalid meter key length. Expected 16-32 characters.");
                }
                if !key.chars().all(|c| c.is_alphanumeric() || c == '-') {
                    anyhow::bail!("Invalid meter key format. Only alphanumeric characters and hyphens allowed.");
                }
            }
            VerificationMethod::ApiKey => {
                // API keys: 32-64 characters
                if key.len() < 32 || key.len() > 64 {
                    anyhow::bail!("Invalid API key length. Expected 32-64 characters.");
                }
            }
            VerificationMethod::QrCode => {
                // QR codes: Base64 encoded data
                if key.is_empty() {
                    anyhow::bail!("QR code data cannot be empty.");
                }
            }
            VerificationMethod::Challenge => {
                // Challenge codes: 6-8 digit numeric
                if !key.chars().all(|c| c.is_numeric()) || key.len() < 6 || key.len() > 8 {
                    anyhow::bail!("Invalid challenge code. Expected 6-8 digit number.");
                }
            }
        }

        Ok(())
    }

    /// Verify meter with utility company API (mock implementation)
    async fn verify_with_utility_api(&self, serial: &str, api_key: &str) -> Result<()> {
        // TODO: Implement actual utility API integration
        // For now, accept any API key with correct format
        tracing::info!(
            meter_serial = %serial,
            "Utility API verification skipped (not implemented)"
        );
        Ok(())
    }

    /// Log verification attempt for audit trail
    async fn log_attempt(
        &self,
        user_id: &Uuid,
        meter_serial: &str,
        method: &VerificationMethod,
        status: &str,
        ip_address: Option<&str>,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO meter_verification_attempts (
                user_id, meter_serial, verification_method, attempt_status, ip_address
            )
            VALUES ($1, $2, $3, $4, $5)
            "#,
            user_id,
            meter_serial,
            method.as_str(),
            status,
            ip_address.map(|s| s.parse::<std::net::IpAddr>().ok()).flatten()
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }

    /// Get user's registered meters
    pub async fn get_user_meters(&self, user_id: &Uuid) -> Result<Vec<MeterRegistry>> {
        let meters = sqlx::query_as!(
            MeterRegistry,
            r#"
            SELECT id, user_id, meter_serial, meter_type, manufacturer,
                   verification_method, verification_status,
                   verified_at, created_at
            FROM meter_registry
            WHERE user_id = $1
            ORDER BY created_at DESC
            "#,
            user_id
        )
        .fetch_all(&self.db)
        .await?;

        Ok(meters)
    }

    /// Verify meter ownership when submitting reading
    pub async fn verify_meter_ownership(
        &self,
        user_id: &Uuid,
        meter_id: &Uuid,
    ) -> Result<bool> {
        let exists: bool = sqlx::query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM meter_registry
                WHERE id = $1 AND user_id = $2 AND verification_status = 'verified'
            ) as "exists!"
            "#,
            meter_id,
            user_id
        )
        .fetch_one(&self.db)
        .await?;

        Ok(exists)
    }

    /// Re-verify meter key (for re-authentication)
    pub async fn reverify_meter_key(
        &self,
        meter_id: &Uuid,
        meter_key: &str,
    ) -> Result<bool> {
        let stored_hash: Option<String> = sqlx::query_scalar!(
            "SELECT meter_key_hash FROM meter_registry WHERE id = $1",
            meter_id
        )
        .fetch_optional(&self.db)
        .await?;

        if let Some(hash) = stored_hash {
            Ok(verify(meter_key, &hash)?)
        } else {
            Ok(false)
        }
    }
}
```

### 3. Add API Handlers

```rust
use axum::{
    extract::{Extension, Json, Path},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::AppError,
    middleware::auth::UserClaims,
    services::meter_verification_service::{MeterVerificationRequest, MeterVerificationService, VerificationMethod},
    AppState,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyMeterRequest {
    pub meter_serial: String,
    pub meter_key: String,
    pub verification_method: String, // "serial", "api_key", "qr_code", "challenge"
    pub manufacturer: Option<String>,
    pub meter_type: String, // "residential", "commercial", "solar"
    pub location_address: Option<String>,
    pub verification_proof: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeterVerificationResponse {
    pub meter_id: String,
    pub meter_serial: String,
    pub verification_status: String,
    pub verified_at: Option<String>,
    pub message: String,
}

/// Verify meter ownership and register meter to user account
#[utoipa::path(
    post,
    path = "/api/meters/verify",
    tag = "Meter Verification",
    request_body = VerifyMeterRequest,
    responses(
        (status = 200, description = "Meter verified successfully", body = MeterVerificationResponse),
        (status = 400, description = "Invalid meter key or meter already claimed"),
        (status = 401, description = "Unauthorized - user not authenticated"),
        (status = 429, description = "Rate limit exceeded")
    ),
    security(
        ("bearer" = [])
    )
)]
pub async fn verify_meter_handler(
    Extension(user_claims): Extension<UserClaims>,
    Extension(app_state): Extension<AppState>,
    Json(payload): Json<VerifyMeterRequest>,
) -> Result<impl IntoResponse, AppError> {
    tracing::info!(
        user_id = %user_claims.sub,
        meter_serial = %payload.meter_serial,
        "Verifying meter ownership"
    );

    // Parse verification method
    let method = match payload.verification_method.as_str() {
        "serial" => VerificationMethod::Serial,
        "api_key" => VerificationMethod::ApiKey,
        "qr_code" => VerificationMethod::QrCode,
        "challenge" => VerificationMethod::Challenge,
        _ => return Err(AppError::bad_request("Invalid verification method")),
    };

    // Build verification request
    let req = MeterVerificationRequest {
        user_id: user_claims.sub,
        meter_serial: payload.meter_serial.clone(),
        meter_key: payload.meter_key,
        verification_method: method,
        manufacturer: payload.manufacturer,
        meter_type: payload.meter_type,
        location_address: payload.location_address,
        verification_proof: payload.verification_proof,
    };

    // Verify meter
    let meter = app_state.meter_verification_service
        .verify_meter(req)
        .await
        .map_err(|e| {
            if e.to_string().contains("already registered") {
                AppError::bad_request("This meter is already registered to another account")
            } else if e.to_string().contains("Rate limit") {
                AppError::too_many_requests("Too many verification attempts. Please try again later.")
            } else {
                AppError::internal(format!("Meter verification failed: {}", e))
            }
        })?;

    Ok((
        StatusCode::OK,
        Json(MeterVerificationResponse {
            meter_id: meter.id.to_string(),
            meter_serial: meter.meter_serial,
            verification_status: meter.verification_status,
            verified_at: meter.verified_at.map(|dt| dt.to_rfc3339()),
            message: "Meter verified successfully. You can now submit meter readings.".to_string(),
        }),
    ))
}

/// Get user's registered meters
#[utoipa::path(
    get,
    path = "/api/meters/registered",
    tag = "Meter Verification",
    responses(
        (status = 200, description = "List of registered meters", body = Vec<MeterRegistryResponse>),
        (status = 401, description = "Unauthorized")
    ),
    security(
        ("bearer" = [])
    )
)]
pub async fn get_registered_meters_handler(
    Extension(user_claims): Extension<UserClaims>,
    Extension(app_state): Extension<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let meters = app_state.meter_verification_service
        .get_user_meters(&user_claims.sub)
        .await
        .map_err(|e| AppError::internal(format!("Failed to fetch meters: {}", e)))?;

    let response: Vec<MeterRegistryResponse> = meters
        .into_iter()
        .map(|m| MeterRegistryResponse {
            meter_id: m.id.to_string(),
            meter_serial: m.meter_serial,
            meter_type: m.meter_type,
            manufacturer: m.manufacturer,
            verification_status: m.verification_status,
            verified_at: m.verified_at.map(|dt| dt.to_rfc3339()),
        })
        .collect();

    Ok(Json(response))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeterRegistryResponse {
    pub meter_id: String,
    pub meter_serial: String,
    pub meter_type: String,
    pub manufacturer: Option<String>,
    pub verification_status: String,
    pub verified_at: Option<String>,
}

/// Check if user has verified meters (used in middleware)
pub async fn check_meter_verification_status(
    user_id: &Uuid,
    service: &MeterVerificationService,
) -> Result<bool, AppError> {
    let meters = service
        .get_user_meters(user_id)
        .await
        .map_err(|e| AppError::internal(format!("Failed to check meter status: {}", e)))?;

    Ok(!meters.is_empty() && meters.iter().any(|m| m.verification_status == "verified"))
}
```

### 4. Update Meter Reading Submission

```rust
// Add meter_id validation to submit_reading handler

// ...existing code...

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitReadingRequest {
    pub meter_id: Uuid,  // NEW: Required meter ID
    pub kwh_amount: String,
    pub reading_timestamp: Option<String>,
    pub meter_signature: Option<String>,
}

pub async fn submit_reading(
    Extension(user_claims): Extension<UserClaims>,
    Extension(app_state): Extension<AppState>,
    Json(payload): Json<SubmitReadingRequest>,
) -> Result<impl IntoResponse, AppError> {
    // 1. Verify meter ownership
    let is_owner = app_state.meter_verification_service
        .verify_meter_ownership(&user_claims.sub, &payload.meter_id)
        .await
        .map_err(|e| AppError::internal(format!("Ownership verification failed: {}", e)))?;

    if !is_owner {
        return Err(AppError::forbidden(
            "You do not own this meter or it is not verified"
        ));
    }

    // 2. Parse and validate kWh amount
    let kwh_amount = payload.kwh_amount.parse::<f64>()
        .map_err(|_| AppError::bad_request("Invalid kWh amount format"))?;

    if kwh_amount <= 0.0 || kwh_amount > 1000.0 {
        return Err(AppError::bad_request("kWh amount must be between 0 and 1000"));
    }

    // 3. Insert reading with meter_id reference
    let reading = sqlx::query!(
        r#"
        INSERT INTO meter_readings (user_id, meter_id, kwh_amount, reading_timestamp, minted, verification_status)
        VALUES ($1, $2, $3, $4, false, 'verified')
        RETURNING id, user_id, meter_id, kwh_amount, reading_timestamp, minted, created_at
        "#,
        user_claims.sub,
        payload.meter_id,
        kwh_amount,
        payload.reading_timestamp.as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now)
    )
    .fetch_one(&app_state.db)
    .await
    .map_err(|e| AppError::internal(format!("Failed to submit reading: {}", e)))?;

    // 4. Broadcast WebSocket event (if auto-trigger enabled)
    // ...existing websocket code...

    Ok((StatusCode::CREATED, Json(/* response */)))
}

// ...existing code...
```

### 5. Update Main Router

```rust
use crate::services::meter_verification_service::MeterVerificationService;

// In AppState struct
pub struct AppState {
    // ...existing fields...
    pub meter_verification_service: Arc<MeterVerificationService>,
}

// In main() function after services initialization
let meter_verification_service = Arc::new(MeterVerificationService::new(db_pool.clone()));

let app_state = Arc::new(AppState {
    // ...existing fields...
    meter_verification_service,
});

// Add meter verification routes
let meter_verification_routes = Router::new()
    .route("/api/meters/verify", post(handlers::meter_verification::verify_meter_handler))
    .route("/api/meters/registered", get(handlers::meter_verification::get_registered_meters_handler))
    .layer(middleware::from_fn(auth_middleware));

let app = Router::new()
    // ...existing routes...
    .merge(meter_verification_routes);
```

### 6. Add Middleware for Meter Verification Check

```rust
use axum::{
    extract::Extension,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{
    error::AppError,
    middleware::auth::UserClaims,
    services::meter_verification_service::MeterVerificationService,
    AppState,
};

/// Middleware to ensure user has at least one verified meter
/// Apply this to meter reading submission endpoints
pub async fn require_verified_meter<B>(
    Extension(user_claims): Extension<UserClaims>,
    Extension(app_state): Extension<AppState>,
    request: Request<B>,
    next: Next<B>,
) -> Result<Response, AppError> {
    let has_verified_meter = app_state.meter_verification_service
        .get_user_meters(&user_claims.sub)
        .await
        .map_err(|e| AppError::internal(format!("Failed to check meter status: {}", e)))?
        .iter()
        .any(|m| m.verification_status == "verified");

    if !has_verified_meter {
        return Err(AppError::forbidden(
            "You must verify at least one meter before submitting readings. Please verify your meter at POST /api/meters/verify"
        ));
    }

    Ok(next.run(request).await)
}
```

## API Endpoint Specification

### POST /api/meters/verify
Verify meter ownership after email verification.

**Request:**
```json
{
  "meterSerial": "SM-2024-A1B2C3D4",
  "meterKey": "a1b2c3d4e5f6g7h8",
  "verificationMethod": "serial",
  "manufacturer": "Landis+Gyr",
  "meterType": "residential",
  "locationAddress": "123 Main St, City, State 12345",
  "verificationProof": "utility-bill-ref-2024-11"
}
```

**Response (200 OK):**
```json
{
  "meterId": "550e8400-e29b-41d4-a716-446655440000",
  "meterSerial": "SM-2024-A1B2C3D4",
  "verificationStatus": "verified",
  "verifiedAt": "2025-11-18T10:30:00Z",
  "message": "Meter verified successfully. You can now submit meter readings."
}
```

**Error Responses:**
- `400 Bad Request` - Invalid meter key format or meter already claimed
- `401 Unauthorized` - User not authenticated
- `429 Too Many Requests` - Rate limit exceeded (5 attempts/hour)

### GET /api/meters/registered
Get list of user's registered meters.

**Response (200 OK):**
```json
[
  {
    "meterId": "550e8400-e29b-41d4-a716-446655440000",
    "meterSerial": "SM-2024-A1B2C3D4",
    "meterType": "residential",
    "manufacturer": "Landis+Gyr",
    "verificationStatus": "verified",
    "verifiedAt": "2025-11-18T10:30:00Z"
  }
]
```

### POST /api/meters/submit-reading (Updated)
Submit meter reading - now requires verified meter.

**Request:**
```json
{
  "meterId": "550e8400-e29b-41d4-a716-446655440000",
  "kwhAmount": "25.5",
  "readingTimestamp": "2025-11-18T10:00:00Z"
}
```

**Error Responses:**
- `403 Forbidden` - User does not own meter or meter not verified
- `400 Bad Request` - Invalid meter ID or kWh amount

## User Flow

### 1. Registration & Email Verification
```
POST /api/auth/register
  ↓
Verify email link
  ↓
POST /api/auth/verify-email?token=xxx
  ↓
User logged in (JWT issued)
```

### 2. Meter Verification (NEW STEP)
```
POST /api/meters/verify
{
  "meterSerial": "SM-2024-A1B2C3D4",
  "meterKey": "a1b2c3d4e5f6g7h8",
  "verificationMethod": "serial",
  ...
}
  ↓
Response: { "verificationStatus": "verified" }
  ↓
User can now submit readings
```

### 3. Submit Readings
```
GET /api/meters/registered
  ↓
Select meter from list
  ↓
POST /api/meters/submit-reading
{
  "meterId": "xxx",
  "kwhAmount": "25.5"
}
  ↓
Tokens minted automatically (if auto-mint enabled)
```

## Security Considerations

### 1. Meter Key Storage
- **Never store plaintext keys** - use bcrypt with cost factor 12+
- **Rotate keys periodically** - allow users to update meter keys
- **Audit all verification attempts** - log IP, timestamp, outcome

### 2. Rate Limiting
- **5 attempts per hour per user** - prevents brute force attacks
- **10 attempts per hour per meter serial** - prevents targeted attacks
- **Exponential backoff** - increase delay after failed attempts

### 3. Fraud Prevention
- **One meter per user** - prevent multiple users claiming same meter
- **Meter serial uniqueness** - enforce at database level
- **Verification proof** - require utility bill or QR scan for high-value accounts
- **Suspicious activity monitoring** - alert on multiple failed verifications

### 4. Data Privacy
- **PII protection** - encrypt location_address in database
- **GDPR compliance** - allow users to delete meter registrations
- **Access logs** - track who accessed meter data and when

## Testing Strategy

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_verify_meter_success() {
        // Test successful meter verification
    }

    #[tokio::test]
    async fn test_verify_meter_already_claimed() {
        // Test error when meter claimed by another user
    }

    #[tokio::test]
    async fn test_rate_limit_enforcement() {
        // Test 5 attempts per hour limit
    }

    #[tokio::test]
    async fn test_invalid_meter_key_format() {
        // Test various invalid key formats
    }
}
```

### Integration Tests
```bash
#!/bin/bash
# filepath: scripts/test-meter-verification.sh

# 1. Register user
TOKEN=$(curl -s -X POST http://localhost:8080/api/auth/register \
  -H "Content-Type: application/json" \
  -d '{"email":"test@example.com","password":"Test123!"}' | jq -r '.token')

# 2. Verify meter (should succeed)
curl -X POST http://localhost:8080/api/meters/verify \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "meterSerial": "SM-2024-TEST001",
    "meterKey": "testkey123456789",
    "verificationMethod": "serial",
    "meterType": "residential"
  }' | jq

# 3. Try to claim same meter with different user (should fail)
TOKEN2=$(curl -s -X POST http://localhost:8080/api/auth/register \
  -H "Content-Type: application/json" \
  -d '{"email":"test2@example.com","password":"Test123!"}' | jq -r '.token')

curl -X POST http://localhost:8080/api/meters/verify \
  -H "Authorization: Bearer $TOKEN2" \
  -H "Content-Type: application/json" \
  -d '{
    "meterSerial": "SM-2024-TEST001",
    "meterKey": "testkey123456789",
    "verificationMethod": "serial",
    "meterType": "residential"
  }' | jq

# 4. Submit reading with verified meter (should succeed)
METER_ID=$(curl -s http://localhost:8080/api/meters/registered \
  -H "Authorization: Bearer $TOKEN" | jq -r '.[0].meterId')

curl -X POST http://localhost:8080/api/meters/submit-reading \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{
    \"meterId\": \"$METER_ID\",
    \"kwhAmount\": \"25.5\"
  }" | jq
```

## Environment Configuration

```bash
# Meter Verification Settings
METER_VERIFICATION_RATE_LIMIT_PER_HOUR=5
METER_VERIFICATION_KEY_MIN_LENGTH=16
METER_VERIFICATION_KEY_MAX_LENGTH=64
METER_VERIFICATION_REQUIRE_PROOF=false  # Set true for production

# Optional: Utility API Integration
UTILITY_API_ENABLED=false
UTILITY_API_ENDPOINT="https://utility-api.example.com/verify"
UTILITY_API_KEY="xxx"
```

## Future Enhancements

### Phase 1 (Current Plan)
- ✅ Basic serial number + key verification
- ✅ Rate limiting and fraud prevention
- ✅ Audit logging

### Phase 2 (Q1 2026)
- 🔄 QR code verification with encrypted payload
- 🔄 Time-based challenge-response (TOTP-like)
- 🔄 Utility company API integration for automated verification

### Phase 3 (Q2 2026)
- ⏳ Mobile app with QR scanner
- ⏳ NFC-based verification (tap meter with phone)
- ⏳ Biometric authentication for high-value transactions

### Phase 4 (Q3 2026)
- ⏳ Smart contract-based verification (on-chain proof)
- ⏳ Multi-meter support for commercial/industrial users
- ⏳ Sub-meter hierarchies (building → apartment meters)

## Success Metrics

- **Verification Success Rate**: > 95% first-attempt success
- **Fraud Prevention**: < 0.1% duplicate meter claims
- **User Completion Rate**: > 90% complete verification after email auth
- **Verification Latency**: p95 < 2 seconds
- **Support Tickets**: < 5% users require manual verification assistance

## Dependencies

**New Crates:**
- `bcrypt = "0.15"` - Password hashing for meter keys
- `base64 = "0.21"` - QR code data encoding (Phase 2)

**Database:**
- PostgreSQL with `uuid-ossp` and `pgcrypto` extensions

**Monitoring:**
- Prometheus metrics for verification attempts, success rates, rate limit hits
- Sentry alerts for high fraud attempt rates

## Migration Path for Existing Users

For users who registered before meter verification was implemented:

1. **Grace Period**: Allow 30 days to verify meters
2. **Email Notification**: Send reminder emails every 7 days
3. **Soft Enforcement**: Show warning banner in UI but allow reading submissions
4. **Hard Enforcement**: After grace period, block reading submissions until verified
5. **Admin Override**: Support team can manually verify meters in exceptional cases

```sql
-- Migration script to handle existing users
UPDATE meter_readings
SET verification_status = 'legacy_unverified'
WHERE created_at < '2025-11-18T00:00:00Z';

-- Allow legacy readings to be submitted without meter_id temporarily
ALTER TABLE meter_readings
ALTER COLUMN meter_id DROP NOT NULL;
```

---

**Implementation Priority**: High  
**Estimated Effort**: 3-4 days  
**Risk Level**: Medium (requires careful fraud prevention)  
**User Impact**: High (security improvement, slight UX friction)
