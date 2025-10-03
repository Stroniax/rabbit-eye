use amqprs::channel::{self, Channel};
use rabbit_eye::{
    lifetime::{self, CtrlC},
    rabbit,
    state::TableState,
    sync, time,
};
use std::{error::Error, sync::Arc};
use tokio::{sync::Mutex, task::JoinHandle};
use tokio_util::sync::CancellationToken;

mod fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    rabbit_eye::engine::run().await
}
