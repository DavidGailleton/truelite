//! `alert.list` events and results: every alert carries its class arguments (`args`) and its
//! e-mail state (`mail`), which the dashboard does not use, and `alert.list` takes no `select`.
//! The bridge keeps only the fields the web app reads, in events and in the call result:
//! [`compact`], [`compact_list`].
//!
//! Schemas: <https://api.truenas.com/v25.10/api_methods_alert.list.html> and
//! <https://api.truenas.com/v25.10/api_events_alert.list.html> (role `ALERT_LIST_READ`).

use serde_json::Value;

use super::reshape::{keep_fields, keep_fields_owned};

/// The event and method name, as the browser subscribes to or calls it.
pub const EVENT: &str = "alert.list";

/// The fields forwarded; the web app reads the same ones (`web/src/lib/alerts.ts`).
pub const FIELDS: &[&str] = &[
    "uuid",
    "id",
    "klass",
    "level",
    "formatted",
    "text",
    "datetime",
    "last_occurrence",
    "dismissed",
    "one_shot",
];

/// The alert with only [`FIELDS`]; `None` when `fields` is not an object.
pub fn compact(fields: &Value) -> Option<Value> {
    keep_fields(fields, FIELDS)
}

/// An `alert.list` result with every alert compacted. Entries that are not objects are kept as
/// they are, so the web app still sees them (and skips them); a result that is not a list is
/// returned unchanged. The kept values are moved, not cloned.
pub fn compact_list(result: Value) -> Value {
    match result {
        Value::Array(alerts) => Value::Array(
            alerts
                .into_iter()
                .map(|a| keep_fields_owned(a, FIELDS).unwrap_or_else(|a| a))
                .collect(),
        ),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("testdata/alert_event.json")).unwrap()
    }

    #[test]
    fn args_and_mail_are_dropped_and_the_rest_kept() {
        let full = fixture();
        let out = compact(&full).unwrap();
        for unused in ["args", "mail", "node", "key", "source"] {
            assert!(out.get(unused).is_none(), "{unused}");
        }
        for &k in FIELDS {
            assert_eq!(out[k], full[k], "{k}");
        }
        let wire = out.to_string().len();
        let all = full.to_string().len();
        assert!(wire * 3 < all * 2, "{wire} of {all} bytes");
    }

    #[test]
    fn missing_fields_stay_missing() {
        let out = compact(&json!({"uuid": "a", "level": "INFO", "extra": 1})).unwrap();
        assert_eq!(out, json!({"uuid": "a", "level": "INFO"}));
    }

    #[test]
    fn a_non_object_is_refused() {
        assert_eq!(compact(&json!("a")), None);
        assert_eq!(compact(&Value::Null), None);
    }

    #[test]
    fn a_result_list_is_compacted_entry_by_entry() {
        let out = compact_list(json!([fixture(), 7]));
        assert_eq!(out[0], compact(&fixture()).unwrap());
        assert_eq!(out[1], 7);
        assert_eq!(compact_list(json!({"a": 1})), json!({"a": 1}));
    }
}
