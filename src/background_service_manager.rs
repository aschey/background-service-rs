use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use futures::stream::FuturesUnordered;
use futures::{future, StreamExt};
use futures_cancel::FutureExt;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::error::{BackgroundServiceError, BackgroundServiceErrors};
use crate::service_info::ServiceInfo;
use crate::ServiceContext;

static MONITOR_INITIALIZED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, PartialEq, Eq)]
pub struct Settings {
    blocking_task_monitor_interval: Option<Duration>,
    completed_task_monitor_interval: Duration,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            blocking_task_monitor_interval: None,
            completed_task_monitor_interval: Duration::from_secs(1),
        }
    }
}

impl Settings {
    pub fn blocking_task_monitor_interval(self, interval: Duration) -> Self {
        Self {
            blocking_task_monitor_interval: Some(interval),
            ..self
        }
    }

    pub fn completed_task_monitor_interval(self, interval: Duration) -> Self {
        Self {
            completed_task_monitor_interval: interval,
            ..self
        }
    }
}

#[derive(Debug)]
pub struct BackgroundServiceManager {
    cancellation_token: CancellationToken,
    services: Arc<RwLock<Vec<ServiceInfo>>>,
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

        let services = Arc::new(RwLock::new(vec![]));
        let services_ = services.clone();
        let interval = settings.completed_task_monitor_interval;
        let mut context = ServiceContext::new(services.clone(), cancellation_token.clone());
        context.add_service((
            "completed_task_reaper",
            move |context: ServiceContext| async move {
                loop {
                    if tokio::time::sleep(interval)
                        .cancel_on_shutdown(&context.cancellation_token())
                        .await
                        .is_err()
                    {
                        break;
                    }

                    let mut remove_indexes = vec![];
                    for (i, service) in services_.read().unwrap().iter().enumerate() {
                        if service.handle.is_finished() {
                            info!("Removing service {:?}", service.name);
                            remove_indexes.push(i);
                        }
                    }
                    if !remove_indexes.is_empty() {
                        remove_indexes.reverse();
                        let mut services_mut = services_.write().unwrap();
                        for index in remove_indexes {
                            services_mut.remove(index);
                        }
                    }
                }

                Ok(())
            },
        ));

        Self {
            services,
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

        {
            // using a block here to ensure mutex is not held across await

            let mut services = self.services.write().unwrap();
            while let Some(service) = services.pop() {
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
