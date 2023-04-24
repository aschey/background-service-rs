use crate::{
    error::{BackgroundServiceError, BackgroundServiceErrors},
    service_info::ServiceInfo,
    ServiceContext,
};
use futures::future::join_all;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::info;

#[derive(Debug)]
pub struct BackgroundServiceManager {
    cancellation_token: CancellationToken,
    services: Arc<RwLock<Option<Vec<ServiceInfo>>>>,
}

impl BackgroundServiceManager {
    pub fn new(cancellation_token: CancellationToken) -> Self {
        Self {
            services: Arc::new(RwLock::new(Some(vec![]))),
            cancellation_token,
        }
    }

    pub fn stop(&self) {
        self.cancellation_token.cancel();
    }

    pub async fn join(self) -> Result<(), BackgroundServiceErrors> {
        let mut errors = vec![];
        if let Some(services) = self.services.write().await.take() {
            let names: Vec<_> = services.iter().map(|s| s.name.clone()).collect();
            let results = join_all(services.into_iter().map(|s| s.handle)).await;

            for (i, result) in results.into_iter().enumerate() {
                match result {
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => errors.push(BackgroundServiceError::ExecutionFailure(
                        names[i].to_owned(),
                        e,
                    )),
                    Err(e) => errors.push(BackgroundServiceError::ExecutionPanic(
                        names[i].to_owned(),
                        e,
                    )),
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(BackgroundServiceErrors(errors))
        }
    }

    pub async fn join_on_cancel(self) -> Result<(), BackgroundServiceErrors> {
        self.cancellation_token.cancelled().await;
        self.cancel().await
    }

    pub async fn cancel(self) -> Result<(), BackgroundServiceErrors> {
        self.cancellation_token.cancel();
        let mut errors = vec![];
        if let Some(services) = self.services.write().await.take() {
            for service in services {
                match tokio::time::timeout(service.timeout, service.handle).await {
                    Ok(Ok(Ok(_))) => info!("Worker {} shutdown successfully", service.name),
                    Ok(Ok(Err(e))) => errors.push(BackgroundServiceError::ExecutionFailure(
                        service.name.to_owned(),
                        e,
                    )),
                    Ok(Err(e)) => errors.push(BackgroundServiceError::ExecutionPanic(
                        service.name.to_owned(),
                        e,
                    )),
                    Err(_) => {
                        errors.push(BackgroundServiceError::TimedOut(service.name.to_owned()))
                    }
                }
            }
        }
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
