use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::BigDecimal;
use sqlx::Row;
use utoipa::{ToSchema, IntoParams};
use uuid::Uuid;
use validator::Validate;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::utils::{PaginationParams, PaginationMeta, SortOrder, validation::Validator};
use crate::AppState;

// ==================== REQUEST/RESPONSE TYPES ====================

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateOfferRequest {
    #[validate(range(min = 0.001))]
    pub energy_amount: f64, // kWh
    #[validate(range(min = 0.01))]
    pub price_per_kwh: f64, // USD
    pub energy_source: String, // "solar", "wind", "hydro", "mixed"
    pub available_from: Option<DateTime<Utc>>,
    pub available_until: Option<DateTime<Utc>>,
}

impl CreateOfferRequest {
    pub fn validate_business_rules(&self) -> Result<()> {
        // Validate energy source
        Validator::validate_energy_source(&self.energy_source)?;
        
        // Validate energy amount
        Validator::validate_energy_reading(self.energy_amount)?;
        
        // Validate price
        Validator::validate_price(self.price_per_kwh)?;
        
        // Validate date range
        Validator::validate_date_range(self.available_from, self.available_until)?;
        
        // Validate available_until is not in the past
        if let Some(until) = self.available_until {
            if until < Utc::now() {
                return Err(ApiError::validation_field(
                    "available_until",
                    "Expiration date cannot be in the past"
                ));
            }
        }
        
        Ok(())
    }
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateOrderRequest {
    #[validate(range(min = 0.001))]
    pub energy_amount: f64, // kWh
    #[validate(range(min = 0.01))]
    pub max_price_per_kwh: f64, // USD
    pub preferred_source: Option<String>,
}

impl CreateOrderRequest {
    pub fn validate_business_rules(&self) -> Result<()> {
        // Validate energy amount
        Validator::validate_energy_reading(self.energy_amount)?;
        
        // Validate price
        Validator::validate_price(self.max_price_per_kwh)?;
        
        // Validate energy source if provided
        if let Some(source) = &self.preferred_source {
            Validator::validate_energy_source(source)?;
        }
        
        Ok(())
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Offer {
    pub id: Uuid,
    pub seller_id: Uuid,
    pub seller_username: Option<String>,
    pub energy_amount: f64,
    pub price_per_kwh: f64,
    pub energy_source: String,
    pub status: String,
    pub available_from: DateTime<Utc>,
    pub available_until: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Order {
    pub id: Uuid,
    pub buyer_id: Uuid,
    pub energy_amount: f64,
    pub max_price_per_kwh: f64,
    pub preferred_source: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Transaction {
    pub id: Uuid,
    pub offer_id: Uuid,
    pub order_id: Uuid,
    pub seller_id: Uuid,
    pub buyer_id: Uuid,
    pub energy_amount: f64,
    pub price_per_kwh: f64,
    pub total_price: f64,
    pub status: String,
    pub blockchain_tx_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub settled_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MarketStats {
    pub average_price: f64,
    pub total_volume: f64,
    pub active_offers: i64,
    pub active_orders: i64,
    pub completed_transactions: i64,
}

#[derive(Debug, Deserialize, Validate, IntoParams)]
pub struct OfferQuery {
    pub energy_source: Option<String>,
    pub min_price: Option<f64>,
    pub max_price: Option<f64>,
    pub min_amount: Option<f64>,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    pub sort_by: Option<String>,
    #[serde(default = "default_sort_order")]
    pub sort_order: SortOrder,
}

fn default_page() -> u32 { 1 }
fn default_page_size() -> u32 { 20 }
fn default_sort_order() -> SortOrder { SortOrder::Desc }

impl OfferQuery {
    pub fn validate_params(&mut self) -> Result<()> {
        if self.page < 1 {
            return Err(ApiError::validation_error("page must be >= 1", Some("page")));
        }
        if self.page_size < 1 || self.page_size > 100 {
            return Err(ApiError::validation_error("page_size must be between 1 and 100", Some("page_size")));
        }
        
        // Validate sort_by field
        if let Some(sort_by) = &self.sort_by {
            match sort_by.as_str() {
                "price_per_kwh" | "energy_amount" | "created_at" | "available_until" => {},
                _ => return Err(ApiError::validation_error(
                    "sort_by must be one of: price_per_kwh, energy_amount, created_at, available_until",
                    Some("sort_by")
                )),
            }
        }
        
        // Validate energy source if provided
        if let Some(source) = &self.energy_source {
            Validator::validate_energy_source(source)?;
        }
        
        // Validate price ranges
        if let Some(min_price) = self.min_price {
            Validator::validate_price(min_price)?;
        }
        if let Some(max_price) = self.max_price {
            Validator::validate_price(max_price)?;
        }
        
        // Validate price range logic
        if let (Some(min), Some(max)) = (self.min_price, self.max_price) {
            if min > max {
                return Err(ApiError::validation_error(
                    "min_price must be less than or equal to max_price",
                    None
                ));
            }
        }
        
        // Validate amount
        if let Some(min_amount) = self.min_amount {
            Validator::validate_amount(min_amount, "min_amount")?;
        }
        
        Ok(())
    }
    
    pub fn limit(&self) -> i64 { self.page_size as i64 }
    pub fn offset(&self) -> i64 { ((self.page - 1) * self.page_size) as i64 }
    pub fn sort_direction(&self) -> &str {
        match self.sort_order {
            SortOrder::Asc => "ASC",
            SortOrder::Desc => "DESC",
        }
    }
    pub fn get_sort_field(&self) -> &str {
        self.sort_by.as_deref().unwrap_or("price_per_kwh")
    }
}

#[derive(Debug, Deserialize, Validate, IntoParams)]
pub struct OrderListQuery {
    pub status: Option<String>,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    pub sort_by: Option<String>,
    #[serde(default = "default_sort_order")]
    pub sort_order: SortOrder,
}

impl OrderListQuery {
    pub fn validate_params(&mut self) -> Result<()> {
        if self.page < 1 {
            return Err(ApiError::validation_error("page must be >= 1", Some("page")));
        }
        if self.page_size < 1 || self.page_size > 100 {
            return Err(ApiError::validation_error("page_size must be between 1 and 100", Some("page_size")));
        }
        
        if let Some(sort_by) = &self.sort_by {
            match sort_by.as_str() {
                "created_at" | "energy_amount" | "max_price_per_kwh" => {},
                _ => return Err(ApiError::validation_error(
                    "sort_by must be one of: created_at, energy_amount, max_price_per_kwh",
                    Some("sort_by")
                )),
            }
        }
        
        // Validate status if provided
        if let Some(status) = &self.status {
            Validator::validate_order_status(status)?;
        }
        
        Ok(())
    }
    
    pub fn limit(&self) -> i64 { self.page_size as i64 }
    pub fn offset(&self) -> i64 { ((self.page - 1) * self.page_size) as i64 }
    pub fn sort_direction(&self) -> &str {
        match self.sort_order {
            SortOrder::Asc => "ASC",
            SortOrder::Desc => "DESC",
        }
    }
    pub fn get_sort_field(&self) -> &str {
        self.sort_by.as_deref().unwrap_or("created_at")
    }
}

#[derive(Debug, Deserialize, Validate, IntoParams)]
pub struct TransactionListQuery {
    pub status: Option<String>,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
    pub sort_by: Option<String>,
    #[serde(default = "default_sort_order")]
    pub sort_order: SortOrder,
}

impl TransactionListQuery {
    pub fn validate_params(&mut self) -> Result<()> {
        if self.page < 1 {
            return Err(ApiError::validation_error("page must be >= 1", Some("page")));
        }
        if self.page_size < 1 || self.page_size > 100 {
            return Err(ApiError::validation_error("page_size must be between 1 and 100", Some("page_size")));
        }
        
        if let Some(sort_by) = &self.sort_by {
            match sort_by.as_str() {
                "created_at" | "settled_at" | "energy_amount" | "total_price" => {},
                _ => return Err(ApiError::validation_error(
                    "sort_by must be one of: created_at, settled_at, energy_amount, total_price",
                    Some("sort_by")
                )),
            }
        }
        
        // Validate status if provided
        if let Some(status) = &self.status {
            Validator::validate_transaction_status(status)?;
        }
        
        Ok(())
    }
    
    pub fn limit(&self) -> i64 { self.page_size as i64 }
    pub fn offset(&self) -> i64 { ((self.page - 1) * self.page_size) as i64 }
    pub fn sort_direction(&self) -> &str {
        match self.sort_order {
            SortOrder::Asc => "ASC",
            SortOrder::Desc => "DESC",
        }
    }
    pub fn get_sort_field(&self) -> &str {
        self.sort_by.as_deref().unwrap_or("created_at")
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OffersResponse {
    pub data: Vec<Offer>,
    pub pagination: PaginationMeta,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrdersResponse {
    pub data: Vec<Order>,
    pub pagination: PaginationMeta,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TransactionsResponse {
    pub data: Vec<Transaction>,
    pub pagination: PaginationMeta,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct OfferFilters {
    pub energy_source: Option<String>,
    pub min_price: Option<f64>,
    pub max_price: Option<f64>,
    pub min_amount: Option<f64>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct PriceHistoryQuery {
    pub hours: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PricePoint {
    pub timestamp: DateTime<Utc>,
    pub average_price: f64,
    pub volume: f64,
}

// ==================== OFFER ENDPOINTS ====================

/// Create a new energy offer
#[utoipa::path(
    post,
    path = "/api/offers",
    tag = "energy-trading",
    request_body = CreateOfferRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Offer created successfully", body = Offer),
        (status = 400, description = "Invalid offer data"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - Authentication required"),
    )
)]
pub async fn create_offer(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<CreateOfferRequest>,
) -> Result<Json<Offer>> {
    // Validate struct constraints
    payload.validate()
        .map_err(|e| ApiError::BadRequest(format!("Validation error: {}", e)))?;
    
    // Validate business rules
    payload.validate_business_rules()?;
    
    // All verified users can create energy offers
    let now = Utc::now();
    let available_from = payload.available_from.unwrap_or(now);
    let available_until = payload
        .available_until
        .unwrap_or_else(|| now + chrono::Duration::days(7));

    // Convert to BigDecimal for database
    let energy_amount_bd = BigDecimal::from_str(&payload.energy_amount.to_string())
        .map_err(|_| ApiError::BadRequest("Invalid energy amount".to_string()))?;
    let price_bd = BigDecimal::from_str(&payload.price_per_kwh.to_string())
        .map_err(|_| ApiError::BadRequest("Invalid price".to_string()))?;

    let offer_id = Uuid::new_v4();

    sqlx::query(
        r#"
        INSERT INTO offers (
            id, created_by, energy_amount, price_per_kwh, energy_source,
            status, available_from, available_until, created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, 'Active', $6, $7, $8, $8)
        "#,
    )
    .bind(offer_id)
    .bind(user.0.sub)
    .bind(energy_amount_bd)
    .bind(price_bd)
    .bind(&payload.energy_source)
    .bind(available_from)
    .bind(available_until)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Database(e))?;

    // Trigger order matching engine to immediately check for matches
    tokio::spawn({
        let engine = state.order_matching_engine.clone();
        async move {
            if let Err(e) = engine.trigger_matching().await {
                tracing::error!("Failed to trigger matching after offer creation: {}", e);
            }
        }
    });

    // Broadcast WebSocket event for new offer
    tokio::spawn({
        let ws_service = state.websocket_service.clone();
        let offer_id_str = offer_id.to_string();
        let energy_amount = payload.energy_amount;
        let price = payload.price_per_kwh;
        let energy_source = payload.energy_source.clone();
        let user_id = user.0.sub.to_string();
        async move {
            ws_service
                .broadcast_offer_created(
                    offer_id_str,
                    energy_amount,
                    price,
                    energy_source,
                    "".to_string(), // location not stored in current schema
                    user_id,
                )
                .await;
        }
    });

    Ok(Json(Offer {
        id: offer_id,
        seller_id: user.0.sub,
        seller_username: Some(user.0.username.clone()),
        energy_amount: payload.energy_amount,
        price_per_kwh: payload.price_per_kwh,
        energy_source: payload.energy_source,
        status: "Active".to_string(),
        available_from,
        available_until,
        created_at: now,
    }))
}

/// List all active offers with pagination and filters
#[utoipa::path(
    get,
    path = "/api/offers",
    tag = "energy-trading",
    params(OfferQuery),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of active offers", body = OffersResponse),
        (status = 401, description = "Unauthorized"),
    )
)]
pub async fn list_offers(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Query(mut params): Query<OfferQuery>,
) -> Result<Json<OffersResponse>> {
    // Validate parameters
    params.validate_params()?;
    
    let limit = params.limit();
    let offset = params.offset();
    let sort_field = params.get_sort_field();
    let sort_direction = params.sort_direction();

    // Build WHERE conditions
    let mut where_conditions = vec!["o.status = 'Active'".to_string(), "o.available_until > NOW()".to_string()];
    let mut bind_count = 1;

    if params.energy_source.is_some() {
        where_conditions.push(format!("o.energy_source = ${}", bind_count));
        bind_count += 1;
    }
    if params.min_price.is_some() {
        where_conditions.push(format!("o.price_per_kwh >= ${}", bind_count));
        bind_count += 1;
    }
    if params.max_price.is_some() {
        where_conditions.push(format!("o.price_per_kwh <= ${}", bind_count));
        bind_count += 1;
    }
    if params.min_amount.is_some() {
        where_conditions.push(format!("o.energy_amount >= ${}", bind_count));
        bind_count += 1;
    }

    let where_clause = where_conditions.join(" AND ");

    // Count total
    let count_query = format!("SELECT COUNT(*) FROM offers o WHERE {}", where_clause);
    let mut count_sqlx = sqlx::query_scalar::<_, i64>(&count_query);
    
    if let Some(source) = &params.energy_source {
        count_sqlx = count_sqlx.bind(source);
    }
    if let Some(min_price) = params.min_price {
        count_sqlx = count_sqlx.bind(min_price);
    }
    if let Some(max_price) = params.max_price {
        count_sqlx = count_sqlx.bind(max_price);
    }
    if let Some(min_amount) = params.min_amount {
        count_sqlx = count_sqlx.bind(min_amount);
    }
    
    let total = count_sqlx
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to count offers: {}", e);
            ApiError::Database(e)
        })?;

    // Build data query
    let data_query = format!(
        r#"
        SELECT o.id, o.created_by as seller_id, u.username as seller_username,
               o.energy_amount, o.price_per_kwh, o.energy_source,
               o.status, o.available_from, o.available_until, o.created_at
        FROM offers o
        JOIN users u ON o.created_by = u.id
        WHERE {}
        ORDER BY o.{} {}
        LIMIT ${} OFFSET ${}
        "#,
        where_clause, sort_field, sort_direction, bind_count, bind_count + 1
    );

    let mut data_sqlx = sqlx::query(&data_query);
    
    if let Some(source) = &params.energy_source {
        data_sqlx = data_sqlx.bind(source);
    }
    if let Some(min_price) = params.min_price {
        data_sqlx = data_sqlx.bind(min_price);
    }
    if let Some(max_price) = params.max_price {
        data_sqlx = data_sqlx.bind(max_price);
    }
    if let Some(min_amount) = params.min_amount {
        data_sqlx = data_sqlx.bind(min_amount);
    }
    
    data_sqlx = data_sqlx.bind(limit);
    data_sqlx = data_sqlx.bind(offset);

    let rows = data_sqlx
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch offers: {}", e);
            ApiError::Database(e)
        })?;

    let offers = rows
        .iter()
        .map(|row| {
            let energy_amount: BigDecimal = row.get("energy_amount");
            let price_per_kwh: BigDecimal = row.get("price_per_kwh");

            Offer {
                id: row.get("id"),
                seller_id: row.get("seller_id"),
                seller_username: row.get("seller_username"),
                energy_amount: energy_amount.to_string().parse().unwrap_or(0.0),
                price_per_kwh: price_per_kwh.to_string().parse().unwrap_or(0.0),
                energy_source: row.get("energy_source"),
                status: row.get("status"),
                available_from: row.get("available_from"),
                available_until: row.get("available_until"),
                created_at: row.get("created_at"),
            }
        })
        .collect();

    // Create pagination metadata
    let pagination = PaginationMeta::new(
        &PaginationParams {
            page: params.page,
            page_size: params.page_size,
            sort_by: params.sort_by.clone(),
            sort_order: params.sort_order,
        },
        total,
    );

    Ok(Json(OffersResponse {
        data: offers,
        pagination,
    }))
}

/// Get specific offer details
#[utoipa::path(
    get,
    path = "/api/offers/{id}",
    tag = "energy-trading",
    params(
        ("id" = Uuid, Path, description = "Offer ID")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Offer details", body = Offer),
        (status = 404, description = "Offer not found"),
    )
)]
pub async fn get_offer(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Offer>> {
    let row = sqlx::query(
        r#"
        SELECT o.id, o.created_by as seller_id, u.username as seller_username,
               o.energy_amount, o.price_per_kwh, o.energy_source,
               o.status, o.available_from, o.available_until, o.created_at
        FROM offers o
        JOIN users u ON o.created_by = u.id
        WHERE o.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Database(e))?
    .ok_or_else(|| ApiError::NotFound("Offer not found".to_string()))?;

    let energy_amount: BigDecimal = row.get("energy_amount");
    let price_per_kwh: BigDecimal = row.get("price_per_kwh");

    Ok(Json(Offer {
        id: row.get("id"),
        seller_id: row.get("seller_id"),
        seller_username: row.get("seller_username"),
        energy_amount: energy_amount.to_string().parse().unwrap_or(0.0),
        price_per_kwh: price_per_kwh.to_string().parse().unwrap_or(0.0),
        energy_source: row.get("energy_source"),
        status: row.get("status"),
        available_from: row.get("available_from"),
        available_until: row.get("available_until"),
        created_at: row.get("created_at"),
    }))
}

/// Cancel an offer (Owner only)
#[utoipa::path(
    patch,
    path = "/api/offers/{id}",
    tag = "energy-trading",
    params(
        ("id" = Uuid, Path, description = "Offer ID")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Offer cancelled successfully"),
        (status = 403, description = "Not the offer owner"),
        (status = 404, description = "Offer not found"),
    )
)]
pub async fn cancel_offer(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    // Verify ownership
    let offer_row = sqlx::query("SELECT created_by FROM offers WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiError::Database(e))?
        .ok_or_else(|| ApiError::NotFound("Offer not found".to_string()))?;

    let created_by: Uuid = offer_row.try_get("created_by").map_err(|e| ApiError::Internal(format!("Failed to get created_by: {}", e)))?;

    if created_by != user.0.sub {
        return Err(ApiError::Forbidden(
            "You can only cancel your own offers".to_string(),
        ));
    }

    sqlx::query("UPDATE offers SET status = 'Cancelled', updated_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiError::Database(e))?;

    Ok(Json(serde_json::json!({
        "message": "Offer cancelled successfully",
        "id": id
    })))
}

// ==================== ORDER ENDPOINTS ====================

/// Create a new energy order
#[utoipa::path(
    post,
    path = "/api/orders",
    tag = "energy-trading",
    request_body = CreateOrderRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Order created successfully", body = Order),
        (status = 400, description = "Invalid order data"),
        (status = 403, description = "Forbidden - Authentication required"),
    )
)]
pub async fn create_order(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<CreateOrderRequest>,
) -> Result<Json<Order>> {
    // Validate struct constraints
    payload.validate()
        .map_err(|e| ApiError::BadRequest(format!("Validation error: {}", e)))?;
    
    // Validate business rules
    payload.validate_business_rules()?;
    
    // All verified users can create energy orders
    let now = Utc::now();
    let order_id = Uuid::new_v4();

    let energy_amount_bd = BigDecimal::from_str(&payload.energy_amount.to_string())
        .map_err(|_| ApiError::BadRequest("Invalid energy amount".to_string()))?;
    let max_price_bd = BigDecimal::from_str(&payload.max_price_per_kwh.to_string())
        .map_err(|_| ApiError::BadRequest("Invalid price".to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO orders (
            id, created_by, energy_amount, max_price_per_kwh, preferred_source,
            status, created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, 'Pending', $6, $6)
        "#,
    )
    .bind(order_id)
    .bind(user.0.sub)
    .bind(energy_amount_bd)
    .bind(max_price_bd)
    .bind(&payload.preferred_source)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Database(e))?;

    // Trigger order matching engine to immediately check for matches
    tokio::spawn({
        let engine = state.order_matching_engine.clone();
        async move {
            if let Err(e) = engine.trigger_matching().await {
                tracing::error!("Failed to trigger matching after order creation: {}", e);
            }
        }
    });

    // Broadcast WebSocket event for new order
    tokio::spawn({
        let ws_service = state.websocket_service.clone();
        let order_id_str = order_id.to_string();
        let energy_amount = payload.energy_amount;
        let max_price = payload.max_price_per_kwh;
        let preferred_source = payload.preferred_source.clone();
        let user_id = user.0.sub.to_string();
        async move {
            ws_service
                .broadcast_order_created(
                    order_id_str,
                    energy_amount,
                    max_price,
                    preferred_source,
                    user_id,
                )
                .await;
        }
    });

    Ok(Json(Order {
        id: order_id,
        buyer_id: user.0.sub,
        energy_amount: payload.energy_amount,
        max_price_per_kwh: payload.max_price_per_kwh,
        preferred_source: payload.preferred_source,
        status: "Pending".to_string(),
        created_at: now,
    }))
}

