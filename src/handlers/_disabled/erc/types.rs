use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

use crate::{
    error::{ApiError, Result},
    utils::{PaginationMeta, SortOrder},
};

#[derive(Debug, Deserialize, ToSchema)]
pub struct IssueErcRequest {
    pub wallet_address: String,
    pub meter_id: Option<String>,
    #[schema(value_type = String)]
    pub kwh_amount: Decimal,
    pub expiry_date: Option<chrono::DateTime<chrono::Utc>>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErcCertificateResponse {
    pub id: Uuid,
    pub certificate_id: String,
    pub user_id: Option<Uuid>,
    pub wallet_address: String,
    #[schema(value_type = String)]
    pub kwh_amount: Option<Decimal>,
    pub issue_date: Option<chrono::DateTime<chrono::Utc>>,
    pub expiry_date: Option<chrono::DateTime<chrono::Utc>>,
    pub issuer_wallet: Option<String>,
    pub status: String,
    pub blockchain_tx_signature: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Validate, ToSchema, IntoParams)]
pub struct GetCertificatesQuery {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    pub sort_by: Option<String>,
    #[serde(default = "default_sort_order")]
    pub sort_order: SortOrder,
    pub status: Option<String>,
}

fn default_page() -> u32 {
    1
}
fn default_page_size() -> u32 {
    20
}
fn default_sort_order() -> SortOrder {
    SortOrder::Desc
}
#[allow(dead_code)]
fn default_limit() -> i64 {
    50
}

impl GetCertificatesQuery {
    pub fn validate_params(&mut self) -> Result<()> {
        if self.page < 1 {
            return Err(ApiError::validation_error(
                "page must be >= 1",
                Some("page"),
            ));
        }
        if self.page_size < 1 || self.page_size > 100 {
            return Err(ApiError::validation_error(
                "page_size must be between 1 and 100",
                Some("page_size"),
            ));
        }

        if let Some(sort_by) = &self.sort_by {
            match sort_by.as_str() {
                "issue_date" | "expiry_date" | "kwh_amount" | "status" => {}
                _ => {
                    return Err(ApiError::validation_error(
                        "sort_by must be one of: issue_date, expiry_date, kwh_amount, status",
                        Some("sort_by"),
                    ));
                }
            }
        }

        // Validate status if provided
        if let Some(status) = &self.status {
            use crate::utils::validation::Validator;
            Validator::validate_certificate_status(status)?;
        }

        Ok(())
    }

    pub fn limit(&self) -> i64 {
        self.page_size as i64
    }
    pub fn offset(&self) -> i64 {
        ((self.page - 1) * self.page_size) as i64
    }
    pub fn sort_direction(&self) -> &str {
        match self.sort_order {
            SortOrder::Asc => "ASC",
            SortOrder::Desc => "DESC",
        }
    }
    pub fn get_sort_field(&self) -> &str {
        self.sort_by.as_deref().unwrap_or("issue_date")
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CertificatesResponse {
    pub data: Vec<ErcCertificateResponse>,
    pub pagination: PaginationMeta,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CertificateStatsResponse {
    pub total_certificates: i64,
    #[schema(value_type = String)]
    pub active_kwh: Decimal,
    #[schema(value_type = String)]
    pub retired_kwh: Decimal,
    #[schema(value_type = String)]
    pub total_kwh: Decimal,
}
