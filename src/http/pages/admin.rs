use axum::extract::{Form, Path, Query, State};
use axum::response::{IntoResponse, Redirect};
use leptos::prelude::*;
use serde::Deserialize;

use super::{PageResult, back_to_admin, date_time, page};
use crate::database::channel::{self, Channel};
use crate::database::video::{self, ListedVideo, Status};
use crate::http::{ApiError, AppState, actions};

#[derive(Deserialize)]
pub struct AdminQuery {
    msg: Option<String>,
}

pub async fn admin(State(state): State<AppState>, Query(q): Query<AdminQuery>) -> PageResult {
    let mut channels = channel::list(&state.db).await?;
    channels.retain(|c| c.enabled);
    let counts = video::status_counts(&state.db).await?;
    let videos = video::list(&state.db, None, None, 100, 0).await?;

    let body = view! {
        <main class="admin">
            {q.msg.map(|m| view! { <p class="msg">{m}</p> })}

            <h2>"Channels"</h2>
            <form method="post" action="/admin/channels">
                <input name="url" placeholder="UC... or youtube.com/channel/UC..." size="40" required/>
                <button>"add"</button>
            </form>
            " "
            <form method="post" action="/admin/poll"><button>"poll now"</button></form>
            {if channels.is_empty() {
                view! { <p class="meta">"No channels yet."</p> }.into_any()
            } else {
                channel_table(channels).into_any()
            }}

            <h2>"Videos"</h2>
            <form method="post" action="/admin/videos">
                <input name="url" placeholder="youtube.com/watch?v=..." size="40" required/>
                <button>"queue"</button>
            </form>
            <p class="meta">{counts_line(&counts)}</p>
            {(!videos.is_empty()).then(|| video_table(videos))}
        </main>
    };
    Ok(page("admin", body).into_response())
}

fn channel_table(channels: Vec<Channel>) -> impl IntoView {
    view! {
        <table>
            <tr><th>"name"</th><th>"id"</th><th>"last polled"</th><th></th></tr>
            {channels.into_iter().map(|c| view! {
                <tr>
                    <td><a href=format!("/?channel={}", c.id)>{c.name}</a></td>
                    <td><a href=format!("https://www.youtube.com/channel/{}", c.id)>{c.id.clone()}</a></td>
                    <td>{c.last_polled_at.map(date_time).unwrap_or_else(|| "never".to_owned())}</td>
                    <td>
                        <form
                            method="post"
                            action=format!("/admin/channels/{}/remove", c.id)
                            onsubmit="return confirm('Remove this channel?')"
                        >
                            <button>"remove"</button>
                        </form>
                    </td>
                </tr>
            }).collect_view()}
        </table>
    }
}

fn video_table(videos: Vec<ListedVideo>) -> impl IntoView {
    view! {
        <table>
            <tr><th>"title"</th><th>"channel"</th><th>"status"</th><th>"tries"</th><th>"added"</th><th>"error"</th><th></th></tr>
            {videos.into_iter().map(|ListedVideo { video: v, channel_name }| {
                let title = if v.title.is_empty() { v.id.clone() } else { v.title };
                let retry_label = match v.status {
                    Status::Failed | Status::Skipped => Some("retry"),
                    Status::Deleted => Some("restore"),
                    _ => None,
                };
                let removable = !matches!(v.status, Status::Deleted | Status::Downloading);
                view! {
                    <tr>
                        <td>
                            {if v.status == Status::Done {
                                view! { <a href=format!("/watch/{}", v.id)>{title}</a> }.into_any()
                            } else {
                                title.into_any()
                            }}
                        </td>
                        <td>{channel_name.unwrap_or_default()}</td>
                        <td>{v.status.as_str()}</td>
                        <td>{v.attempts}</td>
                        <td>{date_time(v.created_at)}</td>
                        <td class="err">{v.last_error}</td>
                        <td class="actions">
                            {retry_label.map(|label| view! {
                                <form method="post" action=format!("/admin/videos/{}/retry", v.id)>
                                    <button>{label}</button>
                                </form>
                            })}
                            {removable.then(|| view! {
                                <form
                                    method="post"
                                    action=format!("/admin/videos/{}/remove", v.id)
                                    onsubmit="return confirm('Remove this video and its files?')"
                                >
                                    <button>"remove"</button>
                                </form>
                            })}
                        </td>
                    </tr>
                }
            }).collect_view()}
        </table>
    }
}

fn counts_line(counts: &[(Status, u32)]) -> String {
    if counts.is_empty() {
        return "No videos yet.".to_owned();
    }
    counts
        .iter()
        .map(|(s, n)| format!("{}: {n}", s.as_str()))
        .collect::<Vec<_>>()
        .join(" · ")
}

#[derive(Deserialize)]
pub struct UrlForm {
    url: String,
}

fn outcome<T>(result: Result<T, ApiError>, ok: impl FnOnce(T) -> String) -> Redirect {
    match result {
        Ok(v) => back_to_admin(&ok(v)),
        Err(e) => back_to_admin(&e.message()),
    }
}

pub async fn add_channel(State(state): State<AppState>, Form(form): Form<UrlForm>) -> Redirect {
    let result = actions::add_channel(&state, &form.url, None).await;
    outcome(result, |c| format!("{} added, polling now", c.name))
}

pub async fn remove_channel(State(state): State<AppState>, Path(id): Path<String>) -> Redirect {
    let result = actions::set_channel_enabled(&state, &id, false).await;
    outcome(result, |c| format!("{} removed", c.name))
}

pub async fn poll(State(state): State<AppState>) -> Redirect {
    actions::poll_now(&state);
    back_to_admin("poll started")
}

pub async fn queue_video(State(state): State<AppState>, Form(form): Form<UrlForm>) -> Redirect {
    let result = actions::queue_video(&state, &form.url).await;
    outcome(result, |v| format!("{} queued", v.id))
}

pub async fn retry_video(State(state): State<AppState>, Path(id): Path<String>) -> Redirect {
    let result = actions::retry_video(&state, &id).await;
    outcome(result, |v| format!("{} queued again", v.id))
}

pub async fn remove_video(State(state): State<AppState>, Path(id): Path<String>) -> Redirect {
    let result = actions::delete_video(&state, &id).await;
    outcome(result, |v| format!("{} removed", v.id))
}
