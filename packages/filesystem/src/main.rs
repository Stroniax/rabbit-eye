use std::{
    env,
    error::Error,
    os::windows::fs::MetadataExt,
    sync::{Arc, Mutex},
};

use amqprs::{
    BasicProperties,
    channel::{BasicPublishArguments, Channel},
    connection::{Connection, OpenConnectionArguments},
};
use tokio::{select, task::JoinHandle};

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

    let period = tokio::time::Duration::from_millis(5_000);
    let mut interval = tokio::time::interval(period);

    let mut doing_work: Option<JoinHandle<()>> = None;

    let mut ctrlc = tokio::spawn(async {
        tokio::signal::ctrl_c()
            .await
            .map_err(|e| format!("Ctrl+C handler error: {:?}", e))?;

        println!("Ctrl+C pressed. Signaling shutdown.");

        Ok::<(), String>(())
    });

    let mut canc = CooperativeCancellation::new();

    loop {
        let res_or_stop = select! {
            _ = &mut ctrlc => {
                println!("Ctr+C pressed. Halting loop.");
                Err(CtrlC)
            }
            _ = interval.tick() => {
                println!("Starting next interval.");
                Ok(())
            }
        };
        // let canc = canc.clone();

        if let Err(_) = res_or_stop {
            print!("Ctrl+C pressed.");
            if let Some(mut handle) = doing_work.take() {
                // First, wait 5s to see if it completes on its own
                println!(" Waiting 5s for graceful completion.");
                let five_seconds = tokio::time::Duration::from_secs(5);
                select! {
                    _ = &mut handle => {
                        println!("Work finished while waiting five seconds for graceful completion.");
                        break;
                    }
                    _ = tokio::time::sleep(five_seconds) => {
                        println!("Work still ongoing after five seconds.");
                    }
                };

                // Then, trigger the cooperative cancellation and see if it completes on its own
                canc.cancel().await;
                println!("Waiting 5s for cooperative cancellation.");
                select! {
                    _ = &mut handle => {
                        println!("Work finished during cooperative cancellation.");
                        break;
                    },
                    _ = tokio::time::sleep(five_seconds) => {
                        println!("Work still ongoing after ten seconds.");
                    }
                };

                // Finally, abort because it will not stop gracefully
                println!("Aborting because no cancellation attempt was heeded.");
                handle.abort();
            } else {
                println!(" No work is ongoing.");
            }
            break;
        }

        match doing_work.take() {
            Some(handle) => {
                println!("Work was ongoing. It is being stopped.");
                handle.abort();
            }
            _ => {}
        }

        let channel_clone = channel.clone();
        let canc = canc.clone();
        doing_work = Some(tokio::spawn(async move {
            check_and_report_files(&channel_clone).await.unwrap();

            let mut i = 0;
            let _pod = PrintOnDrop;
            loop {
                println!("Doing some work (iter {}).", i);
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                i = i + 1;

                if i > 7 && canc.cancelled().await {
                    println!("Cancellation requested. Breaking loop.");
                    break;
                }
                // if i > 3 {
                //     break;
                // }
            }
        }));
    }

    println!("Application gracefully stopped.");

    Ok(())
}

struct PrintOnDrop;
impl Drop for PrintOnDrop {
    fn drop(&mut self) {
        println!("Dropped.");
    }
}

async fn check_and_report_files(channel: &Channel) -> Result<(), Box<dyn Error>> {
    let mut root = tokio::fs::read_dir("C:").await?;
    while let Some(dir_or_file) = root.next_entry().await? {
        let metadata = dir_or_file.metadata().await?;

        println!(
            "File {:?} last written to at {}",
            dir_or_file.file_name(),
            metadata.last_write_time()
        );
    }

    let publish_args = BasicPublishArguments::new("", "rabbit-eye-dev");
    channel
        .basic_publish(BasicProperties::default(), vec![], publish_args)
        .await?;

    Ok(())
}

struct CtrlC;

struct CooperativeCancellation {
    cancelled: Arc<tokio::sync::Mutex<bool>>,
}

impl CooperativeCancellation {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(tokio::sync::Mutex::new(false)),
        }
    }

    pub async fn cancelled(&self) -> bool {
        *self.cancelled.lock().await
    }

    pub async fn cancel(&mut self) -> () {
        let mut lock = self.cancelled.lock().await;
        *lock = true;
    }
}

impl Clone for CooperativeCancellation {
    fn clone(&self) -> Self {
        Self {
            cancelled: self.cancelled.clone(),
        }
    }
}
