//! `app.query` events: TrueNAS sends the whole app on every change (`active_workloads` with its
//! containers, ports and volumes, `portals`, `metadata` with the catalog description and
//! screenshots, `notes`), which the dashboard does not use. The bridge keeps only the fields the
//! web app selects in its own `app.query` call: [`compact`]. The app detail page (#31) needs the
//! workloads, so it treats an event about its app as a signal and queries that one app again
//! (filter by id, dotted `select`); the call result is never compacted.
//!
//! Event schema: <https://api.truenas.com/v25.10/api_events_app.query.html> (role `APPS_READ`).

use serde_json::Value;

use super::reshape::keep_fields;

/// The event name, as the browser subscribes to it.
pub const EVENT: &str = "app.query";

/// The fields forwarded; the web app selects the same ones (`APP_SELECT` in `web/src/lib/apps.ts`).
pub const FIELDS: &[&str] = &[
    "id",
    "name",
    "state",
    "upgrade_available",
    "image_updates_available",
    "custom_app",
    "version",
    "human_version",
];

/// The app with only [`FIELDS`]; `None` when `fields` is not an object.
pub fn compact(fields: &Value) -> Option<Value> {
    keep_fields(fields, FIELDS)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("testdata/app_event.json")).unwrap()
    }

    #[test]
    fn workloads_and_metadata_are_dropped_and_the_rest_kept() {
        let full = fixture();
        let out = compact(&full).unwrap();
        for unused in ["active_workloads", "metadata", "portals", "notes", "config"] {
            assert!(out.get(unused).is_none(), "{unused}");
        }
        for &k in FIELDS {
            assert_eq!(out[k], full[k], "{k}");
        }
        let wire = out.to_string().len();
        let all = full.to_string().len();
        assert!(wire < 512, "{wire} bytes");
        assert!(wire * 3 < all, "{wire} of {all} bytes");
    }

    #[test]
    fn missing_fields_stay_missing() {
        let out = compact(&json!({"id": "plex", "name": "plex", "extra": 1})).unwrap();
        assert_eq!(out, json!({"id": "plex", "name": "plex"}));
    }

    #[test]
    fn a_non_object_is_refused() {
        assert_eq!(compact(&json!("plex")), None);
        assert_eq!(compact(&Value::Null), None);
    }
}
