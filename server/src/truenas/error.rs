//! Typed errors of the TrueNAS client.

use std::fmt;

use super::rpc::RpcError;

#[derive(Debug)]
pub enum Error {
    /// Could not open the WebSocket (DNS, TCP, TLS, HTTP upgrade, bad URL).
    Connect(String),
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
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Connect(e) => write!(f, "cannot connect to the NAS: {e}"),
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
        }
    }
}

impl std::error::Error for Error {}
