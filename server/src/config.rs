//! Configuration from environment variables (see `.env.example`).

use std::net::SocketAddr;

use crate::truenas::{Fingerprint, Scheme, check_url};

#[derive(Debug, Clone)]
pub struct Config {
    /// Address the HTTP server listens on. `TRUELITE_LISTEN`, default `0.0.0.0:8080`.
    pub listen: SocketAddr,
    /// The NAS to talk to; `None` until `TRUELITE_NAS_URL` is set.
    pub nas: Option<NasConfig>,
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
        Ok(Self { listen, nas })
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
