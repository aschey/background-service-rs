use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::service_info::ServiceInfo;
use crate::BackgroundService;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub struct ServiceContext {
    services: Arc<DashMap<u64, ServiceInfo>>,
    notify_tx: mpsc::Sender<u64>,
    cancellation_token: CancellationToken,
}

impl ServiceContext {
    pub(crate) fn new(
        services: Arc<DashMap<u64, ServiceInfo>>,
        notify_tx: mpsc::Sender<u64>,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            cancellation_token,
            notify_tx,
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
        let notify_finished = self.notify_tx.clone();
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        let handle = tokio::spawn(async move {
            let cancellation_token = context.cancellation_token();
            let res = service.run(context).await;
            // If cancellation was requested, the completed task monitor should already be shut down
            // so we don't need to send a notification
            if !cancellation_token.is_cancelled() {
                notify_finished.try_send(id).ok();
            }

            res
        });
        self.services.insert(
            id,
            ServiceInfo {
                handle,
                name,
                timeout: S::shutdown_timeout(),
            },
        );
    }
}
