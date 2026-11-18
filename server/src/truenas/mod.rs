//! TrueNAS middleware API client. This module is the only place that knows the TrueNAS API:
//! endpoint, JSON-RPC wire format, error shapes, authentication and events.
//! Login uses `auth.login_ex`, reconnects use `auth.generate_token` tokens; the SCRAM auth of
//! TrueNAS 27 must also stay inside this module.
//!
//! Reference: <https://api.truenas.com/v25.10/> (JSON-RPC 2.0 over WebSocket).

pub mod apps;
mod auth;
mod client;
mod error;
mod events;
#[cfg(test)]
pub(crate) mod mock;
pub mod pools;
pub mod realtime;
mod reshape;
mod rpc;
mod session;
mod tls;

// Only what the rest of the server uses; the other public types stay reachable through the
// methods that return them.
pub use auth::{LoginStep, PendingOtp, Secret, login};
pub use client::{Scheme, check_url};
pub use error::{AuthFailure, Error};
pub use events::{Event, Update, UpdateKind};
#[cfg(test)]
pub use session::EndReason;
pub use session::{Endpoint, Link, Session, SessionOptions};
pub use tls::Fingerprint;

/// Pinned API version. The NAS (25.10.8) serves `v25.10.0` … `v25.10.5`; the vault reference
/// documents v25.10.5, so the wire format here matches it exactly.
pub const API_PATH: &str = "/api/v25.10.5";
