//! Shared types for meter management

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::error::{ApiError, Result};
use crate::handlers::SortOrder;

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize, ToSchema)]
pub struct SubmitReadingRequest {
    #[schema(value_type = String)]
    pub kwh_amount: Decimal,
    pub reading_timestamp: chrono::DateTime<chrono::Utc>,
    pub meter_signature: Option<String>,
    /// NEW: Required UUID from meter_registry (for verified meters)
    /// For legacy support, this can be omitted during grace period
    pub meter_id: Option<Uuid>,
    /// Legacy meter serial number (for unverified meters)
    pub meter_serial: Option<String>,
    pub wallet_address: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MeterReadingResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub wallet_address: String,
    #[schema(value_type = String)]
    pub kwh_amount: Decimal,
    pub reading_timestamp: chrono::DateTime<chrono::Utc>,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
    pub minted: bool,
    pub mint_tx_signature: Option<String>,
}

/// Query parameters for meter readings
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct GetReadingsQuery {
    /// Page number (1-indexed)
    #[serde(default = "default_page")]
    pub page: u32,

    /// Number of items per page (max 100)
    #[serde(default = "default_per_page")]
    pub per_page: u32,

    /// Sort field: "submitted_at", "reading_timestamp", "kwh_amount"
    #[serde(default = "default_sort_field")]
    pub sort_by: String,

    /// Sort direction: "asc" or "desc"
    #[serde(default)]
    pub sort_order: SortOrder,

    /// Filter by minted status
    pub minted: Option<bool>,
}

fn default_page() -> u32 {
    crate::constants::pagination::DEFAULT_PAGE
}

fn default_per_page() -> u32 {
    crate::constants::pagination::DEFAULT_PER_PAGE
}

fn default_sort_field() -> String {
    "submitted_at".to_string()
}

