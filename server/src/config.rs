//! Configuration from environment variables (see `.env.example`).

use std::net::SocketAddr;
use std::time::Duration;

use crate::truenas::{Fingerprint, Scheme, check_url};

#[derive(Debug, Clone)]
pub struct Config {
    /// Address the HTTP server listens on. `TRUELITE_LISTEN`, default `0.0.0.0:8080`.
    pub listen: SocketAddr,
    /// The NAS to talk to; `None` until `TRUELITE_NAS_URL` is set.
    pub nas: Option<NasConfig>,
    /// A session with no request and no open WebSocket for this long ends.
    /// `TRUELITE_SESSION_IDLE_MINUTES`, default 30.
    pub session_idle: Duration,
    /// A session ends this long after login whatever happens. `TRUELITE_SESSION_MAX_HOURS`,
    /// default 24.
    pub session_max_age: Duration,
    /// Origins accepted on `/ws` and the POSTs besides the request's own (`Host`), for a proxy
    /// that rewrites `Host`. `TRUELITE_ALLOWED_ORIGINS`, comma-separated, lowercase here.
    pub allowed_origins: Vec<String>,
}

/// Where the NAS is and which certificate to trust.
#[derive(Debug, Clone)]
pub struct NasConfig {
    /// `TRUELITE_NAS_URL`: `wss://host[:port]`, or `ws://` to a loopback host for development.
    pub url: String,
    /// `TRUELITE_NAS_FINGERPRINT`: SHA-256 of the NAS certificate. Set iff `url` is `wss://`.
    pub fingerprint: Option<Fingerprint>,
}

impl Config {
    /// # Errors
    ///
    /// A message naming the variable that is invalid.
    pub fn from_env() -> Result<Self, String> {
        Self::from_vars(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()))
    }

    /// Parses the configuration from `get`, which returns a variable's value if it is set.
    ///
    /// # Errors
    ///
    /// A message naming the variable that is invalid.
    pub fn from_vars(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let listen = get("TRUELITE_LISTEN")
            .unwrap_or_else(|| "0.0.0.0:8080".into())
            .parse()
            .map_err(|e| format!("TRUELITE_LISTEN: {e}"))?;
        let fingerprint = get("TRUELITE_NAS_FINGERPRINT")
            .map(|v| v.parse::<Fingerprint>())
            .transpose()
            .map_err(|e| format!("TRUELITE_NAS_FINGERPRINT: {e}"))?;
        let nas = match get("TRUELITE_NAS_URL") {
            None if fingerprint.is_some() => {
                return Err("TRUELITE_NAS_FINGERPRINT is set but TRUELITE_NAS_URL is not".into());
            }
            None => None,
            Some(url) => {
                let url = url.trim().trim_end_matches('/').to_owned();
                let scheme = check_url(&url).map_err(|e| format!("TRUELITE_NAS_URL: {e}"))?;
                match (scheme, fingerprint) {
                    (Scheme::Wss, None) => {
                        return Err(
                            "TRUELITE_NAS_FINGERPRINT is required with a wss:// TRUELITE_NAS_URL; \
                             get it with: openssl s_client -connect <nas>:443 </dev/null \
                             2>/dev/null | openssl x509 -noout -fingerprint -sha256"
                                .into(),
                        );
                    }
                    (Scheme::Ws, Some(_)) => {
                        return Err(
                            "TRUELITE_NAS_FINGERPRINT is set but TRUELITE_NAS_URL is plain ws://"
                                .into(),
                        );
                    }
                    (Scheme::Wss, Some(_)) | (Scheme::Ws, None) => {}
                }
                Some(NasConfig { url, fingerprint })
            }
        };
        let session_idle = positive(&get, "TRUELITE_SESSION_IDLE_MINUTES", 30)?;
        let session_max_age = positive(&get, "TRUELITE_SESSION_MAX_HOURS", 24)?;
        let allowed_origins = match get("TRUELITE_ALLOWED_ORIGINS") {
            None => Vec::new(),
            Some(v) => origins(&v)?,
        };
        Ok(Self {
            allowed_origins,
            listen,
            nas,
            session_idle: Duration::from_secs(session_idle * 60),
            session_max_age: Duration::from_secs(session_max_age * 3600),
        })
    }
}

/// A comma-separated list of origins like `https://nas.example.org` (no path, no trailing `/`).
fn origins(v: &str) -> Result<Vec<String>, String> {
    v.split(',')
        .map(str::trim)
        .filter(|o| !o.is_empty())
        .map(|o| {
            crate::origin::parse(o).map(|_| o.to_ascii_lowercase()).ok_or_else(|| {
                format!(
                    "TRUELITE_ALLOWED_ORIGINS: {o:?} is not an origin like https://nas.example.org \
                     (http or https, no path, no trailing /)"
                )
            })
        })
        .collect()
}

