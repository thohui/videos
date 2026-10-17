use axum::extract::{Query, State};
use axum::response::IntoResponse;
use leptos::prelude::*;
use serde::Deserialize;

use super::{PageResult, clock, date, page};
use crate::database::channel;
use crate::database::video::{self, ListedVideo, Status};
use crate::http::AppState;

#[derive(Deserialize)]
pub struct IndexQuery {
    channel: Option<String>,
}

pub async fn index(State(state): State<AppState>, Query(q): Query<IndexQuery>) -> PageResult {
    let videos = video::list(&state.db, q.channel.as_deref(), Some(Status::Done), 300, 0).await?;
    let mut channels = channel::list(&state.db).await?;
    channels.retain(|c| c.enabled);
    let current = q.channel;

    let body = view! {
        {videos.is_empty().then(|| view! { <p class="empty">"No videos available"</p> })}
        <main>
            <div class="grid">
                {videos.into_iter().map(card).collect_view()}
            </div>
            <nav class="channels">
                <p>"Channels"</p>
                <ul>
                    <li><a href="/" class:active=current.is_none()>"All"</a></li>
                    {channels.into_iter().map(|c| {
                        let active = current.as_deref() == Some(c.id.as_str());
                        let href = format!("/?channel={}", c.id);
                        view! {
                            <li>
                                <a href=href class:active=active>
                                    {if c.name.is_empty() { c.id } else { c.name }}
                                </a>
                            </li>
                        }
                    }).collect_view()}
                </ul>
            </nav>
        </main>
    };
    Ok(page("videos", body).into_response())
}

fn card(v: ListedVideo) -> impl IntoView {
    let ListedVideo {
        video: v,
        channel_name,
    } = v;
    view! {
        <a class="card" href=format!("/watch/{}", v.id)>
            <div class="thumb">
                <img src=format!("/thumb/{}", v.id) alt="" loading="lazy"/>
                {(v.duration_s > 0).then(|| view! { <span class="dur">{clock(v.duration_s)}</span> })}
            </div>
            <p class="title">{v.title}</p>
            <div class="meta">{channel_name.unwrap_or_default()} " · " {date(v.published_at)}</div>
        </a>
    }
}
