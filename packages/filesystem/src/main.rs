use amqprs::connection::{Connection, OpenConnectionArguments};
use std::{env, error::Error};
use tokio::{select, task::JoinHandle};
use tokio_util::sync::CancellationToken;

mod fs;
mod lifetime;
mod rabbit;
mod state;
mod sync;

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

    let canc = CancellationToken::new();
    let ctx = tokio::sync::Mutex::new(lifetime::AppLifetime::new());

    loop {
        let res_or_stop = {
            let mut lifetime = ctx.lock().await;
            lifetime::race_sigterm(interval.tick(), &mut *lifetime).await
        };

        if let Err(_) = res_or_stop {
            print!("Ctrl+C pressed.");
            if let Some(handle) = doing_work.take() {
                println!(" Halting the application.");
                if let Err(_) = lifetime::try_graceful_shutdown(handle, &canc).await {
                    panic!("Failed to gracefully stop the application. Work was aborted.");
                }
            } else {
                println!(" No work is in progress.");
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
            fs::check_and_report_files(&channel_clone).await.unwrap();

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