/// A whole number > 0 (at most ten years in hours, so durations cannot overflow).
fn positive(get: impl Fn(&str) -> Option<String>, var: &str, default: u64) -> Result<u64, String> {
    match get(var) {
        None => Ok(default),
        Some(v) => match v.trim().parse::<u64>() {
            Ok(n) if (1..=87_600).contains(&n) => Ok(n),
            _ => Err(format!(
                "{var}: expected a whole number from 1 to 87600, got {v:?}"
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FP: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn parse(vars: &[(&str, &str)]) -> Result<Config, String> {
        Config::from_vars(|k| {
            vars.iter()
                .find(|(name, _)| *name == k)
                .map(|(_, v)| (*v).to_owned())
        })
    }

    #[test]
    fn no_nas_variables_means_no_nas() {
        let c = parse(&[]).unwrap();
        assert!(c.nas.is_none());
        assert_eq!(c.listen.port(), 8080);
        assert_eq!(c.session_idle, Duration::from_secs(30 * 60));
        assert_eq!(c.session_max_age, Duration::from_secs(24 * 3600));
    }

    #[test]
    fn session_lifetimes_are_whole_positive_numbers() {
        let c = parse(&[
            ("TRUELITE_SESSION_IDLE_MINUTES", "5"),
            ("TRUELITE_SESSION_MAX_HOURS", " 2 "),
        ])
        .unwrap();
        assert_eq!(c.session_idle, Duration::from_secs(300));
        assert_eq!(c.session_max_age, Duration::from_secs(7200));
        for bad in ["0", "-1", "1.5", "ten", "87601"] {
            let err = parse(&[("TRUELITE_SESSION_IDLE_MINUTES", bad)]).unwrap_err();
            assert!(err.starts_with("TRUELITE_SESSION_IDLE_MINUTES"), "{err}");
        }
    }

    #[test]
    fn allowed_origins_are_validated() {
        assert!(parse(&[]).unwrap().allowed_origins.is_empty());
        let c = parse(&[(
            "TRUELITE_ALLOWED_ORIGINS",
            " https://NAS.example.org, http://10.0.0.2:8080 ,",
        )])
        .unwrap();
        assert_eq!(
            c.allowed_origins,
            ["https://nas.example.org", "http://10.0.0.2:8080"]
        );
        for bad in [
            "nas.example.org",
            "https://nas.example.org/",
            "null",
            "ftp://x",
        ] {
            let err = parse(&[("TRUELITE_ALLOWED_ORIGINS", bad)]).unwrap_err();
            assert!(err.starts_with("TRUELITE_ALLOWED_ORIGINS"), "{err}");
        }
    }

    #[test]
    fn wss_with_fingerprint_is_parsed() {
        let c = parse(&[
            ("TRUELITE_NAS_URL", "wss://nas.lan/"),
            ("TRUELITE_NAS_FINGERPRINT", FP),
        ])
        .unwrap();
        let nas = c.nas.unwrap();
        assert_eq!(nas.url, "wss://nas.lan");
        assert_eq!(nas.fingerprint, Some(FP.parse().unwrap()));
    }

    #[test]
    fn wss_without_fingerprint_is_an_error() {
        let err = parse(&[("TRUELITE_NAS_URL", "wss://nas.lan")]).unwrap_err();
        assert!(
            err.contains("TRUELITE_NAS_FINGERPRINT is required"),
            "{err}"
        );
    }

    #[test]
    fn plain_ws_is_only_for_loopback_and_never_with_a_fingerprint() {
        assert!(parse(&[("TRUELITE_NAS_URL", "ws://127.0.0.1:9000")]).is_ok());
        let err = parse(&[("TRUELITE_NAS_URL", "ws://nas.lan")]).unwrap_err();
        assert!(err.starts_with("TRUELITE_NAS_URL"), "{err}");
        assert!(
            parse(&[
                ("TRUELITE_NAS_URL", "ws://127.0.0.1:9000"),
                ("TRUELITE_NAS_FINGERPRINT", FP)
            ])
            .is_err()
        );
    }

    #[test]
    fn bad_fingerprint_or_fingerprint_alone_is_an_error() {
        let err = parse(&[
            ("TRUELITE_NAS_URL", "wss://nas.lan"),
            ("TRUELITE_NAS_FINGERPRINT", "abc"),
        ])
        .unwrap_err();
        assert!(err.starts_with("TRUELITE_NAS_FINGERPRINT"), "{err}");
        assert!(parse(&[("TRUELITE_NAS_FINGERPRINT", FP)]).is_err());
    }
}
