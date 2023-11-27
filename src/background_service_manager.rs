use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use futures::stream::FuturesUnordered;
use futures::{future, StreamExt};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::error::{BackgroundServiceError, BackgroundServiceErrors};
use crate::service_info::ServiceInfo;
use crate::ServiceContext;

static MONITOR_INITIALIZED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, PartialEq, Eq, Default)]
pub struct Settings {
    blocking_task_monitor_interval: Option<Duration>,
}

impl Settings {
    pub fn blocking_task_monitor_interval(self, interval: Duration) -> Self {
        Self {
            blocking_task_monitor_interval: Some(interval),
        }
    }
}

#[derive(Debug)]
pub struct BackgroundServiceManager {
    cancellation_token: CancellationToken,
    services: Arc<DashMap<u64, ServiceInfo>>,
}

impl BackgroundServiceManager {
    pub fn new(cancellation_token: CancellationToken, settings: Settings) -> Self {
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

        let keys: Vec<_> = self.services.iter().map(|s| *s.key()).collect();
        for key in keys {
            if let Some((_, service)) = self.services.remove(&key) {
                unordered.push(async move {
                    let abort_handle = service.handle.abort_handle();
                    match tokio::time::timeout(service.timeout, service.handle).await {
                        Ok(Ok(Ok(_))) => {
                            info!("Worker {} shutdown successfully", service.name);
                            Ok(())
                        }
                        Ok(Ok(Err(e))) => Err(BackgroundServiceError::ExecutionFailure(
                            service.name.to_owned(),
                            e,
                        )),
                        Ok(Err(e)) => Err(BackgroundServiceError::ExecutionPanic(
                            service.name.to_owned(),
                            e,
                        )),
                        Err(_) => {
                            abort_handle.abort();
                            Err(BackgroundServiceError::TimedOut(service.name.to_owned()))
                        }
                    }
                });
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
