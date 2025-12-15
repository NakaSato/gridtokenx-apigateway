//! Profile Handlers Module
//!
//! User profile management handlers.

use axum::{
    extract::State,
    http::HeaderMap,
    Json,
};
use tracing::info;
use uuid::Uuid;

use crate::AppState;
use super::types::{UserResponse, UserRow};

/// Profile Handler - fetches user from database by token
pub async fn profile(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Json<UserResponse> {
    let auth_header = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    
    let token = auth_header.strip_prefix("Bearer ").unwrap_or(auth_header);
    
    info!("👤 Profile request");

    // Try to decode token and get user from database
    if let Ok(claims) = state.jwt_service.decode_token(token) {
        let user_result = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, email, role::text as role, first_name, last_name, wallet_address 
             FROM users WHERE id = $1"
        )
        .bind(claims.sub)
        .fetch_optional(&state.db)
        .await;

        if let Ok(Some(user)) = user_result {
            info!("✅ Returning profile for: {} (from database)", user.username);
            return Json(UserResponse {
                id: user.id,
                username: user.username,
                email: user.email,
                role: user.role,
                first_name: user.first_name.unwrap_or_default(),
                last_name: user.last_name.unwrap_or_default(),
                wallet_address: user.wallet_address,
            });
        }
    }

    // Fallback to guest
    info!("⚠️ Token invalid or user not found");
    Json(UserResponse {
        id: Uuid::new_v4(),
        username: "guest".to_string(),
        email: "guest@gridtokenx.com".to_string(),
        role: "user".to_string(),
        first_name: "Guest".to_string(),
        last_name: "User".to_string(),
        wallet_address: None,
    })
}
