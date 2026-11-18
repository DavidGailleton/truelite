//! Browser sessions: a random id in a cookie maps to a logged-in [`truenas::Session`].
//!
//! The id is 32 random bytes, hex-encoded, sent only in an `HttpOnly`, `Secure`,
//! `SameSite=Strict` cookie with the `__Host-` prefix. The store keys entries by the SHA-256 of
//! the id, so it never holds the id itself and lookups never compare secrets.
//!
//! A session ends when it is idle (no request and no open WebSocket for [`StoreOptions::idle`]),
//! when it is older than [`StoreOptions::max_age`], when its NAS link has ended, or on logout.
//! Ending a session logs it out on the NAS.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use ring::digest;
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::json;
use tokio::task::{JoinHandle, JoinSet};

use crate::truenas::{self, Link};

/// Cookie name. `__Host-` makes browsers require `Secure`, `Path=/` and no `Domain`.
pub const COOKIE: &str = "__Host-truelite";
const ID_BYTES: usize = 32;
const ID_LEN: usize = ID_BYTES * 2;

/// A session id as sent in the cookie. Never printed: `Debug` is redacted and there is no
/// `Display`.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionId(String);

impl SessionId {
    /// A fresh id: 32 bytes from the OS generator, hex-encoded. Also used for one-time-code
    /// login tickets.
    pub(crate) fn random(rng: &SystemRandom) -> Self {
        let mut bytes = [0u8; ID_BYTES];
        // The OS generator only fails when the system is broken; serving on would be worse.
        rng.fill(&mut bytes)
            .expect("the OS random number generator failed");
        let mut id = String::with_capacity(ID_LEN);
        for b in bytes {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            id.push(char::from(HEX[usize::from(b >> 4)]));
            id.push(char::from(HEX[usize::from(b & 0x0f)]));
        }
        SessionId(id)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionId(***)")
    }
}

/// SHA-256 of a well-formed id: what the store keys entries by.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Key([u8; 32]);

impl Key {
    /// `None` unless `id` is exactly [`ID_LEN`] lowercase hex digits.
    pub(crate) fn of(id: &str) -> Option<Key> {
        let well_formed =
            id.len() == ID_LEN && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        if !well_formed {
            return None;
        }
        let mut key = [0; 32];
        key.copy_from_slice(digest::digest(&digest::SHA256, id.as_bytes()).as_ref());
        Some(Key(key))
    }
}

#[derive(Debug, Clone)]
pub struct StoreOptions {
    /// A session with no request and no open WebSocket for this long ends.
    pub idle: Duration,
    /// A session ends this long after login, open WebSocket or not.
    pub max_age: Duration,
    /// At most this many sessions at once; each holds a socket to the NAS.
    pub max_sessions: usize,
    /// How often [`SessionStore::spawn_sweeper`] looks for expired sessions.
    pub sweep_every: Duration,
}

impl Default for StoreOptions {
    fn default() -> Self {
        Self {
            idle: Duration::from_secs(30 * 60),
            max_age: Duration::from_secs(24 * 3600),
            max_sessions: 32,
            sweep_every: Duration::from_secs(60),
        }
    }
}

/// The store already holds [`StoreOptions::max_sessions`] live sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreFull;

impl fmt::Display for StoreFull {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("too many open sessions")
    }
}

impl std::error::Error for StoreFull {}

/// Why a session ended, for the logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    Idle,
    MaxAge,
    /// The NAS link ended (token refused, certificate changed, logged out elsewhere).
    Lost,
    Logout,
    Full,
    Shutdown,
}

impl End {
    fn as_str(self) -> &'static str {
        match self {
            End::Idle => "idle",
            End::MaxAge => "max_age",
            End::Lost => "lost",
            End::Logout => "logout",
            End::Full => "store_full",
            End::Shutdown => "shutdown",
        }
    }
}

struct Entry {
    session: truenas::Session,
    created: Instant,
    last_seen: Instant,
    /// Open browser WebSockets ([`WsGuard`]s); while any is open the session is not idle.
    ws: usize,
}

