use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use futures::Future;
use tokio_util::sync::{CancellationToken, WaitForCancellationFuture};
use tracing::{error, info};

use crate::error::{BackgroundServiceError, BoxedError};
use crate::service_info_wasm::ServiceInfo;
use crate::{next_id, BackgroundService, LocalBackgroundService, TaskId};

#[derive(Clone, Debug)]
pub struct ServiceContext {
    services: Rc<RefCell<HashMap<TaskId, ServiceInfo>>>,
    root_token: CancellationToken,
    self_token: CancellationToken,
    child_token: CancellationToken,
}

impl ServiceContext {
    pub(crate) fn new(
        services: Rc<RefCell<HashMap<TaskId, ServiceInfo>>>,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            root_token: cancellation_token.clone(),
            self_token: cancellation_token.clone(),
            child_token: cancellation_token.child_token(),
            services,
        }
    }

    fn child(&self) -> Self {
        Self {
            child_token: self.child_token.child_token(),
            self_token: self.child_token.clone(),
            ..self.clone()
        }
    }

    pub fn take_service(&self, id: &TaskId) -> Option<ServiceInfo> {
        if let Some(service) = self.services.borrow_mut().remove(id) {
            Some(service)
        } else {
            None
        }
    }

    pub async fn wait_for_shutdown(
        &self,
        id: &TaskId,
    ) -> Option<Result<(), BackgroundServiceError>> {
        if let Some(service) = self.services.borrow_mut().remove(id) {
            Some(service.wait_for_shutdown().await)
        } else {
            None
        }
    }

    pub fn cancel_service(&self, id: &TaskId) {
        if let Some(service) = self.services.borrow().get(id) {
            service.cancel();
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

    pub fn is_cancelled(&self) -> bool {
        self.self_token.is_cancelled()
    }

    pub fn spawn<S: BackgroundService + 'static>(&self, service: S) -> TaskId {
        let name = service.name().to_owned();
        let timeout = service.shutdown_timeout();
        let child = self.child();
        let id = next_id();

        let fut = child.get_service_future(id, service);
        wasm_compat::futures::spawn(async move {
            let _ = fut.await.inspect_err(|e| error!("{e:?}"));
        });
        self.services.borrow_mut().insert(
            id,
            ServiceInfo {
                id,
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

        let fut = child.get_service_future_local(id, service);
        wasm_compat::futures::spawn_local(async move {
            let _ = fut.await.inspect_err(|e| error!("{e:?}"));
        });
        self.services.borrow_mut().insert(
            id,
            ServiceInfo {
                id,
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
}

fn handle_service_remove(
    id: &TaskId,
    res: Result<(), BoxedError>,
    services: Rc<RefCell<HashMap<TaskId, ServiceInfo>>>,
    cancellation_token: CancellationToken,
) -> Result<(), BoxedError> {
    // If cancellation was requested, the manager is being shut down anyway
    // so we don't need to remove the service
    if !cancellation_token.is_cancelled() {
        if let Some(service) = services.borrow_mut().remove(id) {
            info!("Removing {}", service.name);
            if let Err(e) = &res {
                error!("Service {} exited with error: {e:?}", service.name);
            }
        }
    }

    res
}