/// List user's orders with pagination
#[utoipa::path(
    get,
    path = "/api/orders",
    tag = "energy-trading",
    params(OrderListQuery),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of user's orders", body = OrdersResponse),
    )
)]
pub async fn list_orders(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Query(mut params): Query<OrderListQuery>,
) -> Result<Json<OrdersResponse>> {
    // Validate parameters
    params.validate_params()?;
    
    let limit = params.limit();
    let offset = params.offset();
    let sort_field = params.get_sort_field();
    let sort_direction = params.sort_direction();

    // Build WHERE conditions
    let mut where_conditions = vec!["created_by = $1".to_string()];
    let mut bind_count = 2;

    if params.status.is_some() {
        where_conditions.push(format!("status = ${}", bind_count));
        bind_count += 1;
    }

    let where_clause = where_conditions.join(" AND ");

    // Count total
    let count_query = format!("SELECT COUNT(*) FROM trading_orders WHERE {}", where_clause);
    let mut count_sqlx = sqlx::query_scalar::<_, i64>(&count_query);
    count_sqlx = count_sqlx.bind(user.0.sub);
    
    if let Some(status) = &params.status {
        count_sqlx = count_sqlx.bind(status);
    }
    
    let total = count_sqlx
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to count orders: {}", e);
            ApiError::Database(e)
        })?;

    // Build data query
    let data_query = format!(
        r#"
        SELECT id, created_by as buyer_id, energy_amount, max_price_per_kwh,
               preferred_source, status, created_at
        FROM trading_orders
        WHERE {}
        ORDER BY {} {}
        LIMIT ${} OFFSET ${}
        "#,
        where_clause, sort_field, sort_direction, bind_count, bind_count + 1
    );

    let mut data_sqlx = sqlx::query(&data_query);
    data_sqlx = data_sqlx.bind(user.0.sub);
    
    if let Some(status) = &params.status {
        data_sqlx = data_sqlx.bind(status);
    }
    
    data_sqlx = data_sqlx.bind(limit);
    data_sqlx = data_sqlx.bind(offset);

    let rows = data_sqlx
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch orders: {}", e);
            ApiError::Database(e)
        })?;

    let orders = rows
        .iter()
        .map(|row| {
            let energy_amount: BigDecimal = row.get("energy_amount");
            let max_price: BigDecimal = row.get("max_price_per_kwh");

            Order {
                id: row.get("id"),
                buyer_id: row.get("buyer_id"),
                energy_amount: energy_amount.to_string().parse().unwrap_or(0.0),
                max_price_per_kwh: max_price.to_string().parse().unwrap_or(0.0),
                preferred_source: row.get("preferred_source"),
                status: row.get("status"),
                created_at: row.get("created_at"),
            }
        })
        .collect();

    // Create pagination metadata
    let pagination = PaginationMeta::new(
        &PaginationParams {
            page: params.page,
            page_size: params.page_size,
            sort_by: params.sort_by.clone(),
            sort_order: params.sort_order,
        },
        total,
    );

    Ok(Json(OrdersResponse {
        data: orders,
        pagination,
    }))
}

