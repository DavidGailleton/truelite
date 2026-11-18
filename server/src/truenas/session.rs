//! A logged-in NAS connection that heals itself: when the socket drops, a supervisor task
//! reconnects with exponential backoff and logs in again with a token, never with the password.
//!
//! Calls are never resent: one in flight when the socket dies fails with [`Error::Closed`];
//! one made while reconnecting waits (up to the call timeout) for the new connection.

use std::fmt;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::Instant;

use super::auth::{self, Secret, UserInfo};
use super::client::{Client, Notification, Options};
use super::error::{AuthFailure, Error};
use super::events::{Release, Subscription, Topics};
use super::tls::{Fingerprint, client_config};

/// Where the NAS is and how to trust it.
#[derive(Clone)]
pub struct Endpoint {
    url: String,
    tls: Option<Arc<rustls::ClientConfig>>,
}

impl fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Endpoint")
            .field("url", &self.url)
            .field("pinned", &self.tls.is_some())
            .finish()
    }
}

impl Endpoint {
    /// `url` as in [`Client::connect`]; `pin` is required for `wss://`.
    ///
    /// # Errors
    ///
    /// [`Error::Connect`] if the TLS configuration cannot be built.
    pub fn new(url: impl Into<String>, pin: Option<Fingerprint>) -> Result<Self, Error> {
        let tls = pin
            .map(client_config)
            .transpose()
            .map_err(|e| Error::Connect(format!("TLS setup: {e}")))?;
        Ok(Self {
            url: url.into(),
            tls,
        })
    }

    pub(super) async fn connect(
        &self,
        opts: &SessionOptions,
    ) -> Result<(Client, mpsc::Receiver<Notification>), Error> {
        Client::connect(&self.url, self.tls.clone(), opts.client.clone()).await
    }
}

#[derive(Debug, Clone)]
pub struct SessionOptions {
    pub client: Options,
    /// Lifetime asked for each token (sent in whole seconds). A drop longer than this since
    /// the last refresh ends the session.
    pub token_ttl: Duration,
    /// How often a fresh token replaces the current one while connected.
    pub token_refresh: Duration,
    /// First reconnect delay; doubles per failed attempt up to `backoff_max`. TrueNAS locks
    /// out an address after 20 auth requests in 60 s, so keep these well above that rate.
    pub backoff_min: Duration,
    pub backoff_max: Duration,
    /// Events kept per subscribed event name for a slow consumer before it is told it missed
    /// some ([`Event::Missed`](super::Event::Missed)).
    pub event_buffer: usize,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            client: Options::default(),
            token_ttl: Duration::from_secs(600),
            token_refresh: Duration::from_secs(300),
            backoff_min: Duration::from_secs(1),
            backoff_max: Duration::from_secs(30),
            event_buffer: 64,
        }
    }
}

/// State of the upstream link, for the "reconnecting to NAS" UI. Subscriptions are made again
/// before the link is `Up` after a reconnect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    Up,
    Reconnecting { attempt: u32 },
    Ended(EndReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    /// [`Session::logout`] was called.
    LoggedOut,
    /// The NAS no longer accepts the token (expired, revoked, NAS rebooted): log in again.
    SessionLost,
    /// The NAS now presents another certificate than the pinned one.
    CertMismatch,
}

enum Cmd {
    Logout(oneshot::Sender<()>),
    Subscribe {
        name: String,
        /// Events kept for a slow consumer; `None` = [`SessionOptions::event_buffer`].
        buffer: Option<usize>,
        reply: oneshot::Sender<Result<Subscription, Error>>,
    },
    #[cfg(test)]
    KillUpstream,
}

/// Handle to a logged-in session. Cheap to clone; the connection closes when every handle is
/// dropped.
#[derive(Clone)]
pub struct Session {
    inner: Arc<Shared>,
}

struct Shared {
    user: UserInfo,
    link: watch::Receiver<Link>,
    client: watch::Receiver<Option<Client>>,
    cmds: mpsc::Sender<Cmd>,
    call_timeout: Duration,
    notifications: Mutex<Option<mpsc::Receiver<Notification>>>,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("user", &self.inner.user.username)
            .field("link", &*self.inner.link.borrow())
            .finish_non_exhaustive()
    }
}

impl Session {
    /// Takes over a connection on which `auth.login_ex` just succeeded.
    pub(super) async fn start(
        endpoint: Endpoint,
        opts: SessionOptions,
        client: Client,
        notifs: mpsc::Receiver<Notification>,
        user: UserInfo,
    ) -> Result<Session, Error> {
        let token = auth::generate_token(&client, ttl_secs(&opts)).await?;
        let (link_tx, link_rx) = watch::channel(Link::Up);
        let (client_tx, client_rx) = watch::channel(Some(client.clone()));
        let (cmd_tx, cmd_rx) = mpsc::channel(8);
        let (out_tx, out_rx) = mpsc::channel(opts.client.notification_buffer.max(1));
        let call_timeout = opts.client.call_timeout;
        let (release_tx, releases) = mpsc::unbounded_channel();
        let topics = Topics::new(opts.event_buffer, release_tx);
        let sup = Supervisor {
            endpoint,
            opts,
            token,
            token_at: Instant::now(),
            link: link_tx,
            client: client_tx,
            cmds: cmd_rx,
            out: out_tx,
            topics,
            releases,
        };
        tokio::spawn(sup.run(client, notifs));
        tracing::info!(user = %user.username, "logged in to the NAS");
        Ok(Session {
            inner: Arc::new(Shared {
                user,
                link: link_rx,
                client: client_rx,
                cmds: cmd_tx,
                call_timeout,
                notifications: Mutex::new(Some(out_rx)),
            }),
        })
    }

