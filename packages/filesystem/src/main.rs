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

mod lifetime {
    // This will have the application lifetime and functions for scheduling,
    // polling, etc.
}

mod rabbit {
    // This will have all of the communication stuff.
}

mod sync {
    // This will have cooperative cancellation logic
}

mod state {
    // This will have logic for tracking the state of the data that is polled.

    /// State of a singular row, compared with the computed states to see if there was a change.
    struct RowState {
        id: usize,
        hash: usize,
    }

    /// State of a set of data, first the tablehash is compared and if different, then the rowhash is
    /// compared to see if there was a change. If there is no way to determine a tablehash, the value
    /// should be set to 0 and the `ChangeDetector` should return a nonzero value.
    struct State {
        tablehash: Option<usize>,
        rowhash: Vec<RowState>,
    }

    use std::path::PathBuf;
    use tokio_util::sync::CancellationToken;

    trait ChangeDetector {
        type Change;

        /// Produces a hash of the entire observed set. If the change detector cannot reasonably
        /// hash the entire set, it should return None.
        async fn tablehash(&self, cancel: &CancellationToken) -> Option<usize>;

        /// Produces the change set from state. It does not need to modify `State` as the engine will
        /// handle updating each row. The returned `Vec` must consist of (rowid, hash, messagebody).
        /// If `cancel` is triggered during this call, return early with the changes that are known.
        async fn rowhash(
            &self,
            state: &State,
            cancel: &CancellationToken,
        ) -> Vec<(usize, usize, Self::Change)>;
    }

    struct FileChangeDetector {
        root: PathBuf,
    }

    impl ChangeDetector for FileChangeDetector {
        type Change = FileChange;

        async fn tablehash(&self, _cancel: &CancellationToken) -> Option<usize> {
            None
        }

        async fn rowhash(
            &self,
            _state: &State,
            _cancel: &CancellationToken,
        ) -> Vec<(usize, usize, Self::Change)> {
            vec![]
        }
    }

    struct FileChange {
        path: String,
    }
}
