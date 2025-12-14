//! Wallet management handlers

use axum::{extract::State, http::StatusCode, response::Json};
use validator::Validate;

use crate::auth::middleware::AuthenticatedUser;
use crate::auth::UserInfo;
use crate::error::{ApiError, Result};
use crate::AppState;

use super::types::{is_valid_solana_address, log_user_activity, UpdateWalletRequest};

/// Update wallet address for current user
#[utoipa::path(
    post,
    path = "/api/user/wallet",
    tag = "users",
    request_body = UpdateWalletRequest,
    responses(
        (status = 200, description = "Wallet address updated successfully", body = UserInfo),
        (status = 400, description = "Invalid wallet address format"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "User not found")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn update_wallet_address(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(request): Json<UpdateWalletRequest>,
) -> Result<Json<UserInfo>> {
    // Validate request
    request
        .validate()
        .map_err(|e| ApiError::BadRequest(format!("Validation error: {}", e)))?;

    // Check if email is verified (required before wallet connection)
    let user_verified =
        sqlx::query_scalar::<_, bool>("SELECT email_verified FROM users WHERE id = $1")
            .bind(user.0.sub)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?
            .ok_or_else(|| ApiError::NotFound("User not found".to_string()))?;

    if !user_verified {
        return Err(ApiError::email_not_verified());
    }

    // Validate wallet address format
    if !is_valid_solana_address(&request.wallet_address) {
        return Err(ApiError::BadRequest(
            "Invalid Solana wallet address format".to_string(),
        ));
    }

    // Check if wallet address is already in use
    let existing_wallet = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM users WHERE wallet_address = $1 AND id != $2",
    )
    .bind(&request.wallet_address)
    .bind(user.0.sub)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    if existing_wallet > 0 {
        return Err(ApiError::BadRequest(
            "Wallet address is already in use".to_string(),
        ));
    }

    // Update wallet address
    let result = sqlx::query(
        "UPDATE users SET wallet_address = $1, updated_at = NOW() WHERE id = $2 AND is_active = true"
    )
    .bind(&request.wallet_address)
    .bind(user.0.sub)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Failed to update wallet address: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("User not found".to_string()));
    }

    // Log wallet update activity
    let _ = log_user_activity(
        &state.db,
        user.0.sub,
        "wallet_updated".to_string(),
        Some(serde_json::json!({
            "wallet_address": request.wallet_address
        })),
        None,
        None,
    )
    .await;

    // Return updated profile
    crate::handlers::auth::get_profile(State(state), user).await
}

/// Remove wallet address for current user
#[utoipa::path(
    delete,
    path = "/api/user/wallet",
    tag = "users",
    responses(
        (status = 204, description = "Wallet address removed successfully"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "User not found")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn remove_wallet_address(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<StatusCode> {
    // Update wallet address to null
    let result = sqlx::query(
        "UPDATE users SET wallet_address = NULL, blockchain_registered = false, updated_at = NOW()
         WHERE id = $1 AND is_active = true",
    )
    .bind(user.0.sub)
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Failed to remove wallet address: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("User not found".to_string()));
    }

    // Log wallet removal activity
    let _ = log_user_activity(
        &state.db,
        user.0.sub,
        "wallet_removed".to_string(),
        None,
        None,
        None,
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}
