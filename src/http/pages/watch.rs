use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use leptos::prelude::*;

use super::{PageResult, chip, date, not_found, page};
use crate::database::{category, channel};
use crate::database::video::{self, Status};
use crate::http::AppState;

pub async fn watch(State(state): State<AppState>, Path(id): Path<String>) -> PageResult {
    let Some(v) = video::get(&state.db, &id).await?.filter(|v| v.status == Status::Done) else {
        return Ok((StatusCode::NOT_FOUND, page("not found", not_found())).into_response());
    };
    let channel = match &v.channel_id {
        Some(id) => channel::get(&state.db, id).await?,
        None => None,
    };

    let channel = channel.map(|c| {
        let href = format!("/?channel={}", c.id);
        view! { <a href=href>{c.name}</a> " · " }
    });
    let categories = category::list_for_video(&state.db, &v.id).await?;
    let categories = (!categories.is_empty()).then(|| {
        view! {
            <div class="chips">
                {categories.iter().map(|c| view! {
                    <a href=format!("/?category={}", c.id)>{chip(c, ())}</a>
                }).collect_view()}
            </div>
        }
    });
    let title = v.title.clone();
    let body = view! {
        <div class="player">
            <video
                controls
                autoplay
                preload="metadata"
                src=format!("/media/{}", v.id)
                poster=format!("/thumb/{}", v.id)
            ></video>
            <p class="title">{v.title}</p>
            <div class="meta">{channel} {date(v.published_at)}</div>
            {categories}
        </div>
    };
    Ok(page(&title, body).into_response())
}
