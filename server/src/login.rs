//! `POST /api/login` (user name and password) and `POST /api/login/otp` (one-time code).
//!
//! A password login either ends with a session cookie or, for an account with two-factor
//! authentication, parks the half-authenticated NAS connection under a random ticket sent in a
//! short-lived `HttpOnly` cookie until the code arrives. Every attempt, password or code, goes
//! through the [`RateLimiter`].
//!
//! Requests must be JSON (`Content-Type: application/json`), which a cross-site form cannot
//! send without a CORS preflight that truelite never allows.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use ring::rand::SystemRandom;
use serde::Deserialize;
use serde_json::json;

use crate::origin::SameOrigin;
use crate::rate_limit::{LimitOptions, RateLimiter, RetryAfter};
use crate::sessions::{
    Key, LOGOUT_TIMEOUT, SessionId, SessionInfo, SessionStore, cookie, named_cookie, set_cookie,
    spawn_every,
};
use crate::truenas::{self, AuthFailure, Endpoint, LoginStep, PendingOtp, Secret, SessionOptions};

/// Cookie holding the ticket of a login waiting for its one-time code.
pub const OTP_COOKIE: &str = "truelite-otp";
/// How long a login may wait for its one-time code.
const OTP_TTL: Duration = Duration::from_secs(120);
/// Logins waiting for a code at once; each holds a socket to the NAS.
const MAX_PENDING: usize = 8;
/// Wrong codes allowed per login before it must start again.
const MAX_WRONG_CODES: u8 = 3;
/// Bound on one login step, connection included.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(20);

/// A login waiting for its one-time code.
struct Pending {
    otp: PendingOtp,
    created: Instant,
    wrong: u8,
}

impl Pending {
    fn expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.created) > OTP_TTL
    }
}

/// Login state shared by the handlers. Cheap to clone.
#[derive(Clone)]
pub struct Login {
    inner: Arc<Inner>,
}

struct Inner {
    nas: Option<Endpoint>,
    opts: SessionOptions,
    limiter: RateLimiter,
    pending: Mutex<HashMap<Key, Pending>>,
    rng: SystemRandom,
}

impl Login {
    /// `nas` is `None` when no NAS is configured; logins then fail with 503.
    pub fn new(nas: Option<Endpoint>, opts: SessionOptions, limits: LimitOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                nas,
                opts,
                limiter: RateLimiter::new(limits),
                pending: Mutex::new(HashMap::new()),
                rng: SystemRandom::new(),
            }),
        }
    }

    /// Never held across an `.await`; every update is a single insert or remove, so poison is
    /// ignored.
    fn lock(&self) -> MutexGuard<'_, HashMap<Key, Pending>> {
        self.inner
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Stores a login waiting for its code and returns its new ticket.
    fn park(&self, otp: PendingOtp) -> Result<SessionId, LoginError> {
        let ticket = SessionId::random(&self.inner.rng);
        let key = Key::of(ticket.as_str()).expect("generated ids are well formed");
        let now = Instant::now();
        let mut map = self.lock();
        if map.len() >= MAX_PENDING {
            map.retain(|_, p| !p.expired(now));
        }
        if map.len() >= MAX_PENDING {
            tracing::warn!(max = MAX_PENDING, "too many logins waiting for a code");
            return Err(LoginError::Busy);
        }
        map.insert(
            key,
            Pending {
                otp,
                created: now,
                wrong: 0,
            },
        );
        Ok(ticket)
    }

    /// Removes and returns the live login for `ticket`, so only one request uses it at a time.
    fn take(&self, ticket: &str) -> Option<(Key, Pending)> {
        let key = Key::of(ticket)?;
        let pending = self.lock().remove(&key)?;
        (!pending.expired(Instant::now())).then_some((key, pending))
    }

    /// Puts back a login after a wrong code, under the same ticket and with the same deadline.
    fn put_back(&self, key: Key, pending: Pending) {
        self.lock().insert(key, pending);
    }

    /// Forgets expired pending logins (closing their NAS sockets) and full rate-limit buckets.
    pub fn sweep_at(&self, now: Instant) {
        // Dropped after the lock is released: dropping closes NAS sockets.
        let expired: Vec<_> = self.lock().extract_if(|_, p| p.expired(now)).collect();
        drop(expired);
        self.inner.limiter.prune_at(now);
    }

    /// Sweeps every `every` until the runtime stops.
    pub fn spawn_sweeper(&self, every: Duration) -> tokio::task::JoinHandle<()> {
        let login = self.clone();
        spawn_every(every, move |now| login.sweep_at(now))
    }

    fn nas(&self) -> Result<&Endpoint, LoginError> {
        self.inner.nas.as_ref().ok_or(LoginError::NoNas)
    }

    fn check_rate(&self, addr: SocketAddr) -> Result<(), LoginError> {
        self.inner.limiter.check(addr.ip()).map_err(|wait| {
            tracing::info!(client = %addr.ip(), "login rate limited");
            LoginError::RateLimited(wait)
        })
    }

    #[cfg(test)]
    fn pending(&self) -> usize {
        self.lock().len()
    }
}

