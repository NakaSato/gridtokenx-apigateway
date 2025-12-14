use axum::{extract::State, response::Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, ErrorResponse};
use crate::services::event_processor::ReplayStatus;
use crate::AppState;

/// Request payload for event replay
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ReplayEventsRequest {
    pub start_slot: u64,
    pub end_slot: Option<u64>,
}

/// Response for event replay trigger
#[derive(Debug, Serialize, ToSchema)]
pub struct ReplayEventsResponse {
    pub message: String,
    pub job_id: String,
}

/// Trigger event replay
#[utoipa::path(
    post,
    path = "/api/admin/event-processor/replay",
    request_body = ReplayEventsRequest,
    responses(
        (status = 200, description = "Event replay triggered", body = ReplayEventsResponse),
        (status = 403, description = "Admin access required"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Admin - Event Processor",
    security(("bearer_auth" = []))
)]
pub async fn trigger_event_replay(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Json(payload): Json<ReplayEventsRequest>,
) -> Result<Json<ReplayEventsResponse>, ApiError> {
    tracing::info!("Triggering event replay: {:?}", payload);

    match state
        .event_processor
        .replay_events(payload.start_slot, payload.end_slot)
        .await
    {
        Ok(message) => {
            let response = ReplayEventsResponse {
                message,
                job_id: uuid::Uuid::new_v4().to_string(),
            };
            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!("Failed to trigger event replay: {}", e);
            Err(ApiError::Internal(e.to_string()))
        }
    }
}

/// Get event replay status
#[utoipa::path(
    get,
    path = "/api/admin/event-processor/replay",
    tag = "Admin",
    responses(
        (status = 200, description = "Replay status retrieved successfully", body = Option<ReplayStatus>),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    security(
        ("api_key" = [])
    )
)]
pub async fn get_replay_status(
    State(state): State<AppState>,
) -> std::result::Result<
    Json<Option<crate::services::event_processor::ReplayStatus>>,
    ApiError,
> {
    let status = state.event_processor.get_replay_status();
    Ok(Json(status))
}
