//! End-to-end journeys through the whole server, as the web app drives it: log in over HTTP,
//! open `/ws`, call, subscribe, survive the NAS dropping, log out. Everything is real except the
//! NAS, which is the scripted [`fake_nas`](crate::truenas::mock::fake_nas). Run them alone with
//! `cargo test journey`.
//!
//! They live in the crate, not in `tests/`, because truelite is a binary: an integration test
//! could not reach the mock or `router()`.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::allowlist::Allowlist;
use crate::login::{Login, OTP_COOKIE};
use crate::origin::Origins;
use crate::rate_limit::LimitOptions;
use crate::sessions::{COOKIE, SessionStore, StoreOptions};
use crate::test_client::{Client, HttpReply, call, close_code, event, http, next, send};
use crate::truenas::mock::{Mock, Nas, fake_nas, within};
use crate::truenas::{Endpoint, SessionOptions};
use crate::ws::CLOSE_LOGGED_OUT;
use crate::{AppState, router};

/// Address of the test browser.
const BROWSER: &str = "192.0.2.10:50000";

/// The server on a real port, its NAS logins going to a fake NAS.
struct Journey {
    mock: Mock,
    nas: Arc<Mutex<Nas>>,
    state: AppState,
    addr: SocketAddr,
}

impl Journey {
    async fn start() -> Self {
        let (nas, handler) = fake_nas();
        let mock = Mock::start_conn(handler).await;
        let ep = Endpoint::new(mock.url.clone(), None).unwrap();
        // Reconnect fast so the drop scenarios take milliseconds.
        let opts = SessionOptions {
            backoff_min: Duration::from_millis(20),
            backoff_max: Duration::from_millis(100),
            ..SessionOptions::default()
        };
        let state = AppState {
            sessions: SessionStore::new(StoreOptions::default()),
            login: Login::new(Some(ep), opts, LimitOptions::default()),
            allowlist: Allowlist::default(),
            origins: Origins::default(),
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = router(state.clone()).into_make_service_with_connect_info::<SocketAddr>();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            mock,
            nas,
            state,
            addr,
        }
    }