/// Why a login step failed. The response says it in plain words; NAS details are only logged.
#[derive(Debug)]
pub enum LoginError {
    /// Not JSON, or an empty field, or a code that is not 6 to 8 digits.
    BadRequest,
    NoNas,
    RateLimited(RetryAfter),
    BadCredentials,
    Expired,
    /// The account may not use a web UI (`privilege.webui_access`).
    NoWebUi,
    /// The NAS asks to log in elsewhere (`REDIRECT`).
    Unsupported,
    Unreachable,
    TimedOut,
    /// The session store is full.
    Full,
    /// Too many logins are waiting for a code.
    Busy,
    /// A wrong one-time code; the login still waits for the right one.
    WrongCode,
    /// No login is waiting for a code under this ticket: start again. Deletes the ticket cookie.
    StartAgain(Restart),
}

/// Why a login has to start again from the password; tells the user in the `reason` field.
#[derive(Debug, Clone, Copy)]
pub enum Restart {
    /// No ticket, or the ticket expired or was already used.
    Expired,
    /// Too many wrong codes, here or on the NAS.
    TooManyCodes,
}

impl Restart {
    fn as_str(self) -> &'static str {
        match self {
            Restart::Expired => "expired",
            Restart::TooManyCodes => "too_many_codes",
        }
    }
}

impl LoginError {
    /// Maps a NAS error, logging what the user is not told.
    fn from_nas(e: truenas::Error) -> Self {
        match e {
            truenas::Error::Auth(AuthFailure::BadCredentials) => LoginError::BadCredentials,
            truenas::Error::Auth(AuthFailure::Expired) => LoginError::Expired,
            truenas::Error::Auth(AuthFailure::Redirect(_)) => {
                tracing::warn!(error = %e, "login refused");
                LoginError::Unsupported
            }
            truenas::Error::CertMismatch { .. } => {
                tracing::error!(error = %e, "login failed");
                LoginError::Unreachable
            }
            e => {
                tracing::warn!(error = %e, "login failed");
                LoginError::Unreachable
            }
        }
    }
}

impl IntoResponse for LoginError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            LoginError::BadRequest => (StatusCode::BAD_REQUEST, "invalid request"),
            LoginError::NoNas => (StatusCode::SERVICE_UNAVAILABLE, "no NAS is configured"),
            LoginError::RateLimited(_) => (
                StatusCode::TOO_MANY_REQUESTS,
                "too many attempts, try again later",
            ),
            LoginError::BadCredentials => (StatusCode::UNAUTHORIZED, "wrong user name or password"),
            LoginError::Expired => (
                StatusCode::FORBIDDEN,
                "this password or account has expired; change it on the NAS",
            ),
            LoginError::NoWebUi => (
                StatusCode::FORBIDDEN,
                "this account is not allowed to use a web interface",
            ),
            LoginError::Unsupported => (StatusCode::BAD_GATEWAY, "this NAS setup is not supported"),
            LoginError::Unreachable => (StatusCode::BAD_GATEWAY, "cannot reach the NAS"),
            LoginError::TimedOut => (
                StatusCode::GATEWAY_TIMEOUT,
                "the NAS did not answer in time",
            ),
            LoginError::Full | LoginError::Busy => (
                StatusCode::SERVICE_UNAVAILABLE,
                "too many people are logged in, try again later",
            ),
            LoginError::WrongCode => {
                let body = json!({ "error": "wrong code", "otp_required": true });
                return (StatusCode::UNAUTHORIZED, Json(body)).into_response();
            }
            LoginError::StartAgain(why) => {
                return (
                    StatusCode::UNAUTHORIZED,
                    [(header::SET_COOKIE, clear_otp_cookie())],
                    Json(json!({ "error": "log in again", "reason": why.as_str() })),
                )
                    .into_response();
            }
        };
        let mut res = (status, Json(json!({ "error": msg }))).into_response();
        if let LoginError::RateLimited(wait) = self {
            res.headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(wait.secs()));
        }
        res
    }
}

