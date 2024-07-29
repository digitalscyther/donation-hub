mod blockchain;

use std::env;
use amqprs::{callbacks::{DefaultChannelCallback, DefaultConnectionCallback}, channel::{
    BasicConsumeArguments, QueueBindArguments, QueueDeclareArguments,
}, connection::{Connection, OpenConnectionArguments}, consumer::AsyncConsumer, BasicProperties, Deliver};
use amqprs::channel::{BasicAckArguments, Channel};
use async_trait::async_trait;
use log::{error, info};
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, AsyncIter};
use tokio::{task, time};
use tokio::sync::{Notify};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use serde::{Deserialize, Serialize};
use std::str;
use rust_decimal::Decimal;

const PREFIX: &str = "wid:";
const RATE_LIMIT: i16 = 5;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    address: String,
    is_active: bool,
    wallet_id: String,
}

struct MyConsumer {
    no_ack: bool,
    redis: MultiplexedConnection,
}

impl MyConsumer {
    pub fn new(no_ack: bool, redis: MultiplexedConnection) -> Self {
        Self { no_ack, redis }
    }
}

#[async_trait]
impl AsyncConsumer for MyConsumer {
    async fn consume(
        &mut self,
        channel: &Channel,
        deliver: Deliver,
        _basic_properties: BasicProperties,
        content: Vec<u8>,
    ) {
        let msg: Message = serde_json::from_slice::<Message>(&content.clone()).unwrap().into();

        let r_key = format!("{}{}", PREFIX, msg.wallet_id.clone());
        if msg.is_active {
            self.redis.set::<String, Vec<u8>, ()>(r_key, content).await.unwrap();
        } else {
            self.redis.del::<String, i64>(r_key).await.unwrap();
        };

        if !self.no_ack {
            let args = BasicAckArguments::new(deliver.delivery_tag(), false);
            channel.basic_ack(args).await.unwrap();
        }
    }
}

#[tokio::main]
async fn main() {
    let redis_url = &env::var("REDIS_URL").unwrap_or("redis://127.0.0.1:6381/".to_string());
    let redis = get_redis_con(redis_url).await.unwrap();

    let r_clone = redis.clone();
    let t2 = task::spawn(async move {
        monitoring(r_clone).await;
    });

    let t1 = task::spawn(async move {
        rabbit_fn(redis).await;
    });

    let _ = tokio::join!(t1, t2);
}

async fn monitoring(mut redis: MultiplexedConnection) {
    let mut c = 0;
    loop {
        let pattern = format!("{}*", PREFIX);
        let mut keys: Vec<String> = vec![];
        let mut values: Vec<Vec<u8>> = vec![];
        let mut r_clone = redis.clone();
        let mut iterator: AsyncIter<String> = r_clone.scan_match(pattern).await.unwrap();
        while let Some(k) = iterator.next_item().await {
            keys.push(k);
        }

        for k in keys.into_iter() {
            let v = redis.get(k).await.unwrap();
            values.push(v);
        }

        if values.is_empty() {
            info!("[{}] Current map is empty", c);
        } else {
            let msgs = values.iter()
                .map(|bytes| {
                    serde_json::from_slice::<Message>(bytes).unwrap().into()
                })
                .collect();
            let ts = check_wallets(msgs).await;
            info!("[{}] Found transactions:", c);
            for t in ts.iter() {
                info!("# {:?}", t);
            }
        }
        c += 1;
    }
}

#[derive(Debug)]
struct Transaction {
    id: String,
    amount: Decimal,
}

async fn check_wallets(wallets: Vec<Message>) -> Vec<Transaction> {
    let mut transactions: Vec<Transaction> = vec![];
    let batch_size = (RATE_LIMIT + 1) as usize;
    let mut batch: Vec<Message> = vec![];

    async fn process_batch_and_sleep(b: Vec<Message>) -> (Vec<Transaction>, Vec<Message>) {
        let (to_add, to_repeat, sleep_seconds) = process_batch(b).await;
        if let Some(to_sleep) = sleep_seconds {
            time::sleep(time::Duration::from_secs(to_sleep)).await;
        }
        (to_add, to_repeat)
    }

    for item in wallets.into_iter() {
        match batch.len() >= batch_size {
            true => {
                let (to_add, to_repeat) = process_batch_and_sleep(batch).await;
                batch = to_repeat;
                transactions.extend(to_add);
            },
            false => batch.push(item)
        }
    }
    if !batch.is_empty() {
        transactions.extend(process_batch_and_sleep(batch).await.0);
    }

    return transactions;
}

async fn process_batch(batch: Vec<Message>) -> (Vec<Transaction>, Vec<Message>, Option<u64>) {
    let mut ts: Vec<Transaction> = vec![];
    let mut to_retry = vec![];
    for msg in batch.clone().into_iter() {
        match blockchain::get_completed_transactions(&msg.address, None, None).await {
            Ok(trs) => ts.extend(
                trs.into_iter()
                    .map(|(id, amount)| Transaction { id, amount })
                    .collect::<Vec<Transaction>>()
            ),
            Err(e) => {
                error!("{:?}", e);
                to_retry.push(msg);     // TODO possible infinity loop
            }
        };
    }
    let to_retry = vec![batch[0].clone()];
    return (ts, to_retry, Some(5));
}

async fn rabbit_fn(redis: MultiplexedConnection) {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .try_init()
        .ok();

    let connection = Connection::open(&OpenConnectionArguments::new(
        &env::var("RABBITMQ_HOST").unwrap_or("localhost".to_string()),
        5672,
        "guest",
        "guest",
    ))
        .await
        .unwrap();
    connection
        .register_callback(DefaultConnectionCallback)
        .await
        .unwrap();

    let channel = connection.open_channel(None).await.unwrap();
    channel
        .register_callback(DefaultChannelCallback)
        .await
        .unwrap();

    let (queue_name, _, _) = channel
        .queue_declare(QueueDeclareArguments::durable_client_named(
            "amqprs.examples.basic",
        ))
        .await
        .unwrap()
        .unwrap();

    let routing_key = "amqprs.example";
    let exchange_name = "amq.topic";
    channel
        .queue_bind(QueueBindArguments::new(
            &queue_name,
            exchange_name,
            routing_key,
        ))
        .await
        .unwrap();

    let args = BasicConsumeArguments::new(&queue_name, "example_basic_pub_sub");

    channel
        .basic_consume(MyConsumer::new(args.no_ack, redis), args)
        .await
        .unwrap();

    let guard = Notify::new();
    guard.notified().await;
}

async fn get_redis_con(url: &str) -> Result<MultiplexedConnection, String> {
    let client = redis::Client::open(url).unwrap();
    client.get_multiplexed_async_connection().await.map_err(
        |err| format!("Failed create connection:\n{:?}", err)
    )
}