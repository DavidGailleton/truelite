//! WebSocket JSON-RPC client: one connection, one actor task, calls matched by id.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{Connector, connect_async_tls_with_config};

use super::API_PATH;
use super::error::Error;
use super::rpc::{Incoming, Request};

type Reply = oneshot::Sender<Result<Value, Error>>;

/// Server-to-client JSON-RPC notification (`collection_update`, `notify_unsubscribed`).
/// `params` stays raw: the docs' schema and example disagree, event code interprets it.
#[derive(Debug, Clone)]
pub struct Notification {
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone)]
pub struct Options {
    /// Timeout for opening the socket (TCP + TLS + upgrade).
    pub connect_timeout: Duration,
    /// Default timeout of `call`. Job methods use `call_with_timeout(.., None)`.
    pub call_timeout: Duration,
    /// Notifications buffered for a slow consumer before new ones are dropped.
    pub notification_buffer: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            call_timeout: Duration::from_secs(30),
            notification_buffer: 256,
        }
    }
}

enum Command {
    Call { id: u64, text: String, reply: Reply },
    Cancel(u64),
}

/// Handle to one upstream connection. Cheap to clone; the socket closes when every handle is
/// dropped or the NAS goes away.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

struct Inner {
    tx: mpsc::Sender<Command>,
    next_id: AtomicU64,
    call_timeout: Duration,
    closed: watch::Receiver<bool>,
}

impl Client {
    /// Connects to `base` (`wss://nas.local` or `ws://host:port`) at the pinned [`API_PATH`].
    /// `tls` is the rustls config for `wss://` (the fingerprint-pinning verifier goes there);
    /// without it only certificates from an empty root store are accepted, i.e. none.
    pub async fn connect(
        base: &str,
        tls: Option<Arc<rustls::ClientConfig>>,
        opts: Options,
    ) -> Result<(Client, mpsc::Receiver<Notification>), Error> {
        let base = base.trim_end_matches('/');
        if !(base.starts_with("ws://") || base.starts_with("wss://")) {
            return Err(Error::Connect(format!(
                "NAS URL must start with ws:// or wss://, got {base:?}"
            )));
        }
        let url = format!("{base}{API_PATH}");
        let (ws, _) = tokio::time::timeout(
            opts.connect_timeout,
            connect_async_tls_with_config(url.as_str(), None, true, tls.map(Connector::Rustls)),
        )
        .await
        .map_err(|_| Error::Connect(format!("{url}: timed out")))?
        .map_err(|e| Error::Connect(format!("{url}: {e}")))?;
        tracing::debug!("connected to {url}");

        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let (notif_tx, notif_rx) = mpsc::channel(opts.notification_buffer.max(1));
        let (closed_tx, closed_rx) = watch::channel(false);
        tokio::spawn(run(ws, cmd_rx, notif_tx, closed_tx));

        let client = Client {
            inner: Arc::new(Inner {
                tx: cmd_tx,
                next_id: AtomicU64::new(1),
                call_timeout: opts.call_timeout,
                closed: closed_rx,
            }),
        };
        Ok((client, notif_rx))
    }

    /// Calls `method` with positional `params` (a JSON array) and the default timeout.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, Error> {
        self.call_with_timeout(method, params, Some(self.inner.call_timeout))
            .await
    }

    /// Like [`call`](Self::call), deserializing the result.
    pub async fn call_as<T: DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
    ) -> Result<T, Error> {
        serde_json::from_value(self.call(method, params).await?).map_err(Error::Decode)
    }

    /// `timeout: None` waits until the reply or the connection drops (job methods).
    pub async fn call_with_timeout(
        &self,
        method: &str,
        params: Value,
        timeout: Option<Duration>,
    ) -> Result<Value, Error> {
        let (_, reply) = self.start_call(method, params, timeout).await?;
        reply.await
    }

    /// Sends the call and returns its JSON-RPC id with a future for the reply. Jobs report
    /// progress through `core.get_jobs` events whose `message_ids` hold this id.
    pub async fn start_call(
        &self,
        method: &str,
        params: Value,
        timeout: Option<Duration>,
    ) -> Result<
        (
            u64,
            impl Future<Output = Result<Value, Error>> + Send + 'static,
        ),
        Error,
    > {
        if !params.is_array() {
            return Err(Error::Protocol("params must be a JSON array".into()));
        }
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let text = serde_json::to_string(&Request {
            jsonrpc: "2.0",
            id,
            method,
            params: &params,
        })
        .map_err(|e| Error::Protocol(e.to_string()))?;
        let (reply, rx) = oneshot::channel();
        self.inner
            .tx
            .send(Command::Call { id, text, reply })
            .await
            .map_err(|_| Error::Closed)?;

        let tx = self.inner.tx.clone();
        let fut = async move {
            let res = match timeout {
                Some(d) => match tokio::time::timeout(d, rx).await {
                    Ok(r) => r,
                    Err(_) => {
                        let _ = tx.send(Command::Cancel(id)).await;
                        return Err(Error::Timeout);
                    }
                },
                None => rx.await,
            };
            res.unwrap_or(Err(Error::Closed))
        };
        Ok((id, fut))
    }

    pub fn is_closed(&self) -> bool {
        *self.inner.closed.borrow()
    }

    /// Resolves once the connection is gone (the reconnect logic waits on this).
    pub async fn closed(&self) {
        let mut rx = self.inner.closed.clone();
        let _ = rx.wait_for(|c| *c).await;
    }
}

