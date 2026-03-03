use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::{A, FlatRoutes, Route, Router},
    hooks::{use_navigate, use_params_map, use_query_map},
    ParamSegment, StaticSegment,
};
use serde::{Deserialize, Serialize};

/// A Spotify album as returned by search/lookup server fns.
/// Ungated so both the SSR binary and the WASM hydration bundle can
/// (de)serialize it across the server fn boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotifyAlbum {
    pub spotify_id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album_type: String,
    pub release_year: Option<u32>,
    /// True once the cover art BLOB has been stored in the DB. The first
    /// search response may be false; subsequent ones will be true after the
    /// background fetch completes.
    pub has_cover_art: bool,
}

/// A single track as returned by the album detail server fn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub track_id: String,
    pub disc_number: u32,
    pub track_number: u32,
    pub name: String,
    pub artists: Vec<String>,
    pub duration_ms: Option<u32>,
}

/// Album metadata combined with its full track listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumDetail {
    pub album: SpotifyAlbum,
    pub tracks: Vec<Track>,
}

/// A page of search results together with the total result count.
/// Ungated so both the SSR binary and WASM hydration can (de)serialize it.
/// `total` is the total number of cached results (≤50), used by the client
/// to compute the page count without an extra round-trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchPage {
    pub albums: Vec<SpotifyAlbum>,
    pub total: usize,
}

#[server]
pub async fn get_current_user() -> Result<Option<String>, ServerFnError> {
    use crate::auth::server::CurrentUser;
    use axum::Extension;
    let Extension(user): Extension<Option<CurrentUser>> = leptos_axum::extract().await?;
    Ok(user.map(|u| u.username))
}

#[server]
pub async fn search_music(query: String, page: u32) -> Result<SearchPage, ServerFnError> {
    use crate::spotify::SpotifyClient;
    use axum::Extension;
    use sqlx::SqlitePool;
    let page = page.max(1);
    let Extension(pool): Extension<SqlitePool> = leptos_axum::extract().await?;
    let Extension(spotify): Extension<SpotifyClient> = leptos_axum::extract().await?;
    spotify.search(&pool, &query, page).await
}

#[server]
pub async fn get_album_detail(spotify_id: String) -> Result<AlbumDetail, ServerFnError> {
    use crate::spotify::SpotifyClient;
    use axum::Extension;
    use sqlx::SqlitePool;
    let Extension(pool): Extension<SqlitePool> = leptos_axum::extract().await?;
    let Extension(spotify): Extension<SpotifyClient> = leptos_axum::extract().await?;
    spotify.get_album_detail(&pool, &spotify_id).await
}

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <link rel="stylesheet" href="/style.css"/>
                <AutoReload options=options.clone()/>
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    view! {
        <Title text="Musicboxd"/>
        <Router>
            <main>
                <FlatRoutes fallback=|| "Page not found.".into_view()>
                    <Route path=StaticSegment("") view=HomePage/>
                    <Route path=(StaticSegment("album"), ParamSegment("id")) view=AlbumPage/>
                </FlatRoutes>
            </main>
        </Router>
    }
}

