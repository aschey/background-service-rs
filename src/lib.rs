use async_trait::async_trait;
use error::BoxedError;
use futures::Future;
use std::time::Duration;

mod background_service_manager;

pub use background_service_manager::*;

pub mod error;
mod service_context;

pub use service_context::*;

mod service_info;

#[async_trait]
pub trait BackgroundService: Send {
    fn shutdown_timeout() -> Duration {
        Duration::from_secs(1)
    }
    fn name(&self) -> &str;
    async fn run(mut self, context: ServiceContext) -> Result<(), BoxedError>;
}

#[async_trait]
impl<F, Fut> BackgroundService for (String, F)
where
    F: FnOnce(ServiceContext) -> Fut + Send,
    Fut: Future<Output = Result<(), BoxedError>> + Send,
{
    fn name(&self) -> &str {
        &self.0
    }

    async fn run(mut self, context: ServiceContext) -> Result<(), BoxedError> {
        self.1(context).await
    }
}
