use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::error::{ApiError, Result};

// ==================== REQUEST/RESPONSE TYPES ====================

#[derive(Debug, Deserialize, IntoParams)]
pub struct AnalyticsTimeframe {
    /// Timeframe: 1h, 24h, 7d, 30d (default: 24h)
    #[serde(default = "default_timeframe")]
    pub timeframe: String,
    /// Energy source filter (optional)
    pub energy_source: Option<String>,
}

fn default_timeframe() -> String {
    "24h".to_string()
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MarketAnalytics {
    pub timeframe: String,
    pub market_overview: MarketOverview,
    pub trading_volume: TradingVolume,
    pub price_statistics: PriceStatistics,
    pub energy_source_breakdown: Vec<EnergySourceStats>,
    pub top_traders: Vec<TraderStats>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MarketOverview {
    pub total_active_offers: i64,
    pub total_pending_orders: i64,
    pub total_completed_transactions: i64,
    pub total_users_trading: i64,
    pub average_match_time_seconds: f64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TradingVolume {
    pub total_energy_traded_kwh: f64,
    pub total_value_usd: f64,
    pub number_of_transactions: i64,
    pub average_transaction_size_kwh: f64,
    pub volume_trend_percent: f64, // Compared to previous period
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PriceStatistics {
    pub current_avg_price_per_kwh: f64,
    pub lowest_price_per_kwh: f64,
    pub highest_price_per_kwh: f64,
    pub median_price_per_kwh: f64,
    pub price_volatility_percent: f64,
    pub price_trend_percent: f64, // Compared to previous period
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EnergySourceStats {
    pub energy_source: String,
    pub total_volume_kwh: f64,
    pub average_price_per_kwh: f64,
    pub transaction_count: i64,
    pub market_share_percent: f64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TraderStats {
    pub user_id: String,
    pub username: String,
    pub total_volume_kwh: f64,
    pub transaction_count: i64,
    pub average_price_per_kwh: f64,
    pub role: String, // "user", "admin"
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserTradingStats {
    pub user_id: String,
    pub username: String,
    pub timeframe: String,
    pub as_seller: SellerStats,
    pub as_buyer: BuyerStats,
    pub overall: OverallUserStats,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SellerStats {
    pub offers_created: i64,
    pub offers_fulfilled: i64,
    pub total_energy_sold_kwh: f64,
    pub total_revenue_usd: f64,
    pub average_price_per_kwh: f64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BuyerStats {
    pub orders_created: i64,
    pub orders_fulfilled: i64,
    pub total_energy_purchased_kwh: f64,
    pub total_spent_usd: f64,
    pub average_price_per_kwh: f64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OverallUserStats {
    pub total_transactions: i64,
    pub total_volume_kwh: f64,
    pub net_revenue_usd: f64, // revenue - spending
    pub favorite_energy_source: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserWealthHistory {
    pub timeframe: String,
    pub history: Vec<WealthPoint>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WealthPoint {
    pub timestamp: DateTime<Utc>,
    pub balance_usd: f64,
}

// ==================== HELPER FUNCTIONS ====================

pub fn parse_timeframe(timeframe: &str) -> Result<Duration> {
    match timeframe {
        "1h" => Ok(Duration::hours(1)),
        "24h" | "1d" => Ok(Duration::hours(24)),
        "7d" => Ok(Duration::days(7)),
        "30d" => Ok(Duration::days(30)),
        _ => Err(ApiError::validation_field(
            "timeframe",
            "Invalid timeframe. Use: 1h, 24h, 7d, or 30d",
        )),
    }
}

pub fn decimal_to_f64(d: Decimal) -> f64 {
    d.to_f64().unwrap_or(0.0)
}

// ==================== ZONE ANALYTICS TYPES ====================

#[derive(Debug, Serialize, ToSchema)]
pub struct ZoneTradeStats {
    pub timeframe: String,
    pub total_volume_kwh: f64,
    pub intra_zone_volume_kwh: f64,
    pub inter_zone_volume_kwh: f64,
    pub intra_zone_percent: f64,
    pub inter_zone_percent: f64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ZoneRevenueBreakdown {
    pub zone_id: i32,
    pub total_transaction_value: f64,
    pub total_platform_fees: f64,
    pub total_wheeling_charges: f64,
    pub avg_price_per_kwh: f64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ZoneEconomicInsights {
    pub timeframe: String,
    pub trade_stats: ZoneTradeStats,
    pub revenue_breakdown: Vec<ZoneRevenueBreakdown>,
}

// ==================== TRANSACTION TYPES ====================

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct UserTransaction {
    pub operation_type: String,
    pub operation_id: Uuid,
    pub user_id: Option<Uuid>,
    pub signature: Option<String>,
    pub tx_type: Option<String>,
    pub status: String,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SettlementMetadata {
    #[schema(value_type = String)]
    pub energy_amount: Decimal,
    #[schema(value_type = String)]
    pub price_per_kwh: Decimal,
    #[schema(value_type = String)]
    pub total_amount: Decimal,
    #[schema(value_type = String)]
    pub wheeling_charge: Decimal,
    #[schema(value_type = String)]
    pub loss_cost: Decimal,
    #[schema(value_type = String)]
    pub loss_factor: Decimal,
    #[schema(value_type = String)]
    pub effective_energy: Decimal,
    pub buyer_zone_id: Option<i32>,
    pub seller_zone_id: Option<i32>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct TransactionQuery {
    pub transaction_type: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserTransactionsResponse {
    pub transactions: Vec<UserTransaction>,
    pub total: i64,
}
