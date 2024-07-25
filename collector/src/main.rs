use std::env;
use amqprs::{callbacks::{DefaultChannelCallback, DefaultConnectionCallback}, channel::{
    BasicConsumeArguments, QueueBindArguments, QueueDeclareArguments,
}, connection::{Connection, OpenConnectionArguments}, consumer::AsyncConsumer, BasicProperties, Deliver};
use amqprs::channel::{BasicAckArguments, Channel};
use async_trait::async_trait;
use log::{info};
use serde_json::Value;
use tokio::{task, time};
use tokio::sync::Notify;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

struct MyConsumer {
    no_ack: bool,
}

impl MyConsumer {
    pub fn new(no_ack: bool) -> Self {
        Self { no_ack }
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
        time::sleep(time::Duration::from_secs(3)).await;
        let data: Value = serde_json::from_slice(&content).unwrap();
        info!("received: {:?}", data);

        if !self.no_ack {
            let args = BasicAckArguments::new(deliver.delivery_tag(), false);
            channel.basic_ack(args).await.unwrap();
        }
    }
}

#[tokio::main]
async fn main() {
    let t1 = task::spawn(async move {
        rabbit_fn().await;
    });

    let t2 = task::spawn(async move {
        j_print().await;
    });

    let _ = tokio::join!(t1, t2);
}

async fn j_print() {
    let mut c = 0;
    loop {
        info!("j_print: {}", c);
        c += 1;
        time::sleep(time::Duration::from_secs(10*60)).await;
    }
}

async fn rabbit_fn() {
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
        .basic_consume(MyConsumer::new(args.no_ack), args)
        .await
        .unwrap();

    let guard = Notify::new();
    guard.notified().await;
}