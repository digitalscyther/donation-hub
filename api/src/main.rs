use std::env;
use axum::{routing::{get, post, put, delete}, Router, extract::{Path, State, Json}, http::StatusCode, response::IntoResponse, http, Extension};
use std::sync::Arc;
use axum::extract::Request;
use axum::middleware::{self, Next};
use axum::response::Response;
use log::{error, info};
use reqwest::{Client};
use serde_json::json;
use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

mod error;
mod models;
mod state;
mod amqp;
mod hdwallet;
mod transaction;

use crate::error::AppError;
use crate::models::{Donation, JsonDonation, JsonWallet, User, Wallet, WalletData};
use crate::state::AppState;

#[tokio::main]
async fn main() {
    env_logger::init();

    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db = models::get_connection(&db_url).await.expect("Failed to connect to database");

    let http_client = Client::new();

    let app_state = AppState { db, http_client };
    let routes = create_routes(Arc::new(app_state));

    let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let bind_address = format!("{}:{}", host, port);
    info!("Listening on {}", bind_address);
    let listener = tokio::net::TcpListener::bind(bind_address)
        .await
        .unwrap();

    axum::serve(listener, routes.into_make_service()).await.unwrap();
}

fn create_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/donations", post(create_donation))
        .route("/donations", get(list_donation))
        .route("/donations/:id", get(get_donation))
        .route("/donations/:id/transactions", get(get_donation_transactions))
        .route("/donations/:id/transactions/sum", get(get_donation_transactions_sum))
        .route("/donations/:id", put(update_donation))
        .route("/donations/:id", delete(delete_donation))
        .route("/wallets", post(create_wallet))
        .route("/wallets", get(list_wallet))
        .route("/wallets/:id", get(get_wallet))
        .route("/wallets/:id", put(update_wallet))
        .route("/wallets/:id", delete(delete_wallet))
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(auth))
        .with_state(state)
}

async fn create_donation(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<User>,
    Json(j_in_donation): Json<JsonDonation>,
) -> Result<impl IntoResponse, AppError> {
    let in_donation: Donation = j_in_donation.into();

    let j_out_donation: JsonDonation = in_donation.create(user.id, &state.db).await?.into();

    Ok((StatusCode::CREATED, Json(j_out_donation)))
}

async fn gen_wallet(http_client: &Client) -> Result<(String, String), AppError> {
    let (private_key, address) = hdwallet::gen_wallet(http_client)
        .await
        .map_err(|err_msg| {
            error!("Failed gen wallet: {}", err_msg);
            return AppError::InternalServerError
        })?;
    Ok((private_key, address))
}

async fn get_donation(
    Path(id_str): Path<String>,
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<User>,
) -> Result<impl IntoResponse, AppError> {
    let j_wallet: JsonDonation = get_json_donation(&id_str, user.id, &state.db).await?;
    Ok(Json(j_wallet))
}

async fn get_json_donation(id_str: &str, user_id: Uuid, db: &PgPool) -> Result<JsonDonation, AppError> {
    let id = Uuid::parse_str(id_str).map_err(
        |_| return AppError::InvalidInput("Invalid id".to_string())
    )?;

    Ok(Donation::get(id, user_id, db)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => AppError::NotFound,
            _ => AppError::DbError(e),
        })?
        .into())
}

async fn get_donation_transactions(
    Path(id_str): Path<String>,
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<User>,
) -> Result<impl IntoResponse, AppError> {
    let j_donation: JsonDonation = get_json_donation(&id_str, user.id, &state.db).await?;

    if let Some(wid) = j_donation.wallet_id {
        let j_wallet = get_json_wallet(&wid, user.id, &state.db).await?;
        let wallet_data: WalletData = j_wallet.data.unwrap().into();
        let result = transaction::get_transactions(
           &state.http_client, &wallet_data.address,
        ).await.map_err(|_| AppError::InternalServerError)?;

        return Ok(Json(result))
    }

    Ok(Json(json!([])))
}

async fn get_donation_transactions_sum(
    Path(id_str): Path<String>,
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<User>,
) -> Result<impl IntoResponse, AppError> {
    let j_donation: JsonDonation = get_json_donation(&id_str, user.id, &state.db).await?;

    if let Some(wid) = j_donation.wallet_id {
        let j_wallet = get_json_wallet(&wid, user.id, &state.db).await?;
        let wallet_data: WalletData = j_wallet.data.unwrap().into();
        let result = transaction::get_transactions_sum(
           &state.http_client, &wallet_data.address,
        ).await.map_err(|_| AppError::InternalServerError)?;

        return Ok(Json(result))
    }

    Ok(Json(json!({"result": 0})))
}

async fn update_donation(
    Path(id_str): Path<String>,
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<User>,
    Json(j_in_donation): Json<JsonDonation>,
) -> Result<impl IntoResponse, AppError> {
    let id = Uuid::parse_str(&id_str).map_err(
        |_| return AppError::InvalidInput("Invalid id".to_string())
    )?;
    let in_donation: Donation = j_in_donation.into();
    let j_out_donation: JsonDonation = in_donation.update(id, user.id, &state.db)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => AppError::NotFound,
            _ => AppError::DbError(e),
        })?
        .into();

    Ok((StatusCode::OK, Json(j_out_donation)))
}

