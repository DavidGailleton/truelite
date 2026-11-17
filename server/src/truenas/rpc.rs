//! JSON-RPC 2.0 wire types as TrueNAS speaks them.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Outgoing call. `params` is always a positional array; TrueNAS has no batch requests.
#[derive(Serialize)]
pub(super) struct Request<'a> {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: &'a str,
    pub params: &'a Value,
}

/// Any message from the NAS: a response (has `id`) or a notification (has `method`, no `id`).
#[derive(Deserialize)]
pub(super) struct Incoming {
    #[serde(default)]
    pub id: Option<Value>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<RpcError>,
}

/// JSON-RPC error object. `message` may be `null`. Codes: `-32000` too many concurrent calls,
/// `-32001` method call error, plus the standard `-326xx`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RpcError {
    pub code: i64,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub data: Option<ErrorData>,
}

/// TrueNAS details in `error.data`. `trace` and `py_exception` are dropped on purpose: they are
/// large and must never reach the browser.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ErrorData {
    /// errno
    #[serde(default)]
    pub error: Option<i64>,
    #[serde(default)]
    pub errname: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub extra: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_shape() {
        let params = json!([]);
        let s = serde_json::to_value(Request {
            jsonrpc: "2.0",
            id: 1,
            method: "system.info",
            params: &params,
        })
        .unwrap();
        assert_eq!(
            s,
            json!({"jsonrpc":"2.0","id":1,"method":"system.info","params":[]})
        );
    }

    #[test]
    fn error_response_from_docs() {
        let m: Incoming = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":7,"error":{"code":-32001,"message":null,"data":{
                "error":2,"errname":"ENOENT","reason":"not found",
                "trace":{"class":"X","frames":[],"formatted":"..","repr":".."},
                "extra":[],"py_exception":"Traceback"}}}"#,
        )
        .unwrap();
        assert_eq!(m.id, Some(json!(7)));
        let e = m.error.unwrap();
        assert_eq!(e.code, -32001);
        assert_eq!(e.message, None);
        let d = e.data.unwrap();
        assert_eq!(d.errname.as_deref(), Some("ENOENT"));
        assert_eq!(d.reason.as_deref(), Some("not found"));
    }

    #[test]
    fn notification_from_docs() {
        let m: Incoming = serde_json::from_str(
            r#"{"jsonrpc":"2.0","method":"collection_update","params":{
                "collection":"disk.query","event":"CHANGED","fields":{"name":"sda"}}}"#,
        )
        .unwrap();
        assert!(m.id.is_none());
        assert_eq!(m.method.as_deref(), Some("collection_update"));
        assert_eq!(m.params["collection"], "disk.query");
    }
}
