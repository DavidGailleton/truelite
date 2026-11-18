//! The browser's WebSocket (`/ws`): our small protocol on top of a logged-in
//! [`truenas::Session`], forwarding only what the [`Allowlist`] lists. The wire contract is in
//! `docs/ws-protocol.md`.
//!
//! One task per socket owns it and is the only writer. Calls and subscriptions run in their own
//! tasks and hand their messages back over a bounded channel, so a slow browser slows them down
//! instead of growing memory; a subscriber that falls too far behind gets `missed`.

use std::collections::HashMap;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::State;
use axum::extract::ws::{CloseFrame, Message, Utf8Bytes, WebSocket, WebSocketUpgrade};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::task::{AbortHandle, JoinSet};
use tracing::Instrument;

use crate::allowlist::Allowlist;
use crate::origin::SameOrigin;
use crate::sessions::{Authed, NotLoggedIn, SessionStore, WsGuard};
use crate::truenas::{self, Event, Link, Update, UpdateKind};

/// Largest message or frame from the browser.
const MAX_MESSAGE: usize = 64 * 1024;
/// Calls in flight per socket; more are refused with `busy`.
const MAX_CALLS: usize = 16;
/// Topics per socket; more are refused with `busy`.
const MAX_TOPICS: usize = 16;
/// Messages queued for the browser before calls and subscriptions wait.
const OUT_BUFFER: usize = 256;
/// Keeps proxies (Tailscale, a VPS reverse proxy) from dropping an idle socket.
const PING_EVERY: Duration = Duration::from_secs(30);
/// Close code when the session has ended: the web app goes to the login page.
pub const CLOSE_LOGGED_OUT: u16 = 4401;

/// `GET /ws`: upgrades a logged-in browser. 403 from a foreign page (checked first), 401
/// without a live session.
pub async fn upgrade(
    _: SameOrigin,
    Authed { session, id }: Authed,
    State(sessions): State<SessionStore>,
    State(allow): State<Allowlist>,
    ws: WebSocketUpgrade,
) -> Response {
    let Some(guard) = sessions.ws_guard(id.as_str()) else {
        return NotLoggedIn::stale().into_response();
    };
    let span = tracing::info_span!("ws", user = %session.user().username);
    ws.max_message_size(MAX_MESSAGE)
        .max_frame_size(MAX_MESSAGE)
        .on_upgrade(move |socket| {
            let (out_tx, out_rx) = mpsc::channel(OUT_BUFFER);
            let bridge = Bridge {
                session,
                allow,
                out_tx,
                calls: JoinSet::new(),
                subs: JoinSet::new(),
                topics: HashMap::new(),
                _guard: guard,
            };
            bridge.run(socket, out_rx).instrument(span)
        })
}

/// A message from the browser.
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase", deny_unknown_fields)]
enum Request {
    Call {
        id: u32,
        method: String,
        #[serde(default)]
        params: Option<Value>,
    },
    Sub {
        topic: String,
    },
    Unsub {
        topic: String,
    },
}

/// Reads only the id of a message that is not a valid [`Request`], to answer it.
#[derive(Deserialize)]
struct AnyId {
    #[serde(default)]
    id: Option<u32>,
}

/// An error as the browser sees it: a stable `code` and plain words.
#[derive(Debug, Serialize)]
struct WireError {
    code: &'static str,
    message: String,
    /// The TrueNAS error name (`EACCES`, …) when the NAS refused.
    #[serde(skip_serializing_if = "Option::is_none")]
    errname: Option<String>,
}

