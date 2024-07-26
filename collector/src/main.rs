use std::env;
use amqprs::{callbacks::{DefaultChannelCallback, DefaultConnectionCallback}, channel::{
    BasicConsumeArguments, QueueBindArguments, QueueDeclareArguments,
}, connection::{Connection, OpenConnectionArguments}, consumer::AsyncConsumer, BasicProperties, Deliver};
use amqprs::channel::{BasicAckArguments, Channel};
use async_trait::async_trait;
use log::{info};
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, AsyncIter};
use tokio::{task, time};
use tokio::sync::{Notify};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use serde::{Deserialize, Serialize};
use std::str;

const PREFIX: &str = "wid:";

#[derive(Debug, Serialize, Deserialize)]
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
        j_print(r_clone).await;
    });

    let t1 = task::spawn(async move {
        rabbit_fn(redis).await;
    });

    let _ = tokio::join!(t1, t2);
}

async fn j_print(mut redis: MultiplexedConnection) {
    let mut c = 0;
    loop {
        let pattern = format!("{}*", PREFIX);
        let mut values: Vec<String> = vec!();
        let mut iterator: AsyncIter<String> = redis.scan_match(pattern).await.unwrap();
        while let Some(item) = iterator.next_item().await {
            values.push(item)
        }

        if values.is_empty() {
            info!("[{}] Current map is empty", c);
        } else {
            info!("[{}] Current map contents:", c);
            for val in values.iter() {
                info!("{:?}", val);
            }
        }
        c += 1;
        time::sleep(time::Duration::from_secs(5)).await;
    }
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