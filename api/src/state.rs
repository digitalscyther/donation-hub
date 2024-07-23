use reqwest::Client;
use sqlx::PgPool;


pub struct AppState {
    pub db: PgPool,
    pub http_client: Client,
}
