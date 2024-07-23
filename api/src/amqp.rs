use std::env;
use amqprs::{callbacks::{DefaultChannelCallback, DefaultConnectionCallback}, channel::{
    BasicPublishArguments, QueueBindArguments, QueueDeclareArguments,
}, connection::{Connection, OpenConnectionArguments}, BasicProperties};

pub async fn send(message: &str) {
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

    channel
        .basic_publish(BasicProperties::default(), message.to_string().into_bytes(), args)
        .await
        .unwrap();

    channel.close().await.unwrap();
    connection.close().await.unwrap();
}