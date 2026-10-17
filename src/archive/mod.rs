use std::path::{Path as FsPath, PathBuf};
use std::process::ExitStatus;
use std::sync::Arc;

use anyhow::{Context, Result, ensure};
use object_store::buffered::BufWriter;
use object_store::path::Path;
use object_store::{ObjectStore, ObjectStoreExt};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub mod task;

#[derive(Debug, Clone)]
pub enum Outcome {
    Stored(ArchivedVideo),
    Skipped,
}

#[derive(Debug, thiserror::Error)]
pub enum YtDlpError {
    #[error("video is private")]
    Private,
    #[error("video is members-only")]
    MembersOnly,
    #[error("video is age-restricted")]
    AgeRestricted,
    #[error("video is unavailable: {0}")]
    Unavailable(String),
    #[error("video is not available yet: {0}")]
    Upcoming(String),
    #[error("blocked by YouTube: {0}")]
    Blocked(String),
    #[error("yt-dlp failed ({status}): {stderr}")]
    Other { status: ExitStatus, stderr: String },
}

impl YtDlpError {
    fn from_stderr(status: ExitStatus, stderr: &str) -> Self {
        let msg = stderr
            .lines()
            .rev()
            .find(|l| l.starts_with("ERROR:"))
            .unwrap_or(stderr)
            .trim()
            .to_string();
        let lower = msg.to_lowercase();
        let has = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));

        if has(&["private video"]) {
            Self::Private
        } else if has(&["members-only", "join this channel"]) {
            Self::MembersOnly
        } else if has(&["not a bot", "http error 429", "too many requests"]) {
            Self::Blocked(msg)
        } else if has(&[
            "confirm your age",
            "age-restricted",
            "inappropriate for some users",
        ]) {
            Self::AgeRestricted
        } else if has(&["premieres in", "live event will begin", "will begin in"]) {
            Self::Upcoming(msg)
        } else if has(&[
            "video unavailable",
            "video is unavailable",
            "video is not available",
            "has been removed",
            "no longer available",
            "not available in your country",
            "account associated with this video has been terminated",
        ]) {
            Self::Unavailable(msg)
        } else {
            Self::Other {
                status,
                stderr: stderr.to_string(),
            }
        }
    }

    pub fn is_permanent(&self) -> bool {
        matches!(
            self,
            Self::Private | Self::MembersOnly | Self::AgeRestricted | Self::Unavailable(_)
        )
    }
}

pub fn is_permanent(err: &anyhow::Error) -> bool {
    err.downcast_ref::<YtDlpError>()
        .is_some_and(YtDlpError::is_permanent)
}

