use crate::{
    error::{BackgroundServiceError, BackgroundServiceErrors},
    service_info::ServiceInfo,
    ServiceContext,
};
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

    pub async fn stop(self) -> Result<(), BackgroundServiceErrors> {
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
        ServiceContext::new(self.services.clone(), self.cancellation_token.child_token())
    }
}
