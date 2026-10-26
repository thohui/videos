CREATE TABLE channels (
	id             TEXT PRIMARY KEY,
	name           TEXT NOT NULL,
	enabled        BOOLEAN NOT NULL DEFAULT TRUE CHECK (enabled IN (0,1)),
	last_polled_at INTEGER
);

CREATE TABLE videos (
	id              TEXT PRIMARY KEY,
	channel_id      TEXT REFERENCES channels(id),
	title           TEXT NOT NULL DEFAULT '',
	published_at    INTEGER,
	status          TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','downloading','done','failed','deleted','skipped')),
	attempts        INTEGER NOT NULL DEFAULT 0,
	next_attempt_at INTEGER NOT NULL DEFAULT 0,
	last_error      TEXT NOT NULL DEFAULT '',
	duration_s      INTEGER NOT NULL DEFAULT 0,
	size_bytes      INTEGER,
	created_at      INTEGER NOT NULL DEFAULT (unixepoch()),
	updated_at      INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE categories (
	id         INTEGER PRIMARY KEY AUTOINCREMENT,
	name       TEXT NOT NULL UNIQUE,
	color      INTEGER NOT NULL,
	created_at INTEGER NOT NULL DEFAULT (unixepoch()),
	updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE channels_categories (
	channel_id  TEXT NOT NULL REFERENCES channels(id)   ON DELETE CASCADE,
	category_id INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
	PRIMARY KEY (channel_id, category_id)
) WITHOUT ROWID;

CREATE TABLE videos_categories (
	video_id    TEXT NOT NULL REFERENCES videos(id)     ON DELETE CASCADE,
	category_id INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
	PRIMARY KEY (video_id, category_id)
) WITHOUT ROWID;

CREATE TRIGGER trg_videos_default_categories AFTER INSERT ON videos
WHEN NEW.channel_id IS NOT NULL
BEGIN
	INSERT OR IGNORE INTO videos_categories (video_id, category_id)
	SELECT NEW.id, category_id FROM channels_categories WHERE channel_id = NEW.channel_id;
END;

CREATE INDEX idx_videos_status  ON videos(status, next_attempt_at);
CREATE INDEX idx_videos_channel ON videos(channel_id, published_at DESC);
CREATE INDEX idx_videos_categories_category ON videos_categories(category_id);
