//! The reshaped topics: large events cut down to what the web app shows, once per topic when
//! they arrive ([`reshaper`], used by [`Topics`](super::events::Topics)), and the shared helpers
//! that keep only the fields the web app selects.

use serde_json::{Map, Value};

use super::{alerts, apps, datasets, pools, realtime, snapshots, update};

/// The function that cuts an event's `fields` down to what the web app shows, for the large ones
/// (documented in `docs/ws-protocol.md`, "Reshaped topics"). It returns `None` for a payload of
/// another shape, which is then skipped.
pub(super) fn reshaper(event: &str) -> Option<fn(&Value) -> Option<Value>> {
    match event {
        realtime::EVENT => Some(realtime::compact),
        pools::EVENT => Some(pools::compact),
        apps::EVENT => Some(apps::compact),
        alerts::EVENT => Some(alerts::compact),
        datasets::EVENT => Some(datasets::compact),
        snapshots::EVENT => Some(snapshots::compact),
        update::EVENT => Some(update::compact),
        _ => None,
    }
}

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

/// [`keep_fields`] on an owned value, moving the kept values instead of cloning them. A value
/// that is not an object is given back unchanged as the error.
pub(crate) fn keep_fields_owned(value: Value, keep: &[&str]) -> Result<Value, Value> {
    let Value::Object(mut entry) = value else {
        return Err(value);
    };
    let kept: Map<String, Value> = keep
        .iter()
        .filter_map(|&k| entry.remove(k).map(|v| (k.to_owned(), v)))
        .collect();
    Ok(Value::Object(kept))
}