/// The actor: owns the socket and the table of pending calls.
async fn run<S>(
    ws: tokio_tungstenite::WebSocketStream<S>,
    mut cmds: mpsc::Receiver<Command>,
    notifs: mpsc::Sender<Notification>,
    closed: watch::Sender<bool>,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut sink, mut stream) = ws.split();
    let mut pending: HashMap<u64, Reply> = HashMap::new();
    loop {
        tokio::select! {
            cmd = cmds.recv() => match cmd {
                Some(Command::Call { id, text, reply }) => {
                    if let Err(e) = sink.send(Message::text(text)).await {
                        tracing::warn!("NAS socket write failed: {e}");
                        let _ = reply.send(Err(Error::Closed));
                        break;
                    }
                    pending.insert(id, reply);
                }
                Some(Command::Cancel(id)) => {
                    pending.remove(&id);
                }
                None => {
                    let _ = sink.close().await;
                    break;
                }
            },
            msg = stream.next() => match msg {
                Some(Ok(Message::Text(t))) => handle(t.as_str(), &mut pending, &notifs),
                Some(Ok(Message::Close(_))) | None => break,
                // Pings are answered by tungstenite; binary frames are not part of the API.
                Some(Ok(_)) => {}
                Some(Err(e)) => {
                    tracing::warn!("NAS socket error: {e}");
                    break;
                }
            },
        }
    }
    for (_, reply) in pending.drain() {
        let _ = reply.send(Err(Error::Closed));
    }
    let _ = closed.send(true);
    tracing::debug!("NAS connection closed");
}

