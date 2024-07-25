use std::env;
use amqprs::{callbacks::{DefaultChannelCallback, DefaultConnectionCallback}, channel::{
    BasicPublishArguments, QueueBindArguments, QueueDeclareArguments,
}, connection::{Connection, OpenConnectionArguments}, BasicProperties};
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::models::Wallet;

#[derive(Serialize, Deserialize)]
pub struct Message {
    address: String,
    is_active: bool,
    wallet_id: String,
}

impl From<Wallet> for Message {
    fn from(wallet: Wallet) -> Self {
        Message::new(wallet.data.address, wallet.is_active, wallet.id.to_string())
    }
}

impl Message {
    pub fn new(address: String, is_active: bool, wallet_id: String) -> Self {
        Message { address, is_active, wallet_id: wallet_id.to_string() }
    }

    pub async fn send(self) {
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

        let args = BasicPublishArguments::new(exchange_name, routing_key);

        let mut bytes: Vec<u8> = Vec::new();
        serde_json::to_writer(&mut bytes, &json!(self)).unwrap();

        channel
            .basic_publish(
                BasicProperties::default(),
                bytes,
                args
            )
            .await
            .unwrap();

        channel.close().await.unwrap();
        connection.close().await.unwrap();
    }
}