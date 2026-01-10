//! User Wallets Handler
//!
//! Manage multiple wallet addresses linked to a user account

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use tracing::{info, error};
use utoipa::ToSchema;
use validator::Validate;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::AppState;

/// Linked wallet record
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct UserWallet {
    pub id: Uuid,
    pub user_id: Uuid,
    pub wallet_address: String,
    pub label: Option<String>,
    pub is_primary: bool,
    pub verified: bool,
    pub created_at: DateTime<Utc>,
}

/// Request to link a new wallet
#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct LinkWalletRequest {
    /// Solana wallet address (base58 encoded)
    #[validate(length(min = 32, max = 64))]
    pub wallet_address: String,
    
    /// Optional label for the wallet
    #[validate(length(max = 50))]
    pub label: Option<String>,
    
    /// Set as primary wallet
    pub is_primary: Option<bool>,
}

/// Response for wallet operations
#[derive(Debug, Serialize, ToSchema)]
pub struct WalletResponse {
    pub wallet: UserWallet,
    pub message: String,
}

/// List linked wallets response
#[derive(Debug, Serialize, ToSchema)]
pub struct WalletsListResponse {
    pub wallets: Vec<UserWallet>,
    pub count: usize,
}

/// List all wallets linked to user
/// GET /api/v1/wallets
#[utoipa::path(
    get,
    path = "/api/v1/wallets",
    tag = "wallets",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of linked wallets", body = WalletsListResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_wallets(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<WalletsListResponse>> {
    let wallets = sqlx::query_as!(
        UserWallet,
        r#"
        SELECT id, user_id, wallet_address, label, 
               is_primary as "is_primary!", verified as "verified!",
               created_at as "created_at!"
        FROM user_wallets
        WHERE user_id = $1
        ORDER BY is_primary DESC, created_at ASC
        "#,
        user.0.sub
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to list wallets: {}", e);
        ApiError::Internal(format!("Failed to list wallets: {}", e))
    })?;

    Ok(Json(WalletsListResponse {
        count: wallets.len(),
        wallets,
    }))
}

