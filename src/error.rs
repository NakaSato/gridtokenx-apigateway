use axum::{
    Json,
    extract::rejection::JsonRejection,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{error, warn};
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, ApiError>;

/// Error codes for categorizing errors
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ErrorCode {
    // Authentication errors (1xxx)
    #[serde(rename = "AUTH_1001")]
    InvalidCredentials,
    #[serde(rename = "AUTH_1002")]
    TokenExpired,
    #[serde(rename = "AUTH_1003")]
    TokenInvalid,
    #[serde(rename = "AUTH_1004")]
    TokenMissing,
    #[serde(rename = "AUTH_1005")]
    EmailNotVerified,
    #[serde(rename = "AUTH_1006")]
    AccountLocked,
    #[serde(rename = "AUTH_1007")]
    AccountDisabled,

    // Authorization errors (2xxx)
    #[serde(rename = "AUTHZ_2001")]
    InsufficientPermissions,
    #[serde(rename = "AUTHZ_2002")]
    ResourceAccessDenied,
    #[serde(rename = "AUTHZ_2003")]
    RoleNotAuthorized,

    // Validation errors (3xxx)
    #[serde(rename = "VAL_3001")]
    InvalidInput,
    #[serde(rename = "VAL_3002")]
    MissingRequiredField,
    #[serde(rename = "VAL_3003")]
    InvalidFormat,
    #[serde(rename = "VAL_3004")]
    InvalidWalletAddress,
    #[serde(rename = "VAL_3005")]
    InvalidAmount,
    #[serde(rename = "VAL_3006")]
    InvalidEmail,
    #[serde(rename = "VAL_3007")]
    InvalidPassword,
    #[serde(rename = "VAL_3008")]
    PasswordTooWeak,

    // Resource errors (4xxx)
    #[serde(rename = "RES_4001")]
    NotFound,
    #[serde(rename = "RES_4002")]
    AlreadyExists,
    #[serde(rename = "RES_4003")]
    Conflict,
    #[serde(rename = "RES_4004")]
    Gone,

    // Business logic errors (5xxx)
    #[serde(rename = "BIZ_5001")]
    InsufficientBalance,
    #[serde(rename = "BIZ_5002")]
    OrderNotMatched,
    #[serde(rename = "BIZ_5003")]
    TradingNotAllowed,
    #[serde(rename = "BIZ_5004")]
    MeterReadingInvalid,
    #[serde(rename = "BIZ_5005")]
    TokenMintingFailed,
    #[serde(rename = "BIZ_5006")]
    EpochNotActive,

    // Blockchain errors (6xxx)
    #[serde(rename = "BC_6001")]
    BlockchainConnectionFailed,
    #[serde(rename = "BC_6002")]
    BlockchainTransactionFailed,
    #[serde(rename = "BC_6003")]
    TransactionTimeout,
    #[serde(rename = "BC_6004")]
    InvalidSignature,
    #[serde(rename = "BC_6005")]
    InsufficientGasFee,
    #[serde(rename = "BC_6006")]
    ProgramError,

    // Database errors (7xxx)
    #[serde(rename = "DB_7001")]
    DatabaseConnectionFailed,
    #[serde(rename = "DB_7002")]
    QueryFailed,
    #[serde(rename = "DB_7003")]
    DatabaseTransactionFailed,
    #[serde(rename = "DB_7004")]
    ConstraintViolation,

    // External service errors (8xxx)
    #[serde(rename = "EXT_8001")]
    ExternalServiceUnavailable,
    #[serde(rename = "EXT_8002")]
    ExternalServiceTimeout,
    #[serde(rename = "EXT_8003")]
    ExternalServiceError,
    #[serde(rename = "EXT_8004")]
    EmailServiceFailed,
    #[serde(rename = "EXT_8005")]
    ServiceUnavailable,

    // Rate limiting errors (9xxx)
    #[serde(rename = "RATE_9001")]
    RateLimitExceeded,
    #[serde(rename = "RATE_9002")]
    TooManyRequests,

    // Internal errors (9xxx)
    #[serde(rename = "INT_9999")]
    InternalServerError,
    #[serde(rename = "INT_9998")]
    ConfigurationError,
    #[serde(rename = "INT_9997")]
    UnexpectedError,
}

impl ErrorCode {
    /// Get numeric code
    pub fn code(&self) -> u16 {
        match self {
            // Authentication
            ErrorCode::InvalidCredentials => 1001,
            ErrorCode::TokenExpired => 1002,
            ErrorCode::TokenInvalid => 1003,
            ErrorCode::TokenMissing => 1004,
            ErrorCode::EmailNotVerified => 1005,
            ErrorCode::AccountLocked => 1006,
            ErrorCode::AccountDisabled => 1007,

            // Authorization
            ErrorCode::InsufficientPermissions => 2001,
            ErrorCode::ResourceAccessDenied => 2002,
            ErrorCode::RoleNotAuthorized => 2003,

            // Validation
            ErrorCode::InvalidInput => 3001,
            ErrorCode::MissingRequiredField => 3002,
            ErrorCode::InvalidFormat => 3003,
            ErrorCode::InvalidWalletAddress => 3004,
            ErrorCode::InvalidAmount => 3005,
            ErrorCode::InvalidEmail => 3006,
            ErrorCode::InvalidPassword => 3007,
            ErrorCode::PasswordTooWeak => 3008,

            // Resource
            ErrorCode::NotFound => 4001,
            ErrorCode::AlreadyExists => 4002,
            ErrorCode::Conflict => 4003,
            ErrorCode::Gone => 4004,

            // Business Logic
            ErrorCode::InsufficientBalance => 5001,
            ErrorCode::OrderNotMatched => 5002,
            ErrorCode::TradingNotAllowed => 5003,
            ErrorCode::MeterReadingInvalid => 5004,
            ErrorCode::TokenMintingFailed => 5005,
            ErrorCode::EpochNotActive => 5006,

            // Blockchain
            ErrorCode::BlockchainConnectionFailed => 6001,
            ErrorCode::BlockchainTransactionFailed => 6002,
            ErrorCode::TransactionTimeout => 6003,
            ErrorCode::InvalidSignature => 6004,
            ErrorCode::InsufficientGasFee => 6005,
            ErrorCode::ProgramError => 6006,

            // Database
            ErrorCode::DatabaseConnectionFailed => 7001,
            ErrorCode::QueryFailed => 7002,
            ErrorCode::DatabaseTransactionFailed => 7003,
            ErrorCode::ConstraintViolation => 7004,

            // External Service
            ErrorCode::ExternalServiceUnavailable => 8001,
            ErrorCode::ExternalServiceTimeout => 8002,
            ErrorCode::ExternalServiceError => 8003,
            ErrorCode::EmailServiceFailed => 8004,
            ErrorCode::ServiceUnavailable => 8005,

            // Rate Limiting
            ErrorCode::RateLimitExceeded => 9001,
            ErrorCode::TooManyRequests => 9002,

            // Internal
            ErrorCode::InternalServerError => 9999,
            ErrorCode::ConfigurationError => 9998,
            ErrorCode::UnexpectedError => 9997,
        }
    }

    /// Get user-friendly message
    pub fn message(&self) -> &'static str {
        match self {
            // Authentication
            ErrorCode::InvalidCredentials => "Invalid email or password",
            ErrorCode::TokenExpired => "Your session has expired. Please log in again",
            ErrorCode::TokenInvalid => "Invalid authentication token",
            ErrorCode::TokenMissing => "Authentication required. Please log in",
            ErrorCode::EmailNotVerified => "Please verify your email address before proceeding",
            ErrorCode::AccountLocked => "Your account has been locked. Please contact support",
            ErrorCode::AccountDisabled => "Your account has been disabled. Please contact support",

            // Authorization
            ErrorCode::InsufficientPermissions => {
                "You don't have permission to perform this action"
            }
            ErrorCode::ResourceAccessDenied => "Access to this resource is denied",
            ErrorCode::RoleNotAuthorized => "Your role is not authorized for this action",

            // Validation
            ErrorCode::InvalidInput => "Invalid input provided",
            ErrorCode::MissingRequiredField => "Required field is missing",
            ErrorCode::InvalidFormat => "Invalid format provided",
            ErrorCode::InvalidWalletAddress => "Invalid wallet address format",
            ErrorCode::InvalidAmount => "Invalid amount provided",
            ErrorCode::InvalidEmail => "Invalid email address format",
            ErrorCode::InvalidPassword => "Invalid password",
            ErrorCode::PasswordTooWeak => {
                "Password is too weak. Use at least 8 characters with letters and numbers"
            }

            // Resource
            ErrorCode::NotFound => "The requested resource was not found",
            ErrorCode::AlreadyExists => "This resource already exists",
            ErrorCode::Conflict => "A conflict occurred with an existing resource",
            ErrorCode::Gone => "This resource is no longer available",

            // Business Logic
            ErrorCode::InsufficientBalance => "Insufficient balance to complete this transaction",
            ErrorCode::OrderNotMatched => "No matching orders found",
            ErrorCode::TradingNotAllowed => "Trading is not allowed at this time",
            ErrorCode::MeterReadingInvalid => "Invalid meter reading provided",
            ErrorCode::TokenMintingFailed => "Failed to mint energy tokens",
            ErrorCode::EpochNotActive => "Trading epoch is not active",

            // Blockchain
            ErrorCode::BlockchainConnectionFailed => "Failed to connect to blockchain network",
            ErrorCode::BlockchainTransactionFailed => "Blockchain transaction failed",
            ErrorCode::TransactionTimeout => "Blockchain transaction timed out",
            ErrorCode::InvalidSignature => "Invalid transaction signature",
            ErrorCode::InsufficientGasFee => "Insufficient gas fee for transaction",
            ErrorCode::ProgramError => "Blockchain program error occurred",

            // Database
            ErrorCode::DatabaseConnectionFailed => "Database connection failed",
            ErrorCode::QueryFailed => "Database query failed",
            ErrorCode::DatabaseTransactionFailed => "Database transaction failed",
            ErrorCode::ConstraintViolation => "Database constraint violation",

            // External Service
            ErrorCode::ExternalServiceUnavailable => "External service is currently unavailable",
            ErrorCode::ExternalServiceTimeout => "External service request timed out",
            ErrorCode::ExternalServiceError => "External service error occurred",
            ErrorCode::EmailServiceFailed => "Failed to send email",
            ErrorCode::ServiceUnavailable => "Service is currently unavailable",

            // Rate Limiting
            ErrorCode::RateLimitExceeded => "Rate limit exceeded. Please try again later",
            ErrorCode::TooManyRequests => "Too many requests. Please slow down",

            // Internal
            ErrorCode::InternalServerError => "An internal server error occurred",
            ErrorCode::ConfigurationError => "Server configuration error",
            ErrorCode::UnexpectedError => "An unexpected error occurred",
        }
    }
}

