use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use object_store::ObjectStore;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::database::DatabasePool;

mod actions;
mod api;
mod media;
mod pages;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabasePool,
    pub store: Arc<dyn ObjectStore>,
    pub wake: mpsc::Sender<()>,
    pub poll_now: mpsc::Sender<()>,
}

pub async fn run_http_task(
    state: AppState,
    addr: SocketAddr,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let app = Router::new()
        .merge(pages::routes())
        .nest("/api", api::routes())
        .merge(media::routes())
        .with_state(state);

    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;
    tracing::info!("listening on http://{addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(cancel.cancelled_owned())
        .await?;
    Ok(())
}

enum ApiError {
    BadRequest(String),
    NotFound,
    Conflict(String),
    Internal(anyhow::Error),
}

type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn message(&self) -> String {
        match self {
            Self::BadRequest(m) | Self::Conflict(m) => m.clone(),
            Self::NotFound => "not found".to_owned(),
            Self::Internal(e) => {
                tracing::error!("request failed: {e:#}");
                "internal error".to_owned()
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status(), Json(json!({ "error": self.message() }))).into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        Self::Internal(e.into())
    }
}
