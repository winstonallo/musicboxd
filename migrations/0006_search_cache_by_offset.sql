-- Redesign the search result cache to index by (query, offset) instead of
-- query alone. Spotify reduced its search limit from 50 to 10 in February 2026,
-- so caching all results upfront is no longer possible. Instead we cache one
-- page of 10 results at a time, keyed by the exact offset we passed to the API.
-- The total column stores Spotify's reported result count so the UI can compute
-- the page count without an extra round-trip.
DROP TABLE IF EXISTS spotify_search_cache;

CREATE TABLE spotify_search_cache (
    query         TEXT    NOT NULL,
    result_offset INTEGER NOT NULL,   -- Spotify API offset (0, 10, 20, ...)
    spotify_ids   TEXT    NOT NULL,   -- JSON array of spotify_id strings, in result order
    total         INTEGER NOT NULL,   -- Spotify's reported total for this query
    cached_at     TEXT    NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (query, result_offset)
);
