use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, interval, sleep};
use tokio_util::sync::CancellationToken;

use crate::database::DatabasePool;
use crate::database::channel::{self, Channel};
use crate::database::video;

const POLL_INTERVAL_S: i64 = 30 * 60;
const TICK: Duration = Duration::from_secs(60);
const BETWEEN_CHANNELS: Duration = Duration::from_secs(2);

pub async fn run_feed_task(
    db: DatabasePool,
    wake: mpsc::Sender<()>,
    mut poll_now: mpsc::Receiver<()>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let mut interval = interval(TICK);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        let interval_s = tokio::select! {
            _ = interval.tick() => POLL_INTERVAL_S,
            Some(()) = poll_now.recv() => 0,
            _ = cancel.cancelled() => return Ok(()),
        };

        let channels = match channel::due(&db, interval_s).await {
            Ok(channels) => channels,
            Err(e) => {
                tracing::error!("failed to list due channels: {e}");
                continue;
            }
        };

        for ch in channels {
            let result = tokio::select! {
                r = poll_channel(&db, &ch) => r,
                _ = cancel.cancelled() => return Ok(()),
            };
            match result {
                Ok(0) => {}
                Ok(n) => {
                    tracing::info!("{}: {n} new videos", ch.name);
                    let _ = wake.try_send(());
                }
                Err(e) => tracing::warn!("{}: polling failed: {e:#}", ch.name),
            }

            // even on failure, so a broken channel waits a full interval
            if let Err(e) = channel::set_polled(&db, &ch.id).await {
                tracing::error!("{}: failed to mark polled: {e}", ch.name);
            }

            tokio::select! {
                _ = sleep(BETWEEN_CHANNELS) => {}
                _ = cancel.cancelled() => return Ok(()),
            }
        }
    }
}

async fn poll_channel(db: &DatabasePool, ch: &Channel) -> anyhow::Result<u32> {
    let feed = crate::feed::fetch_channel(&ch.id).await?;

    let mut new = 0;
    for entry in feed.entries {
        if entry.link.href.contains("/shorts/") {
            continue;
        }
        let published_at = Some(entry.published.timestamp());
        if video::insert(db, &entry.video_id, Some(&ch.id), &entry.title, published_at).await? {
            new += 1;
        }
    }
    Ok(new)
}
