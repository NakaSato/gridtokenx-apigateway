// Admin endpoints for market monitoring and control
// Requires admin authentication

use axum::{extract::State, response::Json};
use chrono::Utc;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::str::FromStr;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, ErrorResponse};
use crate::services::event_processor_service::ReplayStatus;
use crate::services::wallet_initialization_service::{
    WalletDiagnosis, WalletFixResult, WalletInitializationReport, WalletInitializationService,
};
use crate::AppState;

/// Market health status
#[derive(Debug, Serialize, ToSchema)]
pub struct MarketHealth {
    pub status: String,
    pub order_book_health: OrderBookHealth,
    pub matching_stats: MatchingStatistics,
    pub settlement_stats: SettlementStatistics,
    pub websocket_connections: usize,
    pub timestamp: String,
}

/// Order book health metrics
#[derive(Debug, Serialize, ToSchema)]
pub struct OrderBookHealth {
    pub best_bid: Option<String>,
    pub best_ask: Option<String>,
    pub spread: Option<String>,
    pub spread_percentage: Option<f64>,
    pub buy_orders_count: usize,
    pub sell_orders_count: usize,
    pub total_buy_volume: String,
    pub total_sell_volume: String,
    pub liquidity_score: f64,
}

/// Matching statistics
#[derive(Debug, Serialize, ToSchema)]
pub struct MatchingStatistics {
    pub total_matches_24h: i64,
    pub total_volume_24h: String,
    pub average_price_24h: String,
    pub last_match_time: Option<String>,
    pub pending_orders: i64,
}

/// Settlement statistics
#[derive(Debug, Serialize, ToSchema)]
pub struct SettlementStatistics {
    pub pending_count: i64,
    pub processing_count: i64,
    pub confirmed_count: i64,
    pub failed_count: i64,
    pub total_settled_value: String,
}

/// Trading analytics
#[derive(Debug, Serialize, ToSchema)]
pub struct TradingAnalytics {
    pub total_trades: i64,
    pub total_volume: String,
    pub total_value: String,
    pub average_trade_size: String,
    pub price_statistics: PriceStatistics,
    pub top_traders: Vec<TraderStats>,
    pub hourly_volume: Vec<HourlyVolume>,
}

/// Price statistics
#[derive(Debug, Serialize, ToSchema)]
pub struct PriceStatistics {
    pub current_price: Option<String>,
    pub high_24h: Option<String>,
    pub low_24h: Option<String>,
    pub open_24h: Option<String>,
    pub close_24h: Option<String>,
    pub change_24h: Option<String>,
    pub change_percentage_24h: Option<f64>,
}

/// Trader statistics
#[derive(Debug, Serialize, ToSchema)]
pub struct TraderStats {
    pub user_id: String,
    pub total_trades: i64,
    pub total_volume: String,
    pub buy_volume: String,
    pub sell_volume: String,
}

/// Hourly volume data
#[derive(Debug, Serialize, ToSchema)]
pub struct HourlyVolume {
    pub hour: String,
    pub volume: String,
    pub trade_count: i64,
}

/// Market control request
#[derive(Debug, Deserialize, ToSchema)]
pub struct MarketControlRequest {
    pub action: MarketAction,
}

/// Market actions
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarketAction {
    PauseTrading,
    ResumeTrading,
    ClearOrderBook,
    TriggerMatching,
}

/// Market control response
#[derive(Debug, Serialize, ToSchema)]
pub struct MarketControlResponse {
    pub success: bool,
    pub message: String,
    pub timestamp: String,
}

