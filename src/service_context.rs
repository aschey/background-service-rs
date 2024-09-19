use std::sync::Arc;

use dashmap::DashMap;
use futures::Future;
use tokio::runtime::Handle;
use tokio::task::LocalSet;
use tokio_util::sync::{
    CancellationToken, WaitForCancellationFuture, WaitForCancellationFutureOwned,
};
use tokio_util::task::TaskTracker;
use tracing::{error, info};

use crate::error::{BackgroundServiceError, BoxedError};
use crate::service_info::ServiceInfo;
use crate::{
    next_id, BackgroundService, BlockingBackgroundService, LocalBackgroundService, TaskId,
};

#[derive(Clone, Debug)]
pub struct ServiceContext {
    services: Arc<DashMap<TaskId, ServiceInfo>>,
    tracker: TaskTracker,
    root_token: CancellationToken,
    self_token: CancellationToken,
    child_token: CancellationToken,
}

impl ServiceContext {
    pub(crate) fn new(
        services: Arc<DashMap<TaskId, ServiceInfo>>,
        tracker: TaskTracker,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            root_token: cancellation_token.clone(),
            self_token: cancellation_token.clone(),
            child_token: cancellation_token.child_token(),
            tracker,
            services,
        }
    }

    fn child(&self) -> Self {
        let self_token = self.child_token.child_token();
        Self {
            child_token: self_token.child_token(),
            self_token,
            ..self.clone()
        }
    }

    pub fn take_service(&self, id: &TaskId) -> Option<ServiceInfo> {
        if let Some((_, service)) = self.services.remove(id) {
            Some(service)
        } else {
            None
        }
    }

    pub async fn wait_for_shutdown(
        &self,
        id: &TaskId,
    ) -> Option<Result<(), BackgroundServiceError>> {
        if let Some((_, service)) = self.services.remove(id) {
            Some(service.wait_for_shutdown().await)
        } else {
            None
        }
    }

    pub fn cancel_service(&self, id: &TaskId) {
        if let Some(service) = self.services.get(id) {
            service.cancel();
        }
    }

    pub fn abort_service(&self, id: &TaskId) {
        if let Some(service) = self.services.get(id) {
            service.abort();
        }
    }

    pub fn cancel_all(&self) {
        self.root_token.cancel();
    }

    pub fn cancel_children(&self) {
        self.child_token.cancel();
    }

    pub fn cancelled(&self) -> WaitForCancellationFuture {
        self.self_token.cancelled()
    }

    pub fn cancelled_owned(&self) -> WaitForCancellationFutureOwned {
        self.self_token.clone().cancelled_owned()
    }

    pub fn is_cancelled(&self) -> bool {
        self.self_token.is_cancelled()
    }

    pub fn spawn<S: BackgroundService + Send + 'static>(&self, service: S) -> TaskId {
        let name = service.name().to_owned();
        let timeout = service.shutdown_timeout();
        let child = self.child();
        let id = next_id();

        let handle = self.tracker.spawn(child.get_service_future(id, service));
        self.services.insert(
            id,
            ServiceInfo {
                id,
                handle,
                name,
                timeout,
                cancellation_token: child.self_token,
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
        let child = self.child();
        let id = next_id();

        let handle = self
            .tracker
            .spawn_on(child.get_service_future(id, service), handle);
        self.services.insert(
            id,
            ServiceInfo {
                id,
                handle,
                name,
                timeout,
                cancellation_token: child.self_token,
            },
        );
        id
    }

    pub fn spawn_local<S: LocalBackgroundService + 'static>(&self, service: S) -> TaskId {
        let name = service.name().to_owned();
        let timeout = service.shutdown_timeout();
        let child = self.child();
        let id = next_id();

        let handle = self
            .tracker
            .spawn_local(child.get_service_future_local(id, service));
        self.services.insert(
            id,
            ServiceInfo {
                id,
                handle,
                name,
                timeout,
                cancellation_token: child.self_token,
            },
        );
        id
    }

    pub fn spawn_local_on<S: LocalBackgroundService + 'static>(
        &self,
        service: S,
        local_set: &LocalSet,
    ) -> TaskId {
        let name = service.name().to_owned();
        let timeout = service.shutdown_timeout();
        let child = self.child();
        let id = next_id();

        let handle = self
            .tracker
            .spawn_local_on(child.get_service_future_local(id, service), local_set);
        self.services.insert(
            id,
            ServiceInfo {
                id,
                handle,
                name,
                timeout,
                cancellation_token: child.self_token,
            },
        );
        id
    }

    pub fn spawn_blocking<S: BlockingBackgroundService + Send + 'static>(
        &self,
        service: S,
    ) -> TaskId {
        let name = service.name().to_owned();
        let timeout = service.shutdown_timeout();
        let child = self.child();
        let id = next_id();

        let handle = self
            .tracker
            .spawn_blocking(child.get_service_blocking(id, service));
        self.services.insert(
            id,
            ServiceInfo {
                id,
                handle,
                name,
                timeout,
                cancellation_token: child.self_token,
            },
        );
        id
    }

    pub fn spawn_blocking_on<S: BlockingBackgroundService + Send + 'static>(
        &self,
        service: S,
        handle: &Handle,
    ) -> TaskId {
        let name = service.name().to_owned();
        let timeout = service.shutdown_timeout();
        let child = self.child();
        let id = next_id();

        let handle = self
            .tracker
            .spawn_blocking_on(child.get_service_blocking(id, service), handle);
        self.services.insert(
            id,
            ServiceInfo {
                id,
                handle,
                name,
                timeout,
                cancellation_token: child.self_token,
            },
        );
        id
    }

    fn get_service_future<S: BackgroundService + 'static>(
        &self,
        id: TaskId,
        service: S,
    ) -> impl Future<Output = Result<(), BoxedError>> {
        let context = self.clone();

        let services = self.services.clone();

        async move {
            let cancellation_token = context.root_token.clone();
            let res = service.run(context).await;
            handle_service_remove(&id, res, services, cancellation_token)
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
            let cancellation_token = context.root_token.clone();
            let res = service.run(context).await;
            handle_service_remove(&id, res, services, cancellation_token)
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
            let cancellation_token = context.root_token.clone();
            let res = service.run(context);
            handle_service_remove(&id, res, services, cancellation_token)
        }
    }
}

fn handle_service_remove(
    id: &TaskId,
    res: Result<(), BoxedError>,
    services: Arc<DashMap<TaskId, ServiceInfo>>,
    cancellation_token: CancellationToken,
) -> Result<(), BoxedError> {
    // If cancellation was requested, the manager is being shut down anyway
    // so we don't need to remove the service
    if !cancellation_token.is_cancelled() {
        if let Some((_, service)) = services.remove(id) {
            info!("Removing {}", service.name);
            if let Err(e) = &res {
                error!("Service {} exited with error: {e:?}", service.name);
            }
        }
    }

    res
}