impl Entry {
    fn expiry(&self, now: Instant, opts: &StoreOptions) -> Option<End> {
        if matches!(*self.session.link().borrow(), Link::Ended(_)) {
            Some(End::Lost)
        } else if now.saturating_duration_since(self.created) > opts.max_age {
            Some(End::MaxAge)
        } else if self.ws == 0 && now.saturating_duration_since(self.last_seen) > opts.idle {
            Some(End::Idle)
        } else {
            None
        }
    }
}

/// Logs a session's end and logs it out on the NAS in the background (a no-op if its link has
/// already ended). Call without holding the store lock.
fn end(session: truenas::Session, why: End) {
    tracing::info!(user = %session.user().username, reason = why.as_str(), "session ended");
    if why != End::Logout {
        tokio::spawn(async move { session.logout().await });
    }
}

/// The live sessions. Cheap to clone.
#[derive(Clone)]
pub struct SessionStore {
    inner: Arc<Shared>,
}

struct Shared {
    map: Mutex<HashMap<Key, Entry>>,
    opts: StoreOptions,
    rng: SystemRandom,
}

impl SessionStore {
    pub fn new(opts: StoreOptions) -> Self {
        Self {
            inner: Arc::new(Shared {
                map: Mutex::new(HashMap::new()),
                opts,
                rng: SystemRandom::new(),
            }),
        }
    }

    pub fn options(&self) -> &StoreOptions {
        &self.inner.opts
    }

    /// The map lock. Never held across an `.await`; a panic while holding it leaves the map
    /// consistent (every update is a single insert, remove or field write), so poison is ignored.
    fn lock(&self) -> MutexGuard<'_, HashMap<Key, Entry>> {
        self.inner
            .map
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn new_id(&self) -> SessionId {
        SessionId::random(&self.inner.rng)
    }

    /// Adds a logged-in session and returns its new id.
    ///
    /// # Errors
    ///
    /// [`StoreFull`] when [`StoreOptions::max_sessions`] live sessions exist even after
    /// expired ones are removed; `session` is then logged out.
    pub fn insert(&self, session: truenas::Session) -> Result<SessionId, StoreFull> {
        let id = self.new_id();
        let key = Key::of(id.as_str()).expect("generated ids are well formed");
        let now = Instant::now();
        let mut ended = Vec::new();
        let full = {
            let mut map = self.lock();
            if map.len() >= self.inner.opts.max_sessions {
                take_expired(&mut map, now, &self.inner.opts, &mut ended);
            }
            let full = map.len() >= self.inner.opts.max_sessions;
            if !full {
                tracing::info!(user = %session.user().username, "session started");
                map.insert(
                    key,
                    Entry {
                        session: session.clone(),
                        created: now,
                        last_seen: now,
                        ws: 0,
                    },
                );
            }
            full
        };
        for (s, why) in ended {
            end(s, why);
        }
        if full {
            tracing::warn!(max = self.inner.opts.max_sessions, "session store full");
            end(session, End::Full);
            return Err(StoreFull);
        }
        Ok(id)
    }

    /// The live session for `id`, which counts as activity. An expired one is ended and
    /// `None` returned; so are unknown and malformed ids.
    pub fn get(&self, id: &str) -> Option<truenas::Session> {
        self.touch(id, false).map(|(s, _)| s)
    }

    /// Ends the session for `id` and returns it so the caller can await
    /// [`truenas::Session::logout`].
    pub fn remove(&self, id: &str) -> Option<truenas::Session> {
        let key = Key::of(id)?;
        let entry = self.lock().remove(&key)?;
        end(entry.session.clone(), End::Logout);
        Some(entry.session)
    }

    /// Marks a browser WebSocket open on the session for `id` until the guard drops. While
    /// any is open the session cannot go idle; the idle countdown starts when the last closes.
    pub fn ws_guard(&self, id: &str) -> Option<WsGuard> {
        self.touch(id, true).map(|(_, key)| WsGuard {
            store: self.clone(),
            key,
        })
    }

