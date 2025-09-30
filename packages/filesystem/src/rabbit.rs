use amqprs::{channel::Channel, connection::Connection};

struct ConnectionOptions {
    host: String,
    user: String,
    pass: String,
}

struct RabbitMq {
    connection: Connection,
    default_channel: Channel,
}

impl RabbitMq {
    // pub async fn connect(opts: ConnectionOptions) -> Result<RabbitMq, amqprs::error::Error> {}
}