/// Get comprehensive market health status
#[utoipa::path(
    get,
    path = "/api/admin/market/health",
    responses(
        (status = 200, description = "Market health retrieved", body = MarketHealth),
        (status = 403, description = "Admin access required"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Admin - Market",
    security(("bearer_auth" = []))
)]
pub async fn get_market_health(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<MarketHealth>, ApiError> {
    // Get order book snapshot
    let order_book = state.market_clearing_engine.get_order_book_snapshot().await;

    // Calculate liquidity score (0-100)
    let total_buy: rust_decimal::Decimal = order_book.buy_depth.iter().map(|(_, v)| v).sum();
    let total_sell: rust_decimal::Decimal = order_book.sell_depth.iter().map(|(_, v)| v).sum();
    let total_liquidity = total_buy + total_sell;
    let liquidity_score = if total_liquidity > rust_decimal::Decimal::ZERO {
        ((total_liquidity.to_string().parse::<f64>().unwrap_or(0.0) / 1000.0) * 100.0).min(100.0)
    } else {
        0.0
    };

    let spread_percentage = match (&order_book.best_bid, &order_book.best_ask) {
        (Some(bid), Some(ask)) if *bid > rust_decimal::Decimal::ZERO => Some(
            ((*ask - *bid) / *bid * rust_decimal::Decimal::from(100))
                .to_string()
                .parse::<f64>()
                .unwrap_or(0.0),
        ),
        _ => None,
    };

    // Get matching statistics
    let matching_stats_row = sqlx::query(
        r#"
        SELECT 
            COUNT(*) as total_matches,
            COALESCE(SUM(quantity::numeric), 0) as total_volume,
            COALESCE(AVG(price::numeric), 0) as average_price,
            MAX(executed_at) as last_match_time
        FROM trades
        WHERE executed_at > NOW() - INTERVAL '24 hours'
        "#,
    )
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let pending_orders = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*) 
        FROM trading_orders 
        WHERE status IN ('Pending', 'PartiallyFilled')
        "#,
    )
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::Database)?;

    // Get settlement statistics (if settlement service exists)
    let settlement_stats = SettlementStatistics {
        pending_count: 0,
        processing_count: 0,
        confirmed_count: 0,
        failed_count: 0,
        total_settled_value: "0".to_string(),
    };

    // Get WebSocket connections
    let ws_connections = state.websocket_service.client_count().await;

    // Determine overall status
    let status = if liquidity_score > 50.0 && ws_connections > 0 {
        "healthy"
    } else if liquidity_score > 20.0 {
        "degraded"
    } else {
        "critical"
    };

    Ok(Json(MarketHealth {
        status: status.to_string(),
        order_book_health: OrderBookHealth {
            best_bid: order_book.best_bid.map(|p| p.to_string()),
            best_ask: order_book.best_ask.map(|p| p.to_string()),
            spread: order_book.spread.map(|p| p.to_string()),
            spread_percentage,
            buy_orders_count: order_book.buy_depth.len(),
            sell_orders_count: order_book.sell_depth.len(),
            total_buy_volume: total_buy.to_string(),
            total_sell_volume: total_sell.to_string(),
            liquidity_score,
        },
        matching_stats: MatchingStatistics {
            total_matches_24h: matching_stats_row
                .try_get::<i64, _>("total_matches")
                .unwrap_or(0),
            total_volume_24h: matching_stats_row
                .try_get::<Decimal, _>("total_volume")
                .unwrap_or_default()
                .to_string(),
            average_price_24h: matching_stats_row
                .try_get::<Decimal, _>("average_price")
                .unwrap_or_default()
                .to_string(),
            last_match_time: matching_stats_row
                .try_get::<Option<chrono::DateTime<Utc>>, _>("last_match_time")
                .ok()
                .flatten()
                .map(|t| t.to_rfc3339()),
            pending_orders,
        },
        settlement_stats,
        websocket_connections: ws_connections,
        timestamp: Utc::now().to_rfc3339(),
    }))
}

