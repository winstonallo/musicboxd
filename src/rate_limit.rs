/// In-memory per-IP rate limiter backed by a `Mutex<HashMap>`.
///
/// This module exists because `tower_governor` requires
/// `into_make_service_with_connect_info` and a typestate-heavy API that does
/// not compose cleanly with the existing router structure. The manual approach
/// gives the same per-IP sliding-window semantics without adding a large
/// dependency or significantly restructuring the server entry point.
#[cfg(feature = "ssr")]
pub mod server {
    use axum::{
        body::Body,
        extract::ConnectInfo,
        http::Request,
        http::StatusCode,
        middleware::Next,
        response::{IntoResponse, Response},
    };
    use std::{
        collections::HashMap,
        net::{IpAddr, SocketAddr},
        sync::{Arc, Mutex},
        time::Instant,
    };

    /// Shared state for one rate-limit policy: `max_requests` per `window_secs` seconds.
    pub struct RateLimitStore {
        max_requests: u32,
        window_secs: u64,
        map: Mutex<HashMap<IpAddr, (u32, Instant)>>,
    }

    impl RateLimitStore {
        pub fn new(max_requests: u32, window_secs: u64) -> Self {
            Self {
                max_requests,
                window_secs,
                map: Mutex::new(HashMap::new()),
            }
        }

        /// Returns `true` if the request from `ip` is within the allowed rate.
        pub fn check_and_increment(&self, ip: IpAddr) -> bool {
            let mut map = self.map.lock().expect("rate limit mutex poisoned");
            let now = Instant::now();
            let window = std::time::Duration::from_secs(self.window_secs);

            let entry = map.entry(ip).or_insert((0, now));
            if now.duration_since(entry.1) >= window {
                // Window has elapsed; reset the counter for this IP.
                *entry = (1, now);
                true
            } else if entry.0 < self.max_requests {
                entry.0 += 1;
                true
            } else {
                false
            }
        }
    }

    /// Axum `from_fn_with_state` middleware that enforces per-IP rate limiting.
    pub async fn rate_limit_fn(
        axum::extract::State(store): axum::extract::State<Arc<RateLimitStore>>,
        ConnectInfo(addr): ConnectInfo<SocketAddr>,
        request: Request<Body>,
        next: Next,
    ) -> Response {
        if store.check_and_increment(addr.ip()) {
            next.run(request).await
        } else {
            (StatusCode::TOO_MANY_REQUESTS, "Too Many Requests").into_response()
        }
    }
}
