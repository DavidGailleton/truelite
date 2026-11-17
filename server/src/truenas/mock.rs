//! In-process mock of the TrueNAS JSON-RPC WebSocket server, scripted per test.
//! Seed of #13: grows login, events and reconnect scenarios from here.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};

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

type Handler = Arc<dyn Fn(&str, &Value) -> Reply + Send + Sync>;
/// `None` drops the connection.
type Out = mpsc::UnboundedSender<Option<String>>;

#[derive(Default)]
struct State {
    path: Option<String>,
    calls: Vec<(Value, String)>,
    conn: Option<Out>,
}

pub struct Mock {
    pub url: String,
    state: Arc<Mutex<State>>,
}

impl Mock {
    pub async fn start(handler: impl Fn(&str, &Value) -> Reply + Send + Sync + 'static) -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(State::default()));
        let handler: Handler = Arc::new(handler);
        let st = state.clone();
        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                tokio::spawn(serve(tcp, st.clone(), handler.clone()));
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
        self.state.lock().unwrap().calls.clone()
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
async fn serve(tcp: tokio::net::TcpStream, state: Arc<Mutex<State>>, handler: Handler) {
    let st = state.clone();
    let record_path = move |req: &Request, res: Response| {
        st.lock().unwrap().path = Some(req.uri().path().to_owned());
        Ok(res)
    };
    let Ok(ws) = tokio_tungstenite::accept_hdr_async(tcp, record_path).await else {
        return;
    };
    let (mut sink, mut stream) = ws.split();
    let (out, mut out_rx) = mpsc::unbounded_channel::<Option<String>>();
    state.lock().unwrap().conn = Some(out.clone());
    loop {
        tokio::select! {
            msg = stream.next() => {
                let Some(Ok(Message::Text(t))) = msg else { return };
                let req: Value = serde_json::from_str(t.as_str()).unwrap();
                let id = req["id"].clone();
                let method = req["method"].as_str().unwrap_or_default().to_owned();
                state.lock().unwrap().calls.push((id.clone(), method.clone()));
                answer(handler(&method, &req["params"]), id, out.clone());
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
