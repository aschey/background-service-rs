// taken from the blocking issue example here https://github.com/tokio-rs/tokio/issues/4730#issuecomment-1147165954


use std::process::{self};
use std::thread;
use std::time::Duration;

use background_service::{BackgroundServiceManager, ServiceContext, Settings};
use tokio_util::sync::CancellationToken;

#[tokio::main]
pub async fn main() {
    tracing_subscriber::fmt::init();
    let token = CancellationToken::default();
    let manager = BackgroundServiceManager::new(token.clone(), Settings::default().blocking_task_monitor_interval(Duration::from_secs(2)));
    let mut context = manager.get_context();

    context.add_service(("blocking", |_: ServiceContext| async move {
        let orig_thread_id = format!("{:?}", thread::current().id());
        let mut switched = false;
        loop {
            println!("{:?}: blocking still alive", thread::current().id());
            thread::sleep(Duration::from_secs(2));
            loop {
                // here we loop and sleep until we switch threads, once we do, we never call await
                // again blocking all progress on all other tasks forever
                let thread_id = format!("{:?}", thread::current().id());
                if !switched && thread_id == orig_thread_id {
                    println!("waiting to switch threads");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                } else {
                    switched = true;
                    println!("switched threads, blocking forever");
                    break;
                }
            }
        }
    }));

    context.add_service(("nonblocking", |_: ServiceContext| async move {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            println!("{:?}: nonblocking still alive", thread::current().id());
        }
    }));

    let token = context.cancellation_token();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        token.cancel();
        println!("cancelled");
    });

    let res = manager.join_on_cancel().await;
    println!("{res:?}");
    if let Err(e) = res {
        // one or more services timed out
        if !e.timed_out().is_empty() {
            println!("task timed out");
            // the blocking task will still be running here while the nonblocking was successfully
            // aborted.
            tokio::time::sleep(Duration::from_secs(5)).await;
            // Tokio could hang forever while trying to shut down if a task is stuck in a blocking
            // state. Forcing a process exit here prevents this.
            println!("force exit");
            process::exit(1);
        }
    }
}
