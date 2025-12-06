//! Protected routes that require authentication.
//!
//! Includes: user profile, trading, meters, tokens, ERC certificates, and admin routes.

use axum::{
    middleware::from_fn_with_state,
    routing::{get, post},
    Router,
};

use crate::app_state::AppState;
use crate::auth;
use crate::handlers::{
    self, admin, audit, auth as auth_handlers, blockchain, blockchain_test, epochs, erc,
    governance, meters, oracle, registry, token, trading, transactions, user_management,
};
use crate::middleware;

/// Build protected routes that require authentication.
pub fn protected_routes(app_state: AppState) -> Router<AppState> {
    Router::new()
        // Protected auth routes
        .route("/api/auth/profile", get(auth_handlers::get_profile))
        .route(
            "/api/auth/profile/update",
            post(auth_handlers::update_profile),
        )
        .route("/api/auth/password", post(auth_handlers::change_password))
        // Wallet management routes
        .route(
            "/api/wallet/export",
            post(handlers::wallet_auth::export_wallet_handler),
        )
        // User management routes
        .nest("/api/user", user_routes())
        // Admin-only user management routes
        .nest("/api/users", admin_user_routes())
        // Blockchain interaction routes
        .nest("/api/blockchain", blockchain_routes())
        // Blockchain testing routes
        .nest("/api/test", test_routes())
        // Admin-only routes
        .nest("/api/admin", admin_routes())
        // Oracle routes
        .nest("/api/oracle", oracle_routes())
        // Governance routes
        .nest("/api/governance", governance_routes())
        // Market data routes
        .nest("/api/market-data", market_data_routes())
        // Trading routes
        .nest("/api/trading", trading_routes())
        // Analytics routes
        .route(
            "/api/analytics/market",
            get(handlers::analytics::get_market_analytics),
        )
        .route(
            "/api/analytics/my-stats",
            get(handlers::analytics::get_user_trading_stats),
        )
        // Token routes
        .nest("/api/tokens", token_routes())
        // Meter routes
        .nest("/api/meters", meter_routes())
        // Admin meter routes
        .nest("/api/admin/meters", admin_meter_routes())
        // ERC routes
        .nest("/api/erc", erc_routes())
        // Apply authentication middleware
        .layer(from_fn_with_state(
            app_state,
            auth::middleware::auth_middleware,
        ))
        .layer(axum::middleware::from_fn(
            middleware::auth_logger_middleware,
        ))
}

/// User self-management routes
fn user_routes() -> Router<AppState> {
    Router::new()
        .route("/wallet", post(user_management::update_wallet_address))
        .route(
            "/wallet",
            axum::routing::delete(user_management::remove_wallet_address),
        )
        .route("/activity", get(user_management::get_my_activity))
        // Meter registration routes
        .route("/meters", post(user_management::register_meter_handler))
        .route("/meters", get(user_management::get_user_meters_handler))
        .route(
            "/meters/{meter_id}",
            axum::routing::delete(user_management::delete_meter_handler),
        )
}

/// Admin user management routes
fn admin_user_routes() -> Router<AppState> {
    Router::new()
        .route("/{id}", get(auth_handlers::get_user))
        .route(
            "/{id}",
            axum::routing::put(user_management::admin_update_user),
        )
        .route(
            "/{id}/deactivate",
            post(user_management::admin_deactivate_user),
        )
        .route(
            "/{id}/reactivate",
            post(user_management::admin_reactivate_user),
        )
        .route("/{id}/activity", get(user_management::get_user_activity))
        .route("/", get(auth_handlers::list_users))
}

/// Blockchain interaction routes
fn blockchain_routes() -> Router<AppState> {
    Router::new()
        .route("/transactions", post(blockchain::submit_transaction))
        .route("/transactions", get(blockchain::get_transaction_history))
        .route(
            "/transactions/{signature}",
            get(blockchain::get_transaction_status),
        )
        .route("/programs/{name}", post(blockchain::interact_with_program))
        .route("/accounts/{address}", get(blockchain::get_account_info))
        .route("/network", get(blockchain::get_network_status))
        .route(
            "/users/{wallet_address}",
            get(registry::get_blockchain_user),
        )
}

/// Blockchain testing routes
fn test_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/transactions",
            post(blockchain_test::create_test_transaction),
        )
        .route(
            "/transactions/{signature}",
            get(blockchain_test::get_test_transaction_status),
        )
        .route("/statistics", get(blockchain_test::get_test_statistics))
}

