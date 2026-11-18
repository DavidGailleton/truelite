//! truelite server: serves the web app and (later) bridges it to the TrueNAS API.

mod config;
mod static_files;
// Wired into login (#11) and the WebSocket bridge (#16); only tests use it for now.
#[allow(dead_code, unused_imports)]
mod truenas;

use std::net::SocketAddr;

use axum::{Json, Router, routing::get};
use serde_json::json;
use tracing_subscriber::EnvFilter;

use crate::config::Config;

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

    // Connecting (and so checking the pin) starts with login, #11.
    match &config.nas {
        Some(nas) => match nas.fingerprint {
            Some(pin) => tracing::info!(url = %nas.url, pin = %pin, "NAS configured"),
            None => tracing::warn!(url = %nas.url, "NAS configured over plain ws:// (dev only)"),
        },
        None => tracing::info!("no NAS configured (TRUELITE_NAS_URL)"),
    }

    let app = router();
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
}

pub fn router() -> Router {
    Router::new()
        .route("/api/health", get(health))
        .fallback(static_files::serve)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
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
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_ok() {
        let res = router()
            .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = to_bytes(res.into_body(), 1024).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("\"ok\""));
    }

    #[tokio::test]
    async fn spa_fallback_serves_index() {
        let res = router()
            .oneshot(Request::get("/apps/some-app").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ct = res.headers()["content-type"].to_str().unwrap().to_owned();
        assert!(ct.starts_with("text/html"));
    }
}