    /// An HTTP request from the browser; `cookie` is a full `name=value` pair.
    async fn http(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
        cookie: Option<&str>,
    ) -> HttpReply {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(c) = cookie {
            req = req.header(header::COOKIE, c);
        }
        let body = match body {
            Some(b) => {
                req = req.header(header::CONTENT_TYPE, "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        http(router(self.state.clone()), req.body(body).unwrap(), BROWSER).await
    }

    /// `POST /api/login` with a password.
    async fn log_in(&self, user: &str, password: &str) -> HttpReply {
        self.http(
            "POST",
            "/api/login",
            Some(json!({"username": user, "password": password})),
            None,
        )
        .await
    }

    async fn session(&self, cookie: &str) -> HttpReply {
        self.http("GET", "/api/session", None, Some(cookie)).await
    }

    /// Opens `/ws` from the app's own page and waits for the link to be up.
    async fn open(&self, cookie: &str) -> Client {
        let mut req = format!("ws://{}/ws", self.addr)
            .into_client_request()
            .unwrap();
        let h = req.headers_mut();
        h.insert(
            header::ORIGIN,
            format!("http://{}", self.addr).parse().unwrap(),
        );
        h.insert(header::COOKIE, cookie.parse().unwrap());
        let (mut ws, _) = within(tokio_tungstenite::connect_async(req)).await.unwrap();
        assert_eq!(next(&mut ws).await, json!({"op": "link", "state": "up"}));
        ws
    }

    /// `(method, first param)` of every call on NAS connection `conn`.
    fn calls_on(&self, conn: usize) -> Vec<(String, Value)> {
        self.mock
            .requests()
            .into_iter()
            .filter(|c| c.conn == conn)
            .map(|c| (c.method, c.params[0].clone()))
            .collect()
    }

    fn nas_saw(&self, method: &str) -> bool {
        self.mock.calls().iter().any(|(_, m)| m == method)
    }
}

/// The session cookie a login reply sets, as a `name=value` pair.
fn session_cookie(reply: &HttpReply) -> String {
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    format!(
        "{COOKIE}={}",
        reply.cookie(COOKIE).expect("a session cookie")
    )
}

/// Reads until the link is up and the subscription `resumed`, in either order.
async fn until_resumed(ws: &mut Client) {
    let (mut up, mut resumed) = (false, false);
    while !(up && resumed) {
        let msg = next(ws).await;
        match (msg["op"].as_str(), msg["status"].as_str()) {
            (Some("link"), _) => up = msg["state"] == "up",
            (_, Some("resumed")) => resumed = true,
            _ => panic!("unexpected {msg}"),
        }
    }
}

/// Phase 2 "done when": a user logs in, sees the shell page and survives an upstream reconnect.
#[tokio::test]
async fn log_in_reconnect_and_log_out() {
    let j = Journey::start().await;

    // Log in, then the server knows who we are and serves the app.
    let cookie = session_cookie(&j.log_in("admin", "secret").await);
    assert_eq!(j.session(&cookie).await.body["username"], "admin");
    let page = j.http("GET", "/", None, Some(&cookie)).await;
    assert_eq!(page.status, StatusCode::OK);
    let ct = page.headers[header::CONTENT_TYPE].to_str().unwrap();
    assert!(ct.starts_with("text/html"), "{ct}");

    // The socket: an allowed call reaches the NAS, a refused one does not.
    let mut ws = j.open(&cookie).await;
    let info = call(&mut ws, 1, "system.info").await;
    assert_eq!(info, json!({"id": 1, "result": {"hostname": "truenas"}}));
    let refused = call(&mut ws, 2, "system.reboot").await;
    assert_eq!(refused["error"]["code"], "not_allowed", "{refused}");
    assert!(!j.nas_saw("system.reboot"));

    // An event subscription delivers what the NAS sends.
    send(&mut ws, json!({"op": "sub", "topic": "alert.list"})).await;
    assert_eq!(next(&mut ws).await["status"], "subscribed");
    j.mock.push(event("alert.list", 1));
    assert_eq!(next(&mut ws).await["data"]["id"], 1);

    // The NAS drops and stays down for a moment: the browser sees it, then the link comes back.
    j.mock.set_down(true);
    j.mock.kill();
    assert_eq!(
        next(&mut ws).await,
        json!({"op": "link", "state": "reconnecting"})
    );
    j.mock.set_down(false);
    until_resumed(&mut ws).await;

    // The new NAS connection logged in with a token, never the password, and resubscribed.
    let second = j.calls_on(2);
    assert_eq!(second[0].0, "auth.login_ex");
    assert_eq!(second[0].1["mechanism"], "TOKEN_PLAIN");
    assert!(
        second
            .iter()
            .any(|(m, p)| m == "core.subscribe" && p == "alert.list")
    );
    let passwords = j
        .mock
        .requests()
        .into_iter()
        .filter(|c| c.params[0]["mechanism"] == "PASSWORD_PLAIN")
        .count();
    assert_eq!(passwords, 1, "the password went out again");

    // Calls and events work again on the same socket.
    assert_eq!(
        call(&mut ws, 3, "system.info").await["result"]["hostname"],
        "truenas"
    );
    j.mock.push(event("alert.list", 2));
    assert_eq!(next(&mut ws).await["data"]["id"], 2);

    // Logging out closes the socket, ends the NAS session and the cookie.
    let out = j.http("POST", "/api/logout", None, Some(&cookie)).await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    assert_eq!(close_code(&mut ws).await, CLOSE_LOGGED_OUT);
    assert!(j.nas_saw("auth.logout"));
    assert_eq!(j.session(&cookie).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn one_time_code_login_reaches_the_bridge() {
    let j = Journey::start().await;
    let first = j.log_in("otp", "secret").await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.body);
    assert_eq!(first.cookie(COOKIE), None, "no session before the code");
    let ticket = first.cookie(OTP_COOKIE).expect("a one-time code ticket");

    let done = j
        .http(
            "POST",
            "/api/login/otp",
            Some(json!({"code": "123456"})),
            Some(&format!("{OTP_COOKIE}={ticket}")),
        )
        .await;
    let cookie = session_cookie(&done);
    assert_eq!(j.session(&cookie).await.body["username"], "otp");

    let mut ws = j.open(&cookie).await;
    assert_eq!(
        call(&mut ws, 1, "system.info").await["result"]["hostname"],
        "truenas"
    );
}

#[tokio::test]
async fn a_nas_that_refuses_the_token_logs_the_browser_out() {
    let j = Journey::start().await;
    let cookie = session_cookie(&j.log_in("admin", "secret").await);
    let mut ws = j.open(&cookie).await;

    j.nas.lock().unwrap().reject_tokens = true;
    j.mock.kill();
    assert_eq!(
        next(&mut ws).await,
        json!({"op": "link", "state": "reconnecting"})
    );
    assert_eq!(close_code(&mut ws).await, CLOSE_LOGGED_OUT);
    assert_eq!(j.session(&cookie).await.status, StatusCode::UNAUTHORIZED);
}