/// Admin routes
fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/users/{id}/update-role", post(registry::update_user_role))
        // Governance admin routes
        .route(
            "/governance/emergency-pause",
            post(governance::emergency_pause),
        )
        .route("/governance/unpause", post(governance::emergency_unpause))
        // Token admin routes
        .route("/tokens/mint", post(token::mint_tokens))
        // AMM Routes
        .route("/swap/quote", post(handlers::swap::get_quote))
        .route("/swap/execute", post(handlers::swap::execute_swap))
        .route("/swap/pools", get(handlers::swap::list_pools))
        .route("/swap/history", get(handlers::swap::get_swap_history))
        // Transaction routes
        .nest("/api/tx", transaction_routes())
        // Trading admin routes
        .route(
            "/trading/match-orders",
            post(trading::match_blockchain_orders),
        )
        // Market admin routes
        .route("/market/health", get(admin::get_market_health))
        .route("/market/analytics", get(admin::get_trading_analytics))
        .route("/market/control", post(admin::market_control))
        // Key rotation admin routes
        .route(
            "/keys/rotate",
            post(handlers::key_rotation::initiate_rotation_handler),
        )
        .route(
            "/keys/status",
            get(handlers::key_rotation::get_rotation_status_handler),
        )
        .route(
            "/keys/rollback",
            post(handlers::key_rotation::rollback_rotation_handler),
        )
        // Event Processor routes
        .route(
            "/event-processor/replay",
            post(admin::trigger_event_replay).get(admin::get_replay_status),
        )
        // Wallet management routes
        .route("/wallets/diagnose", get(admin::diagnose_all_wallets))
        .route(
            "/wallets/diagnose/{user_id}",
            get(admin::diagnose_user_wallet),
        )
        .route("/wallets/fix", post(admin::fix_user_wallet))
        .route("/wallets/fix-all", post(admin::fix_all_wallets))
        .route(
            "/wallets/fix-test-users",
            post(admin::fix_test_users_wallets),
        )
        // Audit log routes
        .route("/audit/user/{user_id}", get(audit::get_user_audit_logs))
        .route(
            "/audit/type/{event_type}",
            get(audit::get_audit_logs_by_type),
        )
        .route("/audit/security", get(audit::get_security_events))
        // Epoch management
        .route("/epochs", get(epochs::list_all_epochs))
        .route("/epochs/{epoch_id}/stats", get(epochs::get_epoch_stats))
        .route(
            "/epochs/{epoch_id}/trigger",
            post(epochs::trigger_manual_clearing),
        )
}

/// Transaction routes
fn transaction_routes() -> Router<AppState> {
    Router::new()
        .route("/{id}/status", get(transactions::get_transaction_status))
        .route("/user", get(transactions::get_user_transactions))
        .route("/history", get(transactions::get_transaction_history))
        .route("/stats", get(transactions::get_transaction_stats))
        .route("/{id}/retry", post(transactions::retry_transaction))
}

/// Oracle routes
fn oracle_routes() -> Router<AppState> {
    Router::new()
        .route("/prices", post(oracle::submit_price))
        .route("/prices/current", get(oracle::get_current_prices))
        .route("/data", get(oracle::get_oracle_data))
}

/// Governance routes
fn governance_routes() -> Router<AppState> {
    Router::new().route("/status", get(governance::get_governance_status))
}

/// Market data routes
fn market_data_routes() -> Router<AppState> {
    Router::new()
        .route("/depth", get(handlers::market_data::get_order_book_depth))
        .route(
            "/depth-chart",
            get(handlers::market_data::get_market_depth_chart),
        )
        .route(
            "/clearing-price",
            get(handlers::market_data::get_clearing_price),
        )
        .route(
            "/trades/my-history",
            get(handlers::market_data::get_my_trade_history),
        )
}

/// Trading routes
fn trading_routes() -> Router<AppState> {
    Router::new()
        .route("/orders", post(handlers::energy_trading::create_order))
        .route("/orders", get(handlers::energy_trading::list_orders))
}

/// Token routes
fn token_routes() -> Router<AppState> {
    Router::new()
        .route("/balance/{wallet_address}", get(token::get_token_balance))
        .route("/info", get(token::get_token_info))
        .route("/mint-from-reading", post(token::mint_from_reading))
}

/// Meter routes
fn meter_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/verify",
            post(handlers::meter_verification::verify_meter_handler),
        )
        .route(
            "/registered",
            get(handlers::meter_verification::get_registered_meters_handler),
        )
        .route("/submit-reading", post(meters::submit_reading))
        .route("/my-readings", get(meters::get_my_readings))
        .route(
            "/readings/{wallet_address}",
            get(meters::get_readings_by_wallet),
        )
        .route("/stats", get(meters::get_user_stats))
}

/// Admin meter routes
fn admin_meter_routes() -> Router<AppState> {
    Router::new()
        .route("/unminted", get(meters::get_unminted_readings))
        .route("/mint-from-reading", post(meters::mint_from_reading))
}

/// ERC certificate routes
fn erc_routes() -> Router<AppState> {
    Router::new()
        .route("/issue", post(erc::issue_certificate))
        .route("/my-certificates", get(erc::get_my_certificates))
        .route("/my-stats", get(erc::get_my_certificate_stats))
        .route("/{certificate_id}", get(erc::get_certificate))
        .route("/{certificate_id}/retire", post(erc::retire_certificate))
        .route(
            "/wallet/{wallet_address}",
            get(erc::get_certificates_by_wallet),
        )
}
