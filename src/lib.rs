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
impl<S, F, Fut> BackgroundService for (S, F)
where
    S: AsRef<str> + Send,
    F: FnOnce(ServiceContext) -> Fut + Send,
    Fut: Future<Output = Result<(), BoxedError>> + Send,
{
    fn name(&self) -> &str {
        self.0.as_ref()
    }

    async fn run(mut self, context: ServiceContext) -> Result<(), BoxedError> {
        self.1(context).await
    }
}
