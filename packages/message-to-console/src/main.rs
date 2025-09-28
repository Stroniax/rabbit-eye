use std::{env, error::Error, time::Duration};

use amqprs::{
    Ack, BasicProperties, Cancel, CloseChannel, Deliver, Nack, Return,
    callbacks::{self, ChannelCallback},
    channel::{
        BasicAckArguments, BasicConsumeArguments, Channel, QueueBindArguments,
        QueueDeclareArguments,
    },
    connection::{Connection, OpenConnectionArguments},
    consumer::{self, AsyncConsumer, DefaultConsumer},
};
use async_trait::async_trait;
use tokio::time;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = OpenConnectionArguments::new(
        &env::var("RABBITMQ_HOST").unwrap(),
        5672,
        &env::var("RABBITMQ_USER").unwrap(),
        &env::var("RABBITMQ_PASS").unwrap(),
    );
    let connection = Connection::open(&args).await?;

    let channel = connection.open_channel(None).await?;

    if !channel.is_open() {
        panic!("Channel is not open.");
    }

    println!("Channel open. Declaring queue...");

    let (queue, message_count, consumer_count) = channel
        .queue_declare(
            QueueDeclareArguments::new("rabbit-eye-dev")
                .durable(true)
                .finish(),
        )
        .await?
        .unwrap();

    println!(
        "Queue declared ({}, message count: {}, consumer count: {}). Still open? {}. Binding...",
        queue,
        message_count,
        consumer_count,
        channel.is_open()
    );

    // channel
    //     .queue_bind(
    //         QueueBindArguments::default()
    //             .queue(String::from("rabbit-eye-dev"))
    //             .finish(),
    //     )
    //     .await?;
    // println!("Queue bound. Registering callback...");

    channel.register_callback(EprintlnChannelCallback).await?;

    println!("Callback registered. Consuming...");

    let consume_args = BasicConsumeArguments::new(&queue, "");
    // let consumer = DefaultConsumer::new(false);
    let consumer = EprintlnConsumer;
    channel.basic_consume(consumer, consume_args).await?;

    println!("Consumer registered. Activating...");

    channel.flow(true).await?;

    println!("Flow active. Waiting...");

    time::sleep(Duration::from_millis(60_000)).await;

    println!("Wait completed. Closing application.");

    Ok(())
}

pub struct EprintlnChannelCallback;

#[async_trait]
impl ChannelCallback for EprintlnChannelCallback {
    async fn close(
        &mut self,
        channel: &Channel,
        close: CloseChannel,
    ) -> Result<(), amqprs::error::Error> {
        eprintln!(
            "handle close request for channel {}, cause: {}",
            channel, close
        );
        Ok(())
    }
    async fn cancel(
        &mut self,
        channel: &Channel,
        cancel: Cancel,
    ) -> Result<(), amqprs::error::Error> {
        eprintln!(
            "handle cancel request for consumer {} on channel {}",
            cancel.consumer_tag(),
            channel
        );
        Ok(())
    }
    async fn flow(
        &mut self,
        channel: &Channel,
        active: bool,
    ) -> Result<bool, amqprs::error::Error> {
        eprintln!(
            "handle flow request active={} for channel {}",
            active, channel
        );
        Ok(true)
    }
    async fn publish_ack(&mut self, channel: &Channel, ack: Ack) {
        eprintln!(
            "handle publish ack delivery_tag={} on channel {}",
            ack.delivery_tag(),
            channel
        );
    }
    async fn publish_nack(&mut self, channel: &Channel, nack: Nack) {
        eprintln!(
            "handle publish nack delivery_tag={} on channel {}",
            nack.delivery_tag(),
            channel
        );
    }
    async fn publish_return(
        &mut self,
        channel: &Channel,
        ret: Return,
        _basic_properties: BasicProperties,
        content: Vec<u8>,
    ) {
        eprintln!(
            "handle publish return {} on channel {}, content size: {}",
            ret,
            channel,
            content.len()
        );
    }
}

struct EprintlnConsumer;

#[async_trait]
impl AsyncConsumer for EprintlnConsumer {
    async fn consume(
        &mut self,
        channel: &Channel,
        deliver: Deliver,
        basic_properties: BasicProperties,
        content: Vec<u8>,
    ) {
        eprintln!(
            "Consumed message on channel {}: delivery_tag={}, content size={}",
            channel,
            deliver.delivery_tag(),
            content.len()
        );

        let result = channel
            .basic_ack(BasicAckArguments::new(deliver.delivery_tag(), false))
            .await;

        if let Err(e) = result {
            eprintln!("Error sending basic ack. {}", e);
        }
    }
}
