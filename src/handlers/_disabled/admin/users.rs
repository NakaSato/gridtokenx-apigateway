use axum::{extract::State, response::Json};
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::ApiError;
use crate::services::wallet::initialization::{
    WalletDiagnosis, WalletFixResult, WalletInitializationReport, WalletInitializationService,
};
use crate::AppState;

/// Request to fix a specific user's wallet
#[derive(Debug, Deserialize, ToSchema)]
pub struct FixUserWalletRequest {
    /// User ID to fix
    pub user_id: Uuid,
    /// Force regenerate even if wallet exists
    #[serde(default)]
    pub force_regenerate: bool,
}

/// Request to fix wallets for specific test users by email
#[derive(Debug, Deserialize, ToSchema)]
pub struct FixTestUsersRequest {
    /// List of test user emails
    pub emails: Vec<String>,
}

/// Diagnose all user wallets
///
/// Returns the wallet status for all users, including:
/// - Whether they have encrypted keys
/// - If encryption format is correct (12-byte nonce)
/// - If data can be decrypted
/// - Recommended action
#[utoipa::path(
    get,
    path = "/api/admin/wallets/diagnose",
    tag = "Admin - Wallet Management",
    responses(
        (status = 200, description = "Wallet diagnoses for all users", body = Vec<WalletDiagnosis>),
        (status = 403, description = "Admin access required"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn diagnose_all_wallets(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<Vec<WalletDiagnosis>>, ApiError> {
    tracing::info!("Admin: Diagnosing all user wallets");

    let service = WalletInitializationService::new(
        state.db.clone(),
        state.config.encryption_secret.clone(),
        state.blockchain_service.clone(),
        state.config.solana_rpc_url.clone(),
    );

    match service.diagnose_all_users().await {
        Ok(diagnoses) => {
            tracing::info!("Diagnosed {} user wallets", diagnoses.len());
            Ok(Json(diagnoses))
        }
        Err(e) => {
            tracing::error!("Failed to diagnose wallets: {}", e);
            Err(ApiError::Internal(format!(
                "Failed to diagnose wallets: {}",
                e
            )))
        }
    }
}

/// Diagnose a specific user's wallet
#[utoipa::path(
    get,
    path = "/api/admin/wallets/diagnose/{user_id}",
    tag = "Admin - Wallet Management",
    params(
        ("user_id" = Uuid, Path, description = "User ID to diagnose")
    ),
    responses(
        (status = 200, description = "Wallet diagnosis for user", body = WalletDiagnosis),
        (status = 403, description = "Admin access required"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn diagnose_user_wallet(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    axum::extract::Path(user_id): axum::extract::Path<Uuid>,
) -> Result<Json<WalletDiagnosis>, ApiError> {
    tracing::info!("Admin: Diagnosing wallet for user {}", user_id);

    let service = WalletInitializationService::new(
        state.db.clone(),
        state.config.encryption_secret.clone(),
        state.blockchain_service.clone(),
        state.config.solana_rpc_url.clone(),
    );

    match service.diagnose_user_wallet(user_id).await {
        Ok(diagnosis) => Ok(Json(diagnosis)),
        Err(e) => {
            if e.to_string().contains("not found") {
                Err(ApiError::NotFound(format!("User {} not found", user_id)))
            } else {
                Err(ApiError::Internal(format!(
                    "Failed to diagnose wallet: {}",
                    e
                )))
            }
        }
    }
}

/// Fix a specific user's wallet
///
/// Will generate a new wallet or re-encrypt existing data depending on the issue
#[utoipa::path(
    post,
    path = "/api/admin/wallets/fix",
    tag = "Admin - Wallet Management",
    request_body = FixUserWalletRequest,
    responses(
        (status = 200, description = "Wallet fix result", body = WalletFixResult),
        (status = 403, description = "Admin access required"),
        (status = 404, description = "User not found"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn fix_user_wallet(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Json(request): Json<FixUserWalletRequest>,
) -> Result<Json<WalletFixResult>, ApiError> {
    tracing::info!(
        "Admin: Fixing wallet for user {} (force={})",
        request.user_id,
        request.force_regenerate
    );

    let service = WalletInitializationService::new(
        state.db.clone(),
        state.config.encryption_secret.clone(),
        state.blockchain_service.clone(),
        state.config.solana_rpc_url.clone(),
    );

    match service
        .fix_user_wallet(request.user_id, request.force_regenerate)
        .await
    {
        Ok(result) => {
            tracing::info!(
                "Fixed wallet for user {}: {}",
                request.user_id,
                result.action_taken
            );
            Ok(Json(result))
        }
        Err(e) => {
            tracing::error!("Failed to fix wallet for user {}: {}", request.user_id, e);
            Err(ApiError::Internal(format!("Failed to fix wallet: {}", e)))
        }
    }
}

/// Fix all user wallets with issues
///
/// This will:
/// - Generate wallets for users without encrypted keys
/// - Re-encrypt wallets with legacy 16-byte IV to standard 12-byte nonce
/// - Skip users with valid encryption
#[utoipa::path(
    post,
    path = "/api/admin/wallets/fix-all",
    tag = "Admin - Wallet Management",
    responses(
        (status = 200, description = "Wallet initialization report", body = WalletInitializationReport),
        (status = 403, description = "Admin access required"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn fix_all_wallets(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<WalletInitializationReport>, ApiError> {
    tracing::info!("Admin: Fixing all user wallets");

    let service = WalletInitializationService::new(
        state.db.clone(),
        state.config.encryption_secret.clone(),
        state.blockchain_service.clone(),
        state.config.solana_rpc_url.clone(),
    );

    match service.fix_all_users().await {
        Ok(report) => {
            tracing::info!(
                "Wallet initialization complete: {} created, {} re-encrypted, {} errors",
                report.wallets_created,
                report.wallets_re_encrypted,
                report.errors.len()
            );
            Ok(Json(report))
        }
        Err(e) => {
            tracing::error!("Failed to fix all wallets: {}", e);
            Err(ApiError::Internal(format!("Failed to fix wallets: {}", e)))
        }
    }
}

/// Fix wallets for specific test users by email
///
/// Convenient endpoint for initializing known test user accounts
#[utoipa::path(
    post,
    path = "/api/admin/wallets/fix-test-users",
    tag = "Admin - Wallet Management",
    request_body = FixTestUsersRequest,
    responses(
        (status = 200, description = "Fix results for test users", body = Vec<WalletFixResult>),
        (status = 403, description = "Admin access required"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearer_auth" = []))
)]
pub async fn fix_test_users_wallets(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Json(request): Json<FixTestUsersRequest>,
) -> Result<Json<Vec<WalletFixResult>>, ApiError> {
    tracing::info!("Admin: Fixing wallets for test users: {:?}", request.emails);

    let service = WalletInitializationService::new(
        state.db.clone(),
        state.config.encryption_secret.clone(),
        state.blockchain_service.clone(),
        state.config.solana_rpc_url.clone(),
    );

    let email_refs: Vec<&str> = request.emails.iter().map(|s| s.as_str()).collect();

    match service.initialize_test_users(&email_refs).await {
        Ok(results) => {
            let success_count = results.iter().filter(|r| r.success).count();
            tracing::info!(
                "Fixed wallets for test users: {}/{} successful",
                success_count,
                results.len()
            );
            Ok(Json(results))
        }
        Err(e) => {
            tracing::error!("Failed to fix test user wallets: {}", e);
            Err(ApiError::Internal(format!(
                "Failed to fix test user wallets: {}",
                e
            )))
        }
    }
}
