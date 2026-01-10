//! Authentication Handlers Module
//!
//! Provides authentication and user session management with both
//! legacy routes and new RESTful v1 API endpoints.
//!
//! ## Structure
//! - `types` - All request/response types
//! - `login` - Login and email verification handlers
//! - `registration` - User registration handlers
//! - `profile` - User profile handlers
//! - `meters` - Meter management handlers
//! - `wallets` - Wallet/token balance handlers
//! - `status` - Status endpoint handlers
//! - `wallet_session` - Wallet unlock/lock session handlers
//! - `routes` - Route builders

// Type definitions
pub mod types;

// Handler modules
pub mod login;
pub mod registration;
pub mod password_reset;
pub mod profile;
pub mod meters;
pub mod wallets;
pub mod status;

// Route builders
pub mod routes;

pub use routes::{
    v1_auth_routes, v1_users_routes, v1_meters_routes, v1_wallets_routes, v1_status_routes,
};

// Re-export handler functions
pub use login::{login, verify_email};
pub use registration::{register, resend_verification};
pub use password_reset::{forgot_password, reset_password, change_password};
pub use profile::profile;
pub use meters::{
    get_my_meters, register_meter, get_registered_meters, 
    get_registered_meters_filtered, update_meter_status, verify_meter, create_reading,
    get_meter_stats,
};
pub use wallets::token_balance;
pub use status::{system_status, meter_status, readiness_probe, liveness_probe};

// Re-export types
pub use types::{
    LoginRequest, AuthResponse, UserResponse,
    RegistrationRequest, RegistrationResponse, 
    ForgotPasswordRequest, ResetPasswordRequest,
    MeterResponse, PublicMeterResponse, RegisterMeterRequest, RegisterMeterResponse,
    TokenBalanceResponse, VerifyEmailResponse, VerifyMeterRequest,
    MeterFilterParams, UpdateMeterStatusRequest, CreateReadingRequest, CreateReadingResponse,
    MeterStats, GetTrendsQuery, TrendRecord, TrendResponse,
};
pub use status::{
    HealthResponse, ServiceStatus, ServiceHealth,
    MeterStatusResponse, MeterCounts, ReadinessResponse, LivenessResponse,
};
