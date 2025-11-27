// Middleware module - authentication, CORS, logging, security, etc.

pub mod json_validation;
pub mod metrics;
pub mod metrics_middleware;
pub mod request_logger;
pub mod security_headers;

pub use json_validation::json_validation_middleware;
pub use metrics::{active_requests_middleware, metrics_middleware};
pub use request_logger::{auth_logger_middleware, request_logger_middleware};
pub use security_headers::add_security_headers;
