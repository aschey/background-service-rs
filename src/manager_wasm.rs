use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::stream::FuturesUnordered;
use futures::{future, StreamExt};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::{debug, info};

use crate::error::{BackgroundServiceError, BackgroundServiceErrors};
use crate::service_context_wasm::ServiceContext;
use crate::service_info_wasm::ServiceInfo;
use crate::TaskId;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Settings {
    blocking_task_monitor_interval: Option<Duration>,
    task_wait_duration: Duration,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            blocking_task_monitor_interval: None,
            task_wait_duration: Duration::from_secs(1),
        }
    }
}

impl Settings {
    pub fn blocking_task_monitor_interval(mut self, interval: Duration) -> Self {
        self.blocking_task_monitor_interval = Some(interval);
        self
    }

    pub fn task_wait_duration(mut self, task_wait_duration: Duration) -> Self {
        self.task_wait_duration = task_wait_duration;
        self
    }
}

#[derive(Debug, Clone)]
pub struct Manager {
    cancellation_token: CancellationToken,
    services: Rc<RefCell<HashMap<TaskId, ServiceInfo>>>,
}

impl Manager {
    pub fn new(cancellation_token: CancellationToken, _settings: Settings) -> Self {
        Self {
            services: Default::default(),
            cancellation_token,
        }
    }

    pub fn stop(&self) {
        self.cancellation_token.cancel();
    }

    #[cfg(feature = "signal")]
    pub async fn cancel_on_signal(self) -> Result<(), BackgroundServiceErrors> {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!("Error waiting for shutdown signal: {e:?}");
        }
        self.cancel().await
    }

    pub async fn join_on_cancel(self) -> Result<(), BackgroundServiceErrors> {
        self.cancellation_token.cancelled().await;
        self.cancel().await
    }

    pub async fn cancel(self) -> Result<(), BackgroundServiceErrors> {
        self.cancellation_token.cancel();
        let unordered = FuturesUnordered::new();

        let keys: Vec<_> = {
            let services = self.services.borrow();
            services.keys().map(|k| *k).collect()
        };
        let mut services = self.services.borrow_mut();
        for key in keys {
            if let Some(service) = services.remove(&key) {
                unordered.push(service.wait_for_shutdown());
            }
        }

        let errors = unordered
            .filter_map(|r| future::ready(r.err()))
            .collect::<Vec<_>>()
            .await;

        if errors.is_empty() {
            Ok(())
        } else {
            Err(BackgroundServiceErrors(errors))
        }
    }

    pub fn get_context(&self) -> ServiceContext {
        ServiceContext::new(self.services.clone(), self.cancellation_token.clone())
    }
}