impl GetReadingsQuery {
    const ALLOWED_SORT_FIELDS: &'static [&'static str] =
        &["submitted_at", "reading_timestamp", "kwh_amount"];

    pub fn validate(&mut self) -> Result<()> {
        // Normalize pagination
        if self.page < 1 {
            self.page = 1;
        }
        self.per_page = self
            .per_page
            .clamp(1, crate::constants::pagination::MAX_PER_PAGE);

        // Validate sort field
        if !Self::ALLOWED_SORT_FIELDS.contains(&self.sort_by.as_str()) {
            return Err(ApiError::validation_error(
                format!(
                    "Invalid sort_by field. Allowed values: {}",
                    Self::ALLOWED_SORT_FIELDS.join(", ")
                ),
                Some("sort_by"),
            ));
        }

        Ok(())
    }

    pub fn limit(&self) -> i64 {
        self.per_page as i64
    }

    pub fn offset(&self) -> i64 {
        ((self.page.saturating_sub(1)) * self.per_page) as i64
    }

    pub fn sort_direction(&self) -> &'static str {
        self.sort_order.as_sql()
    }

    pub fn get_sort_field(&self) -> &str {
        &self.sort_by
    }

    /// Alias for per_page for backward compatibility
    pub fn page_size(&self) -> u32 {
        self.per_page
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MeterReadingsResponse {
    pub data: Vec<MeterReadingResponse>,
    pub pagination: crate::utils::PaginationMeta,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MintFromReadingRequest {
    pub reading_id: Uuid,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MintResponse {
    pub message: String,
    pub transaction_signature: String,
    #[schema(value_type = String)]
    pub kwh_amount: Decimal,
    pub wallet_address: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserStatsResponse {
    pub total_readings: i64,
    #[schema(value_type = String)]
    pub unminted_kwh: Decimal,
    #[schema(value_type = String)]
    pub minted_kwh: Decimal,
    #[schema(value_type = String)]
    pub total_kwh: Decimal,
}

// ============================================================================
// Registration Types (from meter_registration.rs)
// ============================================================================

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterMeterRequest {
    pub meter_serial: String,
    pub meter_public_key: String, // Base58 encoded Ed25519 public key
    pub meter_type: String,       // residential, commercial, solar, industrial
    pub location_address: Option<String>,
    pub manufacturer: Option<String>,
    pub installation_date: Option<chrono::NaiveDate>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RegisterMeterResponse {
    pub meter_id: Uuid,
    pub meter_serial: String,
    pub wallet_address: String,
    pub verification_status: String,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MeterInfo {
    pub id: Uuid,
    pub meter_serial: String,
    pub meter_type: Option<String>,
    pub location_address: Option<String>,
    pub verification_status: Option<String>,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Admin verification request (Renamed from VerifyMeterRequest)
#[derive(Debug, Deserialize, ToSchema)]
pub struct AdminVerifyMeterRequest {
    pub verification_status: String, // "verified" or "rejected"
    pub verification_proof: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminVerifyMeterResponse {
    pub meter_id: Uuid,
    pub verification_status: String,
    pub verified_at: chrono::DateTime<chrono::Utc>,
    pub message: String,
}

// ============================================================================
// Verification Types (from meter_verification.rs)
// ============================================================================

pub use crate::services::meter::verification::VerifyMeterResponse;
use crate::services::meter::verification::{
    MeterRegistry, VerifyMeterRequest as ServiceVerifyMeterRequest,
};

#[derive(Debug, Deserialize, ToSchema)]
pub struct VerifyMeterRequestWrapper {
    #[serde(flatten)]
    pub request: ServiceVerifyMeterRequest,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MeterRegistryResponse {
    pub id: Uuid,
    pub meter_serial: String,
    pub verification_method: String,
    pub verification_status: String,
    pub user_id: Uuid,
    pub manufacturer: Option<String>,
    pub meter_type: Option<String>,
    pub location_address: Option<String>,
    pub installation_date: Option<chrono::NaiveDate>,
    pub verification_proof: Option<String>,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub verified_by: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<MeterRegistry> for MeterRegistryResponse {
    fn from(meter: MeterRegistry) -> Self {
        Self {
            id: meter.id,
            meter_serial: meter.meter_serial,
            verification_method: meter.verification_method,
            verification_status: meter.verification_status,
            user_id: meter.user_id,
            manufacturer: meter.manufacturer,
            meter_type: meter.meter_type,
            location_address: meter.location_address,
            installation_date: meter.installation_date,
            verification_proof: meter.verification_proof,
            verified_at: meter.verified_at,
            verified_by: meter.verified_by,
            created_at: meter.created_at,
            updated_at: meter.updated_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GetMetersResponse {
    pub meters: Vec<MeterRegistryResponse>,
    pub total: i64,
}

#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct GetMetersQuery {
    /// Filter by verification status: "verified", "pending", "rejected", "suspended"
    pub status: Option<String>,
    /// Filter by meter type: "residential", "commercial", "solar", "industrial"
    pub meter_type: Option<String>,
    /// Page number (1-indexed)
    #[serde(default = "default_page")]
    pub page: u32,
    /// Number of items per page (max 100)
    #[serde(default = "default_per_page")]
    pub page_size: u32,
}

impl GetMetersQuery {
    pub fn validate(&mut self) -> Result<()> {
        if self.page < 1 {
            self.page = 1;
        }

        if self.page_size < 1 {
            self.page_size = 20;
        } else if self.page_size > 100 {
            self.page_size = 100;
        }

        // Validate status filter
        if let Some(status) = &self.status {
            match status.as_str() {
                "verified" | "pending" | "rejected" | "suspended" => {}
                _ => return Err(ApiError::validation_error(
                    "Invalid status filter. Allowed values: verified, pending, rejected, suspended",
                    Some("status"),
                )),
            }
        }

        // Validate meter type filter
        if let Some(meter_type) = &self.meter_type {
            match meter_type.as_str() {
                "residential" | "commercial" | "solar" | "industrial" => {}
                _ => return Err(ApiError::validation_error(
                    "Invalid meter_type filter. Allowed values: residential, commercial, solar, industrial",
                    Some("meter_type"),
                )),
            }
        }

        Ok(())
    }
}