/// Structured error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
    pub request_id: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub code: ErrorCode,
    pub code_number: u16,
    pub message: String,
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<u64>,
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Authentication failed: {0}")]
    Authentication(String),

    #[error("Authorization failed: {0}")]
    Authorization(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("Blockchain error: {0}")]
    Blockchain(String),

    #[error("External service error: {0}")]
    ExternalService(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Rate limit exceeded")]
    RateLimit,

    #[error("Rate limit exceeded. Please wait {0} seconds before retrying")]
    RateLimitWithRetry(i64),

    #[error("Rate limit exceeded. Retry after {retry_after_seconds} seconds")]
    RateLimitExceeded { retry_after_seconds: u64 },

    #[error("Internal server error: {0}")]
    Internal(String),

    // Enhanced error types with codes
    #[error("{1}")]
    WithCode(ErrorCode, String),

    #[error("{1}")]
    WithCodeAndDetails(ErrorCode, String, String),

    #[error("Validation failed: {field}")]
    ValidationWithField {
        code: ErrorCode,
        field: String,
        message: String,
    },
}

impl ApiError {
    /// Create error with specific error code
    pub fn with_code(code: ErrorCode, message: impl Into<String>) -> Self {
        ApiError::WithCode(code, message.into())
    }

    /// Create error with code and additional details
    pub fn with_details(
        code: ErrorCode,
        message: impl Into<String>,
        details: impl Into<String>,
    ) -> Self {
        ApiError::WithCodeAndDetails(code, message.into(), details.into())
    }