// ==================== TRANSACTION ENDPOINTS ====================

/// List user's transactions with pagination
#[utoipa::path(
    get,
    path = "/api/transactions",
    tag = "energy-trading",
    params(TransactionListQuery),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of user's transactions", body = TransactionsResponse),
    )
)]
pub async fn list_transactions(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Query(mut params): Query<TransactionListQuery>,
) -> Result<Json<TransactionsResponse>> {
    // Validate parameters
    params.validate_params()?;
    
    let limit = params.limit();
    let offset = params.offset();
    let sort_field = params.get_sort_field();
    let sort_direction = params.sort_direction();

    // Build WHERE conditions
    let mut where_conditions = vec!["(seller_id = $1 OR buyer_id = $1)".to_string()];
    let mut bind_count = 2;

    if params.status.is_some() {
        where_conditions.push(format!("status = ${}", bind_count));
        bind_count += 1;
    }

    let where_clause = where_conditions.join(" AND ");

    // Count total
    let count_query = format!("SELECT COUNT(*) FROM transactions WHERE {}", where_clause);
    let mut count_sqlx = sqlx::query_scalar::<_, i64>(&count_query);
    count_sqlx = count_sqlx.bind(user.0.sub);
    
    if let Some(status) = &params.status {
        count_sqlx = count_sqlx.bind(status);
    }
    
    let total = count_sqlx
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to count transactions: {}", e);
            ApiError::Database(e)
        })?;

    // Build data query
    let data_query = format!(
        r#"
        SELECT id, offer_id, order_id, seller_id, buyer_id,
               energy_amount, price_per_kwh, total_price,
               status, blockchain_tx_hash, created_at, settled_at
        FROM transactions
        WHERE {}
        ORDER BY {} {}
        LIMIT ${} OFFSET ${}
        "#,
        where_clause, sort_field, sort_direction, bind_count, bind_count + 1
    );

    let mut data_sqlx = sqlx::query(&data_query);
    data_sqlx = data_sqlx.bind(user.0.sub);
    
    if let Some(status) = &params.status {
        data_sqlx = data_sqlx.bind(status);
    }
    
    data_sqlx = data_sqlx.bind(limit);
    data_sqlx = data_sqlx.bind(offset);

    let rows = data_sqlx
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch transactions: {}", e);
            ApiError::Database(e)
        })?;

    let transactions = rows
        .iter()
        .map(|row| {
            let energy_amount: BigDecimal = row.get("energy_amount");
            let price_per_kwh: BigDecimal = row.get("price_per_kwh");
            let total_price: BigDecimal = row.get("total_price");

            Transaction {
                id: row.get("id"),
                offer_id: row.get("offer_id"),
                order_id: row.get("order_id"),
                seller_id: row.get("seller_id"),
                buyer_id: row.get("buyer_id"),
                energy_amount: energy_amount.to_string().parse().unwrap_or(0.0),
                price_per_kwh: price_per_kwh.to_string().parse().unwrap_or(0.0),
                total_price: total_price.to_string().parse().unwrap_or(0.0),
                status: row.get("status"),
                blockchain_tx_hash: row.get("blockchain_tx_hash"),
                created_at: row.get("created_at"),
                settled_at: row.get("settled_at"),
            }
        })
        .collect();

    // Create pagination metadata
    let pagination = PaginationMeta::new(
        &PaginationParams {
            page: params.page,
            page_size: params.page_size,
            sort_by: params.sort_by.clone(),
            sort_order: params.sort_order,
        },
        total,
    );

    Ok(Json(TransactionsResponse {
        data: transactions,
        pagination,
    }))
}

