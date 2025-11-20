use crate::error::{ApiError, ErrorCode};
use axum::{
    body::Body,
    extract::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::Error as JsonError;

/// Middleware to handle JSON deserialization errors and convert them to structured error responses
pub async fn json_validation_middleware(
    request: Request,
    next: Next,
) -> Result<Response, Response> {
    let (parts, body) = request.into_parts();

    tracing::debug!(
        "JSON validation middleware processing request: {} {}",
        parts.method,
        parts.uri
    );

    // Check if this is a JSON request with a body
    let content_type = parts.headers.get("content-type");
    let is_json = content_type
        .and_then(|ct| ct.to_str().ok())
        .map(|ct| ct.contains("application/json"))
        .unwrap_or(false);

    if !is_json
        || parts.method == axum::http::Method::GET
        || parts.method == axum::http::Method::DELETE
    {
        // Not a JSON request or no body, proceed normally
        let request = Request::from_parts(parts, body);
        Ok(next.run(request).await)
    } else {
        // This is a JSON request with a body, we need to validate it
        let bytes = match axum::body::to_bytes(body, usize::MAX).await {
            Ok(bytes) => bytes,
            Err(_) => {
                let error =
                    ApiError::with_code(ErrorCode::InvalidInput, "Invalid request body format");
                return Err(error.into_response());
            }
        };

        // Try to parse the JSON to catch deserialization errors early
        match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(_) => {
                // JSON is syntactically valid, reconstruct request and proceed
                // The actual field validation will happen in the handler
                let body = Body::from(bytes);
                let request = Request::from_parts(parts, body);
                Ok(next.run(request).await)
            }
            Err(json_error) => {
                // JSON parsing failed, return structured error
                let api_error = handle_json_error(json_error);
                Err(api_error.into_response())
            }
        }
    }
}

/// Convert JSON deserialization errors to structured API errors
fn handle_json_error(error: JsonError) -> ApiError {
    let error_str = error.to_string();

    // Check for specific error patterns to provide better error messages
    if error_str.contains("unknown field") {
        // Extract the field name from the error message
        let field_name = extract_field_name(&error_str);

        if let Some(field) = field_name {
            if field == "role" {
                ApiError::with_details(
                    ErrorCode::InvalidInput,
                    "Role cannot be specified during registration",
                    "All users are assigned to 'user' role by default. Role assignment is handled by administrators.",
                )
            } else {
                ApiError::with_code(ErrorCode::InvalidInput, "Invalid input provided")
            }
        } else {
            ApiError::with_code(
                ErrorCode::InvalidInput,
                "Unknown field in request. Please check the API documentation.",
            )
        }
    } else if error_str.contains("missing field") {
        let field_name = extract_missing_field(&error_str);
        if let Some(field) = field_name {
            ApiError::validation_field(&field, format!("Required field '{}' is missing", field))
        } else {
            ApiError::with_code(ErrorCode::MissingRequiredField, "Required field is missing")
        }
    } else if error_str.contains("invalid type") {
        ApiError::with_code(ErrorCode::InvalidFormat, "Invalid data format provided")
    } else if error_str.contains("expected") && error_str.contains("at line") {
        ApiError::with_code(ErrorCode::InvalidInput, "Invalid JSON structure")
    } else {
        // Generic JSON parsing error
        ApiError::with_code(
            ErrorCode::InvalidFormat,
            "Invalid JSON format in request body",
        )
    }
}

/// Extract field name from "unknown field `field_name`" error message
fn extract_field_name(error_msg: &str) -> Option<String> {
    // Pattern: "unknown field `field_name`"
    if let Some(start) = error_msg.find("unknown field `") {
        let start = start + "unknown field `".len();
        if let Some(end) = error_msg[start..].find('`') {
            return Some(error_msg[start..start + end].to_string());
        }
    }
    None
}

/// Extract missing field name from "missing field `field_name`" error message
fn extract_missing_field(error_msg: &str) -> Option<String> {
    // Pattern: "missing field `field_name`"
    if let Some(start) = error_msg.find("missing field `") {
        let start = start + "missing field `".len();
        if let Some(end) = error_msg[start..].find('`') {
            return Some(error_msg[start..start + end].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_field_name() {
        let error = "unknown field `role`, expected one of `username`, `email`, `password`, `first_name`, `last_name` at line 7 column 10";
        assert_eq!(extract_field_name(error), Some("role".to_string()));
    }

    #[test]
    fn test_extract_missing_field() {
        let error = "missing field `email` at line 3 column 5";
        assert_eq!(extract_missing_field(error), Some("email".to_string()));
    }

    #[test]
    fn test_handle_role_error() {
        #[derive(serde::Deserialize, Debug)]
        #[serde(deny_unknown_fields)]
        struct TestUser {
            _username: String,
        }

        let json_error =
            serde_json::from_str::<TestUser>(r#"{"_username": "test", "role": "admin"}"#)
                .unwrap_err();

        let api_error = handle_json_error(json_error);
        match api_error {
            ApiError::WithCodeAndDetails(code, message, details) => {
                assert_eq!(code, ErrorCode::InvalidInput);
                assert!(message.contains("Role cannot be specified"));
                assert!(details.contains("assigned to 'user' role by default"));
            }
            _ => panic!("Expected WithCodeAndDetails error"),
        }
    }
}
