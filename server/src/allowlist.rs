//! What the browser may reach through the WebSocket bridge. Nothing else is forwarded to the
//! NAS. The lists grow per phase: read methods first, then actions (each behind a confirmation
//! dialog in the web app). Every name is checked against <https://api.truenas.com/v25.10/>.
//!
//! Calls run as the logged-in TrueNAS user, so TrueNAS roles still apply on top of this list.

use std::borrow::Cow;

use serde_json::Value;

/// Methods the web app may call, by exact name.
const METHODS: &[&str] = &[
    // Shell: host name and version (READONLY_ADMIN).
    "system.info",
    // Dashboard: alerts card (ALERT_LIST_READ). Result reshaped by `truenas::alerts`.
    "alert.list",
    // Dashboard: pool health and raw space, with `select` (POOL_READ).
    "pool.query",
    // Dashboard: usable space of the pools' root datasets; storage page: the dataset tree
    // (DATASET_READ). Result reshaped by `truenas::datasets` (properties as bytes).
    "pool.dataset.query",
    // Dashboard card, apps page, app detail (one app by id), with `select` (APPS_READ).
    "app.query",
    // App detail: versions and changelog in the Update dialog (APPS_READ). A read, despite its name.
    "app.upgrade_summary",
    // Snapshots page: one dataset's snapshots (SNAPSHOT_READ). Result reshaped by
    // `truenas::snapshots` (properties as numbers).
    "pool.snapshot.query",
    // Disks page: model, serial, size, pool of each disk (DISK_READ). Result reshaped by
    // `truenas::disks` (never `passwd`). No event: TrueNAS does not send `disk.query` events.
    "disk.query",
    // Disks page: current temperatures with their critical limit (REPORTING_READ). Result
    // reshaped by `truenas::disks` to `{name: {temp, crit}}`. No event: read again on a timer.
    "disk.temperatures",
    // Disks page: 7-day min, max and average temperature per disk (REPORTING_READ).
    "disk.temperature_agg",
];

/// Events the web app may subscribe to, by exact name.
const EVENTS: &[&str] = &[
    // Dashboard: live alerts (ALERT_LIST_READ). Reshaped by `truenas::alerts` (no args, mail).
    "alert.list",
    // Dashboard: live CPU, RAM, network (REPORTING_READ). Reshaped by `truenas::realtime`.
    "reporting.realtime",
    // Dashboard: pool changes (POOL_READ). Reshaped by `truenas::pools` (no topology).
    "pool.query",
    // Dashboard: scrub and resilver progress (POOL_SCRUB_READ).
    "pool.scan",
    // Storage page: datasets created, changed or deleted (DATASET_READ). Reshaped by
    // `truenas::datasets`; space changes send no event.
    "pool.dataset.query",
    // Snapshots page: snapshots taken or deleted from truelite or the TrueNAS UI (SNAPSHOT_READ).
    // Reshaped by `truenas::snapshots`; periodic tasks send no event.
    "pool.snapshot.query",
    // Dashboard, apps page, app detail: app changes (APPS_READ). Reshaped by `truenas::apps` (no
    // workloads, metadata): the detail page queries its app again on each event about it.
    "app.query",
];

/// How the bridge forwards an event to the browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    /// One `data` message per change; large events come reshaped (`truenas::reshape`).
    Collection,
    /// Container log lines, batched; the tail is replayed to a later subscriber.
    Log,
}

/// An allowed event: the name to subscribe to on the NAS, and how to forward it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route<'a> {
    pub upstream: Cow<'a, str>,
    pub stream: Stream,
}

/// Checks the browser's argument string of an event and rebuilds it; `None` refuses it.
type ParamCheck = fn(&str) -> Option<String>;

/// An event with arguments (`name:{json}`). The browser's string is never forwarded as it is.
#[derive(Debug)]
struct ParamEvent {
    name: &'static str,
    check: ParamCheck,
    stream: Stream,
}

const PARAM_EVENTS: &[ParamEvent] = &[
    // App logs page: one container's log (APPS_READ). Arguments checked and rebuilt by
    // `truenas::apps::log_topic`; lines trimmed by `truenas::apps::compact_log_line`.
    ParamEvent {
        name: crate::truenas::apps::LOG_EVENT,
        check: crate::truenas::apps::log_topic,
        stream: Stream::Log,
    },
];

/// Checks the browser's params of an action and rebuilds them; `None` refuses them.
pub type ActionCheck = fn(&Value) -> Option<Value>;

/// Actions: TrueNAS job methods, each with the check that rebuilds its params. The bridge
/// follows their job and sends its progress (`truenas::jobs`). Every one has a confirmation
/// dialog in the web app.
const JOB_METHODS: &[(&str, ActionCheck)] = &[
    // App detail: Start, Stop, Restart (APPS_WRITE). Params `[app_name]`.
    ("app.start", crate::truenas::apps::app_name_param),
    ("app.stop", crate::truenas::apps::app_name_param),
    ("app.redeploy", crate::truenas::apps::app_name_param),
    // App detail: Update (APPS_WRITE). A new version: always the latest, `values` never forwarded.
    ("app.upgrade", crate::truenas::apps::app_upgrade_param),
    // App detail: Update when only newer images of the same version exist; always redeploys.
    (
        "app.pull_images",
        crate::truenas::apps::app_pull_images_param,
    ),
    // Storage page: Check now, Pause, Resume, Stop a scrub (POOL_WRITE). Params
    // `[pool, "START"|"PAUSE"|"STOP"]`; START's job lasts the whole scrub.
    ("pool.scrub.scrub", crate::truenas::pools::scrub_param),
];

