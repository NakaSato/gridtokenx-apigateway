use axum::{extract::State, response::IntoResponse};
use crate::app_state::AppState;

/// Expose Prometheus metrics
pub async fn get_metrics(State(state): State<AppState>) -> impl IntoResponse {
    state.metrics_handle.render()
}
