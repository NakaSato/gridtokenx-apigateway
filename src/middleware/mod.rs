// Middleware module - authentication, rate limiting, CORS, logging, security, versioning, etc.

pub mod request_logger;
pub mod metrics;
pub mod security_headers;
pub mod rate_limiter;
pub mod versioning;

pub use request_logger::{
    request_logger_middleware, auth_logger_middleware
};
pub use metrics::{
    metrics_middleware, active_requests_middleware
};
pub use security_headers::add_security_headers;
pub use versioning::{
    versioning_middleware, version_check_middleware
};
