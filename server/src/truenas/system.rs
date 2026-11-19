//! Restarting and shutting down the NAS (#42): `system.reboot` and `system.shutdown`, both jobs
//! with role `FULL_ADMIN` and params `[reason, {delay}]`.
//!
//! Docs: <https://api.truenas.com/v25.10/api_methods_system.shutdown.html>. The 25.10 page of
//! `system.reboot` only lists `system.reboot.info`; its arguments come from middlewared
//! TS-25.10.8 `api/v25_10_0/system_lifecycle.py` (same as `system.shutdown`) and
//! `plugins/system/lifecycle.py`. The job writes `reason` to the audit log, then runs
//! `shutdown -r now` (or `poweroff`) itself: its reply may come, or the link may drop first.

use serde_json::{Value, json};

/// The reason written to the NAS audit log. Fixed here: the browser never writes to the log.
const REASON: &str = "Requested from truelite";

/// The params of `system.reboot` and `system.shutdown`: the browser sends `[]`; rebuilt as
/// `[REASON, {"delay": null}]` (now).
pub fn power_param(params: &Value) -> Option<Value> {
    if !params.as_array()?.is_empty() {
        return None;
    }
    Some(json!([REASON, {"delay": null}]))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn empty_params_are_rebuilt() {
        assert_eq!(
            power_param(&json!([])),
            Some(json!(["Requested from truelite", {"delay": null}]))
        );
    }

    #[test]
    fn anything_else_is_refused() {
        for bad in [
            json!([{}]),
            json!(["x"]),
            json!(["x", {"delay": 0}]),
            json!([[]]),
            json!({}),
            Value::Null,
            json!("x"),
        ] {
            assert_eq!(power_param(&bad), None, "{bad}");
        }
    }
}
