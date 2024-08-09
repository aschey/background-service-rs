use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use futures::stream::FuturesUnordered;
use futures::{future, StreamExt};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::{debug, info};

use crate::error::{BackgroundServiceError, BackgroundServiceErrors};
use crate::service_info::ServiceInfo;
use crate::{ServiceContext, TaskId};

static MONITOR_INITIALIZED: AtomicBool = AtomicBool::new(false);

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

#[derive(Debug)]
pub struct Manager {
    cancellation_token: CancellationToken,
    services: Arc<DashMap<TaskId, ServiceInfo>>,
    tracker: TaskTracker,
    settings: Settings,
}

impl Manager {
    pub fn new(cancellation_token: CancellationToken, settings: Settings) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(monitor_interval) = settings.blocking_task_monitor_interval {
            if !MONITOR_INITIALIZED.swap(true, Ordering::SeqCst) {
                debug!("initializing monitor");
                let cancellation_token = cancellation_token.clone();
                let rt_handle = tokio::runtime::Handle::current();

                // Workaround for blocking tasks described here
                // https://github.com/tokio-rs/tokio/issues/4730#issuecomment-1147165954
                std::thread::spawn(move || {
                    loop {
                        std::thread::sleep(monitor_interval);
                        rt_handle.spawn(std::future::ready(()));
                        if cancellation_token.is_cancelled() {
                            debug!("shutting down monitor");
                            return;
                        }
                    }
                });
            }
        }

        Self {
            services: Default::default(),
            tracker: TaskTracker::new(),
            cancellation_token,
            settings,
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
        self.tracker.close();
        let unordered = FuturesUnordered::new();

        let keys: Vec<_> = self.services.iter().map(|s| *s.key()).collect();
        for key in keys {
            if let Some((_, service)) = self.services.remove(&key) {
                unordered.push(service.wait_for_shutdown());
            }
        }

        let start_time = Instant::now();
        let mut errors = unordered
            .filter_map(|r| future::ready(r.err()))
            .collect::<Vec<_>>()
            .await;

        let remaining_wait_time = self
            .settings
            .task_wait_duration
            .saturating_sub(Instant::now() - start_time);

        info!("Waiting for any remaining tasks");
        // This should return immediately unless any services are being awaited independently
        // via ServiceContext::take_service
        if tokio::time::timeout(remaining_wait_time, self.tracker.wait())
            .await
            .is_err()
        {
            errors.push(BackgroundServiceError::TimedOut("TaskTracker".to_owned()))
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(BackgroundServiceErrors(errors))
        }
    }

    pub fn get_context(&self) -> ServiceContext {
        ServiceContext::new(
            self.services.clone(),
            self.tracker.clone(),
            self.cancellation_token.clone(),
        )
    }
}
