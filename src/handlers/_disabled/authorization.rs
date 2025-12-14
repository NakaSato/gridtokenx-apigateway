//! Authorization helpers for handler endpoints.
//!
//! This module provides reusable authorization functions for checking
//! user roles and permissions in handlers.

use crate::auth::Claims;
use crate::error::ApiError;

/// Role constants for authorization checks
pub mod roles {
    pub const ADMIN: &str = "admin";
    pub const USER: &str = "user";
    pub const AMI: &str = "ami";
    pub const PRODUCER: &str = "producer";
    pub const CONSUMER: &str = "consumer";
    pub const OPERATOR: &str = "operator";
}

/// Check if user has the required role
pub fn require_role(user: &Claims, required_role: &str) -> Result<(), ApiError> {
    if user.role.to_lowercase() != required_role.to_lowercase() {
        return Err(ApiError::Forbidden(format!(
            "Access denied. Required role: {}",
            required_role
        )));
    }
    Ok(())
}

/// Check if user has any of the required roles
pub fn require_any_role(user: &Claims, required_roles: &[&str]) -> Result<(), ApiError> {
    let user_role = user.role.to_lowercase();
    if required_roles.iter().any(|r| r.to_lowercase() == user_role) {
        return Ok(());
    }
    Err(ApiError::Forbidden(format!(
        "Access denied. Required one of roles: {}",
        required_roles.join(", ")
    )))
}

/// Check if user is an admin
pub fn require_admin(user: &Claims) -> Result<(), ApiError> {
    require_role(user, roles::ADMIN)
}

/// Check if user is admin or the resource owner
pub fn require_admin_or_owner(user: &Claims, resource_user_id: uuid::Uuid) -> Result<(), ApiError> {
    if user.role.to_lowercase() == roles::ADMIN || user.sub == resource_user_id {
        return Ok(());
    }
    Err(ApiError::Forbidden(
        "Access denied. Must be admin or resource owner".to_string(),
    ))
}

/// Check if user can access another user's data
pub fn can_access_user_data(user: &Claims, target_user_id: uuid::Uuid) -> bool {
    user.role.to_lowercase() == roles::ADMIN || user.sub == target_user_id
}

/// Check if user has meter submission permission
pub fn can_submit_meter_readings(user: &Claims) -> Result<(), ApiError> {
    require_any_role(user, &[roles::ADMIN, roles::AMI, roles::PRODUCER, roles::USER])
}

/// Check if user can trade
pub fn can_trade(user: &Claims) -> Result<(), ApiError> {
    require_any_role(user, &[roles::ADMIN, roles::PRODUCER, roles::CONSUMER, roles::USER])
}

/// Check if user can view analytics
pub fn can_view_analytics(user: &Claims) -> Result<(), ApiError> {
    require_any_role(user, &[roles::ADMIN, roles::OPERATOR])
}

/// Trait for authorization checks on handlers
pub trait Authorize {
    /// Check if the user is authorized for this action
    fn is_authorized(&self, user: &Claims) -> Result<(), ApiError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn create_test_claims(role: &str) -> Claims {
        Claims {
            sub: Uuid::new_v4(),
            username: "test_user".to_string(),
            role: role.to_string(),
            exp: chrono::Utc::now().timestamp() + 3600,
            iat: chrono::Utc::now().timestamp(),
            iss: "test".to_string(),
        }
    }

    #[test]
    fn test_require_role() {
        let admin = create_test_claims("admin");
        let user = create_test_claims("user");

        assert!(require_role(&admin, roles::ADMIN).is_ok());
        assert!(require_role(&user, roles::USER).is_ok());
        assert!(require_role(&user, roles::ADMIN).is_err());
    }

    #[test]
    fn test_require_any_role() {
        let producer = create_test_claims("producer");
        
        assert!(require_any_role(&producer, &[roles::PRODUCER, roles::CONSUMER]).is_ok());
        assert!(require_any_role(&producer, &[roles::ADMIN]).is_err());
    }

    #[test]
    fn test_require_admin_or_owner() {
        let admin = create_test_claims("admin");
        let user = create_test_claims("user");
        let other_user_id = Uuid::new_v4();

        // Admin can access any user
        assert!(require_admin_or_owner(&admin, other_user_id).is_ok());
        
        // User can access own data
        assert!(require_admin_or_owner(&user, user.sub).is_ok());
        
        // User cannot access other user's data
        assert!(require_admin_or_owner(&user, other_user_id).is_err());
    }
}