/// Get detailed trading analytics
#[utoipa::path(
    get,
    path = "/api/admin/market/analytics",
    responses(
        (status = 200, description = "Trading analytics retrieved", body = TradingAnalytics),
        (status = 403, description = "Admin access required"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Admin - Market",
    security(("bearer_auth" = []))
)]
pub async fn get_trading_analytics(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<TradingAnalytics>, ApiError> {
    // Get overall trade statistics
    let overall_stats = sqlx::query(
        r#"
        SELECT 
            COUNT(*) as total_trades,
            COALESCE(SUM(quantity::numeric), 0) as total_volume,
            COALESCE(SUM(total_value::numeric), 0) as total_value,
            COALESCE(AVG(quantity::numeric), 0) as avg_trade_size
        FROM trades
        WHERE executed_at > NOW() - INTERVAL '24 hours'
        "#,
    )
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::Database)?;

    // Get price statistics
    let price_stats = sqlx::query(
        r#"
        SELECT 
            (SELECT price::text FROM trades ORDER BY executed_at DESC LIMIT 1) as current_price,
            MAX(price::numeric) as high_24h,
            MIN(price::numeric) as low_24h,
            (SELECT price::numeric FROM trades WHERE executed_at > NOW() - INTERVAL '24 hours' ORDER BY executed_at ASC LIMIT 1) as open_24h,
            (SELECT price::numeric FROM trades ORDER BY executed_at DESC LIMIT 1) as close_24h
        FROM trades
        WHERE executed_at > NOW() - INTERVAL '24 hours'
        "#
    )
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let open_24h = price_stats
        .try_get::<Option<Decimal>, _>("open_24h")
        .ok()
        .flatten()
        .and_then(|d| rust_decimal::Decimal::from_str(&d.to_string()).ok());
    let close_24h = price_stats
        .try_get::<Option<Decimal>, _>("close_24h")
        .ok()
        .flatten()
        .and_then(|d| rust_decimal::Decimal::from_str(&d.to_string()).ok());

    let change_24h = match (open_24h, close_24h) {
        (Some(open), Some(close)) => Some((close - open).to_string()),
        _ => None,
    };

    let change_percentage_24h = match (open_24h, close_24h) {
        (Some(open), Some(close)) if open > rust_decimal::Decimal::ZERO => Some(
            ((close - open) / open * rust_decimal::Decimal::from(100))
                .to_string()
                .parse::<f64>()
                .unwrap_or(0.0),
        ),
        _ => None,
    };

    // Get top traders
    let top_traders = sqlx::query(
        r#"
        SELECT 
            buyer_id as user_id,
            COUNT(*) as total_trades,
            SUM(quantity::numeric) as total_volume
        FROM trades
        WHERE executed_at > NOW() - INTERVAL '24 hours'
        GROUP BY buyer_id
        ORDER BY total_volume DESC
        LIMIT 10
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let trader_stats: Vec<TraderStats> = top_traders
        .into_iter()
        .map(|row| TraderStats {
            user_id: row.try_get::<uuid::Uuid, _>("user_id")
                .map(|u| u.to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            total_trades: row.try_get::<i64, _>("total_trades").unwrap_or(0),
            total_volume: row
                .try_get::<Decimal, _>("total_volume")
                .unwrap_or_default()
                .to_string(),
            buy_volume: row
                .try_get::<Decimal, _>("total_volume")
                .unwrap_or_default()
                .to_string(),
            sell_volume: "0".to_string(),
        })
        .collect();

    // Get hourly volume
    let hourly_data = sqlx::query(
        r#"
        SELECT 
            DATE_TRUNC('hour', executed_at) as hour,
            SUM(quantity::numeric) as volume,
            COUNT(*) as trade_count
        FROM trades
        WHERE executed_at > NOW() - INTERVAL '24 hours'
        GROUP BY DATE_TRUNC('hour', executed_at)
        ORDER BY hour DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::Database)?;

    let hourly_volume: Vec<HourlyVolume> = hourly_data
        .into_iter()
        .map(|row| HourlyVolume {
            hour: row
                .try_get::<Option<chrono::DateTime<Utc>>, _>("hour")
                .ok()
                .flatten()
                .map(|h| h.to_rfc3339())
                .unwrap_or_default(),
            volume: row
                .try_get::<Decimal, _>("volume")
                .unwrap_or_default()
                .to_string(),
            trade_count: row.try_get::<i64, _>("trade_count").unwrap_or(0),
        })
        .collect();

    Ok(Json(TradingAnalytics {
        total_trades: overall_stats.try_get::<i64, _>("total_trades").unwrap_or(0),
        total_volume: overall_stats
            .try_get::<Decimal, _>("total_volume")
            .unwrap_or_default()
            .to_string(),
        total_value: overall_stats
            .try_get::<Decimal, _>("total_value")
            .unwrap_or_default()
            .to_string(),
        average_trade_size: overall_stats
            .try_get::<Decimal, _>("avg_trade_size")
            .unwrap_or_default()
            .to_string(),
        price_statistics: PriceStatistics {
            current_price: price_stats
                .try_get::<Option<String>, _>("current_price")
                .ok()
                .flatten(),
            high_24h: price_stats
                .try_get::<Option<Decimal>, _>("high_24h")
                .ok()
                .flatten()
                .map(|p| p.to_string()),
            low_24h: price_stats
                .try_get::<Option<Decimal>, _>("low_24h")
                .ok()
                .flatten()
                .map(|p| p.to_string()),
            open_24h: open_24h.map(|p| p.to_string()),
            close_24h: close_24h.map(|p| p.to_string()),
            change_24h,
            change_percentage_24h,
        },
        top_traders: trader_stats,
        hourly_volume,
    }))
}

/// Execute market control actions (admin only)
#[utoipa::path(
    post,
    path = "/api/admin/market/control",
    request_body = MarketControlRequest,
    responses(
        (status = 200, description = "Market control action executed", body = MarketControlResponse),
        (status = 403, description = "Admin access required"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Admin - Market",
    security(("bearer_auth" = []))
)]
pub async fn market_control(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Json(request): Json<MarketControlRequest>,
) -> Result<Json<MarketControlResponse>, ApiError> {
    let result = match request.action {
        MarketAction::TriggerMatching => {
            // Manually trigger a matching cycle
            match state.market_clearing_engine.execute_matching_cycle().await {
                Ok(count) => MarketControlResponse {
                    success: true,
                    message: format!("Matching cycle completed: {} trades executed", count),
                    timestamp: Utc::now().to_rfc3339(),
                },
                Err(e) => MarketControlResponse {
                    success: false,
                    message: format!("Matching cycle failed: {}", e),
                    timestamp: Utc::now().to_rfc3339(),
                },
            }
        }
        MarketAction::PauseTrading => {
            // Note: This would require additional state management
            MarketControlResponse {
                success: true,
                message: "Trading paused (feature not yet implemented)".to_string(),
                timestamp: Utc::now().to_rfc3339(),
            }
        }
        MarketAction::ResumeTrading => MarketControlResponse {
            success: true,
            message: "Trading resumed (feature not yet implemented)".to_string(),
            timestamp: Utc::now().to_rfc3339(),
        },
        MarketAction::ClearOrderBook => MarketControlResponse {
            success: false,
            message:
                "Clear order book is a dangerous operation and requires additional confirmation"
                    .to_string(),
            timestamp: Utc::now().to_rfc3339(),
        },
    };

    Ok(Json(result))
}

/// Request payload for event replay
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ReplayEventsRequest {
    pub start_slot: u64,
    pub end_slot: Option<u64>,
}

/// Response for event replay trigger
#[derive(Debug, Serialize, ToSchema)]
pub struct ReplayEventsResponse {
    pub message: String,
    pub job_id: String,
}

/// Trigger event replay
#[utoipa::path(
    post,
    path = "/api/admin/event-processor/replay",
    request_body = ReplayEventsRequest,
    responses(
        (status = 200, description = "Event replay triggered", body = ReplayEventsResponse),
        (status = 403, description = "Admin access required"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Admin - Event Processor",
    security(("bearer_auth" = []))
)]
pub async fn trigger_event_replay(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Json(payload): Json<ReplayEventsRequest>,
) -> Result<Json<ReplayEventsResponse>, ApiError> {
    tracing::info!("Triggering event replay: {:?}", payload);

    match state
        .event_processor_service
        .replay_events(payload.start_slot, payload.end_slot)
        .await
    {
        Ok(message) => {
            let response = ReplayEventsResponse {
                message,
                job_id: uuid::Uuid::new_v4().to_string(),
            };
            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!("Failed to trigger event replay: {}", e);
            Err(ApiError::Internal(e.to_string()))
        }
    }
}

/// Get event replay status
#[utoipa::path(
    get,
    path = "/api/admin/event-processor/replay",
    tag = "Admin",
    responses(
        (status = 200, description = "Replay status retrieved successfully", body = Option<ReplayStatus>),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    )
)]
pub async fn get_replay_status(
    State(state): State<AppState>,
) -> std::result::Result<
    Json<Option<crate::services::event_processor_service::ReplayStatus>>,
    ApiError,
