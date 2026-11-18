//! Typed errors of the TrueNAS client.

use std::fmt;

use super::rpc::RpcError;
use super::tls::Fingerprint;

#[derive(Debug)]
pub enum Error {
    /// Could not open the WebSocket (DNS, TCP, TLS, HTTP upgrade, bad URL). `what` says what
    /// failed (the URL, `TLS setup`, a rule the URL breaks); `source` is the underlying error,
    /// if any.
    Connect {
        what: String,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    /// The NAS presented a certificate other than the pinned one (`TRUELITE_NAS_FINGERPRINT`).
    CertMismatch { got: Fingerprint },
    /// The connection is closed or dropped before the reply arrived.
    Closed,
    /// No reply within the call timeout.
    Timeout,
    /// `-32000`: too many concurrent calls on this connection; back off and retry.
    TooManyCalls,
    /// The method failed on the NAS.
    Rpc(Box<RpcError>),
    /// We sent or received something that breaks the protocol.
    Protocol(String),
    /// The result did not match the expected type.
    Decode(serde_json::Error),
    /// The NAS refused the login.
    Auth(AuthFailure),
    /// The session has ended (logout, or the NAS no longer accepts it): log in again.
    LoggedOut,
}

/// Why `auth.login_ex` refused a login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthFailure {
    /// Unknown user or wrong password (`AUTH_ERR`), or too many wrong one-time codes.
    BadCredentials,
    /// The password or account has expired (`EXPIRED`).
    Expired,
    /// The login must be completed on another server (`REDIRECT`); not supported.
    Redirect(Vec<String>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Connect { what, source } => {
                write!(f, "cannot connect to the NAS: {what}")?;
                match source {
                    Some(e) => write!(f, ": {e}"),
                    None => Ok(()),
                }
            }
            Error::CertMismatch { got } => write!(
                f,
                "the NAS certificate does not match TRUELITE_NAS_FINGERPRINT (got {got}); \
                 if the NAS certificate was replaced on purpose, set the new value"
            ),
            Error::Closed => f.write_str("connection to the NAS closed"),
            Error::Timeout => f.write_str("the NAS did not answer in time"),
            Error::TooManyCalls => f.write_str("too many concurrent calls to the NAS"),
            Error::Rpc(e) => {
                let data = e.data.as_ref();
                match (
                    data.and_then(|d| d.errname.as_deref()),
                    data.and_then(|d| d.reason.as_deref()),
                ) {
                    (Some(name), Some(reason)) => write!(f, "{name}: {reason}"),
                    (None, Some(reason)) => f.write_str(reason),
                    _ => match &e.message {
                        Some(m) => write!(f, "{m} ({})", e.code),
                        None => write!(f, "NAS error {}", e.code),
                    },
                }
            }
            Error::Protocol(e) => write!(f, "protocol error: {e}"),
            Error::Decode(e) => write!(f, "unexpected result from the NAS: {e}"),
            Error::Auth(AuthFailure::BadCredentials) => f.write_str("wrong user name or password"),
            Error::Auth(AuthFailure::Expired) => {
                f.write_str("this password or account has expired; change it on the NAS")
            }
            Error::Auth(AuthFailure::Redirect(urls)) => write!(
                f,
                "this NAS asks to log in on another server ({}), which truelite does not support",
                urls.join(", ")
            ),
            Error::LoggedOut => f.write_str("logged out of the NAS; log in again"),
        }
    }
}

impl Error {
    /// [`Error::Connect`] caused by `source`.
    pub(super) fn connect(
        what: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Error::Connect {
            what: what.into(),
            source: Some(Box::new(source)),
        }
    }

    /// [`Error::Connect`] with no underlying error (a bad URL, a timeout).
    pub(super) fn connect_msg(what: impl Into<String>) -> Self {
        Error::Connect {
            what: what.into(),
            source: None,
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Connect {
                source: Some(e), ..
            } => Some(e.as_ref()),
            Error::Decode(e) => Some(e),
            _ => None,
        }
    }
}
