//! Authentication Handlers Module
//!
//! Provides authentication and user session management with both
//! legacy routes and new RESTful v1 API endpoints.

pub mod stub;

// Re-export V1 route builders (new RESTful API)
pub use stub::{
    v1_auth_routes, v1_users_routes, v1_meters_routes, v1_wallets_routes, v1_status_routes,
};

// Re-export legacy route builders (backward compatibility)
pub use stub::{
    auth_routes, token_routes, user_meter_routes, meter_info_routes,
};

// Re-export handler functions
pub use stub::{
    login, register, profile, verify_email, resend_verification,
    get_my_meters, register_meter, get_registered_meters, verify_meter,
    token_balance,
    // V1 handlers
    get_registered_meters_filtered, update_meter_status, create_reading,
    system_status, meter_status,
};

// Re-export types
pub use stub::{
    LoginRequest, AuthResponse, UserResponse,
    RegistrationRequest, RegistrationResponse, 
    MeterResponse, RegisterMeterRequest, RegisterMeterResponse,
    TokenBalanceResponse, VerifyEmailResponse, VerifyMeterRequest,
    MeterFilterParams, UpdateMeterStatusRequest, CreateReadingRequest, CreateReadingResponse, StatusResponse,
};
