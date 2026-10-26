use super::{ApiError, ApiResult, AppState};
use crate::archive::{delete_files, video_id_from_url};
use crate::database::category::{self, Category};
use crate::database::channel::{self, Channel};
use crate::database::video::{self, Status, Video};
use crate::feed;

pub async fn add_channel(state: &AppState, url: &str, name: Option<String>) -> ApiResult<Channel> {
    let id = feed::channel_id_from_url(url).map_err(|e| ApiError::BadRequest(e.to_string()))?;

    if let Some(existing) = channel::get(&state.db, &id).await? {
        if existing.enabled {
            return Err(ApiError::Conflict(format!("{} is already added", existing.name)));
        }
        channel::set_enabled(&state.db, &id, true).await?;
    } else {
        // also checks that the channel exists
        let feed = feed::fetch_channel(&id)
            .await
            .map_err(|e| ApiError::BadRequest(format!("could not fetch channel feed: {e:#}")))?;
        let name = name.filter(|n| !n.trim().is_empty()).unwrap_or(feed.title);
        channel::insert(&state.db, &id, &name).await?;
    }

    let _ = state.poll_now.try_send(());
    channel::get(&state.db, &id).await?.ok_or(ApiError::NotFound)
}

pub async fn set_channel_enabled(state: &AppState, id: &str, enabled: bool) -> ApiResult<Channel> {
    if !channel::set_enabled(&state.db, id, enabled).await? {
        return Err(ApiError::NotFound);
    }
    if enabled {
        let _ = state.poll_now.try_send(());
    }
    channel::get(&state.db, id).await?.ok_or(ApiError::NotFound)
}

pub fn poll_now(state: &AppState) {
    let _ = state.poll_now.try_send(());
}

pub async fn queue_video(state: &AppState, url: &str) -> ApiResult<Video> {
    let id = video_id_from_url(url).map_err(|e| ApiError::BadRequest(e.to_string()))?;
    video::enqueue(&state.db, &id).await?;
    let _ = state.wake.try_send(());
    video::get(&state.db, &id).await?.ok_or(ApiError::NotFound)
}

pub async fn retry_video(state: &AppState, id: &str) -> ApiResult<Video> {
    let video = video::get(&state.db, id).await?.ok_or(ApiError::NotFound)?;
    if !matches!(video.status, Status::Failed | Status::Skipped | Status::Deleted) {
        return Err(ApiError::Conflict(format!(
            "only failed, skipped or deleted videos can be retried, this one is {}",
            video.status.as_str()
        )));
    }
    video::enqueue(&state.db, id).await?;
    let _ = state.wake.try_send(());
    video::get(&state.db, id).await?.ok_or(ApiError::NotFound)
}

pub async fn delete_video(state: &AppState, id: &str) -> ApiResult<Video> {
    let video = video::get(&state.db, id).await?.ok_or(ApiError::NotFound)?;
    match video.status {
        Status::Deleted => return Ok(video),
        Status::Downloading => {
            return Err(ApiError::Conflict(
                "can't delete a video while it's downloading".to_owned(),
            ));
        }
        _ => {}
    }

    // mark it first so the archiver can't pick it up while the files go
    if !video::mark_deleted(&state.db, id).await? {
        return Err(ApiError::Conflict("video started downloading, try again later".to_owned()));
    }
    delete_files(&state.store, id).await?;
    video::get(&state.db, id).await?.ok_or(ApiError::NotFound)
}

fn category_name(name: &str) -> ApiResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest("category name can't be empty".to_owned()));
    }
    Ok(name.to_owned())
}

fn check_color(color: i64) -> ApiResult<()> {
    if !(0..=0xFFFFFF).contains(&color) {
        return Err(ApiError::BadRequest("color must be an RGB value between 0 and 0xFFFFFF".to_owned()));
    }
    Ok(())
}

