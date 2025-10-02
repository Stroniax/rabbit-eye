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

use crate::fs::check_and_report_files;

mod fs;

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
    let state = std::sync::Arc::new(tokio::sync::Mutex::new(
        rabbit_eye::state::DefaultTableState::<String, u64>::default(),
    ));

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
            if handle.is_finished() {
                // Join to observe result
                handle.await?;
            } else {
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
                        _ = handle.await;
                    }
                };
            }
        }

        let channel_clone = rmq.default_channel().clone();
        let cancel = cancel.clone();
        let state = state.clone();

        doing_work = Some(tokio::spawn(async move {
            let mut state = state.lock().await;
            fs::check_and_report_files(&channel_clone, &cancel, &mut *state)
                .await
                .unwrap();
        }));
    }

    println!("Application gracefully stopped.");

    Ok(())
}

async fn do_poll<State>(
    channel: Channel,
    state: &mut State,
    cancel: CancellationToken,
) -> Result<(), Box<dyn Error>>
where
    State: TableState<String, u64>,
{
    tokio::select! {
        _ = wait_5s_after_cancel(&cancel) => {
            // just stopping this stops the other one
        },
        _ = check_and_report_files(&channel, &cancel, &mut *state) => {
            // If this completes though, all is well
        }
    };

    Ok(())
}

async fn wait_5s_after_cancel(cancel: &CancellationToken) -> () {
    cancel.cancelled().await;
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
}

async fn do_poll_loop<State>(
    cancel: &CancellationToken,
    channel: Channel,
    mut state: State,
) -> Result<(), Box<dyn Error>>
where
    State: TableState<String, u64>,
{
    let mut child_token = None;
    let mut last_work = None;

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));

    let mut state_mutex = Arc::new(Mutex::new(state));
    loop {
        let cont = tokio::select! {
            _ = tokio::signal::ctrl_c() => Err(CtrlC),
            _ = cancel.cancelled() => Err(CtrlC),
            _ = interval.tick() => Ok(()),
        };

        if let Err(_) = cont {
            break;
        }

        child_token
            .take()
            .iter()
            .for_each(|t: &CancellationToken| t.cancel());

        let child = cancel.child_token();
        let mut state_inner = state_mutex.lock().await;
        let work = tokio::spawn(async move {
            let mut state = *state_inner;
            do_poll(channel.clone(), &mut state, child.clone()).await;
        });
        child_token = Some(child);
    }

    Ok(())
}