    fn touch(&self, id: &str, open_ws: bool) -> Option<(truenas::Session, Key)> {
        let key = Key::of(id)?;
        let now = Instant::now();
        let mut map = self.lock();
        let entry = map.get_mut(&key)?;
        let Some(why) = entry.expiry(now, &self.inner.opts) else {
            entry.last_seen = now;
            if open_ws {
                entry.ws += 1;
            }
            return Some((entry.session.clone(), key));
        };
        let entry = map.remove(&key)?;
        drop(map);
        end(entry.session, why);
        None
    }

    /// Ends every session expired at `now`.
    pub fn sweep_at(&self, now: Instant) {
        let mut ended = Vec::new();
        take_expired(&mut self.lock(), now, &self.inner.opts, &mut ended);
        for (s, why) in ended {
            end(s, why);
        }
    }

    /// Sweeps every [`StoreOptions::sweep_every`] until the runtime stops.
    pub fn spawn_sweeper(&self) -> JoinHandle<()> {
        let store = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(store.inner.opts.sweep_every);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            tick.tick().await;
            loop {
                tick.tick().await;
                store.sweep_at(Instant::now());
            }
        })
    }

    /// Ends every session and waits up to `timeout` for the NAS logouts (server shutdown).
    pub async fn logout_all(&self, timeout: Duration) {
        let sessions: Vec<_> = self.lock().drain().map(|(_, e)| e.session).collect();
        if sessions.is_empty() {
            return;
        }
        let mut logouts = JoinSet::new();
        for s in sessions {
            tracing::info!(user = %s.user().username, reason = End::Shutdown.as_str(), "session ended");
            logouts.spawn(async move { s.logout().await });
        }
        if tokio::time::timeout(timeout, logouts.join_all())
            .await
            .is_err()
        {
            tracing::warn!("some NAS logouts did not finish before shutdown");
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.lock().len()
    }
}

fn take_expired(
    map: &mut HashMap<Key, Entry>,
    now: Instant,
    opts: &StoreOptions,
    out: &mut Vec<(truenas::Session, End)>,
) {
    let expired: Vec<(Key, End)> = map
        .iter()
        .filter_map(|(k, e)| e.expiry(now, opts).map(|why| (*k, why)))
        .collect();
    for (k, why) in expired {
        if let Some(e) = map.remove(&k) {
            out.push((e.session, why));
        }
    }
}

/// An open browser WebSocket on a session; see [`SessionStore::ws_guard`].
pub struct WsGuard {
    store: SessionStore,
    key: Key,
}

impl Drop for WsGuard {
    fn drop(&mut self) {
        if let Some(e) = self.store.lock().get_mut(&self.key) {
            e.ws = e.ws.saturating_sub(1);
            e.last_seen = Instant::now();
        }
    }
}

/// `Set-Cookie` value that stores `id` for `max_age`. The server enforces the lifetime anyway;
/// `Max-Age` only lets the cookie survive a browser or PWA restart.
pub fn set_cookie(id: &SessionId, max_age: Duration) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{COOKIE}={}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={}",
        id.as_str(),
        max_age.as_secs()
    ))
    .expect("hex ids are valid header values")
}

/// `Set-Cookie` value that deletes the session cookie.
pub fn clear_cookie() -> HeaderValue {
    HeaderValue::from_static(
        "__Host-truelite=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0",
    )
}

/// The session cookie's value, from any of the request's `Cookie` headers.
pub fn cookie_value(headers: &HeaderMap) -> Option<&str> {
    named_cookie(headers, COOKIE)
}

/// The value of the cookie called `name`, from any of the request's `Cookie` headers.
pub fn named_cookie<'h>(headers: &'h HeaderMap, name: &str) -> Option<&'h str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|pair| {
            let (key, value) = pair.trim().split_once('=')?;
            (key == name).then_some(value)
        })
}

