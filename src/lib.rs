pub mod error;
#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod wasm;

use std::time::Duration;

use error::BoxedError;
use futures::Future;
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
#[cfg(target_arch = "wasm32")]
pub use wasm::*;
use wasm_compat::cell::U64Cell;

wasm_compat::static_init! {
    static NEXT_ID: U64Cell = U64Cell::new(1);
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TaskId(u64);

fn next_id() -> TaskId {
    TaskId(NEXT_ID.with(|id| id.add(1)))
}

pub trait LocalBackgroundService {
    fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(1)
    }
    fn name(&self) -> &str;
    fn run(self, context: ServiceContext) -> impl Future<Output = Result<(), BoxedError>>;
}

#[cfg(not(target_arch = "wasm32"))]
pub trait BackgroundService {
    fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(1)
    }
    fn name(&self) -> &str;
    fn run(self, context: ServiceContext) -> impl Future<Output = Result<(), BoxedError>> + Send;
}

#[cfg(target_arch = "wasm32")]
pub trait BackgroundService {
    fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(1)
    }
    fn name(&self) -> &str;
    fn run(self, context: ServiceContext) -> impl Future<Output = Result<(), BoxedError>>;
}

#[cfg(not(target_arch = "wasm32"))]
pub trait BlockingBackgroundService {
    fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(1)
    }
    fn name(&self) -> &str;
    fn run(self, context: ServiceContext) -> Result<(), BoxedError>;
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
impl<S, F, Fut> BackgroundService for (S, F)
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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
