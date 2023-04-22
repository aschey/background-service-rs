use crate::{error::AddServiceError, service_info::ServiceInfo, BackgroundService};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct ServiceContext {
    services: Arc<RwLock<Option<Vec<ServiceInfo>>>>,
    cancellation_token: CancellationToken,
}

impl ServiceContext {
    pub(crate) fn new(
        services: Arc<RwLock<Option<Vec<ServiceInfo>>>>,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            cancellation_token,
            services,
        }
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.child_token()
    }

    pub async fn add_service<S: BackgroundService + 'static>(
        &mut self,
        service: S,
    ) -> Result<(), AddServiceError> {
        if let Some(services) = &mut *self.services.write().await {
            let context = self.clone();
            let name = service.name().to_owned();
            let handle = tokio::spawn(async move { service.run(context).await });
            services.push(ServiceInfo {
                handle,
                name,
                timeout: S::shutdown_timeout(),
            });
            Ok(())
        } else {
            Err(AddServiceError::ManagerStopped)
        }
    }
}
