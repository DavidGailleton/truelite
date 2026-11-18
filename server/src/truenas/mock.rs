//! In-process mock of the TrueNAS JSON-RPC WebSocket server, scripted per test (#13).
//!
//! [`Mock`] serves `ws://` or `wss://` (self-signed, or an impostor without the certificate's
//! key) and answers each call with a [`Reply`]: a result, an error, raw frames, a delay, no
//! answer or a dropped connection. Tests can push notifications, kill the connection, keep the
//! NAS down, and read back every call it received.
//!
//! [`fake_nas`] is a scripted NAS (passwords, one-time codes, tokens, subscriptions) used by
//! the session, login and bridge tests, and end to end by `journey_tests`.

use std::collections::HashSet;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};

use super::auth::{LoginStep, Secret, login};
use super::session::{Endpoint, Session, SessionOptions};
use super::tls::Fingerprint;

#[derive(Debug)]
struct OneCert(Arc<CertifiedKey>);

impl ResolvesServerCert for OneCert {
    fn resolve(&self, _: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(self.0.clone())
    }
}

/// How the mock answers one call.
pub enum Reply {
    Result(Value),
    /// A JSON-RPC error object.
    Error(Value),
    /// Raw frames sent as-is; `{id}` is replaced by the call id.
    Raw(Vec<String>),
    Delay(Duration, Box<Reply>),
    /// Never answer.
    Ignore,
    /// Drop the connection without a close frame.
    Drop,
}

/// Gets the connection number (1 for the first accepted socket), the method and the params.
type Handler = Arc<dyn Fn(usize, &str, &Value) -> Reply + Send + Sync>;
/// `None` drops the connection.
type Out = mpsc::UnboundedSender<Option<String>>;

/// One call received by the mock.
#[derive(Debug, Clone)]
pub struct Call {
    pub conn: usize,
    pub id: Value,
    pub method: String,
    pub params: Value,
}

#[derive(Default)]
struct State {
    path: Option<String>,
    calls: Vec<Call>,
    conn: Option<Out>,
    conns: usize,
    open: usize,
    /// Refuse new sockets (the NAS is unreachable).
    down: bool,
    refused: Vec<Instant>,
}

/// Bounds a test step so a bug fails the test instead of hanging it.
pub async fn within<F: Future>(f: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(5), f)
        .await
        .expect("test step timed out")
}

pub struct Mock {
    pub url: String,
    state: Arc<Mutex<State>>,
}

impl Mock {
    /// Plain `ws://127.0.0.1:port` mock.
    pub async fn start(handler: impl Fn(&str, &Value) -> Reply + Send + Sync + 'static) -> Mock {
        Self::start_conn(move |_, m, p| handler(m, p)).await
    }

    /// Like [`start`](Self::start), the handler also gets the connection number.
    pub async fn start_conn(
        handler: impl Fn(usize, &str, &Value) -> Reply + Send + Sync + 'static,
    ) -> Mock {
        Self::listen(Arc::new(handler), None).await
    }

    /// `wss://127.0.0.1:port` mock with a fresh self-signed certificate; returns its
    /// fingerprint to pin. Like a real NAS, the certificate's name (`truenas.local`) matches
    /// none of the addresses the client uses.
    pub async fn start_tls(
        handler: impl Fn(&str, &Value) -> Reply + Send + Sync + 'static,
    ) -> (Mock, Fingerprint) {
        Self::start_tls_inner(Arc::new(move |_, m: &str, p: &Value| handler(m, p)), false).await
    }

    /// Like [`start_tls`](Self::start_tls), but signs the handshake with another key than the
    /// certificate's: an attacker replaying the NAS's public certificate.
    pub async fn start_tls_impostor(
        handler: impl Fn(&str, &Value) -> Reply + Send + Sync + 'static,
    ) -> (Mock, Fingerprint) {
        Self::start_tls_inner(Arc::new(move |_, m: &str, p: &Value| handler(m, p)), true).await
    }

