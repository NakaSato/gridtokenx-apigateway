pub struct TokenMath;

impl TokenMath {
    pub fn kwh_to_lamports(amount_kwh: f64) -> u64 {
        (amount_kwh * 1_000_000_000.0) as u64
    }

    pub fn lamports_to_kwh(lamports: u64) -> f64 {
        lamports as f64 / 1_000_000_000.0
    }
}
