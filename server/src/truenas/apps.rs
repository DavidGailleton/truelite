//! `app.query` events: TrueNAS sends the whole app on every change (`active_workloads` with its
//! containers, ports and volumes, `portals`, `metadata` with the catalog description and
//! screenshots, `notes`), which the dashboard does not use. The bridge keeps only the fields the
//! web app selects in its own `app.query` call: [`compact`]. The app detail page (#31) needs the
//! workloads, so it treats an event about its app as a signal and queries that one app again
//! (filter by id, dotted `select`); the call result is never compacted.
//!
//! Event schema: <https://api.truenas.com/v25.10/api_events_app.query.html> (role `APPS_READ`).
//!
//! Container logs (#32): `app.container_log_follow` is an event with arguments
//! (`app.container_log_follow:{"app_name", "container_id", "tail_lines"}`). TrueNAS names its
//! notifications by that full string and shares one docker stream per exact argument string
//! (`common/event_source/manager.py`, TS-25.10.8). [`log_topic`] checks the browser's arguments
//! and builds the upstream name; [`compact_log_line`] trims each line. Schema:
//! <https://api.truenas.com/v25.10/api_events_app.container_log_follow.html> (role `APPS_READ`).
//!
//! Actions (#33): `app.start`, `app.stop`, `app.redeploy` are jobs taking `[app_name]` (role
//! `APPS_WRITE`); [`app_name_param`] checks and rebuilds their params.
//!
//! Updates (#34): `app.upgrade` (`[app_name, {app_version, values, snapshot_hostpaths}]`) when
//! `upgrade_available`, else `app.pull_images` (`[app_name, {redeploy}]`) for newer images of the
//! same version (middlewared `plugins/apps/upgrade.py`, `pull_images.py`, TS-25.10.8). Both are
//! jobs (role `APPS_WRITE`). [`app_upgrade_param`] always asks for the latest version and never
//! forwards `values`; [`app_pull_images_param`] always redeploys.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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

/// The container log event, without its arguments.
pub const LOG_EVENT: &str = "app.container_log_follow";

/// Most lines of history a log subscription may ask for. TrueNAS sends them in one burst; this
/// stays under the client's notification buffer (256) while `core.subscribe` is awaited, and a
/// `null` (the whole log) is never allowed.
pub const LOG_TAIL_MAX: u32 = 200;

/// Longest log line forwarded, in bytes; longer ones are cut and end with `…`.
pub const LOG_LINE_MAX: usize = 8 * 1024;

/// The arguments of [`LOG_EVENT`], in the order TrueNAS documents them.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LogArgs {
    app_name: String,
    container_id: String,
    tail_lines: u32,
}

