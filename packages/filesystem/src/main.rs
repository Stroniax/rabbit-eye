use std::{env, error::Error};

use amqprs::{
    BasicProperties,
    channel::BasicPublishArguments,
    connection::{Connection, OpenConnectionArguments},
};

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

    let publish_args = BasicPublishArguments::new("", "simple-queue");
    channel
        .basic_publish(BasicProperties::default(), vec![], publish_args)
        .await?;

    println!("Message sent.");

    Ok(())
}