/// `Set-Cookie` value that stores an OTP ticket until it expires.
fn otp_cookie(ticket: &SessionId) -> HeaderValue {
    cookie(OTP_COOKIE, ticket.as_str(), OTP_TTL)
}

/// `Set-Cookie` value that deletes the OTP ticket cookie.
fn clear_otp_cookie() -> HeaderValue {
    static CLEAR: LazyLock<HeaderValue> = LazyLock::new(|| cookie(OTP_COOKIE, "", Duration::ZERO));
    CLEAR.clone()
}

/// Body of `POST /api/login`. No `Debug`: it holds the password.
#[derive(Deserialize)]
pub struct PasswordForm {
    username: String,
    password: String,
}

/// Body of `POST /api/login/otp`. No `Debug`: it holds the code.
#[derive(Deserialize)]
pub struct OtpForm {
    code: String,
}

/// Logs in with a user name and password.
pub async fn password(
    _: SameOrigin,
    State(login): State<Login>,
    State(sessions): State<SessionStore>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    form: Result<Json<PasswordForm>, JsonRejection>,
) -> Result<Response, LoginError> {
    let Json(form) = form.map_err(|_| LoginError::BadRequest)?;
    let username = form.username.trim();
    if username.is_empty() || form.password.is_empty() {
        return Err(LoginError::BadRequest);
    }
    let nas = login.nas()?;
    login.check_rate(addr)?;
    let step = tokio::time::timeout(
        LOGIN_TIMEOUT,
        truenas::login(
            nas,
            login.inner.opts.clone(),
            username,
            Secret::new(form.password),
        ),
    )
    .await
    .map_err(|_| LoginError::TimedOut)?
    .map_err(LoginError::from_nas)?;
    match step {
        LoginStep::Done(session) => admit(session, &sessions).await,
        LoginStep::OtpRequired(otp) => {
            let ticket = login.park(otp)?;
            Ok((
                [(header::SET_COOKIE, otp_cookie(&ticket))],
                Json(json!({ "otp_required": true })),
            )
                .into_response())
        }
    }
}

/// Sends the one-time code of a login started with [`password`].
pub async fn otp(
    _: SameOrigin,
    State(login): State<Login>,
    State(sessions): State<SessionStore>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    form: Result<Json<OtpForm>, JsonRejection>,
) -> Result<Response, LoginError> {
    let Json(form) = form.map_err(|_| LoginError::BadRequest)?;
    let code = form.code.trim();
    if !(6..=8).contains(&code.len()) || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(LoginError::BadRequest);
    }
    let ticket =
        named_cookie(&headers, OTP_COOKIE).ok_or(LoginError::StartAgain(Restart::Expired))?;
    login.check_rate(addr)?;
    let (key, mut pending) = login
        .take(ticket)
        .ok_or(LoginError::StartAgain(Restart::Expired))?;
    // The pending login is now out of the store: anything but a wrong code ends it, so every
    // other outcome also deletes the ticket cookie.
    let step = match tokio::time::timeout(LOGIN_TIMEOUT, pending.otp.submit(code)).await {
        Err(_) => return Ok(ended(LoginError::TimedOut)),
        // TrueNAS gave up on this login (too many wrong codes on its side).
        Ok(Err(truenas::Error::Auth(_))) => {
            return Err(LoginError::StartAgain(Restart::TooManyCodes));
        }
        Ok(Err(e)) => return Ok(ended(LoginError::from_nas(e))),
        Ok(Ok(step)) => step,
    };
    match step {
        LoginStep::Done(session) => Ok(match admit(session, &sessions).await {
            Ok(mut res) => {
                res.headers_mut()
                    .append(header::SET_COOKIE, clear_otp_cookie());
                res
            }
            Err(e) => ended(e),
        }),
        LoginStep::OtpRequired(otp) => {
            pending.otp = otp;
            pending.wrong += 1;
            if pending.wrong >= MAX_WRONG_CODES {
                tracing::info!(user = %pending.otp.username(), "too many wrong codes");
                return Err(LoginError::StartAgain(Restart::TooManyCodes));
            }
            login.put_back(key, pending);
            Err(LoginError::WrongCode)
        }
    }
}

