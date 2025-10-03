use std::{
    error::Error,
    fmt::Display,
    time::{Duration, Instant},
};
use tokio::{
    select,
    signal::ctrl_c,
    spawn,
    task::{JoinError, JoinHandle},
    time::{interval, sleep},
};
use tokio_util::sync::CancellationToken;

pub async fn run() -> Result<(), Box<dyn Error>> {
    // This token is the master token of the application. When it is triggered
    // the program aborts immediately.
    let abort_token = CancellationToken::new();

    // This token is the first to cancel, which stops iterating but does not stop any ongoing work.
    let stop_looping_token = abort_token.child_token();

    // This token is the second to cancel, which gives ongoing work an opportunity to stop.
    let graceful_stop_token = stop_looping_token.child_token();

    // This token is the clone that `cancel` is called on when 5s after the Ctrl+C
    let ctrlc_stop_looping_token = stop_looping_token.clone();
    let ctrlc_graceful_stop_token = graceful_stop_token.clone();
    let ctrlc_abort_token = abort_token.clone();

    _ = spawn(async move {
        if let Err(_) = ctrl_c().await {
            eprintln!("Failed to register listener for Ctrl+C.");
            return;
        }

        // Stop the loop from iterating.
        eprintln!("Shutdown in progress. Ctrl+C pressed. Allowing program to complete.");
        ctrlc_stop_looping_token.cancel();

        // Allow 5s for regular termination
        let secs_5 = Duration::from_secs(5);
        sleep(secs_5).await;

        // Trigger cancellation token and allow 5s for graceful termination
        eprintln!("Shutdown in progress. Gracefully terminating running processes.");
        ctrlc_graceful_stop_token.cancel();
        sleep(secs_5).await;

        // Abort the listener
        eprintln!("Shutdown in progress. Aborting running processes.");
        ctrlc_abort_token.cancel();
    });

    let loop_task = run_loop(stop_looping_token, graceful_stop_token);
    let loop_task_cancelable = abort_token.run_until_cancelled_owned(loop_task).await;

    loop_task_cancelable.unwrap_or(Err(Box::new(CancelledError)))?;

    Ok(())
}

#[derive(Debug)]
struct CancelledError;

impl Display for CancelledError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Cancelled")?;

        Ok(())
    }
}

impl Error for CancelledError {
    fn cause(&self) -> Option<&dyn Error> {
        None
    }

    fn description(&self) -> &str {
        "The operation was cancelled."
    }

    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

async fn run_loop(
    stop_looping: CancellationToken,
    graceful_stop: CancellationToken,
) -> Result<(), Box<dyn Error>> {
    let duration = Duration::from_secs(5);
    let mut interval = interval(duration);
    let mut iter_i = 0;

    let mut work_handle: Option<JoinHandleWithTimeout<_>> = None;

    loop {
        println!("Waiting for next scheduled iteration {}.", iter_i);
        let wait_result = stop_looping.run_until_cancelled(interval.tick()).await;

        match wait_result {
            Some(instant) => {
                println!("{:?} Interval {} elapsed.", instant.elapsed(), iter_i);
                iter_i += 1;

                if let Some(old_work) = work_handle.take() {
                    println!(
                        "{:?} Waiting for previous iteration to complete.",
                        instant.elapsed()
                    );
                    old_work.run_until_abort().await?;
                }

                let iter_n = iter_i;
                let stop_subtask = graceful_stop.child_token();
                let stop_previous = stop_subtask.clone();
                let work = spawn(async move {
                    _ = run_iter(iter_n, stop_subtask).await;
                });
                work_handle = Some(JoinHandleWithTimeout::new(work, stop_previous));
            }
            None => {
                println!("Interval interrupted.");
                break;
            }
        }
    }

    if let Some((work, _)) = work_handle {
        eprintln!("Waiting for last work to clean up.");
        work.await?;
    }

    Ok(())
}

async fn run_iter(i: usize, graceful_stop: CancellationToken) -> Result<(), Box<dyn Error>> {
    println!("Running iteration {}.", i);
    graceful_stop
        .run_until_cancelled(sleep(Duration::from_secs(10)))
        .await
        .map_or_else(|| Err(CancelledError), |_| Ok(()))
        .inspect_err(|_| eprintln!("Iteration {} was terminated.", i))?;
    println!("Iteration complete {}.", i);
    Ok(())
}

struct JoinHandleWithTimeout<T> {
    handle: JoinHandle<T>,
    cancel: CancellationToken,
}

impl<T> JoinHandleWithTimeout<T> {
    fn new(handle: JoinHandle<T>, cancel: CancellationToken) -> Self {
        Self { handle, cancel }
    }

    /// Waits for the join handle to finish. If the token is cancelled,
    /// the join handle is aborted.
    async fn run_until_abort(mut self) -> Result<T, JoinError> {
        select! {
            r = &mut self.handle => {
                r
            },
            _ = self.cancel.cancelled() => {
                self.handle.abort();
                self.handle.await
            }
        }
    }
}
