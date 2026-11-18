//! Login handshake with `auth.login_ex` (password, then an optional one-time code) and the
//! secrets it handles. The password is used for one call and dropped; reconnects use a token
//! (see [`session`](super::session)).
//!
//! Checked on a 25.10.8 NAS: `privilege.roles` arrives as `{"$set": [...]}`, `authenticator` is
//! a top-level field of the response, and a token outlives the connection that made it.

use std::fmt;

use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::client::{Client, Notification};
use super::error::{AuthFailure, Error};
use super::session::{Endpoint, Session, SessionOptions};

/// A password or token. Never printed: `Debug` is redacted and there is no `Display`.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub(super) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// The logged-in TrueNAS user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserInfo {
    pub username: String,
    /// Logged in with a one-time code (`authenticator: LEVEL_2`).
    pub two_factor: bool,
    /// Every role the user holds, already expanded by TrueNAS (`FULL_ADMIN`, `POOL_READ`, …).
    pub roles: Vec<String>,
    /// May use a web UI (`privilege.webui_access`).
    pub webui_access: bool,
}

impl UserInfo {
    #[cfg(test)]
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }
}

/// Result of a login step.
#[must_use]
#[derive(Debug)]
pub enum LoginStep {
    Done(Session),
    /// The account uses two-factor authentication: ask for the code, then
    /// [`PendingOtp::submit`]. Also returned again after a wrong code.
    OtpRequired(PendingOtp),
}

/// A login waiting for its one-time code. Holds the half-authenticated connection; dropping it
/// abandons the login.
pub struct PendingOtp {
    endpoint: Endpoint,
    opts: SessionOptions,
    client: Client,
    notifs: mpsc::Receiver<Notification>,
    username: String,
}

impl fmt::Debug for PendingOtp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingOtp")
            .field("username", &self.username)
            .finish_non_exhaustive()
    }
}

impl PendingOtp {
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Sends the one-time code on the same connection (`auth.login_ex_continue`).
    ///
    /// # Errors
    ///
    /// [`Error::Auth`] when the NAS gives up (too many wrong codes), or a connection error.
    pub async fn submit(self, code: &str) -> Result<LoginStep, Error> {
        let res = self
            .client
            .call_as(
                "auth.login_ex_continue",
                json!([{"mechanism": "OTP_TOKEN", "otp_token": code}]),
            )
            .await?;
        finish(res, self).await
    }
}

/// Opens a connection to the NAS and logs in with a user name and password
/// (`auth.login_ex` `PASSWORD_PLAIN`). The password is not kept.
///
/// # Errors
///
/// [`Error::Auth`] when the NAS refuses the login, or a connection error
/// ([`Error::Connect`], [`Error::CertMismatch`], …).
pub async fn login(
    endpoint: &Endpoint,
    opts: SessionOptions,
    username: &str,
    password: Secret,
) -> Result<LoginStep, Error> {
    let (client, notifs) = endpoint.connect(&opts).await?;
    let res = client
        .call_as(
            "auth.login_ex",
            json!([{
                "mechanism": "PASSWORD_PLAIN",
                "username": username,
                "password": password.expose(),
            }]),
        )
        .await?;
    drop(password);
    let pending = PendingOtp {
        endpoint: endpoint.clone(),
        opts,
        client,
        notifs,
        username: username.to_owned(),
    };
    finish(res, pending).await
}

async fn finish(res: LoginResponse, pending: PendingOtp) -> Result<LoginStep, Error> {
    match res {
        LoginResponse::Success {
            user_info,
            authenticator,
        } => {
            let raw = user_info
                .ok_or_else(|| Error::Protocol("login succeeded without user_info".into()))?;
            let user = raw.into_user(authenticator.as_deref() == Some("LEVEL_2"));
            let PendingOtp {
                endpoint,
                opts,
                client,
                notifs,
                ..
            } = pending;
            let session = Session::start(endpoint, opts, client, notifs, user).await?;
            Ok(LoginStep::Done(session))
        }
        LoginResponse::OtpRequired {} => Ok(LoginStep::OtpRequired(pending)),
        other => Err(other.failure()),
    }
}

/// `TOKEN_PLAIN` login on a fresh connection, used to reconnect.
pub(super) async fn login_with_token(client: &Client, token: &Secret) -> Result<(), Error> {
    let res: LoginResponse = client
        .call_as(
            "auth.login_ex",
            json!([{"mechanism": "TOKEN_PLAIN", "token": token.expose()}]),
        )
        .await?;
    match res {
        LoginResponse::Success { .. } => Ok(()),
        other => Err(other.failure()),
    }
}

/// `auth.generate_token(ttl, attrs, match_origin, single_use)`: a reusable token bound to
/// truelite's address. `ttl` is whole seconds.
pub(super) async fn generate_token(client: &Client, ttl_secs: u64) -> Result<Secret, Error> {
    client
        .call_as::<String>("auth.generate_token", json!([ttl_secs, {}, true, false]))
        .await
        .map(Secret)
}

/// `auth.login_ex` / `auth.login_ex_continue` result.
#[derive(Deserialize)]
#[serde(tag = "response_type", rename_all = "SCREAMING_SNAKE_CASE")]
pub(super) enum LoginResponse {
    Success {
        #[serde(default)]
        user_info: Option<RawUserInfo>,
        #[serde(default)]
        authenticator: Option<String>,
    },
    OtpRequired {},
    AuthErr {},
    Expired {},
    Redirect {
        #[serde(default)]
        urls: Vec<String>,
    },
}

