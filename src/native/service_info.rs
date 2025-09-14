use std::error::Error;
use std::thread;
use std::time::Duration;

use tokio::sync::oneshot;
use tokio::task::{self, AbortHandle};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::TaskId;
use crate::error::{BackgroundServiceError, BoxedError};

#[derive(Debug)]
pub(crate) enum JoinHandle<T> {
    Async(task::JoinHandle<T>),
    Sync(thread::JoinHandle<T>, oneshot::Receiver<()>),
}

enum WaitResult<T> {
    TimedOut,
    Completed(T),
    JoinError(Box<dyn Error + Send + Sync>),
}

impl<T> JoinHandle<T> {
    fn abort_handle(&self) -> Option<AbortHandle> {
        if let Self::Async(h) = self {
            Some(h.abort_handle())
        } else {
            None
        }
    }

    async fn wait(self, timeout: Duration) -> WaitResult<T> {
        match self {
            Self::Async(h) => match tokio::time::timeout(timeout, h).await {
                Ok(Ok(res)) => WaitResult::Completed(res),
                Ok(Err(e)) => WaitResult::JoinError(e.into()),
                Err(_) => WaitResult::TimedOut,
            },
            Self::Sync(h, rx) => match tokio::time::timeout(timeout, rx).await {
                Ok(Ok(())) => match h.join() {
                    Ok(res) => WaitResult::Completed(res),
                    Err(e) => WaitResult::JoinError(format!("{e:?}").into()),
                },
                Ok(Err(e)) => WaitResult::JoinError(e.into()),
                Err(_) => WaitResult::TimedOut,
            },
        }
    }
}

#[derive(Debug)]
pub struct ServiceInfo {
    pub(crate) name: String,
    pub(crate) id: TaskId,
    pub(crate) timeout: Duration,
    pub(crate) handle: JoinHandle<Result<(), BoxedError>>,
    pub(crate) cancellation_token: CancellationToken,
}

impl ServiceInfo {
    pub async fn wait_for_shutdown(self) -> Result<(), BackgroundServiceError> {
        let abort_handle = self.handle.abort_handle();
        match self.handle.wait(self.timeout).await {
            WaitResult::Completed(Ok(())) => {
                info!("Worker {} shutdown successfully", self.name);
                Ok(())
            }
            WaitResult::Completed(Err(e)) => Err(BackgroundServiceError::ExecutionFailure(
                self.name.to_owned(),
                e,
            )),
            WaitResult::JoinError(e) => Err(BackgroundServiceError::ExecutionPanic(
                self.name.to_owned(),
                e,
            )),
            WaitResult::TimedOut => {
                if let Some(h) = abort_handle {
                    h.abort();
                }
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
        if let JoinHandle::Async(h) = &self.handle {
            h.abort();
        }
    }

    pub fn cancel(&self) {
        self.cancellation_token.cancel();
    }
}
