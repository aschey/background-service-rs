use std::time::Duration;

use background_service::{Manager, ServiceContext, Settings};
use tokio_util::sync::CancellationToken;
use tracing::info;

#[tokio::main]
pub async fn main() {
    tracing_subscriber::fmt::init();
    let token = CancellationToken::default();
    let manager = Manager::new(token.clone(), Settings::default());
    let context = manager.get_context();
    context.spawn(("service1", |context: ServiceContext| async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        context.spawn(("service2", |context: ServiceContext| async move {
            info!("starting service2");

            tokio::time::sleep(Duration::from_secs(1)).await;

            context.spawn(("service3", |context: ServiceContext| async move {
                info!("starting service3");
                context.cancelled().await;
                info!("service3 cancelled");
                Ok(())
            }));

            context.spawn(("service4", |context: ServiceContext| async move {
                info!("starting service4");
                context.cancelled().await;
                info!("service4 cancelled");
                Ok(())
            }));
            tokio::time::sleep(Duration::from_secs(1)).await;
            context.cancel_children();
            tokio::time::sleep(Duration::from_secs(1)).await;
            context.cancel_all();
            context.cancelled().await;
            Ok(())
        }));
        context.cancelled().await;
        Ok(())
    }));
    let service5 = context.spawn(("service5", |context: ServiceContext| async move {
        context.cancelled().await;
        info!("service5 cancelled");
        Ok(())
    }));
    context.cancel_service(&service5);
    manager.join_on_cancel().await.unwrap();
}
