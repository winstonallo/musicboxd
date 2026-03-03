#![cfg(feature = "ssr")]

use musicboxd::spotify::SpotifyClient;
use sqlx::SqlitePool;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Inserts a minimal `spotify_albums` row for a given ID.
async fn seed_album(pool: &SqlitePool, spotify_id: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO spotify_albums \
         (spotify_id, title, artists, album_type, raw_json) \
         VALUES (?, ?, '[]', 'album', '{}')",
    )
    .bind(spotify_id)
    .bind(spotify_id) // use id as title — we only care about ordering/slicing
    .execute(pool)
    .await
    .unwrap();
}

/// Seeds albums and a cache entry for the given query, offset, and total.
/// `ids` are the album IDs for this specific page (offset) of results.
async fn seed_page(
    pool: &SqlitePool,
    query: &str,
    offset: u32,
    ids: &[String],
    total: u32,
) {
    for id in ids {
        seed_album(pool, id).await;
    }
    let ids_json = serde_json::to_string(ids).unwrap();
    sqlx::query(
        "INSERT INTO spotify_search_cache \
         (query, result_offset, spotify_ids, total, cached_at) \
         VALUES (?, ?, ?, ?, datetime('now'))",
    )
    .bind(query)
    .bind(offset as i64)
    .bind(&ids_json)
    .bind(total as i64)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn first_page_returns_correct_albums(pool: SqlitePool) {
    let ids: Vec<String> = (0..10).map(|i| format!("id-{i}")).collect();
    seed_page(&pool, "test", 0, &ids, 30).await;
    let client = SpotifyClient::unconfigured();

    let page = client.search(&pool, "test", 1).await.unwrap();

    assert_eq!(page.total, 30);
    assert_eq!(page.albums.len(), 10);
    assert_eq!(page.albums[0].spotify_id, ids[0]);
    assert_eq!(page.albums[9].spotify_id, ids[9]);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn third_page_returns_correct_albums(pool: SqlitePool) {
    let ids: Vec<String> = (20..30).map(|i| format!("id-{i}")).collect();
    seed_page(&pool, "test", 20, &ids, 30).await;
    let client = SpotifyClient::unconfigured();

    let page = client.search(&pool, "test", 3).await.unwrap();

    assert_eq!(page.total, 30);
    assert_eq!(page.albums.len(), 10);
    assert_eq!(page.albums[0].spotify_id, "id-20");
    assert_eq!(page.albums[9].spotify_id, "id-29");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn partial_last_page_returns_fewer_albums(pool: SqlitePool) {
    // Only 5 albums exist at offset 10, but total is 15.
    let ids: Vec<String> = (10..15).map(|i| format!("id-{i}")).collect();
    seed_page(&pool, "test", 10, &ids, 15).await;
    let client = SpotifyClient::unconfigured();

    let page = client.search(&pool, "test", 2).await.unwrap();

    assert_eq!(page.total, 15);
    assert_eq!(page.albums.len(), 5);
    assert_eq!(page.albums[4].spotify_id, "id-14");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn total_is_preserved_from_cache(pool: SqlitePool) {
    // Seed only page 1, but total says there are 200 results.
    let ids: Vec<String> = (0..10).map(|i| format!("id-{i}")).collect();
    seed_page(&pool, "test", 0, &ids, 200).await;
    let client = SpotifyClient::unconfigured();

    let page = client.search(&pool, "test", 1).await.unwrap();

    assert_eq!(page.total, 200, "total must be preserved from the cache row");
    assert_eq!(page.albums.len(), 10);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn pages_are_independent_cache_entries(pool: SqlitePool) {
    let page1_ids: Vec<String> = (0..10).map(|i| format!("id-{i}")).collect();
    let page2_ids: Vec<String> = (10..20).map(|i| format!("id-{i}")).collect();
    seed_page(&pool, "test", 0, &page1_ids, 20).await;
    seed_page(&pool, "test", 10, &page2_ids, 20).await;
    let client = SpotifyClient::unconfigured();

    let p1 = client.search(&pool, "test", 1).await.unwrap();
    let p2 = client.search(&pool, "test", 2).await.unwrap();

    let ids1: std::collections::HashSet<_> = p1.albums.iter().map(|a| &a.spotify_id).collect();
    let ids2: std::collections::HashSet<_> = p2.albums.iter().map(|a| &a.spotify_id).collect();

    assert!(ids1.is_disjoint(&ids2), "page 1 and page 2 must not overlap");
    assert_eq!(ids1.len() + ids2.len(), 20, "all pages must cover all seeded albums");
}
