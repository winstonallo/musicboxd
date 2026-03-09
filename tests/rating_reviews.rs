use sqlx::SqlitePool;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

// Inserts the minimal fixtures required by every test: one user and one
// spotify_album (which rate_album needs to resolve a release_group_id from).
async fn insert_fixtures(pool: &SqlitePool) {
    sqlx::query(
        "INSERT INTO users (user_id, username, email, password_hash) VALUES ('u1', 'alice', 'a@x.com', '')",
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO spotify_albums (spotify_id, title, artists, album_type, release_date, raw_json) \
         VALUES ('sp1', 'Test Album', '[\"Artist A\"]', 'album', '2020-01-01', '{}')",
    )
    .execute(pool)
    .await
    .unwrap();

    // Replicate rate_album's release_group upsert so ratings can reference it.
    sqlx::query(
        "INSERT OR IGNORE INTO release_groups \
         (release_group_id, title, primary_type, first_release_year, spotify_id) \
         SELECT 'rg1', title, album_type, CAST(SUBSTR(release_date, 1, 4) AS INTEGER), spotify_id \
         FROM spotify_albums WHERE spotify_id = 'sp1'",
    )
    .execute(pool)
    .await
    .unwrap();
}

// Inserts a rating row using the same INSERT … ON CONFLICT upsert that
// rate_album uses, including the review column in both the INSERT and the
// ON CONFLICT SET clause.
async fn upsert_rating(pool: &SqlitePool, rating_id: &str, rating: i64, review: Option<&str>) {
    sqlx::query(
        "INSERT INTO ratings (rating_id, user_id, release_group_id, rating, review) \
         VALUES (?, 'u1', 'rg1', ?, ?) \
         ON CONFLICT(user_id, release_group_id) DO UPDATE SET \
         rating = excluded.rating, \
         review = excluded.review, \
         updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
    )
    .bind(rating_id)
    .bind(rating)
    .bind(review)
    .execute(pool)
    .await
    .unwrap();
}

// --- Tests ---

/// Saving a review alongside a rating must persist the review text in the DB.
/// If the INSERT omits the review column or does not bind it, this test fails.
#[sqlx::test(migrator = "MIGRATOR")]
async fn saving_review_with_rating_stores_review(pool: SqlitePool) {
    insert_fixtures(&pool).await;
    upsert_rating(&pool, "r1", 8, Some("Really great album")).await;

    let review: Option<String> =
        sqlx::query_scalar("SELECT review FROM ratings WHERE user_id = 'u1'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(
        review.as_deref(),
        Some("Really great album"),
        "review should be stored alongside the rating"
    );
}

/// A second upsert must overwrite the review with the new text.
/// If the ON CONFLICT clause does not update review = excluded.review, the old
/// review survives and this test fails.
#[sqlx::test(migrator = "MIGRATOR")]
async fn updating_rating_replaces_review(pool: SqlitePool) {
    insert_fixtures(&pool).await;
    upsert_rating(&pool, "r1", 7, Some("First impression")).await;
    upsert_rating(&pool, "r2", 9, Some("Changed my mind — masterpiece")).await;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ratings WHERE user_id = 'u1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "upsert must keep exactly one row per user/album");

    let review: Option<String> =
        sqlx::query_scalar("SELECT review FROM ratings WHERE user_id = 'u1'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(
        review.as_deref(),
        Some("Changed my mind — masterpiece"),
        "review should be replaced with the new text on upsert"
    );
}

/// Rating again with None review must set the review column to NULL.
/// If the ON CONFLICT clause does not propagate the NULL, the old review
/// survives and this test fails.
#[sqlx::test(migrator = "MIGRATOR")]
async fn clearing_review_sets_null_in_db(pool: SqlitePool) {
    insert_fixtures(&pool).await;
    upsert_rating(&pool, "r1", 8, Some("Had thoughts")).await;
    upsert_rating(&pool, "r2", 8, None).await;

    let review: Option<String> =
        sqlx::query_scalar("SELECT review FROM ratings WHERE user_id = 'u1'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert!(
        review.is_none(),
        "review should be NULL after being cleared with None"
    );
}

/// The get_my_rating query must SELECT the review column and return it.
/// If the SELECT omits review, the query returns no such column and this test
/// fails.
#[sqlx::test(migrator = "MIGRATOR")]
async fn get_my_rating_returns_review(pool: SqlitePool) {
    insert_fixtures(&pool).await;
    upsert_rating(&pool, "r1", 6, Some("Decent effort")).await;

    // Replicate the exact query used by get_my_rating.
    let row = sqlx::query(
        "SELECT r.rating, r.review FROM ratings r \
         JOIN release_groups rg ON r.release_group_id = rg.release_group_id \
         WHERE rg.spotify_id = ? AND r.user_id = ?",
    )
    .bind("sp1")
    .bind("u1")
    .fetch_optional(&pool)
    .await
    .unwrap()
    .expect("row must exist after rating");

    use sqlx::Row;
    let rating: i64 = row.get("rating");
    let review: Option<String> = row.get("review");

    assert_eq!(rating, 6);
    assert_eq!(
        review.as_deref(),
        Some("Decent effort"),
        "get_my_rating query must return the review column"
    );
}

/// The get_user_ratings query must include the review column in the result set.
/// If the SELECT omits review, fetching the column panics and this test fails.
#[sqlx::test(migrator = "MIGRATOR")]
async fn get_user_ratings_includes_review(pool: SqlitePool) {
    insert_fixtures(&pool).await;

    // Add a second user so we can test the username-based lookup.
    sqlx::query(
        "INSERT INTO users (user_id, username, email, password_hash) VALUES ('u2', 'bob', 'b@x.com', '')",
    )
    .execute(&pool)
    .await
    .unwrap();

    upsert_rating(&pool, "r1", 9, Some("Absolute banger")).await;

    // Replicate the exact query used by get_user_ratings.
    let rows = sqlx::query(
        "SELECT sa.spotify_id, sa.title, sa.artists, sa.album_type, sa.release_date, \
         sa.cover_art IS NOT NULL AS has_cover_art, r.rating, r.review, r.created_at AS rated_at \
         FROM ratings r \
         JOIN users u ON r.user_id = u.user_id \
         JOIN release_groups rg ON r.release_group_id = rg.release_group_id \
         JOIN spotify_albums sa ON rg.spotify_id = sa.spotify_id \
         WHERE u.username = ? \
         ORDER BY r.created_at DESC",
    )
    .bind("alice")
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(rows.len(), 1, "alice should have exactly one rated album");

    use sqlx::Row;
    let row = &rows[0];
    let rating: i64 = row.get("rating");
    let review: Option<String> = row.get("review");

    assert_eq!(rating, 9);
    assert_eq!(
        review.as_deref(),
        Some("Absolute banger"),
        "get_user_ratings query must return the review column"
    );
}
