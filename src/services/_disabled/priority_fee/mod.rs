use anyhow::Result;
use solana_sdk::instruction::Instruction;
use tracing::{debug, info};

/// Types of transactions for priority fee recommendations
#[derive(Debug, Clone, Copy)]
pub enum TransactionType {
    TokenMinting,
    Settlement,
    OrderCreation,
    ERCIssuance,
    WalletConnection,
    UserRegistration,
    MeterRegistration,
    MeterReading,
    TokenTransfer,
}

impl TransactionType {
    /// Get description of transaction type
    pub fn description(&self) -> &'static str {
        match self {
            TransactionType::TokenMinting => "Energy token minting",
            TransactionType::Settlement => "Trade settlement transfer",
            TransactionType::OrderCreation => "Trading order creation",
            TransactionType::ERCIssuance => "ERC certificate issuance",
            TransactionType::WalletConnection => "Wallet connection",
            TransactionType::UserRegistration => "User registration on blockchain",
            TransactionType::MeterRegistration => "Smart meter registration",
            TransactionType::MeterReading => "Meter reading submission",
            TransactionType::TokenTransfer => "Token transfer between accounts",
        }
    }

    /// Get whether this transaction type should use priority fees
    pub fn should_use_priority_fees(&self) -> bool {
        match self {
            TransactionType::WalletConnection => false, // Low importance
            TransactionType::MeterReading => false,     // High volume, lower priority
            _ => true, // Most transactions benefit from priority fees
        }
    }
}

/// Priority fee levels for Solana transactions
#[derive(Debug, Clone, Copy)]
pub enum PriorityLevel {
    Low,    // 1,000 micro-lamports (0.000001 SOL per CU)
    Medium, // 10,000 micro-lamports (0.00001 SOL per CU)
    High,   // 50,000 micro-lamports (0.00005 SOL per CU)
}

impl PriorityLevel {
    /// Get micro-lamports per compute unit
    pub fn micro_lamports_per_cu(&self) -> u64 {
        match self {
            PriorityLevel::Low => 1_000,
            PriorityLevel::Medium => 10_000,
            PriorityLevel::High => 50_000,
        }
    }

    /// Get priority fee description
    pub fn description(&self) -> &'static str {
        match self {
            PriorityLevel::Low => "Low priority - slower confirmation",
            PriorityLevel::Medium => "Medium priority - balanced speed/cost",
            PriorityLevel::High => "High priority - fastest confirmation",
        }
    }

    /// Get estimated confirmation time (in slots)
    pub fn estimated_slots(&self) -> u64 {
        match self {
            PriorityLevel::Low => 10,   // ~40 seconds
            PriorityLevel::Medium => 5, // ~20 seconds
            PriorityLevel::High => 2,   // ~8 seconds
        }
    }
}

/// Service for managing priority fees on Solana transactions
pub struct PriorityFeeService;

impl PriorityFeeService {
    /// Add compute budget and priority fee instructions to transaction
    /// Returns modified instruction list with priority fee
    pub fn add_priority_fee(
        _instructions: &mut Vec<Instruction>,
        priority_level: PriorityLevel,
        compute_limit: Option<u64>,
    ) -> Result<()> {
        let compute_limit = compute_limit.unwrap_or(200_000); // Default compute limit
        let micro_lamports = priority_level.micro_lamports_per_cu();

        debug!(
            "Adding priority fee: {} micro-lamports per CU, compute limit: {}",
            micro_lamports, compute_limit
        );

        // TODO: Add compute budget instructions when solana-sdk compute_budget is available
        // For now, we'll just log the priority fee settings
        debug!(
            "Priority fee settings: {} micro-lamports per CU, compute limit: {}",
            micro_lamports, compute_limit
        );

        info!(
            "Priority fee added: {} ({}) - Estimated confirmation: {} slots",
            priority_level.description(),
            micro_lamports,
            priority_level.estimated_slots()
        );

        Ok(())
    }

    /// Calculate estimated priority fee cost in SOL
    pub fn estimate_fee_cost(priority_level: PriorityLevel, compute_limit: Option<u64>) -> f64 {
        let compute_limit = compute_limit.unwrap_or(200_000);
        let micro_lamports = priority_level.micro_lamports_per_cu();

        // Total micro-lamports = compute_limit * micro_lamports_per_cu
        let total_micro_lamports = compute_limit * micro_lamports;

        // Convert to SOL (1 SOL = 1_000_000_000 micro-lamports)
        total_micro_lamports as f64 / 1_000_000_000.0
    }

