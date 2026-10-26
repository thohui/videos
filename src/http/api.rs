use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;

use super::{ApiError, ApiResult, AppState, actions};
use crate::database::category::{self, Category};
use crate::database::channel::{self, Channel};
use crate::database::video::{self, ListedVideo, Status, Video};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/channels", get(list_channels).post(create_channel))
        .route("/channels/{id}", get(show_channel).patch(update_channel))
        .route("/channels/{id}/categories", get(list_channel_categories))
        .route(
            "/channels/{id}/categories/{category_id}",
            put(add_channel_category).delete(remove_channel_category),
        )
        .route("/poll", post(poll))
        .route("/videos", get(list_videos).post(create_video))
        .route("/videos/{id}", get(show_video).delete(delete_video))
        .route("/videos/{id}/retry", post(retry_video))
        .route("/videos/{id}/categories", get(list_video_categories))
        .route(
            "/videos/{id}/categories/{category_id}",
            put(add_video_category).delete(remove_video_category),
        )
        .route("/categories", get(list_categories).post(create_category))
        .route(
            "/categories/{id}",
            get(show_category).patch(update_category).delete(delete_category),
        )
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
    category: Option<i64>,
    status: Option<Status>,
    limit: Option<u32>,
    offset: Option<u32>,
}

async fn list_videos(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<ListedVideo>>> {
    let limit = q.limit.unwrap_or(100).min(1000);
    let videos = video::list(&state.db, q.channel.as_deref(), q.category, q.status, limit, q.offset.unwrap_or(0)).await?;
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

async fn list_categories(State(state): State<AppState>) -> ApiResult<Json<Vec<Category>>> {
    Ok(Json(category::list(&state.db).await?))
}

async fn show_category(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Category>> {
    category::get(&state.db, id)
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

#[derive(Deserialize)]
struct NewCategory {
    name: String,
    color: i64,
}

async fn create_category(
    State(state): State<AppState>,
    Json(body): Json<NewCategory>,
) -> ApiResult<(StatusCode, Json<Category>)> {
    let category = actions::add_category(&state, &body.name, body.color).await?;
    Ok((StatusCode::CREATED, Json(category)))
}

#[derive(Deserialize)]
struct CategoryUpdate {
    name: Option<String>,
    color: Option<i64>,
}

async fn update_category(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<CategoryUpdate>,
) -> ApiResult<Json<Category>> {
    Ok(Json(actions::update_category(&state, id, body.name.as_deref(), body.color).await?))
}

async fn delete_category(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    actions::delete_category(&state, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_channel_categories(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<Category>>> {
    channel::get(&state.db, &id).await?.ok_or(ApiError::NotFound)?;
    Ok(Json(category::list_for_channel(&state.db, &id).await?))
}

async fn add_channel_category(
    State(state): State<AppState>,
    Path((id, category_id)): Path<(String, i64)>,
) -> ApiResult<Json<Vec<Category>>> {
    Ok(Json(actions::add_channel_category(&state, &id, category_id).await?))
}

async fn remove_channel_category(
    State(state): State<AppState>,
    Path((id, category_id)): Path<(String, i64)>,
) -> ApiResult<Json<Vec<Category>>> {
    Ok(Json(actions::remove_channel_category(&state, &id, category_id).await?))
}

async fn list_video_categories(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<Category>>> {
    video::get(&state.db, &id).await?.ok_or(ApiError::NotFound)?;
    Ok(Json(category::list_for_video(&state.db, &id).await?))
}

async fn add_video_category(
    State(state): State<AppState>,
    Path((id, category_id)): Path<(String, i64)>,
) -> ApiResult<Json<Vec<Category>>> {
    Ok(Json(actions::add_video_category(&state, &id, category_id).await?))
}

async fn remove_video_category(
    State(state): State<AppState>,
    Path((id, category_id)): Path<(String, i64)>,
) -> ApiResult<Json<Vec<Category>>> {
    Ok(Json(actions::remove_video_category(&state, &id, category_id).await?))
}
