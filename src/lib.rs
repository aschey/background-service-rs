mod background_service_manager;
pub mod error;
mod service_context;
mod service_info;

use std::time::Duration;

pub use background_service_manager::*;
use error::BoxedError;
use futures::Future;
pub use service_context::*;

pub trait BackgroundService: Send {
    fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(1)
    }
    fn name(&self) -> &str;
    fn run(self, context: ServiceContext) -> impl Future<Output = Result<(), BoxedError>> + Send;
}

impl<S, F, Fut> BackgroundService for (S, F)
where
    S: AsRef<str> + Send,
    F: FnOnce(ServiceContext) -> Fut + Send,
    Fut: Future<Output = Result<(), BoxedError>> + Send,
{
    fn name(&self) -> &str {
        self.0.as_ref()
    }

    async fn run(self, context: ServiceContext) -> Result<(), BoxedError> {
        self.1(context).await
    }
}
