use std::time::Duration;

use tokio::task::JoinHandle;
use tracing::info;

use crate::error::{BackgroundServiceError, BoxedError};
use crate::TaskId;

#[derive(Debug)]
pub struct ServiceInfo {
    pub(crate) name: String,
    pub(crate) id: TaskId,
    pub(crate) timeout: Duration,
    pub(crate) handle: JoinHandle<Result<(), BoxedError>>,
}

impl ServiceInfo {
    pub async fn shutdown(self) -> Result<(), BackgroundServiceError> {
        let abort_handle = self.handle.abort_handle();
        match tokio::time::timeout(self.timeout, self.handle).await {
            Ok(Ok(Ok(_))) => {
                info!("Worker {} shutdown successfully", self.name);
                Ok(())
            }
            Ok(Ok(Err(e))) => Err(BackgroundServiceError::ExecutionFailure(
                self.name.to_owned(),
                e,
            )),
            Ok(Err(e)) => Err(BackgroundServiceError::ExecutionPanic(
                self.name.to_owned(),
                e,
            )),
            Err(_) => {
                abort_handle.abort();
                Err(BackgroundServiceError::TimedOut(self.name.to_owned()))
            }
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn id(&self) -> TaskId {
        self.id
    }

    pub fn abort(&self) {
        self.handle.abort();
    }
}
