use amqprs::{
    Ack, BasicProperties, Cancel, CloseChannel, Deliver, Nack, Return,
    callbacks::ChannelCallback,
    channel::{BasicAckArguments, BasicConsumeArguments, Channel, QueueDeclareArguments},
    connection::{Connection, OpenConnectionArguments},
    consumer::AsyncConsumer,
};
use async_trait::async_trait;
use std::{env, error::Error};

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

    eprintln!("Channel open. Declaring queue...");

    let (queue, message_count, consumer_count) = channel
        .queue_declare(
            QueueDeclareArguments::new("rabbit-eye-dev")
                .durable(true)
                .finish(),
        )
        .await?
        .unwrap();

    eprintln!(
        "Queue declared ({}, message count: {}, consumer count: {}). Still open? {}. Binding...",
        queue,
        message_count,
        consumer_count,
        channel.is_open()
    );

    channel.register_callback(EprintlnChannelCallback).await?;

    eprintln!("Callback registered. Consuming...");

    let consume_args = BasicConsumeArguments::new(&queue, "");
    // let consumer = DefaultConsumer::new(false);
    let consumer = PrintlnConsumer;
    channel.basic_consume(consumer, consume_args).await?;

    eprintln!("Consumer registered. Activating...");

    channel.flow(true).await?;

    eprintln!("Flow active. Waiting...");

    tokio::signal::ctrl_c().await?;

    eprintln!("Ctrl+C received. Shutting down.");

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

struct PrintlnConsumer;

#[async_trait]
impl AsyncConsumer for PrintlnConsumer {
    async fn consume(
        &mut self,
        channel: &Channel,
        deliver: Deliver,
        _basic_properties: BasicProperties,
        content: Vec<u8>,
    ) {
        println!(
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