    /// Create validation error for specific field
    pub fn validation_field(field: impl Into<String>, message: impl Into<String>) -> Self {
        ApiError::ValidationWithField {
            code: ErrorCode::InvalidInput,
            field: field.into(),
            message: message.into(),
        }
    }

    /// Create general validation error
    pub fn validation_error(message: impl Into<String>, field: Option<&str>) -> Self {
        if let Some(field_name) = field {
            ApiError::ValidationWithField {
                code: ErrorCode::InvalidInput,
                field: field_name.to_string(),
                message: message.into(),
            }
        } else {
            ApiError::with_code(ErrorCode::InvalidInput, message)
        }
    }

    /// Helper: Invalid credentials
    pub fn invalid_credentials() -> Self {
        ApiError::with_code(ErrorCode::InvalidCredentials, "Invalid credentials")
    }

    /// Helper: Token expired
    pub fn token_expired() -> Self {
        ApiError::with_code(ErrorCode::TokenExpired, "Token expired")
    }

    /// Helper: Email not verified
    pub fn email_not_verified() -> Self {
        ApiError::with_code(ErrorCode::EmailNotVerified, "Email not verified")
    }

    /// Helper: Insufficient balance
    pub fn insufficient_balance(amount: &str) -> Self {
        ApiError::with_details(
            ErrorCode::InsufficientBalance,
            "Insufficient balance",
            format!("Required: {}", amount),
        )
    }

