//! `pool.dataset.query`, call results and events: every ZFS property comes as an object
//! `{parsed, rawvalue, value, source, source_info}` (about 150 bytes), and events send the whole
//! dataset. The bridge keeps a few plain fields and turns each space property into a number of
//! bytes: [`compact_entry`]. The storage page's tree and the dashboard's root usage read only
//! those.
//!
//! Schema: <https://api.truenas.com/v25.10/api_methods_pool.dataset.query.html>, event
//! <https://api.truenas.com/v25.10/api_events_pool.dataset.query.html> (role `DATASET_READ`).

use serde_json::{Map, Value};

/// The method and the event name, as the browser calls and subscribes to them.
pub const EVENT: &str = "pool.dataset.query";

/// Plain fields kept as they are.
const FIELDS: &[&str] = &["id", "type", "encrypted", "locked", "key_loaded"];

/// ZFS properties kept, each as bytes (`DATASET_TREE_PARAMS` in `web/src/lib/datasets.ts`).
const PROPS: &[&str] = &["used", "available", "quota", "refquota"];

/// A property object as bytes: its `parsed` number, else its `rawvalue` read as an integer.
/// `None` for anything else (a missing or unreadable property is left out).
pub(super) fn bytes(prop: &Value) -> Option<u64> {
    let prop = prop.as_object()?;
    prop.get("parsed")
        .and_then(Value::as_u64)
        .or_else(|| prop.get("rawvalue")?.as_str()?.parse().ok())
}

/// One dataset with [`FIELDS`] and [`PROPS`] as numbers; `None` when it is not an object.
/// `children`, `user_properties`, `mountpoint` and every other property are dropped.
pub fn compact_entry(entry: &Value) -> Option<Value> {
    let obj = entry.as_object()?;
    let mut out: Map<String, Value> = FIELDS
        .iter()
        .filter_map(|&k| obj.get(k).map(|v| (k.to_owned(), v.clone())))
        .collect();
    for &k in PROPS {
        if let Some(n) = obj.get(k).and_then(bytes) {
            out.insert(k.to_owned(), Value::from(n));
        }
    }
    Some(Value::Object(out))
}

/// Event `fields`: the same as a call result entry.
pub fn compact(fields: &Value) -> Option<Value> {
    compact_entry(fields)
}

/// A `pool.dataset.query` result with every entry compacted; entries that are not objects are
/// dropped. A result that is not a list is returned unchanged.
pub fn compact_list(result: Value) -> Value {
    match result {
        Value::Array(list) => Value::Array(list.iter().filter_map(compact_entry).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("testdata/datasets.json")).unwrap()
    }

    #[test]
    fn a_list_keeps_plain_fields_and_bytes() {
        let full = fixture();
        let out = compact_list(full.clone());
        let list = out.as_array().unwrap();
        assert_eq!(list.len(), full.as_array().unwrap().len());
        assert_eq!(
            list[2],
            json!({"id": "tank/media/photos", "type": "FILESYSTEM", "encrypted": false,
                "locked": false, "key_loaded": null, "used": 120_000_000_000_u64,
                "available": 80_000_000_000_u64, "quota": 200_000_000_000_u64, "refquota": 0})
        );
        assert_eq!(list[5]["locked"], true);
        assert_eq!(list[4]["type"], "VOLUME");
        for gone in [
            "children",
            "user_properties",
            "mountpoint",
            "compression",
            "name",
        ] {
            assert!(list[0].get(gone).is_none(), "{gone}");
        }
        let wire = out.to_string().len();
        let all = full.to_string().len();
        assert!(wire * 3 < all, "{wire} of {all} bytes");
    }

    #[test]
    fn a_property_falls_back_to_its_raw_value() {
        let out = compact_entry(&json!({"id": "a",
            "used": {"parsed": null, "rawvalue": "42"},
            "available": {"parsed": "x", "rawvalue": "none"},
            "quota": 7}))
        .unwrap();
        assert_eq!(out, json!({"id": "a", "used": 42}));
    }

    #[test]
    fn a_bad_shape_never_panics() {
        assert_eq!(compact_list(json!({"x": 1})), json!({"x": 1}));
        assert_eq!(
            compact_list(json!([1, "a", {"id": "b"}])),
            json!([{"id": "b"}])
        );
        assert_eq!(compact(&Value::Null), None);
    }
}
