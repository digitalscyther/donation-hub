use std::env;
use axum::{routing::{get, post, put, delete}, Router, extract::{Path, State, Json}, http::StatusCode, response::IntoResponse, http, Extension};
use std::sync::Arc;
use axum::extract::Request;
use axum::middleware::{self, Next};
use axum::response::Response;
use log::{error, info};
use reqwest::{Client};
use serde_json::json;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

mod error;
mod models;
mod state;
mod amqp;
mod hdwallet;

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
        .route("/donations", get(get_donations))
        .route("/donations/:id", get(get_donation))
        .route("/donations/:id", put(update_donation))
        .route("/donations/:id", delete(delete_donation))
        .route("/wallets", post(create_wallet))
        // .route("/wallets", get(get_wallets))
        // .route("/wallets/:id", get(get_wallet))
        // .route("/wallets/:id", put(update_wallet))
        // .route("/wallets/:id", delete(delete_wallet))
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(auth))
        .with_state(state)
}

async fn create_donation(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<User>,
    Json(j_in_donation): Json<JsonDonation>,
) -> Result<impl IntoResponse, AppError> {
    let mut in_donation: Donation = j_in_donation.into();
    let mut created_wallet = None;

    if let Some(wallet_id) = in_donation.wallet_id {
        Wallet::get(&state.db, wallet_id, user.id)
            .await
            .map_err(|_| return AppError::InvalidInput("Invalid wallet_id".to_string()))?;
    } else {
        let (private_key, address) = gen_wallet(&state.http_client).await?;
        let wallet = Wallet::create(
            &state.db, address.clone(), Some(private_key), user.id
        ).await?;
        in_donation.wallet_id = Some(wallet.id);
        created_wallet = Some(wallet);
    }

    let j_out_donation: JsonDonation = in_donation.create(user.id, &state.db).await?.into();
    if let Some(wallet) = created_wallet {
        amqp::send(&wallet.data.address).await;
    }

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
    let id = Uuid::parse_str(&id_str).map_err(
        |_| return AppError::InvalidInput("Invalid id".to_string())
    )?;

    let j_donation: JsonDonation = Donation::get(id, user.id, &state.db)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => AppError::NotFound,
            _ => AppError::DbError(e),
        })?
        .into();
    Ok(Json(j_donation))
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

async fn get_donations(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<User>,
) -> Result<impl IntoResponse, AppError> {
    info!("{:?}", user);
    let donations: Vec<Donation> = Donation::list(user.id, &state.db)
        .await
        .map_err(AppError::DbError)?;

    let j_donations: Vec<JsonDonation> = donations.into_iter().map(|donation| donation.into()).collect();
    Ok(Json(j_donations))
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

    let j_out_wallet: JsonWallet = Wallet::create(
        &state.db,
        in_wallet.data.address,
        in_wallet.data.private_key,
        user.id,
    ).await?.into();

    Ok((StatusCode::CREATED, Json(j_out_wallet)))
}