/// What the web app learns about a session (`GET /api/session` and a successful login).
pub fn session_json(session: &truenas::Session) -> serde_json::Value {
    let user = session.user();
    let link = match *session.link().borrow() {
        Link::Up => "up",
        Link::Reconnecting { .. } => "reconnecting",
        Link::Ended(_) => "ended",
    };
    json!({
        "username": user.username,
        "roles": user.roles,
        "two_factor": user.two_factor,
        "link": link,
    })
}

/// A request from a logged-in browser. Rejects with 401 otherwise.
pub struct Authed {
    pub session: truenas::Session,
    #[allow(
        dead_code,
        reason = "the WebSocket bridge (#16) opens a WsGuard with it"
    )]
    pub id: SessionId,
}

impl<S> FromRequestParts<S> for Authed
where
    SessionStore: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = NotLoggedIn;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Some(value) = cookie_value(&parts.headers) else {
            return Err(NotLoggedIn {
                stale_cookie: false,
            });
        };
        match SessionStore::from_ref(state).get(value) {
            Some(session) => Ok(Authed {
                session,
                id: SessionId(value.to_owned()),
            }),
            None => Err(NotLoggedIn { stale_cookie: true }),
        }
    }
}

/// 401 response; also deletes a cookie that no longer names a session.
#[derive(Debug)]
pub struct NotLoggedIn {
    stale_cookie: bool,
}