/// Get specific transaction details
#[utoipa::path(
    get,
    path = "/api/transactions/{id}",
    tag = "energy-trading",
    params(
        ("id" = Uuid, Path, description = "Transaction ID")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Transaction details", body = Transaction),
        (status = 403, description = "Not authorized to view this transaction"),
        (status = 404, description = "Transaction not found"),
    )
)]
pub async fn get_transaction(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Transaction>> {
    let row = sqlx::query(
        r#"
        SELECT id, offer_id, order_id, seller_id, buyer_id,
               energy_amount, price_per_kwh, total_price,
               status, blockchain_tx_hash, created_at, settled_at
        FROM transactions
        WHERE id = $1 AND (seller_id = $2 OR buyer_id = $2)
        "#,
    )
    .bind(id)
    .bind(user.0.sub)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Database(e))?
    .ok_or_else(|| ApiError::NotFound("Transaction not found or access denied".to_string()))?;

    let energy_amount: BigDecimal = row.get("energy_amount");
    let price_per_kwh: BigDecimal = row.get("price_per_kwh");
    let total_price: BigDecimal = row.get("total_price");

    Ok(Json(Transaction {
        id: row.get("id"),
        offer_id: row.get("offer_id"),
        order_id: row.get("order_id"),
        seller_id: row.get("seller_id"),
        buyer_id: row.get("buyer_id"),
        energy_amount: energy_amount.to_string().parse().unwrap_or(0.0),
        price_per_kwh: price_per_kwh.to_string().parse().unwrap_or(0.0),
        total_price: total_price.to_string().parse().unwrap_or(0.0),
        status: row.get("status"),
        blockchain_tx_hash: row.get("blockchain_tx_hash"),
        created_at: row.get("created_at"),
        settled_at: row.get("settled_at"),
    }))
}

