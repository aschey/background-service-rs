use crate::error::BoxedError;
use std::time::Duration;
use tokio::task::JoinHandle;

#[derive(Debug)]
pub(crate) struct ServiceInfo {
    pub(crate) name: String,
    pub(crate) timeout: Duration,
    pub(crate) handle: JoinHandle<Result<(), BoxedError>>,
}
