-- Cached track listing for each album. Populated on first visit to the album
-- detail page; the ON DELETE CASCADE means tracks are cleaned up automatically
-- if an album row is ever removed from spotify_albums.
CREATE TABLE spotify_tracks (
    spotify_id   TEXT    NOT NULL REFERENCES spotify_albums(spotify_id) ON DELETE CASCADE,
    track_id     TEXT    NOT NULL,
    disc_number  INTEGER NOT NULL DEFAULT 1,
    track_number INTEGER NOT NULL,
    name         TEXT    NOT NULL,
    artists      TEXT    NOT NULL,  -- JSON array of display-name strings
    duration_ms  INTEGER,
    PRIMARY KEY (spotify_id, track_id)
);