#[component]
fn HomePage() -> impl IntoView {
    let query_map = use_query_map();
    let navigate = use_navigate();

    // Query lives in the URL so back-navigation and refresh restore it.
    let url_q = move || query_map.read().get("q").unwrap_or_default();

    // Text box mirrors the URL query; kept in sync by the Effect below.
    let (input, set_input) = signal(url_q());
    Effect::new(move |_| set_input.set(url_q()));

    let current_user = Resource::new(|| (), |_| get_current_user());

    // Infinite scroll: page number is in-memory only — scrolling is a session
    // gesture, not something to encode in the URL.
    let (page, set_page) = signal(1u32);
    let (albums, set_albums) = signal(Vec::<SpotifyAlbum>::new());

    // When the query changes, reset accumulated results and restart from page 1.
    Effect::new(move |prev_q: Option<String>| {
        let q = url_q();
        if prev_q.as_deref() != Some(&q) {
            set_page.set(1);
            set_albums.set(vec![]);
        }
        q
    });

    let page_result = Resource::new(
        move || (url_q(), page.get()),
        |(q, p)| async move {
            if q.trim().is_empty() {
                return Ok(SearchPage { albums: vec![], total: 0 });
            }
            search_music(q, p).await
        },
    );

    // Accumulate pages: replace on page 1 (fresh query), append on later pages.
    Effect::new(move |_| {
        let Some(Ok(result)) = page_result.get() else { return };
        let fetched = result.albums;
        if page.get_untracked() == 1 {
            set_albums.set(fetched);
        } else {
            set_albums.update(|a| a.extend(fetched));
        }
    });


    view! {
        <header class="site-header">
            <span class="logo">"Musicboxd"</span>
            <div class="header-auth">
                <Suspense fallback=|| ()>
                    {move || current_user.get().map(|res| {
                        match res {
                            Ok(Some(username)) => view! {
                                <span class="auth-user">{username}</span>
                                <a class="auth-link" rel="external" href="/auth/logout">"Sign out"</a>
                            }.into_any(),
                            _ => view! {
                                <a class="auth-link oauth-btn" rel="external" href="/auth/google">
                                    <img class="oauth-icon" src="/google-icon.svg" alt="" width="14" height="14"/>
                                    "Sign in with Google"
                                </a>
                                <a class="auth-link oauth-btn" rel="external" href="/auth/github">
                                    <img class="oauth-icon" src="/github-icon.svg" alt="" width="14" height="14"/>
                                    "Sign in with GitHub"
                                </a>
                            }.into_any(),
                        }
                    })}
                </Suspense>
            </div>
        </header>
        <form class="search-form" on:submit=move |ev| {
            ev.prevent_default();
            let q = input.get_untracked();
            let dest = if q.trim().is_empty() {
                "/".to_string()
            } else {
                format!("/?q={}", url_encode_query(&q))
            };
            navigate(&dest, Default::default());
        }>
            <input
                class="search-input"
                type="text"
                placeholder="Search for music..."
                prop:value=move || input.get()
                on:input=move |ev| set_input.set(event_target_value(&ev))
            />
            <button class="search-btn" type="submit">"Search"</button>
        </form>
        {move || {
            if url_q().trim().is_empty() {
                return None;
            }
            Some(view! {
                <ul class="results-list">
                    {move || albums.get().into_iter().map(|album| {
                        let cover_src = format!("/album-art/{}", album.spotify_id);
                        let href = format!("/album/{}", album.spotify_id);
                        let artists = album.artists.join(", ");
                        let year = album
                            .release_year
                            .map(|y| y.to_string())
                            .unwrap_or_else(|| "????".to_string());
                        view! {
                            <li class="result-card">
                                <A href=href attr:class="result-card-link">
                                    <img class="result-cover" src=cover_src alt="Album cover" width="72" height="72"/>
                                    <div class="result-info">
                                        <span class="result-title">{album.title}</span>
                                        <span class="result-artist">{artists}</span>
                                        <div class="result-meta">
                                            <span class="result-type">{album.album_type}</span>
                                            <span class="result-year">{year}</span>
                                        </div>
                                    </div>
                                </A>
                            </li>
                        }
                    }).collect_view()}
                </ul>
                <Suspense fallback=move || {
                    if albums.with(Vec::is_empty) {
                        view! { <p class="status-msg">"Searching..."</p> }.into_any()
                    } else {
                        view! { <p class="status-msg">"Loading..."</p> }.into_any()
                    }
                }>
                    {move || page_result.get().map(|res| match res {
                        Err(e) => view! {
                            <p class="status-msg">"Error: " {e.to_string()}</p>
                        }.into_any(),
                        Ok(r) if r.albums.is_empty() && page.get() == 1 => view! {
                            <p class="status-msg">"No results found."</p>
                        }.into_any(),
                        Ok(_) => view! {
                            <div class="load-more-bar">
                                <button class="load-more-btn"
                                    on:click=move |_| set_page.update(|p| *p += 1)>
                                    "Load more"
                                </button>
                            </div>
                        }.into_any(),
                    })}
                </Suspense>
            })
        }}
    }
}

#[component]
fn AlbumPage() -> impl IntoView {
    let params = use_params_map();
    let spotify_id = move || params.read().get("id").unwrap_or_default();

    let detail = Resource::new(spotify_id, |id| async move { get_album_detail(id).await });

    view! {
        <header class="site-header">
            <A href="/" attr:class="logo">"Musicboxd"</A>
        </header>
        <Suspense fallback=move || view! { <p class="status-msg">"Loading..."</p> }>
            {move || detail.get().map(|res| match res {
                Err(e) => view! {
                    <p class="status-msg">"Error: " {e.to_string()}</p>
                }.into_any(),
                Ok(d) => {
                    let cover_src = format!("/album-art/{}", d.album.spotify_id);
                    let artists = d.album.artists.join(", ");
                    let year = d.album.release_year.map(|y| y.to_string()).unwrap_or_else(|| "????".to_string());
                    view! {
                        <div class="album-detail">
                            <div class="album-header">
                                <img class="album-cover" src=cover_src alt="Album cover" width="200" height="200"/>
                                <div class="album-meta">
                                    <h1 class="album-title">{d.album.title}</h1>
                                    <p class="album-artists">{artists}</p>
                                    <p class="album-info">
                                        <span class="album-type">{d.album.album_type}</span>
                                        " · "
                                        <span class="album-year">{year}</span>
                                    </p>
                                </div>
                            </div>
                            <ul class="track-list">
                                {d.tracks.into_iter().map(|track| {
                                    let duration = format_duration(track.duration_ms);
                                    let track_artists = track.artists.join(", ");
                                    view! {
                                        <li class="track-row">
                                            <span class="track-num">{track.track_number}</span>
                                            <div class="track-info">
                                                <div class="track-name">{track.name}</div>
                                                {(!track_artists.is_empty()).then(|| view! {
                                                    <div class="track-artists">{track_artists}</div>
                                                })}
                                            </div>
                                            <span class="track-duration">{duration}</span>
                                        </li>
                                    }
                                }).collect_view()}
                            </ul>
                        </div>
                    }.into_any()
                }
            })}
        </Suspense>
    }
}

/// Encodes a search query for use in a URL query string.
/// Converts spaces to `+` and percent-encodes the characters that are
/// structurally significant in a URL (`%`, `&`, `#`, `+`).
fn url_encode_query(s: &str) -> String {
    s.replace('%', "%25")
        .replace('&', "%26")
        .replace('#', "%23")
        .replace('+', "%2B")
        .replace(' ', "+")
}

fn format_duration(ms: Option<u32>) -> String {
    let ms = match ms {
        Some(v) => v,
        None => return String::new(),
    };
    let total_secs = ms / 1000;
    format!("{}:{:02}", total_secs / 60, total_secs % 60)
}