    /// Helper: Resource not found
    pub fn not_found(resource: &str) -> Self {
        ApiError::with_code(ErrorCode::NotFound, format!("{} not found", resource))
    }

    /// Helper: Resource already exists
    pub fn already_exists(resource: &str) -> Self {
        ApiError::with_code(
            ErrorCode::AlreadyExists,
            format!("{} already exists", resource),
        )
    }

    /// Helper: Invalid wallet address
    pub fn invalid_wallet() -> Self {
        ApiError::with_code(ErrorCode::InvalidWalletAddress, "Invalid wallet address")
    }

    /// Get error code
    fn error_code(&self) -> ErrorCode {
        match self {
            ApiError::Authentication(_) => ErrorCode::InvalidCredentials,
            ApiError::Authorization(_) => ErrorCode::InsufficientPermissions,
            ApiError::BadRequest(_) => ErrorCode::InvalidInput,
            ApiError::Unauthorized(_) => ErrorCode::TokenMissing,
            ApiError::Forbidden(_) => ErrorCode::ResourceAccessDenied,
            ApiError::Validation(_) => ErrorCode::InvalidInput,
            ApiError::NotFound(_) => ErrorCode::NotFound,
            ApiError::Conflict(_) => ErrorCode::Conflict,
            ApiError::RateLimit => ErrorCode::RateLimitExceeded,
            ApiError::RateLimitWithRetry(_) => ErrorCode::RateLimitExceeded,
            ApiError::RateLimitExceeded { .. } => ErrorCode::RateLimitExceeded,
            ApiError::Database(_) => ErrorCode::QueryFailed,
            ApiError::Redis(_) => ErrorCode::ExternalServiceError,
            ApiError::Blockchain(_) => ErrorCode::BlockchainTransactionFailed,
            ApiError::ExternalService(_) => ErrorCode::ExternalServiceError,
            ApiError::Configuration(_) => ErrorCode::ConfigurationError,
            ApiError::Internal(_) => ErrorCode::InternalServerError,
            ApiError::WithCode(code, _) => *code,
            ApiError::WithCodeAndDetails(code, _, _) => *code,
            ApiError::ValidationWithField { code, .. } => *code,
        }
    }

