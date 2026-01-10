pub mod market;
pub mod user;
pub mod types;
pub mod admin;

use axum::{routing::get, Router, middleware::from_fn};
use crate::AppState;
use crate::auth::middleware::require_admin_role;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/market", get(market::get_market_analytics))
        .route("/my-stats", get(user::get_user_trading_stats))
        .route("/my-history", get(user::get_user_wealth_history))
        .route("/transactions", get(user::get_user_transactions))
        .route("/admin/stats", get(admin::get_admin_stats).layer(from_fn(require_admin_role)))
        .route("/admin/activity", get(admin::get_admin_activity).layer(from_fn(require_admin_role)))
        .route("/admin/health", get(admin::get_system_health).layer(from_fn(require_admin_role)))
        .route("/admin/zones/economic", get(admin::get_zone_economic_insights).layer(from_fn(require_admin_role)))
}
