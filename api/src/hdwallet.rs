use std::env;
use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize)]
struct ResponseBody {
    private_key: String,
    addresses: Addresses,
}

#[derive(Deserialize)]
struct Addresses {
    p2pkh: String,
}

pub async fn gen_wallet(client: &Client) -> Result<(String, String), String> {
    let request_body = serde_json::json!({
        "symbol": "TRX"
    });

    let response = client
        .post(
            env::var("GENERATE_WALLET_URL")
                .unwrap_or("http://localhost:8001/generate".to_string())
        )
        .json(&request_body)
        .send()
        .await
        .map_err(|err_msg| format!("Failed get response: {}", err_msg))?;

    let response_body: ResponseBody = response.json()
        .await
        .map_err(|err_msg| format!("Failed read response body: {}", err_msg))?;

    return Ok((response_body.private_key, response_body.addresses.p2pkh));
}