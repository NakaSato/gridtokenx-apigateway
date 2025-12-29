//! P2P Simulator Proxy
//!
//! This module provides a proxy service to communicate with the Smart Meter Simulator
//! for P2P transaction cost calculations and market pricing data.

use axum::{extract::State, response::Json};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use tracing::{error, instrument};

use crate::error::{ApiError, ErrorCode, Result};
use crate::AppState;

use super::types::{P2PCalculateCostRequest, P2PMarketPrices, P2PTransactionCost};

/// Simulator response for transaction cost calculation
#[derive(Debug, Deserialize)]
struct SimulatorTransactionCost {
    energy_cost: f64,
    wheeling_charge: f64,
    loss_cost: f64,
    total_cost: f64,
    effective_energy: f64,
    loss_factor: f64,
    loss_allocation: String,
    zone_distance_km: f64,
    buyer_zone: i32,
    seller_zone: i32,
    is_grid_compliant: bool,
    grid_violation_reason: Option<String>,
}

/// Simulator response for market prices
#[derive(Debug, Deserialize)]
struct SimulatorMarketPrices {
    base_price_thb_kwh: f64,
    grid_import_price_thb_kwh: f64,
    grid_export_price_thb_kwh: f64,
    loss_allocation_model: String,
    wheeling_charges: HashMap<String, f64>,
    loss_factors: HashMap<String, f64>,
}

/// Get the simulator URL from environment or use default
fn get_simulator_url() -> String {
    std::env::var("SIMULATOR_URL").unwrap_or_else(|_| "http://localhost:8000".to_string())
}

/// Calculate P2P transaction cost by proxying to the simulator
///
/// POST /api/v1/trading/p2p/calculate-cost
#[instrument(skip(_state))]
pub async fn calculate_p2p_cost(
    State(_state): State<AppState>,
    Json(payload): Json<P2PCalculateCostRequest>,
) -> Result<Json<P2PTransactionCost>> {
    let client = Client::new();
    let simulator_url = get_simulator_url();

    // Build request to simulator
    let request_body = serde_json::json!({
        "buyer_zone_id": payload.buyer_zone_id,
        "seller_zone_id": payload.seller_zone_id,
        "energy_amount": payload.energy_amount,
        "agreed_price": payload.agreed_price
    });

    // Call simulator's P2P cost calculation endpoint
    let response = client
        .post(format!("{}/api/v1/p2p/calculate-cost", simulator_url))
        .json(&request_body)
        .send()
        .await
        .map_err(|e| {
            error!("Failed to connect to simulator: {}", e);
            ApiError::with_code(ErrorCode::ExternalServiceError, format!("Failed to connect to simulator: {}", e))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        error!("Simulator returned error: {} - {}", status, error_text);
        return Err(ApiError::with_code(
            ErrorCode::ExternalServiceError,
            format!("Simulator error: {} - {}", status, error_text)
        ));
    }

    let simulator_response: SimulatorTransactionCost = response.json::<SimulatorTransactionCost>().await.map_err(|e| {
        error!("Failed to parse simulator response: {}", e);
        ApiError::with_code(ErrorCode::ExternalServiceError, format!("Failed to parse simulator response: {}", e))
    })?;

    // Convert to our response type
    let cost = P2PTransactionCost {
        energy_cost: simulator_response.energy_cost,
        wheeling_charge: simulator_response.wheeling_charge,
        loss_cost: simulator_response.loss_cost,
        total_cost: simulator_response.total_cost,
        effective_energy: simulator_response.effective_energy,
        loss_factor: simulator_response.loss_factor,
        loss_allocation: simulator_response.loss_allocation,
        zone_distance_km: simulator_response.zone_distance_km,
        buyer_zone: simulator_response.buyer_zone,
        seller_zone: simulator_response.seller_zone,
        is_grid_compliant: simulator_response.is_grid_compliant,
        grid_violation_reason: simulator_response.grid_violation_reason,
    };

    Ok(Json(cost))
}

/// Get current market prices from the simulator
///
/// GET /api/v1/trading/p2p/market-prices
#[instrument(skip(_state))]
pub async fn get_p2p_market_prices(State(_state): State<AppState>) -> Result<Json<P2PMarketPrices>> {
    let client = Client::new();
    let simulator_url = get_simulator_url();

    // Call simulator's market prices endpoint
    let response = client
        .get(format!("{}/api/v1/p2p/market-prices", simulator_url))
        .send()
        .await
        .map_err(|e| {
            error!("Failed to connect to simulator: {}", e);
            ApiError::with_code(ErrorCode::ExternalServiceError, format!("Failed to connect to simulator: {}", e))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        error!("Simulator returned error: {} - {}", status, error_text);
        return Err(ApiError::with_code(
            ErrorCode::ExternalServiceError,
            format!("Simulator error: {} - {}", status, error_text)
        ));
    }

    let simulator_response: SimulatorMarketPrices = response.json::<SimulatorMarketPrices>().await.map_err(|e| {
        error!("Failed to parse simulator response: {}", e);
        ApiError::with_code(ErrorCode::ExternalServiceError, format!("Failed to parse simulator response: {}", e))
    })?;

    // Convert to our response type
    let prices = P2PMarketPrices {
        base_price_thb_kwh: simulator_response.base_price_thb_kwh,
        grid_import_price_thb_kwh: simulator_response.grid_import_price_thb_kwh,
        grid_export_price_thb_kwh: simulator_response.grid_export_price_thb_kwh,
        loss_allocation_model: simulator_response.loss_allocation_model,
        wheeling_charges: simulator_response.wheeling_charges,
        loss_factors: simulator_response.loss_factors,
    };

    Ok(Json(prices))
}
