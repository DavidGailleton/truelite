//! TrueNAS middleware API client. This module is the only place that knows the TrueNAS API:
//! endpoint, JSON-RPC wire format, error shapes, authentication and events.
//! Login uses `auth.login_ex`, reconnects use `auth.generate_token` tokens; the SCRAM auth of
//! TrueNAS 27 must also stay inside this module.
//!
//! Reference: <https://api.truenas.com/v25.10/> (JSON-RPC 2.0 over WebSocket).

pub mod alerts;
pub mod apps;
mod auth;
mod client;
pub mod datasets;
pub mod disks;
mod error;
mod events;
pub mod jobs;
#[cfg(test)]
pub(crate) mod mock;
pub mod pools;
pub mod realtime;
mod reshape;
mod rpc;
mod session;
pub mod snapshots;
pub mod system;
mod tls;
pub mod update;

// Only what the rest of the server uses; the other public types stay reachable through the
// methods that return them.
pub use auth::{LoginStep, PendingOtp, Secret, login};
pub use client::{Scheme, check_url};
pub use error::{AuthFailure, Error};
pub use events::{Event, Subscription, TopicOptions, Update, UpdateKind};
#[cfg(test)]
pub use session::EndReason;
pub use session::{Endpoint, Link, Session, SessionOptions};
pub use tls::Fingerprint;

/// Pinned API version. The NAS (25.10.8) serves `v25.10.0` … `v25.10.5`; the vault reference
/// documents v25.10.5, so the wire format here matches it exactly. CI reads this line to tag the
/// image and title the release (`truenasX.Y`), keep it a plain literal.
pub const API_VERSION: &str = "25.10.5";

/// WebSocket path of [`API_VERSION`].
pub const API_PATH: &str = "/api/v25.10.5";

#[cfg(test)]
mod tests {
    #[test]
    fn api_path_matches_version() {
        assert_eq!(super::API_PATH, format!("/api/v{}", super::API_VERSION));
    }
}
