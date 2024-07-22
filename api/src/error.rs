// error.rs
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error; // Using thiserror crate for easier error definition

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    DbError(#[from] sqlx::Error),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Resource not found")]
    NotFound,

    #[error("Webhook delivery failed: {0}")]
    WebhookError(#[from] reqwest::Error), // If using reqwest for webhooks

    #[error("Internal server error")]
    InternalServerError,
}

// Implement IntoResponse for AppError
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::DbError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string()),
            AppError::InvalidInput(msg) => (StatusCode::BAD_REQUEST, msg),
            // AppError::InvalidInput(_) => (StatusCode::BAD_REQUEST, "TODO"),
            AppError::NotFound => (StatusCode::NOT_FOUND, "Not found".to_string()),
            AppError::WebhookError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Webhook delivery failed".to_string()),
            AppError::InternalServerError => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string()),
        };

        let body = Json(json!({
            "error": error_message,
        }));

        (status, body).into_response()
    }
}
