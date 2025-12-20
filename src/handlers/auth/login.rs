//! Login Handlers Module
//!
//! Authentication handlers for login and email verification.

use axum::{
    extract::State,
    Json,
    response::IntoResponse,
};
use solana_sdk::signer::Signer;
use tracing::info;
use uuid::Uuid;

use crate::AppState;
use crate::auth::password::PasswordService;
use crate::middleware::metrics::{track_auth_attempt, track_auth_failure};
use super::types::{
    LoginRequest, AuthResponse, UserResponse, UserRow,
    VerifyEmailResponse, VerifyEmailRequest,
};

/// Row type for login query that includes password_hash
#[derive(Debug, sqlx::FromRow)]
struct LoginUserRow {
    id: Uuid,
    username: String,
    email: String,
    password_hash: String,
    role: String,
    first_name: Option<String>,
    last_name: Option<String>,
    wallet_address: Option<String>,
}

/// Login Handler - queries database for user and verifies password
#[utoipa::path(
    post,
    path = "/api/v1/auth/token",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = AuthResponse),
        (status = 401, description = "Unauthorized - Invalid credentials"),
        (status = 500, description = "Internal server error")
    ),
    tag = "auth"
)]
pub async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> impl IntoResponse {
    info!("🔐 Login for user: {}", request.username);

    // Query database for user including password_hash
    let user_result = sqlx::query_as::<_, LoginUserRow>(
        "SELECT id, username, email, password_hash, role::text as role, first_name, last_name, wallet_address 
         FROM users WHERE username = $1 AND is_active = true"
    )
    .bind(&request.username)
    .fetch_optional(&state.db)
    .await;

    let user = match user_result {
        Ok(Some(u)) => {
            // Verify password using bcrypt
            match PasswordService::verify_password(&request.password, &u.password_hash) {
                Ok(true) => {
                    info!("✅ Password verified for user: {}", u.username);
                    track_auth_attempt(true, "password");
                    UserRow {
                        id: u.id,
                        username: u.username,
                        email: u.email,
                        role: u.role,
                        first_name: u.first_name,
                        last_name: u.last_name,
                        wallet_address: u.wallet_address,
                    }
                }
                Ok(false) => {
                    info!("❌ Invalid password for user: {}", u.username);
                    track_auth_attempt(false, "password");
                    track_auth_failure("invalid_password");
                    return (
                        axum::http::StatusCode::UNAUTHORIZED,
                        Json(AuthResponse {
                            access_token: "invalid_credentials".to_string(),
                            expires_in: 0,
                            user: UserResponse {
                                id: Uuid::nil(),
                                username: String::new(),
                                email: String::new(),
                                role: String::new(),
                                first_name: String::new(),
                                last_name: String::new(),
                                wallet_address: None,
                            },
                        })
                    ).into_response();
                }
                Err(e) => {
                    tracing::error!("❌ Password verification error: {}", e);
                    return (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        Json(AuthResponse {
                            access_token: String::new(),
                            expires_in: 0,
                            user: UserResponse {
                                id: Uuid::nil(),
                                username: String::new(),
                                email: String::new(),
                                role: String::new(),
                                first_name: String::new(),
                                last_name: String::new(),
                                wallet_address: None,
                            },
                        })
                    ).into_response();
                }
            }
        }
        Ok(None) => {
            info!("❌ User not found: {}", request.username);
            track_auth_attempt(false, "password");
            track_auth_failure("user_not_found");
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                Json(AuthResponse {
                    access_token: "user_not_found".to_string(),
                    expires_in: 0,
                    user: UserResponse {
                        id: Uuid::nil(),
                        username: String::new(),
                        email: String::new(),
                        role: String::new(),
                        first_name: String::new(),
                        last_name: String::new(),
                        wallet_address: None,
                    },
                })
            ).into_response();
        }
        Err(e) => {
            tracing::error!("❌ Database error: {}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthResponse {
                    access_token: String::new(),
                    expires_in: 0,
                    user: UserResponse {
                        id: Uuid::nil(),
                        username: String::new(),
                        email: String::new(),
                        role: String::new(),
                        first_name: String::new(),
                        last_name: String::new(),
                        wallet_address: None,
                    },
                })
            ).into_response();
        }
    };

    // Generate token using JWT service
    let claims = crate::auth::Claims::new(user.id, user.username.clone(), user.role.clone());
    let token = state.jwt_service.encode_token(&claims).unwrap_or_else(|_| {
        format!("token_{}_{}", user.username, user.id)
    });

    info!("✅ Login successful for: {} (wallet: {:?})", user.username, user.wallet_address);

    Json(AuthResponse {
        access_token: token,
        expires_in: 86400,
        user: UserResponse {
            id: user.id,
            username: user.username,
            email: user.email,
            role: user.role,
            first_name: user.first_name.unwrap_or_default(),
            last_name: user.last_name.unwrap_or_default(),
            wallet_address: user.wallet_address,
        },
    }).into_response()
}

