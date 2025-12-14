pub mod types;

use anyhow::Result;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use sqlx::PgPool;
use tracing::{info, instrument};
use uuid::Uuid;

use crate::error::ApiError;

use crate::models::amm::{
    AddLiquidityRequest, CreatePoolRequest, LiquidityOperationResponse, LiquidityPool,
    RemoveLiquidityRequest, SwapQuote,
};

pub use types::*;

#[derive(Clone)]
pub struct AmmService {
    db: PgPool,
}

impl AmmService {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Get a liquidity pool by ID
    pub async fn get_pool(&self, pool_id: Uuid) -> Result<LiquidityPool, ApiError> {
        sqlx::query_as::<_, LiquidityPool>(
            r#"
            SELECT id, name, token_a, token_b, reserve_a, reserve_b, total_supply, fee_rate, created_at, updated_at
            FROM liquidity_pools
            WHERE id = $1
            "#,
        )
        .bind(pool_id)
        .fetch_optional(&self.db)
        .await
        .map_err(ApiError::Database)?
        .ok_or_else(|| ApiError::NotFound("Liquidity pool not found".to_string()))
    }

    /// List all available liquidity pools
    pub async fn list_pools(&self) -> Result<Vec<LiquidityPool>, ApiError> {
        sqlx::query_as::<_, LiquidityPool>(
            r#"
            SELECT id, name, token_a, token_b, reserve_a, reserve_b, total_supply, fee_rate, created_at, updated_at
            FROM liquidity_pools
            ORDER BY name ASC
            "#,
        )
        .fetch_all(&self.db)
        .await
        .map_err(ApiError::Database)
    }

    /// Create a new liquidity pool
    pub async fn create_pool(&self, request: CreatePoolRequest) -> Result<LiquidityPool, ApiError> {
        let pool_id = Uuid::new_v4();
        let name = format!("{}-{}", request.token_a, request.token_b);

        sqlx::query_as::<_, LiquidityPool>(
            r#"
            INSERT INTO liquidity_pools (
                id, name, token_a, token_b, reserve_a, reserve_b, total_supply, fee_rate, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, 0, 0, 0, $5, NOW(), NOW())
            RETURNING id, name, token_a, token_b, reserve_a, reserve_b, total_supply, fee_rate, created_at, updated_at
            "#,
        )
        .bind(pool_id)
        .bind(name)
        .bind(request.token_a)
        .bind(request.token_b)
        .bind(request.fee_rate)
        .fetch_one(&self.db)
        .await
        .map_err(|e| {
            if e.to_string().contains("unique constraint") {
                ApiError::BadRequest("Pool already exists for these tokens".to_string())
            } else {
                ApiError::Database(e)
            }
        })
    }

    /// Add liquidity to a pool
    pub async fn add_liquidity(
        &self,
        request: AddLiquidityRequest,
    ) -> Result<LiquidityOperationResponse, ApiError> {
        let mut tx = self.db.begin().await.map_err(ApiError::Database)?;

        // Lock pool for update
        let pool = sqlx::query_as::<_, LiquidityPool>(
            r#"
            SELECT id, name, token_a, token_b, reserve_a, reserve_b, total_supply, fee_rate, created_at, updated_at
            FROM liquidity_pools
            WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(request.pool_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::Database)?
        .ok_or_else(|| ApiError::NotFound("Liquidity pool not found".to_string()))?;

        // Calculate shares to mint
        let shares = if pool.total_supply == Decimal::ZERO {
            // Initial liquidity: sqrt(a * b)
            let product = request.amount_a * request.amount_b;
            product.sqrt().unwrap_or(Decimal::ZERO)
        } else {
            // Subsequent liquidity: min(a * supply / reserve_a, b * supply / reserve_b)
            let share_a = (request.amount_a * pool.total_supply) / pool.reserve_a;
            let share_b = (request.amount_b * pool.total_supply) / pool.reserve_b;
            share_a.min(share_b)
        };

        if let Some(min_shares) = request.min_shares {
            if shares < min_shares {
                return Err(ApiError::BadRequest(format!(
                    "Slippage tolerance exceeded. Shares {} < min {}",
                    shares, min_shares
                )));
            }
        }

        if shares <= Decimal::ZERO {
            return Err(ApiError::BadRequest(
                "Insufficient liquidity added".to_string(),
            ));
        }

        // Update pool
        let new_reserve_a = pool.reserve_a + request.amount_a;
        let new_reserve_b = pool.reserve_b + request.amount_b;
        let new_total_supply = pool.total_supply + shares;

        sqlx::query(
            r#"
            UPDATE liquidity_pools
            SET reserve_a = $1, reserve_b = $2, total_supply = $3, updated_at = NOW()
            WHERE id = $4
            "#,
        )
        .bind(new_reserve_a)
        .bind(new_reserve_b)
        .bind(new_total_supply)
        .bind(request.pool_id)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::Database)?;

        tx.commit().await.map_err(ApiError::Database)?;

        Ok(LiquidityOperationResponse {
            pool_id: request.pool_id,
            shares,
            amount_a: request.amount_a,
            amount_b: request.amount_b,
            total_supply: new_total_supply,
        })
    }

