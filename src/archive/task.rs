use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use crate::archive::{Archiver, Outcome, YtDlpError, is_permanent};
use crate::database::video::{self, Status, Video};
use crate::database::{DatabaseError, DatabasePool};

const MAX_ATTEMPTS: u32 = 5;
const MAX_IDLE: Duration = Duration::from_secs(5 * 60);
const BLOCKED_PAUSE: Duration = Duration::from_secs(30 * 60);
const UPCOMING_RETRY: Duration = Duration::from_secs(6 * 60 * 60);
const MAX_RETRY: Duration = Duration::from_secs(24 * 60 * 60);

pub async fn run_archive_task(
    db: DatabasePool,
    archiver: Archiver,
    mut wake: mpsc::Receiver<()>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    loop {
        match video::claim_next(&db).await {
            Ok(Some(job)) => {
                tracing::info!("{}: downloading (attempt {})", job.id, job.attempts);
                let result = tokio::select! {
                    r = archiver.archive(&job.id) => r,
                    _ = cancel.cancelled() => return Ok(()),
                };

                let blocked = matches!(yt_dlp_error(&result), Some(YtDlpError::Blocked(_)));
                if let Err(e) = record(&db, &job, result).await {
                    tracing::error!("{}: failed to record result: {e}", job.id);
                }

                // the next video would hit the same wall, so back off entirely
                if blocked {
                    tracing::warn!(
                        "blocked by YouTube, pausing for {}m",
                        BLOCKED_PAUSE.as_secs() / 60
                    );
                    tokio::select! {
                        _ = sleep(BLOCKED_PAUSE) => {}
                        _ = cancel.cancelled() => return Ok(()),
                    }
                }
                continue;
            }
            Ok(None) => {}
            Err(e) => tracing::error!("failed to claim next video: {e}"),
        }

        let idle = idle_time(&db).await;
        tokio::select! {
            Some(()) = wake.recv() => {}
            _ = sleep(idle) => {}
            _ = cancel.cancelled() => return Ok(()),
        }
    }
}

async fn record(
    db: &DatabasePool,
    job: &Video,
    result: anyhow::Result<Outcome>,
) -> Result<(), DatabaseError> {
    let e = match result {
        Ok(Outcome::Stored(v)) => {
            tracing::info!("{}: archived {:?}", job.id, v.title);
            return video::mark_done(db, &job.id, &v).await;
        }
        Ok(Outcome::Skipped) => {
            tracing::info!("{}: skipped (live or too short)", job.id);
            return video::set_status(db, &job.id, Status::Skipped).await;
        }
        Err(e) => e,
    };

    let msg = format!("{e:#}");
    let delay = match e.downcast_ref::<YtDlpError>() {
        // not the video's fault, so these don't count towards MAX_ATTEMPTS
        Some(YtDlpError::Blocked(_)) => Some(BLOCKED_PAUSE),
        Some(YtDlpError::Upcoming(_)) => Some(UPCOMING_RETRY),
        _ if is_permanent(&e) || job.attempts >= MAX_ATTEMPTS => None,
        _ => Some(backoff(job.attempts)),
    };

    match delay {
        Some(delay) => {
            tracing::warn!(
                "{}: failed, retrying in {}m: {msg}",
                job.id,
                delay.as_secs() / 60
            );
            video::retry_at(db, &job.id, &msg, now() + delay.as_secs() as i64).await
        }
        None => {
            tracing::warn!("{}: failed permanently: {msg}", job.id);
            video::mark_failed(db, &job.id, &msg).await
        }
    }
}

fn yt_dlp_error(result: &anyhow::Result<Outcome>) -> Option<&YtDlpError> {
    result.as_ref().err()?.downcast_ref()
}

// 5m, 10m, 20m, etc. capped at a day
fn backoff(attempts: u32) -> Duration {
    let mins = 5u64.saturating_mul(1 << attempts.saturating_sub(1).min(16));
    Duration::from_secs(mins * 60).min(MAX_RETRY)
}

async fn idle_time(db: &DatabasePool) -> Duration {
    match video::next_attempt_at(db).await {
        Ok(Some(at)) => Duration::from_secs((at - now()).max(1) as u64).min(MAX_IDLE),
        _ => MAX_IDLE,
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
