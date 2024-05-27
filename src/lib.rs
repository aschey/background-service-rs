mod background_service_manager;
pub mod error;
mod service_context;
mod service_info;

use std::time::Duration;

pub use background_service_manager::*;
use error::BoxedError;
use futures::Future;
pub use service_context::*;

pub trait LocalBackgroundService {
    fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(1)
    }
    fn name(&self) -> &str;
    fn run(self, context: ServiceContext) -> impl Future<Output = Result<(), BoxedError>>;
}

pub trait BackgroundService {
    fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(1)
    }
    fn name(&self) -> &str;
    fn run(self, context: ServiceContext) -> impl Future<Output = Result<(), BoxedError>> + Send;
}

pub trait BlockingBackgroundService {
    fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(1)
    }
    fn name(&self) -> &str;
    fn run(self, context: ServiceContext) -> Result<(), BoxedError>;
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

impl<S, F, Fut> LocalBackgroundService for (S, F)
where
    S: AsRef<str>,
    F: FnOnce(ServiceContext) -> Fut,
    Fut: Future<Output = Result<(), BoxedError>>,
{
    fn name(&self) -> &str {
        self.0.as_ref()
    }

    async fn run(self, context: ServiceContext) -> Result<(), BoxedError> {
        self.1(context).await
    }
}

impl<S, F> BlockingBackgroundService for (S, F)
where
    S: AsRef<str> + Send,
    F: FnOnce(ServiceContext) -> Result<(), BoxedError>,
{
    fn name(&self) -> &str {
        self.0.as_ref()
    }

    fn run(self, context: ServiceContext) -> Result<(), BoxedError> {
        self.1(context)
    }
}

impl<F, Fut> BackgroundService for F
where
    F: FnOnce(ServiceContext) -> Fut + Send,
    Fut: Future<Output = Result<(), BoxedError>> + Send,
{
    fn name(&self) -> &str {
        "<task>"
    }

    async fn run(self, context: ServiceContext) -> Result<(), BoxedError> {
        self(context).await
    }
}

impl<F, Fut> LocalBackgroundService for F
where
    F: FnOnce(ServiceContext) -> Fut,
    Fut: Future<Output = Result<(), BoxedError>>,
{
    fn name(&self) -> &str {
        "<task>"
    }

    async fn run(self, context: ServiceContext) -> Result<(), BoxedError> {
        self(context).await
    }
}

impl<F> BlockingBackgroundService for F
where
    F: FnOnce(ServiceContext) -> Result<(), BoxedError>,
{
    fn name(&self) -> &str {
        "<task>"
    }

    fn run(self, context: ServiceContext) -> Result<(), BoxedError> {
        self(context)
    }
}
