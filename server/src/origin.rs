//! Same-origin guard for `/ws` and the state-changing POSTs.
//!
//! `SameSite=Strict` keeps the session cookie away from other *sites*, but a page of the same
//! site on another origin (another app on the NAS's address and a different port, another
//! `*.ts.net` host) still sends it. Browsers always send `Origin` on a WebSocket handshake and
//! on a POST, so [`SameOrigin`] refuses any request whose `Origin` is not the address the
//! request was sent to, or one listed in `TRUELITE_ALLOWED_ORIGINS`.

use std::sync::Arc;

use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

/// Extra origins accepted even when they do not match `Host` (a proxy that rewrites it).
/// Stored lowercase, as `scheme://authority`.
#[derive(Debug, Clone, Default)]
pub struct Origins(Arc<[String]>);

impl Origins {
    pub fn new(list: Vec<String>) -> Self {
        Self(list.into())
    }
}

/// Proof that the request came from the app's own origin. Put it first in a handler's
/// arguments, so a foreign page is refused before anything else runs.
#[derive(Debug, Clone, Copy)]
pub struct SameOrigin;

/// Why [`SameOrigin`] refused a request.
#[derive(Debug)]
pub struct ForeignOrigin;

impl IntoResponse for ForeignOrigin {
    fn into_response(self) -> Response {
        (
            StatusCode::FORBIDDEN,
            "this page is not allowed to talk to truelite",
        )
            .into_response()
    }
}

impl<S> FromRequestParts<S> for SameOrigin
where
    S: Send + Sync,
    Origins: FromRef<S>,
{
    type Rejection = ForeignOrigin;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let origin = parts
            .headers
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok());
        let host = parts
            .headers
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .or_else(|| parts.uri.authority().map(|a| a.as_str()));
        let Origins(extra) = Origins::from_ref(state);
        match origin {
            Some(o) if allowed(o, host, &extra) => Ok(SameOrigin),
            _ => {
                tracing::warn!(
                    origin = origin.unwrap_or("-"),
                    host = host.unwrap_or("-"),
                    path = parts.uri.path(),
                    "refused: foreign origin"
                );
                Err(ForeignOrigin)
            }
        }
    }
}

/// Whether `origin` may talk to the server reached as `host`. Only the authority is compared:
/// behind a TLS proxy or Tailscale serve the server cannot see the outer scheme.
pub fn allowed(origin: &str, host: Option<&str>, extra: &[String]) -> bool {
    let Some(authority) = parse(origin) else {
        return false;
    };
    // `config` stores the list lowercase, but `extra` may come from elsewhere (tests): compare
    // ignoring case rather than rely on it.
    if extra.iter().any(|e| e.eq_ignore_ascii_case(origin)) {
        return true;
    }
    host.is_some_and(|h| !h.is_empty() && h.eq_ignore_ascii_case(authority))
}

/// The authority of a serialized origin (`http(s)://authority`, nothing after it), or `None`
/// for anything else (`null`, a path, an empty host).
pub fn parse(origin: &str) -> Option<&str> {
    let rest =
        strip_prefix_ci(origin, "https://").or_else(|| strip_prefix_ci(origin, "http://"))?;
    let bad = |c: char| matches!(c, '/' | '?' | '#' | '@' | ' ') || c.is_ascii_control();
    (!rest.is_empty() && !rest.contains(bad)).then_some(rest)
}

/// `s` without `prefix`, compared ignoring ASCII case; `None` if `s` does not start with it.
fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    s.get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .map(|_| &s[prefix.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_authority_is_allowed() {
        assert!(allowed(
            "http://127.0.0.1:8080",
            Some("127.0.0.1:8080"),
            &[]
        ));
        assert!(allowed(
            "https://NAS.tail.ts.net",
            Some("nas.tail.ts.net"),
            &[]
        ));
        // Behind a TLS proxy the server only sees plain HTTP: the scheme is not compared.
        assert!(allowed("https://nas.lan", Some("nas.lan"), &[]));
    }

    #[test]
    fn other_hosts_and_ports_are_refused() {
        assert!(!allowed(
            "http://127.0.0.1:8081",
            Some("127.0.0.1:8080"),
            &[]
        ));
        assert!(!allowed("http://evil.example", Some("nas.lan"), &[]));
        assert!(!allowed(
            "http://nas.lan.evil.example",
            Some("nas.lan"),
            &[]
        ));
        assert!(!allowed("http://nas.lan", None, &[]));
        assert!(!allowed("http://nas.lan", Some(""), &[]));
    }

    #[test]
    fn malformed_origins_are_refused() {
        for o in [
            "",
            "null",
            "nas.lan",
            "ftp://nas.lan",
            "http://",
            "http://nas.lan/",
            "http://nas.lan/x",
            "http://user@nas.lan",
            "http://nas.lan?x",
            "http",
        ] {
            assert!(!allowed(o, Some("nas.lan"), &[]), "{o}");
            assert!(!allowed(o, Some(o), &[o.to_owned()]), "{o}");
        }
    }

    #[test]
    fn listed_origins_are_allowed_whatever_the_host() {
        let extra = vec!["https://nas.example.org".to_owned()];
        assert!(allowed(
            "https://nas.example.org",
            Some("10.0.0.2:8080"),
            &extra
        ));
        assert!(allowed("https://NAS.example.org", None, &extra));
        // The scheme matters for a listed origin.
        assert!(!allowed(
            "http://nas.example.org",
            Some("10.0.0.2:8080"),
            &extra
        ));
    }
}