impl WireError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            errname: None,
        }
    }

    fn from_nas(e: &truenas::Error) -> Self {
        use truenas::Error as E;
        let code = match e {
            E::Timeout => "timeout",
            E::Closed => "reconnecting",
            E::TooManyCalls => "busy",
            E::LoggedOut => "logged_out",
            E::Rpc(_)
            | E::Connect(_)
            | E::CertMismatch { .. }
            | E::Protocol(_)
            | E::Decode(_)
            | E::Auth(_) => "nas",
        };
        let errname = match e {
            E::Rpc(rpc) => rpc.data.as_ref().and_then(|d| d.errname.clone()),
            _ => None,
        };
        Self {
            code,
            message: e.to_string(),
            errname,
        }
    }

    fn busy() -> Self {
        Self::new("busy", "too many requests at once on this page; try again")
    }
}

/// The answer to a call.
#[derive(Serialize)]
#[serde(untagged)]
enum Reply {
    Ok { id: u32, result: Value },
    Err { id: Option<u32>, error: WireError },
}

/// A change in a subscribed collection.
#[derive(Serialize)]
struct TopicData<'a> {
    topic: &'a str,
    data: UpdateData<'a>,
}

#[derive(Serialize)]
struct UpdateData<'a> {
    kind: &'static str,
    id: &'a Option<Value>,
    fields: &'a Value,
}

/// Everything else a topic tells the browser.
#[derive(Serialize)]
struct TopicStatus<'a> {
    topic: &'a str,
    #[serde(flatten)]
    status: Status,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
enum Status {
    /// The subscription is live: query the current state now, changes follow.
    Subscribed,
    /// Subscribed again after a reconnect: events of the gap are lost, query again.
    Resumed,
    /// The browser read too slowly and missed `count` events: query again.
    Missed { count: u64 },
    /// Refused or ended by the NAS. Nothing follows.
    Ended { error: WireError },
}

#[derive(Serialize)]
struct LinkState {
    op: &'static str,
    state: &'static str,
}

fn encode(v: &impl Serialize) -> String {
    // Every message is built from strings, numbers and `Value`s: it always serializes.
    serde_json::to_string(v).expect("wire messages serialize")
}

fn reply_err(id: Option<u32>, error: WireError) -> String {
    encode(&Reply::Err { id, error })
}

fn status(topic: &str, status: Status) -> String {
    encode(&TopicStatus { topic, status })
}

fn data(topic: &str, u: &Update) -> String {
    let kind = match u.kind {
        UpdateKind::Added => "added",
        UpdateKind::Changed => "changed",
        UpdateKind::Removed => "removed",
        UpdateKind::Other => "other",
    };
    encode(&TopicData {
        topic,
        data: UpdateData {
            kind,
            id: &u.id,
            fields: &u.fields,
        },
    })
}

/// `None` once the session has ended.
fn link_state(link: &Link) -> Option<&'static str> {
    match link {
        Link::Up => Some("up"),
        Link::Reconnecting { .. } => Some("reconnecting"),
        Link::Ended(_) => None,
    }
}

fn link_message(state: &'static str) -> Message {
    Message::text(encode(&LinkState { op: "link", state }))
}

fn logged_out_close() -> Message {
    Message::Close(Some(CloseFrame {
        code: CLOSE_LOGGED_OUT,
        reason: Utf8Bytes::from_static("log in again"),
    }))
}

/// One browser socket.
struct Bridge {
    session: truenas::Session,
    allow: Allowlist,
    out_tx: mpsc::Sender<String>,
    calls: JoinSet<()>,
    /// Aborted, and so unsubscribed, when the socket closes.
    subs: JoinSet<()>,
    topics: HashMap<String, AbortHandle>,
    /// Keeps the session from going idle while the socket is open. Dropped last.
    _guard: WsGuard,
}

