//! `update.status` results and events: whether a TrueNAS update is available, with its release
//! notes. TrueNAS sends the new version's `manifest` (checksums, file lists), which the web app
//! never shows; the bridge keeps only what the web app reads: [`compact`], [`compact_result`].
//!
//! Schemas: <https://api.truenas.com/v25.10/api_methods_update.status.html> and
//! <https://api.truenas.com/v25.10/api_events_update.status.html> (role `SYSTEM_UPDATE_READ`).
//! The event's `fields` are documented as `{status: <UpdateStatus>}`; both that and a flat
//! `UpdateStatus` are accepted until the NAS shows which one it sends.
//!
//! Installing: [`run_param`] checks the params of the `update.run` job (role
//! `SYSTEM_UPDATE_WRITE`; <https://api.truenas.com/v25.10/api_methods_update.run.html>). It
//! downloads (progress 0–50) then installs (50–100) and, with `reboot`, restarts the NAS 10 s
//! after it answers. A pre-check warning fails the job with `EAGAIN`; accepting it means calling
//! again with `resume` (middlewared TS-25.10.8 `plugins/update_/update.py`).

use serde_json::{Map, Value, json};

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

/// Longest train or version name accepted (TrueNAS uses e.g. `TrueNAS-SCALE-Goldeye`, `25.10.9`).
const NAME_MAX: usize = 64;

/// A train or version name: letters, digits, `.`, `_`, `-`.
fn name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= NAME_MAX
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// The params of `update.run`: the browser sends `[{train, version, reboot, resume?}]`; rebuilt
/// with exactly those keys (never `dataset_name`). `train` and `version` are both required, so
/// the NAS installs the version the user saw, not whatever is newest by then.
pub fn run_param(params: &Value) -> Option<Value> {
    let [Value::Object(attrs)] = params.as_array()?.as_slice() else {
        return None;
    };
    let train = attrs.get("train")?.as_str().filter(|t| name_ok(t))?;
    let version = attrs.get("version")?.as_str().filter(|v| name_ok(v))?;
    let reboot = attrs.get("reboot")?.as_bool()?;
    let resume = match attrs.get("resume") {
        None => false,
        Some(r) => r.as_bool()?,
    };
    Some(json!([{
        "train": train,
        "version": version,
        "reboot": reboot,
        "resume": resume,
    }]))
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
    fn restart_required_keeps_its_errname() {
        let applied = json!({"code": "ERROR", "status": null, "error": {
            "errname": "EREBOOTREQUIRED",
            "reason": "System update was already applied, system reboot is required."},
            "update_download_progress": null});
        assert_eq!(compact(&applied).unwrap(), applied);
    }

    #[test]
    fn run_params_are_rebuilt() {
        let base = json!({"train": "TrueNAS-SCALE-Goldeye", "version": "25.10.9", "reboot": true});
        let want = json!([{"train": "TrueNAS-SCALE-Goldeye", "version": "25.10.9",
            "reboot": true, "resume": false}]);
        assert_eq!(run_param(&json!([base])), Some(want));
        let mut extra = base.clone();
        extra["resume"] = json!(true);
        extra["dataset_name"] = json!("boot-pool/ROOT/evil");
        let out = run_param(&json!([extra])).unwrap();
        assert_eq!(out[0]["resume"], true);
        assert!(out[0].get("dataset_name").is_none());
    }

    #[test]
    fn bad_run_params_are_refused() {
        let ok = json!({"train": "TrueNAS-SCALE-Goldeye", "version": "25.10.9", "reboot": false});
        let with = |k: &str, v: Value| {
            let mut a = ok.clone();
            a[k] = v;
            json!([a])
        };
        let without = |k: &str| {
            let mut a = ok.clone();
            a.as_object_mut().unwrap().remove(k);
            json!([a])
        };
        for bad in [
            json!([]),
            json!(ok),
            json!([ok, ok]),
            json!(["25.10.9"]),
            without("train"),
            without("version"),
            without("reboot"),
            with("train", Value::Null),
            with("version", Value::Null),
            with("version", json!("")),
            with("version", json!("25.10.9; reboot")),
            with("version", json!("../x")),
            with("train", json!("a".repeat(65))),
            with("reboot", json!("yes")),
            with("resume", json!(1)),
        ] {
            assert_eq!(run_param(&bad), None, "{bad}");
        }
    }

    #[test]
    fn other_shapes_are_refused_or_left_alone() {
        assert_eq!(compact(&json!({"n": 1})), None);
        assert_eq!(compact(&json!({"status": "x"})), None);
        assert_eq!(compact(&Value::Null), None);
        assert_eq!(compact_result(json!([1])), json!([1]));
    }
}
