//! What the browser does, for tests: HTTP requests through the router and a WebSocket client on
//! `/ws`. Shared by the `login`, `ws` and journey tests.

use std::net::SocketAddr;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode, header};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tower::ServiceExt;

use crate::truenas::mock::within;

/// Host the HTTP test requests are sent to; the app's page is `http://truelite.test`.
pub const HOST: &str = "truelite.test";

/// A browser WebSocket on `/ws`.
pub type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// An HTTP response, body parsed as JSON (`Null` when it is not JSON).
pub struct HttpReply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
}

impl HttpReply {
    /// The value of the `Set-Cookie` for `name` (empty when it deletes the cookie).
    pub fn cookie(&self, name: &str) -> Option<String> {
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|v| v.to_str().unwrap())
            .find_map(|v| {
                let (pair, _) = v.split_once(';')?;
                let (k, val) = pair.split_once('=')?;
                (k == name).then(|| val.to_owned())
            })
    }
}

/// Sends `req` from the client address `from` (`ip:port`), to [`HOST`], as the app's own page
/// unless it sets `Origin` itself.
pub async fn http(router: Router, mut req: Request<Body>, from: &str) -> HttpReply {
    let h = req.headers_mut();
    h.insert(header::HOST, HeaderValue::from_static(HOST));
    if !h.contains_key(header::ORIGIN) {
        h.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://truelite.test"),
        );
    }
    let addr: SocketAddr = from.parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(addr));
    let res = within(router.oneshot(req)).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = to_bytes(res.into_body(), 64 * 1024).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    HttpReply {
        status,
        headers,
        body,
    }
}

pub async fn send(ws: &mut Client, msg: Value) {
    ws.send(Message::text(msg.to_string())).await.unwrap();
}

/// The next JSON message, skipping pings.
pub async fn next(ws: &mut Client) -> Value {
    loop {
        match within(ws.next()).await.expect("socket ended").unwrap() {
            Message::Text(t) => return serde_json::from_str(t.as_str()).unwrap(),
            Message::Ping(_) | Message::Pong(_) => {}
            other => panic!("expected JSON, got {other:?}"),
        }
    }
}

/// Calls `method` without params and returns the reply.
pub async fn call(ws: &mut Client, id: u32, method: &str) -> Value {
    send(
        ws,
        serde_json::json!({"op": "call", "id": id, "method": method}),
    )
    .await;
    next(ws).await
}

/// The close code the server ends the socket with, skipping messages before it.
pub async fn close_code(ws: &mut Client) -> u16 {
    let frame = within(async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Close(frame))) => return frame,
                Some(Ok(_)) => {}
                other => panic!("expected a close frame, got {other:?}"),
            }
        }
    })
    .await
    .expect("close frame has a code");
    u16::from(frame.code)
}

/// A `collection_update` notification from the NAS.
pub fn event(collection: &str, n: i64) -> Value {
    serde_json::json!({"jsonrpc": "2.0", "method": "collection_update", "params": {
        "msg": "added", "collection": collection, "id": n, "fields": {"n": n}}})
}
