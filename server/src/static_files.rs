//! The built web app (`web/build`) embedded in the binary.
//! Unknown paths fall back to `index.html` so client-side routes work on reload.

use axum::{
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::Embed;
use std::sync::LazyLock;

#[derive(Embed)]
#[folder = "../web/build/"]
// In dev the web app is served by Vite, so web/build may not exist yet.
#[allow_missing = true]
struct Assets;

/// The `Content-Security-Policy` for this build: it lists the hashes of the inline scripts of
/// the embedded `index.html` (see [`crate::security`]). Computed once.
pub fn csp() -> &'static str {
    static CSP: LazyLock<String> = LazyLock::new(|| {
        let index = Assets::get("index.html");
        let html = index
            .as_ref()
            .and_then(|f| std::str::from_utf8(&f.data).ok());
        crate::security::csp(html)
    });
    &CSP
}

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    if let Some(res) = file(path) {
        return res;
    }
    file("index.html").unwrap_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            "web app not built: run `npm run build` in web/",
        )
            .into_response()
    })
}

fn file(path: &str) -> Option<Response> {
    let path = if path.is_empty() { "index.html" } else { path };
    let asset = Assets::get(path)?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    // Hashed build assets never change; everything else must be revalidated.
    let cache = if path.starts_with("_app/immutable/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    Some(
        (
            [
                (header::CONTENT_TYPE, mime.as_ref().to_owned()),
                (header::CACHE_CONTROL, cache.to_owned()),
            ],
            asset.data,
        )
            .into_response(),
    )
}
