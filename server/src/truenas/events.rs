//! Event subscriptions (`core.subscribe`). One upstream subscription per event name and session,
//! shared by every [`Subscription`] handle (browser tabs of the same login) and made again after
//! a reconnect.
//!
//! The NAS sends events as `collection_update` notifications that name the event in
//! `collection`, not the subscription id, so they are routed by event name.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;
use tokio::sync::{broadcast, mpsc};

use super::client::Notification;
use super::error::Error;
use super::reshape::reshaper;
use super::rpc::{ErrorData, RpcError};

/// What changed in a collection event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateKind {
    Added,
    Changed,
    Removed,
    /// A kind this version does not know.
    Other,
}

/// One `collection_update`. `fields` stays raw (its shape depends on the event), except for the
/// reshaped topics, where it is already cut down to what the web app shows
/// ([`reshaper`](super::reshape::reshaper)).
#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    pub kind: UpdateKind,
    /// The changed entry's id, when the event has one.
    pub id: Option<Value>,
    pub fields: Value,
}

/// What a [`Subscription`] receives.
#[derive(Debug, Clone)]
pub enum Event {
    Update(Arc<Update>),
    /// Subscribed again after a reconnect. Events of the gap are lost: query the state again.
    Resumed,
    /// This consumer was too slow and missed this many events: query the state again.
    Missed(u64),
    /// The NAS ended the subscription (lost permission, failed resubscribe). Nothing follows.
    Ended(Arc<Error>),
}

/// How a topic keeps events, set by the handle that creates it.
#[derive(Debug, Clone, Copy, Default)]
pub struct TopicOptions {
    /// Events kept for a slow consumer before it gets [`Event::Missed`]; `None` = the session
    /// default ([`SessionOptions::event_buffer`](super::SessionOptions::event_buffer)).
    pub buffer: Option<usize>,
    /// Last updates kept and replayed to a handle that joins the topic later. TrueNAS sends a
    /// stream's history (a log tail) only to the subscription that starts it, so without this a
    /// second tab, or a quick unsubscribe and subscribe again, would start empty. 0 = none.
    pub replay: usize,
}

/// Sent by a dropped [`Subscription`]: event name and the generation of its topic, so a handle
/// of an ended topic never releases a newer topic of the same name.
pub(super) type Release = (Arc<str>, u64);

/// A live subscription to one event. Dropping the last handle of an event unsubscribes on the
/// NAS. It does not keep the session alive: when the session ends, [`recv`](Self::recv)
/// returns `None`.
#[derive(Debug)]
pub struct Subscription {
    name: Arc<str>,
    generation: u64,
    rx: broadcast::Receiver<Event>,
    /// Updates from before this handle joined ([`TopicOptions::replay`]), received first.
    backlog: VecDeque<Arc<Update>>,
    // Unbounded because `Drop` cannot wait; one small message per handle ever made.
    release: mpsc::UnboundedSender<Release>,
}

impl Subscription {
    /// The event name, as given to [`Session::subscribe`](super::Session::subscribe).
    #[cfg(test)]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The next event; `None` once the subscription or the session has ended.
    pub async fn recv(&mut self) -> Option<Event> {
        if let Some(u) = self.backlog.pop_front() {
            return Some(Event::Update(u));
        }
        match self.rx.recv().await {
            Ok(e) => Some(e),
            Err(broadcast::error::RecvError::Lagged(n)) => Some(Event::Missed(n)),
            Err(broadcast::error::RecvError::Closed) => None,
        }
    }

