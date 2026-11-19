//! `update.status` results and events: whether a TrueNAS update is available, with its release
//! notes. TrueNAS sends the new version's `manifest` (checksums, file lists), which the web app
//! never shows; the bridge keeps only what the web app reads: [`compact`], [`compact_result`].
//!
//! Schemas: <https://api.truenas.com/v25.10/api_methods_update.status.html> and
//! <https://api.truenas.com/v25.10/api_events_update.status.html> (role `SYSTEM_UPDATE_READ`).
//! The event's `fields` are documented as `{status: <UpdateStatus>}`; both that and a flat
//! `UpdateStatus` are accepted until the NAS shows which one it sends.

use serde_json::{Map, Value};

use super::reshape::keep_fields_owned;

/// The event and method name, as the browser subscribes to or calls it.
pub const EVENT: &str = "update.status";

/// The top-level fields forwarded; `update_download_progress` is for the install step (#41).
const FIELDS: &[&str] = &["code", "status", "error", "update_download_progress"];

/// The fields of `status.new_version` forwarded: never `manifest`.
const NEW_VERSION_FIELDS: &[&str] = &["version", "release_notes", "release_notes_url"];

/// An event's `fields` cut down to a flat status; `None` when they hold no object with a `code`.
pub fn compact(fields: &Value) -> Option<Value> {
    let status = match fields.get("code") {
        Some(_) => fields,
        None => fields.get("status")?,
    };
    status.get("code")?;
    compact_owned(status.clone()).ok()
}

/// An `update.status` result cut down the same way; a result of another shape is returned
/// unchanged. The kept values are moved, not cloned.
pub fn compact_result(result: Value) -> Value {
    compact_owned(result).unwrap_or_else(|v| v)
}

fn compact_owned(value: Value) -> Result<Value, Value> {
    let mut out = keep_fields_owned(value, FIELDS)?;
    if let Some(Value::Object(new)) = out.pointer_mut("/status/new_version") {
        let kept: Map<String, Value> = NEW_VERSION_FIELDS
            .iter()
            .filter_map(|&k| new.remove(k).map(|v| (k.to_owned(), v)))
            .collect();
        *new = kept;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("testdata/update_status.json")).unwrap()
    }

    #[test]
    fn the_manifest_is_dropped_and_the_notes_kept() {
        let full = fixture();
        let out = compact_result(full.clone());
        let new = &out["status"]["new_version"];
        assert!(new.get("manifest").is_none());
        assert_eq!(new["version"], full["status"]["new_version"]["version"]);
        assert_eq!(
            new["release_notes_url"],
            full["status"]["new_version"]["release_notes_url"]
        );
        assert_eq!(
            out["status"]["current_version"],
            full["status"]["current_version"]
        );
        assert!(out.get("extra").is_none());
        assert!(out.to_string().len() * 2 < full.to_string().len());
    }

    #[test]
    fn wrapped_and_flat_events_give_the_same_status() {
        let flat = compact(&fixture()).unwrap();
        let wrapped = compact(&json!({"status": fixture()})).unwrap();
        assert_eq!(flat, wrapped);
        assert_eq!(flat, compact_result(fixture()));
    }

    #[test]
    fn up_to_date_and_error_forms_pass_through() {
        let current = json!({"code": "NORMAL", "status": {"current_version":
            {"train": "TrueNAS-25.10", "profile": "GENERAL", "matches_profile": true},
            "new_version": null}, "error": null, "update_download_progress": null});
        assert_eq!(compact(&current).unwrap(), current);
        let error = json!({"code": "ERROR", "status": null,
            "error": {"errname": "ENONET", "reason": "no network"},
            "update_download_progress": null});
        assert_eq!(compact(&error).unwrap(), error);
    }

    #[test]
    fn other_shapes_are_refused_or_left_alone() {
        assert_eq!(compact(&json!({"n": 1})), None);
        assert_eq!(compact(&json!({"status": "x"})), None);
        assert_eq!(compact(&Value::Null), None);
        assert_eq!(compact_result(json!([1])), json!([1]));
    }
}
