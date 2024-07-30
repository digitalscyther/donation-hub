use log::{error, warn};
use reqwest::Error;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Deserialize;

#[derive(Deserialize)]
struct ApiResponse {
    token_transfers: Vec<TokenTransfer>,
}

#[derive(Deserialize)]
struct TokenTransfer {
    transaction_id: String,
    quant: String,
}

pub enum GetTransactionError {
    Request(Error),
    RetryAfter(Option<u64>)
}

pub async fn get_completed_transactions(address: &str, start_ts: Option<i64>, end_ts: Option<i64>) -> Result<Vec<(String, Decimal)>, GetTransactionError> {
    let url = "https://apilist.tronscanapi.com/api/token_trc20/transfers";
    let mut params = vec![
        ("confirm", "true".to_string()),
        ("toAddress", address.to_string()),
    ];
    for (key, ts) in [("start_timestamp", start_ts), ("end_timestamp", end_ts)] {
        if let Some(timestamp) = ts {
            params.push((key, timestamp.to_string()));
        }
    }

    let url = reqwest::Url::parse_with_params(url, &params).unwrap().to_string();
    let response = reqwest::get(&url).await.map_err(
        |err| GetTransactionError::Request(err)
    )?;

    let response_status = response.status();
    if vec![
        http::StatusCode::FORBIDDEN,
        http::StatusCode::REQUEST_TIMEOUT,
        http::StatusCode::GATEWAY_TIMEOUT
    ].contains(&response_status) {
        let retry_after: Option<u64> = Some(response
            .headers()
            .get("retry-after")
            .and_then(|hv| hv.to_str().ok())
            .and_then(|hv_str| hv_str.parse::<u64>().map_err(
                |e| {
                    warn!("Invalid format of retry=({}): {:?}", hv_str, e.clone());
                    e
                }
            ).ok())
            .unwrap_or(1000));
        return Err(GetTransactionError::RetryAfter(retry_after));
    }

    let data: Result<ApiResponse, _> = response.json().await;

    if let Err(err) = data {
        error!("Failed get response(code={}): {:?}", response_status, err);
        let res: Vec<(String, Decimal)> = vec![];
        return Ok(res)
    }

    let transactions: Vec<(String, Decimal)> = data.unwrap().token_transfers
        .into_iter()
        .map(|transfer| {
            let amount = transfer.quant;
            let amount: Decimal = Decimal::from_str_exact(&amount).unwrap() / dec!(1_000_000.0);
            (transfer.transaction_id, amount)
        })
        .collect();

    Ok(transactions)
}