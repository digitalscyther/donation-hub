use log::error;
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

pub async fn get_completed_transactions(address: &str, start_ts: Option<i64>, end_ts: Option<i64>) -> Result<Vec<(String, Decimal)>, Error> {
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
    let response = reqwest::get(&url).await?;
    let data: Result<ApiResponse, _> = response.json().await;

    if let Err(err) = data {
        error!("Failed get response: {:?}", err);
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