//! Role-based access control (RBAC) module.
//!
//! This module provides a flexible and extensible role and permission system
//! that can be easily extended as the application grows.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Permission represents a specific action on a resource.
/// Format: "resource:action" (e.g., "energy:read", "trading:create")
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Permission(String);

impl Permission {
    pub fn new(resource: &str, action: &str) -> Self {
        Self(format!("{}:{}", resource, action))
    }

    pub fn wildcard(resource: &str) -> Self {
        Self(format!("{}:*", resource))
    }

    pub fn resource(&self) -> &str {
        self.0.split(':').next().unwrap_or("")
    }

    pub fn action(&self) -> &str {
        self.0.split(':').nth(1).unwrap_or("")
    }

    pub fn is_wildcard(&self) -> bool {
        self.0.ends_with(":*")
    }

    /// Check if this permission grants access to the requested permission
    pub fn grants(&self, requested: &Permission) -> bool {
        if self.0 == requested.0 {
            return true;
        }

        // Check wildcard permissions
        if self.is_wildcard() && self.resource() == requested.resource() {
            return true;
        }

        false
    }
}

impl From<&str> for Permission {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Role represents a user's role with associated permissions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Admin,
    AMI,
    Producer,
    Consumer,
    Operator,
}

impl Role {
    /// Get all permissions for this role
    pub fn permissions(&self) -> HashSet<Permission> {
        match self {
            Role::User => Self::user_permissions(),
            Role::Admin => Self::admin_permissions(),
            Role::AMI => Self::ami_permissions(),
            Role::Producer => Self::producer_permissions(),
            Role::Consumer => Self::consumer_permissions(),
            Role::Operator => Self::operator_permissions(),
        }
    }

    /// Check if role has a specific permission
    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.permissions().iter().any(|p| p.grants(permission))
    }

    /// Check if role has any of the specified permissions
    pub fn has_any_permission(&self, permissions: &[Permission]) -> bool {
        permissions.iter().any(|p| self.has_permission(p))
    }

    /// Check if role has all of the specified permissions
    pub fn has_all_permissions(&self, permissions: &[Permission]) -> bool {
        permissions.iter().all(|p| self.has_permission(p))
    }

    fn user_permissions() -> HashSet<Permission> {
        [
            "energy:read",
            "energy:submit",
            "trading:read",
            "trading:create",
            "profile:read",
            "profile:update",
            "meters:read",
        ]
        .into_iter()
        .map(Permission::from)
        .collect()
    }

    fn admin_permissions() -> HashSet<Permission> {
        [
            "energy:*",
            "trading:*",
            "profile:*",
            "analytics:*",
            "users:*",
            "admin:*",
            "meters:*",
            "system:*",
        ]
        .into_iter()
        .map(Permission::from)
        .collect()
    }

    fn ami_permissions() -> HashSet<Permission> {
        [
            "energy:submit",
            "meters:read",
            "meters:update",
            "readings:submit",
        ]
        .into_iter()
        .map(Permission::from)
        .collect()
    }

    fn producer_permissions() -> HashSet<Permission> {
        [
            "energy:read",
            "energy:submit",
            "offers:*",
            "trading:read",
            "trading:create",
            "transactions:read",
            "profile:read",
            "profile:update",
            "meters:read",
        ]
        .into_iter()
        .map(Permission::from)
        .collect()
    }

    fn consumer_permissions() -> HashSet<Permission> {
        [
            "energy:read",
            "orders:*",
            "offers:read",
            "trading:read",
            "trading:create",
            "transactions:read",
            "profile:read",
            "profile:update",
        ]
        .into_iter()
        .map(Permission::from)
        .collect()
    }

    fn operator_permissions() -> HashSet<Permission> {
        [
            "energy:read",
            "meters:*",
            "readings:*",
            "analytics:read",
            "system:health",
        ]
        .into_iter()
        .map(Permission::from)
        .collect()
    }
}

impl std::str::FromStr for Role {
    type Err = RoleParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "user" => Ok(Role::User),
            "admin" => Ok(Role::Admin),
            "ami" => Ok(Role::AMI),
            "producer" => Ok(Role::Producer),
            "consumer" => Ok(Role::Consumer),
            "operator" => Ok(Role::Operator),
            _ => Err(RoleParseError(s.to_string())),
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Role::User => "user",
            Role::Admin => "admin",
            Role::AMI => "ami",
            Role::Producer => "producer",
            Role::Consumer => "consumer",
            Role::Operator => "operator",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug)]
pub struct RoleParseError(String);

impl std::fmt::Display for RoleParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid role: {}", self.0)
    }
}

impl std::error::Error for RoleParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_creation() {
        let perm = Permission::new("energy", "read");
        assert_eq!(perm.resource(), "energy");
        assert_eq!(perm.action(), "read");
        assert!(!perm.is_wildcard());
    }

    #[test]
    fn test_wildcard_permission() {
        let wildcard = Permission::wildcard("energy");
        let specific = Permission::new("energy", "read");

        assert!(wildcard.is_wildcard());
        assert!(wildcard.grants(&specific));
        assert!(!specific.grants(&wildcard));
    }

    #[test]
    fn test_role_permissions() {
        let admin = Role::Admin;
        assert!(admin.has_permission(&Permission::new("users", "create")));
        assert!(admin.has_permission(&Permission::new("energy", "read")));

        let user = Role::User;
        assert!(user.has_permission(&Permission::new("energy", "read")));
        assert!(!user.has_permission(&Permission::new("users", "create")));
    }

    #[test]
    fn test_role_from_str() {
        assert_eq!("admin".parse::<Role>().unwrap(), Role::Admin);
        assert_eq!("USER".parse::<Role>().unwrap(), Role::User);
        assert!("invalid".parse::<Role>().is_err());
    }
}
