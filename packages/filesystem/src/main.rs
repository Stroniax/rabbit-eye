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

    let mut cancel = CancellationToken::new();
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

        if let Some(mut handle) = doing_work.take() {
            println!("Work was ongoing. It is being stopped.");

            // Try to gracefully stop it. In case there are too many changes to observe in
            // the window, and we can batch through because a diff may be faster than full
            cancel.cancel();
            cancel = CancellationToken::new();

            tokio::select! {
                _ = &mut handle => {
                    eprintln!("Gracefully stopped a previous iteration.");
                },
                _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                    eprintln!("Aborting a previous iteration.");
                    handle.abort();
                }
            };
        }

        let channel_clone = rmq.default_channel().clone();
        let cancel = cancel.clone();
        let state = state.clone();

        doing_work = Some(tokio::spawn(async move {
            let mut state = state.lock().await;
            fs::check_and_report_files(&channel_clone, &cancel, &mut state)
                .await
                .unwrap();
        }));
    }

    println!("Application gracefully stopped.");

    Ok(())
}
