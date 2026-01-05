use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use tracing::instrument;

use crate::{
    error::{ApiError, Result},
    services::market_clearing::revenue::{PlatformRevenueSummary, RevenueRecord},
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Get platform revenue summary
///
/// GET /api/v1/trading/revenue/summary
#[instrument(skip(state))]
pub async fn get_revenue_summary(
    State(state): State<AppState>,
) -> Result<Json<PlatformRevenueSummary>> {
    let summary = state.market_clearing.get_platform_revenue_summary().await.map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(summary))
}

/// Get detailed platform revenue records
///
/// GET /api/v1/trading/revenue/records
#[instrument(skip(state))]
pub async fn get_revenue_records(
    State(state): State<AppState>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<Vec<RevenueRecord>>> {
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    
    let records = state.market_clearing.get_revenue_records(limit, offset).await.map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(records))
}