    pub fn user(&self) -> &UserInfo {
        &self.inner.user
    }

    /// Watch the link state.
    pub fn link(&self) -> watch::Receiver<Link> {
        self.inner.link.clone()
    }

    /// The session's notifications that are not events of a [`Subscription`], across
    /// reconnects. Only the first caller gets them.
    pub fn take_notifications(&self) -> Option<mpsc::Receiver<Notification>> {
        self.inner
            .notifications
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    /// See [`Client::call`].
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, Error> {
        self.client().await?.call(method, params).await
    }

    /// See [`Client::call_as`].
    pub async fn call_as<T: DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
    ) -> Result<T, Error> {
        self.client().await?.call_as(method, params).await
    }

    /// See [`Client::start_call`].
    pub async fn start_call(
        &self,
        method: &str,
        params: Value,
        timeout: Option<Duration>,
    ) -> Result<
        (
            u64,
            impl Future<Output = Result<Value, Error>> + Send + use<>,
        ),
        Error,
    > {
        self.client()
            .await?
            .start_call(method, params, timeout)
            .await
    }

    /// Subscribes to `event` (`alert.list`, `reporting.realtime`, …). Handles of the same event
    /// share one subscription on the NAS; dropping the last one unsubscribes. After a reconnect
    /// the subscription is made again and handles get [`Event::Resumed`](super::Event::Resumed).
    ///
    /// # Errors
    ///
    /// [`Error::Rpc`] when the NAS refuses the event (unknown, or a role is missing),
    /// [`Error::LoggedOut`] when the session has ended, and the errors of [`Session::call`].
    pub async fn subscribe(&self, event: &str) -> Result<Subscription, Error> {
        self.subscribe_with_buffer(event, None).await
    }

    /// [`subscribe`](Self::subscribe), keeping up to `buffer` events for a slow consumer instead
    /// of [`SessionOptions::event_buffer`], for a stream that comes in bursts (a log tail). Only
    /// the handle that creates the topic sets it.
    ///
    /// # Errors
    ///
    /// As [`subscribe`](Self::subscribe).
    pub async fn subscribe_with_buffer(
        &self,
        event: &str,
        buffer: Option<usize>,
    ) -> Result<Subscription, Error> {
        self.client().await?;
        let (reply, rx) = oneshot::channel();
        self.inner
            .cmds
            .send(Cmd::Subscribe {
                name: event.to_owned(),
                buffer,
                reply,
            })
            .await
            .map_err(|_| Error::LoggedOut)?;
        rx.await.map_err(|_| Error::LoggedOut)?
    }

    /// Logs out on the NAS (`auth.logout`) and ends the session for every handle.
    pub async fn logout(&self) {
        let (done, wait) = oneshot::channel();
        if self.inner.cmds.send(Cmd::Logout(done)).await.is_ok() {
            let _ = wait.await;
        }
    }

    /// The current connection, waiting for a reconnect up to the call timeout.
    async fn client(&self) -> Result<Client, Error> {
        let mut link = self.inner.link.clone();
        let state = tokio::time::timeout(
            self.inner.call_timeout,
            link.wait_for(|l| !matches!(l, Link::Reconnecting { .. })),
        )
        .await
        .map_err(|_| Error::Closed)?
        .map(|l| l.clone())
        .map_err(|_| Error::LoggedOut)?;
        match state {
            Link::Up => self.inner.client.borrow().clone().ok_or(Error::Closed),
            Link::Ended(_) => Err(Error::LoggedOut),
            Link::Reconnecting { .. } => unreachable!("waited for another state"),
        }
    }

    /// Closes the upstream socket as if the NAS dropped it (smoke test against a real NAS).
    #[cfg(test)]
    pub(super) async fn kill_upstream(&self) {
        let _ = self.inner.cmds.send(Cmd::KillUpstream).await;
    }
}

fn ttl_secs(opts: &SessionOptions) -> u64 {
    opts.token_ttl.as_secs().max(1)
}

/// What ended a connected period.
enum Disconnect {
    Dropped,
    /// Logout or every handle gone: stop.
    Stop,
}

/// Why one reconnect attempt failed.
enum Attempt {
    Retry(Error),
    Fatal(EndReason),
}

struct Supervisor {
    endpoint: Endpoint,
    opts: SessionOptions,
    token: Secret,
    token_at: Instant,
    link: watch::Sender<Link>,
    client: watch::Sender<Option<Client>>,
    cmds: mpsc::Receiver<Cmd>,
    out: mpsc::Sender<Notification>,
    topics: Topics,
    releases: mpsc::UnboundedReceiver<Release>,
}