/// Link a new wallet to user account
/// POST /api/v1/wallets
#[utoipa::path(
    post,
    path = "/api/v1/wallets",
    tag = "wallets",
    request_body = LinkWalletRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Wallet linked", body = WalletResponse),
        (status = 400, description = "Invalid wallet address or already linked"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn link_wallet(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<LinkWalletRequest>,
) -> Result<Json<WalletResponse>> {
    info!("Linking wallet {} for user {}", payload.wallet_address, user.0.sub);

    // Validate wallet address format (basic check)
    if payload.wallet_address.len() < 32 || payload.wallet_address.len() > 64 {
        return Err(ApiError::BadRequest("Invalid wallet address format".to_string()));
    }

    let wallet_id = Uuid::new_v4();
    let now = Utc::now();
    let set_primary = payload.is_primary.unwrap_or(false);

    // Begin transaction
    let mut tx = state.db.begin().await.map_err(|e| ApiError::Internal(e.to_string()))?;

    // If setting as primary, unset any existing primary
    if set_primary {
        sqlx::query!(
            "UPDATE user_wallets SET is_primary = false WHERE user_id = $1 AND is_primary = true",
            user.0.sub
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to update primary: {}", e)))?;
    }

    // Check if this is the first wallet (auto-set as primary)
    let existing_count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM user_wallets WHERE user_id = $1",
        user.0.sub
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?
    .unwrap_or(0);

    let is_primary = set_primary || existing_count == 0;

    // Insert the wallet
    let wallet = sqlx::query_as!(
        UserWallet,
        r#"
        INSERT INTO user_wallets (id, user_id, wallet_address, label, is_primary, verified, created_at)
        VALUES ($1, $2, $3, $4, $5, false, $6)
        RETURNING id, user_id, wallet_address, label, 
                  is_primary as "is_primary!", verified as "verified!",
                  created_at as "created_at!"
        "#,
        wallet_id,
        user.0.sub,
        payload.wallet_address,
        payload.label,
        is_primary,
        now
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        if e.to_string().contains("unique_wallet_address") {
            ApiError::BadRequest("This wallet address is already linked to an account".to_string())
        } else {
            error!("Failed to link wallet: {}", e);
            ApiError::Internal(format!("Failed to link wallet: {}", e))
        }
    })?;

    tx.commit().await.map_err(|e| ApiError::Internal(e.to_string()))?;

    info!("Linked wallet {} as {} for user {}", 
          wallet.wallet_address, 
          if is_primary { "primary" } else { "secondary" },
          user.0.sub);

    Ok(Json(WalletResponse {
        wallet,
        message: format!("Wallet linked successfully{}", if is_primary { " as primary" } else { "" }),
    }))
}

/// Remove a linked wallet
/// DELETE /api/v1/wallets/:id
#[utoipa::path(
    delete,
    path = "/api/v1/wallets/{id}",
    tag = "wallets",
    params(("id" = Uuid, Path, description = "Wallet ID to remove")),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Wallet removed"),
        (status = 400, description = "Cannot remove primary wallet"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Wallet not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn remove_wallet(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(wallet_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    info!("Removing wallet {} for user {}", wallet_id, user.0.sub);

    // Check if it's the primary wallet
    let wallet = sqlx::query!(
        "SELECT is_primary FROM user_wallets WHERE id = $1 AND user_id = $2",
        wallet_id,
        user.0.sub
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    match wallet {
        None => return Err(ApiError::NotFound("Wallet not found".to_string())),
        Some(w) if w.is_primary.unwrap_or(false) => {
            // Check if there are other wallets to promote
            let other_count: i64 = sqlx::query_scalar!(
                "SELECT COUNT(*) FROM user_wallets WHERE user_id = $1 AND id != $2",
                user.0.sub,
                wallet_id
            )
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .unwrap_or(0);

            if other_count == 0 {
                return Err(ApiError::BadRequest("Cannot remove the only wallet. Link another wallet first.".to_string()));
            }
        }
        _ => {}
    }

    // Delete the wallet
    let result = sqlx::query!(
        "DELETE FROM user_wallets WHERE id = $1 AND user_id = $2",
        wallet_id,
        user.0.sub
    )
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Failed to remove wallet: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("Wallet not found".to_string()));
    }

    // If we removed the primary, promote the oldest remaining wallet
    sqlx::query!(
        r#"
        UPDATE user_wallets 
        SET is_primary = true 
        WHERE user_id = $1 
          AND id = (SELECT id FROM user_wallets WHERE user_id = $1 ORDER BY created_at ASC LIMIT 1)
          AND NOT EXISTS (SELECT 1 FROM user_wallets WHERE user_id = $1 AND is_primary = true)
        "#,
        user.0.sub
    )
    .execute(&state.db)
    .await
    .ok();

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Wallet removed successfully"
    })))
}

/// Set a wallet as primary
/// PUT /api/v1/wallets/:id/primary
#[utoipa::path(
    put,
    path = "/api/v1/wallets/{id}/primary",
    tag = "wallets",
    params(("id" = Uuid, Path, description = "Wallet ID to set as primary")),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Wallet set as primary", body = WalletResponse),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Wallet not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn set_primary_wallet(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(wallet_id): Path<Uuid>,
) -> Result<Json<WalletResponse>> {
    info!("Setting wallet {} as primary for user {}", wallet_id, user.0.sub);

    // Begin transaction
    let mut tx = state.db.begin().await.map_err(|e| ApiError::Internal(e.to_string()))?;

    // Unset current primary
    sqlx::query!(
        "UPDATE user_wallets SET is_primary = false WHERE user_id = $1 AND is_primary = true",
        user.0.sub
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Set new primary
    let wallet = sqlx::query_as!(
        UserWallet,
        r#"
        UPDATE user_wallets 
        SET is_primary = true
        WHERE id = $1 AND user_id = $2
        RETURNING id, user_id, wallet_address, label, 
                  is_primary as "is_primary!", verified as "verified!",
                  created_at as "created_at!"
        "#,
        wallet_id,
        user.0.sub
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    match wallet {
        Some(w) => {
            tx.commit().await.map_err(|e| ApiError::Internal(e.to_string()))?;
            Ok(Json(WalletResponse {
                wallet: w,
                message: "Wallet set as primary".to_string(),
            }))
        }
        None => {
            tx.rollback().await.ok();
            Err(ApiError::NotFound("Wallet not found".to_string()))
        }
    }
}

