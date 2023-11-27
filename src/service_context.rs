use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::service_info::ServiceInfo;
use crate::BackgroundService;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct ServiceContext {
    services: Arc<DashMap<u64, ServiceInfo>>,
    cancellation_token: CancellationToken,
}

impl ServiceContext {
    pub(crate) fn new(
        services: Arc<DashMap<u64, ServiceInfo>>,
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
        let timeout = service.shutdown_timeout();

        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        let services = self.services.clone();
        let handle = tokio::spawn(async move {
            let cancellation_token = context.cancellation_token();
            let res = service.run(context).await;
            // If cancellation was requested, the manager is being shut down anyway
            // so we don't need to remove the service
            if !cancellation_token.is_cancelled() {
                if let Some((_, service)) = services.remove(&id) {
                    info!("Removing {}", service.name);
                    if let Err(e) = &res {
                        error!("Service {} exited with error: {e:?}", service.name);
                    }
                }
            }

            res
        });
        self.services.insert(
            id,
            ServiceInfo {
                handle,
                name,
                timeout,
            },
        );
    }
}
