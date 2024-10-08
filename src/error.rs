use std::error::Error;
use std::sync::Arc;

use tokio::task::JoinError;

#[derive(thiserror::Error, Debug, Clone)]
#[error("Some background services failed to execute: {0:?}")]
pub struct BackgroundServiceErrors(pub Arc<Vec<BackgroundServiceError>>);

impl BackgroundServiceErrors {
    pub fn timed_out(&self) -> Vec<String> {
        self.0
            .iter()
            .filter_map(|err| {
                if let BackgroundServiceError::TimedOut(name) = err {
                    Some(name.to_owned())
                } else {
                    None
                }
            })
            .collect()
    }
}

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