    async fn start_tls_inner(handler: Handler, wrong_key: bool) -> (Mock, Fingerprint) {
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = rcgen::CertificateParams::new(vec!["truenas.local".into()])
            .unwrap()
            .self_signed(&key)
            .unwrap();
        let fingerprint = Fingerprint::of(cert.der());
        let signing = if wrong_key {
            rcgen::KeyPair::generate().unwrap()
        } else {
            key
        };
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let signing_key = provider
            .key_provider
            .load_private_key(PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
                signing.serialize_der(),
            )))
            .unwrap();
        // `CertifiedKey::new` does not check that the key matches the certificate, which the
        // impostor needs; `with_single_cert` would refuse it.
        let certified = Arc::new(CertifiedKey::new(vec![cert.der().clone()], signing_key));
        let config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(OneCert(certified)));
        let acceptor = TlsAcceptor::from(Arc::new(config));
        (Self::listen(handler, Some(acceptor)).await, fingerprint)
    }

    async fn listen(handler: Handler, tls: Option<TlsAcceptor>) -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = match tls {
            Some(_) => format!("wss://{addr}"),
            None => format!("ws://{addr}"),
        };
        let state = Arc::new(Mutex::new(State::default()));
        let st = state.clone();
        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                {
                    let mut s = st.lock().unwrap();
                    if s.down {
                        s.refused.push(Instant::now());
                        continue;
                    }
                }
                let (st, handler) = (st.clone(), handler.clone());
                match tls.clone() {
                    None => {
                        tokio::spawn(serve(tcp, st, handler));
                    }
                    Some(acceptor) => {
                        tokio::spawn(async move {
                            // A client that refuses our certificate aborts the handshake.
                            if let Ok(stream) = acceptor.accept(tcp).await {
                                serve(stream, st, handler).await;
                            }
                        });
                    }
                }
            }
        });
        Mock { url, state }
    }

    /// Path of the last WebSocket upgrade request.
    pub fn path(&self) -> Option<String> {
        self.state.lock().unwrap().path.clone()
    }

    /// `(id, method)` of every call received, in order.
    pub fn calls(&self) -> Vec<(Value, String)> {
        self.requests()
            .into_iter()
            .map(|c| (c.id, c.method))
            .collect()
    }

    /// Every call received, in order, with its connection and params.
    pub fn requests(&self) -> Vec<Call> {
        self.state.lock().unwrap().calls.clone()
    }

    /// Drops the current connection without a close frame (middlewared restarting).
    pub fn kill(&self) {
        if let Some(out) = &self.state.lock().unwrap().conn {
            let _ = out.send(None);
        }
    }

    /// While down, new sockets are closed right after `accept` and their time recorded.
    pub fn set_down(&self, down: bool) {
        self.state.lock().unwrap().down = down;
    }

    /// When each refused socket arrived.
    pub fn refused(&self) -> Vec<Instant> {
        self.state.lock().unwrap().refused.clone()
    }

    /// Resolves once no connection is open.
    pub async fn disconnected(&self) {
        while self.state.lock().unwrap().open > 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    /// Sends a raw message (e.g. a notification) on the current connection.
    pub fn push(&self, msg: Value) {
        let st = self.state.lock().unwrap();
        st.conn
            .as_ref()
            .expect("no connection")
            .send(Some(msg.to_string()))
            .unwrap();
    }
}

// The handshake callback's error type is tungstenite's, not ours.
#[allow(clippy::result_large_err)]
async fn serve<S>(tcp: S, state: Arc<Mutex<State>>, handler: Handler)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let st = state.clone();
    let record_path = move |req: &Request, res: Response| {
        st.lock().unwrap().path = Some(req.uri().path().to_owned());
        Ok(res)
    };
    let Ok(ws) = tokio_tungstenite::accept_hdr_async(tcp, record_path).await else {
        return;
    };
    let (out, mut out_rx) = mpsc::unbounded_channel::<Option<String>>();
    let conn = {
        let mut s = state.lock().unwrap();
        s.conn = Some(out.clone());
        s.conns += 1;
        s.open += 1;
        s.conns
    };
    serve_ws(ws, conn, &state, &handler, out, &mut out_rx).await;
    state.lock().unwrap().open -= 1;
}

async fn serve_ws<S>(
    ws: tokio_tungstenite::WebSocketStream<S>,
    conn: usize,
    state: &Mutex<State>,
    handler: &Handler,
    out: Out,
    out_rx: &mut mpsc::UnboundedReceiver<Option<String>>,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut sink, mut stream) = ws.split();
    loop {
        tokio::select! {
            msg = stream.next() => {
                let Some(Ok(Message::Text(t))) = msg else { return };
                let req: Value = serde_json::from_str(t.as_str()).unwrap();
                let id = req["id"].clone();
                let method = req["method"].as_str().unwrap_or_default().to_owned();
                let params = req["params"].clone();
                state.lock().unwrap().calls.push(Call {
                    conn,
                    id: id.clone(),
                    method: method.clone(),
                    params: params.clone(),
                });
                answer(handler(conn, &method, &params), id, out.clone());
            }
            o = out_rx.recv() => match o {
                Some(Some(text)) => {
                    if sink.send(Message::text(text)).await.is_err() {
                        return;
                    }
                }
                _ => return,
            },
        }
    }
}

fn answer(reply: Reply, id: Value, out: Out) {
    let frame = |v: Value| Some(v.to_string());
    match reply {
        Reply::Result(r) => {
            let _ = out.send(frame(json!({"jsonrpc":"2.0","id":id,"result":r})));
        }
        Reply::Error(e) => {
            let _ = out.send(frame(json!({"jsonrpc":"2.0","id":id,"error":e})));
        }
        Reply::Raw(frames) => {
            for f in frames {
                let _ = out.send(Some(f.replace("{id}", &id.to_string())));
            }
        }
        Reply::Delay(d, inner) => {
            tokio::spawn(async move {
                tokio::time::sleep(d).await;
                answer(*inner, id, out);
            });
        }
        Reply::Ignore => {}
        Reply::Drop => {
            let _ = out.send(None);
        }
    }
}