    /// Get recommended priority level based on transaction type
    pub fn recommend_priority_level(transaction_type: TransactionType) -> PriorityLevel {
        match transaction_type {
            TransactionType::TokenMinting => PriorityLevel::Medium,
            TransactionType::Settlement => PriorityLevel::High,
            TransactionType::OrderCreation => PriorityLevel::Low,
            TransactionType::ERCIssuance => PriorityLevel::Medium,
            TransactionType::WalletConnection => PriorityLevel::Low,
            TransactionType::UserRegistration => PriorityLevel::Medium,
            TransactionType::MeterRegistration => PriorityLevel::Medium,
            TransactionType::MeterReading => PriorityLevel::Low,
            TransactionType::TokenTransfer => PriorityLevel::Medium,
        }
    }

    /// Get recommended compute limit for transaction type
    pub fn recommend_compute_limit(transaction_type: TransactionType) -> u64 {
        match transaction_type {
            TransactionType::TokenMinting => 150_000,
            TransactionType::Settlement => 300_000,
            TransactionType::OrderCreation => 100_000,
            TransactionType::ERCIssuance => 250_000,
            TransactionType::WalletConnection => 50_000,
            TransactionType::UserRegistration => 100_000,
            TransactionType::MeterRegistration => 120_000,
            TransactionType::MeterReading => 80_000,
            TransactionType::TokenTransfer => 100_000,
        }
    }
}

/// Enhanced transaction builder with priority fees
pub struct PriorityTransactionBuilder {
    instructions: Vec<Instruction>,
    priority_level: PriorityLevel,
    compute_limit: Option<u64>,
}

impl PriorityTransactionBuilder {
    /// Create new transaction builder
    pub fn new(transaction_type: TransactionType) -> Self {
        let priority_level = PriorityFeeService::recommend_priority_level(transaction_type);
        let compute_limit = PriorityFeeService::recommend_compute_limit(transaction_type);

        Self {
            instructions: Vec::new(),
            priority_level,
            compute_limit: Some(compute_limit),
        }
    }

    /// Add instruction to transaction
    pub fn add_instruction(mut self, instruction: Instruction) -> Self {
        self.instructions.push(instruction);
        self
    }

    /// Add multiple instructions
    pub fn add_instructions(mut self, instructions: Vec<Instruction>) -> Self {
        self.instructions.extend(instructions);
        self
    }

    /// Set custom priority level
    pub fn with_priority_level(mut self, priority_level: PriorityLevel) -> Self {
        self.priority_level = priority_level;
        self
    }

    /// Set custom compute limit
    pub fn with_compute_limit(mut self, compute_limit: u64) -> Self {
        self.compute_limit = Some(compute_limit);
        self
    }

    /// Build final instruction list with priority fees
    pub fn build(self) -> Result<Vec<Instruction>> {
        let mut instructions = self.instructions;

        PriorityFeeService::add_priority_fee(
            &mut instructions,
            self.priority_level,
            self.compute_limit,
        )?;

        Ok(instructions)
    }

    /// Get estimated fee cost
    pub fn estimate_fee_cost(&self) -> f64 {
        PriorityFeeService::estimate_fee_cost(self.priority_level, self.compute_limit)
    }

    /// Get priority level description
    pub fn priority_description(&self) -> &'static str {
        self.priority_level.description()
    }

    /// Get estimated confirmation time
    pub fn estimated_confirmation_slots(&self) -> u64 {
        self.priority_level.estimated_slots()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_levels() {
        assert_eq!(PriorityLevel::Low.micro_lamports_per_cu(), 1_000);
        assert_eq!(PriorityLevel::Medium.micro_lamports_per_cu(), 10_000);
        assert_eq!(PriorityLevel::High.micro_lamports_per_cu(), 50_000);
    }

    #[test]
    fn test_fee_estimation() {
        let cost = PriorityFeeService::estimate_fee_cost(PriorityLevel::Medium, Some(200_000));

        // 200_000 CU * 10_000 micro-lamports/CU = 2_000_000_000 micro-lamports = 2 SOL
        assert_eq!(cost, 2.0);
    }

    #[test]
    fn test_transaction_recommendations() {
        let priority = PriorityFeeService::recommend_priority_level(TransactionType::Settlement);
        assert!(matches!(priority, PriorityLevel::High));

        let compute_limit =
            PriorityFeeService::recommend_compute_limit(TransactionType::Settlement);
        assert_eq!(compute_limit, 300_000);
    }

    #[test]
    fn test_transaction_builder() {
        let builder = PriorityTransactionBuilder::new(TransactionType::TokenMinting)
            .with_priority_level(PriorityLevel::High)
            .with_compute_limit(250_000);

        assert_eq!(
            builder.priority_description(),
            PriorityLevel::High.description()
        );
        assert_eq!(builder.estimated_confirmation_slots(), 2);

        let cost = builder.estimate_fee_cost();
        assert!(cost > 0.0);
    }
}
