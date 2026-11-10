//! NEAR RPC calls and the NEP-413 signature check.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;

/// The NEP-413 tag: 2^31 + 413.
const NEP413_TAG: u32 = 2_147_484_061;

pub struct Near {
    pub rpc_url: String,
    pub contract: String,
}

/// An RPC failure.
#[derive(Debug, PartialEq)]
pub enum RpcError {
    /// The node answered that the account or the access key does not exist.
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

#[derive(Debug, Clone, Deserialize)]
pub struct LicenseView {
    pub token_id: String,
    pub updates_until_ms: u64,
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
        let unknown = err.contains("does not exist while viewing");
        return Err(if unknown {
            RpcError::Unknown(format!("RPC error: {err}"))
        } else {
            RpcError::Other(format!("RPC error: {err}"))
        });
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

    /// True if `public_key` is a full-access key of `account_id` now. An
    /// unknown key or account is `Ok(false)`; a network failure is an error.
    pub fn is_full_access_key(&self, account_id: &str, public_key: &str) -> Result<bool, RpcError> {
        match self.rpc(json!({
            "request_type": "view_access_key",
            "finality": "final",
            "account_id": account_id,
            "public_key": public_key,
        })) {
            Ok(r) => Ok(r["permission"] == json!("FullAccess")),
            Err(RpcError::Unknown(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub fn is_entitled(&self, account_id: &str, version: &str) -> Result<bool, RpcError> {
        self.view(
            "is_entitled",
            json!({ "account_id": account_id, "version": version }),
        )
    }

    pub fn licenses_for_owner(&self, account_id: &str) -> Result<Vec<LicenseView>, RpcError> {
        self.view("licenses_for_owner", json!({ "account_id": account_id }))
    }

    pub fn find_artifact(&self, sha256: &str) -> Result<Option<ArtifactMatch>, RpcError> {
        self.view("find_artifact", json!({ "sha256": sha256 }))
    }
}

/// The fields of a NEP-413 signed message.
pub struct SignedMessage<'a> {
    pub message: &'a str,
    pub nonce: &'a [u8; 32],
    pub recipient: &'a str,
    pub callback_url: Option<&'a str>,
    /// "ed25519:<base58>"
    pub public_key: &'a str,
    /// Base64 of the 64-byte ed25519 signature, or "ed25519:<base58>".
    pub signature: &'a str,
}

fn borsh_string(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

/// The SHA-256 that a NEP-413 wallet signs.
pub fn nep413_hash(
    message: &str,
    nonce: &[u8; 32],
    recipient: &str,
    callback_url: Option<&str>,
) -> [u8; 32] {
    let mut buf = Vec::new();
    buf.extend_from_slice(&NEP413_TAG.to_le_bytes());
    borsh_string(&mut buf, message);
    buf.extend_from_slice(nonce);
    borsh_string(&mut buf, recipient);
    match callback_url {
        None => buf.push(0),
        Some(url) => {
            buf.push(1);
            borsh_string(&mut buf, url);
        }
    }
    Sha256::digest(&buf).into()
}

/// Checks the ed25519 signature of a NEP-413 message. It does not check
/// that the key belongs to the account: see `Near::is_full_access_key`.
pub fn verify_nep413(m: &SignedMessage) -> Result<(), String> {
    let key_b58 = m
        .public_key
        .strip_prefix("ed25519:")
        .ok_or("Only ed25519 keys are supported")?;
    let key_bytes: [u8; 32] = bs58::decode(key_b58)
        .into_vec()
        .map_err(|_| "The public key is not base58")?
        .try_into()
        .map_err(|_| "The public key must be 32 bytes")?;
    // NEP-413 says base64. Some wallets send "ed25519:<base58>".
    let sig_vec = match m.signature.strip_prefix("ed25519:") {
        Some(b58) => bs58::decode(b58)
            .into_vec()
            .map_err(|_| "The signature is not base58")?,
        None => B64
            .decode(m.signature)
            .map_err(|_| "The signature is not base64")?,
    };
    let sig_bytes: [u8; 64] = sig_vec
        .try_into()
        .map_err(|_| "The signature must be 64 bytes")?;
    let key = VerifyingKey::from_bytes(&key_bytes).map_err(|_| "The public key is not valid")?;
    let sig = Signature::from_bytes(&sig_bytes);
    let hash = nep413_hash(m.message, m.nonce, m.recipient, m.callback_url);
    key.verify_strict(&hash, &sig)
        .map_err(|_| "The signature does not match".to_string())
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
    use ed25519_dalek::{Signer, SigningKey};

    /// Vectors made by near-api-js 7.4.0 `KeyPairSigner.signNep413Message`
    /// (an implementation independent of this one), with the secret seed
    /// [7; 32], message "Register BuildDeed Checksum", recipient "builddeed"
    /// and nonce 0, 1, ..., 31.
    const VECTOR_KEY: &str = "ed25519:GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB";
    const VECTOR_SIG: &str =
        "4xjdp/ig/1tOxhaN1drBtIsRv3IIEecJXJpVrF+OUAE16fgnxjX3VbqDrf9gObwj/7usT/P5PsNKs5bxjiD1Aw==";
    const VECTOR_SIG_CALLBACK: &str =
        "ksUxBHSn/znQJMKOMxBXJG71mBlOBOlE3aYIAAkGzFaBR46j+U4LpC0bQrMlmsKY3OLyjHKS7zYKWSgZZMHUAQ==";
    const VECTOR_CALLBACK: &str = "https://13rac1.github.io/builddeed/sign.html";

    fn vector_nonce() -> [u8; 32] {
        std::array::from_fn(|i| i as u8)
    }

    fn vector<'a>(nonce: &'a [u8; 32]) -> SignedMessage<'a> {
        SignedMessage {
            message: "Register BuildDeed Checksum",
            nonce,
            recipient: "builddeed",
            callback_url: None,
            public_key: VECTOR_KEY,
            signature: VECTOR_SIG,
        }
    }

    #[test]
    fn near_api_js_vectors_verify() {
        let nonce = vector_nonce();
        assert_eq!(verify_nep413(&vector(&nonce)), Ok(()));
        let with_callback = SignedMessage {
            callback_url: Some(VECTOR_CALLBACK),
            signature: VECTOR_SIG_CALLBACK,
            ..vector(&nonce)
        };
        assert_eq!(verify_nep413(&with_callback), Ok(()));
        // Each field is part of the signed hash.
        let fail = |m: SignedMessage| verify_nep413(&m).expect_err("must fail");
        assert_eq!(
            fail(SignedMessage {
                message: "Register X",
                ..vector(&nonce)
            }),
            "The signature does not match"
        );
        assert_eq!(
            fail(SignedMessage {
                recipient: "other",
                ..vector(&nonce)
            }),
            "The signature does not match"
        );
        assert_eq!(
            fail(SignedMessage {
                callback_url: Some(VECTOR_CALLBACK),
                ..vector(&nonce)
            }),
            "The signature does not match"
        );
        let other_nonce = [1u8; 32];
        assert_eq!(fail(vector(&other_nonce)), "The signature does not match");
    }

    #[test]
    fn our_key_matches_the_vector_key() {
        // The seed [7; 32] gives the public key of the vectors.
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let pk = format!(
            "ed25519:{}",
            bs58::encode(sk.verifying_key().to_bytes()).into_string()
        );
        assert_eq!(pk, VECTOR_KEY);
        // A base58 signature is accepted as well as base64.
        let nonce = vector_nonce();
        let hash = nep413_hash("Register BuildDeed Checksum", &nonce, "builddeed", None);
        let sig58 = format!(
            "ed25519:{}",
            bs58::encode(sk.sign(&hash).to_bytes()).into_string()
        );
        assert_eq!(
            verify_nep413(&SignedMessage {
                signature: &sig58,
                ..vector(&nonce)
            }),
            Ok(())
        );
    }

    #[test]
    fn bad_inputs_give_clear_errors() {
        let nonce = vector_nonce();
        let cases: [(SignedMessage, &str); 4] = [
            (
                SignedMessage {
                    public_key: "secp256k1:abc",
                    ..vector(&nonce)
                },
                "Only ed25519",
            ),
            (
                SignedMessage {
                    public_key: "ed25519:0OIl",
                    ..vector(&nonce)
                },
                "not base58",
            ),
            (
                SignedMessage {
                    signature: "!!!",
                    ..vector(&nonce)
                },
                "not base64",
            ),
            (
                SignedMessage {
                    signature: "AAAA",
                    ..vector(&nonce)
                },
                "must be 64 bytes",
            ),
        ];
        for (m, want) in cases {
            let e = verify_nep413(&m).expect_err(want);
            assert!(e.contains(want), "{want}: {e}");
        }
    }

    fn near(url: String) -> Near {
        Near {
            rpc_url: url,
            contract: "demo.builddeed.testnet".into(),
        }
    }

    #[test]
    fn access_key_kinds() {
        use fake_rpc::*;
        let url = serve(vec![
            ok(json!({ "permission": "FullAccess", "nonce": 1 })),
            ok(
                json!({ "permission": { "FunctionCall": { "allowance": null, "receiver_id": "x", "method_names": [] } } }),
            ),
            error("UNKNOWN_ACCESS_KEY"),
            error("UNKNOWN_ACCOUNT"),
            error("INTERNAL_ERROR"),
            (503, "{}".into()),
        ]);
        let n = near(url);
        assert_eq!(n.is_full_access_key("a", "k"), Ok(true), "full access");
        assert_eq!(
            n.is_full_access_key("a", "k"),
            Ok(false),
            "function-call key"
        );
        assert_eq!(n.is_full_access_key("a", "k"), Ok(false), "unknown key");
        assert_eq!(n.is_full_access_key("a", "k"), Ok(false), "unknown account");
        assert!(
            matches!(n.is_full_access_key("a", "k"), Err(RpcError::Other(_))),
            "server error"
        );
        assert!(
            matches!(n.is_full_access_key("a", "k"), Err(RpcError::Other(_))),
            "HTTP 503"
        );
    }

    #[test]
    fn no_network_is_an_error_not_an_unknown_key() {
        // Nothing listens on this port.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let n = near(format!("http://127.0.0.1:{port}"));
        assert!(matches!(
            n.is_full_access_key("a", "k"),
            Err(RpcError::Other(_))
        ));
    }

    #[test]
    fn view_decodes_the_result_bytes() {
        use fake_rpc::*;
        let url = serve(vec![view(json!(true)), view(json!(null))]);
        let n = near(url);
        assert_eq!(n.is_entitled("a", "0.1.0"), Ok(true));
        assert!(n.find_artifact(&"a".repeat(64)).unwrap().is_none());
    }

    #[test]
    fn legacy_text_error_is_classified() {
        let r =
            json!({ "result": { "error": "access key ed25519:x does not exist while viewing" } });
        assert!(matches!(classify(&r), Err(RpcError::Unknown(_))));
        let r = json!({ "result": { "error": "something else" } });
        assert!(matches!(classify(&r), Err(RpcError::Other(_))));
    }
}
