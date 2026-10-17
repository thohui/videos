use rusqlite::{OptionalExtension, Row};
use serde::Serialize;

use super::{DatabaseError, DatabasePool};

#[derive(Debug, Clone, Serialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub last_polled_at: Option<i64>,
}

impl Channel {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            enabled: row.get("enabled")?,
            last_polled_at: row.get("last_polled_at")?,
        })
    }
}

pub async fn insert(db: &DatabasePool, id: &str, name: &str) -> Result<(), DatabaseError> {
    let (id, name) = (id.to_owned(), name.to_owned());
    db.interact(move |c| {
        c.execute("INSERT INTO channels (id, name) VALUES (?1, ?2)", (id, name))
    })
    .await?;
    Ok(())
}

pub async fn get(db: &DatabasePool, id: &str) -> Result<Option<Channel>, DatabaseError> {
    let id = id.to_owned();
    db.interact(move |c| {
        c.query_row("SELECT * FROM channels WHERE id = ?1", [id], Channel::from_row)
            .optional()
    })
    .await
}

pub async fn list(db: &DatabasePool) -> Result<Vec<Channel>, DatabaseError> {
    db.interact(|c| {
        c.prepare("SELECT * FROM channels ORDER BY name")?
            .query_map([], Channel::from_row)?
            .collect()
    })
    .await
}

pub async fn due(db: &DatabasePool, interval_s: i64) -> Result<Vec<Channel>, DatabaseError> {
    db.interact(move |c| {
        c.prepare(
            "SELECT * FROM channels
             WHERE enabled AND (last_polled_at IS NULL OR last_polled_at <= unixepoch() - ?1)
             ORDER BY last_polled_at",
        )?
        .query_map([interval_s], Channel::from_row)?
        .collect()
    })
    .await
}

pub async fn set_polled(db: &DatabasePool, id: &str) -> Result<(), DatabaseError> {
    let id = id.to_owned();
    db.interact(move |c| {
        c.execute(
            "UPDATE channels SET last_polled_at = unixepoch() WHERE id = ?1",
            [id],
        )
    })
    .await?;
    Ok(())
}

/// Returns false if there is no such channel.
pub async fn set_enabled(db: &DatabasePool, id: &str, enabled: bool) -> Result<bool, DatabaseError> {
    let id = id.to_owned();
    let n = db
        .interact(move |c| {
            c.execute(
                "UPDATE channels SET enabled = ?2 WHERE id = ?1",
                (id, enabled),
            )
        })
        .await?;
    Ok(n > 0)
}
