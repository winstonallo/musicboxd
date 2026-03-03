use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::{FlatRoutes, Route, Router},
    StaticSegment,
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

#[server]
pub async fn get_current_user() -> Result<Option<String>, ServerFnError> {
    use crate::auth::server::CurrentUser;
    use axum::Extension;
    let Extension(user): Extension<Option<CurrentUser>> = leptos_axum::extract().await?;
    Ok(user.map(|u| u.username))
}

#[server]
pub async fn search_music(query: String) -> Result<Vec<SpotifyAlbum>, ServerFnError> {
    use crate::spotify::SpotifyClient;
    use axum::Extension;
    use sqlx::SqlitePool;
    let Extension(pool): Extension<SqlitePool> = leptos_axum::extract().await?;
    let Extension(spotify): Extension<SpotifyClient> = leptos_axum::extract().await?;
    spotify.search(&pool, &query).await
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
                </FlatRoutes>
            </main>
        </Router>
    }
}

#[component]
fn HomePage() -> impl IntoView {
    let (input, set_input) = signal(String::new());
    let (query, set_query) = signal(String::new());

    let current_user = Resource::new(|| (), |_| get_current_user());

    let results = Resource::new(
        move || query.get(),
        |q| async move {
            if q.trim().is_empty() {
                Ok(vec![])
            } else {
                search_music(q).await
            }
        },
    );

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
                                <a class="auth-link" rel="external" href="/auth/google">"Sign in with Google"</a>
                                <a class="auth-link" rel="external" href="/auth/github">"Sign in with GitHub"</a>
                            }.into_any(),
                        }
                    })}
                </Suspense>
            </div>
        </header>
        <form class="search-form" on:submit=move |ev| {
            ev.prevent_default();
            set_query.set(input.get_untracked());
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
        <Suspense fallback=move || view! { <p class="status-msg">"Searching..."</p> }>
            {move || {
                if query.get().trim().is_empty() {
                    return None;
                }
                results.get().map(|res| {
                    match res {
                        Ok(albums) if albums.is_empty() => {
                            view! { <p class="status-msg">"No results found."</p> }.into_any()
                        }
                        Ok(albums) => {
                            view! {
                                <ul class="results-list">
                                    {albums.into_iter().map(|album| {
                                                        let cover_src = format!("/album-art/{}", album.spotify_id);
                                        let artists = album.artists.join(", ");
                                        let year = album
                                            .release_year
                                            .map(|y| y.to_string())
                                            .unwrap_or_else(|| "????".to_string());
                                        view! {
                                            <li class="result-card">
                                                <img class="result-cover" src=cover_src alt="Album cover" width="72" height="72"/>
                                                <div class="result-info">
                                                    <span class="result-title">{album.title}</span>
                                                    <span class="result-artist">{artists}</span>
                                                    <div class="result-meta">
                                                        <span class="result-type">{album.album_type}</span>
                                                        <span class="result-year">{year}</span>
                                                    </div>
                                                </div>
                                            </li>
                                        }
                                    }).collect_view()}
                                </ul>
                            }.into_any()
                        }
                        Err(e) => {
                            view! { <p class="status-msg">"Error: " {e.to_string()}</p> }.into_any()
                        }
                    }
                })
            }}
        </Suspense>
    }
}
