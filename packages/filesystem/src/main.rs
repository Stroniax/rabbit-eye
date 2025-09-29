use amqprs::{
    BasicProperties,
    channel::{BasicPublishArguments, Channel},
    connection::{Connection, OpenConnectionArguments},
};
use std::{env, error::Error, os::windows::fs::MetadataExt};
use tokio::{select, task::JoinHandle};
use tokio_util::sync::CancellationToken;

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

    let period = tokio::time::Duration::from_secs(5);
    let mut interval = tokio::time::interval(period);

    let mut doing_work: Option<JoinHandle<()>> = None;

    let mut ctrlc = tokio::spawn(async {
        tokio::signal::ctrl_c()
            .await
            .map_err(|e| format!("Ctrl+C handler error: {:?}", e))?;

        println!("Ctrl+C pressed. Signaling shutdown.");

        Ok::<(), String>(())
    });

    let canc = CancellationToken::new();

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
                canc.cancel();
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
                panic!("The program could not stop gracefully. Work had to be aborted.");
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

                if canc.is_cancelled() {
                    println!("Cancellation requested. Breaking loop.");
                    break;
                }
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
