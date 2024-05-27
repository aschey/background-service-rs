use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use futures::Future;
use tokio::runtime::Handle;
use tokio::task::LocalSet;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::{error, info};

use crate::error::BoxedError;
use crate::service_info::ServiceInfo;
use crate::{BackgroundService, BlockingBackgroundService, LocalBackgroundService};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TaskId(u64);

fn next_id() -> TaskId {
    TaskId(NEXT_ID.fetch_add(1, Ordering::SeqCst))
}

#[derive(Clone)]
pub struct ServiceContext {
    services: Arc<DashMap<TaskId, ServiceInfo>>,
    tracker: TaskTracker,
    cancellation_token: CancellationToken,
}

impl ServiceContext {
    pub(crate) fn new(
        services: Arc<DashMap<TaskId, ServiceInfo>>,
        tracker: TaskTracker,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            cancellation_token,
            tracker,
            services,
        }
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.clone()
    }

    pub fn child_token(&self) -> CancellationToken {
        self.cancellation_token.child_token()
    }

    pub async fn take_service(&self, id: &TaskId) -> Option<ServiceInfo> {
        if let Some((_, service)) = self.services.remove(id) {
            Some(service)
        } else {
            None
        }
    }

    pub fn spawn<S: BackgroundService + Send + 'static>(&self, service: S) -> TaskId {
        let name = service.name().to_owned();
        let timeout = service.shutdown_timeout();

        let id = next_id();

        let handle = self.tracker.spawn(self.get_service_future(id, service));
        self.services.insert(
            id,
            ServiceInfo {
                id,
                handle,
                name,
                timeout,
            },
        );
        id
    }

    pub fn spawn_on<S: BackgroundService + Send + 'static>(
        &self,
        service: S,
        handle: &Handle,
    ) -> TaskId {
        let name = service.name().to_owned();
        let timeout = service.shutdown_timeout();

        let id = next_id();

        let handle = self
            .tracker
            .spawn_on(self.get_service_future(id, service), handle);
        self.services.insert(
            id,
            ServiceInfo {
                id,
                handle,
                name,
                timeout,
            },
        );
        id
    }

    pub fn spawn_local<S: LocalBackgroundService + 'static>(&self, service: S) -> TaskId {
        let name = service.name().to_owned();
        let timeout = service.shutdown_timeout();

        let id = next_id();

        let handle = self
            .tracker
            .spawn_local(self.get_service_future_local(id, service));
        self.services.insert(
            id,
            ServiceInfo {
                id,
                handle,
                name,
                timeout,
            },
        );
        id
    }

    pub fn spawn_local_on<S: BackgroundService + 'static>(
        &self,
        service: S,
        local_set: &LocalSet,
    ) -> TaskId {
        let name = service.name().to_owned();
        let timeout = service.shutdown_timeout();

        let id = next_id();

        let handle = self
            .tracker
            .spawn_local_on(self.get_service_future(id, service), local_set);
        self.services.insert(
            id,
            ServiceInfo {
                id,
                handle,
                name,
                timeout,
            },
        );
        id
    }

    pub fn spawn_blocking<S: BlockingBackgroundService + Send + 'static>(&self, service: S) {
        let name = service.name().to_owned();
        let timeout = service.shutdown_timeout();

        let id = next_id();

        let handle = self
            .tracker
            .spawn_blocking(self.get_service_blocking(id, service));
        self.services.insert(
            id,
            ServiceInfo {
                id,
                handle,
                name,
                timeout,
            },
        );
    }

    pub fn spawn_blocking_on<S: BlockingBackgroundService + Send + 'static>(
        &self,
        service: S,
        handle: &Handle,
    ) {
        let name = service.name().to_owned();
        let timeout = service.shutdown_timeout();

        let id = next_id();

        let handle = self
            .tracker
            .spawn_blocking_on(self.get_service_blocking(id, service), handle);
        self.services.insert(
            id,
            ServiceInfo {
                id,
                handle,
                name,
                timeout,
            },
        );
    }

    fn get_service_future<S: BackgroundService + 'static>(
        &self,
        id: TaskId,
        service: S,
    ) -> impl Future<Output = Result<(), BoxedError>> {
        let context = self.clone();

        let services = self.services.clone();

        async move {
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
        }
    }

    fn get_service_future_local<S: LocalBackgroundService + 'static>(
        &self,
        id: TaskId,
        service: S,
    ) -> impl Future<Output = Result<(), BoxedError>> {
        let context = self.clone();

        let services = self.services.clone();

        async move {
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
        }
    }

    fn get_service_blocking<S: BlockingBackgroundService + 'static>(
        &self,
        id: TaskId,
        service: S,
    ) -> impl FnOnce() -> Result<(), BoxedError> {
        let context = self.clone();

        let services = self.services.clone();

        move || {
            let cancellation_token = context.cancellation_token();
            let res = service.run(context);
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
        }
    }
}
