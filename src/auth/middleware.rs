use axum::{
    extract::State,
    http::{header::AUTHORIZATION, StatusCode, Request},
    middleware::Next,
    response::Response,
    body::Body,
};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::auth::{Claims, Role};
use crate::error::{ApiError, Result};
use crate::AppState;

/// JWT Authentication middleware
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    let token = match auth_header {
        Some(auth_value) if auth_value.starts_with("Bearer ") => {
            &auth_value[7..] // Remove "Bearer " prefix
        }
        _ => {
            // Check for X-API-Key header
            if let Some(api_key) = request.headers().get("X-API-Key").and_then(|h| h.to_str().ok()) {
                 // Check if it matches engineering API key
                 if api_key == state.config.engineering_api_key {
                     // Create synthetic claims for simulator
                     let claims = Claims::new(
                         Uuid::parse_str("63c1d015-6765-4843-9ca3-5ba21ee54d7e").unwrap(), // Use nil UUID for simulator
                         "simulator".to_string(),
                         "ami".to_string(), // Use AMI role
                     );
                     request.extensions_mut().insert(claims);
                     return next.run(request).await;
                 }
            }

            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Body::from("Missing or invalid Authorization header"))
                .expect("Failed to build unauthorized response");
        }
    };

    // Check if token matches engineering API key (Simulator sends it as Bearer token)
    if token == state.config.engineering_api_key {
         // Create synthetic claims for simulator
         let claims = Claims::new(
             Uuid::parse_str("63c1d015-6765-4843-9ca3-5ba21ee54d7e").unwrap(), // Use nil UUID for simulator
             "simulator".to_string(),
             "ami".to_string(), // Use AMI role
         );
         request.extensions_mut().insert(claims);
         return next.run(request).await;
    }

    match state.jwt_service.decode_token(token) {
        Ok(claims) => {
            // Add claims to request extensions for use in handlers
            request.extensions_mut().insert(claims);
            next.run(request).await
        }
        Err(_) => Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::from("Invalid or expired token"))
            .expect("Failed to build unauthorized response"),
    }
}

/// Role-based authorization middleware for admin access
pub async fn require_admin_role(
    user: AuthenticatedUser,
    request: Request<Body>,
    next: Next,
) -> Response {
    let user_role = match Role::from_str(&user.0.role) {
        Ok(role) => role,
        Err(_) => {
            return Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Body::from("Invalid user role"))
                .expect("Failed to build forbidden response");
        }
    };

    if user_role == Role::Admin {
        next.run(request).await
    } else {
        Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Body::from("Admin access required"))
            .expect("Failed to build forbidden response")
    }
}

/// Extractor for authenticated user claims
#[derive(Clone)]
pub struct AuthenticatedUser(pub Claims);

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> std::result::Result<Self, Self::Rejection> {
        let claims = parts
            .extensions
            .get::<Claims>()
            .cloned()
            .ok_or_else(|| ApiError::Unauthorized("No authentication found".to_string()))?;

        Ok(AuthenticatedUser(claims))
    }
}

/// Verify API key against database
async fn verify_api_key(state: &AppState, key: &str) -> Result<crate::auth::ApiKey> {
    let query = "
        SELECT id, key_hash, name, permissions, is_active, created_at, last_used_at
        FROM api_keys
        WHERE is_active = true
    ";

    let api_keys = sqlx::query_as::<_, ApiKeyRow>(query)
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiError::Internal(format!("Database error: {}", e)))?;

    for api_key_row in api_keys {
        if state.api_key_service.verify_key(key, &api_key_row.key_hash)? {
            // Update last_used_at
            let _ = sqlx::query("UPDATE api_keys SET last_used_at = NOW() WHERE id = $1")
                .bind(api_key_row.id)
                .execute(&state.db)
                .await;

            return Ok(crate::auth::ApiKey {
                id: api_key_row.id,
                key_hash: api_key_row.key_hash,
                name: api_key_row.name,
                permissions: serde_json::from_value(api_key_row.permissions)
                    .unwrap_or_default(),
                is_active: api_key_row.is_active,
                created_at: api_key_row.created_at,
                last_used_at: api_key_row.last_used_at,
            });
        }
    }

    Err(ApiError::Unauthorized("Invalid API key".to_string()))
}

#[allow(dead_code)]
#[derive(sqlx::FromRow)]
struct ApiKeyRow {
    id: uuid::Uuid,
    key_hash: String,
    name: String,
    permissions: serde_json::Value,
    is_active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_hierarchy() {
        // Admin should have access to all roles
        let admin_role = Role::Admin;
        assert!(admin_role.can_access("users:create"));
        assert!(admin_role.can_access("energy:read"));
        assert!(admin_role.can_access("admin:settings"));

        // User should have limited access
        let user_role = Role::User;
        assert!(user_role.can_access("energy:read"));
        assert!(user_role.can_access("trading:create"));
        assert!(!user_role.can_access("users:create"));
        assert!(!user_role.can_access("admin:settings"));
    }
}
