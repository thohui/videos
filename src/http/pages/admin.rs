use axum::extract::{Form, Path, Query, State};
use axum::response::{IntoResponse, Redirect};
use leptos::prelude::*;
use serde::Deserialize;

use super::{PageResult, back_to_admin, chip, date_time, hex_color, page, parse_hex_color};
use crate::database::category::{self, Category};
use crate::database::channel::{self, Channel};
use crate::database::video::{self, ListedVideo, Status};
use crate::http::pages::AdminRedirectMessage;
use crate::http::{ApiError, AppState, actions};

#[derive(Deserialize)]
pub struct AdminQuery {
    msg: Option<String>,
    error: Option<String>,
}

pub async fn admin(State(state): State<AppState>, Query(q): Query<AdminQuery>) -> PageResult {
    let mut channels = channel::list(&state.db).await?;
    channels.retain(|c| c.enabled);
    let mut channel_rows = Vec::with_capacity(channels.len());
    for c in channels {
        let defaults = category::list_for_channel(&state.db, &c.id).await?;
        channel_rows.push((c, defaults));
    }
    let categories = category::list(&state.db).await?;
    let counts = video::status_counts(&state.db).await?;
    let videos = video::list(&state.db, None, None, None, 100, 0).await?;

    let body = view! {
        <main class="admin">
            {q.msg.map(|m| view! { <p class="msg">{m}</p> })}
            {q.error.map(|m| view! {<p> class="error">{m}</p>})}

            <h2>"Channels"</h2>
            <form method="post" action="/admin/channels">
                <input name="url" placeholder="UC... or youtube.com/channel/UC..." size="40" required/>
                <button>"add"</button>
            </form>
            " "
            <form method="post" action="/admin/poll"><button>"poll now"</button></form>
            {if channel_rows.is_empty() {
                view! { <p class="meta">"No channels yet."</p> }.into_any()
            } else {
                channel_table(channel_rows, &categories).into_any()
            }}

            <h2>"Categories"</h2>
            <form method="post" action="/admin/categories">
                <input name="name" placeholder="name" required/>
                " "
                <input type="color" name="color" value="#888888"/>
                " "
                <button>"add"</button>
            </form>
            {if categories.is_empty() {
                view! { <p class="meta">"No categories yet."</p> }.into_any()
            } else {
                category_table(&categories).into_any()
            }}

            <h2>"Videos"</h2>
            <form method="post" action="/admin/videos">
                <input name="url" placeholder="youtube.com/watch?v=..." size="40" required/>
                <button>"queue"</button>
            </form>
            <p class="meta">{counts_line(&counts)}</p>
            {(!videos.is_empty()).then(|| video_table(videos, &categories))}
        </main>
    };
    Ok(page("admin", body).into_response())
}

fn category_cell(action: String, assigned: &[Category], all: &[Category]) -> impl IntoView + use<> {
    let chips = assigned.iter().map(|c| chip(c, ())).collect_view();
    let checkboxes = all
        .iter()
        .map(|c| {
            let checked = assigned.iter().any(|a| a.id == c.id);
            view! {
                <label>
                    <input type="checkbox" name="category_id" value=c.id checked=checked/>
                    {chip(c, ())}
                </label>
            }
        })
        .collect_view();
    let picker = (!all.is_empty()).then(|| {
        view! {
            <details class="picker">
                <summary>"edit"</summary>
                <form method="post" action=action>
                    {checkboxes}
                    <button>"save"</button>
                </form>
            </details>
        }
    });
    view! { <div class="chips">{chips}{picker}</div> }
}

fn category_table(categories: &[Category]) -> impl IntoView + use<> {
    view! {
        <table>
            <tr><th>"category"</th><th>"edit"</th><th></th></tr>
            {categories.iter().map(|c| view! {
                <tr>
                    <td>{chip(c, ())}</td>
                    <td>
                        <form method="post" action=format!("/admin/categories/{}", c.id)>
                            <input name="name" value=c.name.clone() required/>
                            " "
                            <input type="color" name="color" value=hex_color(c.color)/>
                            " "
                            <button>"save"</button>
                        </form>
                    </td>
                    <td class="actions">
                        <a href=format!("/?category={}", c.id)>"videos"</a>
                        " "
                        <form
                            method="post"
                            action=format!("/admin/categories/{}/remove", c.id)
                            onsubmit="return confirm('Remove this category from all videos and channels?')"
                        >
                            <button>"remove"</button>
                        </form>
                    </td>
                </tr>
            }).collect_view()}
        </table>
    }
}

