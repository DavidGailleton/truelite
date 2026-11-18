//! The built web app (`web/build`) embedded in the binary.
//! Unknown paths fall back to `index.html` so client-side routes work on reload.
//! Files are served precompressed (the build's `.br` and `.gz`) with an `ETag`, so a
//! revalidation costs a 304 instead of a full download.

use axum::{
    http::{HeaderMap, HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::Embed;
use std::{fmt::Write, sync::LazyLock};

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

/// The router fallback: an embedded file, `index.html` for unknown paths, 404 under `/api/`.
pub async fn serve(uri: Uri, headers: HeaderMap) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    if let Some(res) = file(path, &headers) {
        return res;
    }
    file("index.html", &headers).unwrap_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            "web app not built: run `npm run build` in web/",
        )
            .into_response()
    })
}

/// A precompressed variant the build writes next to each file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Encoding {
    Br,
    Gzip,
}

impl Encoding {
    fn suffix(self) -> &'static str {
        match self {
            Self::Br => ".br",
            Self::Gzip => ".gz",
        }
    }

    fn header(self) -> HeaderValue {
        HeaderValue::from_static(match self {
            Self::Br => "br",
            Self::Gzip => "gzip",
        })
    }
}

/// One embedded file, in the best encoding the client accepts, or a 304 when its cached copy
/// is still current. `None` when there is no such file.
fn file(path: &str, headers: &HeaderMap) -> Option<Response> {
    let path = if path.is_empty() { "index.html" } else { path };
    let plain = Assets::get(path)?;
    let accept = headers
        .get(header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok());
    let (asset, encoding) = encodings(accept)
        .iter()
        .find_map(|&e| Assets::get(&format!("{path}{}", e.suffix())).map(|a| (a, Some(e))))
        .unwrap_or((plain, None));

    // The hash of the bytes sent, so each encoding has its own tag.
    let mut tag = String::with_capacity(66);
    tag.push('"');
    for b in asset.metadata.sha256_hash() {
        let _ = write!(tag, "{b:02x}");
    }
    tag.push('"');
    let fresh = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|list| etag_matches(list, &tag));
    let etag = HeaderValue::try_from(tag).ok()?;
    // Hashed build assets never change; everything else must be revalidated.
    let cache = HeaderValue::from_static(if path.starts_with("_app/immutable/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    });
    let vary = HeaderValue::from_static("accept-encoding");

    if fresh {
        let headers = [
            (header::ETAG, etag),
            (header::CACHE_CONTROL, cache),
            (header::VARY, vary),
        ];
        return Some((StatusCode::NOT_MODIFIED, headers).into_response());
    }

    let mime = mime_guess::from_path(path)
        .first_raw()
        .unwrap_or("application/octet-stream");
    let mut res = (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(mime)),
            (header::CACHE_CONTROL, cache),
            (header::VARY, vary),
            (header::ETAG, etag),
        ],
        asset.data,
    )
        .into_response();
    if let Some(e) = encoding {
        res.headers_mut()
            .insert(header::CONTENT_ENCODING, e.header());
    }
    Some(res)
}

/// The precompressed variants the client accepts, best first (`br`, then `gzip`), from its
/// `Accept-Encoding` header. A coding with `q=0` is refused.
fn encodings(accept: Option<&str>) -> &'static [Encoding] {
    let (mut br, mut gzip) = (false, false);
    for token in accept.unwrap_or_default().split(',') {
        let mut parts = token.split(';');
        let name = parts.next().unwrap_or_default().trim();
        if parts.any(is_q_zero) {
            continue;
        }
        if name.eq_ignore_ascii_case("br") {
            br = true;
        } else if name.eq_ignore_ascii_case("gzip") {
            gzip = true;
        }
    }
    match (br, gzip) {
        (true, true) => &[Encoding::Br, Encoding::Gzip],
        (true, false) => &[Encoding::Br],
        (false, true) => &[Encoding::Gzip],
        (false, false) => &[],
    }
}

/// Whether a parameter is `q=0` (also `q=0.0`, `q=0.000`).
fn is_q_zero(param: &str) -> bool {
    let Some((key, value)) = param.split_once('=') else {
        return false;
    };
    key.trim().eq_ignore_ascii_case("q")
        && value.trim().strip_prefix('0').is_some_and(|rest| {
            rest.is_empty()
                || rest
                    .strip_prefix('.')
                    .is_some_and(|d| d.bytes().all(|b| b == b'0'))
        })
}

/// Whether an `If-None-Match` list (`"a", W/"b"`, or `*`) names `etag`.
fn etag_matches(if_none_match: &str, etag: &str) -> bool {
    if_none_match
        .split(',')
        .map(str::trim)
        .any(|tag| tag == "*" || tag.strip_prefix("W/").unwrap_or(tag) == etag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Encoding::{Br, Gzip};

    #[test]
    fn encodings_prefer_br_then_gzip() {
        assert_eq!(encodings(None), &[]);
        assert_eq!(encodings(Some("")), &[]);
        assert_eq!(encodings(Some("gzip, deflate, br, zstd")), &[Br, Gzip]);
        assert_eq!(encodings(Some("GZIP")), &[Gzip]);
        assert_eq!(encodings(Some(" br;q=0.5 ")), &[Br]);
        assert_eq!(encodings(Some("identity, *")), &[]);
    }

    #[test]
    fn encodings_skip_q_zero() {
        assert_eq!(encodings(Some("br;q=0, gzip")), &[Gzip]);
        assert_eq!(encodings(Some("br; q=0.0, gzip;Q=0.000")), &[]);
        assert_eq!(encodings(Some("br;q=0.01")), &[Br]);
        assert_eq!(encodings(Some("gzip;q=1.0")), &[Gzip]);
    }

    #[test]
    fn if_none_match_lists() {
        let tag = "\"ab12\"";
        assert!(etag_matches("\"ab12\"", tag));
        assert!(etag_matches("\"x\", W/\"ab12\"", tag));
        assert!(etag_matches("*", tag));
        assert!(!etag_matches("\"x\", \"ab1\"", tag));
        assert!(!etag_matches("", tag));
    }
}