impl IntoResponse for NotLoggedIn {
    fn into_response(self) -> Response {
        let mut res = (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "not logged in" })),
        )
            .into_response();
        if self.stale_cookie {
            res.headers_mut().insert(header::SET_COOKIE, clear_cookie());
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::truenas::mock::{Mock, Nas, logged_in, within};
    use crate::truenas::{EndReason, SessionOptions};

    const MIN: Duration = Duration::from_secs(60);

    fn store() -> SessionStore {
        SessionStore::new(StoreOptions {
            max_sessions: 2,
            ..StoreOptions::default()
        })
    }

    fn session_opts() -> SessionOptions {
        SessionOptions {
            backoff_min: Duration::from_millis(20),
            backoff_max: Duration::from_millis(100),
            ..SessionOptions::default()
        }
    }

    async fn admin() -> (Mock, Arc<Mutex<Nas>>, truenas::Session) {
        logged_in(session_opts()).await
    }

    /// Waits until the mock has received `auth.logout`.
    async fn logged_out_on_nas(mock: &Mock) {
        within(async {
            while !mock.calls().iter().any(|(_, m)| m == "auth.logout") {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
    }

    #[tokio::test]
    async fn ids_are_random_hex_and_never_printed() {
        let store = store();
        let (a, b) = (store.new_id(), store.new_id());
        assert_eq!(a.as_str().len(), 64);
        assert!(Key::of(a.as_str()).is_some());
        assert_ne!(a, b);
        assert_eq!(format!("{a:?}"), "SessionId(***)");
    }

    #[tokio::test]
    async fn get_finds_inserted_sessions_only() {
        let (_mock, _nas, s) = admin().await;
        let store = store();
        let id = store.insert(s).unwrap();
        assert_eq!(store.get(id.as_str()).unwrap().user().username, "admin");
        let upper = id.as_str().to_uppercase();
        for bad in [
            "",
            "abc",
            &upper,
            &"0".repeat(64),
            &format!("{}0", id.as_str()),
        ] {
            assert!(store.get(bad).is_none(), "{bad:?}");
        }
        assert_eq!(store.len(), 1);
    }

    #[tokio::test]
    async fn idle_sessions_end_and_log_out_on_the_nas() {
        let (mock, _nas, s) = admin().await;
        let store = store();
        let id = store.insert(s).unwrap();
        store.sweep_at(Instant::now() + 29 * MIN);
        assert_eq!(store.len(), 1);
        store.sweep_at(Instant::now() + 31 * MIN);
        assert_eq!(store.len(), 0);
        assert!(store.get(id.as_str()).is_none());
        logged_out_on_nas(&mock).await;
    }

    #[tokio::test]
    async fn an_open_websocket_keeps_the_session_alive() {
        let (_mock, _nas, s) = admin().await;
        let store = store();
        let id = store.insert(s).unwrap();
        let guard = store.ws_guard(id.as_str()).unwrap();
        store.sweep_at(Instant::now() + 60 * MIN);
        assert_eq!(store.len(), 1);
        drop(guard);
        // The countdown starts when the socket closes.
        store.sweep_at(Instant::now() + 29 * MIN);
        assert_eq!(store.len(), 1);
        store.sweep_at(Instant::now() + 31 * MIN);
        assert_eq!(store.len(), 0);
    }

    #[tokio::test]
    async fn max_age_ends_even_an_open_session() {
        let (mock, _nas, s) = admin().await;
        let store = store();
        let id = store.insert(s).unwrap();
        let _guard = store.ws_guard(id.as_str()).unwrap();
        store.sweep_at(Instant::now() + 24 * 60 * MIN + MIN);
        assert_eq!(store.len(), 0);
        logged_out_on_nas(&mock).await;
    }

    #[tokio::test]
    async fn a_lost_nas_session_ends_the_browser_session() {
        let (mock, nas, s) = admin().await;
        let store = store();
        let id = store.insert(s.clone()).unwrap();
        nas.lock().unwrap().reject_tokens = true;
        mock.kill();
        let mut link = s.link();
        within(link.wait_for(|l| *l == Link::Ended(EndReason::SessionLost)))
            .await
            .unwrap();
        assert!(store.get(id.as_str()).is_none());
        assert_eq!(store.len(), 0);
    }

    #[tokio::test]
    async fn a_full_store_refuses_and_logs_the_extra_session_out() {
        let store = store();
        let mut mocks = Vec::new();
        for _ in 0..2 {
            let (mock, _nas, s) = admin().await;
            store.insert(s).unwrap();
            mocks.push(mock);
        }
        let (mock, _nas, s) = admin().await;
        assert_eq!(store.insert(s), Err(StoreFull));
        logged_out_on_nas(&mock).await;
        // Expired sessions make room.
        store.sweep_at(Instant::now() + 31 * MIN);
        let (_mock, _nas, s) = admin().await;
        assert!(store.insert(s).is_ok());
    }

    #[tokio::test]
    async fn remove_and_logout_all_end_sessions() {
        let (_mock, _nas, s) = admin().await;
        let store = store();
        let id = store.insert(s).unwrap();
        let s = store.remove(id.as_str()).unwrap();
        assert!(store.remove(id.as_str()).is_none());
        within(s.logout()).await;
        assert_eq!(*s.link().borrow(), Link::Ended(EndReason::LoggedOut));

        let (mock, _nas, s) = admin().await;
        store.insert(s).unwrap();
        within(store.logout_all(Duration::from_secs(2))).await;
        assert_eq!(store.len(), 0);
        logged_out_on_nas(&mock).await;
    }

    #[test]
    fn the_cookie_is_found_among_others() {
        let mut h = HeaderMap::new();
        assert_eq!(cookie_value(&h), None);
        h.append(header::COOKIE, HeaderValue::from_static("a=1; truelite=x"));
        assert_eq!(cookie_value(&h), None);
        h.append(
            header::COOKIE,
            HeaderValue::from_static("b=2;__Host-truelite=abc ; c=3"),
        );
        assert_eq!(cookie_value(&h), Some("abc"));
    }

    #[test]
    fn set_cookie_attributes() {
        let id = SessionId("ab".repeat(32));
        assert_eq!(
            set_cookie(&id, Duration::from_secs(86_400)),
            format!(
                "__Host-truelite={}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=86400",
                id.as_str()
            )
        );
        let clear = clear_cookie();
        let clear = clear.to_str().unwrap();
        assert!(clear.starts_with(&format!("{COOKIE}=;")));
        assert!(clear.ends_with("Max-Age=0"));
    }
}