fn channel_table(
    channels: Vec<(Channel, Vec<Category>)>,
    categories: &[Category],
) -> impl IntoView + use<> {
    view! {
        <table>
            <tr><th>"name"</th><th>"id"</th><th>"last polled"</th><th>"default categories"</th><th></th></tr>
            {channels.into_iter().map(|(c, defaults)| view! {
                <tr>
                    <td><a href=format!("/?channel={}", c.id)>{c.name}</a></td>
                    <td><a href=format!("https://www.youtube.com/channel/{}", c.id)>{c.id.clone()}</a></td>
                    <td>{c.last_polled_at.map(date_time).unwrap_or_else(|| "never".to_owned())}</td>
                    <td>{category_cell(format!("/admin/channels/{}/categories", c.id), &defaults, categories)}</td>
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

fn video_table(videos: Vec<ListedVideo>, all_categories: &[Category]) -> impl IntoView + use<> {
    view! {
        <table>
            <tr><th>"title"</th><th>"channel"</th><th>"categories"</th><th>"status"</th><th>"tries"</th><th>"added"</th><th>"error"</th><th></th></tr>
            {videos.into_iter().map(|ListedVideo { video: v, channel_name, categories }| {
                let categories = category_cell(format!("/admin/videos/{}/categories", v.id), &categories, all_categories);
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
                        <td>{categories}</td>
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
        Ok(v) => back_to_admin(Some(AdminRedirectMessage::Notification(ok(v)))),
        Err(e) => back_to_admin(Some(AdminRedirectMessage::Error(e.message()))),
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
    back_to_admin(None)
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

#[derive(Deserialize)]
pub struct CategoryForm {
    name: String,
    color: String,
}

fn category_ids(form: Vec<(String, String)>) -> Result<Vec<i64>, AdminRedirectMessage> {
    form.into_iter()
        .filter(|(key, _)| key == "category_id")
        .map(|(_, value)| {
            value
                .parse()
                .map_err(|_| AdminRedirectMessage::Error(format!("invalid category id {value}")))
        })
        .collect()
}

pub async fn add_category(
    State(state): State<AppState>,
    Form(form): Form<CategoryForm>,
) -> Redirect {
    let Some(color) = parse_hex_color(&form.color) else {
        return back_to_admin(Some(AdminRedirectMessage::Error("invalid color".into())));
    };
    let result = actions::add_category(&state, &form.name, color).await;
    outcome(result, |c| format!("{} added", c.name))
}

pub async fn update_category(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(form): Form<CategoryForm>,
) -> Redirect {
    let Some(color) = parse_hex_color(&form.color) else {
        return back_to_admin(Some(AdminRedirectMessage::Error("invalid color".into())));
    };
    let result = actions::update_category(&state, id, Some(&form.name), Some(color)).await;
    outcome(result, |c| format!("{} saved", c.name))
}

pub async fn remove_category(State(state): State<AppState>, Path(id): Path<i64>) -> Redirect {
    let result = actions::delete_category(&state, id).await;
    outcome(result, |_| "category removed".to_owned())
}

pub async fn set_channel_categories(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Form(form): Form<Vec<(String, String)>>,
) -> Redirect {
    let ids = match category_ids(form) {
        Ok(ids) => ids,
        Err(e) => return back_to_admin(Some(e)),
    };
    let result = actions::set_channel_categories(&state, &id, ids).await;
    outcome(result, |_| {
        "default categories saved, they apply to videos found from now on".to_owned()
    })
}

pub async fn set_video_categories(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Form(form): Form<Vec<(String, String)>>,
) -> Redirect {
    let ids = match category_ids(form) {
        Ok(ids) => ids,
        Err(e) => return back_to_admin(Some(e)),
    };
    let result = actions::set_video_categories(&state, &id, ids).await;
    outcome(result, |_| format!("categories saved for {id}"))
}
