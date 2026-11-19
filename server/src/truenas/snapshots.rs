//! `pool.snapshot.*`: one dataset's snapshots on the storage page, taken and deleted from there.
//!
//! - Call results and events carry every requested ZFS property as an object
//!   `{value, rawvalue, source, parsed}`. The bridge keeps a few plain fields and turns the
//!   properties into numbers: [`compact_entry`].
//! - `pool.snapshot.create` and `pool.snapshot.delete` are plain calls, not jobs (middlewared
//!   TS-25.10.8, `plugins/pool_/snapshot.py`). Their params are rebuilt from checked values
//!   ([`create_param`], [`delete_param`]): never a naming schema, an exclude list, properties,
//!   VMware sync, a recursive or deferred delete.
//!
//! Schema: <https://api.truenas.com/v25.10/api_methods_pool.snapshot.query.html> (role
//! `SNAPSHOT_READ`), `pool.snapshot.create` (`SNAPSHOT_WRITE`), `pool.snapshot.delete`
//! (`SNAPSHOT_DELETE`), event `pool.snapshot.query` (`SNAPSHOT_READ`).

use serde_json::{Map, Value, json};

use super::datasets::bytes;

/// The method and the event name, as the browser calls and subscribes to them.
pub const EVENT: &str = "pool.snapshot.query";

/// Plain fields kept as they are.
const FIELDS: &[&str] = &["id", "dataset", "snapshot_name"];

/// Space properties kept, each as bytes (`SNAPSHOT_PROPS` in `web/src/lib/snapshots.ts`).
const SPACE: &[&str] = &["used", "referenced"];

/// Longest snapshot name accepted (ZFS allows 255 bytes for the whole `dataset@name`).
const NAME_MAX: usize = 200;

/// Longest dataset path accepted.
const DATASET_MAX: usize = 255;

/// The creation time in seconds since the epoch: `rawvalue` is the ZFS number of seconds;
/// `parsed` may be that number or a date as `{"$date": milliseconds}`.
fn created(prop: &Value) -> Option<u64> {
    let prop = prop.as_object()?;
    if let Some(s) = prop.get("rawvalue").and_then(Value::as_str)
        && let Ok(n) = s.parse()
    {
        return Some(n);
    }
    match prop.get("parsed")? {
        Value::Number(n) => n.as_u64(),
        Value::Object(date) => date.get("$date")?.as_u64().map(|ms| ms / 1000),
        _ => None,
    }
}

/// One snapshot with [`FIELDS`], [`SPACE`] as bytes, `created` as seconds and `txg` (creation
/// order) as a number; `None` when it is not an object. Every other field is dropped.
pub fn compact_entry(entry: &Value) -> Option<Value> {
    let obj = entry.as_object()?;
    let mut out: Map<String, Value> = FIELDS
        .iter()
        .filter_map(|&k| obj.get(k).map(|v| (k.to_owned(), v.clone())))
        .collect();
    if let Some(txg) = obj
        .get("createtxg")
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<u64>().ok())
    {
        out.insert("txg".to_owned(), Value::from(txg));
    }
    if let Some(props) = obj.get("properties").and_then(Value::as_object) {
        for &k in SPACE {
            if let Some(n) = props.get(k).and_then(bytes) {
                out.insert(k.to_owned(), Value::from(n));
            }
        }
        if let Some(s) = props.get("creation").and_then(created) {
            out.insert("created".to_owned(), Value::from(s));
        }
    }
    Some(Value::Object(out))
}

/// Event `fields`: the same as a call result entry.
pub fn compact(fields: &Value) -> Option<Value> {
    compact_entry(fields)
}

/// A `pool.snapshot.query` result with every entry compacted; entries that are not objects are
/// dropped. A result that is not a list (`get`) is compacted alone, or returned unchanged.
pub fn compact_list(result: Value) -> Value {
    match result {
        Value::Array(list) => Value::Array(list.iter().filter_map(compact_entry).collect()),
        Value::Object(_) => compact_entry(&result).unwrap_or(result),
        other => other,
    }
}

/// A ZFS name component: letters, digits, `_ - : .` and inner spaces (`zfs_component_namecheck`).
fn component_ok(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with(' ')
        && !s.ends_with(' ')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b':' | b'.' | b' '))
}

/// A snapshot name, the part after `@`.
fn snapshot_name_ok(name: &str) -> bool {
    name.len() <= NAME_MAX && component_ok(name)
}

/// A dataset path: `pool/child/…`, each part a valid component.
fn dataset_ok(path: &str) -> bool {
    path.len() <= DATASET_MAX && path.split('/').all(component_ok)
}