impl Bridge {
    async fn run(mut self, mut socket: WebSocket, mut out_rx: mpsc::Receiver<String>) {
        tracing::debug!("socket open");
        let mut link = self.session.link();
        let first = link_state(&link.borrow_and_update());
        let Some(first) = first else {
            let _ = socket.send(logged_out_close()).await;
            return;
        };
        if socket.send(link_message(first)).await.is_err() {
            return;
        }
        let mut ping =
            tokio::time::interval_at(tokio::time::Instant::now() + PING_EVERY, PING_EVERY);
        ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            // Every branch is cancel-safe: `recv`s and `changed` lose nothing when another
            // branch wins.
            tokio::select! {
                msg = socket.recv() => {
                    let Some(Ok(msg)) = msg else { break };
                    let reply = match msg {
                        Message::Text(text) => self.handle(text.as_str()),
                        Message::Binary(_) => Some(reply_err(
                            None,
                            WireError::new("bad_request", "send JSON text messages"),
                        )),
                        Message::Close(_) => break,
                        // Axum answers pings itself.
                        Message::Ping(_) | Message::Pong(_) => None,
                    };
                    if let Some(reply) = reply
                        && socket.send(Message::text(reply)).await.is_err()
                    {
                        break;
                    }
                }
                // Never `None`: `self` holds a sender.
                Some(text) = out_rx.recv() => {
                    if socket.send(Message::text(text)).await.is_err() {
                        break;
                    }
                }
                changed = link.changed() => {
                    let state = match changed {
                        Ok(()) => link_state(&link.borrow_and_update()),
                        Err(_) => None,
                    };
                    let Some(state) = state else {
                        tracing::debug!("session ended, closing");
                        let _ = socket.send(logged_out_close()).await;
                        break;
                    };
                    if socket.send(link_message(state)).await.is_err() {
                        break;
                    }
                }
                _ = ping.tick() => {
                    if socket.send(Message::Ping(Bytes::new())).await.is_err() {
                        break;
                    }
                }
            }
        }
        tracing::debug!("socket closed");
    }

    /// Handles one text message. Returns the immediate answer, if any; later answers come
    /// through `out_tx`. Never waits, so the loop never blocks on its own channel.
    fn handle(&mut self, text: &str) -> Option<String> {
        let req = match serde_json::from_str::<Request>(text) {
            Ok(req) => req,
            Err(e) => {
                let id = serde_json::from_str::<AnyId>(text).ok().and_then(|a| a.id);
                return Some(reply_err(
                    id,
                    WireError::new("bad_request", format!("bad request: {e}")),
                ));
            }
        };
        match req {
            Request::Call { id, method, params } => self.call(id, method, params),
            Request::Sub { topic } => self.sub(topic),
            Request::Unsub { topic } => {
                if let Some(task) = self.topics.remove(&topic) {
                    tracing::debug!(%topic, "unsubscribe");
                    task.abort();
                }
                None
            }
        }
    }

    fn call(&mut self, id: u32, method: String, params: Option<Value>) -> Option<String> {
        let params = params.unwrap_or_else(|| Value::Array(Vec::new()));
        if !params.is_array() {
            return Some(reply_err(
                Some(id),
                WireError::new("bad_request", "params must be a list"),
            ));
        }
        if !self.allow.method(&method) {
            tracing::warn!(%method, "refused: method not allowed");
            return Some(reply_err(
                Some(id),
                WireError::new("not_allowed", "truelite does not allow this action"),
            ));
        }
        while self.calls.try_join_next().is_some() {}
        if self.calls.len() >= MAX_CALLS {
            return Some(reply_err(Some(id), WireError::busy()));
        }
        let session = self.session.clone();
        let out = self.out_tx.clone();
        self.calls.spawn(
            async move {
                // Params are never logged: later phases send passwords and keys in them.
                let reply = match session.call(&method, params).await {
                    Ok(result) => Reply::Ok { id, result },
                    Err(e) => {
                        tracing::debug!(%method, error = %e, "call failed");
                        Reply::Err {
                            id: Some(id),
                            error: WireError::from_nas(&e),
                        }
                    }
                };
                // Fails only when the socket has closed.
                let _ = out.send(encode(&reply)).await;
            }
            .in_current_span(),
        );
        None
    }

    fn sub(&mut self, topic: String) -> Option<String> {
        if !self.allow.event(&topic) {
            tracing::warn!(%topic, "refused: event not allowed");
            let error = WireError::new("not_allowed", "truelite does not allow this event");
            return Some(status(&topic, Status::Ended { error }));
        }
        while self.subs.try_join_next().is_some() {}
        self.topics.retain(|_, task| !task.is_finished());
        if self.topics.contains_key(&topic) {
            return None;
        }
        if self.topics.len() >= MAX_TOPICS {
            let error = WireError::busy();
            return Some(status(&topic, Status::Ended { error }));
        }
        tracing::debug!(%topic, "subscribe");
        let task = self.subs.spawn(
            forward(self.session.clone(), topic.clone(), self.out_tx.clone()).in_current_span(),
        );
        self.topics.insert(topic, task);
        None
    }
}

