//! What the browser may reach through the WebSocket bridge. Nothing else is forwarded to the
//! NAS. The lists grow per phase: read methods first, then actions (each behind a confirmation
//! dialog in the web app). Every name is checked against <https://api.truenas.com/v25.10/>.
//!
//! Calls run as the logged-in TrueNAS user, so TrueNAS roles still apply on top of this list.

/// Methods the web app may call, by exact name.
const METHODS: &[&str] = &[
    // Shell: host name and version (READONLY_ADMIN).
    "system.info",
    // Shell: alert badge (ALERT_LIST_READ).
    "alert.list",
    // Dashboard: pool health and raw space, with `select` (POOL_READ).
    "pool.query",
    // Dashboard: usable space of the pools' root datasets (DATASET_READ).
    "pool.dataset.query",
    // Dashboard: apps summary, with `select` (APPS_READ).
    "app.query",
];

/// Events the web app may subscribe to, by exact name.
const EVENTS: &[&str] = &[
    // Shell: live alert badge (ALERT_LIST_READ).
    "alert.list",
    // Dashboard: live CPU, RAM, network (REPORTING_READ). Reshaped by `truenas::realtime`.
    "reporting.realtime",
    // Dashboard: pool changes (POOL_READ). Reshaped by `truenas::pools` (no topology).
    "pool.query",
    // Dashboard: scrub and resilver progress (POOL_SCRUB_READ).
    "pool.scan",
    // Dashboard: app changes (APPS_READ). Reshaped by `truenas::apps` (no workloads, metadata).
    "app.query",
];

/// Exact-name lists of the methods and events the bridge forwards. No prefixes, no wildcards.
#[derive(Debug, Clone, Copy)]
pub struct Allowlist {
    methods: &'static [&'static str],
    events: &'static [&'static str],
}

impl Allowlist {
    /// A custom list (tests).
    #[cfg(test)]
    pub const fn new(methods: &'static [&'static str], events: &'static [&'static str]) -> Self {
        Self { methods, events }
    }

    /// Whether the browser may call `method`.
    pub fn method(&self, method: &str) -> bool {
        self.methods.contains(&method)
    }

    /// Whether the browser may subscribe to `event`.
    pub fn event(&self, event: &str) -> bool {
        self.events.contains(&event)
    }
}

impl Default for Allowlist {
    fn default() -> Self {
        Self {
            methods: METHODS,
            events: EVENTS,
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
        assert!(a.event("alert.list"));
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
        assert!(a.event("reporting.realtime"));
        assert!(a.method("pool.query") && a.method("pool.dataset.query"));
        assert!(a.event("pool.query") && a.event("pool.scan"));
        assert!(!a.method("pool.scan") && !a.method("pool.dataset"));
        assert!(a.method("app.query") && a.event("app.query"));
        for near in ["app", "app.stats", "app.start", "app.query.", "apps.query"] {
            assert!(!a.method(near) && !a.event(near), "{near:?}");
        }
        assert!(!a.event("alert"));
        assert!(!a.event("reporting.realtime:{\"interval\": 2}"));
        assert!(!a.method(""));
    }

    /// Phase 2 is read-only: no action may slip in before its phase and its dialog.
    #[test]
    fn no_action_is_listed_yet() {
        let writes = [
            ".create", ".update", ".delete", ".do_", ".start", ".stop", ".restart", ".upgrade",
            "reboot", "shutdown", ".set", "dismiss", "restore",
        ];
        for name in METHODS.iter().chain(EVENTS) {
            assert!(
                !writes.iter().any(|w| name.contains(w)),
                "{name} looks like an action"
            );
        }
    }

    #[test]
    fn no_name_is_listed_twice() {
        for list in [METHODS, EVENTS] {
            let mut sorted = list.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), list.len());
        }
    }
}
