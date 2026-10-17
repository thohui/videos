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

CREATE INDEX idx_videos_status  ON videos(status, next_attempt_at);
CREATE INDEX idx_videos_channel ON videos(channel_id, published_at DESC);
