use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::error::BackgroundServiceError;
use crate::TaskId;

#[derive(Debug)]
pub struct ServiceInfo {
    pub(crate) name: String,
    pub(crate) id: TaskId,
    pub(crate) timeout: Duration,
    pub(crate) cancellation_token: CancellationToken,
}

impl ServiceInfo {
    pub async fn wait_for_shutdown(self) -> Result<(), BackgroundServiceError> {
        Ok(())
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

    pub fn cancel(&self) {
        self.cancellation_token.cancel();
    }
}