    /// Get error details
    fn error_details(&self) -> Option<String> {
        match self {
            ApiError::WithCodeAndDetails(_, _, details) => Some(details.clone()),
            _ => None,
        }
    }

    /// Get field name for validation errors
    fn error_field(&self) -> Option<String> {
        match self {
            ApiError::ValidationWithField { field, .. } => Some(field.clone()),
            _ => None,
        }
    }

    /// Get status code
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::Authentication(_)
            | ApiError::Unauthorized(_)
            | ApiError::WithCode(ErrorCode::TokenExpired, _)
            | ApiError::WithCode(ErrorCode::TokenInvalid, _)
            | ApiError::WithCode(ErrorCode::TokenMissing, _)
            | ApiError::WithCode(ErrorCode::EmailNotVerified, _) => StatusCode::UNAUTHORIZED,

            ApiError::Authorization(_)
            | ApiError::Forbidden(_)
            | ApiError::WithCode(ErrorCode::InsufficientPermissions, _)
            | ApiError::WithCode(ErrorCode::ResourceAccessDenied, _) => StatusCode::FORBIDDEN,

            ApiError::BadRequest(_)
            | ApiError::Validation(_)
            | ApiError::ValidationWithField { .. }
            | ApiError::WithCode(ErrorCode::InvalidInput, _)
            | ApiError::WithCode(ErrorCode::InvalidWalletAddress, _)
            | ApiError::WithCode(ErrorCode::InvalidAmount, _) => StatusCode::BAD_REQUEST,

            ApiError::NotFound(_) | ApiError::WithCode(ErrorCode::NotFound, _) => {
                StatusCode::NOT_FOUND
            }

            ApiError::Conflict(_)
            | ApiError::WithCode(ErrorCode::Conflict, _)
            | ApiError::WithCode(ErrorCode::AlreadyExists, _) => StatusCode::CONFLICT,

            ApiError::RateLimit
            | ApiError::RateLimitWithRetry(_)
            | ApiError::RateLimitExceeded { .. }
            | ApiError::WithCode(ErrorCode::RateLimitExceeded, _)
            | ApiError::WithCode(ErrorCode::TooManyRequests, _) => StatusCode::TOO_MANY_REQUESTS,

            ApiError::Blockchain(_)
            | ApiError::ExternalService(_)
            | ApiError::WithCode(ErrorCode::BlockchainConnectionFailed, _)
            | ApiError::WithCode(ErrorCode::ExternalServiceUnavailable, _)
            | ApiError::WithCode(ErrorCode::ServiceUnavailable, _) => StatusCode::BAD_GATEWAY,

