CREATE TABLE series (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id TEXT NOT NULL,
    title TEXT NOT NULL,
    alias TEXT UNIQUE,
    category TEXT NOT NULL,
    last_episode TEXT,
    last_interaction_at TEXT,
    cover_url TEXT,
    UNIQUE (source_id, title)
);

CREATE TABLE seen (
    dedup_key TEXT PRIMARY KEY,
    series_id INTEGER NOT NULL REFERENCES series (id) ON DELETE CASCADE,
    episode TEXT NOT NULL,
    resolution TEXT NOT NULL,
    method TEXT NOT NULL,
    link TEXT NOT NULL,
    first_seen_at TEXT NOT NULL
);

CREATE INDEX seen_series_id_idx ON seen (series_id);

CREATE TABLE pending (
    series_id INTEGER NOT NULL REFERENCES series (id) ON DELETE CASCADE,
    episode TEXT NOT NULL,
    first_seen_at TEXT NOT NULL,
    desired_resolution TEXT NOT NULL,
    best_resolution TEXT,
    best_variant_json TEXT,
    PRIMARY KEY (series_id, episode)
);

CREATE TABLE interactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    series_id INTEGER NOT NULL REFERENCES series (id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    detail TEXT,
    at TEXT NOT NULL
);

CREATE INDEX interactions_series_id_idx ON interactions (series_id);
