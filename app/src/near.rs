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

impl Near {
    fn rpc(&self, params: Value) -> Result<Value, String> {
        let body = json!({ "jsonrpc": "2.0", "id": "builddeed", "method": "query", "params": params });
        let resp: Value = ureq::post(&self.rpc_url)
            .timeout(Duration::from_secs(15))
            .send_json(body)
            .map_err(|e| format!("RPC request failed: {e}"))?
            .into_json()
            .map_err(|e| format!("RPC reply is not JSON: {e}"))?;
        if let Some(err) = resp.get("error") {
            return Err(format!("RPC error: {err}"));
        }
        let result = resp.get("result").cloned().ok_or("RPC reply has no result")?;
        if let Some(err) = result.get("error") {
            return Err(format!("RPC error: {err}"));
        }
        Ok(result)
    }

    /// Calls a view method of the license contract.
    pub fn view<T: serde::de::DeserializeOwned>(&self, method: &str, args: Value) -> Result<T, String> {
        let result = self.rpc(json!({
            "request_type": "call_function",
            "finality": "final",
            "account_id": self.contract,
            "method_name": method,
            "args_base64": B64.encode(args.to_string()),
        }))?;
        let bytes: Vec<u8> = serde_json::from_value(result["result"].clone())
            .map_err(|e| format!("Unexpected view result: {e}"))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("Unexpected view result: {e}"))
    }

    /// True if `public_key` is a full-access key of `account_id` now.
    pub fn is_full_access_key(&self, account_id: &str, public_key: &str) -> Result<bool, String> {
        match self.rpc(json!({
            "request_type": "view_access_key",
            "finality": "final",
            "account_id": account_id,
            "public_key": public_key,
        })) {
            Ok(r) => Ok(r["permission"] == json!("FullAccess")),
            // An unknown key is an RPC error, not a failure of the network.
            Err(e) if e.contains("UNKNOWN_ACCESS_KEY") || e.contains("does not exist") => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub fn is_entitled(&self, account_id: &str, version: &str) -> Result<bool, String> {
        self.view("is_entitled", json!({ "account_id": account_id, "version": version }))
    }

    pub fn licenses_for_owner(&self, account_id: &str) -> Result<Vec<LicenseView>, String> {
        self.view("licenses_for_owner", json!({ "account_id": account_id }))
    }

    pub fn find_artifact(&self, sha256: &str) -> Result<Option<ArtifactMatch>, String> {
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
    /// Base64 of the 64-byte ed25519 signature.
    pub signature_b64: &'a str,
}

fn borsh_string(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

/// The SHA-256 that a NEP-413 wallet signs.
pub fn nep413_hash(message: &str, nonce: &[u8; 32], recipient: &str, callback_url: Option<&str>) -> [u8; 32] {
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
    let sig_vec = match m.signature_b64.strip_prefix("ed25519:") {
        Some(b58) => bs58::decode(b58).into_vec().map_err(|_| "The signature is not base58")?,
        None => B64.decode(m.signature_b64).map_err(|_| "The signature is not base64")?,
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

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn verify_accepts_a_good_signature_and_refuses_a_changed_message() {
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let pk = format!("ed25519:{}", bs58::encode(sk.verifying_key().to_bytes()).into_string());
        let nonce = [1u8; 32];
        let hash = nep413_hash("hello", &nonce, "builddeed", None);
        let sig = B64.encode(sk.sign(&hash).to_bytes());
        let mut m = SignedMessage {
            message: "hello",
            nonce: &nonce,
            recipient: "builddeed",
            callback_url: None,
            public_key: &pk,
            signature_b64: &sig,
        };
        assert!(verify_nep413(&m).is_ok());
        let sig58 = format!("ed25519:{}", bs58::encode(sk.sign(&hash).to_bytes()).into_string());
        m.signature_b64 = &sig58;
        assert!(verify_nep413(&m).is_ok());
        m.message = "hellO";
        assert!(verify_nep413(&m).is_err());
        m.message = "hello";
        m.recipient = "other";
        assert!(verify_nep413(&m).is_err());
        m.recipient = "builddeed";
        m.callback_url = Some("https://x");
        assert!(verify_nep413(&m).is_err());
    }

    #[test]
    fn hash_layout_matches_borsh() {
        // Tag, then "a" as a u32 length and a byte, 32 nonce bytes, "b", None.
        let mut expected = Vec::new();
        expected.extend_from_slice(&NEP413_TAG.to_le_bytes());
        expected.extend_from_slice(&[1, 0, 0, 0, b'a']);
        expected.extend_from_slice(&[0u8; 32]);
        expected.extend_from_slice(&[1, 0, 0, 0, b'b']);
        expected.push(0);
        let want: [u8; 32] = Sha256::digest(&expected).into();
        assert_eq!(nep413_hash("a", &[0u8; 32], "b", None), want);
    }
}