fn handle(text: &str, pending: &mut HashMap<u64, Reply>, notifs: &mpsc::Sender<Notification>) {
    let msg: Incoming = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("unparsable message from the NAS: {e}");
            return;
        }
    };
    match (msg.id, msg.method) {
        (Some(id), _) => {
            let Some(reply) = id.as_u64().and_then(|id| pending.remove(&id)) else {
                tracing::debug!("reply with unknown id {id}");
                return;
            };
            let res = match msg.error {
                Some(e) if e.code == -32000 => Err(Error::TooManyCalls),
                Some(e) => Err(Error::Rpc(Box::new(e))),
                None => Ok(msg.result.unwrap_or(Value::Null)),
            };
            let _ = reply.send(res);
        }
        (None, Some(method)) => {
            let n = Notification {
                method,
                params: msg.params,
            };
            if let Err(mpsc::error::TrySendError::Full(n)) = notifs.try_send(n) {
                tracing::warn!("notification buffer full, dropping {}", n.method);
            }
        }
        (None, None) => tracing::debug!("NAS message with neither id nor method"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::mock::{Mock, Reply as R};
    use super::*;
    use serde_json::json;

    fn opts(call_timeout_ms: u64) -> Options {
        Options {
            call_timeout: Duration::from_millis(call_timeout_ms),
            ..Options::default()
        }
    }

    async fn within<F: Future>(f: F) -> F::Output {
        tokio::time::timeout(Duration::from_secs(5), f)
            .await
            .expect("test step timed out")
    }

    fn standard(method: &str, _params: &Value) -> R {
        match method {
            "core.ping" => R::Result(json!("pong")),
            "fast" => R::Result(json!("fast")),
            "slow" => R::Delay(
                Duration::from_millis(200),
                Box::new(R::Result(json!("slow"))),
            ),
            "never" => R::Ignore,
            "boom" => R::Drop,
            "missing" => R::Error(json!({"code":-32001,"message":"method call error","data":{
                "error":2,"errname":"ENOENT","reason":"pool not found","trace":null,
                "extra":[],"py_exception":"Traceback ..."}})),
            "busy" => R::Error(json!({"code":-32000,"message":"too many concurrent calls"})),
            "odd" => R::Raw(vec![
                r#"{"jsonrpc":"2.0","id":"abc","result":1}"#.into(),
                r#"{"jsonrpc":"2.0","id":{id},"error":{"code":-32601,"message":null}}"#.into(),
            ]),
            "app.upgrade" => R::Delay(Duration::from_millis(300), Box::new(R::Result(json!(true)))),
            _ => R::Error(json!({"code":-32601,"message":"Method not found"})),
        }
    }

    #[tokio::test]
    async fn call_ok() {
        let mock = Mock::start(standard).await;
        let (c, _n) = Client::connect(&mock.url, None, opts(1000)).await.unwrap();
        let pong: String = within(c.call_as("core.ping", json!([]))).await.unwrap();
        assert_eq!(pong, "pong");
        assert_eq!(mock.path().as_deref(), Some(API_PATH));
        assert_eq!(mock.calls()[0].1, "core.ping");
    }

    #[tokio::test]
    async fn out_of_order() {
        let mock = Mock::start(standard).await;
        let (c, _n) = Client::connect(&mock.url, None, opts(1000)).await.unwrap();
        let (slow, fast) =
            within(async { tokio::join!(c.call("slow", json!([])), c.call("fast", json!([]))) })
                .await;
        assert_eq!(slow.unwrap(), "slow");
        assert_eq!(fast.unwrap(), "fast");
    }

    #[tokio::test]
    async fn rpc_error_typed() {
        let mock = Mock::start(standard).await;
        let (c, _n) = Client::connect(&mock.url, None, opts(1000)).await.unwrap();
        let err = within(c.call("missing", json!([]))).await.unwrap_err();
        let Error::Rpc(e) = &err else {
            panic!("expected Rpc, got {err:?}")
        };
        assert_eq!(e.code, -32001);
        let d = e.data.as_ref().unwrap();
        assert_eq!(d.errname.as_deref(), Some("ENOENT"));
        assert_eq!(d.error, Some(2));
        assert_eq!(err.to_string(), "ENOENT: pool not found");
        let busy = within(c.call("busy", json!([]))).await.unwrap_err();
        assert!(matches!(busy, Error::TooManyCalls), "{busy:?}");
    }

    #[tokio::test]
    async fn timeout() {
        let mock = Mock::start(standard).await;
        let (c, _n) = Client::connect(&mock.url, None, opts(100)).await.unwrap();
        let err = within(c.call("never", json!([]))).await.unwrap_err();
        assert!(matches!(err, Error::Timeout), "{err:?}");
        // The connection is still usable after a timed-out call.
        assert_eq!(within(c.call("fast", json!([]))).await.unwrap(), "fast");
    }

    #[tokio::test]
    async fn drop_fails_pending() {
        let mock = Mock::start(standard).await;
        let (c, mut n) = Client::connect(&mock.url, None, opts(1000)).await.unwrap();
        let pending = tokio::spawn({
            let c = c.clone();
            async move { c.call("never", json!([])).await }
        });
        // Let "never" reach the mock before dropping the connection.
        within(c.call("fast", json!([]))).await.unwrap();
        let err = within(c.call("boom", json!([]))).await.unwrap_err();
        assert!(matches!(err, Error::Closed), "{err:?}");
        let err = within(pending).await.unwrap().unwrap_err();
        assert!(matches!(err, Error::Closed), "{err:?}");
        within(c.closed()).await;
        assert!(c.is_closed());
        assert!(within(n.recv()).await.is_none());
        let err = c.call("fast", json!([])).await.unwrap_err();
        assert!(matches!(err, Error::Closed), "{err:?}");
    }

    #[tokio::test]
    async fn notification_forwarded() {
        let mock = Mock::start(standard).await;
        let (c, mut n) = Client::connect(&mock.url, None, opts(1000)).await.unwrap();
        within(c.call("core.ping", json!([]))).await.unwrap();
        mock.push(
            json!({"jsonrpc":"2.0","method":"collection_update","params":{
            "msg":"changed","collection":"alert.list","id":"a1","fields":{"level":"CRITICAL"}}}),
        );
        let note = within(n.recv()).await.unwrap();
        assert_eq!(note.method, "collection_update");
        assert_eq!(note.params["collection"], "alert.list");
    }

    #[tokio::test]
    async fn connect_refused() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let err = Client::connect(&format!("ws://{addr}"), None, opts(1000))
            .await
            .err()
            .unwrap();
        assert!(matches!(err, Error::Connect(_)), "{err:?}");
        let err = Client::connect("https://nas.local", None, opts(1000))
            .await
            .err()
            .unwrap();
        assert!(matches!(err, Error::Connect(_)), "{err:?}");
    }

    #[tokio::test]
    async fn string_id_and_null_message() {
        let mock = Mock::start(standard).await;
        let (c, _n) = Client::connect(&mock.url, None, opts(1000)).await.unwrap();
        let err = within(c.call("odd", json!([]))).await.unwrap_err();
        let Error::Rpc(e) = &err else {
            panic!("expected Rpc, got {err:?}")
        };
        assert_eq!((e.code, e.message.as_deref()), (-32601, None));
        assert!(matches!(
            c.call("x", json!({"not": "an array"})).await,
            Err(Error::Protocol(_))
        ));
    }

    #[tokio::test]
    async fn job_no_timeout_and_id() {
        let mock = Mock::start(standard).await;
        let (c, _n) = Client::connect(&mock.url, None, opts(100)).await.unwrap();
        let (id, reply) = c
            .start_call("app.upgrade", json!(["plex"]), None)
            .await
            .unwrap();
        assert_eq!(within(reply).await.unwrap(), json!(true));
        assert!(
            mock.calls()
                .contains(&(json!(id), "app.upgrade".to_owned()))
        );
    }
}