    /// The next event if one is already waiting, without waiting: lets a consumer batch a
    /// burst. `None` when nothing is waiting or the subscription has ended (then
    /// [`recv`](Self::recv) says which).
    pub fn try_recv(&mut self) -> Option<Event> {
        if let Some(u) = self.backlog.pop_front() {
            return Some(Event::Update(u));
        }
        match self.rx.try_recv() {
            Ok(e) => Some(e),
            Err(broadcast::error::TryRecvError::Lagged(n)) => Some(Event::Missed(n)),
            Err(broadcast::error::TryRecvError::Empty | broadcast::error::TryRecvError::Closed) => {
                None
            }
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        // Fails only when the session is gone, and then there is nothing to release.
        let _ = self.release.send((self.name.clone(), self.generation));
    }
}

struct Topic {
    name: Arc<str>,
    tx: broadcast::Sender<Event>,
    /// Subscription id on the current connection; `None` while disconnected.
    upstream: Option<String>,
    refs: usize,
    generation: u64,
    /// The last [`TopicOptions::replay`] updates, for handles that join later.
    history: VecDeque<Arc<Update>>,
    replay: usize,
    /// Cuts each update's `fields` down once, for every handle ([`reshaper`]).
    reshape: Option<fn(&Value) -> Option<Value>>,
    /// An odd payload was already logged: the NAS could repeat it every 2 s.
    warned: bool,
}

/// The session's subscriptions, owned by its supervisor task.
pub(super) struct Topics {
    map: HashMap<Arc<str>, Topic>,
    next_generation: u64,
    buffer: usize,
    release: mpsc::UnboundedSender<Release>,
}

impl Topics {
    /// `buffer`: events kept per event name for a slow consumer before it gets
    /// [`Event::Missed`].
    pub(super) fn new(buffer: usize, release: mpsc::UnboundedSender<Release>) -> Self {
        Self {
            map: HashMap::new(),
            next_generation: 0,
            buffer: buffer.max(1),
            release,
        }
    }

    /// A new handle on an existing topic, `None` if there is none for `name`.
    pub(super) fn join(&mut self, name: &str) -> Option<Subscription> {
        let topic = self.map.get_mut(name)?;
        topic.refs += 1;
        Some(Subscription {
            name: topic.name.clone(),
            generation: topic.generation,
            rx: topic.tx.subscribe(),
            backlog: topic.history.clone(),
            release: self.release.clone(),
        })
    }

    /// Registers a topic subscribed upstream as `upstream` (`None` while disconnected).
    /// A new topic with its first handle, kept as `opts` says.
    pub(super) fn insert(
        &mut self,
        name: &str,
        upstream: Option<String>,
        opts: TopicOptions,
    ) -> Subscription {
        let name: Arc<str> = Arc::from(name);
        let (tx, rx) = broadcast::channel(opts.buffer.unwrap_or(self.buffer).max(1));
        let generation = self.next_generation;
        self.next_generation += 1;
        self.map.insert(
            name.clone(),
            Topic {
                name: name.clone(),
                tx,
                upstream,
                refs: 1,
                generation,
                history: VecDeque::with_capacity(opts.replay),
                replay: opts.replay,
                reshape: reshaper(&name),
                warned: false,
            },
        );
        Subscription {
            name,
            generation,
            rx,
            backlog: VecDeque::new(),
            release: self.release.clone(),
        }
    }

    /// Drops one handle. Returns the upstream id to unsubscribe when it was the last one.
    pub(super) fn release(&mut self, (name, generation): Release) -> Option<String> {
        let topic = self.map.get_mut(&name)?;
        if topic.generation != generation {
            return None;
        }
        topic.refs = topic.refs.saturating_sub(1);
        if topic.refs > 0 {
            return None;
        }
        self.map.remove(&name)?.upstream
    }

    /// Delivers an event notification to its topic. Gives the notification back when it is not
    /// one (another method, or an event nobody subscribed to here).
    pub(super) fn dispatch(&mut self, n: Notification) -> Option<Notification> {
        let Some(name) = n.params.get("collection").and_then(Value::as_str) else {
            return Some(n);
        };
        match n.method.as_str() {
            "collection_update" => {
                let Some(topic) = self.map.get_mut(name) else {
                    return Some(n);
                };
                let mut update = parse_update(n.params);
                // A removal carries only the id: nothing to reshape.
                if let Some(compact) = topic.reshape
                    && !update.fields.is_null()
                {
                    match compact(&update.fields) {
                        Some(fields) => update.fields = fields,
                        None => {
                            if !topic.warned {
                                tracing::warn!(
                                    event = %topic.name,
                                    "payload of an unexpected shape, not forwarded"
                                );
                                topic.warned = true;
                            }
                            return None;
                        }
                    }
                }
                let update = Arc::new(update);
                if topic.replay > 0 {
                    if topic.history.len() == topic.replay {
                        topic.history.pop_front();
                    }
                    topic.history.push_back(update.clone());
                }
                // No receiver is fine: the last handle's release is on its way.
                let _ = topic.tx.send(Event::Update(update));
                None
            }
            "notify_unsubscribed" if self.map.contains_key(name) => {
                let name = name.to_owned();
                tracing::warn!(event = %name, "the NAS ended a subscription");
                self.end(&name, parse_unsubscribed(&n.params));
                None
            }
            _ => Some(n),
        }
    }

