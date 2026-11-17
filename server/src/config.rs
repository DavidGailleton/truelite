//! Configuration from environment variables (see `.env.example`).

use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct Config {
    /// Address the HTTP server listens on. `TRUELITE_LISTEN`, default `0.0.0.0:8080`.
    pub listen: SocketAddr,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let listen = std::env::var("TRUELITE_LISTEN")
            .unwrap_or_else(|_| "0.0.0.0:8080".into())
            .parse()
            .map_err(|e| format!("TRUELITE_LISTEN: {e}"))?;
        Ok(Self { listen })
    }
}
