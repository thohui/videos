use super::{ApiError, ApiResult, AppState};
use crate::archive::{delete_files, video_id_from_url};
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
