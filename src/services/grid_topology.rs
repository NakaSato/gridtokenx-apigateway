use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;

/// Service to manage grid topology and calculate transmission costs
#[derive(Clone, Debug)]
pub struct GridTopologyService;

impl GridTopologyService {
    pub fn new() -> Self {
        Self
    }

    /// Calculate wheeling charge (transmission fee) in THB per kWh
    /// returns: Fee in THB
    pub fn calculate_wheeling_charge(&self, from_zone: Option<i32>, to_zone: Option<i32>) -> Decimal {
        match (from_zone, to_zone) {
            (Some(mz), Some(bz)) => {
                if mz == bz {
                    // Local distribution fee only
                    Decimal::from_f64(0.50).expect("hardcoded decimal 0.50")
                } else {
                    let distance = (mz - bz).abs();
                    if distance == 1 {
                        // Adjacent zone
                        Decimal::from_f64(1.00).expect("hardcoded decimal 1.00")
                    } else {
                        // Cross-zone transmission
                        Decimal::from_f64(1.50).expect("hardcoded decimal 1.50") + Decimal::from(distance) * Decimal::from_f64(0.1).expect("hardcoded decimal 0.1")
                    }
                }
            }
            _ => {
                // Default high fee if zones unknown
                Decimal::from_f64(2.00).expect("hardcoded decimal 2.00")
            }
        }
    }

    /// Calculate technical loss (%)
    /// returns: Percentage as Decimal (e.g., 0.03 for 3%)
    pub fn calculate_loss_factor(&self, from_zone: Option<i32>, to_zone: Option<i32>) -> Decimal {
        match (from_zone, to_zone) {
            (Some(mz), Some(bz)) => {
                if mz == bz {
                    // Minimal local loss
                    Decimal::from_f64(0.01).expect("hardcoded decimal 0.01")
                } else {
                    let distance = (mz - bz).abs();
                    if distance == 1 {
                        Decimal::from_f64(0.03).expect("hardcoded decimal 0.03")
                    } else {
                        // Max cap at 15%
                        let loss = 0.03 + (distance as f64 * 0.01);
                        Decimal::from_f64(loss.min(0.15)).expect("loss calculation overflow")
                    }
                }
            }
            _ => {
                // Conservative default
                Decimal::from_f64(0.05).expect("hardcoded decimal 0.05")
            }
        }
    }

    /// Calculate actual cost of losses for a given energy amount and price
    /// Loss Cost = Energy * LossFactor * EnergyPrice
    pub fn calculate_loss_cost(&self, energy_amount: Decimal, price: Decimal, loss_factor: Decimal) -> Decimal {
        energy_amount * price * loss_factor
    }
}
