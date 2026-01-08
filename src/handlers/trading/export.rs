//! Order History Export Handler
//!
//! Exports trading history in CSV format

use axum::{
    extract::{State, Query},
    response::{IntoResponse, Response},
    http::{header, StatusCode},
};
use chrono::{DateTime, Utc, NaiveDate};
use serde::Deserialize;
use tracing::{info, error};

use crate::auth::middleware::AuthenticatedUser;
use crate::database::schema::types::{OrderSide, OrderStatus, OrderType};
use crate::AppState;

/// Query params for export
#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    /// Start date (YYYY-MM-DD)
    pub start_date: Option<NaiveDate>,
    /// End date (YYYY-MM-DD)
    pub end_date: Option<NaiveDate>,
    /// Filter by side (buy/sell)
    pub side: Option<String>,
    /// Filter by status (pending/filled/cancelled)
    pub status: Option<String>,
}

/// Trade record for export
struct TradeRecord {
    date: DateTime<Utc>,
    order_id: String,
    order_type: String,
    side: String,
    energy_amount: f64,
    price_per_kwh: f64,
    total_value: f64,
    status: String,
}

/// Export trading history as CSV
/// GET /api/v1/trading/export/csv
#[utoipa::path(
    get,
    path = "/api/v1/trading/export/csv",
    tag = "trading",
    params(
        ("start_date" = Option<String>, Query, description = "Start date (YYYY-MM-DD)"),
        ("end_date" = Option<String>, Query, description = "End date (YYYY-MM-DD)"),
        ("side" = Option<String>, Query, description = "Filter by side (buy/sell)"),
        ("status" = Option<String>, Query, description = "Filter by status")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "CSV file download", content_type = "text/csv"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn export_csv(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Query(params): Query<ExportQuery>,
) -> Response {
    info!("Exporting trade history for user: {}", user.0.sub);

    // Build query with filters
    let start_date = params.start_date
        .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
        .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    
    let end_date = params.end_date
        .map(|d| d.and_hms_opt(23, 59, 59).unwrap())
        .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));

    // Query orders with filters
    let orders = match fetch_orders(&state, user.0.sub, start_date, end_date, &params.side, &params.status).await {
        Ok(orders) => orders,
        Err(e) => {
            error!("Failed to fetch orders for export: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to export orders").into_response();
        }
    };

    // Generate CSV content
    let csv = generate_csv(&orders);

    // Set filename with date
    let filename = format!(
        "gridtokenx_trades_{}.csv",
        Utc::now().format("%Y%m%d_%H%M%S")
    );

    // Return CSV response with proper headers
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (header::CONTENT_DISPOSITION, &format!("attachment; filename=\"{}\"", filename)),
        ],
        csv,
    ).into_response()
}

/// Fetch orders from database with filters
async fn fetch_orders(
    state: &AppState,
    user_id: uuid::Uuid,
    start_date: Option<DateTime<Utc>>,
    end_date: Option<DateTime<Utc>>,
    side_filter: &Option<String>,
    status_filter: &Option<String>,
) -> anyhow::Result<Vec<TradeRecord>> {
    // Base query
    let rows = sqlx::query!(
        r#"
        SELECT 
            id, order_type as "order_type!: OrderType", side as "side!: OrderSide", 
            energy_amount, price_per_kwh, filled_amount,
            status as "status!: OrderStatus", created_at as "created_at!"
        FROM trading_orders
        WHERE user_id = $1
          AND ($2::timestamptz IS NULL OR created_at >= $2)
          AND ($3::timestamptz IS NULL OR created_at <= $3)
        ORDER BY created_at DESC
        LIMIT 10000
        "#,
        user_id,
        start_date,
        end_date
    )
    .fetch_all(&state.db)
    .await?;

    // Apply additional filters and convert to TradeRecord
    let mut records = Vec::with_capacity(rows.len());
    
    for row in rows {
        let side_str = format!("{:?}", row.side).to_lowercase();
        let status_str = format!("{:?}", row.status).to_lowercase();
        
        // Apply side filter
        if let Some(ref filter) = side_filter {
            if !side_str.contains(&filter.to_lowercase()) {
                continue;
            }
        }
        
        // Apply status filter
        if let Some(ref filter) = status_filter {
            if !status_str.contains(&filter.to_lowercase()) {
                continue;
            }
        }
        
        let energy = row.energy_amount.to_string().parse::<f64>().unwrap_or(0.0);
        let price = row.price_per_kwh.to_string().parse::<f64>().unwrap_or(0.0);
        
        records.push(TradeRecord {
            date: row.created_at,
            order_id: row.id.to_string(),
            order_type: format!("{:?}", row.order_type).to_lowercase(),
            side: side_str,
            energy_amount: energy,
            price_per_kwh: price,
            total_value: energy * price,
            status: status_str,
        });
    }

    Ok(records)
}

/// Generate CSV content from trade records
fn generate_csv(records: &[TradeRecord]) -> String {
    let mut csv = String::new();
    
    // Header
    csv.push_str("Date,Order ID,Type,Side,Amount (kWh),Price (per kWh),Total Value,Status\n");
    
    // Data rows
    for record in records {
        csv.push_str(&format!(
            "{},{},{},{},{:.4},{:.6},{:.4},{}\n",
            record.date.format("%Y-%m-%d %H:%M:%S"),
            record.order_id,
            record.order_type,
            record.side,
            record.energy_amount,
            record.price_per_kwh,
            record.total_value,
            record.status
        ));
    }

    // Summary at the end
    if !records.is_empty() {
        let total_trades = records.len();
        let total_volume: f64 = records.iter().map(|r| r.energy_amount).sum();
        let total_value: f64 = records.iter().map(|r| r.total_value).sum();
        
        csv.push_str("\n");
        csv.push_str(&format!("# Summary: {} trades, {:.2} kWh total volume, {:.2} total value\n", 
            total_trades, total_volume, total_value));
    }

    csv
}

/// Export trading history as JSON (alternative format)
/// GET /api/v1/trading/export/json
pub async fn export_json(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Query(params): Query<ExportQuery>,
) -> Response {
    info!("Exporting trade history (JSON) for user: {}", user.0.sub);

    let start_date = params.start_date
        .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
        .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    
    let end_date = params.end_date
        .map(|d| d.and_hms_opt(23, 59, 59).unwrap())
        .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));

    let orders = match fetch_orders(&state, user.0.sub, start_date, end_date, &params.side, &params.status).await {
        Ok(orders) => orders,
        Err(e) => {
            error!("Failed to fetch orders for export: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to export orders").into_response();
        }
    };

    // Convert to JSON-serializable format
    let json_records: Vec<serde_json::Value> = orders.iter().map(|r| {
        serde_json::json!({
            "date": r.date.to_rfc3339(),
            "order_id": r.order_id,
            "order_type": r.order_type,
            "side": r.side,
            "energy_amount": r.energy_amount,
            "price_per_kwh": r.price_per_kwh,
            "total_value": r.total_value,
            "status": r.status
        })
    }).collect();

    let response = serde_json::json!({
        "trades": json_records,
        "count": json_records.len(),
        "exported_at": Utc::now().to_rfc3339()
    });

    let filename = format!(
        "gridtokenx_trades_{}.json",
        Utc::now().format("%Y%m%d_%H%M%S")
    );

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json; charset=utf-8"),
            (header::CONTENT_DISPOSITION, &format!("attachment; filename=\"{}\"", filename)),
        ],
        serde_json::to_string_pretty(&response).unwrap_or_default(),
    ).into_response()
}
