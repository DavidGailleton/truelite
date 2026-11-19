//! `disk.*`: the disks page (`/storage/disks`): every disk with its pool and temperature.
//!
//! - `disk.query` answers with `passwd` and `kmip_uid` when asked (`extra.passwords`): the bridge
//!   keeps only [`FIELDS`] ([`compact_list`]), so they never reach the browser.
//! - `disk.temperatures` answers `{name: °C}`, or `{name: [°C, critical °C]}` with
//!   `include_thresholds`; [`compact_temps`] gives `{name: {temp, crit}}` for both.
//! - `disk.temperature_agg` (`{name: {min, max, avg}}`, at most 7 days) is small and goes as is.
//! - No `disk.query` event: it is documented, but middlewared sets `event_send = False`.
//! - SMART is not in the 25.10 API: `smart.test.*`, `smart.config` and `disk.smart_attributes`
//!   were removed (issue #61).
//!
//! Schema: <https://api.truenas.com/v25.10/api_methods_disk.query.html> (role `DISK_READ`),
//! <https://api.truenas.com/v25.10/api_methods_disk.temperatures.html> and
//! <https://api.truenas.com/v25.10/api_methods_disk.temperature_agg.html> (`REPORTING_READ`).

use serde_json::{Map, Value, json};

use super::reshape::keep_fields_owned;

/// The method that lists disks.
pub const QUERY: &str = "disk.query";

/// The method that reads the current temperatures.
pub const TEMPERATURES: &str = "disk.temperatures";

/// Fields kept from each disk (`DISK_SELECT` in `web/src/lib/disks.ts`).
const FIELDS: &[&str] = &[
    "identifier",
    "name",
    "serial",
    "model",
    "size",
    "type",
    "rotationrate",
    "bus",
    "pool",
    "description",
];

/// A `disk.query` result with only [`FIELDS`] in each entry; entries that are not objects are
/// dropped. A result that is not a list (`get`) is cut down alone, or returned unchanged.
pub fn compact_list(result: Value) -> Value {
    match result {
        Value::Array(list) => Value::Array(
            list.into_iter()
                .filter_map(|d| keep_fields_owned(d, FIELDS).ok())
                .collect(),
        ),
        Value::Object(_) => keep_fields_owned(result, FIELDS).unwrap_or_else(|v| v),
        other => other,
    }
}

/// A temperature in °C: a number, else `null`.
fn degrees(v: Option<&Value>) -> Value {
    v.and_then(Value::as_f64).map_or(Value::Null, Value::from)
}

/// A `disk.temperatures` result as `{name: {temp, crit}}`: `temp` and `crit` are numbers or
/// `null` (no reading, no threshold). A result that is not an object gives `{}`.
pub fn compact_temps(result: Value) -> Value {
    let Value::Object(map) = result else {
        return Value::Object(Map::new());
    };
    let out: Map<String, Value> = map
        .into_iter()
        .map(|(name, v)| {
            let (temp, crit) = match &v {
                Value::Array(pair) => (degrees(pair.first()), degrees(pair.get(1))),
                one => (degrees(Some(one)), Value::Null),
            };
            (name, json!({"temp": temp, "crit": crit}))
        })
        .collect();
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn disks_keep_only_the_shown_fields() {
        let list = compact_list(fixture(include_str!("testdata/disks.json")));
        let first = &list[0];
        assert_eq!(first["name"], "sda");
        assert_eq!(first["pool"], "tank");
        assert_eq!(first["rotationrate"], 7200);
        for gone in ["passwd", "kmip_uid", "hddstandby", "devname", "enclosure"] {
            assert!(first.get(gone).is_none(), "{gone} kept");
        }
        // An SSD: no rotation rate, no pool; missing fields stay missing.
        assert_eq!(list[1]["rotationrate"], Value::Null);
        assert!(list[1].get("description").is_none());
        // Entries that are not objects are dropped.
        assert_eq!(list.as_array().unwrap().len(), 2);
    }

    #[test]
    fn one_disk_from_get_is_cut_down_too() {
        let one = compact_list(json!({"name": "sda", "passwd": "x"}));
        assert_eq!(one, json!({"name": "sda"}));
        assert_eq!(compact_list(json!("odd")), json!("odd"));
    }

    #[test]
    fn temperatures_come_in_one_shape() {
        let t = compact_temps(fixture(include_str!("testdata/disk_temps.json")));
        assert_eq!(t["sda"], json!({"temp": 36.0, "crit": 60.0}));
        // No reading (asleep or no sensor), no threshold.
        assert_eq!(t["sdb"], json!({"temp": null, "crit": null}));
        // Without `include_thresholds`: a plain number.
        assert_eq!(t["nvme0n1"], json!({"temp": 41.0, "crit": null}));
        assert_eq!(t["sdc"], json!({"temp": 33.0, "crit": null}));
    }

    #[test]
    fn a_bad_temperature_shape_never_panics() {
        assert_eq!(compact_temps(json!([1, 2])), json!({}));
        assert_eq!(compact_temps(Value::Null), json!({}));
        let t = compact_temps(json!({"sda": "hot", "sdb": [], "sdc": {"x": 1}}));
        assert_eq!(t["sda"], json!({"temp": null, "crit": null}));
        assert_eq!(t["sdb"], json!({"temp": null, "crit": null}));
        assert_eq!(t["sdc"], json!({"temp": null, "crit": null}));
    }
}