    /// The connection is gone: no upstream id is valid any more.
    pub(super) fn mark_down(&mut self) {
        for topic in self.map.values_mut() {
            topic.upstream = None;
        }
    }

    /// Event names to subscribe again on a new connection.
    pub(super) fn names(&self) -> Vec<Arc<str>> {
        self.map.keys().cloned().collect()
    }

    /// Subscribed upstream as `upstream`, for a topic inserted before its `core.subscribe` was
    /// answered (events that come before the answer then reach it).
    pub(super) fn subscribed(&mut self, name: &str, upstream: String) {
        if let Some(topic) = self.map.get_mut(name) {
            topic.upstream = Some(upstream);
        }
    }

    /// Forgets a topic whose `core.subscribe` failed; its handle is dropped by the caller.
    pub(super) fn remove(&mut self, name: &str) {
        self.map.remove(name);
    }

    /// Being subscribed again after a reconnect: handles get [`Event::Resumed`] before any
    /// event of the new subscription (the history the NAS sends again may come before its
    /// answer), then [`subscribed`](Self::subscribed) records the new id.
    pub(super) fn resuming(&mut self, name: &str) {
        if let Some(topic) = self.map.get_mut(name) {
            // The NAS sends the history again on the new subscription.
            topic.history.clear();
            let _ = topic.tx.send(Event::Resumed);
        }
    }

    /// `count` notifications were lost before reaching any topic: every handle gets
    /// [`Event::Missed`] and queries its state again. Kept histories now have a gap.
    pub(super) fn missed_all(&mut self, count: u64) {
        for topic in self.map.values_mut() {
            topic.history.clear();
            let _ = topic.tx.send(Event::Missed(count));
        }
    }

