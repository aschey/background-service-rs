use async_trait::async_trait;
use background_service::{
    error::BoxedError, BackgroundService, BackgroundServiceManager, ServiceContext,
};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::info;

#[tokio::main]
pub async fn main() {
    tracing_subscriber::fmt::init();
    let token = CancellationToken::default();
    let manager = BackgroundServiceManager::new(token.clone());
    let mut context = manager.get_context();
    context
        .add_service(("simple".to_owned(), |context: ServiceContext| async move {
            let mut seconds = 0;
            let cancellation_token = context.cancellation_token();
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {
                        info!("Service has been running for {seconds} seconds");
                        seconds += 1;
                    }
                    _ = cancellation_token.cancelled() => {
                        info!("Received cancellation request");
                        return Ok(());
                    }
                }
            }
        }))
        .await
        .unwrap();

    context.add_service(Service).await.unwrap();
    let token = context.cancellation_token();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        token.cancel();
    });
    manager.join_on_cancel().await.unwrap();
}
struct Service;

#[async_trait]
impl BackgroundService for Service {
    fn shutdown_timeout() -> Duration {
        Duration::from_secs(3)
    }

    fn name(&self) -> &str {
        "service"
    }

    async fn run(mut self, mut context: ServiceContext) -> Result<(), BoxedError> {
        let cancellation_token = context.cancellation_token();
        loop {
            tokio::select! {
                _ = tokio:: time:: sleep(Duration::from_secs(3)) => {
                    info!("Spawning another service");

                    context.add_service(("child".to_owned(), |context: ServiceContext| async move {
                        info!("Service waiting for cancellation");
                        context.cancellation_token().cancelled().await;
                        info!("Received cancellation request");
                        Ok(())
                    })).await.unwrap();
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