            ApiError::Database(_)
            | ApiError::Redis(_)
            | ApiError::Configuration(_)
            | ApiError::Internal(_)
            | ApiError::WithCode(_, _)
            | ApiError::WithCodeAndDetails(_, _, _) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Log error with appropriate level
    fn log_error(&self, request_id: &str) {
        match self.status_code() {
            status if status.is_server_error() => {
                error!(
                    request_id = %request_id,
                    error = %self,
                    "Server error occurred"
                );
            }
            status if status.is_client_error() => {
                warn!(
                    request_id = %request_id,
                    error = %self,
                    "Client error occurred"
                );
            }
            _ => {}
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let request_id = Uuid::new_v4().to_string();
        let status = self.status_code();
        let code = self.error_code();

        // Log the error
        self.log_error(&request_id);

        // Build error response
        let error_response = ErrorResponse {
            error: ErrorDetail {
                code,
                code_number: code.code(),
                message: match &self {
                    ApiError::WithCode(_, msg) | ApiError::WithCodeAndDetails(_, msg, _) => {
                        msg.clone()
                    }
                    ApiError::ValidationWithField { message, .. } => message.clone(),
                    ApiError::RateLimitWithRetry(seconds) => {
                        format!(
                            "Rate limit exceeded. Please wait {} seconds before retrying",
                            seconds
                        )
                    }
                    ApiError::RateLimitExceeded { .. } => code.message().to_string(),
                    _ => code.message().to_string(),
                },
                details: self.error_details(),
                field: self.error_field(),
                retry_after: match &self {
                    ApiError::RateLimitWithRetry(seconds) => Some(*seconds as u64),
                    ApiError::RateLimitExceeded {
                        retry_after_seconds,
                    } => Some(*retry_after_seconds),
                    _ => None,
                },
            },
            request_id,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        // Add Retry-After header for rate limit errors
        let mut response = (status, Json(error_response)).into_response();
        match &self {
            ApiError::RateLimitWithRetry(seconds) => {
                response.headers_mut().insert(
                    "Retry-After",
                    seconds
                        .to_string()
                        .parse()
                        .expect("Failed to parse retry-after seconds"),
                );
            }
            ApiError::RateLimitExceeded {
                retry_after_seconds,
            } => {
                response.headers_mut().insert(
                    "Retry-After",
                    retry_after_seconds
                        .to_string()
                        .parse()
                        .expect("Failed to parse retry-after seconds"),
                );
            }
            _ => {}
        }

        response
    }
}

impl ApiError {
    fn error_type(&self) -> &'static str {
        match self {
            ApiError::Authentication(_) => "authentication_error",
            ApiError::Authorization(_) => "authorization_error",
            ApiError::BadRequest(_) => "bad_request",
            ApiError::Unauthorized(_) => "unauthorized",
            ApiError::Forbidden(_) => "forbidden",
            ApiError::Validation(_) => "validation_error",
            ApiError::Database(_) => "database_error",
            ApiError::Redis(_) => "cache_error",
            ApiError::Blockchain(_) => "blockchain_error",
            ApiError::ExternalService(_) => "external_service_error",
            ApiError::Configuration(_) => "configuration_error",
            ApiError::NotFound(_) => "not_found",
            ApiError::Conflict(_) => "conflict",
            ApiError::RateLimit => "rate_limit_exceeded",
            ApiError::RateLimitWithRetry(_) => "rate_limit_exceeded",
            ApiError::RateLimitExceeded { .. } => "rate_limit_exceeded",
            ApiError::Internal(_) => "internal_error",
            ApiError::WithCode(code, _) => match code {
                ErrorCode::InvalidCredentials => "authentication_error",
                ErrorCode::InsufficientPermissions => "authorization_error",
                ErrorCode::NotFound => "not_found",
                _ => "error",
            },
            ApiError::WithCodeAndDetails(code, _, _) => match code {
                ErrorCode::InvalidCredentials => "authentication_error",
                ErrorCode::InsufficientPermissions => "authorization_error",
                ErrorCode::NotFound => "not_found",
                _ => "error",
            },
            ApiError::ValidationWithField { .. } => "validation_error",
        }
    }
}

/// Handle Axum JSON rejections and convert to structured API errors
pub fn handle_rejection(err: JsonRejection) -> Response {
    match err {
        JsonRejection::JsonDataError(e) => ApiError::with_details(
            ErrorCode::InvalidInput,
            "Invalid input provided",
            e.to_string(),
        )
        .into_response(),
        JsonRejection::JsonSyntaxError(_) => {
            ApiError::with_code(ErrorCode::InvalidFormat, "Invalid JSON format").into_response()
        }
        JsonRejection::MissingJsonContentType(_) => {
            ApiError::with_code(ErrorCode::InvalidFormat, "JSON content type required")
                .into_response()
        }
        JsonRejection::BytesRejection(_) => {
            ApiError::with_code(ErrorCode::InvalidInput, "Invalid request body format")
                .into_response()
        }
        _ => ApiError::with_details(
            ErrorCode::InvalidInput,
            "Invalid input provided",
            format!("{:?}", err),
        )
        .into_response(),
    }
}