/// A scripted NAS: users `admin`/`secret`, `otp`/`secret` + code `123456`, `noui`/`secret`
/// (no web UI access), `old` (expired), `elsewhere` (redirect).
#[derive(Default)]
pub struct Nas {
    pub authed: HashSet<usize>,
    pub otp_pending: HashSet<usize>,
    pub tokens: Vec<String>,
    pub reject_tokens: bool,
    pub subscriptions: usize,
}

fn user_info(name: &str) -> Value {
    json!({"pw_name": name, "pw_uid": 1000, "privilege": {
        "roles": {"$set": ["READONLY_ADMIN"]}, "webui_access": name != "noui"}})
}

fn not_authenticated() -> Reply {
    Reply::Error(
        json!({"code": -32001, "message": "Method call error", "data": {
        "error": 207, "errname": "ENOTAUTHENTICATED",
        "reason": "[ENOTAUTHENTICATED] Not authenticated"}}),
    )
}

pub fn fake_nas() -> (
    Arc<Mutex<Nas>>,
    impl Fn(usize, &str, &Value) -> Reply + Send + Sync,
) {
    let nas = Arc::new(Mutex::new(Nas::default()));
    let st = nas.clone();
    let handler = move |conn: usize, method: &str, params: &Value| {
        let mut nas = st.lock().unwrap();
        let p = &params[0];
        let success = |nas: &mut Nas, name: &str, level: &str| {
            nas.authed.insert(conn);
            Reply::Result(json!({"response_type": "SUCCESS", "authenticator": level,
                "user_info": user_info(name)}))
        };
        match method {
            "auth.login_ex" => match (p["mechanism"].as_str(), p["username"].as_str()) {
                (Some("PASSWORD_PLAIN"), Some(user)) => match (user, p["password"].as_str()) {
                    ("admin", Some("secret")) => success(&mut nas, "admin", "LEVEL_1"),
                    ("noui", Some("secret")) => success(&mut nas, "noui", "LEVEL_1"),
                    ("otp", Some("secret")) => {
                        nas.otp_pending.insert(conn);
                        Reply::Result(json!({"response_type": "OTP_REQUIRED", "username": "otp"}))
                    }
                    ("old", _) => Reply::Result(json!({"response_type": "EXPIRED"})),
                    ("elsewhere", _) => {
                        Reply::Result(json!({"response_type": "REDIRECT", "urls": ["wss://b"]}))
                    }
                    _ => Reply::Result(json!({"response_type": "AUTH_ERR"})),
                },
                (Some("TOKEN_PLAIN"), _) => {
                    let ok = !nas.reject_tokens
                        && p["token"]
                            .as_str()
                            .is_some_and(|t| nas.tokens.iter().any(|k| k == t));
                    if ok {
                        success(&mut nas, "admin", "LEVEL_1")
                    } else {
                        Reply::Result(json!({"response_type": "AUTH_ERR"}))
                    }
                }
                _ => Reply::Error(json!({"code": -32602, "message": "Invalid params"})),
            },
            "auth.login_ex_continue" if nas.otp_pending.contains(&conn) => {
                if p["otp_token"] == "123456" {
                    nas.otp_pending.remove(&conn);
                    success(&mut nas, "otp", "LEVEL_2")
                } else {
                    Reply::Result(json!({"response_type": "OTP_REQUIRED", "username": "otp"}))
                }
            }
            _ if !nas.authed.contains(&conn) => not_authenticated(),
            "auth.generate_token" => {
                let t = format!("tok-{}", nas.tokens.len() + 1);
                nas.tokens.push(t.clone());
                Reply::Result(json!(t))
            }
            "auth.logout" => {
                nas.authed.remove(&conn);
                Reply::Result(Value::Null)
            }
            "core.subscribe" if *p == "pool.query" => Reply::Error(
                json!({"code": -32001, "message": "Method call error", "data": {
                "error": 13, "errname": "EACCES", "reason": "Not authorized"}}),
            ),
            "core.subscribe" => {
                nas.subscriptions += 1;
                Reply::Result(json!(format!("sub-{}", nas.subscriptions)))
            }
            "core.unsubscribe" => Reply::Result(Value::Null),
            "system.info" => Reply::Result(json!({"hostname": "truenas"})),
            "alert.list" => Reply::Result(json!([])),
            "fast" => Reply::Result(json!("fast")),
            "slow" => Reply::Delay(
                Duration::from_millis(300),
                Box::new(Reply::Result(json!("slow"))),
            ),
            "never" => Reply::Ignore,
            _ => Reply::Error(json!({"code": -32601, "message": "Method not found"})),
        }
    };
    (nas, handler)
}

/// A fake NAS ([`fake_nas`]) and a session logged in on it as `admin`.
pub async fn logged_in(opts: SessionOptions) -> (Mock, Arc<Mutex<Nas>>, Session) {
    let (nas, handler) = fake_nas();
    let mock = Mock::start_conn(handler).await;
    let ep = Endpoint::new(mock.url.clone(), None).unwrap();
    match within(login(&ep, opts, "admin", Secret::new("secret"))).await {
        Ok(LoginStep::Done(s)) => (mock, nas, s),
        other => panic!("login failed: {other:?}"),
    }
}