/// The params of `pool.snapshot.create`: the browser sends
/// `[{"dataset": path, "name": name, "recursive": bool}]` (`recursive` may be left out), rebuilt
/// with those three keys only. Any other key or shape is refused.
pub fn create_param(params: &Value) -> Option<Value> {
    let [Value::Object(data)] = params.as_array()?.as_slice() else {
        return None;
    };
    let recursive = match data.get("recursive") {
        Some(Value::Bool(b)) => *b,
        None => false,
        Some(_) => return None,
    };
    let expected = 2 + usize::from(data.contains_key("recursive"));
    let (Some(Value::String(dataset)), Some(Value::String(name))) =
        (data.get("dataset"), data.get("name"))
    else {
        return None;
    };
    (data.len() == expected && dataset_ok(dataset) && snapshot_name_ok(name))
        .then(|| json!([{"dataset": dataset, "name": name, "recursive": recursive}]))
}

/// The params of `pool.snapshot.delete`: `[id]` from the browser (`dataset@name`), rebuilt as
/// `[id, {}]`: one snapshot, no recursive or deferred delete.
pub fn delete_param(params: &Value) -> Option<Value> {
    let [Value::String(id)] = params.as_array()?.as_slice() else {
        return None;
    };
    let (dataset, name) = id.split_once('@')?;
    (dataset_ok(dataset) && snapshot_name_ok(name)).then(|| json!([id, {}]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("testdata/snapshots.json")).unwrap()
    }

    #[test]
    fn a_list_keeps_plain_fields_and_numbers() {
        let full = fixture();
        let out = compact_list(full.clone());
        let list = out.as_array().unwrap();
        assert_eq!(list.len(), full.as_array().unwrap().len());
        assert_eq!(
            list[0],
            json!({"id": "tank/media@manual-2026-11-19_10-00", "dataset": "tank/media",
                "snapshot_name": "manual-2026-11-19_10-00", "txg": 120_345,
                "used": 1_048_576, "referenced": 52_428_800_000_u64, "created": 1_795_082_400})
        );
        for gone in ["properties", "pool", "name", "type", "createtxg"] {
            assert!(list[0].get(gone).is_none(), "{gone}");
        }
        let wire = out.to_string().len();
        let all = full.to_string().len();
        assert!(wire * 2 < all, "{wire} of {all} bytes");
    }

    #[test]
    fn creation_reads_parsed_dates_too() {
        let date = json!({"properties": {"creation": {"rawvalue": "x", "parsed": {"$date": 1_795_082_400_123_u64}}}});
        assert_eq!(compact_entry(&date).unwrap()["created"], 1_795_082_400);
        let secs = json!({"properties": {"creation": {"parsed": 5}}});
        assert_eq!(compact_entry(&secs).unwrap()["created"], 5);
    }

    #[test]
    fn a_bad_shape_never_panics() {
        assert_eq!(compact_list(json!("x")), json!("x"));
        assert_eq!(
            compact_list(json!([1, {"id": "a@b", "createtxg": 7, "properties": []}])),
            json!([{"id": "a@b"}])
        );
        assert_eq!(compact(&Value::Null), None);
    }

    #[test]
    fn create_params_are_rebuilt() {
        assert_eq!(
            create_param(&json!([{"dataset": "tank/media", "name": "manual-1"}])),
            Some(json!([{"dataset": "tank/media", "name": "manual-1", "recursive": false}]))
        );
        assert_eq!(
            create_param(&json!([{"recursive": true, "name": "a b", "dataset": "tank"}])),
            Some(json!([{"dataset": "tank", "name": "a b", "recursive": true}]))
        );
        let long = "a".repeat(NAME_MAX + 1);
        for bad in [
            json!([]),
            json!([{"dataset": "tank"}]),
            json!([{"dataset": "tank", "name": "x", "naming_schema": "auto-%Y"}]),
            json!([{"dataset": "tank", "name": "x", "properties": {}}]),
            json!([{"dataset": "tank", "name": "x", "recursive": "yes"}]),
            json!([{"dataset": "tank", "name": "x"}, {}]),
            json!([{"dataset": "tank@x", "name": "x"}]),
            json!([{"dataset": "tank/", "name": "x"}]),
            json!([{"dataset": "/tank", "name": "x"}]),
            json!([{"dataset": "tank", "name": ""}]),
            json!([{"dataset": "tank", "name": " x"}]),
            json!([{"dataset": "tank", "name": "x/y"}]),
            json!([{"dataset": "tank", "name": "x@y"}]),
            json!([{"dataset": "tank", "name": "%Y"}]),
            json!([{"dataset": "tank", "name": long}]),
            json!({"dataset": "tank", "name": "x"}),
        ] {
            assert_eq!(create_param(&bad), None, "{bad}");
        }
    }

    #[test]
    fn delete_params_are_one_snapshot() {
        assert_eq!(
            delete_param(&json!(["tank/media@auto-2026-11-19_00-00"])),
            Some(json!(["tank/media@auto-2026-11-19_00-00", {}]))
        );
        for bad in [
            json!(["tank/media"]),
            json!(["tank@a@b"]),
            json!(["@a"]),
            json!(["tank@"]),
            json!(["tank@a", {"recursive": true}]),
            json!(["tank@a", {"defer": true}]),
            json!([{"id": "tank@a"}]),
            json!([]),
        ] {
            assert_eq!(delete_param(&bad), None, "{bad}");
        }
    }
}
