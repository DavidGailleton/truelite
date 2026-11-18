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
    // Dashboard: usable space of the pools' root datasets (DATASET_READ).
    "pool.dataset.query",
    // Dashboard card, apps page, app detail (one app by id), with `select` (APPS_READ).
    "app.query",
    // App detail: versions and changelog in the Update dialog (APPS_READ). A read, despite its name.
    "app.upgrade_summary",
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
    // Dashboard, apps page, app detail: app changes (APPS_READ). Reshaped by `truenas::apps` (no
    // workloads, metadata): the detail page queries its app again on each event about it.
    "app.query",
];

/// Events with arguments (`name:{json}`), each with the check that rebuilds the upstream name
/// from the browser's argument string. The browser's string is never forwarded as it is.
type ParamCheck = fn(&str) -> Option<String>;

const PARAM_EVENTS: &[(&str, ParamCheck)] = &[
    // App logs page: one container's log (APPS_READ). Arguments checked and rebuilt by
    // `truenas::apps::log_topic`; lines trimmed by `truenas::apps::compact_log_line`.
    (
        crate::truenas::apps::LOG_EVENT,
        crate::truenas::apps::log_topic,
    ),
];

/// Checks the browser's params of an action and rebuilds them; `None` refuses them.
pub type JobCheck = fn(&Value) -> Option<Value>;

/// Actions: TrueNAS job methods, each with the check that rebuilds its params. The bridge
/// follows their job and sends its progress (`truenas::jobs`). Every one has a confirmation
/// dialog in the web app.
const JOB_METHODS: &[(&str, JobCheck)] = &[
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
];

/// Exact-name lists of the methods and events the bridge forwards. No prefixes, no wildcards.
#[derive(Debug, Clone, Copy)]
pub struct Allowlist {
    methods: &'static [&'static str],
    events: &'static [&'static str],
    param_events: &'static [(&'static str, ParamCheck)],
    jobs: &'static [(&'static str, JobCheck)],
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
        }
    }

    /// Whether the browser may call `method`.
    pub fn method(&self, method: &str) -> bool {
        self.methods.contains(&method)
    }

    /// The params check of `method` when it is an allowed action (a job method).
    pub fn job(&self, method: &str) -> Option<JobCheck> {
        self.jobs
            .iter()
            .find(|(n, _)| *n == method)
            .map(|(_, check)| *check)
    }

    /// The name to subscribe to on the NAS when the browser may follow `topic`: the topic
    /// itself for a listed event, or the rebuilt `name:{json}` of an event with arguments.
    pub fn event<'a>(&self, topic: &'a str) -> Option<Cow<'a, str>> {
        if self.events.contains(&topic) {
            return Some(Cow::Borrowed(topic));
        }
        let (name, arg) = topic.split_once(':')?;
        let (_, check) = self.param_events.iter().find(|(n, _)| *n == name)?;
        check(arg).map(Cow::Owned)
    }
}

impl Default for Allowlist {
    fn default() -> Self {
        Self {
            methods: METHODS,
            events: EVENTS,
            param_events: PARAM_EVENTS,
            jobs: JOB_METHODS,
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
        assert_eq!(
            a.event(&topic).unwrap(),
            format!(
                r#"app.container_log_follow:{{"app_name":"plex","container_id":"{id}","tail_lines":50}}"#
            )
        );
        assert!(matches!(a.event("alert.list").unwrap(), Cow::Borrowed(_)));
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

    /// Actions live only in `JOB_METHODS`, behind a params check and a dialog: no action
    /// slips into the plain lists.
    #[test]
    fn actions_are_only_job_methods() {
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
        ];
        // Reads whose name looks like an action: checked one by one in the TrueNAS docs.
        const READS: &[&str] = &["app.upgrade_summary"];
        let params = PARAM_EVENTS.iter().map(|(n, _)| n);
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
    }

    #[test]
    fn no_name_is_listed_twice() {
        let jobs: Vec<&str> = JOB_METHODS.iter().map(|(n, _)| *n).collect();
        for list in [METHODS, EVENTS, &jobs] {
            let mut sorted = list.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), list.len());
        }
    }
}
