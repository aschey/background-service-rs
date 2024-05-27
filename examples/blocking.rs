use std::time::Duration;

use background_service::error::BoxedError;
use background_service::{BackgroundService, BackgroundServiceManager, ServiceContext, Settings};
use tokio_util::sync::CancellationToken;
use tracing::info;

#[tokio::main]
pub async fn main() {
    tracing_subscriber::fmt::init();
    let token = CancellationToken::default();
    let manager = BackgroundServiceManager::new(token.clone(), Settings::default());
    let context = manager.get_context();
    context.spawn_blocking(("blocking", move |context: ServiceContext| {
        let mut seconds = 0;
        let cancellation_token = context.cancellation_token();
        loop {
            if cancellation_token.is_cancelled() {
                return Ok(());
            }
            info!("Service has been running for {seconds} seconds");
            seconds += 1;
            std::thread::sleep(Duration::from_secs(1));
        }
    }));

    context.spawn(Service);
    let token = context.cancellation_token();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        token.cancel();
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
        let cancellation_token = context.cancellation_token();
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(3)) => {
                    info!("Spawning another service");

                    context.spawn(("child", |context: ServiceContext| async move {
                        info!("Service waiting for cancellation");
                        context.cancellation_token().cancelled().await;
                        info!("Received cancellation request");
                        Ok(())
                    }));

                    context.spawn(("child2", |_: ServiceContext| async move {
                        info!("exiting");
                        Ok(())
                    }));
                }
                _ = cancellation_token.cancelled() => {
                    info!("Received cancellation request. Waiting 1 second to shut down.");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    return Ok(());
                }
            }
        }
    }
}