impl Supervisor {
    async fn run(mut self, mut client: Client, mut notifs: mpsc::Receiver<Notification>) {
        loop {
            match self.connected(&client, &mut notifs).await {
                Disconnect::Stop => return,
                Disconnect::Dropped => {}
            }
            tracing::warn!("lost the NAS connection, reconnecting");
            match self.reconnect().await {
                Some((c, n)) => (client, notifs) = (c, n),
                None => return,
            }
        }
    }

    /// Serves one connection until it drops, the user logs out or every handle is gone.
    async fn connected(
        &mut self,
        client: &Client,
        notifs: &mut mpsc::Receiver<Notification>,
    ) -> Disconnect {
        let every = self.opts.token_refresh;
        let mut refresh = tokio::time::interval_at(Instant::now() + every, every);
        loop {
            tokio::select! {
                // The client's actor drops the notification sender when its socket closes, so
                // `None` here is the disconnect signal, after buffered events are delivered.
                n = notifs.recv() => match n {
                    Some(n) => {
                        let Some(n) = self.topics.dispatch(n) else { continue };
                        if let Err(mpsc::error::TrySendError::Full(n)) = self.out.try_send(n) {
                            tracing::warn!("session notification buffer full, dropping {}", n.method);
                        }
                    }
                    None => return Disconnect::Dropped,
                },
                Some(r) = self.releases.recv() => self.release(Some(client), r).await,
                _ = refresh.tick() => self.refresh_token(client).await,
                cmd = self.cmds.recv() => match cmd {
                    Some(Cmd::Logout(done)) => {
                        let res = client.call("auth.logout", json!([])).await;
                        if let Err(e) = res {
                            tracing::debug!("auth.logout failed: {e}");
                        }
                        self.end(EndReason::LoggedOut);
                        let _ = done.send(());
                        return Disconnect::Stop;
                    }
                    Some(Cmd::Subscribe { name, buffer, reply }) => {
                        let res = self.subscribe(Some(client), &name, buffer).await;
                        let _ = reply.send(res);
                    }
                    #[cfg(test)]
                    Some(Cmd::KillUpstream) => client.kill().await,
                    None => {
                        tracing::debug!("every session handle dropped, closing");
                        return Disconnect::Stop;
                    }
                },
            }
        }
    }

    async fn refresh_token(&mut self, client: &Client) {
        match auth::generate_token(client, ttl_secs(&self.opts)).await {
            Ok(token) => {
                self.token = token;
                self.token_at = Instant::now();
            }
            // The current token is still valid until its TTL; the next tick retries.
            Err(e) => tracing::warn!("could not refresh the NAS token: {e}"),
        }
    }

    /// Reconnects with backoff until it works, the session can no longer be resumed, or the
    /// user logs out meanwhile.
    async fn reconnect(&mut self) -> Option<(Client, mpsc::Receiver<Notification>)> {
        self.client.send_replace(None);
        self.topics.mark_down();
        let mut attempt: u32 = 0;
        loop {
            attempt = attempt.saturating_add(1);
            if self.token_at.elapsed() >= self.opts.token_ttl {
                tracing::warn!("NAS token expired while disconnected");
                self.end(EndReason::SessionLost);
                return None;
            }
            self.link.send_replace(Link::Reconnecting { attempt });
            match self.attempt().await {
                Ok((client, notifs)) => {
                    tracing::info!(attempt, "reconnected to the NAS");
                    self.resubscribe(&client).await;
                    self.client.send_replace(Some(client.clone()));
                    self.link.send_replace(Link::Up);
                    return Some((client, notifs));
                }
                Err(Attempt::Fatal(reason)) => {
                    tracing::warn!(?reason, "cannot resume the NAS session");
                    self.end(reason);
                    return None;
                }
                Err(Attempt::Retry(e)) => {
                    let delay = backoff(&self.opts, attempt);
                    tracing::debug!(attempt, ?delay, "reconnect failed: {e}");
                    tokio::select! {
                        () = tokio::time::sleep(delay) => {}
                        cmd = self.cmds.recv() => match cmd {
                            Some(Cmd::Logout(done)) => {
                                self.end(EndReason::LoggedOut);
                                let _ = done.send(());
                                return None;
                            }
                            // Sent just before the link dropped: made upstream once back.
                            Some(Cmd::Subscribe { name, buffer, reply }) => {
                                let res = self.subscribe(None, &name, buffer).await;
                                let _ = reply.send(res);
                            }
                            #[cfg(test)]
                            Some(Cmd::KillUpstream) => {}
                            None => return None,
                        },
                        Some(r) = self.releases.recv() => self.release(None, r).await,
                    }
                }
            }
        }
    }

    async fn attempt(&mut self) -> Result<(Client, mpsc::Receiver<Notification>), Attempt> {
        let (client, notifs) = self
            .endpoint
            .connect(&self.opts)
            .await
            .map_err(|e| match e {
                Error::CertMismatch { got } => {
                    tracing::error!("{}", Error::CertMismatch { got });
                    Attempt::Fatal(EndReason::CertMismatch)
                }
                e => Attempt::Retry(e),
            })?;
        auth::login_with_token(&client, &self.token)
            .await
            .map_err(|e| match e {
                Error::Auth(AuthFailure::BadCredentials | AuthFailure::Expired) => {
                    Attempt::Fatal(EndReason::SessionLost)
                }
                e => Attempt::Retry(e),
            })?;
        self.refresh_token(&client).await;
        Ok((client, notifs))
    }