/// The response for `e` that also deletes the OTP ticket cookie: the pending login is gone.
fn ended(e: LoginError) -> Response {
    let mut res = e.into_response();
    res.headers_mut()
        .append(header::SET_COOKIE, clear_otp_cookie());
    res
}

/// Turns a NAS session into a browser session: checks web UI access, stores it and sets the
/// session cookie.
async fn admit(session: truenas::Session, sessions: &SessionStore) -> Result<Response, LoginError> {
    if !session.user().webui_access {
        tracing::info!(user = %session.user().username, "login refused: no web UI access");
        let _ = tokio::time::timeout(LOGOUT_TIMEOUT, session.logout()).await;
        return Err(LoginError::NoWebUi);
    }
    // Serialized now: `insert` takes the session.
    let body = Json(SessionInfo::of(&session)).into_response();
    let id = sessions.insert(session).map_err(|_| LoginError::Full)?;
    Ok((
        [(
            header::SET_COOKIE,
            set_cookie(&id, sessions.options().max_age),
        )],
        body,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use serde_json::Value;

    use crate::AppState;
    use crate::sessions::{COOKIE, StoreOptions};
    use crate::test_client::{HttpReply as Reply, http as send};
    use crate::truenas::mock::{Mock, fake_nas};

    const CLIENT: &str = "192.0.2.1:50000";

    struct Harness {
        mock: Mock,
        state: AppState,
    }

    impl Harness {
        async fn new() -> Self {
            Self::with_store(StoreOptions::default()).await
        }

        async fn with_store(store: StoreOptions) -> Self {
            let (_nas, handler) = fake_nas();
            let mock = Mock::start_conn(handler).await;
            let ep = Endpoint::new(mock.url.clone(), None).unwrap();
            let state = AppState {
                sessions: SessionStore::new(store),
                login: Login::new(Some(ep), SessionOptions::default(), LimitOptions::default()),
                allowlist: crate::allowlist::Allowlist::default(),
                origins: crate::origin::Origins::default(),
            };
            Self { mock, state }
        }

        fn router(&self) -> Router {
            crate::router(self.state.clone())
        }

        async fn post(&self, uri: &str, body: Value, cookie: Option<&str>, from: &str) -> Reply {
            let mut req = Request::post(uri).header(header::CONTENT_TYPE, "application/json");
            if let Some(c) = cookie {
                req = req.header(header::COOKIE, c);
            }
            send(
                self.router(),
                req.body(Body::from(body.to_string())).unwrap(),
                from,
            )
            .await
        }

        async fn password(&self, user: &str, pw: &str) -> Reply {
            self.post(
                "/api/login",
                json!({"username": user, "password": pw}),
                None,
                CLIENT,
            )
            .await
        }

        async fn code(&self, ticket: &str, code: &str) -> Reply {
            self.post(
                "/api/login/otp",
                json!({ "code": code }),
                Some(&format!("{OTP_COOKIE}={ticket}")),
                CLIENT,
            )
            .await
        }
    }

    async fn session(h: &Harness, id: &str) -> Reply {
        let req = Request::get("/api/session")
            .header(header::COOKIE, format!("{COOKIE}={id}"))
            .body(Body::empty())
            .unwrap();
        send(h.router(), req, CLIENT).await
    }

    #[tokio::test]
    async fn password_login_sets_a_session_cookie() {
        let h = Harness::new().await;
        let r = h.password("admin", "secret").await;
        assert_eq!(r.status, StatusCode::OK);
        assert_eq!(r.body["username"], "admin");
        assert_eq!(r.body["two_factor"], false);
        let id = r.cookie(COOKIE).unwrap();
        assert_eq!(id.len(), 64);
        let s = session(&h, &id).await;
        assert_eq!(s.status, StatusCode::OK);
        assert_eq!(s.body["username"], "admin");
    }

    #[tokio::test]
    async fn refused_logins_say_why_in_plain_words() {
        let h = Harness::new().await;
        for (user, status, msg) in [
            (
                "admin",
                StatusCode::UNAUTHORIZED,
                "wrong user name or password",
            ),
            (
                "nobody",
                StatusCode::UNAUTHORIZED,
                "wrong user name or password",
            ),
            (
                "old",
                StatusCode::FORBIDDEN,
                "this password or account has expired; change it on the NAS",
            ),
            (
                "elsewhere",
                StatusCode::BAD_GATEWAY,
                "this NAS setup is not supported",
            ),
        ] {
            let r = h.password(user, "wrong").await;
            assert_eq!(r.status, status, "{user}");
            assert_eq!(r.body["error"], msg, "{user}");
            assert_eq!(r.cookie(COOKIE), None);
        }
    }

    #[tokio::test]
    async fn an_account_without_web_ui_access_is_refused_and_logged_out() {
        let h = Harness::new().await;
        let r = h.password("noui", "secret").await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
        assert_eq!(r.cookie(COOKIE), None);
        assert!(h.mock.calls().iter().any(|(_, m)| m == "auth.logout"));
    }

    #[tokio::test]
    async fn one_time_code_flow() {
        let h = Harness::new().await;
        let r = h.password("otp", "secret").await;
        assert_eq!(r.status, StatusCode::OK);
        assert_eq!(r.body, json!({"otp_required": true}));
        assert_eq!(r.cookie(COOKIE), None);
        let ticket = r.cookie(OTP_COOKIE).unwrap();
        assert_eq!(ticket.len(), 64);

        let r = h.code(&ticket, "000000").await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
        assert_eq!(r.body["otp_required"], true);
        assert_eq!(r.cookie(OTP_COOKIE), None, "a wrong code keeps the ticket");

        let r = h.code(&ticket, "123456").await;
        assert_eq!(r.status, StatusCode::OK);
        assert_eq!(r.body["username"], "otp");
        assert_eq!(r.body["two_factor"], true);
        assert_eq!(r.cookie(OTP_COOKIE).as_deref(), Some(""));
        let id = r.cookie(COOKIE).unwrap();
        assert_eq!(session(&h, &id).await.status, StatusCode::OK);

        // The ticket is single use.
        let r = h.code(&ticket, "123456").await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
        assert_eq!(r.body["error"], "log in again");
        assert_eq!(r.cookie(OTP_COOKIE).as_deref(), Some(""));
        assert_eq!(h.state.login.pending(), 0);
    }

    #[tokio::test]
    async fn three_wrong_codes_end_the_login() {
        let h = Harness::new().await;
        let ticket = h
            .password("otp", "secret")
            .await
            .cookie(OTP_COOKIE)
            .unwrap();
        for _ in 0..2 {
            assert_eq!(h.code(&ticket, "000000").await.body["otp_required"], true);
        }
        let r = h.code(&ticket, "000000").await;
        assert_eq!(r.body["error"], "log in again");
        assert_eq!(r.body["reason"], "too_many_codes");
        assert_eq!(r.cookie(OTP_COOKIE).as_deref(), Some(""));
        assert_eq!(h.state.login.pending(), 0);
        assert_eq!(
            h.code(&ticket, "123456").await.body["error"],
            "log in again"
        );
    }

    #[tokio::test]
    async fn unknown_or_expired_tickets_start_again() {
        let h = Harness::new().await;
        let r = h.code(&"a".repeat(64), "123456").await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
        assert_eq!(r.body["reason"], "expired");
        assert_eq!(r.cookie(OTP_COOKIE).as_deref(), Some(""));

        let ticket = h
            .password("otp", "secret")
            .await
            .cookie(OTP_COOKIE)
            .unwrap();
        h.state
            .login
            .sweep_at(Instant::now() + OTP_TTL + Duration::from_secs(1));
        assert_eq!(h.state.login.pending(), 0);
        assert_eq!(
            h.code(&ticket, "123456").await.body["error"],
            "log in again"
        );
    }

    #[tokio::test]
    async fn the_sixth_attempt_from_one_client_is_rate_limited() {
        let h = Harness::new().await;
        for _ in 0..5 {
            assert_eq!(
                h.password("admin", "wrong").await.status,
                StatusCode::UNAUTHORIZED
            );
        }
        let r = h.password("admin", "secret").await;
        assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(r.headers[header::RETRY_AFTER], "60");
        // The NAS saw only five attempts.
        let attempts = h
            .mock
            .calls()
            .iter()
            .filter(|(_, m)| m == "auth.login_ex")
            .count();
        assert_eq!(attempts, 5);
        // Another client still gets in.
        let r = h
            .post(
                "/api/login",
                json!({"username": "admin", "password": "secret"}),
                None,
                "198.51.100.9:1234",
            )
            .await;
        assert_eq!(r.status, StatusCode::OK);
    }

    #[tokio::test]
    async fn bad_requests_are_refused_before_the_nas_or_the_limit() {
        let h = Harness::new().await;
        for body in [
            json!({"username": "admin", "password": ""}),
            json!({"username": "  ", "password": "secret"}),
            json!({"username": "admin"}),
        ] {
            let r = h.post("/api/login", body, None, CLIENT).await;
            assert_eq!(r.status, StatusCode::BAD_REQUEST);
        }
        for code in ["12345", "123456789", "12345a", ""] {
            assert_eq!(
                h.code(&"a".repeat(64), code).await.status,
                StatusCode::BAD_REQUEST
            );
        }
        let form = Request::post("/api/login")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("username=admin&password=secret"))
            .unwrap();
        assert_eq!(
            send(h.router(), form, CLIENT).await.status,
            StatusCode::BAD_REQUEST
        );
        let big = json!({"username": "admin", "password": "x".repeat(crate::LOGIN_BODY_LIMIT)});
        assert_eq!(
            h.post("/api/login", big, None, CLIENT).await.status,
            StatusCode::BAD_REQUEST
        );

        assert!(h.mock.calls().is_empty());
        assert_eq!(h.password("admin", "secret").await.status, StatusCode::OK);
    }

    #[tokio::test]
    async fn foreign_origins_are_refused_before_the_nas() {
        let h = Harness::new().await;
        for (uri, body) in [
            ("/api/login", r#"{"username":"admin","password":"secret"}"#),
            ("/api/login/otp", r#"{"code":"123456"}"#),
        ] {
            for origin in ["http://evil.example", "null", "http://truelite.test:8081"] {
                let req = Request::post(uri)
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::ORIGIN, origin)
                    .body(Body::from(body))
                    .unwrap();
                let r = send(h.router(), req, CLIENT).await;
                assert_eq!(r.status, StatusCode::FORBIDDEN, "{uri} from {origin}");
                assert_eq!(r.cookie(COOKIE), None);
            }
        }
        assert!(h.mock.calls().is_empty());
        // The rate limit was not spent on them.
        assert_eq!(h.password("admin", "secret").await.status, StatusCode::OK);
    }

    #[tokio::test]
    async fn nas_and_store_failures() {
        // No NAS configured.
        let state = AppState {
            sessions: SessionStore::new(StoreOptions::default()),
            login: Login::new(None, SessionOptions::default(), LimitOptions::default()),
            allowlist: crate::allowlist::Allowlist::default(),
            origins: crate::origin::Origins::default(),
        };
        let req = Request::post("/api/login")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"username":"admin","password":"secret"}"#))
            .unwrap();
        let r = send(crate::router(state), req, CLIENT).await;
        assert_eq!(r.status, StatusCode::SERVICE_UNAVAILABLE);

        // The NAS is down.
        let h = Harness::new().await;
        h.mock.set_down(true);
        let r = h.password("admin", "secret").await;
        assert_eq!(r.status, StatusCode::BAD_GATEWAY);
        assert_eq!(r.body["error"], "cannot reach the NAS");

        // The session store is full.
        let h = Harness::with_store(StoreOptions {
            max_sessions: 0,
            ..StoreOptions::default()
        })
        .await;
        let r = h.password("admin", "secret").await;
        assert_eq!(r.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(r.cookie(COOKIE), None);
    }
}
