pub mod admin;
pub mod analytics;
pub mod audit;
pub mod auth;
pub mod authorization;
pub mod blockchain;
pub mod blockchain_test;
pub mod dashboard;
pub mod email_verification;
pub mod energy_trading;
pub mod epochs;
pub mod erc;
pub mod extractors;
pub mod governance;
pub mod health;
pub mod market_data;
// pub mod meter_registration;  // Using user_management handlers instead
pub mod meter_verification;
pub mod meters;
pub mod metrics;
pub mod oracle;
pub mod queries;
pub mod registry;
pub mod response;
pub mod swap;
pub mod token;
pub mod trading;
pub mod transactions;
pub mod user_management;
pub mod wallet_auth;
pub mod websocket;

// Re-export commonly used types
pub use authorization::{
    can_access_user_data, can_submit_meter_readings, can_trade, can_view_analytics,
    require_admin, require_admin_or_owner, require_any_role, require_role, roles,
};
pub use extractors::{DateRangeParams, PaginationParams, SearchParams, SortOrder, ValidatedUuid};
pub use queries::{
    AuditLogQuery, MeterReadingsQuery, OrderQuery, StandardPaginationQuery, TransactionQuery,
    UserSearchQuery,
};
pub use response::{ApiResponse, ListResponse, PaginatedResponse};