    /// A handle on `name`, subscribing upstream first if it is new. `client` is `None` while
    /// disconnected: the topic is then subscribed by [`resubscribe`](Self::resubscribe).
    async fn subscribe(
        &mut self,
        client: Option<&Client>,
        name: &str,
        buffer: Option<usize>,
    ) -> Result<Subscription, Error> {
        if let Some(sub) = self.topics.join(name) {
            return Ok(sub);
        }
        let upstream = match client {
            Some(c) => Some(c.call_as::<String>("core.subscribe", json!([name])).await?),
            None => None,
        };
        tracing::debug!(event = name, "subscribed");
        Ok(self.topics.insert(name, upstream, buffer))
    }

    async fn release(&mut self, client: Option<&Client>, r: Release) {
        let Some(id) = self.topics.release(r) else {
            return;
        };
        if let Some(c) = client
            && let Err(e) = c.call("core.unsubscribe", json!([id])).await
        {
            tracing::debug!("core.unsubscribe failed: {e}");
        }
    }

    /// Makes every subscription again on a new connection.
    async fn resubscribe(&mut self, client: &Client) {
        for name in self.topics.names() {
            match client
                .call_as::<String>("core.subscribe", json!([&*name]))
                .await
            {
                Ok(id) => self.topics.resumed(&name, id),
                // Dropped again: the next reconnect retries.
                Err(Error::Closed) => {}
                Err(e) => {
                    tracing::warn!(event = %name, "cannot subscribe again: {e}");
                    self.topics.end(&name, e);
                }
            }
        }
    }

    fn end(&self, reason: EndReason) {
        self.client.send_replace(None);
        self.link.send_replace(Link::Ended(reason));
    }
}

/// `min · 2^(attempt-1)`, capped at `max`, with ±20 % jitter so many sessions do not reconnect
/// in lockstep.
fn backoff(opts: &SessionOptions, attempt: u32) -> Duration {
    let exp = attempt.saturating_sub(1).min(16);
    let base = opts
        .backoff_min
        .saturating_mul(1 << exp)
        .min(opts.backoff_max);
    let mut byte = [0u8; 1];
    let jitter = match ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut byte) {
        Ok(()) => 0.8 + 0.4 * f64::from(byte[0]) / 255.0,
        Err(_) => 1.0,
    };
    base.mul_f64(jitter)
}

#[cfg(test)]
mod tests {
    use super::super::auth::{LoginStep, login};
    use super::super::events::{Event, Update, UpdateKind};
    use super::super::mock::{Mock, Nas, fake_nas, within};
    use super::*;

    fn opts() -> SessionOptions {
        SessionOptions {
            client: Options {
                call_timeout: Duration::from_secs(2),
                ..Options::default()
            },
            token_ttl: Duration::from_secs(60),
            token_refresh: Duration::from_secs(60),
            backoff_min: Duration::from_millis(20),
            backoff_max: Duration::from_millis(400),
            event_buffer: 64,
        }
    }

    async fn start() -> (Mock, Arc<Mutex<Nas>>, Endpoint) {
        let (nas, handler) = fake_nas();
        let mock = Mock::start_conn(handler).await;
        let ep = Endpoint::new(mock.url.clone(), None).unwrap();
        (mock, nas, ep)
    }

    async fn admin(ep: &Endpoint, opts: SessionOptions) -> Session {
        match within(login(ep, opts, "admin", Secret::new("secret"))).await {
            Ok(LoginStep::Done(s)) => s,
            other => panic!("login failed: {other:?}"),
        }
    }

    async fn wait_link(s: &Session, want: impl Fn(&Link) -> bool) {
        let mut rx = s.link();
        within(rx.wait_for(|l| want(l))).await.unwrap();
    }

    /// Runs `drop_socket`, then waits until the link went down and came back `Up`. Watching
    /// versions, not values: a fast reconnect may hide the `Reconnecting` value.
    async fn reconnected<F: Future>(s: &Session, drop_socket: F) {
        let mut rx = s.link();
        rx.borrow_and_update();
        drop_socket.await;
        within(async {
            loop {
                rx.changed().await.unwrap();
                if *rx.borrow_and_update() == Link::Up {
                    break;
                }
            }
        })
        .await;
    }

    /// Methods received on connection `conn`.
    fn methods_on(mock: &Mock, conn: usize) -> Vec<String> {
        mock.requests()
            .into_iter()
            .filter(|r| r.conn == conn)
            .map(|r| r.method)
            .collect()
    }

    #[tokio::test]
    async fn password_login_then_token() {
        let (mock, _nas, ep) = start().await;
        let s = admin(&ep, opts()).await;
        assert_eq!(s.user().username, "admin");
        assert_eq!(s.user().roles, ["READONLY_ADMIN"]);
        assert!(!s.user().two_factor);
        assert_eq!(within(s.call("fast", json!([]))).await.unwrap(), "fast");
        assert_eq!(
            methods_on(&mock, 1),
            ["auth.login_ex", "auth.generate_token", "fast"]
        );
        assert_eq!(*s.link().borrow(), Link::Up);
    }