#[derive(Debug, Clone)]
pub struct ArchivedVideo {
    pub id: String,
    pub title: String,
    pub video: Path,
    pub thumbnail: Option<Path>,
    pub duration_s: Option<u32>,
    pub published_at: Option<i64>,
    pub size_bytes: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VideoInfo {
    id: String,
    title: String,
    duration: Option<f64>,
    timestamp: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct Archiver {
    store: Arc<dyn ObjectStore>,
    max_height: u32,
    temp_dir: PathBuf,
    yt_dlp: String,
}

impl Archiver {
    pub fn new(store: Arc<dyn ObjectStore>) -> Self {
        Self {
            store,
            max_height: 1080,
            temp_dir: std::env::temp_dir(),
            yt_dlp: "yt-dlp".into(),
        }
    }

    pub fn store(&self) -> &Arc<dyn ObjectStore> {
        &self.store
    }

    pub fn video_path(id: &str) -> Path {
        Path::from(format!("{id}/video.mp4"))
    }

    pub fn thumbnail_path(id: &str) -> Path {
        Path::from(format!("{id}/thumb.jpg"))
    }

    pub fn info_path(id: &str) -> Path {
        Path::from(format!("{id}/info.json"))
    }

    /// True if the video for `id` is already in the store.
    pub async fn is_archived(&self, id: &str) -> Result<bool> {
        validate_id(id)?;
        match self.store.head(&Self::video_path(id)).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(e).context("checking store"),
        }
    }

    pub async fn archive(&self, id: &str) -> Result<Outcome> {
        validate_id(id)?;
        let tmp = self
            .temp_dir
            .join(format!("ytdl-{id}-{}", std::process::id()));

        tokio::fs::create_dir_all(&tmp)
            .await
            .with_context(|| format!("creating temp dir {}", tmp.display()))?;

        let result = self.run(id, &tmp).await;
        tokio::fs::remove_dir_all(&tmp).await.ok(); // always clean up
        result.with_context(|| format!("archiving {id}"))
    }

    async fn run(&self, id: &str, tmp: &FsPath) -> Result<Outcome> {
        let h = self.max_height;
        let format = format!(
            "bv*[vcodec^=avc1][height<={h}]+ba[acodec^=mp4a]/b[ext=mp4][height<={h}]/b[height<={h}]/b"
        );
        let url = format!("https://www.youtube.com/watch?v={id}");
        let tmp_str = tmp.to_str().context("temp path is not UTF-8")?;

        let out = Command::new(&self.yt_dlp)
            .args([
                "--no-progress",
                "--no-playlist",
                "--ignore-errors", // a failed thumbnail download shouldn't lose the video
                "-j",
                "--no-simulate",
                "-f",
                &format,
                "--merge-output-format",
                "mp4",
                "--remux-video",
                "mp4",
                "--postprocessor-args",
                "Merger+ffmpeg_o:-movflags +faststart",
                "--match-filter",
                "!is_live & !was_live & duration > 90", // no live streams or shorts
                "--write-thumbnail",
                "--convert-thumbnails",
                "jpg",
                "-P",
                tmp_str,
                "-o",
                "video.%(ext)s",
                "-o",
                "thumbnail:thumb.%(ext)s",
                &url,
            ])
            .kill_on_drop(true)
            .output()
            .await
            .context("failed to run yt-dlp (is it installed and on PATH?)")?;

        // With --ignore-errors the exit code can be non-zero even when the video is fine, so judge success by whether video.mp4 exists.
        let video_file = tmp.join("video.mp4");
        if !tokio::fs::try_exists(&video_file).await? {
            if out.status.success() {
                return Ok(Outcome::Skipped);
            }
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(YtDlpError::from_stderr(out.status, stderr.trim()).into());
        }

        let stdout = String::from_utf8_lossy(&out.stdout);
        let info_line = stdout.lines().find(|l| l.starts_with('{')).unwrap_or("{}");

        let info: VideoInfo =
            serde_json::from_str(info_line).context("parsing yt-dlp -j output")?;
        let size_bytes = tokio::fs::metadata(&video_file)
            .await
            .ok()
            .map(|m| m.len() as i64);

        let video = Self::video_path(id);
        upload_file(&self.store, &video_file, &video)
            .await
            .with_context(|| format!("uploading {video}"))?;

        let thumb_file = tmp.join("thumb.jpg");
        let thumbnail = if tokio::fs::try_exists(&thumb_file).await? {
            let p = Self::thumbnail_path(id);
            let data = tokio::fs::read(&thumb_file)
                .await
                .context("reading thumbnail")?;
            self.store
                .put(&p, data.into())
                .await
                .with_context(|| format!("uploading {p}"))?;
            Some(p)
        } else {
            None
        };

        Ok(Outcome::Stored(ArchivedVideo {
            id: id.to_string(),
            title: info.title,
            video,
            thumbnail,
            duration_s: info.duration.map(|d| d.round() as u32),
            published_at: info.timestamp,
            size_bytes,
        }))
    }
}

pub async fn delete_files(store: &Arc<dyn ObjectStore>, id: &str) -> Result<()> {
    validate_id(id)?;
    for path in [
        Archiver::video_path(id),
        Archiver::thumbnail_path(id),
        Archiver::info_path(id),
    ] {
        match store.delete(&path).await {
            Ok(()) | Err(object_store::Error::NotFound { .. }) => {}
            Err(e) => return Err(e).with_context(|| format!("deleting {path}")),
        }
    }
    Ok(())
}

pub fn validate_id(id: &str) -> Result<()> {
    ensure!(
        id.len() == 11
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "invalid YouTube video id: {id:?}"
    );
    Ok(())
}

pub fn video_id_from_url(input: &str) -> Result<String> {
    let input = input.trim();
    if validate_id(input).is_ok() {
        return Ok(input.to_owned());
    }

    let url = Url::parse(input)
        .or_else(|_| Url::parse(&format!("https://{input}")))
        .with_context(|| format!("not a URL: {input:?}"))?;
    let host = url.host_str().unwrap_or_default();
    let host = host
        .strip_prefix("www.")
        .or_else(|| host.strip_prefix("m."))
        .or_else(|| host.strip_prefix("music."))
        .unwrap_or(host);
    let mut path = url.path_segments().into_iter().flatten();

    let id = match (host, path.next()) {
        ("youtu.be", Some(id)) => Some(id.to_owned()),
        ("youtube.com" | "youtube-nocookie.com", Some("watch")) => url
            .query_pairs()
            .find(|(k, _)| k == "v")
            .map(|(_, v)| v.into_owned()),
        ("youtube.com" | "youtube-nocookie.com", Some("shorts" | "embed" | "live" | "v")) => {
            path.next().map(str::to_owned)
        }
        _ => None,
    };
    let id = id.with_context(|| format!("not a YouTube video URL: {input:?}"))?;
    validate_id(&id)?;
    Ok(id)
}

async fn upload_file(store: &Arc<dyn ObjectStore>, src: &FsPath, dest: &Path) -> Result<()> {
    let mut file = tokio::fs::File::open(src)
        .await
        .with_context(|| format!("opening {}", src.display()))?;
    let mut writer = BufWriter::new(store.clone(), dest.clone());
    if let Err(e) = tokio::io::copy(&mut file, &mut writer).await {
        writer.abort().await.ok();
        return Err(e.into());
    }
    writer.shutdown().await?; // object only appears once this succeeds
    Ok(())
}