pub async fn add_category(state: &AppState, name: &str, color: i64) -> ApiResult<Category> {
    let name = category_name(name)?;
    check_color(color)?;
    match category::insert(&state.db, name.clone(), color).await {
        Err(e) if e.is_unique_constraint_violation() => {
            Err(ApiError::Conflict(format!("a category named {name} already exists")))
        }
        result => Ok(result?),
    }
}

pub async fn update_category(
    state: &AppState,
    id: i64,
    name: Option<&str>,
    color: Option<i64>,
) -> ApiResult<Category> {
    let name = name.map(category_name).transpose()?;
    if let Some(color) = color {
        check_color(color)?;
    }
    match category::update(&state.db, id, name.clone(), color).await {
        Err(e) if e.is_unique_constraint_violation() => Err(ApiError::Conflict(format!(
            "a category named {} already exists",
            name.unwrap_or_default()
        ))),
        result => result?.ok_or(ApiError::NotFound),
    }
}

pub async fn delete_category(state: &AppState, id: i64) -> ApiResult<()> {
    if !category::delete(&state.db, id).await? {
        return Err(ApiError::NotFound);
    }
    Ok(())
}

pub async fn add_video_category(state: &AppState, video_id: &str, category_id: i64) -> ApiResult<Vec<Category>> {
    video::get(&state.db, video_id).await?.ok_or(ApiError::NotFound)?;
    category::get(&state.db, category_id).await?.ok_or(ApiError::NotFound)?;
    category::add_to_video(&state.db, video_id, category_id).await?;
    Ok(category::list_for_video(&state.db, video_id).await?)
}

pub async fn remove_video_category(state: &AppState, video_id: &str, category_id: i64) -> ApiResult<Vec<Category>> {
    if !category::remove_from_video(&state.db, video_id, category_id).await? {
        return Err(ApiError::NotFound);
    }
    Ok(category::list_for_video(&state.db, video_id).await?)
}

/// Only applies to videos found after this, existing videos keep their categories.
pub async fn add_channel_category(state: &AppState, channel_id: &str, category_id: i64) -> ApiResult<Vec<Category>> {
    channel::get(&state.db, channel_id).await?.ok_or(ApiError::NotFound)?;
    category::get(&state.db, category_id).await?.ok_or(ApiError::NotFound)?;
    category::add_to_channel(&state.db, channel_id, category_id).await?;
    Ok(category::list_for_channel(&state.db, channel_id).await?)
}

pub async fn remove_channel_category(
    state: &AppState,
    channel_id: &str,
    category_id: i64,
) -> ApiResult<Vec<Category>> {
    if !category::remove_from_channel(&state.db, channel_id, category_id).await? {
        return Err(ApiError::NotFound);
    }
    Ok(category::list_for_channel(&state.db, channel_id).await?)
}

async fn check_categories_exist(state: &AppState, category_ids: &[i64]) -> ApiResult<()> {
    let known = category::list(&state.db).await?;
    if let Some(id) = category_ids.iter().find(|id| !known.iter().any(|c| c.id == **id)) {
        return Err(ApiError::BadRequest(format!("there is no category {id}")));
    }
    Ok(())
}

pub async fn set_video_categories(
    state: &AppState,
    video_id: &str,
    category_ids: Vec<i64>,
) -> ApiResult<Vec<Category>> {
    video::get(&state.db, video_id).await?.ok_or(ApiError::NotFound)?;
    check_categories_exist(state, &category_ids).await?;
    category::set_for_video(&state.db, video_id, category_ids).await?;
    Ok(category::list_for_video(&state.db, video_id).await?)
}

/// Only applies to videos found after this, existing videos keep their categories.
pub async fn set_channel_categories(
    state: &AppState,
    channel_id: &str,
    category_ids: Vec<i64>,
) -> ApiResult<Vec<Category>> {
    channel::get(&state.db, channel_id).await?.ok_or(ApiError::NotFound)?;
    check_categories_exist(state, &category_ids).await?;
    category::set_for_channel(&state.db, channel_id, category_ids).await?;
    Ok(category::list_for_channel(&state.db, channel_id).await?)
}
