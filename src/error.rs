use std::error::Error;
use tokio::task::JoinError;

#[derive(thiserror::Error, Debug)]
#[error("Some background services failed to execute: {0:?}")]
pub struct BackgroundServiceErrors(pub Vec<BackgroundServiceError>);

#[derive(thiserror::Error, Debug)]
pub enum BackgroundServiceError {
    #[error("Service {0} failed to shut down within the timeout")]
    TimedOut(String),
    #[error("Service {0} encountered an error: {1:?}")]
    ExecutionFailure(String, BoxedError),
    #[error("Service {0} panicked: {1}")]
    ExecutionPanic(String, JoinError),
}

pub type BoxedError = Box<dyn Error + Send + Sync + 'static>;
