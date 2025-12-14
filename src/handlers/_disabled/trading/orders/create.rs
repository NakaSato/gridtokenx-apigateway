use axum::{extract::State, response::Json};
use base64::{engine::general_purpose, Engine as _};
use chrono::Utc;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{Keypair, Signer};

use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::database::schema::types::{OrderSide, OrderStatus, OrderType};
use crate::error::{ApiError, Result};
use crate::models::trading::CreateOrderRequest;
use crate::services::AuditEvent;
use crate::AppState;

use crate::handlers::trading::types::CreateOrderResponse;

/// Create a new trading order
/// POST /api/trading/orders
#[utoipa::path(
    post,
    path = "/api/trading/orders",
    tag = "trading",
    request_body = CreateOrderRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Order created successfully", body = CreateOrderResponse),
        (status = 400, description = "Invalid order parameters"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_order(
    State(_state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<CreateOrderRequest>,
) -> Result<Json<CreateOrderResponse>> {
    tracing::info!("Creating trading order for user: {}", user.0.sub);

    // Validate energy amount
    if payload.energy_amount <= rust_decimal::Decimal::ZERO {
        return Err(ApiError::BadRequest(
            "Energy amount must be positive".to_string(),
        ));
    }

    // Validate price based on order type
    let price_per_kwh_bd = match payload.order_type {
        OrderType::Limit => {
            let price = payload.price_per_kwh.ok_or_else(|| {
                ApiError::BadRequest("Price per kWh is required for Limit orders".to_string())
            })?;
            if price <= rust_decimal::Decimal::ZERO {
                return Err(ApiError::BadRequest(
                    "Price per kWh must be positive".to_string(),
                ));
            }
            price
        }
        OrderType::Market => {
            // For market orders, price is not set (or ignored)
            rust_decimal::Decimal::ZERO
        }
    };

    // Create trading order
    let order_id = Uuid::new_v4();
    let now = Utc::now();
    let expires_at = payload
        .expiry_time
        .unwrap_or_else(|| now + chrono::Duration::days(1));

    // Get or create current epoch for Market Clearing Engine
    let epoch = _state
        .market_clearing
        .get_or_create_epoch(now)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get/create epoch: {}", e);
            ApiError::Internal("Failed to assign order to epoch".to_string())
        })?;

    tracing::info!(
        "Assigning order {} to epoch {} (number: {})",
        order_id,
        epoch.id,
        epoch.epoch_number
    );

    let energy_amount_bd = payload.energy_amount;
    let filled_amount_bd = rust_decimal::Decimal::ZERO;

    sqlx::query!(
        r#"
        INSERT INTO trading_orders (
            id, user_id, order_type, side, energy_amount, price_per_kwh,
            filled_amount, status, expires_at, created_at, epoch_id
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        "#,
        order_id,
        user.0.sub,
        payload.order_type as OrderType,
        payload.side as OrderSide,
        energy_amount_bd,
        price_per_kwh_bd,
        filled_amount_bd,
        OrderStatus::Pending as OrderStatus,
        expires_at,
        now,
        epoch.id
    )
    .execute(&_state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create trading order: {}", e);
        ApiError::Database(e)
    })?;

    // Log order creation to audit logs
    _state.audit_logger.log_async(AuditEvent::OrderCreated {
        user_id: user.0.sub,
        order_id,
        order_type: format!("{:?}", payload.side),
        amount: payload.energy_amount.to_string(),
        price: price_per_kwh_bd.to_string(),
    });

    // ========================================================================
    // ON-CHAIN ORDER CREATION (Added to fix P2P Settlement)
    // ========================================================================
    let (signature, order_pda) = if _state.config.tokenization.enable_real_blockchain {
        // Fetch user keys to sign the transaction
        let db_user = sqlx::query!(
            "SELECT wallet_address, encrypted_private_key, wallet_salt, encryption_iv FROM users WHERE id = $1",
            user.0.sub
        )
        .fetch_optional(&_state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch user keys: {}", e);
            ApiError::Database(e)
        })?
        .ok_or_else(|| ApiError::Internal("User data not found".to_string()))?;

        let keypair = if let (Some(enc_key), Some(iv), Some(salt)) = (
            db_user.encrypted_private_key,
            db_user.encryption_iv,
            db_user.wallet_salt,
        ) {
            tracing::info!(
                "User {} has encrypted keys, proceeding with on-chain order creation",
                user.0.sub
            );

            let master_secret = std::env::var("WALLET_MASTER_SECRET")
                .unwrap_or_else(|_| "dev-secret-key".to_string());

            // Encode bytea to Base64 strings for WalletService
            let enc_key_b64 = general_purpose::STANDARD.encode(enc_key);
            let iv_b64 = general_purpose::STANDARD.encode(iv);
            let salt_b64 = general_purpose::STANDARD.encode(salt);

            // Decrypt private key
            let private_key_bytes = crate::services::WalletService::decrypt_private_key(
                &master_secret,
                &enc_key_b64,
                &salt_b64,
                &iv_b64,
            )
            .map_err(|e| {
                tracing::error!("Failed to decrypt user key: {}", e);
                ApiError::Internal(format!("Failed to decrypt key: {}", e))
            })?;

            // Keypair::from_bytes seems missing, use base58 workaround
            let keypair =
                Keypair::from_base58_string(&bs58::encode(&private_key_bytes).into_string());
            keypair
        } else {
            // LAZY WALLET GENERATION
            tracing::info!("User {} missing keys, generating new wallet...", user.0.sub);

            let master_secret = std::env::var("WALLET_MASTER_SECRET")
                .unwrap_or_else(|_| "dev-secret-key".to_string());
            let new_keypair = Keypair::new();
            let pubkey = new_keypair.pubkey().to_string();

            let (enc_key_b64, salt_b64, iv_b64) =
                crate::services::WalletService::encrypt_private_key(
                    &master_secret,
                    &new_keypair.to_bytes(),
                )
                .map_err(|e| {
                    tracing::error!("Failed to encrypt new key: {}", e);
                    ApiError::Internal(format!("Key encryption failed: {}", e))
                })?;

            // Decode b64 to bytes for DB storage
            let enc_key_bytes = general_purpose::STANDARD
                .decode(&enc_key_b64)
                .unwrap_or_default();
            let salt_bytes = general_purpose::STANDARD
                .decode(&salt_b64)
                .unwrap_or_default();
            let iv_bytes = general_purpose::STANDARD
                .decode(&iv_b64)
                .unwrap_or_default();

            // Update user record with new wallet
            sqlx::query!(
                 "UPDATE users SET wallet_address=$1, encrypted_private_key=$2, wallet_salt=$3, encryption_iv=$4 WHERE id=$5",
                 pubkey, enc_key_bytes, salt_bytes, iv_bytes, user.0.sub
            )
            .execute(&_state.db)
            .await
            .map_err(|e| {
                 tracing::error!("Failed to persist new wallet: {}", e);
                 ApiError::Database(e)
            })?;

            // Request Airdrop to fund the new wallet
            if let Err(e) = _state
                .wallet_service
                .request_airdrop(&new_keypair.pubkey(), 2.0)
                .await
            {
                tracing::error!("Failed to request airdrop: {}", e);
                return Err(ApiError::Internal(format!(
                    "Failed to fund new wallet: {}",
                    e
                )));
            }

            tracing::info!("Generated and persisted new wallet for user {}", user.0.sub);
            new_keypair
        };

        // Proceed with on-chain creation using `keypair` (from either branch)
        // Derive Market PDA
        let trading_program_id = _state
            .blockchain_service
            .trading_program_id()
            .map_err(|e| ApiError::Internal(format!("Failed to get trading program ID: {}", e)))?;
        let (market_pda, _) = Pubkey::find_program_address(&[b"market"], &trading_program_id);

        // Convert decimals to u64
        let amount_str = payload.energy_amount.to_string();
        let amount_u64 = amount_str
            .split('.')
            .next()
            .unwrap_or("0")
            .parse::<u64>()
            .unwrap_or(0);

        let price_str = payload
            .price_per_kwh
            .unwrap_or(rust_decimal::Decimal::ZERO)
            .to_string();
        let price_u64 = price_str
            .split('.')
            .next()
            .unwrap_or("0")
            .parse::<u64>()
            .unwrap_or(0);

        // Execute Transaction
        let (signature, order_pda) = _state
            .blockchain_service
            .execute_create_order(
                &keypair,
                &market_pda.to_string(),
                amount_u64,
                price_u64,
                match payload.side {
                    OrderSide::Buy => "buy",
                    OrderSide::Sell => "sell",
                },
                None, // Certificate ID
            )
            .await
            .map_err(|e| {
                tracing::error!("On-chain order creation failed: {}", e);
                ApiError::Internal(format!("On-chain failure: {}", e))
            })?;

        tracing::info!("On-chain order created. Signature: {}, PDA: {}", signature, order_pda);
        (signature.to_string(), Some(order_pda))
    } else {
        tracing::info!("Mocking on-chain order creation for user: {}", user.0.sub);
        (format!("mock_order_sig_{}", order_id), None)
    };

    // Update DB with signature and PDA
    // Using sqlx::query instead of macro because we are in offline mode and 
    // the new query is not in sqlx-data.json yet.
    if let Some(pda) = order_pda {
        sqlx::query(
            "UPDATE trading_orders SET blockchain_tx_signature = $1, order_pda = $2 WHERE id = $3",
        )
        .bind(&signature)
        .bind(pda)
        .bind(order_id)
        .execute(&_state.db)
        .await
        .map_err(|e| ApiError::Database(e))?;
    } else {
         sqlx::query(
            "UPDATE trading_orders SET blockchain_tx_signature = $1 WHERE id = $2",
        )
        .bind(&signature)
        .bind(order_id)
        .execute(&_state.db)
        .await
        .map_err(|e| ApiError::Database(e))?;
    }

    let signature_str = Some(signature);

    Ok(Json(CreateOrderResponse {
        id: order_id,
        status: OrderStatus::Pending,
        created_at: now,
        // Append signature info to message?
        message: format!(
            "Order created successfully and assigned to epoch {} for matching. Tx: {:?}",
            epoch.epoch_number, signature_str
        ),
    }))
}
