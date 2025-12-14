//! Simplified Auth Stub Handler
//! 
//! Mocks authentication endpoints for testing E2E flow without full database/email dependency.

use axum::{
    extract::State,
    Json,
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

use crate::AppState;

/// Login Request
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Registration Request
#[derive(Debug, Deserialize)]
pub struct RegistrationRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub first_name: String,
    pub last_name: String,
}

/// User Response
#[derive(Debug, Serialize, Clone)]
pub struct UserResponse {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub role: String,
    pub first_name: String,
    pub last_name: String,
}

/// Auth Response (Token)
#[derive(Debug, Serialize, Clone)]
pub struct AuthResponse {
    pub access_token: String,
    pub expires_in: i64,
    pub user: UserResponse,
}

/// Registration Response
#[derive(Debug, Serialize)]
pub struct RegistrationResponse {
    pub message: String,
    pub email_verification_sent: bool,
    pub auth: Option<AuthResponse>,
}

/// Mock Login Handler
pub async fn login(
    State(_state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Json<AuthResponse> {
    info!("🔐 Mock login for user: {}", request.username);

    // Mock successful login
    let user = UserResponse {
        id: Uuid::new_v4(),
        username: request.username,
        email: "mock@example.com".to_string(),
        role: "user".to_string(),
        first_name: "Mock".to_string(),
        last_name: "User".to_string(),
    };

    Json(AuthResponse {
        access_token: "mock_jwt_token_12345".to_string(),
        expires_in: 86400,
        user,
    })
}

/// Mock Register Handler
pub async fn register(
    State(_state): State<AppState>,
    Json(request): Json<RegistrationRequest>,
) -> Json<RegistrationResponse> {
    info!("📝 Mock registration for user: {}", request.username);

    let user = UserResponse {
        id: Uuid::new_v4(),
        username: request.username,
        email: request.email,
        role: "user".to_string(),
        first_name: request.first_name,
        last_name: request.last_name,
    };

    let auth = AuthResponse {
        access_token: "mock_jwt_token_12345".to_string(),
        expires_in: 86400,
        user,
    };

    Json(RegistrationResponse {
        message: "Registration successful (Mock)".to_string(),
        email_verification_sent: false, // Bypass verification
        auth: Some(auth),
    })
}

/// Mock Profile Handler
pub async fn profile() -> Json<UserResponse> {
    info!("👤 Mock profile request");
    
    // Return a mock user profile
    // In a real stub, we might decode the token, but here we just return generic data
    Json(UserResponse {
        id: Uuid::new_v4(),
        username: "mock_user".to_string(),
        email: "mock@example.com".to_string(),
        role: "user".to_string(),
        first_name: "Mock".to_string(),
        last_name: "User".to_string(),
    })
}

/// Build auth routes
pub fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/register", post(register))
        .route("/profile", axum::routing::get(profile))
}