/// TrueNAS app names: lowercase letters, digits and dashes, starting with a letter, at most 40.
fn app_name_ok(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && name.len() <= 40
        && !name.ends_with('-')
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The params of the app actions (`app.start`, `app.stop`, `app.redeploy`: `[app_name]`),
/// rebuilt from a checked name; `None` for anything else (another length, a bad name).
pub fn app_name_param(params: &Value) -> Option<Value> {
    match params.as_array()?.as_slice() {
        [Value::String(name)] if app_name_ok(name) => Some(json!([name])),
        _ => None,
    }
}

/// The params of `app.upgrade`: the browser sends `[app_name]` or
/// `[app_name, {"snapshot_hostpaths": bool}]`; rebuilt as
/// `[app_name, {"app_version": "latest", "snapshot_hostpaths": bool}]`. Any other key (`values`,
/// `app_version`) or shape is refused.
pub fn app_upgrade_param(params: &Value) -> Option<Value> {
    let (name, snapshot) = match params.as_array()?.as_slice() {
        [Value::String(name)] => (name, false),
        [Value::String(name), Value::Object(options)] => match options.get("snapshot_hostpaths") {
            Some(Value::Bool(b)) if options.len() == 1 => (name, *b),
            None if options.is_empty() => (name, false),
            _ => return None,
        },
        _ => return None,
    };
    app_name_ok(name)
        .then(|| json!([name, {"app_version": "latest", "snapshot_hostpaths": snapshot}]))
}

/// The params of `app.pull_images`: `[app_name]` from the browser, rebuilt as
/// `[app_name, {"redeploy": true}]` (newer images are only useful once the app runs them).
pub fn app_pull_images_param(params: &Value) -> Option<Value> {
    match params.as_array()?.as_slice() {
        [Value::String(name)] if app_name_ok(name) => Some(json!([name, {"redeploy": true}])),
        _ => None,
    }
}

/// Docker container ids: 64 lowercase hex digits, or the 12-digit short form.
fn container_id_ok(id: &str) -> bool {
    matches!(id.len(), 12 | 64) && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The upstream subscription name for the browser's argument string (the part after
/// `app.container_log_follow:`), rebuilt from the checked values; `None` if anything is off.
pub fn log_topic(arg: &str) -> Option<String> {
    let args: LogArgs = serde_json::from_str(arg).ok()?;
    let ok = app_name_ok(&args.app_name)
        && container_id_ok(&args.container_id)
        && (1..=LOG_TAIL_MAX).contains(&args.tail_lines);
    if !ok {
        return None;
    }
    Some(format!(
        "{LOG_EVENT}:{}",
        serde_json::to_string(&args).ok()?
    ))
}

/// One log event `{data, timestamp}` as `{time, text}`: the trailing newline dropped, the text
/// cut at [`LOG_LINE_MAX`] bytes. `None` when `data` is not a string.
pub fn compact_log_line(fields: &Value) -> Option<Value> {
    let data = fields.get("data")?.as_str()?;
    let data = data.trim_end_matches(['\n', '\r']);
    let text = if data.len() > LOG_LINE_MAX {
        let mut end = LOG_LINE_MAX;
        while !data.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &data[..end])
    } else {
        data.to_owned()
    };
    let time = fields.get("timestamp").filter(|t| t.is_string());
    Some(json!({"time": time.cloned().unwrap_or(Value::Null), "text": text}))
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

    const ID: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn a_log_topic_is_rebuilt_in_a_fixed_order() {
        let arg =
            format!(r#" {{"tail_lines": 200, "container_id": "{ID}", "app_name": "jellyfin"}} "#);
        assert_eq!(
            log_topic(&arg).unwrap(),
            format!(
                r#"app.container_log_follow:{{"app_name":"jellyfin","container_id":"{ID}","tail_lines":200}}"#
            )
        );
        let short = r#"{"app_name":"a-1","container_id":"0123456789ab","tail_lines":1}"#;
        assert!(log_topic(short).is_some());
    }

    #[test]
    fn odd_log_arguments_are_refused() {
        let ok = |app: &str, id: &str, tail: &str| {
            log_topic(&format!(
                r#"{{"app_name":"{app}","container_id":"{id}","tail_lines":{tail}}}"#
            ))
            .is_some()
        };
        assert!(ok("plex", ID, "200"));
        for tail in ["0", "201", "null", "-1", "1.5", "\"5\""] {
            assert!(!ok("plex", ID, tail), "{tail}");
        }
        for app in [
            "",
            "Plex",
            "1plex",
            "plex-",
            "pl ex",
            "plex/..",
            &"a".repeat(41),
        ] {
            assert!(!ok(app, ID, "5"), "{app}");
        }
        let upper = ID.to_uppercase();
        for id in [
            "",
            "0123456789a",
            &ID[..63],
            &upper,
            &format!("{ID}0"),
            "0123456789ag",
        ] {
            assert!(!ok("plex", id, "5"), "{id}");
        }
        for arg in [
            format!(r#"{{"app_name":"plex","container_id":"{ID}"}}"#),
            format!(r#"{{"app_name":"plex","container_id":"{ID}","tail_lines":5,"x":1}}"#),
            format!(r#"{{"app_name":"plex","container_id":"{ID}","tail_lines":5}} x"#),
            "[]".to_owned(),
            String::new(),
        ] {
            assert!(log_topic(&arg).is_none(), "{arg}");
        }
    }

    #[test]
    fn log_lines_lose_their_newline_and_keep_their_time() {
        let line = compact_log_line(
            &json!({"data": "hello\r\n", "timestamp": "2026-11-18 10:00:01+00:00"}),
        );
        assert_eq!(
            line.unwrap(),
            json!({"time": "2026-11-18 10:00:01+00:00", "text": "hello"})
        );
        let bare = compact_log_line(&json!({"data": "x", "timestamp": null})).unwrap();
        assert_eq!(bare, json!({"time": null, "text": "x"}));
        assert_eq!(compact_log_line(&json!({"timestamp": "t"})), None);
        assert_eq!(compact_log_line(&json!({"data": 3})), None);
    }

    #[test]
    fn long_log_lines_are_cut_on_a_char_boundary() {
        // 'é' is 2 bytes: an odd prefix puts the limit inside one.
        let data = format!("x{}", "é".repeat(LOG_LINE_MAX));
        let text = compact_log_line(&json!({"data": data})).unwrap()["text"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(text.ends_with('…'));
        assert!(text.len() <= LOG_LINE_MAX + '…'.len_utf8());
        assert!(text.len() > LOG_LINE_MAX - 2);
    }

    #[test]
    fn action_params_are_one_checked_app_name() {
        assert_eq!(app_name_param(&json!(["plex"])), Some(json!(["plex"])));
        assert_eq!(
            app_name_param(&json!(["my-app2"])),
            Some(json!(["my-app2"]))
        );
        for refused in [
            json!([]),
            json!(["Plex"]),
            json!(["plex", "x"]),
            json!([{"app_name": "plex"}]),
            json!(["../x"]),
            json!(["plex-"]),
            json!(["2plex"]),
            json!([""]),
            json!("plex"),
            json!([1]),
        ] {
            assert_eq!(app_name_param(&refused), None, "{refused}");
        }
    }

    #[test]
    fn upgrade_params_ask_for_the_latest_version_and_nothing_else() {
        let latest = |name: &str, snap: bool| json!([name, {"app_version": "latest", "snapshot_hostpaths": snap}]);
        assert_eq!(
            app_upgrade_param(&json!(["plex"])),
            Some(latest("plex", false))
        );
        assert_eq!(
            app_upgrade_param(&json!(["plex", {}])),
            Some(latest("plex", false))
        );
        assert_eq!(
            app_upgrade_param(&json!(["plex", {"snapshot_hostpaths": true}])),
            Some(latest("plex", true))
        );
        for refused in [
            json!([]),
            json!(["Plex"]),
            json!(["plex", {"snapshot_hostpaths": "yes"}]),
            json!(["plex", {"snapshot_hostpaths": true, "values": {"x": 1}}]),
            json!(["plex", {"values": {}}]),
            json!(["plex", {"app_version": "1.0.0"}]),
            json!(["plex", true]),
            json!(["plex", {}, {}]),
            json!("plex"),
        ] {
            assert_eq!(app_upgrade_param(&refused), None, "{refused}");
        }
    }

    #[test]
    fn pull_images_params_always_redeploy() {
        assert_eq!(
            app_pull_images_param(&json!(["plex"])),
            Some(json!(["plex", {"redeploy": true}]))
        );
        for refused in [
            json!([]),
            json!(["Plex"]),
            json!(["plex", {"redeploy": false}]),
            json!(["plex", {}]),
        ] {
            assert_eq!(app_pull_images_param(&refused), None, "{refused}");
        }
    }
}
