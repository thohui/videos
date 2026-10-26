use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use rusqlite::{OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use super::category::Category;
use super::{DatabaseError, DatabasePool};
use crate::archive::ArchivedVideo;

#[derive(Debug, Clone, Serialize)]
pub struct Video {
    pub id: String,
    pub channel_id: Option<String>,
    pub title: String,
    pub published_at: Option<i64>,
    pub status: Status,
    pub attempts: u32,
    pub next_attempt_at: i64,
    pub last_error: String,
    pub duration_s: u32,
    pub size_bytes: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Video {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            channel_id: row.get("channel_id")?,
            title: row.get("title")?,
            published_at: row.get("published_at")?,
            status: row.get("status")?,
            attempts: row.get("attempts")?,
            next_attempt_at: row.get("next_attempt_at")?,
            last_error: row.get("last_error")?,
            duration_s: row.get("duration_s")?,
            size_bytes: row.get("size_bytes")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ListedVideo {
    #[serde(flatten)]
    pub video: Video,
    pub channel_name: Option<String>,
    pub categories: Vec<Category>,
}

impl ListedVideo {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let categories: String = row.get("categories")?;
        Ok(Self {
            video: Video::from_row(row)?,
            channel_name: row.get("channel_name")?,
            categories: serde_json::from_str(&categories)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into()))?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Downloading,
    Done,
    Failed,
    Deleted,
    Skipped,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Downloading => "downloading",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Deleted => "deleted",
            Self::Skipped => "skipped",
        }
    }
}

impl ToSql for Status {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(self.as_str().into())
    }
}

impl FromSql for Status {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "pending" => Ok(Self::Pending),
            "downloading" => Ok(Self::Downloading),
            "done" => Ok(Self::Done),
            "failed" => Ok(Self::Failed),
            "deleted" => Ok(Self::Deleted),
            "skipped" => Ok(Self::Skipped),
            other => Err(FromSqlError::Other(format!("unknown video status {other:?}").into())),
        }
    }
}

/// Returns false if the video was already known.
pub async fn insert(
    db: &DatabasePool,
    id: &str,
    channel_id: Option<&str>,
    title: &str,
    published_at: Option<i64>,
) -> Result<bool, DatabaseError> {
    let (id, channel_id, title) = (id.to_owned(), channel_id.map(str::to_owned), title.to_owned());
    let n = db
        .interact(move |c| {
            c.execute(
                "INSERT OR IGNORE INTO videos (id, channel_id, title, published_at) VALUES (?1, ?2, ?3, ?4)",
                (id, channel_id, title, published_at),
            )
        })
        .await?;
    Ok(n > 0)
}

/// Queues a single video, or gives a failed, skipped or deleted one another try.
pub async fn enqueue(db: &DatabasePool, id: &str) -> Result<(), DatabaseError> {
    let id = id.to_owned();
    db.interact(move |c| {
        c.execute(
            "INSERT INTO videos (id) VALUES (?1)
             ON CONFLICT (id) DO UPDATE
             SET status = 'pending', attempts = 0, next_attempt_at = 0, updated_at = unixepoch()
             WHERE status IN ('failed', 'skipped', 'deleted')",
            [id],
        )
    })
    .await?;
    Ok(())
}

pub async fn get(db: &DatabasePool, id: &str) -> Result<Option<Video>, DatabaseError> {
    let id = id.to_owned();
    db.interact(move |c| {
        c.query_row("SELECT * FROM videos WHERE id = ?1", [id], Video::from_row)
            .optional()
    })
    .await
}

pub async fn list(
    db: &DatabasePool,
    channel_id: Option<&str>,
    category_id: Option<i64>,
    status: Option<Status>,
    limit: u32,
    offset: u32,
) -> Result<Vec<ListedVideo>, DatabaseError> {
    let channel_id = channel_id.map(str::to_owned);
    db.interact(move |c| {
        c.prepare(
            "SELECT videos.*, channels.name AS channel_name,
               (SELECT json_group_array(json_object('id', c.id, 'name', c.name, 'color', c.color) ORDER BY c.name)
                FROM videos_categories vc JOIN categories c ON c.id = vc.category_id
                WHERE vc.video_id = videos.id) AS categories
             FROM videos LEFT JOIN channels ON channels.id = videos.channel_id
             WHERE (?1 IS NULL OR videos.channel_id = ?1)
               AND (?2 IS NULL OR EXISTS (
                     SELECT 1 FROM videos_categories vc
                     WHERE vc.video_id = videos.id AND vc.category_id = ?2))
               AND (videos.status = ?3 OR (?3 IS NULL AND videos.status != 'deleted'))
             ORDER BY coalesce(videos.published_at, videos.created_at) DESC
             LIMIT ?4 OFFSET ?5",
        )?
        .query_map((channel_id, category_id, status, limit, offset), ListedVideo::from_row)?
        .collect()
    })
    .await
}

