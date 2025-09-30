// This will have the application lifetime and functions for scheduling,
// polling, etc.

use tokio::task::JoinHandle;

/// Signal for Result of a race between an operation and Ctrl+C.
pub struct CtrlC;

// static CTRL_C: JoinHandle<()> = tokio::spawn(async {
//     let sig = tokio::signal::ctrl_c()
//         .await
//         .expect("Failed to register Ctrl+C handler.");
// });

pub struct AppLifetime {
    ctrl_c: Option<JoinHandle<()>>,
}

impl AppLifetime {
    pub fn new() -> Self {
        Self { ctrl_c: None }
    }
}

fn init_lifetime_ctrl_c(lifetime: &mut AppLifetime) -> &mut JoinHandle<()> {
    if let None = lifetime.ctrl_c {
        let handle = tokio::spawn(async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to register Ctrl+C handler.");
        });
        lifetime.ctrl_c = Some(handle);
    }
    lifetime.ctrl_c.as_mut().unwrap()
}

/// Runs a future until and unless SIGTERM (Ctrl+C) is received by the process.
pub async fn race_sigterm<F: Future>(f: F, lifetime: &mut AppLifetime) -> Result<F::Output, CtrlC> {
    let mut ctrl_c = init_lifetime_ctrl_c(lifetime);
    tokio::select! {
        _ = &mut ctrl_c => {
            Err(CtrlC)
        },
        result = f => {
            Ok(result)
        }
    }
}

pub async fn try_graceful_shutdown<T>(
    mut f: tokio::task::JoinHandle<T>,
    cancel: &crate::sync::CancellationToken,
) -> Result<T, ()> {
    let five_secs = std::time::Duration::from_secs(5);

    // First try waiting 5s for it to stop on its own
    tokio::select! {
        result = &mut f => {
            return Ok(result.map_err(|_| ())?)
        },
        _ = tokio::time::sleep(five_secs) => {
            eprintln!("Did not respond after five seconds. Trying cooperative cancellation.")
        }
    }

    // Try cancelling via token and allowing the process to finish
    cancel.cancel();
    tokio::select! {
        result = &mut f => {
            return Ok(result.map_err(|_| ())?)
        }
        _ = tokio::time::sleep(five_secs) => {
            eprintln!("Did not respond after five seconds. Aborting.");
        }
    }

    // And then abort
    f.abort();
    Err(())
}
