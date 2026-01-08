//! Notifications Handler
//!
//! Handles listing, reading, and managing notification preferences

use axum::{extract::{State, Path, Query}, response::Json};
use serde::Deserialize;
use uuid::Uuid;
use tracing::{info, error};

use crate::auth::middleware::AuthenticatedUser;
use crate::error::{ApiError, Result};
use crate::models::notification::{
    Notification, NotificationPreferences, UpdatePreferencesRequest,
    NotificationListResponse, NotificationType,
};
use crate::AppState;

/// Query params for listing notifications
#[derive(Debug, Deserialize)]
pub struct ListNotificationsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub unread_only: Option<bool>,
}

/// List user's notifications
/// GET /api/v1/notifications
#[utoipa::path(
    get,
    path = "/api/v1/notifications",
    tag = "notifications",
    params(
        ("limit" = Option<i64>, Query, description = "Max notifications to return"),
        ("offset" = Option<i64>, Query, description = "Offset for pagination"),
        ("unread_only" = Option<bool>, Query, description = "Only return unread notifications")
    ),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of notifications", body = NotificationListResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_notifications(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Query(params): Query<ListNotificationsQuery>,
) -> Result<Json<NotificationListResponse>> {
    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);
    let unread_only = params.unread_only.unwrap_or(false);

    let notifications = if unread_only {
        sqlx::query_as!(
            Notification,
            r#"
            SELECT id, user_id, notification_type as "notification_type!: NotificationType",
                   title, message, data, read, created_at as "created_at!"
            FROM notifications
            WHERE user_id = $1 AND read = false
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            user.0.sub, limit, offset
        )
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query_as!(
            Notification,
            r#"
            SELECT id, user_id, notification_type as "notification_type!: NotificationType",
                   title, message, data, read, created_at as "created_at!"
            FROM notifications
            WHERE user_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            user.0.sub, limit, offset
        )
        .fetch_all(&state.db)
        .await
    }.map_err(|e| {
        error!("Failed to list notifications: {}", e);
        ApiError::Internal(format!("Failed to list notifications: {}", e))
    })?;

    // Get counts
    let counts = sqlx::query!(
        r#"
        SELECT 
            COUNT(*) FILTER (WHERE read = false) as "unread_count!",
            COUNT(*) as "total!"
        FROM notifications
        WHERE user_id = $1
        "#,
        user.0.sub
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Failed to get counts: {}", e)))?;

    Ok(Json(NotificationListResponse {
        notifications,
        unread_count: counts.unread_count,
        total: counts.total,
    }))
}

/// Mark a notification as read
/// PUT /api/v1/notifications/:id/read
#[utoipa::path(
    put,
    path = "/api/v1/notifications/{id}/read",
    tag = "notifications",
    params(("id" = Uuid, Path, description = "Notification ID")),
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Notification marked as read"),
        (status = 404, description = "Notification not found"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn mark_as_read(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(notification_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query!(
        "UPDATE notifications SET read = true WHERE id = $1 AND user_id = $2",
        notification_id, user.0.sub
    )
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Failed to mark as read: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("Notification not found".to_string()));
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Notification marked as read"
    })))
}

/// Mark all notifications as read
/// PUT /api/v1/notifications/read-all
#[utoipa::path(
    put,
    path = "/api/v1/notifications/read-all",
    tag = "notifications",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "All notifications marked as read"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn mark_all_as_read(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query!(
        "UPDATE notifications SET read = true WHERE user_id = $1 AND read = false",
        user.0.sub
    )
    .execute(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Failed to mark all as read: {}", e)))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("{} notifications marked as read", result.rows_affected())
    })))
}

/// Get notification preferences
/// GET /api/v1/notifications/preferences
#[utoipa::path(
    get,
    path = "/api/v1/notifications/preferences",
    tag = "notifications",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Notification preferences", body = NotificationPreferences),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_preferences(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<NotificationPreferences>> {
    // Try to get existing preferences, or create default
    let preferences = sqlx::query_as!(
        NotificationPreferences,
        r#"
        SELECT user_id, order_filled, order_matched, conditional_triggered,
               recurring_executed, price_alerts, escrow_events, system_announcements,
               email_enabled, push_enabled, updated_at as "updated_at!"
        FROM user_notification_preferences
        WHERE user_id = $1
        "#,
        user.0.sub
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiError::Internal(format!("Failed to get preferences: {}", e)))?;

    if let Some(prefs) = preferences {
        Ok(Json(prefs))
    } else {
        // Create default preferences
        let prefs = sqlx::query_as!(
            NotificationPreferences,
            r#"
            INSERT INTO user_notification_preferences (user_id)
            VALUES ($1)
            RETURNING user_id, order_filled, order_matched, conditional_triggered,
                      recurring_executed, price_alerts, escrow_events, system_announcements,
                      email_enabled, push_enabled, updated_at as "updated_at!"
            "#,
            user.0.sub
        )
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to create preferences: {}", e)))?;

        Ok(Json(prefs))
    }
}

/// Update notification preferences
/// PUT /api/v1/notifications/preferences
#[utoipa::path(
    put,
    path = "/api/v1/notifications/preferences",
    tag = "notifications",
    request_body = UpdatePreferencesRequest,
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Preferences updated", body = NotificationPreferences),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn update_preferences(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(payload): Json<UpdatePreferencesRequest>,
) -> Result<Json<NotificationPreferences>> {
    info!("Updating notification preferences for user: {}", user.0.sub);

    // Upsert preferences
    let prefs = sqlx::query_as!(
        NotificationPreferences,
        r#"
        INSERT INTO user_notification_preferences (user_id, order_filled, order_matched,
            conditional_triggered, recurring_executed, price_alerts, escrow_events,
            system_announcements, email_enabled, push_enabled, updated_at)
        VALUES ($1, 
            COALESCE($2, true), COALESCE($3, true), COALESCE($4, true),
            COALESCE($5, true), COALESCE($6, true), COALESCE($7, true),
            COALESCE($8, true), COALESCE($9, false), COALESCE($10, true), NOW())
        ON CONFLICT (user_id) DO UPDATE SET
            order_filled = COALESCE($2, user_notification_preferences.order_filled),
            order_matched = COALESCE($3, user_notification_preferences.order_matched),
            conditional_triggered = COALESCE($4, user_notification_preferences.conditional_triggered),
            recurring_executed = COALESCE($5, user_notification_preferences.recurring_executed),
            price_alerts = COALESCE($6, user_notification_preferences.price_alerts),
            escrow_events = COALESCE($7, user_notification_preferences.escrow_events),
            system_announcements = COALESCE($8, user_notification_preferences.system_announcements),
            email_enabled = COALESCE($9, user_notification_preferences.email_enabled),
            push_enabled = COALESCE($10, user_notification_preferences.push_enabled),
            updated_at = NOW()
        RETURNING user_id, order_filled, order_matched, conditional_triggered,
                  recurring_executed, price_alerts, escrow_events, system_announcements,
                  email_enabled, push_enabled, updated_at as "updated_at!"
        "#,
        user.0.sub,
        payload.order_filled,
        payload.order_matched,
        payload.conditional_triggered,
        payload.recurring_executed,
        payload.price_alerts,
        payload.escrow_events,
        payload.system_announcements,
        payload.email_enabled,
        payload.push_enabled
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        error!("Failed to update preferences: {}", e);
        ApiError::Internal(format!("Failed to update preferences: {}", e))
    })?;

    Ok(Json(prefs))
}
