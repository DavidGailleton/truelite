//! `pool.query` events: TrueNAS sends the whole pool on every change, `topology` included (one
//! entry per vdev and disk, with I/O counters), which is most of the payload and unused by the
//! dashboard. The bridge keeps only the fields the web app selects in its own `pool.query` call:
//! [`compact`].
//!
//! The pools page does ask for `topology` in its `pool.query` call; the call result comes back
//! with each pool's topology cut down to names, states and error counts: [`compact_list`]. The
//! API docs give only the six group keys; the node fields come from the middleware's output
//! (`testdata/pool_event.json`), so every one is optional.
//!
//! Event schema: <https://api.truenas.com/v25.10/api_events_pool.query.html> (role `POOL_READ`).

use serde_json::{Map, Value};

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

/// The vdev groups of a topology (`PoolTopology`, all required arrays in 25.10).
pub const GROUPS: &[&str] = &["data", "log", "cache", "spare", "special", "dedup"];

/// The fields kept on each topology node (a vdev or a disk).
const NODE_FIELDS: &[&str] = &["name", "type", "status", "disk"];

/// `stats` error counters, as the web app names them.
const ERRORS: &[(&str, &str)] = &[
    ("read", "read_errors"),
    ("write", "write_errors"),
    ("checksum", "checksum_errors"),
];

/// Deepest node kept: a real pool nests vdev → replacing/spare → disk; anything deeper is junk.
const MAX_DEPTH: usize = 6;

/// One node with [`NODE_FIELDS`], `errors` and its children; `None` when it is not an object.
fn compact_node(node: &Value, depth: usize) -> Option<Value> {
    let entry = node.as_object()?;
    let Value::Object(mut out) = keep_fields(node, NODE_FIELDS)? else {
        return None;
    };
    if let Some(stats) = entry.get("stats").and_then(Value::as_object) {
        let errors: Map<String, Value> = ERRORS
            .iter()
            .filter_map(|&(name, key)| {
                stats
                    .get(key)
                    .filter(|v| v.is_u64())
                    .map(|v| (name.to_owned(), v.clone()))
            })
            .collect();
        out.insert("errors".to_owned(), Value::Object(errors));
    }
    let children = match entry.get("children").and_then(Value::as_array) {
        Some(list) if depth < MAX_DEPTH => list
            .iter()
            .filter_map(|c| compact_node(c, depth + 1))
            .collect(),
        _ => Vec::new(),
    };
    out.insert("children".to_owned(), Value::Array(children));
    Some(Value::Object(out))
}

/// A pool's `topology` with each group's nodes compacted (no `stats`, `path`, `guid`); a missing
/// or unreadable group becomes `[]`. `null` when `topology` is not an object (TrueNAS sends
/// `null` when it is not available).
pub fn compact_topology(topology: &Value) -> Value {
    let Some(groups) = topology.as_object() else {
        return Value::Null;
    };
    let out: Map<String, Value> = GROUPS
        .iter()
        .map(|&g| {
            let nodes = groups
                .get(g)
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(|n| compact_node(n, 0)).collect())
                .unwrap_or_default();
            (g.to_owned(), Value::Array(nodes))
        })
        .collect();
    Value::Object(out)
}

/// A `pool.query` result with each pool's `topology` compacted. Pools without one (the
/// dashboard's call selects none) and a result that is not a list are returned unchanged.
pub fn compact_list(mut result: Value) -> Value {
    if let Some(list) = result.as_array_mut() {
        for pool in list.iter_mut().filter_map(Value::as_object_mut) {
            if let Some(t) = pool.get_mut("topology") {
                *t = compact_topology(t);
            }
        }
    }
    result
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

    fn topology_fixture() -> Value {
        serde_json::from_str(include_str!("testdata/pool_topology.json")).unwrap()
    }

    #[test]
    fn a_topology_keeps_names_states_and_errors() {
        let full = fixture();
        let out = compact_topology(&full["topology"]);
        let vdev = &out["data"][0];
        assert_eq!(vdev["name"], "raidz1-0");
        assert_eq!(vdev["type"], "RAIDZ1");
        assert_eq!(vdev["status"], "ONLINE");
        assert_eq!(
            vdev["errors"],
            json!({"read": 0, "write": 0, "checksum": 0})
        );
        let disk = &vdev["children"][0];
        assert_eq!(disk["disk"], "sda");
        assert_eq!(disk["children"], json!([]));
        for gone in ["stats", "path", "guid", "unavail_disk", "device"] {
            assert!(disk.get(gone).is_none(), "{gone}");
        }
        for &g in GROUPS {
            assert!(out[g].is_array(), "{g}");
        }
        let wire = out.to_string().len();
        let all = full["topology"].to_string().len();
        assert!(wire * 3 < all, "{wire} of {all} bytes");
    }

    #[test]
    fn every_group_and_nested_disks_are_kept() {
        let out = compact_topology(&topology_fixture());
        assert_eq!(out["data"][0]["type"], "MIRROR");
        let sdb = &out["data"][0]["children"][1];
        assert_eq!(sdb["status"], "DEGRADED");
        assert_eq!(sdb["errors"]["read"], 12);
        // A replacement in progress: the old and the new disk under a REPLACING node.
        let replacing = &out["data"][1]["children"][0];
        assert_eq!(replacing["type"], "REPLACING");
        assert_eq!(replacing["children"][1]["disk"], "sdf");
        assert_eq!(out["cache"][0]["disk"], "nvme0n1");
        assert_eq!(out["spare"][0]["status"], "AVAIL");
        assert_eq!(out["log"], json!([]));
    }

    #[test]
    fn a_bad_topology_never_panics() {
        assert_eq!(compact_topology(&Value::Null), Value::Null);
        assert_eq!(compact_topology(&json!([1])), Value::Null);
        let out = compact_topology(&json!({"data": "x", "log": [1, {"name": "a", "stats": 3}]}));
        assert_eq!(out["data"], json!([]));
        assert_eq!(out["log"], json!([{"name": "a", "children": []}]));
        let mut deep = json!({"name": "leaf"});
        for _ in 0..20 {
            deep = json!({"name": "n", "children": [deep]});
        }
        let out = compact_topology(&json!({"data": [deep]}));
        let mut node = &out["data"][0];
        let mut depth = 0;
        while let Some(c) = node["children"].get(0) {
            node = c;
            depth += 1;
        }
        assert_eq!(depth, MAX_DEPTH);
    }

    #[test]
    fn a_result_is_compacted_only_where_it_has_a_topology() {
        let dash = json!([{"id": 1, "name": "tank", "status": "ONLINE"}]);
        assert_eq!(compact_list(dash.clone()), dash);
        assert_eq!(compact_list(json!({"x": 1})), json!({"x": 1}));
        let page = compact_list(json!([{"id": 1, "topology": topology_fixture()}, 5]));
        assert_eq!(page[0]["id"], 1);
        assert!(page[0]["topology"]["data"][0].get("stats").is_none());
        assert_eq!(page[1], 5);
    }
}
