CREATE TABLE IF NOT EXISTS users (
    user_id       TEXT PRIMARY KEY,
    username      TEXT NOT NULL,
    email         TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE (username),
    UNIQUE (email)
);

CREATE TABLE IF NOT EXISTS follows (
    follower_id TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    followee_id TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (follower_id, followee_id),
    CHECK (follower_id != followee_id)
);

CREATE TABLE IF NOT EXISTS release_groups (
    release_group_id   TEXT PRIMARY KEY,
    title              TEXT NOT NULL,
    primary_type       TEXT,
    first_release_year INTEGER,
    created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS release_group_external_ids (
    release_group_id TEXT NOT NULL REFERENCES release_groups(release_group_id) ON DELETE CASCADE,
    provider         TEXT NOT NULL,
    external_id      TEXT NOT NULL,
    PRIMARY KEY (release_group_id, provider),
    UNIQUE (provider, external_id)
);

CREATE TABLE IF NOT EXISTS ratings (
    rating_id        TEXT PRIMARY KEY,
    user_id          TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    release_group_id TEXT NOT NULL REFERENCES release_groups(release_group_id),
    rating           INTEGER NOT NULL CHECK (rating BETWEEN 1 AND 10),
    review           TEXT,
    created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE (user_id, release_group_id)
);