    #[tokio::test]
    async fn refused_logins_are_typed() {
        let (_mock, _nas, ep) = start().await;
        for (user, want) in [
            ("admin", AuthFailure::BadCredentials),
            ("old", AuthFailure::Expired),
            ("elsewhere", AuthFailure::Redirect(vec!["wss://b".into()])),
        ] {
            let res = within(login(&ep, opts(), user, Secret::new("wrong"))).await;
            match res {
                Err(Error::Auth(f)) => assert_eq!(f, want, "{user}"),
                other => panic!("{user}: {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn one_time_code_on_the_same_connection() {
        let (mock, _nas, ep) = start().await;
        let step = within(login(&ep, opts(), "otp", Secret::new("secret")))
            .await
            .unwrap();
        let LoginStep::OtpRequired(pending) = step else {
            panic!("expected OtpRequired")
        };
        assert_eq!(pending.username(), "otp");
        let LoginStep::OtpRequired(pending) = within(pending.submit("000000")).await.unwrap()
        else {
            panic!("a wrong code asks again")
        };
        let LoginStep::Done(s) = within(pending.submit("123456")).await.unwrap() else {
            panic!("the right code logs in")
        };
        assert!(s.user().two_factor);
        assert_eq!(
            methods_on(&mock, 1),
            [
                "auth.login_ex",
                "auth.login_ex_continue",
                "auth.login_ex_continue",
                "auth.generate_token"
            ]
        );
    }

    /// Issue #11 "done when": killing the upstream socket reconnects without asking again.
    #[tokio::test]
    async fn reconnects_with_the_token_after_a_drop() {
        let (mock, nas, ep) = start().await;
        let s = admin(&ep, opts()).await;
        reconnected(&s, async { mock.kill() }).await;
        assert_eq!(within(s.call("fast", json!([]))).await.unwrap(), "fast");

        let second: Vec<_> = mock
            .requests()
            .into_iter()
            .filter(|r| r.conn == 2)
            .collect();
        assert_eq!(second[0].method, "auth.login_ex");
        assert_eq!(second[0].params[0]["mechanism"], "TOKEN_PLAIN");
        assert_eq!(second[0].params[0]["token"], "tok-1");
        assert!(
            mock.requests()
                .iter()
                .filter(|r| r.params[0]["mechanism"] == "PASSWORD_PLAIN")
                .count()
                == 1,
            "the password is sent once, at login"
        );
        // A fresh token was made on the new connection.
        assert_eq!(nas.lock().unwrap().tokens.len(), 2);
    }

    #[tokio::test]
    async fn backs_off_while_the_nas_is_down_then_recovers() {
        let (mock, _nas, ep) = start().await;
        let s = admin(&ep, opts()).await;
        mock.set_down(true);
        mock.kill();
        within(async {
            while mock.refused().len() < 5 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        mock.set_down(false);
        wait_link(&s, |l| *l == Link::Up).await;
        let t = mock.refused();
        let gaps: Vec<_> = t.windows(2).map(|w| w[1] - w[0]).collect();
        // 20, 40, 80, 160 ms ± 20 %: each gap clearly longer than the one before.
        for w in gaps.windows(2) {
            assert!(w[1] > w[0], "gaps should grow: {gaps:?}");
        }
        assert_eq!(within(s.call("fast", json!([]))).await.unwrap(), "fast");
    }

    #[tokio::test]
    async fn a_rejected_token_ends_the_session() {
        let (mock, nas, ep) = start().await;
        let s = admin(&ep, opts()).await;
        nas.lock().unwrap().reject_tokens = true;
        mock.kill();
        wait_link(&s, |l| *l == Link::Ended(EndReason::SessionLost)).await;
        let err = within(s.call("fast", json!([]))).await.unwrap_err();
        assert!(matches!(err, Error::LoggedOut), "{err:?}");
    }

    #[tokio::test]
    async fn an_expired_token_ends_the_session_without_trying() {
        let (mock, _nas, ep) = start().await;
        let s = admin(
            &ep,
            SessionOptions {
                token_ttl: Duration::from_millis(50),
                ..opts()
            },
        )
        .await;
        tokio::time::sleep(Duration::from_millis(80)).await;
        mock.kill();
        wait_link(&s, |l| *l == Link::Ended(EndReason::SessionLost)).await;
        assert!(methods_on(&mock, 2).is_empty());
    }

    #[tokio::test]
    async fn the_token_is_refreshed_and_the_newest_one_used() {
        let (mock, nas, ep) = start().await;
        let s = admin(
            &ep,
            SessionOptions {
                token_refresh: Duration::from_millis(50),
                ..opts()
            },
        )
        .await;
        within(async {
            while nas.lock().unwrap().tokens.len() < 3 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        reconnected(&s, async { mock.kill() }).await;
        let login = mock.requests().into_iter().find(|r| r.conn == 2).unwrap();
        let used = login.params[0]["token"].as_str().unwrap().to_owned();
        let n: usize = used.trim_start_matches("tok-").parse().unwrap();
        assert!(n >= 3, "used {used}, an old token");
    }

    #[tokio::test]
    async fn calls_are_never_resent_and_wait_for_the_reconnect() {
        let (mock, _nas, ep) = start().await;
        let s = admin(&ep, opts()).await;
        let in_flight = tokio::spawn({
            let s = s.clone();
            async move { s.call("never", json!([])).await }
        });
        within(async {
            while !methods_on(&mock, 1).iter().any(|m| m == "never") {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        mock.set_down(true);
        mock.kill();
        let err = within(in_flight).await.unwrap().unwrap_err();
        assert!(matches!(err, Error::Closed), "{err:?}");
        wait_link(&s, |l| matches!(l, Link::Reconnecting { .. })).await;

        let waiting = tokio::spawn({
            let s = s.clone();
            async move { s.call("fast", json!([])).await }
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        mock.set_down(false);
        assert_eq!(within(waiting).await.unwrap().unwrap(), "fast");
        let nevers = mock
            .requests()
            .iter()
            .filter(|r| r.method == "never")
            .count();
        assert_eq!(nevers, 1, "a call in flight is not resent");
    }

    #[tokio::test]
    async fn logout_ends_every_handle() {
        let (mock, _nas, ep) = start().await;
        let s = admin(&ep, opts()).await;
        let other = s.clone();
        within(s.logout()).await;
        assert!(methods_on(&mock, 1).contains(&"auth.logout".to_owned()));
        assert_eq!(*other.link().borrow(), Link::Ended(EndReason::LoggedOut));
        let err = within(other.call("fast", json!([]))).await.unwrap_err();
        assert!(matches!(err, Error::LoggedOut), "{err:?}");
    }

    #[tokio::test]
    async fn dropping_every_handle_closes_the_connection() {
        let (mock, _nas, ep) = start().await;
        let s = admin(&ep, opts()).await;
        drop(s);
        within(mock.disconnected()).await;
    }

    #[tokio::test]
    async fn notifications_survive_a_reconnect() {
        let (mock, _nas, ep) = start().await;
        let s = admin(&ep, opts()).await;
        let mut n = s.take_notifications().unwrap();
        assert!(s.take_notifications().is_none());
        mock.push(json!({"jsonrpc": "2.0", "method": "collection_update", "params": {"id": 1}}));
        assert_eq!(within(n.recv()).await.unwrap().params["id"], 1);
        reconnected(&s, async { mock.kill() }).await;
        mock.push(json!({"jsonrpc": "2.0", "method": "collection_update", "params": {"id": 2}}));
        assert_eq!(within(n.recv()).await.unwrap().params["id"], 2);
    }

    #[tokio::test]
    async fn debug_output_holds_no_secret() {
        let (mock, nas, ep) = start().await;
        let s = admin(&ep, opts()).await;
        let token = nas.lock().unwrap().tokens[0].clone();
        let text = format!("{s:?} {ep:?}");
        assert!(!text.contains("secret") && !text.contains(&token), "{text}");
        drop(mock);
    }

    fn event(collection: &str, n: i64) -> Value {
        json!({"jsonrpc": "2.0", "method": "collection_update", "params": {
            "msg": "added", "collection": collection, "id": n, "fields": {"n": n}}})
    }

    async fn next(sub: &mut Subscription) -> Event {
        within(sub.recv()).await.expect("subscription closed")
    }

    fn update(e: Event) -> Arc<Update> {
        match e {
            Event::Update(u) => u,
            other => panic!("expected an update, got {other:?}"),
        }
    }

    fn requests_of(mock: &Mock, method: &str) -> Vec<Value> {
        mock.requests()
            .into_iter()
            .filter(|r| r.method == method)
            .map(|r| r.params)
            .collect()
    }

    /// Waits until the supervisor has handled everything pushed so far: a marker notification
    /// that is no event comes out of `take_notifications` after them.
    async fn settled(mock: &Mock, n: &mut mpsc::Receiver<Notification>) {
        mock.push(json!({"jsonrpc": "2.0", "method": "marker", "params": {}}));
        while within(n.recv()).await.unwrap().method != "marker" {}
    }

    #[tokio::test]
    async fn an_event_reaches_its_subscriber() {
        let (mock, _nas, ep) = start().await;
        let s = admin(&ep, opts()).await;
        let mut other = s.take_notifications().unwrap();
        let mut sub = within(s.subscribe("alert.list")).await.unwrap();
        assert_eq!(sub.name(), "alert.list");
        assert_eq!(
            requests_of(&mock, "core.subscribe"),
            [json!(["alert.list"])]
        );
        mock.push(event("disk.query", 1));
        mock.push(event("alert.list", 2));
        let u = update(next(&mut sub).await);
        assert_eq!(u.kind, UpdateKind::Added);
        assert_eq!(
            (u.id.clone(), u.fields.clone()),
            (Some(json!(2)), json!({"n": 2}))
        );
        // An event nobody subscribed to still reaches the plain notification stream.
        assert_eq!(
            within(other.recv()).await.unwrap().params["collection"],
            "disk.query"
        );
    }

    #[tokio::test]
    async fn two_subscribers_share_one_upstream_subscription() {
        let (mock, _nas, ep) = start().await;
        let s = admin(&ep, opts()).await;
        let mut a = within(s.subscribe("alert.list")).await.unwrap();
        let mut b = within(s.clone().subscribe("alert.list")).await.unwrap();
        assert_eq!(requests_of(&mock, "core.subscribe").len(), 1);
        mock.push(event("alert.list", 1));
        assert_eq!(update(next(&mut a).await).id, Some(json!(1)));
        assert_eq!(update(next(&mut b).await).id, Some(json!(1)));
    }

    #[tokio::test]
    async fn the_last_drop_unsubscribes_once() {
        let (mock, _nas, ep) = start().await;
        let s = admin(&ep, opts()).await;
        let mut n = s.take_notifications().unwrap();
        let a = within(s.subscribe("alert.list")).await.unwrap();
        let b = within(s.subscribe("alert.list")).await.unwrap();
        drop(a);
        settled(&mock, &mut n).await;
        assert!(requests_of(&mock, "core.unsubscribe").is_empty());
        drop(b);
        within(async {
            while requests_of(&mock, "core.unsubscribe").is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        assert_eq!(requests_of(&mock, "core.unsubscribe"), [json!(["sub-1"])]);
        // The next subscriber subscribes upstream again.
        let _c = within(s.subscribe("alert.list")).await.unwrap();
        assert_eq!(requests_of(&mock, "core.subscribe").len(), 2);
    }

    /// Issue #12: subscriptions are made again on the new connection before the link is `Up`.
    #[tokio::test]
    async fn subscriptions_come_back_after_a_reconnect() {
        let (mock, _nas, ep) = start().await;
        let s = admin(&ep, opts()).await;
        let mut sub = within(s.subscribe("alert.list")).await.unwrap();
        reconnected(&s, async { mock.kill() }).await;
        assert!(methods_on(&mock, 2).contains(&"core.subscribe".to_owned()));
        assert!(matches!(next(&mut sub).await, Event::Resumed));
        mock.push(event("alert.list", 7));
        assert_eq!(update(next(&mut sub).await).id, Some(json!(7)));
    }

    #[tokio::test]
    async fn a_refused_event_is_a_typed_error() {
        let (mock, _nas, ep) = start().await;
        let s = admin(&ep, opts()).await;
        for _ in 0..2 {
            let err = within(s.subscribe("vm.query")).await.unwrap_err();
            let Error::Rpc(e) = &err else {
                panic!("expected Rpc, got {err:?}")
            };
            assert_eq!(e.data.as_ref().unwrap().errname.as_deref(), Some("EACCES"));
        }
        // Nothing was registered: the second try asked the NAS again.
        assert_eq!(requests_of(&mock, "core.subscribe").len(), 2);
    }

    #[tokio::test]
    async fn the_nas_ending_a_subscription_ends_it() {
        let (mock, _nas, ep) = start().await;
        let s = admin(&ep, opts()).await;
        let mut old = within(s.subscribe("alert.list")).await.unwrap();
        mock.push(
            json!({"jsonrpc": "2.0", "method": "notify_unsubscribed", "params": {
            "collection": "alert.list", "error": {"error": 13, "errname": "EACCES",
            "reason": "Not authorized", "trace": null, "extra": [], "py_exception": ""}}}),
        );
        let Event::Ended(e) = next(&mut old).await else {
            panic!("expected Ended")
        };
        assert!(matches!(*e, Error::Rpc(_)), "{e:?}");
        assert!(within(old.recv()).await.is_none());

        let mut new = within(s.subscribe("alert.list")).await.unwrap();
        assert_eq!(requests_of(&mock, "core.subscribe").len(), 2);
        // Dropping a handle of the ended subscription leaves the new one alone.
        drop(old);
        mock.push(event("alert.list", 3));
        assert_eq!(update(next(&mut new).await).id, Some(json!(3)));
        assert!(requests_of(&mock, "core.unsubscribe").is_empty());
    }

    #[tokio::test]
    async fn logout_closes_subscriptions() {
        let (_mock, _nas, ep) = start().await;
        let s = admin(&ep, opts()).await;
        let mut sub = within(s.subscribe("alert.list")).await.unwrap();
        within(s.logout()).await;
        assert!(within(sub.recv()).await.is_none());
        let err = within(s.subscribe("alert.list")).await.unwrap_err();
        assert!(matches!(err, Error::LoggedOut), "{err:?}");
    }

    #[tokio::test]
    async fn a_slow_subscriber_is_told_it_missed_events() {
        let (mock, _nas, ep) = start().await;
        let s = admin(
            &ep,
            SessionOptions {
                event_buffer: 2,
                ..opts()
            },
        )
        .await;
        let mut n = s.take_notifications().unwrap();
        let mut sub = within(s.subscribe("alert.list")).await.unwrap();
        for i in 1..=4 {
            mock.push(event("alert.list", i));
        }
        settled(&mock, &mut n).await;
        assert!(matches!(next(&mut sub).await, Event::Missed(2)));
        assert_eq!(update(next(&mut sub).await).id, Some(json!(3)));
        assert_eq!(update(next(&mut sub).await).id, Some(json!(4)));
    }

    #[tokio::test]
    async fn subscribing_while_reconnecting_waits_for_the_link() {
        let (mock, _nas, ep) = start().await;
        let s = admin(&ep, opts()).await;
        mock.set_down(true);
        mock.kill();
        wait_link(&s, |l| matches!(l, Link::Reconnecting { .. })).await;
        let pending = tokio::spawn({
            let s = s.clone();
            async move { s.subscribe("alert.list").await }
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        mock.set_down(false);
        let mut sub = within(pending).await.unwrap().unwrap();
        mock.push(event("alert.list", 1));
        assert_eq!(update(next(&mut sub).await).id, Some(json!(1)));
    }

    /// Against a real NAS: subscribe to `reporting.realtime` and wait for one event. Receiving
    /// it proves events are routed by the name given to `core.subscribe`.
    /// Prints the payload and its compact form (`truenas::realtime`). Same variables as
    /// `nas_login_smoke`; `cargo test nas_events_smoke -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "needs a real NAS and credentials"]
    async fn nas_events_smoke() {
        let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k} not set"));
        let pin: Fingerprint = var("TRUELITE_NAS_FINGERPRINT").parse().unwrap();
        let ep = Endpoint::new(var("TRUELITE_NAS_URL"), Some(pin)).unwrap();
        let password = Secret::new(var("TRUELITE_NAS_PASSWORD"));
        let step = within(login(
            &ep,
            SessionOptions::default(),
            &var("TRUELITE_NAS_USER"),
            password,
        ))
        .await
        .unwrap_or_else(|e| panic!("{e}"));
        let LoginStep::Done(s) = step else {
            panic!("this account needs a one-time code; use one without 2FA")
        };
        let mut sub = within(s.subscribe("reporting.realtime")).await.unwrap();
        let u = update(next(&mut sub).await);
        let keys: Vec<_> = u
            .fields
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default();
        println!("reporting.realtime {:?}: {keys:?}", u.kind);
        // Full payload (fixture for `realtime.rs` tests) and what the browser would get.
        println!("{}", serde_json::to_string_pretty(&u.fields).unwrap());
        println!(
            "compact: {:?}",
            crate::truenas::realtime::compact(&u.fields)
        );
        drop(sub);
        tokio::time::sleep(Duration::from_millis(200)).await;
        within(s.logout()).await;
    }

    /// Against a real NAS (#32): follow the log of the first running app's first container, as
    /// the bridge would, and check its lines arrive under the full `name:{args}` collection.
    /// Same variables as `nas_events_smoke`; needs a running app;
    /// `cargo test nas_logs_smoke -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "needs a real NAS, credentials and a running app"]
    async fn nas_logs_smoke() {
        let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k} not set"));
        let pin: Fingerprint = var("TRUELITE_NAS_FINGERPRINT").parse().unwrap();
        let ep = Endpoint::new(var("TRUELITE_NAS_URL"), Some(pin)).unwrap();
        let password = Secret::new(var("TRUELITE_NAS_PASSWORD"));
        let step = within(login(
            &ep,
            SessionOptions::default(),
            &var("TRUELITE_NAS_USER"),
            password,
        ))
        .await
        .unwrap_or_else(|e| panic!("{e}"));
        let LoginStep::Done(s) = step else {
            panic!("this account needs a one-time code; use one without 2FA")
        };
        let apps = within(s.call(
            "app.query",
            json!([[["state", "=", "RUNNING"]], {"select": ["name", "active_workloads"]}]),
        ))
        .await
        .unwrap();
        let app = &apps[0];
        let name = app["name"].as_str().expect("no running app");
        let container = app["active_workloads"]["container_details"][0]["id"]
            .as_str()
            .unwrap();
        println!("{name} container {container}");
        let arg = json!({"app_name": name, "container_id": container, "tail_lines": 5});
        let topic = crate::truenas::apps::log_topic(&arg.to_string()).expect("ids truelite takes");
        let mut sub = within(s.subscribe_with_buffer(&topic, Some(512)))
            .await
            .unwrap();
        for _ in 0..5 {
            let u = update(next(&mut sub).await);
            println!(
                "{:?} {:?}",
                u.kind,
                crate::truenas::apps::compact_log_line(&u.fields)
            );
        }
        drop(sub);
        tokio::time::sleep(Duration::from_millis(200)).await;
        within(s.logout()).await;
    }

    /// Against a real NAS: log in, drop the upstream socket, check the session comes back.
    /// `TRUELITE_NAS_URL`, `TRUELITE_NAS_FINGERPRINT`, `TRUELITE_NAS_USER`,
    /// `TRUELITE_NAS_PASSWORD`; `cargo test nas_login_smoke -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "needs a real NAS and credentials"]
    async fn nas_login_smoke() {
        let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k} not set"));
        let pin: Fingerprint = var("TRUELITE_NAS_FINGERPRINT").parse().unwrap();
        let ep = Endpoint::new(var("TRUELITE_NAS_URL"), Some(pin)).unwrap();
        let password = Secret::new(var("TRUELITE_NAS_PASSWORD"));
        let step = within(login(
            &ep,
            SessionOptions::default(),
            &var("TRUELITE_NAS_USER"),
            password,
        ))
        .await
        .unwrap_or_else(|e| panic!("{e}"));
        let LoginStep::Done(s) = step else {
            panic!("this account needs a one-time code; use one without 2FA")
        };
        println!(
            "logged in as {} ({} roles)",
            s.user().username,
            s.user().roles.len()
        );
        let me: Value = within(s.call("auth.me", json!([]))).await.unwrap();
        reconnected(&s, s.kill_upstream()).await;
        let again: Value = within(s.call("auth.me", json!([]))).await.unwrap();
        assert_eq!(me["pw_name"], again["pw_name"]);
        println!("reconnected with the token as {}", again["pw_name"]);
        within(s.logout()).await;
    }
}
