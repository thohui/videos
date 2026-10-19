use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use object_store::ObjectStore;
use object_store::local::LocalFileSystem;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::Level;

use crate::archive::Archiver;
use crate::archive::task::run_archive_task;
use crate::database::{DatabasePool, run_migrations, video};
use crate::feed::task::run_feed_task;
use crate::http::{AppState, run_http_task};

mod archive;
mod config;
mod database;
mod feed;
mod http;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    let config = config::Config::from_env()?;

    tokio::fs::create_dir_all(Path::new(&config.data_directory))
        .await
        .context("failed to create data directory")?;

    let db = DatabasePool::connect(&config.database_path).await?;
    run_migrations(&db).await?;

    let requeued = video::reset_downloading(&db).await?;
    if requeued > 0 {
        tracing::info!("requeued {requeued} interrupted downloads");
    }

    let store: Arc<dyn ObjectStore> = Arc::new(
        LocalFileSystem::new_with_prefix(config.data_directory)?.with_automatic_cleanup(true),
    );
    let archiver = Archiver::new(store.clone());

    let addr: SocketAddr = config.address;

    let (wake_tx, wake_rx) = mpsc::channel(1);
    let (poll_tx, poll_rx) = mpsc::channel(1);
    let cancel = CancellationToken::new();
    let state = AppState {
        db: db.clone(),
        store,
        wake: wake_tx.clone(),
        poll_now: poll_tx,
    };

    let mut tasks = JoinSet::new();
    tasks.spawn(run_feed_task(db.clone(), wake_tx, poll_rx, cancel.clone()));
    tasks.spawn(run_archive_task(
        db.clone(),
        archiver,
        wake_rx,
        cancel.clone(),
    ));
    tasks.spawn(run_http_task(state, addr, cancel.clone()));

    tokio::select! {
        _ = tokio::signal::ctrl_c() => tracing::info!("shutting down"),
        Some(res) = tasks.join_next() => tracing::error!("task exited early: {res:?}"),
    }

    cancel.cancel();
    while let Some(res) = tasks.join_next().await {
        match res {
            Ok(Err(e)) => tracing::error!("task failed: {e:#}"),
            Err(e) => tracing::error!("task panicked: {e}"),
            Ok(Ok(())) => {}
        }
    }
    Ok(())
}