/// Subscribes to `topic` and forwards its events until it ends, the socket closes or the task
/// is aborted (`unsub`). Dropping the subscription unsubscribes on the NAS.
async fn forward(session: truenas::Session, topic: String, out: mpsc::Sender<String>) {
    let mut sub = match session.subscribe(&topic).await {
        Ok(sub) => sub,
        Err(e) => {
            tracing::debug!(%topic, error = %e, "subscribe failed");
            let error = WireError::from_nas(&e);
            let _ = out.send(status(&topic, Status::Ended { error })).await;
            return;
        }
    };
    if out.send(status(&topic, Status::Subscribed)).await.is_err() {
        return;
    }
    while let Some(event) = sub.recv().await {
        let (msg, last) = match event {
            Event::Update(u) => (data(&topic, &u), false),
            Event::Resumed => (status(&topic, Status::Resumed), false),
            Event::Missed(count) => (status(&topic, Status::Missed { count }), false),
            Event::Ended(e) => {
                let error = WireError::from_nas(&e);
                (status(&topic, Status::Ended { error }), true)
            }
        };
        if out.send(msg).await.is_err() || last {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::time::Instant;

    use axum::body::Body;
    use axum::http::{Request as HttpRequest, StatusCode, header};
    use futures_util::{SinkExt, StreamExt};
    use serde_json::json;
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::{self, Message as WsMessage};
    use tower::ServiceExt;

    use super::*;
    use crate::AppState;
    use crate::login::Login;
    use crate::rate_limit::LimitOptions;
    use crate::sessions::{COOKIE, StoreOptions};
    use crate::test_client::{Client, call, close_code, event, next, send};
    use crate::truenas::SessionOptions;
    use crate::truenas::mock::{Mock, logged_in, within};

    const MIN: Duration = Duration::from_secs(60);

    struct Harness {
        mock: Mock,
        state: AppState,
        id: String,
        addr: SocketAddr,
    }

    impl Harness {
        async fn new() -> Self {
            Self::with(Allowlist::default()).await
        }

        /// A logged-in `admin` on the fake NAS, served on a real port.
        async fn with(allowlist: Allowlist) -> Self {
            let opts = SessionOptions {
                backoff_min: Duration::from_millis(20),
                backoff_max: Duration::from_millis(100),
                ..SessionOptions::default()
            };
            let (mock, _nas, session) = logged_in(opts).await;
            let state = AppState {
                sessions: SessionStore::new(StoreOptions::default()),
                login: Login::new(None, SessionOptions::default(), LimitOptions::default()),
                allowlist,
                origins: crate::origin::Origins::default(),
            };
            let id = state.sessions.insert(session).unwrap().as_str().to_owned();
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let app =
                crate::router(state.clone()).into_make_service_with_connect_info::<SocketAddr>();
            tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            Self {
                mock,
                state,
                id,
                addr,
            }
        }

        /// Opens `/ws` as the app's own page would.
        async fn connect(&self, cookie: Option<&str>) -> Result<Client, tungstenite::Error> {
            let origin = format!("http://{}", self.addr);
            self.connect_from(cookie, Some(&origin)).await
        }

        async fn connect_from(
            &self,
            cookie: Option<&str>,
            origin: Option<&str>,
        ) -> Result<Client, tungstenite::Error> {
            let mut req = format!("ws://{}/ws", self.addr)
                .into_client_request()
                .unwrap();
            if let Some(o) = origin {
                req.headers_mut().insert(header::ORIGIN, o.parse().unwrap());
            }
            if let Some(c) = cookie {
                let value = format!("{COOKIE}={c}").parse().unwrap();
                req.headers_mut().insert(header::COOKIE, value);
            }
            within(tokio_tungstenite::connect_async(req))
                .await
                .map(|(ws, _)| ws)
        }

        /// An open socket, past its first `link` message.
        async fn open(&self) -> Client {
            let mut ws = self.connect(Some(&self.id)).await.unwrap();
            assert_eq!(next(&mut ws).await, json!({"op": "link", "state": "up"}));
            ws
        }

        fn methods(&self) -> Vec<String> {
            self.mock.calls().into_iter().map(|(_, m)| m).collect()
        }

        async fn until_nas_saw(&self, method: &str) {
            within(async {
                while !self.methods().iter().any(|m| m == method) {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await;
        }
    }

    fn http_status(res: Result<Client, tungstenite::Error>) -> u16 {
        match res {
            Err(tungstenite::Error::Http(r)) => r.status().as_u16(),
            other => panic!("expected an HTTP refusal, got {:?}", other.map(|_| ())),
        }
    }

    /// Issue #17: another origin (same host, another port; another host; none) cannot open the
    /// socket, even with the cookie, and is refused before the session is looked at.
    #[tokio::test]
    async fn foreign_origins_cannot_open_the_socket() {
        let h = Harness::new().await;
        let other_port = format!("http://{}:{}", h.addr.ip(), h.addr.port() + 1);
        for origin in [
            Some(other_port.as_str()),
            Some("http://evil.example"),
            Some("null"),
            None,
        ] {
            let res = h.connect_from(Some(&h.id), origin).await;
            assert_eq!(http_status(res), 403, "{origin:?}");
            // A dead cookie would be 401: the origin is checked first.
            let res = h.connect_from(Some(&"a".repeat(64)), origin).await;
            assert_eq!(http_status(res), 403, "{origin:?}");
        }
        assert!(h.mock.calls().iter().all(|(_, m)| m != "core.subscribe"));
        // The real page still gets in.
        h.open().await;
    }

    /// Issue #16 "done when": a method outside the list never reaches the NAS.
    #[tokio::test]
    async fn a_method_outside_the_allowlist_is_refused() {
        let h = Harness::new().await;
        let mut ws = h.open().await;
        // `fast` exists on the fake NAS, `pool.create` would be an action: both stay here.
        for (id, method) in [(7, "pool.create"), (8, "fast"), (9, "system.reboot")] {
            let reply = call(&mut ws, id, method).await;
            assert_eq!(reply["id"], id);
            assert_eq!(reply["error"]["code"], "not_allowed", "{reply}");
        }
        // A listed call after them goes through, so the NAS has seen everything sent before.
        assert_eq!(call(&mut ws, 10, "system.info").await["id"], 10);
        let seen = h.methods();
        assert!(seen.contains(&"system.info".to_owned()));
        for m in ["pool.create", "fast", "system.reboot"] {
            assert!(!seen.contains(&m.to_owned()), "{m} reached the NAS");
        }
    }

    #[tokio::test]
    async fn an_allowed_call_returns_the_nas_result() {
        let h = Harness::new().await;
        let mut ws = h.open().await;
        let reply = call(&mut ws, 1, "system.info").await;
        assert_eq!(reply, json!({"id": 1, "result": {"hostname": "truenas"}}));
        let req = h.mock.requests().pop().unwrap();
        assert_eq!(
            (req.method.as_str(), req.params),
            ("system.info", json!([]))
        );
    }

    #[tokio::test]
    async fn replies_come_back_as_they_finish_with_their_ids() {
        static METHODS: &[&str] = &["slow", "fast", "never"];
        let h = Harness::with(Allowlist::new(METHODS, &[])).await;
        let mut ws = h.open().await;
        send(
            &mut ws,
            json!({"op": "call", "id": 1, "method": "slow", "params": []}),
        )
        .await;
        send(&mut ws, json!({"op": "call", "id": 2, "method": "fast"})).await;
        assert_eq!(next(&mut ws).await, json!({"id": 2, "result": "fast"}));
        assert_eq!(next(&mut ws).await, json!({"id": 1, "result": "slow"}));
    }

    #[tokio::test]
    async fn nas_errors_keep_their_name() {
        static METHODS: &[&str] = &["missing"];
        let h = Harness::with(Allowlist::new(METHODS, &[])).await;
        let mut ws = h.open().await;
        let reply = call(&mut ws, 3, "missing").await;
        assert_eq!(reply["id"], 3);
        assert_eq!(reply["error"]["code"], "nas");
        assert!(
            reply["error"]["message"]
                .as_str()
                .unwrap()
                .contains("-32601")
        );
    }

    #[tokio::test]
    async fn bad_requests_get_bad_request_and_the_socket_stays_open() {
        let h = Harness::new().await;
        let mut ws = h.open().await;
        let bad = [
            (Value::Null, "not json".to_owned()),
            (
                json!(3),
                json!({"op": "call", "id": 3, "method": "system.info", "params": {"a": 1}})
                    .to_string(),
            ),
            (json!(4), json!({"op": "nope", "id": 4}).to_string()),
            (
                json!(5),
                json!({"op": "call", "id": 5, "method": "system.info", "jsonrpc": "2.0"})
                    .to_string(),
            ),
            (
                Value::Null,
                json!({"op": "call", "id": "x", "method": "system.info"}).to_string(),
            ),
            (Value::Null, json!({"op": "sub"}).to_string()),
        ];
        for (id, text) in bad {
            ws.send(WsMessage::text(text.clone())).await.unwrap();
            let reply = next(&mut ws).await;
            assert_eq!(reply["id"], id, "{text}");
            assert_eq!(reply["error"]["code"], "bad_request", "{text}");
        }
        ws.send(WsMessage::binary(vec![1, 2, 3])).await.unwrap();
        assert_eq!(next(&mut ws).await["error"]["code"], "bad_request");
        assert_eq!(call(&mut ws, 6, "system.info").await["id"], 6);
    }

    #[tokio::test]
    async fn a_message_over_the_limit_closes_the_socket() {
        let h = Harness::new().await;
        let mut ws = h.open().await;
        let huge = json!({"op": "call", "id": 1, "method": "system.info",
            "params": ["x".repeat(MAX_MESSAGE)]});
        let _ = ws.send(WsMessage::text(huge.to_string())).await;
        within(async {
            while let Some(Ok(m)) = ws.next().await {
                assert!(!m.is_text(), "answered an oversized message: {m:?}");
            }
        })
        .await;
    }

    #[tokio::test]
    async fn no_cookie_or_a_dead_session_is_401() {
        let h = Harness::new().await;
        for cookie in [None, Some("a".repeat(64))] {
            match h.connect(cookie.as_deref()).await {
                Err(tungstenite::Error::Http(res)) => {
                    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
                }
                other => panic!("expected a 401, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn sub_forwards_updates_then_unsub_unsubscribes() {
        let h = Harness::new().await;
        let mut ws = h.open().await;
        send(&mut ws, json!({"op": "sub", "topic": "alert.list"})).await;
        assert_eq!(
            next(&mut ws).await,
            json!({"topic": "alert.list", "status": "subscribed"})
        );
        // Subscribing twice on one socket changes nothing.
        send(&mut ws, json!({"op": "sub", "topic": "alert.list"})).await;
        h.mock.push(event("disk.query", 1));
        h.mock.push(event("alert.list", 2));
        assert_eq!(
            next(&mut ws).await,
            json!({"topic": "alert.list", "data": {"kind": "added", "id": 2, "fields": {"n": 2}}})
        );
        let subscribes = h
            .methods()
            .iter()
            .filter(|m| *m == "core.subscribe")
            .count();
        assert_eq!(subscribes, 1);

        send(&mut ws, json!({"op": "sub", "topic": "pool.query"})).await;
        let refused = next(&mut ws).await;
        assert_eq!(
            (
                &refused["topic"],
                &refused["status"],
                &refused["error"]["code"]
            ),
            (&json!("pool.query"), &json!("ended"), &json!("not_allowed"))
        );

        send(&mut ws, json!({"op": "unsub", "topic": "alert.list"})).await;
        h.until_nas_saw("core.unsubscribe").await;
    }

    #[tokio::test]
    async fn an_event_the_nas_refuses_ends_with_its_name() {
        static EVENTS: &[&str] = &["pool.query"];
        let h = Harness::with(Allowlist::new(&[], EVENTS)).await;
        let mut ws = h.open().await;
        send(&mut ws, json!({"op": "sub", "topic": "pool.query"})).await;
        let ended = next(&mut ws).await;
        assert_eq!(ended["status"], "ended");
        assert_eq!(ended["error"]["code"], "nas");
        assert_eq!(ended["error"]["errname"], "EACCES");
    }

    #[tokio::test]
    async fn closing_the_socket_unsubscribes() {
        let h = Harness::new().await;
        let mut ws = h.open().await;
        send(&mut ws, json!({"op": "sub", "topic": "alert.list"})).await;
        assert_eq!(next(&mut ws).await["status"], "subscribed");
        ws.close(None).await.unwrap();
        h.until_nas_saw("core.unsubscribe").await;
    }

    /// Phase 2 "done when": the browser survives an upstream reconnect.
    #[tokio::test]
    async fn an_upstream_drop_shows_reconnecting_then_resumed() {
        let h = Harness::new().await;
        let mut ws = h.open().await;
        send(&mut ws, json!({"op": "sub", "topic": "alert.list"})).await;
        assert_eq!(next(&mut ws).await["status"], "subscribed");

        h.mock.set_down(true);
        h.mock.kill();
        assert_eq!(
            next(&mut ws).await,
            json!({"op": "link", "state": "reconnecting"})
        );
        h.mock.set_down(false);
        // `resumed` and the link going up travel separately: accept either order.
        let (mut up, mut resumed) = (false, false);
        while !(up && resumed) {
            let msg = next(&mut ws).await;
            match (msg["op"].as_str(), msg["status"].as_str()) {
                (Some("link"), _) => up = msg["state"] == "up",
                (_, Some("resumed")) => resumed = true,
                _ => panic!("unexpected {msg}"),
            }
        }
        assert_eq!(call(&mut ws, 1, "system.info").await["id"], 1);
        h.mock.push(event("alert.list", 3));
        assert_eq!(next(&mut ws).await["data"]["id"], 3);
    }

    #[tokio::test]
    async fn logout_closes_the_socket_with_4401() {
        let h = Harness::new().await;
        let mut ws = h.open().await;
        let logout = HttpRequest::post("/api/logout")
            .header(header::HOST, "truelite.test")
            .header(header::ORIGIN, "https://truelite.test")
            .header(header::COOKIE, format!("{COOKIE}={}", h.id))
            .body(Body::empty())
            .unwrap();
        let res = crate::router(h.state.clone())
            .oneshot(logout)
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        assert_eq!(close_code(&mut ws).await, CLOSE_LOGGED_OUT);
    }

    #[tokio::test]
    async fn an_open_socket_keeps_the_session_from_idling() {
        let h = Harness::new().await;
        let mut ws = h.open().await;
        h.state.sessions.sweep_at(Instant::now() + 60 * MIN);
        assert!(h.state.sessions.get(&h.id).is_some());
        assert_eq!(call(&mut ws, 1, "system.info").await["id"], 1);
    }
}
