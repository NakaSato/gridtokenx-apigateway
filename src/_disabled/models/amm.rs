use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use thiserror::Error;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Error, PartialEq)]
pub enum AmmError {
    #[error("Invalid input token for this pool")]
    InvalidToken,
    #[error("Input amount must be positive")]
    InvalidInputAmount,
    #[error("Insufficient liquidity in pool")]
    InsufficientLiquidity,
}

/// Liquidity Pool model
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct LiquidityPool {
    pub id: Uuid,
    pub name: String,
    pub token_a: String,
    pub token_b: String,
    #[schema(value_type = String)]
    pub reserve_a: Decimal,
    #[schema(value_type = String)]
    pub reserve_b: Decimal,
    #[schema(value_type = String)]
    pub total_supply: Decimal, // Total LP tokens supply
    #[schema(value_type = String)]
    pub fee_rate: Decimal,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to create a new liquidity pool
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreatePoolRequest {
    pub token_a: String,
    pub token_b: String,
    #[schema(value_type = String)]
    pub fee_rate: Decimal,
}

/// Request to add liquidity to a pool
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddLiquidityRequest {
    pub pool_id: Uuid,
    #[schema(value_type = String)]
    pub amount_a: Decimal,
    #[schema(value_type = String)]
    pub amount_b: Decimal,
    #[schema(value_type = Option<String>)]
    pub min_shares: Option<Decimal>, // Slippage protection for LP tokens
}

/// Request to remove liquidity from a pool
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RemoveLiquidityRequest {
    pub pool_id: Uuid,
    #[schema(value_type = String)]
    pub shares: Decimal,
    #[schema(value_type = Option<String>)]
    pub min_amount_a: Option<Decimal>,
    #[schema(value_type = Option<String>)]
    pub min_amount_b: Option<Decimal>,
}

/// Response for liquidity operations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LiquidityOperationResponse {
    pub pool_id: Uuid,
    #[schema(value_type = String)]
    pub shares: Decimal,
    #[schema(value_type = String)]
    pub amount_a: Decimal,
    #[schema(value_type = String)]
    pub amount_b: Decimal,
    #[schema(value_type = String)]
    pub total_supply: Decimal,
}

/// Swap quote response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SwapQuote {
    pub pool_id: Uuid,
    #[schema(value_type = String)]
    pub input_amount: Decimal,
    #[schema(value_type = String)]
    pub output_amount: Decimal,
    #[schema(value_type = String)]
    pub price_impact: Decimal,
    #[schema(value_type = String)]
    pub fee_amount: Decimal,
    #[schema(value_type = String)]
    pub exchange_rate: Decimal,
}

impl LiquidityPool {
    /// Calculate swap output based on Constant Product Formula (x * y = k)
    pub fn calculate_swap(
        &self,
        input_token: &str,
        input_amount: Decimal,
    ) -> Result<SwapQuote, AmmError> {
        // Determine input/output reserves
        let (input_reserve, output_reserve) = if input_token == self.token_a {
            (self.reserve_a, self.reserve_b)
        } else if input_token == self.token_b {
            (self.reserve_b, self.reserve_a)
        } else {
            return Err(AmmError::InvalidToken);
        };

        if input_amount <= Decimal::ZERO {
            return Err(AmmError::InvalidInputAmount);
        }

        if input_reserve <= Decimal::ZERO || output_reserve <= Decimal::ZERO {
            return Err(AmmError::InsufficientLiquidity);
        }

        // Calculate fee
        let fee_amount = input_amount * self.fee_rate;
        let input_with_fee = input_amount - fee_amount;

        // Calculate output: y_out = (y * x_in) / (x + x_in)
        let numerator = output_reserve * input_with_fee;
        let denominator = input_reserve + input_with_fee;

        if denominator == Decimal::ZERO {
            return Err(AmmError::InsufficientLiquidity);
        }

        let output_amount = numerator / denominator;

        // Calculate price impact
        let ideal_price = output_reserve / input_reserve;
        let realized_price = output_amount / input_amount;
        let price_impact = if ideal_price > Decimal::ZERO {
            (Decimal::ONE - (realized_price / ideal_price)) * Decimal::from(100)
        } else {
            Decimal::ZERO
        };

        Ok(SwapQuote {
            pool_id: self.id,
            input_amount,
            output_amount,
            price_impact: price_impact.round_dp(2),
            fee_amount,
            exchange_rate: realized_price,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_calculate_swap_success() {
        let pool = LiquidityPool {
            id: Uuid::new_v4(),
            name: "TokenA-TokenB".to_string(),
            token_a: "TokenA".to_string(),
            token_b: "TokenB".to_string(),
            reserve_a: Decimal::from_str("1000").unwrap(),
            reserve_b: Decimal::from_str("1000").unwrap(),
            total_supply: Decimal::from_str("1000").unwrap(),
            fee_rate: Decimal::from_str("0.003").unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let quote = pool
            .calculate_swap("TokenA", Decimal::from_str("100").unwrap())
            .unwrap();

        // Expected output: ~90.66
        assert!(quote.output_amount > Decimal::from_str("90.6").unwrap());
        assert!(quote.output_amount < Decimal::from_str("90.7").unwrap());
        assert_eq!(quote.fee_amount, Decimal::from_str("0.3").unwrap());
    }

    #[test]
    fn test_calculate_swap_insufficient_liquidity() {
        let pool = LiquidityPool {
            id: Uuid::new_v4(),
            name: "TokenA-TokenB".to_string(),
            token_a: "TokenA".to_string(),
            token_b: "TokenB".to_string(),
            reserve_a: Decimal::ZERO,
            reserve_b: Decimal::ZERO,
            total_supply: Decimal::ZERO,
            fee_rate: Decimal::from_str("0.003").unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let result = pool.calculate_swap("TokenA", Decimal::from_str("100").unwrap());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), AmmError::InsufficientLiquidity);
    }

    #[test]
    fn test_calculate_swap_invalid_token() {
        let pool = LiquidityPool {
            id: Uuid::new_v4(),
            name: "TokenA-TokenB".to_string(),
            token_a: "TokenA".to_string(),
            token_b: "TokenB".to_string(),
            reserve_a: Decimal::from_str("1000").unwrap(),
            reserve_b: Decimal::from_str("1000").unwrap(),
            total_supply: Decimal::from_str("1000").unwrap(),
            fee_rate: Decimal::from_str("0.003").unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let result = pool.calculate_swap("InvalidToken", Decimal::from_str("100").unwrap());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), AmmError::InvalidToken);
    }

    #[test]
    fn test_calculate_swap_zero_input() {
        let pool = LiquidityPool {
            id: Uuid::new_v4(),
            name: "TokenA-TokenB".to_string(),
            token_a: "TokenA".to_string(),
            token_b: "TokenB".to_string(),
            reserve_a: Decimal::from_str("1000").unwrap(),
            reserve_b: Decimal::from_str("1000").unwrap(),
            total_supply: Decimal::from_str("1000").unwrap(),
            fee_rate: Decimal::from_str("0.003").unwrap(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let result = pool.calculate_swap("TokenA", Decimal::ZERO);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), AmmError::InvalidInputAmount);
    }
}