// ==================== MARKET ENDPOINTS ====================

/// Get market statistics
#[utoipa::path(
    get,
    path = "/api/market/stats",
    tag = "energy-trading",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Market statistics", body = MarketStats),
    )
)]
pub async fn get_market_stats(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<MarketStats>> {
    let stats_row = sqlx::query(
        r#"
        SELECT
            COALESCE(AVG(price_per_kwh), 0) as avg_price,
            COALESCE(SUM(energy_amount), 0) as total_volume,
            COUNT(DISTINCT CASE WHEN status = 'Completed' THEN id END) as completed_tx
        FROM transactions
        WHERE created_at > NOW() - INTERVAL '24 hours'
        "#
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Database(e))?;

    let avg_price: BigDecimal = stats_row.try_get("avg_price").map_err(|e| ApiError::Internal(format!("Failed to get avg_price: {}", e)))?;
    let total_volume: BigDecimal = stats_row.try_get("total_volume").map_err(|e| ApiError::Internal(format!("Failed to get total_volume: {}", e)))?;
    let completed_tx: i64 = stats_row.try_get("completed_tx").map_err(|e| ApiError::Internal(format!("Failed to get completed_tx: {}", e)))?;

    let active_offers_row = sqlx::query("SELECT COUNT(*) as count FROM offers WHERE status = 'Active'")
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiError::Database(e))?;
    let active_offers: i64 = active_offers_row.try_get("count").map_err(|e| ApiError::Internal(format!("Failed to get count: {}", e)))?;

    let active_orders_row = sqlx::query("SELECT COUNT(*) as count FROM trading_orders WHERE status = 'pending'")
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiError::Database(e))?;
    let active_orders: i64 = active_orders_row.try_get("count").map_err(|e| ApiError::Internal(format!("Failed to get count: {}", e)))?;

    Ok(Json(MarketStats {
        average_price: avg_price.to_string().parse().unwrap_or(0.0),
        total_volume: total_volume.to_string().parse().unwrap_or(0.0),
        active_offers,
        active_orders,
        completed_transactions: completed_tx,
    }))
}