> {
    let status = state.event_processor_service.get_replay_status();
    Ok(Json(status))
}

// =============================================================================
// Wallet Initialization Endpoints
// =============================================================================

/// Request to fix a specific user's wallet
#[derive(Debug, Deserialize, ToSchema)]
pub struct FixUserWalletRequest {
    /// User ID to fix
    pub user_id: Uuid,
    /// Force regenerate even if wallet exists
    #[serde(default)]
    pub force_regenerate: bool,
}

/// Request to fix wallets for specific test users by email
#[derive(Debug, Deserialize, ToSchema)]
pub struct FixTestUsersRequest {
    /// List of test user emails
    pub emails: Vec<String>,
}

/// Diagnose all user wallets
///
/// Returns the wallet status for all users, including:
/// - Whether they have encrypted keys
/// - If encryption format is correct (12-byte nonce)
/// - If data can be decrypted
/// - Recommended action
#[utoipa::path(
    get,
    path = "/api/admin/wallets/diagnose",
    tag = "Admin - Wallet Management",
    responses(
        (status = 200, description = "Wallet diagnoses for all users", body = Vec<WalletDiagnosis>),
        (status = 403, description = "Admin access required"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn diagnose_all_wallets(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<Vec<WalletDiagnosis>>, ApiError> {
    tracing::info!("Admin: Diagnosing all user wallets");

    let service = WalletInitializationService::new(
        state.db.clone(),
        state.config.encryption_secret.clone(),
        state.blockchain_service.clone(),
        state.config.solana_rpc_url.clone(),
    );

    match service.diagnose_all_users().await {
        Ok(diagnoses) => {
            tracing::info!("Diagnosed {} user wallets", diagnoses.len());
            Ok(Json(diagnoses))
        }
        Err(e) => {
            tracing::error!("Failed to diagnose wallets: {}", e);
            Err(ApiError::Internal(format!(
                "Failed to diagnose wallets: {}",
                e
            )))
        }
    }
}

/// Diagnose a specific user's wallet
#[utoipa::path(
    get,
    path = "/api/admin/wallets/diagnose/{user_id}",
    tag = "Admin - Wallet Management",
    params(
        ("user_id" = Uuid, Path, description = "User ID to diagnose")
    ),
    responses(
        (status = 200, description = "Wallet diagnosis for user", body = WalletDiagnosis),
        (status = 403, description = "Admin access required"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn diagnose_user_wallet(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    axum::extract::Path(user_id): axum::extract::Path<Uuid>,
) -> Result<Json<WalletDiagnosis>, ApiError> {
    tracing::info!("Admin: Diagnosing wallet for user {}", user_id);

    let service = WalletInitializationService::new(
        state.db.clone(),
        state.config.encryption_secret.clone(),
        state.blockchain_service.clone(),
        state.config.solana_rpc_url.clone(),
    );

    match service.diagnose_user_wallet(user_id).await {
        Ok(diagnosis) => Ok(Json(diagnosis)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(ApiError::NotFound(format!("User {} not found", user_id)))
            } else {
                Err(ApiError::Internal(format!(
                    "Failed to diagnose wallet: {}",
                    e
                )))
            }
        }
    }
}

/// Fix a specific user's wallet
///
/// Will generate a new wallet or re-encrypt existing data depending on the issue
#[utoipa::path(
    post,
    path = "/api/admin/wallets/fix",
    tag = "Admin - Wallet Management",
    request_body = FixUserWalletRequest,
    responses(
        (status = 200, description = "Wallet fix result", body = WalletFixResult),
        (status = 403, description = "Admin access required"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn fix_user_wallet(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Json(request): Json<FixUserWalletRequest>,
) -> Result<Json<WalletFixResult>, ApiError> {
    tracing::info!(
        "Admin: Fixing wallet for user {} (force={})",
        request.user_id,
        request.force_regenerate
    );

    let service = WalletInitializationService::new(
        state.db.clone(),
        state.config.encryption_secret.clone(),
        state.blockchain_service.clone(),
        state.config.solana_rpc_url.clone(),
    );

    match service
        .fix_user_wallet(request.user_id, request.force_regenerate)
        .await
    {
        Ok(result) => {
            tracing::info!(
                "Fixed wallet for user {}: {}",
                request.user_id,
                result.action_taken
            );
            Ok(Json(result))
        }
        Err(e) => {
            tracing::error!("Failed to fix wallet for user {}: {}", request.user_id, e);
            Err(ApiError::Internal(format!("Failed to fix wallet: {}", e)))
        }
    }
}

/// Fix all user wallets with issues
///
/// This will:
/// - Generate wallets for users without encrypted keys
/// - Re-encrypt wallets with legacy 16-byte IV to standard 12-byte nonce
/// - Skip users with valid encryption
#[utoipa::path(
    post,
    path = "/api/admin/wallets/fix-all",
    tag = "Admin - Wallet Management",
    responses(
        (status = 200, description = "Wallet initialization report", body = WalletInitializationReport),
        (status = 403, description = "Admin access required"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn fix_all_wallets(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<WalletInitializationReport>, ApiError> {
    tracing::info!("Admin: Fixing all user wallets");

    let service = WalletInitializationService::new(
        state.db.clone(),
        state.config.encryption_secret.clone(),
        state.blockchain_service.clone(),
        state.config.solana_rpc_url.clone(),
    );

    match service.fix_all_users().await {
        Ok(report) => {
            tracing::info!(
                "Wallet initialization complete: {} created, {} re-encrypted, {} errors",
                report.wallets_created,
                report.wallets_re_encrypted,
                report.errors.len()
            );
            Ok(Json(report))
        }
        Err(e) => {
            tracing::error!("Failed to fix all wallets: {}", e);
            Err(ApiError::Internal(format!(
                "Failed to fix wallets: {}",
                e
            )))
        }
    }
}

/// Fix wallets for specific test users by email
///
/// Convenient endpoint for initializing known test user accounts
#[utoipa::path(
    post,
    path = "/api/admin/wallets/fix-test-users",
    tag = "Admin - Wallet Management",
    request_body = FixTestUsersRequest,
    responses(
        (status = 200, description = "Fix results for test users", body = Vec<WalletFixResult>),
        (status = 403, description = "Admin access required"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn fix_test_users_wallets(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Json(request): Json<FixTestUsersRequest>,
) -> Result<Json<Vec<WalletFixResult>>, ApiError> {
    tracing::info!("Admin: Fixing wallets for test users: {:?}", request.emails);

    let service = WalletInitializationService::new(
        state.db.clone(),
        state.config.encryption_secret.clone(),
        state.blockchain_service.clone(),
        state.config.solana_rpc_url.clone(),
    );

    let email_refs: Vec<&str> = request.emails.iter().map(|s| s.as_str()).collect();

    match service.initialize_test_users(&email_refs).await {
        Ok(results) => {
            let success_count = results.iter().filter(|r| r.success).count();
            tracing::info!(
                "Fixed wallets for test users: {}/{} successful",
                success_count,
                results.len()
            );
            Ok(Json(results))
        }
        Err(e) => {
            tracing::error!("Failed to fix test user wallets: {}", e);
            Err(ApiError::Internal(format!(
                "Failed to fix test user wallets: {}",
                e
            )))
        }
    }
}
