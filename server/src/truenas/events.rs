//! Event subscriptions (`core.subscribe`). One upstream subscription per event name and session,
//! shared by every [`Subscription`] handle (browser tabs of the same login) and made again after
//! a reconnect.
//!
//! The NAS sends events as `collection_update` notifications that name the event in
//! `collection`, not the subscription id, so they are routed by event name.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{broadcast, mpsc};

use super::client::Notification;
use super::error::Error;
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

/// One `collection_update`. `fields` stays raw: its shape depends on the event.
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
    // Unbounded because `Drop` cannot wait; one small message per handle ever made.
    release: mpsc::UnboundedSender<Release>,
}

impl Subscription {
    /// The event name, as given to [`Session::subscribe`](super::Session::subscribe).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The next event; `None` once the subscription or the session has ended.
    pub async fn recv(&mut self) -> Option<Event> {
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
            release: self.release.clone(),
        })
    }

    /// Registers a topic subscribed upstream as `upstream` (`None` while disconnected).
    /// A new topic with its first handle. `buffer` overrides the default for this topic.
    pub(super) fn insert(
        &mut self,
        name: &str,
        upstream: Option<String>,
        buffer: Option<usize>,
    ) -> Subscription {
        let name: Arc<str> = Arc::from(name);
        let (tx, rx) = broadcast::channel(buffer.unwrap_or(self.buffer).max(1));
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
            },
        );
        Subscription {
            name,
            generation,
            rx,
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
                let Some(topic) = self.map.get(name) else {
                    return Some(n);
                };
                // No receiver is fine: the last handle's release is on its way.
                let _ = topic
                    .tx
                    .send(Event::Update(Arc::new(parse_update(n.params))));
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

    /// Subscribed again as `upstream` after a reconnect.
    pub(super) fn resumed(&mut self, name: &str, upstream: String) {
        if let Some(topic) = self.map.get_mut(name) {
            topic.upstream = Some(upstream);
            let _ = topic.tx.send(Event::Resumed);
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
        .and_then(|e| serde_json::from_value::<ErrorData>(e.clone()).ok());
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
        let a = t.insert("alert.list", Some("sub-1".into()), None);
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
        let old = t.insert("alert.list", Some("sub-1".into()), None);
        t.end("alert.list", Error::Closed);
        let new = t.insert("alert.list", Some("sub-2".into()), None);
        drop(old);
        assert_eq!(t.release(rel.try_recv().unwrap()), None);
        drop(new);
        assert_eq!(t.release(rel.try_recv().unwrap()), Some("sub-2".into()));
    }

    #[test]
    fn dispatch_routes_by_collection() {
        let (mut t, _rel) = topics();
        let mut sub = t.insert("alert.list", Some("sub-1".into()), None);
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
        let mut big = t.insert("log:x", Some("sub-1".into()), Some(8));
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

        let mut small = t.insert("log:y", Some("sub-2".into()), None);
        for n in 0..6 {
            let mut l = line(n);
            l.params["collection"] = json!("log:y");
            t.dispatch(l);
        }
        assert!(matches!(small.try_recv(), Some(Event::Missed(2))));
    }
}