/// Actions that are plain calls (no job): each with the check that rebuilds its params. The
/// answer comes back like any call's. Every one has a confirmation dialog in the web app.
const ACTIONS: &[(&str, ActionCheck)] = &[
    // Snapshots page: Take snapshot (SNAPSHOT_WRITE). Params `[{dataset, name, recursive}]`.
    (
        "pool.snapshot.create",
        crate::truenas::snapshots::create_param,
    ),
    // Snapshots page: Delete, after typing its name (SNAPSHOT_DELETE). Params `[id]`, one
    // snapshot: never recursive or deferred.
    (
        "pool.snapshot.delete",
        crate::truenas::snapshots::delete_param,
    ),
];

/// Exact-name lists of the methods and events the bridge forwards. No prefixes, no wildcards.
#[derive(Debug, Clone, Copy)]
pub struct Allowlist {
    methods: &'static [&'static str],
    events: &'static [&'static str],
    param_events: &'static [ParamEvent],
    jobs: &'static [(&'static str, ActionCheck)],
    actions: &'static [(&'static str, ActionCheck)],
}

impl Allowlist {
    /// A custom list (tests).
    #[cfg(test)]
    pub const fn new(methods: &'static [&'static str], events: &'static [&'static str]) -> Self {
        Self {
            methods,
            events,
            param_events: &[],
            jobs: &[],
            actions: &[],
        }
    }

    /// Whether the browser may call `method`.
    pub fn method(&self, method: &str) -> bool {
        self.methods.contains(&method)
    }

    /// The params check of `method` when it is an allowed action that runs as a job.
    pub fn job(&self, method: &str) -> Option<ActionCheck> {
        find(self.jobs, method)
    }

    /// The params check of `method` when it is an allowed action that is a plain call.
    pub fn action(&self, method: &str) -> Option<ActionCheck> {
        find(self.actions, method)
    }

    /// Where and how to follow `topic` when the browser may: the topic itself for a listed
    /// event, or the rebuilt `name:{json}` of an event with arguments.
    pub fn event<'a>(&self, topic: &'a str) -> Option<Route<'a>> {
        if self.events.contains(&topic) {
            return Some(Route {
                upstream: Cow::Borrowed(topic),
                stream: Stream::Collection,
            });
        }
        let (name, arg) = topic.split_once(':')?;
        let event = self.param_events.iter().find(|e| e.name == name)?;
        Some(Route {
            upstream: Cow::Owned((event.check)(arg)?),
            stream: event.stream,
        })
    }
}

fn find(list: &[(&str, ActionCheck)], method: &str) -> Option<ActionCheck> {
    list.iter()
        .find(|(n, _)| *n == method)
        .map(|(_, check)| *check)
}

