use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use crate::database::{DatabaseError, DatabasePool};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub color: i64,
}

impl Category {
    pub fn from_row(r: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get(0)?,
            name: r.get(1)?,
            color: r.get(2)?,
        })
    }
}

pub async fn list(pool: &DatabasePool) -> Result<Vec<Category>, DatabaseError> {
    pool.interact(move |c| {
        c.prepare("SELECT id, name, color FROM categories ORDER BY name")?
            .query_map([], Category::from_row)?
            .collect()
    })
    .await
}

pub async fn get(pool: &DatabasePool, id: i64) -> Result<Option<Category>, DatabaseError> {
    pool.interact(move |c| {
        c.query_row(
            "SELECT id, name, color FROM categories WHERE id = ?1",
            params![id],
            Category::from_row,
        )
        .optional()
    })
    .await
}

pub async fn insert(pool: &DatabasePool, name: String, color: i64) -> Result<Category, DatabaseError> {
    pool.interact(move |c| {
        c.query_one(
            "INSERT INTO categories (name, color) VALUES (?1, ?2) RETURNING id, name, color",
            params![name, color],
            Category::from_row,
        )
    })
    .await
}

pub async fn update(
    pool: &DatabasePool,
    id: i64,
    name: Option<String>,
    color: Option<i64>,
) -> Result<Option<Category>, DatabaseError> {
    pool.interact(move |c| {
        c.query_row(
            "UPDATE categories
             SET name = coalesce(?2, name), color = coalesce(?3, color), updated_at = unixepoch()
             WHERE id = ?1
             RETURNING id, name, color",
            params![id, name, color],
            Category::from_row,
        )
        .optional()
    })
    .await
}

/// Returns false if there is no such category.
pub async fn delete(pool: &DatabasePool, id: i64) -> Result<bool, DatabaseError> {
    let n = pool
        .interact(move |c| c.execute("DELETE FROM categories WHERE id = ?1", params![id]))
        .await?;
    Ok(n > 0)
}

pub async fn list_for_video(pool: &DatabasePool, video_id: &str) -> Result<Vec<Category>, DatabaseError> {
    let video_id = video_id.to_owned();
    pool.interact(move |c| {
        c.prepare(
            "SELECT categories.id, categories.name, categories.color
             FROM videos_categories JOIN categories ON categories.id = videos_categories.category_id
             WHERE videos_categories.video_id = ?1
             ORDER BY categories.name",
        )?
        .query_map(params![video_id], Category::from_row)?
        .collect()
    })
    .await
}

pub async fn add_to_video(pool: &DatabasePool, video_id: &str, category_id: i64) -> Result<(), DatabaseError> {
    let video_id = video_id.to_owned();
    pool.interact(move |c| {
        c.execute(
            "INSERT OR IGNORE INTO videos_categories (video_id, category_id) VALUES (?1, ?2)",
            params![video_id, category_id],
        )
    })
    .await?;
    Ok(())
}

/// Returns false if the video didn't have the category.
pub async fn remove_from_video(pool: &DatabasePool, video_id: &str, category_id: i64) -> Result<bool, DatabaseError> {
    let video_id = video_id.to_owned();
    let n = pool
        .interact(move |c| {
            c.execute(
                "DELETE FROM videos_categories WHERE video_id = ?1 AND category_id = ?2",
                params![video_id, category_id],
            )
        })
        .await?;
    Ok(n > 0)
}

pub async fn list_for_channel(pool: &DatabasePool, channel_id: &str) -> Result<Vec<Category>, DatabaseError> {
    let channel_id = channel_id.to_owned();
    pool.interact(move |c| {
        c.prepare(
            "SELECT categories.id, categories.name, categories.color
             FROM channels_categories JOIN categories ON categories.id = channels_categories.category_id
             WHERE channels_categories.channel_id = ?1
             ORDER BY categories.name",
        )?
        .query_map(params![channel_id], Category::from_row)?
        .collect()
    })
    .await
}

pub async fn add_to_channel(pool: &DatabasePool, channel_id: &str, category_id: i64) -> Result<(), DatabaseError> {
    let channel_id = channel_id.to_owned();
    pool.interact(move |c| {
        c.execute(
            "INSERT OR IGNORE INTO channels_categories (channel_id, category_id) VALUES (?1, ?2)",
            params![channel_id, category_id],
        )
    })
    .await?;
    Ok(())
}

/// Returns false if the channel didn't have the category as a default.
pub async fn remove_from_channel(
    pool: &DatabasePool,
    channel_id: &str,
    category_id: i64,
) -> Result<bool, DatabaseError> {
    let channel_id = channel_id.to_owned();
    let n = pool
        .interact(move |c| {
            c.execute(
                "DELETE FROM channels_categories WHERE channel_id = ?1 AND category_id = ?2",
                params![channel_id, category_id],
            )
        })
        .await?;
    Ok(n > 0)
}

fn set_links(
    c: &mut rusqlite::Connection,
    table: &str,
    owner_column: &str,
    owner_id: &str,
    category_ids: &[i64],
) -> rusqlite::Result<()> {
    let tx = c.transaction()?;
    tx.execute(&format!("DELETE FROM {table} WHERE {owner_column} = ?1"), params![owner_id])?;
    {
        let mut insert = tx.prepare(&format!(
            "INSERT OR IGNORE INTO {table} ({owner_column}, category_id) VALUES (?1, ?2)"
        ))?;
        for category_id in category_ids {
            insert.execute(params![owner_id, category_id])?;
        }
    }
    tx.commit()
}

pub async fn set_for_video(pool: &DatabasePool, video_id: &str, category_ids: Vec<i64>) -> Result<(), DatabaseError> {
    let video_id = video_id.to_owned();
    pool.interact(move |c| set_links(c, "videos_categories", "video_id", &video_id, &category_ids))
        .await
}

pub async fn set_for_channel(
    pool: &DatabasePool,
    channel_id: &str,
    category_ids: Vec<i64>,
) -> Result<(), DatabaseError> {
    let channel_id = channel_id.to_owned();
    pool.interact(move |c| set_links(c, "channels_categories", "channel_id", &channel_id, &category_ids))
        .await
}