async fn delete_donation(
    Path(id_str): Path<String>,
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<User>,
) -> Result<impl IntoResponse, AppError> {
    let id = Uuid::parse_str(&id_str).map_err(
        |_| return AppError::InvalidInput("Invalid id".to_string())
    )?;

    if Donation::delete(id, user.id, &state.db)
        .await
        .map_err(AppError::DbError)?
        .rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn list_donation(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<User>,
) -> Result<impl IntoResponse, AppError> {
    let donations: Vec<Donation> = Donation::list(user.id, &state.db)
        .await
        .map_err(AppError::DbError)?;

    let j_wallets: Vec<JsonDonation> = donations.into_iter().map(|donation| donation.into()).collect();
    Ok(Json(j_wallets))
}

async fn auth(mut req: Request, next: Next) -> Result<Response, StatusCode> {
    let auth_header = req.headers()
        .get(http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    let auth_header = if let Some(auth_header) = auth_header {
        auth_header
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    if let Some(current_user) = authorize_current_user(auth_header).await {
        // insert the current user into a request extension so the handler can
        // extract it
        req.extensions_mut().insert(current_user);
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

async fn authorize_current_user(auth_token: &str) -> Option<User> {
    let user_id = match Uuid::parse_str(&auth_token) {
        Ok(uuid) => uuid,
        Err(_) => return None
    };

    match User::get(user_id).await {
        Ok(user) => Some(user),
        Err(sqlx::Error::RowNotFound) => None,
        Err(e) => {
            error!("{:?}", e);
            None
        },
    }
}

async fn create_wallet(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<User>,
    Json(mut j_in_wallet): Json<JsonWallet>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(data) = j_in_wallet.data.clone() {
        let data: WalletData = data.try_into().map_err(
            |_| {
                return AppError::InvalidInput("Invalid data".to_string())
            }
        )?;
        data.validate().map_err(|err| return AppError::InvalidInput(err))?;
    } else {
        let (private_key, address) = gen_wallet(&state.http_client).await?;
        j_in_wallet.data = Some(json!(WalletData { address, private_key: Some(private_key) }))
    }

    let in_wallet: Wallet = j_in_wallet.into();

    let out_wallet: Wallet = Wallet::create(
        &state.db,
        in_wallet.data.address,
        in_wallet.data.private_key,
        in_wallet.is_active,
        user.id,
    ).await?;

    let j_out_wallet: JsonWallet = out_wallet.clone().into();

    if out_wallet.is_active {
        let msg: amqp::Message = out_wallet.into();
        msg.send().await;
    }

    Ok((StatusCode::CREATED, Json(j_out_wallet)))
}

async fn list_wallet(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<User>,
) -> Result<impl IntoResponse, AppError> {
    let wallets: Vec<Wallet> = Wallet::list(user.id, &state.db)
        .await
        .map_err(AppError::DbError)?;

    let j_wallets: Vec<JsonWallet> = wallets.into_iter().map(|wallet| wallet.into()).collect();
    Ok(Json(j_wallets))
}

async fn get_wallet(
    Path(id_str): Path<String>,
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<User>,
) -> Result<impl IntoResponse, AppError> {
    let j_wallet: JsonWallet = get_json_wallet(&id_str, user.id, &state.db).await?;

    Ok(Json(j_wallet))
}

async fn get_json_wallet(id_str: &str, user_id: Uuid, db: &PgPool) -> Result<JsonWallet, AppError> {
    let id = Uuid::parse_str(id_str).map_err(
        |_| return AppError::InvalidInput("Invalid id".to_string())
    )?;

    Ok(Wallet::get(db, id, user_id)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => AppError::NotFound,
            _ => AppError::DbError(e),
        })?
        .into())
}

async fn update_wallet(
    Path(id_str): Path<String>,
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<User>,
    Json(j_in_wallet): Json<JsonWallet>,
) -> Result<impl IntoResponse, AppError> {
    let id = Uuid::parse_str(&id_str).map_err(
        |_| return AppError::InvalidInput("Invalid id".to_string())
    )?;
    let in_wallet: Wallet = j_in_wallet.into();
    let out_wallet: Wallet = in_wallet.update(id, user.id, &state.db)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => AppError::NotFound,
            _ => AppError::DbError(e),
        })?;

    let j_out_wallet: JsonWallet = out_wallet.clone().into();

    let msg: amqp::Message = out_wallet.into();
    msg.send().await;

    Ok((StatusCode::OK, Json(j_out_wallet)))
}

async fn delete_wallet(
    Path(id_str): Path<String>,
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<User>,
) -> Result<impl IntoResponse, AppError> {
    let id = Uuid::parse_str(&id_str).map_err(
        |_| return AppError::InvalidInput("Invalid id".to_string())
    )?;

    let donations_ids = Donation::ids_by_wallet_id(id, user.id, &state.db)
        .await
        .map_err(AppError::DbError)?;
    if ! donations_ids.is_empty() {
        return Err(AppError::InvalidInput(
            format!(
                "Сan not be deleted, because linked to donations: {}",
                donations_ids
                .iter()
                .map(Uuid::to_string)
                .collect::<Vec<String>>()
                .join(", ")
            )
        ));
    }

    if Wallet::delete(id, user.id, &state.db)
        .await
        .map_err(AppError::DbError)?
        .rows_affected() == 0 {
            return Err(AppError::NotFound);
        }

    Ok(StatusCode::NO_CONTENT)
}
