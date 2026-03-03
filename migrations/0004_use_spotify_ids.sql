-- Replace the MusicBrainz-era external-IDs indirection with a direct spotify_id
-- column on release_groups. The release_group_external_ids table was designed to
-- store provider/external_id pairs (e.g. provider="musicbrainz"), which is no
-- longer needed now that Spotify is the sole data source.
--
-- SQLite does not support ADD COLUMN ... REFERENCES or DROP TABLE with FK checks
-- easily, so we recreate release_groups to add the nullable spotify_id FK and
-- drop the now-redundant external_ids table.

PRAGMA foreign_keys = OFF;

CREATE TABLE release_groups_new (
    release_group_id   TEXT PRIMARY KEY,
    title              TEXT NOT NULL,
    primary_type       TEXT,
    first_release_year INTEGER,
    spotify_id         TEXT UNIQUE REFERENCES spotify_albums(spotify_id),
    created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

INSERT INTO release_groups_new
    (release_group_id, title, primary_type, first_release_year, created_at)
SELECT release_group_id, title, primary_type, first_release_year, created_at
FROM release_groups;

DROP TABLE release_group_external_ids;
DROP TABLE release_groups;
ALTER TABLE release_groups_new RENAME TO release_groups;

PRAGMA foreign_keys = ON;
