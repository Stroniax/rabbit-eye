use amqprs::{
    channel::Channel,
    connection::{Connection, OpenConnectionArguments},
};

pub struct ConnectionOptions {
    host: String,
    user: String,
    pass: String,
}

impl ConnectionOptions {
    /// Fails if an environment variable was not set.
    pub fn read_from_env() -> Result<Self, ()> {
        let map_err = |_| ();
        let host = std::env::var("RABBITMQ_HOST").map_err(&map_err)?;
        let user = std::env::var("RABBITMQ_USER").map_err(&map_err)?;
        let pass = std::env::var("RABBITMQ_PASS").map_err(&map_err)?;

        Ok(Self { host, user, pass })
    }
}

pub struct RabbitMq {
    connection: Connection,
    default_channel: Channel,
}

impl RabbitMq {
    pub async fn connect(opts: ConnectionOptions) -> Result<RabbitMq, amqprs::error::Error> {
        let connect_args = OpenConnectionArguments::new(&opts.host, 5672, &opts.user, &opts.pass);
        let connection = Connection::open(&connect_args).await?;
        let default_channel = connection.open_channel(None).await?;

        let rmq = Self {
            connection,
            default_channel,
        };
        Ok(rmq)
    }

    pub fn default_channel(&self) -> &Channel {
        &self.default_channel
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}
