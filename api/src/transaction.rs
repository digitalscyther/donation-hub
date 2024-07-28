use std::env;
use reqwest::{Client, Response};
use serde_json::Value;


async fn fetch_json_response(client: &Client, url: &str) -> Result<Value, String> {
    let response: Response = client.get(url)
        .send()
        .await
        .map_err(|err| format!("Failed to get response: {}", err))?;

    response.json()
        .await
        .map_err(|err| format!("Failed to read response body: {}", err))
}

pub async fn get_transactions(client: &Client, address: &str) -> Result<Value, String> {
    let base_url: String = env::var("TRANSACTIONS_URL")
        .unwrap_or_else(|_| "http://localhost:3002".to_string());
    let url: String = format!("{}/transactions/{}", base_url, address);

    fetch_json_response(client, &url).await
}

pub async fn get_transactions_sum(client: &Client, address: &str) -> Result<Value, String> {
    let base_url: String = env::var("TRANSACTIONS_URL")
        .unwrap_or_else(|_| "http://localhost:3002".to_string());
    let url: String = format!("{}/transactions/{}/sum", base_url, address);

    fetch_json_response(client, &url).await
}