impl LoginResponse {
    fn failure(self) -> Error {
        match self {
            LoginResponse::Expired {} => Error::Auth(AuthFailure::Expired),
            LoginResponse::Redirect { urls } => Error::Auth(AuthFailure::Redirect(urls)),
            LoginResponse::Success { .. }
            | LoginResponse::OtpRequired {}
            | LoginResponse::AuthErr {} => Error::Auth(AuthFailure::BadCredentials),
        }
    }
}

#[derive(Deserialize)]
pub(super) struct RawUserInfo {
    pw_name: String,
    #[serde(default)]
    privilege: Option<RawPrivilege>,
}

#[derive(Deserialize)]
struct RawPrivilege {
    #[serde(default)]
    roles: Roles,
    #[serde(default)]
    webui_access: bool,
}

/// A Python set serialized by middlewared as `{"$set": [...]}`; a plain list is accepted too.
#[derive(Deserialize)]
#[serde(untagged)]
enum Roles {
    Set {
        #[serde(rename = "$set")]
        set: Vec<String>,
    },
    List(Vec<String>),
    Other(Value),
}

impl Default for Roles {
    fn default() -> Self {
        Roles::List(Vec::new())
    }
}

impl RawUserInfo {
    fn into_user(self, two_factor: bool) -> UserInfo {
        let (mut roles, webui_access) = match self.privilege {
            Some(p) => {
                let roles = match p.roles {
                    Roles::Set { set } | Roles::List(set) => set,
                    Roles::Other(v) => {
                        tracing::warn!("unexpected privilege.roles shape: {v}");
                        Vec::new()
                    }
                };
                (roles, p.webui_access)
            }
            None => (Vec::new(), false),
        };
        roles.sort_unstable();
        UserInfo {
            username: self.pw_name,
            two_factor,
            roles,
            webui_access,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_from_the_real_nas() {
        let res: LoginResponse = serde_json::from_value(json!({
            "response_type": "SUCCESS",
            "authenticator": "LEVEL_1",
            "user_info": {
                "pw_name": "truenas_admin", "pw_gecos": "", "pw_dir": "/home/truenas_admin",
                "pw_shell": "/usr/bin/zsh", "pw_uid": 950, "pw_gid": 950, "grouplist": [950],
                "sid": null, "source": "LOCAL", "local": true, "attributes": {},
                "two_factor_config": {}, "account_attributes": ["LOCAL"],
                "privilege": {
                    "allowlist": [{"method": "*", "resource": "*"}],
                    "roles": {"$set": ["READONLY_ADMIN", "FULL_ADMIN"]},
                    "web_shell": true, "webui_access": true
                }
            }
        }))
        .unwrap();
        let LoginResponse::Success {
            user_info,
            authenticator,
        } = res
        else {
            panic!("not a success")
        };
        let user = user_info
            .unwrap()
            .into_user(authenticator.as_deref() == Some("LEVEL_2"));
        assert_eq!(user.username, "truenas_admin");
        assert!(!user.two_factor);
        assert!(user.webui_access);
        assert_eq!(user.roles, ["FULL_ADMIN", "READONLY_ADMIN"]);
        assert!(user.has_role("FULL_ADMIN"));
    }

    #[test]
    fn roles_as_a_plain_list_or_missing() {
        let raw: RawUserInfo = serde_json::from_value(json!({
            "pw_name": "ro", "pw_uid": 3000,
            "privilege": {"roles": ["READONLY_ADMIN"], "webui_access": true}
        }))
        .unwrap();
        assert_eq!(raw.into_user(true).roles, ["READONLY_ADMIN"]);
        let raw: RawUserInfo =
            serde_json::from_value(json!({"pw_name": "x", "pw_uid": 1, "privilege": null}))
                .unwrap();
        let user = raw.into_user(false);
        assert!(user.roles.is_empty() && !user.webui_access);
    }

    #[test]
    fn failures_map_to_typed_errors() {
        let f = |v: Value| {
            serde_json::from_value::<LoginResponse>(v)
                .unwrap()
                .failure()
        };
        assert!(matches!(
            f(json!({"response_type": "AUTH_ERR"})),
            Error::Auth(AuthFailure::BadCredentials)
        ));
        assert!(matches!(
            f(json!({"response_type": "EXPIRED"})),
            Error::Auth(AuthFailure::Expired)
        ));
        let Error::Auth(AuthFailure::Redirect(urls)) =
            f(json!({"response_type": "REDIRECT", "urls": ["wss://other"]}))
        else {
            panic!("not a redirect")
        };
        assert_eq!(urls, ["wss://other"]);
        assert!(matches!(
            serde_json::from_value::<LoginResponse>(
                json!({"response_type": "OTP_REQUIRED", "username": "admin"})
            )
            .unwrap(),
            LoginResponse::OtpRequired {}
        ));
    }

    #[test]
    fn secret_is_never_printed() {
        let s = Secret::new("hunter2");
        assert_eq!(format!("{s:?}"), "Secret(***)");
        assert_eq!(s.expose(), "hunter2");
    }
}
