use std::sync::Arc;

use crossbeam::queue::SegQueue;
use tokio_util::sync::CancellationToken;

use crate::service_info::ServiceInfo;
use crate::BackgroundService;

#[derive(Clone)]
pub struct ServiceContext {
    services: Arc<SegQueue<ServiceInfo>>,
    cancellation_token: CancellationToken,
}

impl ServiceContext {
    pub(crate) fn new(
        services: Arc<SegQueue<ServiceInfo>>,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            cancellation_token,
            services,
        }
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.clone()
    }

    pub fn child_token(&self) -> CancellationToken {
        self.cancellation_token.child_token()
    }

    pub fn add_service<S: BackgroundService + 'static>(&mut self, service: S) {
        let context = self.clone();
        let name = service.name().to_owned();
        let handle = tokio::spawn(async move { service.run(context).await });
        self.services.push(ServiceInfo {
            handle,
            name,
            timeout: S::shutdown_timeout(),
        });
    }
}
