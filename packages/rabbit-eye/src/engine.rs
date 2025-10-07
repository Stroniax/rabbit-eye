use std::{
    error::Error,
    fmt::Display,
    io::Write,
    time::{Duration, Instant},
};
use tokio::{
    select,
    signal::ctrl_c,
    spawn,
    task::{JoinError, JoinHandle},
    time::{Interval, interval, sleep, timeout},
};
use tokio_util::sync::CancellationToken;

pub async fn run() -> Result<(), Box<dyn Error>> {
    let life = AppLifetime::start();

    let loop_worker = loop_until_cancel(life.natural(), life.graceful());
    life.run_until_abort(loop_worker).await;

    Ok(())
}

async fn loop_until_cancel(stop_loop: CancellationToken, stop_work: CancellationToken) {
    let dur = Duration::from_secs(5);
    let mut interval = interval(dur);

    let mut work = RenewableWorker::new();
    while !stop_loop.is_cancelled() {
        println!("Waiting for next interval...");
        if let None = stop_loop.run_until_cancelled(interval.tick()).await {
            break;
        }

        let token = stop_work.child_token();
        let work_token = token.clone();
        print!("Next interval reached. Work is running... ");
        work.finish_and_renew(
            async move {
                for i in 1..15 {
                    work_token
                        .run_until_cancelled(sleep(Duration::from_secs(1)))
                        .await;
                    print!("{} ", i);
                    _ = std::io::stdout().flush();
                }
                println!("!");
            },
            token,
            Duration::from_secs(5),
        )
        .await;
    }

    // Try to wait for the work to complete, unless the `stop_work` token is cancelled
    _ = stop_work.run_until_cancelled(work.wait()).await;

    // Then try canceling it, and aborting if that does not work
    _ = work.close_with_abort_after(Duration::from_secs(5)).await;

    println!("Work stopped.");
}

struct RenewableWorker {
    handle: Option<(JoinHandle<()>, CancellationToken)>,
}

async fn wait_or_abort<T>(handle: JoinHandle<T>) -> Result<T, JoinError> {
    let h = AbortOnDropJoinHandle::new(handle);
    h.wait().await
}

struct AbortOnDropJoinHandle<T> {
    handle: Option<JoinHandle<T>>,
}

impl<T> AbortOnDropJoinHandle<T> {
    fn new(handle: JoinHandle<T>) -> Self {
        Self {
            handle: Some(handle),
        }
    }

    async fn wait(mut self) -> Result<T, JoinError> {
        if let Some(handle) = self.handle.as_mut() {
            let res = handle.await;
            self.handle = None;
            res
        } else {
            unreachable!()
        }
    }
}

impl<T> Drop for AbortOnDropJoinHandle<T> {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort()
        }
    }
}

impl RenewableWorker {
    fn new() -> Self {
        Self { handle: None }
    }

    /// Stops current work (natural 5s, graceful 5s, then aborts).
    /// Then starts the new future `f`.
    ///
    /// It will cancel the token immediately, then wait `grace_period` to see if the previous task
    /// finishes gracefully. If not, the previous task will be aborted.
    pub async fn finish_and_renew<F>(&mut self, f: F, t: CancellationToken, grace_period: Duration)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if let Some((mut handle, cancel)) = self.handle.take() {
            cancel.cancel();
            select! {
                _ = sleep(grace_period) => {
                    handle.abort()
                }
                _ = &mut handle => {
                }
            }
        }

        let handle = spawn(f);
        self.handle = Some((handle, t));
    }

    pub async fn wait(&mut self) -> Result<(), JoinError> {
        if let Some((handle, _)) = self.handle.as_mut() {
            handle.await?;
        }

        Ok(())
    }

    pub async fn close_with_abort_after(
        mut self,
        abort_after: Duration,
    ) -> Option<Result<(), JoinError>> {
        if let Some((handle, cancel)) = self.handle.take() {
            cancel.cancel();

            let waiter = AbortOnDropJoinHandle::new(handle);
            select! {
                r = waiter.wait() => Some(r),
                _ = sleep(abort_after) => None
            }
        } else {
            Some(Ok(()))
        }
    }
}

impl Drop for RenewableWorker {
    fn drop(&mut self) {
        if let Some((handle, _)) = self.handle.take() {
            handle.abort();
        }
    }
}

pub struct AppLifetime {
    handle: JoinHandle<()>,
    abort: CancellationToken,
    graceful: CancellationToken,
    natural: CancellationToken,
}

impl AppLifetime {
    fn start() -> Self {
        let abort = CancellationToken::new();
        let graceful = abort.child_token();
        let natural = graceful.child_token();

        let ctrlc_abort = abort.clone();
        let ctrlc_graceful = graceful.clone();
        let ctrlc_natural = natural.clone();

        let handle = spawn(async move {
            _ = ctrl_c().await;
            let five_secs = Duration::from_secs(5);

            // Indicate natural stop, and wait 5s
            eprintln!("Stopping. Attempting natural stop.");
            ctrlc_natural.cancel();
            ctrlc_graceful.run_until_cancelled(sleep(five_secs)).await;

            // Indicate graceful stop, and wait 5s
            eprintln!("Stopping. Attempting graceful stop.");
            ctrlc_graceful.cancel();
            ctrlc_abort.run_until_cancelled(sleep(five_secs)).await;

            // Indicate abort and end this task. Anything racing this task will be stopped.
            eprintln!("Stopping. Aborting.");
            ctrlc_abort.cancel();
        });

        Self {
            handle,
            abort,
            graceful,
            natural,
        }
    }

    /// Runs a future until this app lifetime abort token is cancelled.
    async fn run_until_abort<F>(&self, future: F) -> Option<F::Output>
    where
        F: Future,
    {
        select! {
            _ = self.abort.cancelled() => None,
            v = future => Some(v),
        }
    }

    async fn run_until_abort_owned<F>(self, future: F) -> Result<F::Output, ()>
    where
        F: Future,
    {
        select! {
            _ = self.handle => Err(()),
            v = future => Ok(v),
        }
    }

    fn abort(&self) -> CancellationToken {
        self.abort.clone()
    }

    fn graceful(&self) -> CancellationToken {
        self.graceful.clone()
    }

    fn natural(&self) -> CancellationToken {
        self.natural.clone()
    }
}
