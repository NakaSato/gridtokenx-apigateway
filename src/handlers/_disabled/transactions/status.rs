//! Transaction Status Types and Utilities
//!
//! This module provides helper types and mapping functions for transaction status handling.

use crate::models::transaction::{TransactionStatus, TransactionType};

/// Map order status string to TransactionStatus
pub fn map_order_status(status: &str) -> TransactionStatus {
    match status {
        "pending" => TransactionStatus::Pending,
        "filled" => TransactionStatus::Settled,
        "cancelled" => TransactionStatus::Failed,
        "partially_filled" => TransactionStatus::Processing,
        _ => TransactionStatus::Pending,
    }
}

/// Map blockchain instruction name to TransactionType
pub fn map_instruction_to_type(instruction: Option<&str>) -> TransactionType {
    match instruction {
        Some("place_order") => TransactionType::EnergyTrade,
        Some("swap") => TransactionType::Swap,
        Some("mint") => TransactionType::TokenMint,
        Some("transfer") => TransactionType::TokenTransfer,
        Some("vote") => TransactionType::GovernanceVote,
        _ => TransactionType::RegistryUpdate,
    }
}

/// Map blockchain status string to TransactionStatus
pub fn map_blockchain_status(status: &str) -> TransactionStatus {
    match status {
        "Confirmed" | "Finalized" => TransactionStatus::Confirmed,
        "Failed" => TransactionStatus::Failed,
        _ => TransactionStatus::Pending,
    }
}