impl Default for Allowlist {
    fn default() -> Self {
        Self {
            methods: METHODS,
            events: EVENTS,
            param_events: PARAM_EVENTS,
            jobs: JOB_METHODS,
            actions: ACTIONS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_exactly() {
        let a = Allowlist::default();
        assert!(a.method("system.info"));
        assert!(a.event("alert.list").is_some());
        for near in [
            "system",
            "system.",
            "system.info.",
            "System.info",
            " system.info",
            "*",
        ] {
            assert!(!a.method(near), "{near:?}");
        }
        assert!(a.event("reporting.realtime").is_some());
        assert!(a.method("pool.query") && a.method("pool.dataset.query"));
        assert!(a.event("pool.query").is_some() && a.event("pool.scan").is_some());
        assert!(!a.method("pool.scan") && !a.method("pool.dataset"));
        assert!(a.method("app.query") && a.event("app.query").is_some());
        for near in ["app", "app.stats", "app.start", "app.query.", "apps.query"] {
            assert!(!a.method(near) && a.event(near).is_none(), "{near:?}");
        }
        assert!(a.event("alert").is_none());
        assert!(a.event("reporting.realtime:{\"interval\": 2}").is_none());
        assert!(!a.method(""));
    }

    #[test]
    fn log_topics_are_rebuilt_and_anything_else_with_arguments_refused() {
        let a = Allowlist::default();
        let id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let topic = format!(
            r#"app.container_log_follow:{{ "tail_lines":50, "container_id":"{id}", "app_name":"plex" }}"#
        );
        let route = a.event(&topic).unwrap();
        assert_eq!(route.stream, Stream::Log);
        assert_eq!(
            route.upstream,
            format!(
                r#"app.container_log_follow:{{"app_name":"plex","container_id":"{id}","tail_lines":50}}"#
            )
        );
        let plain = a.event("alert.list").unwrap();
        assert!(matches!(plain.upstream, Cow::Borrowed(_)));
        assert_eq!(plain.stream, Stream::Collection);
        for refused in [
            "app.container_log_follow".to_owned(),
            "app.container_log_follow:".to_owned(),
            topic.replace("50", "500"),
            topic.replace("app.container_log_follow", "app.stats"),
            topic.replace("app.container_log_follow", "App.container_log_follow"),
            format!(" {topic}"),
            r#"alert.list:{}"#.to_owned(),
        ] {
            assert!(a.event(&refused).is_none(), "{refused}");
        }
        // A test list knows no event with arguments.
        assert!(Allowlist::new(&[], &[]).event(&topic).is_none());
    }

    /// Actions live only in `JOB_METHODS` and `ACTIONS`, behind a params check and a dialog:
    /// no action slips into the plain lists.
    #[test]
    fn actions_are_only_checked_methods() {
        let writes = [
            ".create",
            ".update",
            ".delete",
            ".do_",
            ".start",
            ".stop",
            ".restart",
            ".upgrade",
            ".redeploy",
            "reboot",
            "shutdown",
            ".set",
            "dismiss",
            "restore",
            ".pull",
            ".rollback",
            "scrub",
        ];
        // Reads whose name looks like an action: checked one by one in the TrueNAS docs.
        const READS: &[&str] = &["app.upgrade_summary"];
        let params = PARAM_EVENTS.iter().map(|e| &e.name);
        for name in METHODS.iter().chain(EVENTS).chain(params) {
            if READS.contains(name) {
                continue;
            }
            assert!(
                !writes.iter().any(|w| name.contains(w)),
                "{name} looks like an action"
            );
        }
        let a = Allowlist::default();
        for job in ["app.start", "app.stop", "app.redeploy"] {
            assert!(!a.method(job), "{job} must go through its check");
            let check = a.job(job).unwrap();
            assert_eq!(
                check(&serde_json::json!(["plex"])),
                Some(serde_json::json!(["plex"]))
            );
            assert_eq!(check(&serde_json::json!(["Plex"])), None);
        }
        let upgrade = a.job("app.upgrade").unwrap();
        assert_eq!(upgrade(&serde_json::json!(["plex", {"values": {}}])), None);
        let pull = a.job("app.pull_images").unwrap();
        assert_eq!(
            pull(&serde_json::json!(["plex"])),
            Some(serde_json::json!(["plex", {"redeploy": true}]))
        );
        assert!(!a.method("app.upgrade") && !a.method("app.pull_images"));
        assert!(a.method("app.upgrade_summary") && a.job("app.upgrade_summary").is_none());
        for near in [
            "app.rollback",
            "app.upgrade.",
            "app.pull_images.",
            "app.delete",
            "app.start.",
            "App.start",
            "app",
        ] {
            assert!(a.job(near).is_none(), "{near:?}");
        }
        assert!(Allowlist::new(&[], &[]).job("app.start").is_none());
        for action in ["pool.snapshot.create", "pool.snapshot.delete"] {
            assert!(!a.method(action) && a.job(action).is_none(), "{action}");
            assert!(a.action(action).is_some(), "{action}");
        }
        let delete = a.action("pool.snapshot.delete").unwrap();
        assert_eq!(
            delete(&serde_json::json!(["tank@a"])),
            Some(serde_json::json!(["tank@a", {}]))
        );
        assert_eq!(
            delete(&serde_json::json!(["tank@a", {"recursive": true}])),
            None
        );
        for near in [
            "pool.snapshot.rollback",
            "pool.snapshot",
            "pool.snapshot.create.",
        ] {
            assert!(
                a.action(near).is_none() && a.job(near).is_none(),
                "{near:?}"
            );
        }
        assert!(a.method("pool.snapshot.query") && a.event("pool.snapshot.query").is_some());
        let scrub = a.job("pool.scrub.scrub").unwrap();
        assert_eq!(
            scrub(&serde_json::json!(["tank", "PAUSE"])),
            Some(serde_json::json!(["tank", "PAUSE"]))
        );
        assert!(!a.method("pool.scrub.scrub") && a.action("pool.scrub.scrub").is_none());
        for near in [
            "pool.scrub.run",
            "pool.scrub.create",
            "pool.scrub.update",
            "pool.scrub.delete",
            "pool.scrub.query",
            "pool.scrub",
            "pool.scrub.scrub.",
            "zpool.scrub.run",
        ] {
            assert!(
                !a.method(near) && a.job(near).is_none() && a.action(near).is_none(),
                "{near:?}"
            );
        }
        assert!(a.action("app.start").is_none());
        assert!(
            Allowlist::new(&[], &[])
                .action("pool.snapshot.create")
                .is_none()
        );
    }

    #[test]
    fn no_name_is_listed_twice() {
        // A name is a job or a plain action, never both.
        let mut actions: Vec<&str> = JOB_METHODS.iter().map(|(n, _)| *n).collect();
        actions.extend(ACTIONS.iter().map(|(n, _)| *n));
        for list in [METHODS, EVENTS, &actions] {
            let mut sorted = list.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), list.len());
        }
    }
}