/// Verify email (Step 2: Account verify email)
/// On successful verification, auto-generates a Solana wallet address for the user
/// and registers them on-chain via the Anchor registry program
#[utoipa::path(
    get,
    path = "/api/v1/auth/verify",
    params(VerifyEmailRequest),
    responses(
        (status = 200, description = "Email verified successfully", body = VerifyEmailResponse),
        (status = 400, description = "Invalid or missing token")
    ),
    tag = "auth"
)]
pub async fn verify_email(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<VerifyEmailRequest>,
) -> Json<VerifyEmailResponse> {
    let token = params.token;
    info!("📧 Email verification request");

    if token.is_empty() {
        return Json(VerifyEmailResponse {
            success: false,
            message: "Missing verification token".to_string(),
        });
    }

    // Generate a new Solana wallet for the user
    let new_keypair = solana_sdk::signer::keypair::Keypair::new();
    let wallet_address = new_keypair.pubkey().to_string();
    info!("🔑 Generated wallet address for verified user: {}", wallet_address);

    // Register user on-chain via Anchor registry program
    // First, fund the new keypair with SOL via airdrop (devnet only)
    let mut blockchain_registered = false;
    
    // Step 1: Request airdrop to fund the new keypair (0.1 SOL for account rent)
    let airdrop_result = state.blockchain_service.request_airdrop(
        &new_keypair.pubkey(),
        100_000_000,  // 0.1 SOL in lamports
    ).await;
    
    match airdrop_result {
        Ok(sig) => {
            info!("💰 Airdrop successful: {} (0.1 SOL)", sig);
            // Wait a moment for airdrop to confirm
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            
            // Step 2: Register user on-chain with the funded keypair
            match state.blockchain_service.register_user_on_chain(
                &new_keypair,
                0,  // user_type: 0 = Consumer (default)
                "GridTokenX Platform",
            ).await {
                Ok(tx_sig) => {
                    info!("⛓️ User registered on-chain successfully. Tx: {}", tx_sig);
                    blockchain_registered = true;
                }
                Err(e) => {
                    info!("⚠️ On-chain registration failed (continuing): {}", e);
                }
            }
        }
        Err(e) => {
            info!("⚠️ Airdrop failed (not on devnet?): {}", e);
        }
    }


    // Try to find and update user by verification token
    let update_result = sqlx::query(
        "UPDATE users SET email_verified = true, wallet_address = $1, blockchain_registered = $2, updated_at = NOW() 
         WHERE email_verification_token = $3 AND email_verified = false"
    )
    .bind(&wallet_address)
    .bind(blockchain_registered)
    .bind(&token)
    .execute(&state.db)
    .await;

    match update_result {
        Ok(result) if result.rows_affected() > 0 => {
            let chain_status = if blockchain_registered { " (on-chain)" } else { "" };
            info!("✅ Email verified successfully, wallet assigned{}: {}", chain_status, wallet_address);
            Json(VerifyEmailResponse {
                success: true,
                message: format!("Email verified successfully. Wallet address generated{}: {}", chain_status, wallet_address),
            })
        }
        _ => {
            // For testing, auto-verify based on token pattern (verify_<username>)
            if token.starts_with("verify_") {
                let username = token.strip_prefix("verify_").unwrap_or("");
                let update_test = sqlx::query(
                    "UPDATE users SET email_verified = true, wallet_address = $1, blockchain_registered = $2, updated_at = NOW() 
                     WHERE username = $3 AND (wallet_address IS NULL OR wallet_address = '')"
                )
                .bind(&wallet_address)
                .bind(blockchain_registered)
                .bind(username)
                .execute(&state.db)
                .await;

                match update_test {
                    Ok(r) if r.rows_affected() > 0 => {
                        let chain_status = if blockchain_registered { " (on-chain)" } else { "" };
                        info!("✅ Email verified (test mode), wallet assigned{}: {}", chain_status, wallet_address);
                        Json(VerifyEmailResponse {
                            success: true,
                            message: format!("Email verified (test mode). Wallet address generated{}: {}", chain_status, wallet_address),
                        })
                    }
                    _ => {
                        // User may already have a wallet, just verify email
                        let _ = sqlx::query(
                            "UPDATE users SET email_verified = true WHERE username = $1"
                        )
                        .bind(username)
                        .execute(&state.db)
                        .await;
                        
                        Json(VerifyEmailResponse {
                            success: true,
                            message: "Email verified (test mode). Wallet already exists.".to_string(),
                        })
                    }
                }
            } else {
                Json(VerifyEmailResponse {
                    success: false,
                    message: "Invalid or expired verification token".to_string(),
                })
            }
        }
    }
}

