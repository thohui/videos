use axum::extract::{Query, State};
use axum::response::IntoResponse;
use leptos::prelude::*;
use serde::Deserialize;

use super::{PageResult, chip, clock, date, page};
use crate::database::{category, channel};
use crate::database::video::{self, ListedVideo, Status};
use crate::http::AppState;

#[derive(Deserialize)]
pub struct IndexQuery {
    channel: Option<String>,
    category: Option<i64>,
}

fn filter_href(channel: Option<&str>, category: Option<i64>) -> String {
    let mut params = Vec::new();
    if let Some(channel) = channel {
        params.push(("channel", channel.to_owned()));
    }
    if let Some(category) = category {
        params.push(("category", category.to_string()));
    }
    if params.is_empty() {
        return "/".to_owned();
    }
    format!("/?{}", serde_urlencoded::to_string(params).unwrap_or_default())
}

pub async fn index(State(state): State<AppState>, Query(q): Query<IndexQuery>) -> PageResult {
    let videos = video::list(&state.db, q.channel.as_deref(), q.category, Some(Status::Done), 300, 0).await?;
    let mut channels = channel::list(&state.db).await?;
    channels.retain(|c| c.enabled);
    let categories = category::list(&state.db).await?;
    let (current_channel, current_category) = (q.channel, q.category);

    let channel_links = {
        let current_channel = current_channel.clone();
        view! {
            <nav class="channels">
                <p>"Channels"</p>
                <ul>
                    <li>
                        <a href=filter_href(None, current_category) class:active=current_channel.is_none()>"All"</a>
                    </li>
                    {channels.into_iter().map(|c| {
                        let active = current_channel.as_deref() == Some(c.id.as_str());
                        let href = filter_href(Some(&c.id), current_category);
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
        }
    };

    let category_links = (!categories.is_empty()).then(|| {
        let current_channel = current_channel.as_deref();
        view! {
            <nav class="channels">
                <p>"Categories"</p>
                <ul>
                    <li>
                        <a href=filter_href(current_channel, None) class:active=current_category.is_none()>"All"</a>
                    </li>
                    {categories.iter().map(|c| {
                        let active = current_category == Some(c.id);
                        let href = filter_href(current_channel, Some(c.id));
                        view! { <li><a href=href class:active=active>{chip(c, ())}</a></li> }
                    }).collect_view()}
                </ul>
            </nav>
        }
    });

    let body = view! {
        {videos.is_empty().then(|| view! { <p class="empty">"No videos available"</p> })}
        <main>
            <div class="grid">
                {videos.into_iter().map(card).collect_view()}
            </div>
            {channel_links}
            {category_links}
        </main>
    };
    Ok(page("videos", body).into_response())
}

fn card(v: ListedVideo) -> impl IntoView {
    let ListedVideo {
        video: v,
        channel_name,
        categories,
    } = v;
    view! {
        <a class="card" href=format!("/watch/{}", v.id)>
            <div class="thumb">
                <img src=format!("/thumb/{}", v.id) alt="" loading="lazy"/>
                {(v.duration_s > 0).then(|| view! { <span class="dur">{clock(v.duration_s)}</span> })}
            </div>
            <p class="title">{v.title}</p>
            <div class="meta">{channel_name.unwrap_or_default()} " · " {date(v.published_at)}</div>
            {(!categories.is_empty()).then(|| view! {
                <div class="chips">{categories.iter().map(|c| chip(c, ())).collect_view()}</div>
            })}
        </a>
    }
}
