//! `reporting.realtime`: TrueNAS pushes CPU, memory, network, disk, ZFS and pool statistics every
//! 2 s, several KB with one entry per core and per interface (the docker `veth*` ones included).
//! The dashboard shows a few numbers of it, so the bridge sends only those: [`compact`].
//!
//! Event schema: <https://api.truenas.com/v25.10/api_events_reporting.realtime.html>
//! (role `REPORTING_READ`, default interval 2 s).

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The event name, as the browser subscribes to it.
pub const EVENT: &str = "reporting.realtime";

/// Interfaces left out by name prefix: loopback, container and VM plumbing (docker `veth*`,
/// `docker0`, compose `br-*`, incus/libvirt `vnet*`/`tap*`/`virbr*`, kubernetes CNIs). Their
/// traffic is already counted on the physical interface or bridge it goes through.
const VIRTUAL_PREFIXES: &[&str] = &[
    "lo", "veth", "vnet", "tap", "docker", "br-", "virbr", "kube", "cni", "flannel",
];

/// Only the fields read; serde skips the rest, so new TrueNAS fields don't break this.
#[derive(Deserialize)]
struct Raw {
    cpu: HashMap<String, RawCpu>,
    memory: RawMemory,
    interfaces: HashMap<String, RawIface>,
}

#[derive(Deserialize)]
struct RawCpu {
    usage: f64,
    #[serde(default)]
    temp: Option<f64>,
}

#[derive(Deserialize)]
struct RawMemory {
    physical_memory_total: u64,
    physical_memory_available: u64,
    arc_size: u64,
}

#[derive(Deserialize)]
struct RawIface {
    link_state: String,
    #[serde(default)]
    speed: Option<f64>,
    #[serde(default)]
    received_bytes_rate: f64,
    #[serde(default)]
    sent_bytes_rate: f64,
}

/// What the browser receives, documented in `docs/ws-protocol.md`.
#[derive(Serialize)]
struct Compact {
    /// Whole-CPU usage, % (one decimal).
    cpu: f64,
    /// CPU temperature, °C, when the NAS reports one.
    temp: Option<f64>,
    mem: Memory,
    /// Physical interfaces and bridges by name, sorted.
    net: BTreeMap<String, Iface>,
}

/// Bytes.
#[derive(Serialize)]
struct Memory {
    total: u64,
    available: u64,
    arc: u64,
}

#[derive(Serialize)]
struct Iface {
    /// Bytes per second.
    rx: u64,
    tx: u64,
    up: bool,
    /// Link speed, Mb/s, when known.
    speed: Option<u64>,
}

/// The few numbers the dashboard shows, from one `reporting.realtime` payload; `None` when the
/// payload does not have the documented shape.
pub fn compact(fields: &Value) -> Option<Value> {
    let raw = Raw::deserialize(fields).ok()?;
    let total = raw.cpu.get("cpu")?;
    // The aggregate often has no temperature; the hottest core is what matters then.
    let temp = total.temp.or_else(|| {
        raw.cpu
            .iter()
            .filter(|(name, _)| *name != "cpu")
            .filter_map(|(_, c)| c.temp)
            .max_by(f64::total_cmp)
    });
    let net = raw
        .interfaces
        .into_iter()
        .filter(|(name, _)| !VIRTUAL_PREFIXES.iter().any(|p| name.starts_with(p)))
        .map(|(name, i)| {
            let iface = Iface {
                rx: rate(i.received_bytes_rate),
                tx: rate(i.sent_bytes_rate),
                up: i.link_state == "LINK_STATE_UP",
                speed: i.speed.filter(|s| *s > 0.0).map(rate),
            };
            (name, iface)
        })
        .collect();
    let out = Compact {
        cpu: (total.usage.clamp(0.0, 100.0) * 10.0).round() / 10.0,
        temp: temp.map(|t| (t * 10.0).round() / 10.0),
        mem: Memory {
            total: raw.memory.physical_memory_total,
            available: raw.memory.physical_memory_available,
            arc: raw.memory.arc_size,
        },
        net,
    };
    serde_json::to_value(out).ok()
}

/// A non-negative rate rounded to a whole number (sub-byte precision is noise on the wire).
fn rate(v: f64) -> u64 {
    // `as` saturates: NaN → 0, huge → u64::MAX, which is what a display wants.
    v.max(0.0).round() as u64
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("testdata/realtime.json")).unwrap()
    }

    #[test]
    fn the_fixture_shrinks_to_what_the_dashboard_shows() {
        let out = compact(&fixture()).unwrap();
        assert_eq!(
            out,
            json!({
                "cpu": 12.3,
                "temp": 47.5,
                "mem": {"total": 68_719_476_736_u64, "available": 30_000_000_000_u64,
                        "arc": 25_000_000_000_u64},
                "net": {
                    "br0": {"rx": 1201, "tx": 340, "up": true, "speed": null},
                    "enp3s0": {"rx": 1250, "tx": 360, "up": true, "speed": 1000},
                    "enp4s0": {"rx": 0, "tx": 0, "up": false, "speed": null}
                }
            })
        );
        let wire = out.to_string().len();
        let full = fixture().to_string().len();
        assert!(wire < 1024, "{wire} bytes");
        assert!(wire * 5 < full, "{wire} of {full} bytes");
    }

    #[test]
    fn virtual_interfaces_are_left_out() {
        let out = compact(&fixture()).unwrap();
        let names: Vec<_> = out["net"].as_object().unwrap().keys().collect();
        assert!(
            names
                .iter()
                .all(|n| !n.starts_with("veth") && !n.starts_with("docker") && *n != "lo"),
            "{names:?}"
        );
    }

    #[test]
    fn temp_falls_back_to_the_hottest_core_then_null() {
        let mut f = fixture();
        f["cpu"]["cpu"]["temp"] = Value::Null;
        assert_eq!(compact(&f).unwrap()["temp"], json!(51.0));
        for (_, c) in f["cpu"].as_object_mut().unwrap() {
            c["temp"] = Value::Null;
        }
        assert_eq!(compact(&f).unwrap()["temp"], Value::Null);
    }

    #[test]
    fn a_payload_of_another_shape_is_refused() {
        assert_eq!(compact(&json!({"n": 1})), None);
        assert_eq!(compact(&Value::Null), None);
        let mut no_total = fixture();
        no_total["cpu"].as_object_mut().unwrap().remove("cpu");
        assert_eq!(compact(&no_total), None);
        let mut bad_memory = fixture();
        bad_memory["memory"]["arc_size"] = json!("lots");
        assert_eq!(compact(&bad_memory), None);
    }

    #[test]
    fn odd_numbers_are_clamped() {
        let mut f = fixture();
        f["cpu"]["cpu"]["usage"] = json!(100.04);
        f["interfaces"]["enp3s0"]["received_bytes_rate"] = json!(-3.0);
        let out = compact(&f).unwrap();
        assert_eq!(out["cpu"], json!(100.0));
        assert_eq!(out["net"]["enp3s0"]["rx"], json!(0));
    }
}
