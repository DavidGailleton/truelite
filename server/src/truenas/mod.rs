//! TrueNAS middleware API client. This module is the only place that knows the TrueNAS API:
//! endpoint, JSON-RPC wire format, error shapes and (later) authentication and events.
//!
//! Reference: <https://api.truenas.com/v25.10/> (JSON-RPC 2.0 over WebSocket).

mod client;
mod error;
#[cfg(test)]
mod mock;
mod rpc;
mod tls;

pub use client::{Client, Notification, Options, Scheme, check_url};
pub use error::Error;
pub use rpc::{ErrorData, RpcError};
pub use tls::{Fingerprint, FingerprintParseError, client_config};

/// Pinned API version. The NAS (25.10.8) serves `v25.10.0` … `v25.10.5`; the vault reference
/// documents v25.10.5, so the wire format here matches it exactly.
pub const API_PATH: &str = "/api/v25.10.5";
