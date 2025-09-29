use std::{env, error::Error, os::windows::fs::MetadataExt};

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

    struct CtrlC;

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

        if let Err(_) = res_or_stop {
            print!("Ctrl+C pressed.");
            if let Some(handle) = doing_work.take() {
                let ten_seconds = tokio::time::Duration::from_secs(10);
                println!(" Waiting 10s for work to finish.");
                let timed_out = tokio::time::timeout(ten_seconds, handle).await;
                match timed_out {
                    Err(_) => {
                        println!("Aborted after time-out.");
                        todo!("Panic here or allow graceful shutdown to report.");
                    }
                    Ok(result) => result?,
                }
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
        doing_work = Some(tokio::spawn(async move {
            check_and_report_files(&channel_clone).await.unwrap();

            let mut i = 0;
            let _pod = PrintOnDrop;
            loop {
                println!("Doing some work (iter {}).", i);
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                i = i + 1;
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