    /// Remove liquidity from a pool
    pub async fn remove_liquidity(
        &self,
        request: RemoveLiquidityRequest,
    ) -> Result<LiquidityOperationResponse, ApiError> {
        let mut tx = self.db.begin().await.map_err(ApiError::Database)?;

        // Lock pool for update
        let pool = sqlx::query_as::<_, LiquidityPool>(
            r#"
            SELECT id, name, token_a, token_b, reserve_a, reserve_b, total_supply, fee_rate, created_at, updated_at
            FROM liquidity_pools
            WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(request.pool_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::Database)?
        .ok_or_else(|| ApiError::NotFound("Liquidity pool not found".to_string()))?;

        if request.shares <= Decimal::ZERO || request.shares > pool.total_supply {
            return Err(ApiError::BadRequest("Invalid share amount".to_string()));
        }

        // Calculate amounts to return
        let amount_a = (request.shares * pool.reserve_a) / pool.total_supply;
        let amount_b = (request.shares * pool.reserve_b) / pool.total_supply;

        if let Some(min_a) = request.min_amount_a {
            if amount_a < min_a {
                return Err(ApiError::BadRequest(format!(
                    "Slippage tolerance exceeded. Amount A {} < min {}",
                    amount_a, min_a
                )));
            }
        }

        if let Some(min_b) = request.min_amount_b {
            if amount_b < min_b {
                return Err(ApiError::BadRequest(format!(
                    "Slippage tolerance exceeded. Amount B {} < min {}",
                    amount_b, min_b
                )));
            }
        }

        // Update pool
        let new_reserve_a = pool.reserve_a - amount_a;
        let new_reserve_b = pool.reserve_b - amount_b;
        let new_total_supply = pool.total_supply - request.shares;

        sqlx::query(
            r#"
            UPDATE liquidity_pools
            SET reserve_a = $1, reserve_b = $2, total_supply = $3, updated_at = NOW()
            WHERE id = $4
            "#,
        )
        .bind(new_reserve_a)
        .bind(new_reserve_b)
        .bind(new_total_supply)
        .bind(request.pool_id)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::Database)?;

        tx.commit().await.map_err(ApiError::Database)?;

        Ok(LiquidityOperationResponse {
            pool_id: request.pool_id,
            shares: request.shares,
            amount_a,
            amount_b,
            total_supply: new_total_supply,
        })
    }

    /// Calculate swap output based on Constant Product Formula (x * y = k)
    pub async fn calculate_swap_output(
        &self,
        pool_id: Uuid,
        input_token: &str,
        input_amount: Decimal,
    ) -> Result<SwapQuote, ApiError> {
        let pool = self.get_pool(pool_id).await?;

        pool.calculate_swap(input_token, input_amount)
            .map_err(|e| ApiError::BadRequest(e.to_string()))
    }

    /// Execute a swap transaction
    #[instrument(skip(self))]
    pub async fn execute_swap(
        &self,
        user_id: Uuid,
        pool_id: Uuid,
        input_token: String,
        input_amount: Decimal,
        min_output_amount: Decimal,
    ) -> Result<SwapTransaction, ApiError> {
        let mut tx = self.db.begin().await.map_err(ApiError::Database)?;

        // Lock pool row for update
        let pool = sqlx::query_as::<_, LiquidityPool>(
            r#"
            SELECT id, name, token_a, token_b, reserve_a, reserve_b, total_supply, fee_rate, created_at, updated_at
            FROM liquidity_pools
            WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(pool_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::Database)?
        .ok_or_else(|| ApiError::NotFound("Liquidity pool not found".to_string()))?;

        if input_amount <= Decimal::ZERO {
            return Err(ApiError::BadRequest(
                "Input amount must be positive".to_string(),
            ));
        }

        // Calculate swap using model logic
        let quote = pool
            .calculate_swap(&input_token, input_amount)
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;

        // Slippage check
        if quote.output_amount < min_output_amount {
            return Err(ApiError::BadRequest(format!(
                "Slippage tolerance exceeded. Output {} < min {}",
                quote.output_amount, min_output_amount
            )));
        }

        // Determine which reserve to update
        let (new_reserve_a, new_reserve_b) = if input_token == pool.token_a {
            (
                pool.reserve_a + input_amount,
                pool.reserve_b - quote.output_amount,
            )
        } else {
            (
                pool.reserve_a - quote.output_amount,
                pool.reserve_b + input_amount,
            )
        };

        // Update pool in DB
        // Note: total_supply doesn't change for swaps
        sqlx::query(
            r#"
            UPDATE liquidity_pools
            SET reserve_a = $1, reserve_b = $2, updated_at = NOW()
            WHERE id = $3
            "#,
        )
        .bind(new_reserve_a)
        .bind(new_reserve_b)
        .bind(pool_id)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::Database)?;

        // Record transaction
        let swap_tx = sqlx::query_as::<_, SwapTransaction>(
            r#"
            INSERT INTO swap_transactions (
                id, pool_id, user_id, input_token, input_amount, output_token, output_amount, fee_amount,
                status, slippage_tolerance, tx_hash, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'completed', NULL, NULL, NOW())
            RETURNING id, pool_id, user_id, input_token, input_amount, output_token, output_amount, fee_amount,
                      status, slippage_tolerance, tx_hash, created_at
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(pool_id)
        .bind(user_id)
        .bind(input_token.clone())
        .bind(input_amount)
        .bind(if input_token == pool.token_a {
            pool.token_b
        } else {
            pool.token_a
        })
        .bind(quote.output_amount)
        .bind(quote.fee_amount)
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::Database)?;

        // Commit transaction
        tx.commit().await.map_err(ApiError::Database)?;

        info!(
            "Swap executed successfully: {} -> {}",
            swap_tx.id, quote.output_amount
        );

        Ok(swap_tx)
    }

    /// Get user swap history
    pub async fn get_user_swap_history(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<SwapTransaction>, ApiError> {
        sqlx::query_as::<_, SwapTransaction>(
            r#"
            SELECT id, pool_id, user_id, input_token, input_amount, output_token, output_amount, fee_amount, 
                   status, slippage_tolerance, tx_hash, created_at
            FROM swap_transactions
            WHERE user_id = $1
            ORDER BY created_at DESC
            LIMIT 50
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db)
        .await
        .map_err(ApiError::Database)
    }
}
