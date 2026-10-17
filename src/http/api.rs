use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use super::{ApiError, ApiResult, AppState, actions};
use crate::database::channel::{self, Channel};
use crate::database::video::{self, ListedVideo, Status, Video};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/channels", get(list_channels).post(create_channel))
        .route("/channels/{id}", get(show_channel).patch(update_channel))
        .route("/poll", post(poll))
        .route("/videos", get(list_videos).post(create_video))
        .route("/videos/{id}", get(show_video).delete(delete_video))
        .route("/videos/{id}/retry", post(retry_video))
}

async fn list_channels(State(state): State<AppState>) -> ApiResult<Json<Vec<Channel>>> {
    Ok(Json(channel::list(&state.db).await?))
}

async fn show_channel(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Channel>> {
    channel::get(&state.db, &id)
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

#[derive(Deserialize)]
struct NewChannel {
    url: String,
    name: Option<String>,
}

async fn create_channel(
    State(state): State<AppState>,
    Json(body): Json<NewChannel>,
) -> ApiResult<(StatusCode, Json<Channel>)> {
    let channel = actions::add_channel(&state, &body.url, body.name).await?;
    Ok((StatusCode::CREATED, Json(channel)))
}

#[derive(Deserialize)]
struct ChannelUpdate {
    enabled: bool,
}

async fn update_channel(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ChannelUpdate>,
) -> ApiResult<Json<Channel>> {
    Ok(Json(actions::set_channel_enabled(&state, &id, body.enabled).await?))
}

async fn poll(State(state): State<AppState>) -> StatusCode {
    actions::poll_now(&state);
    StatusCode::ACCEPTED
}

#[derive(Deserialize)]
struct ListQuery {
    channel: Option<String>,
    status: Option<Status>,
    limit: Option<u32>,
    offset: Option<u32>,
}

async fn list_videos(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<ListedVideo>>> {
    let limit = q.limit.unwrap_or(100).min(1000);
    let videos = video::list(&state.db, q.channel.as_deref(), q.status, limit, q.offset.unwrap_or(0)).await?;
    Ok(Json(videos))
}

async fn show_video(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Video>> {
    video::get(&state.db, &id)
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

#[derive(Deserialize)]
struct NewVideo {
    url: String,
}

async fn create_video(
    State(state): State<AppState>,
    Json(body): Json<NewVideo>,
) -> ApiResult<(StatusCode, Json<Video>)> {
    let video = actions::queue_video(&state, &body.url).await?;
    Ok((StatusCode::ACCEPTED, Json(video)))
}

async fn retry_video(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Video>> {
    Ok(Json(actions::retry_video(&state, &id).await?))
}

async fn delete_video(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Video>> {
    Ok(Json(actions::delete_video(&state, &id).await?))
}
