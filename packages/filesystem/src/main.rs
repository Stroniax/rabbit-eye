use std::error::Error;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

mod fs;
mod lifetime;
mod rabbit;
mod state;
mod sync;
mod time;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Connect to RabbitMQ
    let connection_options = rabbit::ConnectionOptions::read_from_env().expect(
        "Missing required environment variable RABBITMQ_HOST, RABBITMQ_USER, or RABBITMQ_PASS.",
    );
    let rmq = rabbit::RabbitMq::connect(connection_options)
        .await
        .expect("Unable to connect to RabbitMQ server.");

    // Run work in a loop
    let schedule_options = time::ScheduleOptions::default();
    let mut interval = tokio::time::interval(schedule_options.interval());

    let mut doing_work: Option<JoinHandle<()>> = None;

    let cancel = CancellationToken::new();
    let ctx = tokio::sync::Mutex::new(lifetime::AppLifetime::new());
    let state = std::sync::Arc::new(tokio::sync::Mutex::new(crate::state::State::empty()));

    loop {
        let res_or_stop = {
            let mut lifetime = ctx.lock().await;
            lifetime::race_sigterm(interval.tick(), &mut *lifetime).await
        };

        if let Err(_) = res_or_stop {
            print!("Ctrl+C pressed.");
            if let Some(handle) = doing_work.take() {
                println!(" Halting the application.");
                if let Err(_) = lifetime::try_graceful_shutdown(handle, &cancel).await {
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

        let channel_clone = rmq.default_channel().clone();
        let cancel = cancel.clone();
        let state = state.clone();

        doing_work = Some(tokio::spawn(async move {
            let mut state = state.lock().await;
            fs::check_and_report_files(&channel_clone, &cancel, &mut state)
                .await
                .unwrap();

            let mut i = 0;
            let _pod = PrintOnDrop;
            loop {
                println!("Doing some work (iter {}).", i);
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                i = i + 1;

                if cancel.is_cancelled() {
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