pub async fn status_counts(db: &DatabasePool) -> Result<Vec<(Status, u32)>, DatabaseError> {
    db.interact(|c| {
        c.prepare("SELECT status, count(*) FROM videos WHERE status != 'deleted' GROUP BY status ORDER BY status")?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect()
    })
    .await
}

// Manually queued videos have no published_at, so they come first.
pub async fn claim_next(db: &DatabasePool) -> Result<Option<Video>, DatabaseError> {
    db.interact(|c| {
        c.query_row(
            "UPDATE videos
             SET status = 'downloading', attempts = attempts + 1, updated_at = unixepoch()
             WHERE id = (
                 SELECT id FROM videos
                 WHERE status = 'pending' AND next_attempt_at <= unixepoch()
                 ORDER BY published_at
                 LIMIT 1
             )
             RETURNING *",
            [],
            Video::from_row,
        )
        .optional()
    })
    .await
}

pub async fn reset_downloading(db: &DatabasePool) -> Result<usize, DatabaseError> {
    db.interact(|c| {
        c.execute(
            "UPDATE videos SET status = 'pending', updated_at = unixepoch() WHERE status = 'downloading'",
            [],
        )
    })
    .await
}

pub async fn next_attempt_at(db: &DatabasePool) -> Result<Option<i64>, DatabaseError> {
    db.interact(|c| {
        c.query_row(
            "SELECT min(next_attempt_at) FROM videos WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )
    })
    .await
}

pub async fn set_status(db: &DatabasePool, id: &str, status: Status) -> Result<(), DatabaseError> {
    let id = id.to_owned();
    db.interact(move |c| {
        c.execute(
            "UPDATE videos SET status = ?2, updated_at = unixepoch() WHERE id = ?1",
            (id, status),
        )
    })
    .await?;
    Ok(())
}

pub async fn mark_done(db: &DatabasePool, id: &str, v: &ArchivedVideo) -> Result<(), DatabaseError> {
    let id = id.to_owned();
    let (title, duration_s, published_at, size_bytes) =
        (v.title.clone(), v.duration_s, v.published_at, v.size_bytes);
    db.interact(move |c| {
        c.execute(
            "UPDATE videos
             SET status = 'done', title = ?2, duration_s = ?3, published_at = coalesce(published_at, ?4),
                 size_bytes = ?5, last_error = '', updated_at = unixepoch()
             WHERE id = ?1",
            (id, title, duration_s, published_at, size_bytes),
        )
    })
    .await?;
    Ok(())
}

/// Returns false if there is no such video or it's being downloaded right now.
pub async fn mark_deleted(db: &DatabasePool, id: &str) -> Result<bool, DatabaseError> {
    let id = id.to_owned();
    let n = db
        .interact(move |c| {
            c.execute(
                "UPDATE videos SET status = 'deleted', updated_at = unixepoch()
                 WHERE id = ?1 AND status != 'downloading'",
                [id],
            )
        })
        .await?;
    Ok(n > 0)
}

pub async fn mark_failed(db: &DatabasePool, id: &str, error: &str) -> Result<(), DatabaseError> {
    let (id, error) = (id.to_owned(), error.to_owned());
    db.interact(move |c| {
        c.execute(
            "UPDATE videos SET status = 'failed', last_error = ?2, updated_at = unixepoch() WHERE id = ?1",
            (id, error),
        )
    })
    .await?;
    Ok(())
}

pub async fn retry_at(db: &DatabasePool, id: &str, error: &str, at: i64) -> Result<(), DatabaseError> {
    let (id, error) = (id.to_owned(), error.to_owned());
    db.interact(move |c| {
        c.execute(
            "UPDATE videos SET status = 'pending', last_error = ?2, next_attempt_at = ?3, updated_at = unixepoch()
             WHERE id = ?1",
            (id, error, at),
        )
    })
    .await?;
    Ok(())
}
