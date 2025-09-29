use std::{env, error::Error};

use amqprs::{
    BasicProperties,
    channel::BasicPublishArguments,
    connection::{Connection, OpenConnectionArguments},
};
use tokio::{join, select, task::JoinHandle};

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

    let publish_args = BasicPublishArguments::new("", "rabbit-eye-dev");
    channel
        .basic_publish(BasicProperties::default(), vec![], publish_args)
        .await?;

    let period = tokio::time::Duration::from_millis(5_000);
    let mut interval = tokio::time::interval(period);

    let mut doing_work: Option<JoinHandle<()>> = None;

    let ctrlc = tokio::spawn(async {
        tokio::signal::ctrl_c()
            .await
            .map_err(|e| format!("{:?}", e))?;

        println!("Ctrl+C pressed. Signaling shutdown.");

        Ok::<(), String>(())
    });

    loop {
        interval.tick().await;

        if ctrlc.is_finished() {
            print!("Ctrl+C pressed.");
            if let Some(handle) = doing_work.take() {
                println!(" Waiting for work to finish.");
                handle.await?;
            } else {
                println!(" No work is ongoing.");
            }
            break;
        }

        match doing_work {
            Some(handle) => {
                println!("Work was ongoing. It is being stopped.");
                handle.abort();
            }
            _ => {}
        }

        doing_work = Some(tokio::spawn(async {
            let mut i = 0;
            let _pod = PrintOnDrop;
            loop {
                println!("Doing some work (iter {}).", i);
                tokio::time::sleep(tokio::time::Duration::from_millis(1_000)).await;
                i = i + 1;
                if i > 3 {
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
