//! NEAR RPC view calls to the license contract.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

pub struct Near {
    pub rpc_url: String,
    pub contract: String,
}

/// An RPC failure.
#[derive(Debug, PartialEq)]
pub enum RpcError {
    /// The node answered that the account does not exist.
    Unknown(String),
    /// Any other failure, including no network.
    Other(String),
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RpcError::Unknown(s) | RpcError::Other(s) => f.write_str(s),
        }
    }
}

impl From<RpcError> for String {
    fn from(e: RpcError) -> String {
        e.to_string()
    }
}

/// The license that a computer is registered on.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DeviceLicense {
    pub token_id: String,
    pub owner_id: String,
    pub updates_until_ms: u64,
    pub covers_version: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    pub platform: String,
    pub attestation_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArtifactMatch {
    pub version: String,
    pub commit: String,
    pub released_at_ms: u64,
    pub artifact: Artifact,
}

/// The names that nearcore gives to "does not exist" errors.
const UNKNOWN_CAUSES: [&str; 2] = ["UNKNOWN_ACCESS_KEY", "UNKNOWN_ACCOUNT"];

/// Classifies an RPC reply: the result, or the error by its cause name.
fn classify(resp: &Value) -> Result<Value, RpcError> {
    if let Some(err) = resp.get("error") {
        let cause = err
            .pointer("/cause/name")
            .and_then(Value::as_str)
            .unwrap_or("");
        return Err(if UNKNOWN_CAUSES.contains(&cause) {
            RpcError::Unknown(format!("RPC error: {cause}"))
        } else {
            RpcError::Other(format!("RPC error: {err}"))
        });
    }
    let result = resp
        .get("result")
        .cloned()
        .ok_or_else(|| RpcError::Other("RPC reply has no result".into()))?;
    // Older nodes put a query error in result.error as text.
    if let Some(err) = result.get("error").and_then(Value::as_str) {
        return Err(RpcError::Other(format!("RPC error: {err}")));
    }
    Ok(result)
}

impl Near {
    fn rpc(&self, params: Value) -> Result<Value, RpcError> {
        let body =
            json!({ "jsonrpc": "2.0", "id": "builddeed", "method": "query", "params": params });
        let resp: Value = match ureq::post(&self.rpc_url)
            .timeout(Duration::from_secs(15))
            .send_json(body)
        {
            Ok(r) => r.into_json(),
            // nearcore answers some query errors with an HTTP error status
            // and a JSON body.
            Err(ureq::Error::Status(_, r)) => r.into_json(),
            Err(e) => return Err(RpcError::Other(format!("RPC request failed: {e}"))),
        }
        .map_err(|e| RpcError::Other(format!("RPC reply is not JSON: {e}")))?;
        classify(&resp)
    }

    /// Calls a view method of the license contract.
    pub fn view<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        args: Value,
    ) -> Result<T, RpcError> {
        let result = self.rpc(json!({
            "request_type": "call_function",
            "finality": "final",
            "account_id": self.contract,
            "method_name": method,
            "args_base64": B64.encode(args.to_string()),
        }))?;
        let bytes: Vec<u8> = serde_json::from_value(result["result"].clone())
            .map_err(|e| RpcError::Other(format!("Unexpected view result: {e}")))?;
        serde_json::from_slice(&bytes)
            .map_err(|e| RpcError::Other(format!("Unexpected view result: {e}")))
    }

    /// The license that this computer is registered on, if any.
    pub fn device_license(
        &self,
        device_id: &str,
        version: &str,
    ) -> Result<Option<DeviceLicense>, RpcError> {
        self.view(
            "device_license",
            json!({ "device_id": device_id, "version": version }),
        )
    }

    pub fn find_artifact(&self, sha256: &str) -> Result<Option<ArtifactMatch>, RpcError> {
        self.view("find_artifact", json!({ "sha256": sha256 }))
    }
}

/// A loopback HTTP server that answers each request with the next reply.
#[cfg(test)]
pub mod fake_rpc {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    /// Starts the server and returns its URL. Each reply is (status, body).
    pub fn serve(replies: Vec<(u16, String)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for (status, body) in replies {
                let (mut s, _) = listener.accept().unwrap();
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut len = 0usize;
                loop {
                    let mut line = String::new();
                    r.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap();
                    }
                }
                let mut body_in = vec![0u8; len];
                r.read_exact(&mut body_in).unwrap();
                let _ = write!(
                    s,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        url
    }

    pub fn ok(result: serde_json::Value) -> (u16, String) {
        (
            200,
            serde_json::json!({ "jsonrpc": "2.0", "id": "x", "result": result }).to_string(),
        )
    }

    pub fn error(cause: &str) -> (u16, String) {
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": "x",
            "error": { "name": "HANDLER_ERROR", "cause": { "name": cause, "info": {} },
                       "code": -32000, "message": "Server error" }
        });
        (200, body.to_string())
    }

    /// The reply of a view call that returns `value`.
    pub fn view(value: serde_json::Value) -> (u16, String) {
        let bytes: Vec<u8> = value.to_string().into_bytes();
        ok(serde_json::json!({ "result": bytes, "logs": [] }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(url: String) -> Near {
        Near {
            rpc_url: url,
            contract: "demo.builddeed.testnet".into(),
        }
    }

    #[test]
    fn device_license_decodes_a_seat_and_no_seat() {
        use fake_rpc::*;
        let seat = json!({ "token_id": "1", "owner_id": "alice.testnet",
                           "updates_until_ms": 1_825_882_492_821u64, "covers_version": true });
        let url = serve(vec![view(seat), view(json!(null))]);
        let n = near(url);
        let d = n.device_license(&"a".repeat(64), "0.1.1").unwrap().unwrap();
        assert_eq!(d.owner_id, "alice.testnet");
        assert!(d.covers_version);
        assert_eq!(n.device_license(&"b".repeat(64), "0.1.1"), Ok(None));
    }

    #[test]
    fn errors_are_classified() {
        use fake_rpc::*;
        let url = serve(vec![
            error("UNKNOWN_ACCOUNT"),
            error("INTERNAL_ERROR"),
            (503, "{}".into()),
        ]);
        let n = near(url);
        assert!(matches!(
            n.device_license("x", "1"),
            Err(RpcError::Unknown(_))
        ));
        assert!(matches!(
            n.device_license("x", "1"),
            Err(RpcError::Other(_))
        ));
        assert!(matches!(
            n.device_license("x", "1"),
            Err(RpcError::Other(_))
        ));
    }

    #[test]
    fn no_network_is_an_error() {
        // Nothing listens on this port.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let n = near(format!("http://127.0.0.1:{port}"));
        assert!(matches!(
            n.device_license("x", "1"),
            Err(RpcError::Other(_))
        ));
    }

    #[test]
    fn legacy_text_error_is_an_error() {
        let r = json!({ "result": { "error": "something" } });
        assert!(matches!(classify(&r), Err(RpcError::Other(_))));
    }
}