/// Get price history
#[utoipa::path(
    get,
    path = "/api/market/price-history",
    tag = "energy-trading",
    params(PriceHistoryQuery),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Historical price data", body = Vec<PricePoint>),
    )
)]
pub async fn get_price_history(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Query(query): Query<PriceHistoryQuery>,
) -> Result<Json<Vec<PricePoint>>> {
    let hours = query.hours.unwrap_or(24);

    let rows = sqlx::query(
        r#"
        SELECT
            date_trunc('hour', created_at) as hour,
            AVG(price_per_kwh) as avg_price,
            SUM(energy_amount) as volume
        FROM transactions
        WHERE created_at > NOW() - ($1 || ' hours')::INTERVAL
        GROUP BY hour
        ORDER BY hour ASC
        "#,
    )
    .bind(hours)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Database(e))?;

    let price_points = rows
        .iter()
        .map(|row| {
            let avg_price: Option<BigDecimal> = row.get("avg_price");
            let volume: Option<BigDecimal> = row.get("volume");

            PricePoint {
                timestamp: row.get("hour"),
                average_price: avg_price
                    .map(|p| p.to_string().parse().unwrap_or(0.0))
                    .unwrap_or(0.0),
                volume: volume
                    .map(|v| v.to_string().parse().unwrap_or(0.0))
                    .unwrap_or(0.0),
            }
        })
        .collect();

    Ok(Json(price_points))
}

// Helper to parse BigDecimal from string
use std::str::FromStr;
