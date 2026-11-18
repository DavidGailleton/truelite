//! Shared helper for the reshaped topics: keep only the fields the web app selects.

use serde_json::{Map, Value};

/// `fields` with only the keys in `keep`; `None` when `fields` is not an object. Keys TrueNAS
/// adds later are dropped, missing ones stay missing.
pub(crate) fn keep_fields(fields: &Value, keep: &[&str]) -> Option<Value> {
    let entry = fields.as_object()?;
    let kept: Map<String, Value> = keep
        .iter()
        .filter_map(|&k| entry.get(k).map(|v| (k.to_owned(), v.clone())))
        .collect();
    Some(Value::Object(kept))
}
