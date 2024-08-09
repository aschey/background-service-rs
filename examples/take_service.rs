use std::time::Duration;

use background_service::error::BoxedError;
use background_service::{BackgroundService, Manager, ServiceContext, Settings};
use tokio_util::sync::CancellationToken;
use tracing::info;

#[tokio::main]
pub async fn main() {
    tracing_subscriber::fmt::init();
    let token = CancellationToken::default();
    // Need to increase the task wait duration to prevent service2 from causing a timeout after
    // removal
    let manager = Manager::new(
        token.clone(),
        Settings::default().task_wait_duration(Duration::from_secs(5)),
    );
    let context = manager.get_context();

    context.spawn(Service);
    let service2 = context.spawn(Service2);

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        token.cancel();
        // Service will no longer be tracked in the manager after taking it here
        context.wait_for_shutdown(&service2).await.unwrap().unwrap()
    });
    manager.join_on_cancel().await.unwrap();
}
struct Service;

impl BackgroundService for Service {
    fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(3)
    }

    fn name(&self) -> &str {
        "service"
    }

    async fn run(self, context: ServiceContext) -> Result<(), BoxedError> {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(3)) => {
                    info!("Spawning another service");

                    context.spawn(("child", |context: ServiceContext| async move {
                        info!("Service waiting for cancellation");
                        context.cancelled().await;
                        info!("Received cancellation request");
                        Ok(())
                    }));

                    context.spawn(("child2", |_: ServiceContext| async move {
                        info!("exiting");
                        Ok(())
                    }));
                }
                _ =  context.cancelled() => {
                    info!("Received cancellation request. Waiting 1 second to shut down.");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    return Ok(());
                }
            }
        }
    }
}

struct Service2;

impl BackgroundService for Service2 {
    fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(5)
    }

    fn name(&self) -> &str {
        "service2"
    }

    async fn run(self, context: ServiceContext) -> Result<(), BoxedError> {
        let mut seconds = 0;

        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    info!("Service has been running for {seconds} seconds");
                    seconds += 1;
                }
                _ =  context.cancelled() => {
                    info!("Received cancellation request. Waiting 4 seconds to shut down.");
                    tokio::time::sleep(Duration::from_secs(4)).await;
                    return Ok(());
                }
            }
        }
    }
}
