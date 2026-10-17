use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use object_store::{GetOptions, GetRange, ObjectStoreExt};

use super::AppState;
use crate::archive::{Archiver, validate_id};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/media/{id}", get(video))
        .route("/thumb/{id}", get(thumbnail))
}

async fn thumbnail(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if validate_id(&id).is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let bytes = match state.store.get(&Archiver::thumbnail_path(&id)).await {
        Ok(r) => r.bytes().await,
        Err(e) => Err(e),
    };
    match bytes {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            bytes,
        )
            .into_response(),
        Err(e) => error_response(e),
    }
}

async fn video(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    if validate_id(&id).is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let path = Archiver::video_path(&id);
    let size = match state.store.head(&path).await {
        Ok(meta) => meta.size,
        Err(e) => return error_response(e),
    };

    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| parse_range(v, size));
    let (start, end) = range.unwrap_or((0, size));

    let options = GetOptions {
        range: Some(GetRange::Bounded(start..end)),
        ..Default::default()
    };
    let stream = match state.store.get_opts(&path, options).await {
        Ok(r) => r.into_stream(),
        Err(e) => return error_response(e),
    };

    let mut response = Response::builder()
        .header(header::CONTENT_TYPE, "video/mp4")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, end - start);
    if range.is_some() {
        response = response
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_RANGE, format!("bytes {start}-{}/{size}", end - 1));
    }
    response
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Parses a single `bytes=` range into a half-open `start..end`.
fn parse_range(header: &str, size: u64) -> Option<(u64, u64)> {
    let spec = header.strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None;
    }
    let (start, end) = spec.split_once('-')?;
    let (start, end) = match (start.trim(), end.trim()) {
        ("", n) => (size.saturating_sub(n.parse().ok()?), size),
        (s, "") => (s.parse().ok()?, size),
        (s, e) => (s.parse().ok()?, e.parse::<u64>().ok()?.saturating_add(1).min(size)),
    };
    (start < end).then_some((start, end))
}

fn error_response(e: object_store::Error) -> Response {
    match e {
        object_store::Error::NotFound { .. } => StatusCode::NOT_FOUND.into_response(),
        e => {
            tracing::error!("reading from store: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
