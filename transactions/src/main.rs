use axum::{extract::{Json}, http::StatusCode, routing::{get, post}, Router};
use serde::{Deserialize, Serialize};
use std::{env};
use std::collections::HashSet;
use std::sync::Arc;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use rust_decimal::Decimal;
use log::{error, info, warn};
use serde_json::json;
use sqlx::{Error, PgPool};
use sqlx::types::time::OffsetDateTime;
use thiserror::Error;
use tower_http::trace::TraceLayer;

mod models;

use crate::models::{Transaction};

const DUPLICATE_CODE: &str = "23505";

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    DbError(#[from] sqlx::Error),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Resource not found")]
    NotFound,

    #[error("Internal server error")]
    InternalServerError,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::DbError(err) => {
                warn!("DB error: {:?}", err);
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
            },
            AppError::InvalidInput(msg) => (StatusCode::BAD_REQUEST, msg),
            // AppError::InvalidInput(_) => (StatusCode::BAD_REQUEST, "TODO"),
            AppError::NotFound => (StatusCode::NOT_FOUND, "Not found".to_string()),
            AppError::InternalServerError => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string()),
        };

        let body = Json(json!({
            "error": error_message,
        }));

        (status, body).into_response()
    }
}

pub struct AppState {
    pub db: PgPool,
}

#[derive(Deserialize, Serialize, Debug)]
struct JsonTransaction {
    id: String,
    address: String,
    amount: Decimal,
    r#type: String,
}

impl From<JsonTransaction> for Transaction {
    fn from(value: JsonTransaction) -> Self {
        Transaction {
            id: value.id,
            address: value.address,
            amount: value.amount,
            r#type: value.r#type,
            created_at: Some(OffsetDateTime::from_unix_timestamp(Utc::now().timestamp()).unwrap()),
        }
    }
}

impl Into<JsonTransaction> for Transaction {
    fn into(self) -> JsonTransaction {
        JsonTransaction {
            id: self.id,
            address: self.address,
            amount: self.amount,
            r#type: self.r#type,
        }
    }
}

async fn create(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<JsonTransaction>
) -> Result<impl IntoResponse, AppError> {
    let in_transaction: Transaction = payload.into();

    let response: JsonTransaction = in_transaction.create(&state.db).await.map_err(
        |err| {
            if let Error::Database(db_err) = &err {
                if db_err.code().unwrap() == DUPLICATE_CODE {
                    return AppError::InvalidInput("ID duplicate".to_string())
                }
            };
           return AppError::DbError(err);
        }
    )?.into();

    Ok((StatusCode::CREATED, Json(response)))
}

async fn create_batch(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<Vec<JsonTransaction>>
) -> Result<impl IntoResponse, AppError> {
    let mut created: HashSet<Transaction> = HashSet::new();
    let mut created_ids: HashSet<String> = HashSet::new();
    let mut duplicates: HashSet<String> = HashSet::new();
    let mut errors: HashSet<String> = HashSet::new();

    for item in payload.into_iter() {
        let transaction: Transaction = item.into();
        match transaction.clone().create(&state.db).await {
            Ok(tr) => {
                created.insert(tr.clone());
                created_ids.insert(tr.id);
            },
            Err(Error::Database(db_err)) if db_err.code().unwrap() == DUPLICATE_CODE => {
                if !created_ids.contains(&transaction.id) {
                    duplicates.insert(transaction.id);
                }
            },
            Err(e) => {
                errors.insert(transaction.id);
                error!("Failed create transaction: {:?}", e);
            }
        }
    }

    let created: Vec<JsonTransaction> = created.into_iter().map(|tr| tr.into()).collect();

    Ok((StatusCode::CREATED, Json(json!({"created": created, "duplicates": duplicates, "errors": errors}))))
}

async fn list(
    Path(address): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let transactions: Vec<Transaction> = Transaction::list(&address, &state.db)
        .await
        .map_err(AppError::DbError)?;

    let result: Vec<JsonTransaction> = transactions.into_iter().map(|donation| donation.into()).collect();
    Ok(Json(result))
}

async fn sum(
    Path(address): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let sum = Transaction::sum(&address, &state.db).await?;

    Ok((StatusCode::OK, Json(json!({"result": sum}))))
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db = models::get_connection(&db_url).await.expect("Failed to connect to database");

    let app_state = AppState { db };
    let state = Arc::new(app_state);

    let router = Router::new()
        .route("/transactions", post(create))
        .route("/transactions/batch", post(create_batch))
        .route("/transactions/:address", get(list))
        .route("/transactions/:address/sum", get(sum))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("PORT").unwrap_or_else(|_| "3001".to_string());
    let bind_address = format!("{}:{}", host, port);
    info!("Listening on {}", bind_address);
    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .unwrap();

    axum::serve(listener, router.into_make_service()).await.unwrap();
}
