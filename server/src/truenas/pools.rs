//! `pool.query` events: TrueNAS sends the whole pool on every change, `topology` included (one
//! entry per vdev and disk, with I/O counters), which is most of the payload and unused by the
//! dashboard. The bridge keeps only the fields the web app selects in its own `pool.query` call:
//! [`compact`].
//!
//! Event schema: <https://api.truenas.com/v25.10/api_events_pool.query.html> (role `POOL_READ`).

use serde_json::Value;

use super::reshape::keep_fields;

/// The event name, as the browser subscribes to it.
pub const EVENT: &str = "pool.query";

/// The fields forwarded; the web app selects the same ones (`POOL_SELECT` in `web/src/lib/pools.ts`).
pub const FIELDS: &[&str] = &[
    "id",
    "name",
    "status",
    "healthy",
    "warning",
    "status_code",
    "status_detail",
    "size",
    "allocated",
    "free",
    "scan",
];

/// The pool with only [`FIELDS`]; `None` when `fields` is not an object. Keys TrueNAS adds later
/// are dropped, missing ones stay missing.
pub fn compact(fields: &Value) -> Option<Value> {
    keep_fields(fields, FIELDS)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("testdata/pool_event.json")).unwrap()
    }

    #[test]
    fn topology_is_dropped_and_the_rest_kept() {
        let full = fixture();
        let out = compact(&full).unwrap();
        assert!(out.get("topology").is_none());
        assert!(out.get("autotrim").is_none());
        for &k in FIELDS {
            assert_eq!(out[k], full[k], "{k}");
        }
        let wire = out.to_string().len();
        let all = full.to_string().len();
        assert!(wire < 1024, "{wire} bytes");
        assert!(wire * 3 < all, "{wire} of {all} bytes");
    }

    #[test]
    fn missing_fields_stay_missing() {
        let out = compact(&json!({"id": 1, "name": "tank", "extra": true})).unwrap();
        assert_eq!(out, json!({"id": 1, "name": "tank"}));
    }

    #[test]
    fn a_non_object_is_refused() {
        assert_eq!(compact(&json!([1, 2])), None);
        assert_eq!(compact(&Value::Null), None);
    }
}
