use axum::Router;
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use chrono::DateTime;
use leptos::prelude::*;

use super::AppState;

mod admin;
mod index;
mod watch;

const STYLE: &str = include_str!("../../../style/main.css");

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(index::index))
        .route("/watch/{id}", get(watch::watch))
        .route("/admin", get(admin::admin))
        .route("/admin/channels", post(admin::add_channel))
        .route("/admin/channels/{id}/remove", post(admin::remove_channel))
        .route("/admin/poll", post(admin::poll))
        .route("/admin/videos", post(admin::queue_video))
        .route("/admin/videos/{id}/retry", post(admin::retry_video))
        .route("/admin/videos/{id}/remove", post(admin::remove_video))
        .route("/static/style.css", get(style))
        .fallback(|| async { (StatusCode::NOT_FOUND, page("not found", not_found())) })
}

async fn style() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css")], STYLE)
}

fn page(title: &str, body: impl IntoView + 'static) -> Html<String> {
    let title = title.to_owned();
    let html = view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <title>{title}</title>
                <link rel="stylesheet" href="/static/style.css"/>
            </head>
            <body>
                <header>
                    <a href="/">"videos"</a>
                    <a class="nav" href="/admin">"admin"</a>
                </header>
                {body}
            </body>
        </html>
    };
    Html(html.to_html())
}

fn not_found() -> impl IntoView {
    view! { <p class="empty">"Not found"</p> }
}

fn back_to_admin(msg: &str) -> Redirect {
    let query = serde_urlencoded::to_string([("msg", msg)]).unwrap_or_default();
    Redirect::to(&format!("/admin?{query}"))
}

struct PageError(anyhow::Error);

impl<E: Into<anyhow::Error>> From<E> for PageError {
    fn from(e: E) -> Self {
        Self(e.into())
    }
}

impl IntoResponse for PageError {
    fn into_response(self) -> Response {
        tracing::error!("request failed: {:#}", self.0);
        (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
    }
}

type PageResult = Result<Response, PageError>;

/// 4:05 or 1:02:05
fn clock(secs: u32) -> String {
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn format_time(ts: i64, fmt: &str) -> String {
    DateTime::from_timestamp(ts, 0)
        .map(|t| t.format(fmt).to_string())
        .unwrap_or_default()
}

fn date(ts: Option<i64>) -> String {
    ts.map(|ts| format_time(ts, "%-d %b %Y")).unwrap_or_default()
}

fn date_time(ts: i64) -> String {
    format_time(ts, "%-d %b %H:%M")
}
