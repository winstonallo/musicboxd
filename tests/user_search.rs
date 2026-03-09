use sqlx::Row;
use sqlx::SqlitePool;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// The exact SQL used by the `search_users` server function. Parameterised as
/// (viewer_id, like_pattern) so every test exercises the real query path.
const SEARCH_SQL: &str = "\
    SELECT u.user_id, u.username, u.bio, \
           (SELECT COUNT(*) FROM follows WHERE followee_id = u.user_id) AS follower_count, \
           CASE WHEN f.follower_id IS NOT NULL THEN 1 ELSE 0 END AS is_following \
    FROM users u \
    LEFT JOIN follows f ON f.follower_id = ? AND f.followee_id = u.user_id \
    WHERE u.username LIKE ? ESCAPE '\\' \
    ORDER BY u.username \
    LIMIT 50";

#[sqlx::test(migrator = "MIGRATOR")]
async fn search_users_returns_matching_usernames(pool: SqlitePool) {
    sqlx::query("INSERT INTO users (user_id, username, email) VALUES ('u1', 'alice', 'a@x.com')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO users (user_id, username, email) VALUES ('u2', 'bob', 'b@x.com')")
        .execute(&pool).await.unwrap();

    let rows = sqlx::query(SEARCH_SQL)
        .bind("")
        .bind("%ali%")
        .fetch_all(&pool)
        .await
        .unwrap();

    assert_eq!(rows.len(), 1, "expected exactly one result for 'ali'");
    let name: String = rows[0].get("username");
    assert_eq!(name, "alice");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn search_users_excludes_non_matching(pool: SqlitePool) {
    sqlx::query("INSERT INTO users (user_id, username, email) VALUES ('u1', 'alice', 'a@x.com')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO users (user_id, username, email) VALUES ('u2', 'bob', 'b@x.com')")
        .execute(&pool).await.unwrap();

    let rows = sqlx::query(SEARCH_SQL)
        .bind("")
        .bind("%carol%")
        .fetch_all(&pool)
        .await
        .unwrap();

    assert_eq!(rows.len(), 0, "no users should match 'carol'");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn search_users_is_following_true_when_viewer_follows(pool: SqlitePool) {
    sqlx::query("INSERT INTO users (user_id, username, email) VALUES ('u1', 'alice', 'a@x.com')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO users (user_id, username, email) VALUES ('u2', 'bob', 'b@x.com')")
        .execute(&pool).await.unwrap();

    sqlx::query("INSERT INTO follows (follower_id, followee_id) VALUES ('u1', 'u2')")
        .execute(&pool).await.unwrap();

    let rows = sqlx::query(SEARCH_SQL)
        .bind("u1")      // viewer is alice
        .bind("%bob%")
        .fetch_all(&pool)
        .await
        .unwrap();

    assert_eq!(rows.len(), 1, "bob should appear in results");
    let is_following: i64 = rows[0].get("is_following");
    assert_eq!(is_following, 1, "alice follows bob so is_following should be 1");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn search_users_is_following_false_when_not_logged_in(pool: SqlitePool) {
    sqlx::query("INSERT INTO users (user_id, username, email) VALUES ('u1', 'alice', 'a@x.com')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO users (user_id, username, email) VALUES ('u2', 'bob', 'b@x.com')")
        .execute(&pool).await.unwrap();

    // alice follows bob, but the viewer is unauthenticated (empty string viewer_id)
    sqlx::query("INSERT INTO follows (follower_id, followee_id) VALUES ('u1', 'u2')")
        .execute(&pool).await.unwrap();

    let rows = sqlx::query(SEARCH_SQL)
        .bind("")      // no viewer — simulates unauthenticated request
        .bind("%bob%")
        .fetch_all(&pool)
        .await
        .unwrap();

    assert_eq!(rows.len(), 1, "bob should appear in results");
    let is_following: i64 = rows[0].get("is_following");
    assert_eq!(is_following, 0, "unauthenticated viewer should see is_following = 0");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn search_users_follower_count_correct(pool: SqlitePool) {
    sqlx::query("INSERT INTO users (user_id, username, email) VALUES ('u1', 'alice', 'a@x.com')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO users (user_id, username, email) VALUES ('u2', 'bob', 'b@x.com')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO users (user_id, username, email) VALUES ('u3', 'carol', 'c@x.com')")
        .execute(&pool).await.unwrap();

    // Both bob and carol follow alice
    sqlx::query("INSERT INTO follows (follower_id, followee_id) VALUES ('u2', 'u1')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO follows (follower_id, followee_id) VALUES ('u3', 'u1')")
        .execute(&pool).await.unwrap();

    let rows = sqlx::query(SEARCH_SQL)
        .bind("")
        .bind("%alice%")
        .fetch_all(&pool)
        .await
        .unwrap();

    assert_eq!(rows.len(), 1, "alice should appear in results");
    let follower_count: i64 = rows[0].get("follower_count");
    assert_eq!(follower_count, 2, "alice should have 2 followers");
}