    /// Ends a topic: its handles get [`Event::Ended`], then `None`.
    pub(super) fn end(&mut self, name: &str, err: Error) {
        if let Some(topic) = self.map.remove(name) {
            let _ = topic.tx.send(Event::Ended(Arc::new(err)));
        }
    }
}

/// `collection_update` params: `{msg, collection, id, fields}`. The docs' example uses `event`
/// instead of `msg` and upper case, so both are read, ignoring case.
fn parse_update(mut params: Value) -> Update {
    let kind = params
        .get("msg")
        .or_else(|| params.get("event"))
        .and_then(Value::as_str)
        .map_or(UpdateKind::Other, |m| {
            if m.eq_ignore_ascii_case("added") {
                UpdateKind::Added
            } else if m.eq_ignore_ascii_case("changed") {
                UpdateKind::Changed
            } else if m.eq_ignore_ascii_case("removed") {
                UpdateKind::Removed
            } else {
                UpdateKind::Other
            }
        });
    Update {
        kind,
        id: params.get_mut("id").map(Value::take),
        fields: params.get_mut("fields").map_or(Value::Null, Value::take),
    }
}

/// `notify_unsubscribed` params: `{collection, error: {error, errname, reason, …}}`.
fn parse_unsubscribed(params: &Value) -> Error {
    let data = params
        .get("error")
        .and_then(|e| ErrorData::deserialize(e).ok());
    Error::Rpc(Box::new(RpcError {
        code: -32001,
        message: Some("subscription ended by the NAS".into()),
        data,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn topics() -> (Topics, mpsc::UnboundedReceiver<Release>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Topics::new(4, tx), rx)
    }

    fn note(method: &str, params: Value) -> Notification {
        Notification {
            method: method.into(),
            params,
        }
    }

    #[test]
    fn update_from_the_wire_and_from_the_docs_example() {
        let u = parse_update(json!({"msg": "changed", "collection": "alert.list",
            "id": "a1", "fields": {"level": "CRITICAL"}}));
        assert_eq!(
            u,
            Update {
                kind: UpdateKind::Changed,
                id: Some(json!("a1")),
                fields: json!({"level": "CRITICAL"}),
            }
        );
        let u = parse_update(json!({"collection": "disk.query", "event": "CHANGED",
            "fields": {"name": "sda"}}));
        assert_eq!((u.kind, u.id), (UpdateKind::Changed, None));
        assert_eq!(
            parse_update(json!({"msg": "weird"})).kind,
            UpdateKind::Other
        );
    }

    #[test]
    fn unsubscribed_error_is_typed() {
        let e = parse_unsubscribed(&json!({"collection": "pool.query", "error": {
            "error": 13, "errname": "EACCES", "reason": "Not authorized",
            "trace": null, "extra": [], "py_exception": "Traceback"}}));
        let Error::Rpc(e) = e else {
            panic!("expected Rpc")
        };
        let d = e.data.unwrap();
        assert_eq!(d.errname.as_deref(), Some("EACCES"));
        assert_eq!(d.reason.as_deref(), Some("Not authorized"));
    }

    #[test]
    fn the_last_release_returns_the_upstream_id() {
        let (mut t, mut rel) = topics();
        let a = t.insert("alert.list", Some("sub-1".into()), TopicOptions::default());
        let b = t.join("alert.list").unwrap();
        drop(a);
        assert_eq!(t.release(rel.try_recv().unwrap()), None);
        drop(b);
        assert_eq!(t.release(rel.try_recv().unwrap()), Some("sub-1".into()));
        assert!(t.join("alert.list").is_none());
    }

    #[test]
    fn a_handle_of_an_ended_topic_does_not_release_its_successor() {
        let (mut t, mut rel) = topics();
        let old = t.insert("alert.list", Some("sub-1".into()), TopicOptions::default());
        t.end("alert.list", Error::Closed);
        let new = t.insert("alert.list", Some("sub-2".into()), TopicOptions::default());
        drop(old);
        assert_eq!(t.release(rel.try_recv().unwrap()), None);
        drop(new);
        assert_eq!(t.release(rel.try_recv().unwrap()), Some("sub-2".into()));
    }

    #[test]
    fn dispatch_routes_by_collection() {
        let (mut t, _rel) = topics();
        let mut sub = t.insert("alert.list", Some("sub-1".into()), TopicOptions::default());
        let other = note("collection_update", json!({"collection": "disk.query"}));
        assert!(t.dispatch(other).is_some());
        let mine = note(
            "collection_update",
            json!({"msg": "added", "collection": "alert.list", "id": 1}),
        );
        assert!(t.dispatch(mine).is_none());
        let Ok(Event::Update(u)) = sub.rx.try_recv() else {
            panic!("expected an update")
        };
        assert_eq!((u.kind, u.id.clone()), (UpdateKind::Added, Some(json!(1))));
        assert!(t.dispatch(note("something_else", json!({}))).is_some());
    }

    #[test]
    fn a_topic_buffer_can_be_set_and_try_recv_never_waits() {
        // The default here is 4: a topic made with 8 holds 8 events before it lags.
        let (mut t, _rel) = topics();
        let line = |n: u64| {
            note(
                "collection_update",
                json!({"msg": "added", "collection": "log:x", "fields": {"n": n}}),
            )
        };
        let mut big = t.insert(
            "log:x",
            Some("sub-1".into()),
            TopicOptions {
                buffer: Some(8),
                replay: 0,
            },
        );
        assert!(big.try_recv().is_none());
        for n in 0..8 {
            assert!(t.dispatch(line(n)).is_none());
        }
        for n in 0..8 {
            let Some(Event::Update(u)) = big.try_recv() else {
                panic!("expected update {n}")
            };
            assert_eq!(u.fields, json!({"n": n}));
        }
        assert!(big.try_recv().is_none());

        let mut small = t.insert("log:y", Some("sub-2".into()), TopicOptions::default());
        for n in 0..6 {
            let mut l = line(n);
            l.params["collection"] = json!("log:y");
            t.dispatch(l);
        }
        assert!(matches!(small.try_recv(), Some(Event::Missed(2))));
    }

    fn line(name: &str, n: u64) -> Notification {
        note(
            "collection_update",
            json!({"msg": "added", "collection": name, "fields": {"n": n}}),
        )
    }

    fn next_n(sub: &mut Subscription) -> Option<Value> {
        match sub.try_recv() {
            Some(Event::Update(u)) => Some(u.fields["n"].clone()),
            _ => None,
        }
    }

    #[test]
    fn a_handle_that_joins_later_gets_the_last_updates_first() {
        let (mut t, _rel) = topics();
        let opts = TopicOptions {
            buffer: Some(8),
            replay: 3,
        };
        let mut first = t.insert("log:x", Some("sub-1".into()), opts);
        for n in 0..5 {
            t.dispatch(line("log:x", n));
        }
        let mut second = t.join("log:x").unwrap();
        t.dispatch(line("log:x", 5));
        let got: Vec<_> = std::iter::from_fn(|| next_n(&mut second)).collect();
        assert_eq!(got, [json!(2), json!(3), json!(4), json!(5)]);
        assert_eq!(std::iter::from_fn(|| next_n(&mut first)).count(), 6);

        // The NAS resends the history on a new subscription: the old one is not replayed.
        t.resuming("log:x");
        t.subscribed("log:x", "sub-2".into());
        let mut third = t.join("log:x").unwrap();
        assert!(third.try_recv().is_none());

        // A topic without replay keeps nothing.
        let _plain = t.insert("alert.list", Some("sub-3".into()), TopicOptions::default());
        t.dispatch(line("alert.list", 1));
        assert!(t.join("alert.list").unwrap().try_recv().is_none());
    }

    #[test]
    fn a_reshaped_topic_is_cut_once_for_broadcast_and_replay() {
        let (mut t, _rel) = topics();
        let opts = TopicOptions {
            buffer: None,
            replay: 2,
        };
        let mut first = t.insert("pool.query", Some("sub-1".into()), opts);
        let pool = |fields: Value| {
            note(
                "collection_update",
                json!({"msg": "changed", "collection": "pool.query", "id": 1, "fields": fields}),
            )
        };
        assert!(
            t.dispatch(pool(json!({"name": "tank", "topology": {"data": []}})))
                .is_none()
        );
        // Not an object: skipped, for every handle and in the history.
        assert!(t.dispatch(pool(json!("odd"))).is_none());
        let removed = json!({"msg": "removed", "collection": "pool.query", "id": 2});
        assert!(t.dispatch(note("collection_update", removed)).is_none());

        let mut second = t.join("pool.query").unwrap();
        for sub in [&mut first, &mut second] {
            let Some(Event::Update(u)) = sub.try_recv() else {
                panic!("expected the reshaped update")
            };
            assert_eq!(u.fields, json!({"name": "tank"}));
            let Some(Event::Update(u)) = sub.try_recv() else {
                panic!("expected the removal")
            };
            assert_eq!((u.kind, &u.fields), (UpdateKind::Removed, &Value::Null));
            assert!(sub.try_recv().is_none());
        }
        assert!(t.map["pool.query"].warned);
    }

    #[test]
    fn missed_all_reaches_every_topic() {
        let (mut t, _rel) = topics();
        let mut a = t.insert("alert.list", Some("sub-1".into()), TopicOptions::default());
        let mut b = t.insert("pool.query", Some("sub-2".into()), TopicOptions::default());
        t.missed_all(3);
        assert!(matches!(a.try_recv(), Some(Event::Missed(3))));
        assert!(matches!(b.try_recv(), Some(Event::Missed(3))));
    }
}
