-- Cached Spotify album metadata and cover art.
-- search_hit_count tracks how many times this album has appeared in search results;
-- every 100 hits we refresh from the Spotify API to pick up metadata changes.
CREATE TABLE spotify_albums (
    spotify_id      TEXT    PRIMARY KEY,
    title           TEXT    NOT NULL,
    artists         TEXT    NOT NULL,   -- JSON array of artist name strings
    album_type      TEXT,               -- "album", "single", "ep", "compilation"
    release_date    TEXT,               -- as returned by Spotify (YYYY, YYYY-MM, or YYYY-MM-DD)
    cover_art       BLOB,               -- raw JPEG bytes; NULL until first fetch completes
    cover_art_url   TEXT,               -- original Spotify CDN URL, for re-fetching
    raw_json        TEXT    NOT NULL,   -- full Spotify album object, for schema-free future fields
    cached_at       TEXT    NOT NULL DEFAULT (datetime('now')),
    search_hit_count INTEGER NOT NULL DEFAULT 0
);

-- Maps a normalized search query to an ordered list of Spotify IDs.
-- Entries expire after 24 hours so new releases appear in results promptly.
CREATE TABLE spotify_search_cache (
    query       TEXT    PRIMARY KEY,  -- lowercased, whitespace-normalized
    spotify_ids TEXT    NOT NULL,     -- JSON array of spotify_id strings, in result order
    cached_at   TEXT    NOT NULL DEFAULT (datetime('now'))
);
