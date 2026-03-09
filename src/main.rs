#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::{
        extract::{Extension, Path},
        http::{header, StatusCode},
        response::IntoResponse,
        routing::get,
        Router,
    };
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use musicboxd::app::{shell, App};
    use musicboxd::auth::server::{
        github_callback, github_login, google_callback, google_login, logout, session_auth,
        OAuthConfig,
    };
    use musicboxd::rate_limit::server::{rate_limit_fn, RateLimitStore};
    use musicboxd::spotify::SpotifyClient;
    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::{Row, SqlitePool};
    use std::net::SocketAddr;
    use std::sync::Arc;

    let conf = get_configuration(None).unwrap();
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;

    let routes = generate_route_list(App);

    let db_path = std::env::var("DATABASE_PATH").unwrap_or_else(|_| "musicboxd.db".to_string());

    let pool = SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .foreign_keys(true),
    )
    .await
    .expect("failed to open database");

    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("failed to run migrations");

    let base_url = std::env::var("BASE_URL").unwrap_or_else(|_| format!("http://{}", addr));

    // Validate BASE_URL format; a malformed value would cause OAuth client construction to
    // fail at request time, so catch it here at startup where the error is visible.
    if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
        eprintln!("Error: BASE_URL is not a valid URL (must start with http:// or https://): {base_url}");
        std::process::exit(1);
    }
    if !base_url.starts_with("https://") {
        eprintln!("Warning: BASE_URL does not use HTTPS ({base_url}). This is insecure in production.");
    }

    let oauth_config = OAuthConfig::from_env(&base_url).unwrap_or_else(|e| {
        eprintln!("Warning: OAuth not configured ({e}). Sign-in will be unavailable.");
        OAuthConfig {
            google_client_id: String::new(),
            google_client_secret: String::new(),
            github_client_id: String::new(),
            github_client_secret: String::new(),
            base_url: base_url.clone(),
        }
    });

    let spotify_client = SpotifyClient::from_env().unwrap_or_else(|e| {
        eprintln!("Warning: Spotify not configured ({e}). Music search will be unavailable.");
        SpotifyClient::unconfigured()
    });

    async fn album_art(
        Path(spotify_id): Path<String>,
        Extension(pool): Extension<SqlitePool>,
    ) -> impl IntoResponse {
        let row = sqlx::query(
            "SELECT cover_art, cover_art_url FROM spotify_albums WHERE spotify_id = ?",
        )
        .bind(&spotify_id)
        .fetch_optional(&pool)
        .await;

        match row {
            Ok(Some(r)) => {
                // Serve the cached blob if available.
                let bytes: Option<Vec<u8>> = r.get("cover_art");
                if let Some(b) = bytes {
                    return ([(header::CONTENT_TYPE, "image/jpeg")], b).into_response();
                }

                // Blob not yet stored (background fetch still pending or failed).
                // Fetch from the CDN URL synchronously so the image appears
                // immediately on first load, and cache the result for next time.
                let url: Option<String> = r.get("cover_art_url");
                if let Some(url) = url {
                    match reqwest::get(&url).await {
                        Ok(resp) if resp.status().is_success() => {
                            match resp.bytes().await {
                                Ok(b) => {
                                    let vec = b.to_vec();
                                    // Best-effort store; ignore errors.
                                    let _ = sqlx::query(
                                        "UPDATE spotify_albums SET cover_art = ? WHERE spotify_id = ?",
                                    )
                                    .bind(vec.as_slice())
                                    .bind(&spotify_id)
                                    .execute(&pool)
                                    .await;
                                    return ([(header::CONTENT_TYPE, "image/jpeg")], vec)
                                        .into_response();
                                }
                                Err(e) => eprintln!("album-art read error for {spotify_id}: {e}"),
                            }
                        }
                        Ok(resp) => {
                            eprintln!("album-art CDN HTTP {} for {spotify_id}", resp.status());
                        }
                        Err(e) => eprintln!("album-art fetch error for {spotify_id}: {e}"),
                    }
                }
            }
            Ok(None) => {}
            Err(e) => eprintln!("album-art DB error for {spotify_id}: {e}"),
        }

        StatusCode::NOT_FOUND.into_response()
    }

    // Per-IP rate limiters for sensitive routes. Auth allows 10 req/60s per IP;
    // album-art allows 30 req/60s per IP.
    let auth_limiter: Arc<RateLimitStore> = Arc::new(RateLimitStore::new(10, 60));
    let album_art_limiter: Arc<RateLimitStore> = Arc::new(RateLimitStore::new(30, 60));

    let auth_routes = Router::new()
        .route("/auth/google", get(google_login))
        .route("/auth/google/callback", get(google_callback))
        .route("/auth/github", get(github_login))
        .route("/auth/github/callback", get(github_callback))
        .route("/auth/logout", get(logout))
        .route_layer(axum::middleware::from_fn_with_state(
            auth_limiter,
            rate_limit_fn,
        ));

    let album_art_routes = Router::new()
        .route("/album-art/:spotify_id", get(album_art))
        .route_layer(axum::middleware::from_fn_with_state(
            album_art_limiter,
            rate_limit_fn,
        ));

    let app = Router::new()
        .merge(auth_routes)
        .merge(album_art_routes)
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            {
                let pool = pool.clone();
                let spotify_client = spotify_client.clone();
                move || {
                    provide_context(pool.clone());
                    provide_context(spotify_client.clone());
                }
            },
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(leptos_options)
        .layer(axum::middleware::from_fn(session_auth))
        .layer(Extension(oauth_config))
        .layer(Extension(spotify_client))
        .layer(Extension(pool))
        .layer(
            tower_http::set_header::SetResponseHeaderLayer::if_not_present(
                axum::http::header::HeaderName::from_static("x-content-type-options"),
                axum::http::HeaderValue::from_static("nosniff"),
            ),
        )
        .layer(
            tower_http::set_header::SetResponseHeaderLayer::if_not_present(
                axum::http::header::HeaderName::from_static("x-frame-options"),
                axum::http::HeaderValue::from_static("DENY"),
            ),
        )
        .layer(
            tower_http::set_header::SetResponseHeaderLayer::if_not_present(
                axum::http::header::HeaderName::from_static("x-xss-protection"),
                axum::http::HeaderValue::from_static("1; mode=block"),
            ),
        )
        .layer(
            tower_http::set_header::SetResponseHeaderLayer::if_not_present(
                axum::http::header::HeaderName::from_static("referrer-policy"),
                axum::http::HeaderValue::from_static("strict-origin-when-cross-origin"),
            ),
        );

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("Listening on http://{addr}");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

#[cfg(not(feature = "ssr"))]
pub fn main() {}
