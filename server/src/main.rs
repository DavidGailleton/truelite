//! truelite server: serves the web app and (later) bridges it to the TrueNAS API.

mod config;
mod login;
mod rate_limit;
mod sessions;
mod static_files;
// Calls and event subscriptions are used by the WebSocket bridge (#16).
#[allow(dead_code, unused_imports)]
mod truenas;

use std::net::SocketAddr;
use std::time::Duration;

use axum::extract::{DefaultBodyLimit, FromRef, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::login::Login;
use crate::rate_limit::LimitOptions;
use crate::sessions::{
    Authed, SessionStore, StoreOptions, clear_cookie, cookie_value, session_json,
};
use crate::truenas::{Endpoint, SessionOptions};

/// How long logout waits for the NAS to confirm.
const LOGOUT_TIMEOUT: Duration = Duration::from_secs(5);
/// Largest login request body; a user name, a password and some JSON.
const LOGIN_BODY_LIMIT: usize = 4096;

/// State shared by every handler.
#[derive(Clone)]
pub struct AppState {
    pub sessions: SessionStore,
    pub login: Login,
}

impl FromRef<AppState> for Login {
    fn from_ref(state: &AppState) -> Self {
        state.login.clone()
    }
}

impl FromRef<AppState> for SessionStore {
    fn from_ref(state: &AppState) -> Self {
        state.sessions.clone()
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("invalid configuration: {e}");
            std::process::exit(1);
        }
    };

    // The first connection (and so the first pin check) happens at the first login.
    let endpoint = match &config.nas {
        Some(nas) => {
            match nas.fingerprint {
                Some(pin) => tracing::info!(url = %nas.url, pin = %pin, "NAS configured"),
                None => {
                    tracing::warn!(url = %nas.url, "NAS configured over plain ws:// (dev only)");
                }
            }
            match Endpoint::new(nas.url.clone(), nas.fingerprint) {
                Ok(ep) => Some(ep),
                Err(e) => {
                    tracing::error!("invalid NAS configuration: {e}");
                    std::process::exit(1);
                }
            }
        }
        None => {
            tracing::warn!("no NAS configured (TRUELITE_NAS_URL): logins will fail");
            None
        }
    };

    let sessions = SessionStore::new(StoreOptions {
        idle: config.session_idle,
        max_age: config.session_max_age,
        ..StoreOptions::default()
    });
    sessions.spawn_sweeper();
    let login = Login::new(endpoint, SessionOptions::default(), LimitOptions::default());
    login.spawn_sweeper(sessions.options().sweep_every);
    let app = router(AppState {
        sessions: sessions.clone(),
        login,
    });
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .unwrap_or_else(|e| panic!("cannot bind {}: {e}", config.listen));
    tracing::info!("truelite listening on http://{}", config.listen);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .expect("server error");
    sessions.logout_all(LOGOUT_TIMEOUT).await;
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/login", post(login::password))
        .route("/api/login/otp", post(login::otp))
        .route_layer(DefaultBodyLimit::max(LOGIN_BODY_LIMIT))
        .route("/api/session", get(session_info))
        .route("/api/logout", post(logout))
        .fallback(static_files::serve)
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

/// Who is logged in, for the web app. 401 when nobody is.
async fn session_info(Authed { session, .. }: Authed) -> Json<serde_json::Value> {
    Json(session_json(&session))
}

/// Ends the session, here and on the NAS, and deletes the cookie. Succeeds without a session.
async fn logout(State(sessions): State<SessionStore>, headers: HeaderMap) -> impl IntoResponse {
    if let Some(session) = cookie_value(&headers).and_then(|id| sessions.remove(id))
        && tokio::time::timeout(LOGOUT_TIMEOUT, session.logout())
            .await
            .is_err()
    {
        tracing::warn!(user = %session.user().username, "NAS logout timed out");
    }
    (
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, clear_cookie())],
    )
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, Response, StatusCode};
    use tower::ServiceExt;

    use crate::truenas::mock::logged_in;

    fn test_state() -> AppState {
        AppState {
            sessions: SessionStore::new(StoreOptions::default()),
            login: Login::new(None, SessionOptions::default(), LimitOptions::default()),
        }
    }

    async fn send(state: &AppState, req: Request<Body>) -> Response<Body> {
        router(state.clone()).oneshot(req).await.unwrap()
    }

    fn with_cookie(method: &str, uri: &str, id: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::COOKIE, format!("{}={id}", sessions::COOKIE))
            .body(Body::empty())
            .unwrap()
    }

    fn set_cookie(res: &Response<Body>) -> Option<&str> {
        res.headers()
            .get(header::SET_COOKIE)
            .map(|v| v.to_str().unwrap())
    }

    #[tokio::test]
    async fn health_ok() {
        let res = router(test_state())
            .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 1024).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("\"ok\""));
    }

    #[tokio::test]
    async fn spa_fallback_serves_index() {
        let res = router(test_state())
            .oneshot(Request::get("/apps/some-app").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ct = res.headers()["content-type"].to_str().unwrap().to_owned();
        assert!(ct.starts_with("text/html"));
    }

    #[tokio::test]
    async fn session_needs_a_live_cookie() {
        let state = test_state();
        let res = send(
            &state,
            Request::get("/api/session").body(Body::empty()).unwrap(),
        )
        .await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(set_cookie(&res), None);

        let res = send(&state, with_cookie("GET", "/api/session", &"a".repeat(64))).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        assert!(set_cookie(&res).unwrap().ends_with("Max-Age=0"));
    }

    #[tokio::test]
    async fn session_then_logout() {
        let (mock, _nas, session) = logged_in(SessionOptions::default()).await;
        let state = test_state();
        let id = state.sessions.insert(session).unwrap();

        let res = send(&state, with_cookie("GET", "/api/session", id.as_str())).await;
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 4096).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["username"], "admin");
        assert_eq!(body["roles"], json!(["READONLY_ADMIN"]));
        assert_eq!(body["link"], "up");

        let res = send(&state, with_cookie("POST", "/api/logout", id.as_str())).await;
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        assert!(set_cookie(&res).unwrap().ends_with("Max-Age=0"));
        assert!(mock.calls().iter().any(|(_, m)| m == "auth.logout"));

        let res = send(&state, with_cookie("GET", "/api/session", id.as_str())).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        // Logging out twice, or without a session, is fine.
        let res = send(&state, with_cookie("POST", "/api/logout", id.as_str())).await;
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn unknown_api_paths_are_404() {
        let res = router(test_state())
            .oneshot(Request::get("/api/nope").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
