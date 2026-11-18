//! Security headers on every response: a strict Content-Security-Policy and the usual
//! hardening headers. Built once at startup.
//!
//! The SvelteKit build puts one inline bootstrap `<script>` in `index.html`, whose text changes
//! with every build. Instead of `'unsafe-inline'`, its SHA-256 is computed from the embedded
//! page and listed in `script-src`.

use axum::Router;
use axum::http::{HeaderName, HeaderValue, header};
use ring::digest::{SHA256, digest};
use tower_http::set_header::SetResponseHeaderLayer;

/// Directives other than `script-src`.
const CSP_REST: &str = "style-src 'self'; img-src 'self' data:; font-src 'self'; \
    connect-src 'self'; manifest-src 'self'; worker-src 'self'; base-uri 'none'; \
    form-action 'self'; frame-ancestors 'none'";

/// The `Content-Security-Policy` value allowing scripts from this origin plus the inline
/// scripts of `html` (the embedded `index.html`, if any).
pub fn csp(html: Option<&str>) -> String {
    let mut script_src = String::from("'self'");
    for hash in html.map(inline_script_hashes).unwrap_or_default() {
        script_src.push_str(" 'sha256-");
        script_src.push_str(&hash);
        script_src.push('\'');
    }
    format!("default-src 'none'; script-src {script_src}; {CSP_REST}")
}

/// Base64 SHA-256 of the text of every `<script>` without a `src` attribute, as a browser
/// hashes it: the exact characters between the opening tag's `>` and `</script>`.
pub fn inline_script_hashes(html: &str) -> Vec<String> {
    let mut hashes = Vec::new();
    let mut rest = html;
    while let Some(start) = find_ci(rest, "<script") {
        let after = &rest[start + "<script".len()..];
        let Some(tag_end) = after.find('>') else {
            break;
        };
        let attrs = &after[..tag_end];
        let body = &after[tag_end + 1..];
        let Some(close) = find_ci(body, "</script") else {
            break;
        };
        if !has_src(attrs) {
            hashes.push(base64(digest(&SHA256, &body.as_bytes()[..close]).as_ref()));
        }
        rest = &body[close..];
    }
    hashes
}

/// Whether a tag's attribute text has a `src` attribute.
fn has_src(attrs: &str) -> bool {
    attrs.split(|c: char| c.is_ascii_whitespace()).any(|a| {
        a.split('=')
            .next()
            .is_some_and(|n| n.eq_ignore_ascii_case("src"))
    })
}

/// Byte offset of the first ASCII case-insensitive match of `needle` (ASCII) in `hay`.
fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    hay.as_bytes()
        .windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}

/// Standard base64 with padding (RFC 4648 §4).
pub fn base64(data: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ABC[(n >> (18 - 6 * i) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Adds the security headers to every response of `router` that does not set them itself.
///
/// # Panics
///
/// If `csp` is not a valid header value; [`csp`] only produces printable ASCII.
pub fn apply(router: Router, csp: &str) -> Router {
    let set =
        |name: HeaderName, value: HeaderValue| SetResponseHeaderLayer::if_not_present(name, value);
    router
        .layer(set(
            HeaderName::from_static("cross-origin-opener-policy"),
            HeaderValue::from_static("same-origin"),
        ))
        .layer(set(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(set(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(set(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(set(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_str(csp).expect("the CSP is a valid header value"),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc_4648() {
        for (input, want) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), want, "{input}");
        }
        assert_eq!(base64(&[0xfb, 0xff, 0xbf]), "+/+/");
    }

    #[test]
    fn hashes_inline_scripts_only_with_exact_text() {
        let html = "<head><script src=\"/a.js\"></script>\
            <SCRIPT type=module>alert(1)</SCRIPT></head>\
            <body><script>\n\t\tlet x = 1;\n\t</script><script src='/b.js' defer></script></body>";
        let hashes = inline_script_hashes(html);
        // printf 'alert(1)' | openssl dgst -sha256 -binary | base64
        assert_eq!(
            hashes,
            vec![
                "bhHHL3z2vDgxUt0W3dWQOrprscmda2Y5pLsLg4GF+pI=".to_owned(),
                base64(digest(&SHA256, b"\n\t\tlet x = 1;\n\t").as_ref()),
            ]
        );
        assert!(inline_script_hashes("<p>no scripts</p>").is_empty());
        assert!(inline_script_hashes("<script>unterminated").is_empty());
        // `data-src` is not `src`.
        assert_eq!(
            inline_script_hashes("<script data-src=x>1</script>").len(),
            1
        );
    }

    #[test]
    fn csp_is_strict() {
        let csp = csp(Some("<script>alert(1)</script>"));
        assert!(csp.starts_with("default-src 'none'; script-src 'self' 'sha256-bhHHL3z2"));
        assert!(csp.contains("frame-ancestors 'none'"));
        assert!(!csp.contains("unsafe"));
        assert!(super::csp(None).contains("script-src 'self';"));
    }
}
