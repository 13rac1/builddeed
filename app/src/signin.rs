//! Sign-in: the app proves that the user controls a NEAR account.
//!
//! 1. The app makes a random nonce and listens on 127.0.0.1 on a random port.
//! 2. The app opens the browser at the website's sign page.
//! 3. The wallet signs a NEP-413 message. The page sends the result to
//!    http://127.0.0.1:<port>/callback.
//! 4. The app checks the signature, then checks on chain that the key is a
//!    full-access key of the account and that the account owns a license.
//!
//! A license is for one person. The signed callback works on any computer of
//! that person, so one license can register more than one computer.

use crate::near::{verify_nep413, Near, SignedMessage};
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

pub const RECIPIENT: &str = "builddeed";
const WAIT: Duration = Duration::from_secs(300);
/// The longest request line that the callback reads.
const MAX_LINE: u64 = 8 * 1024;

pub struct Pending {
    listener: TcpListener,
    nonce: [u8; 32],
    message: String,
    pub sign_url: String,
}

#[derive(Debug)]
pub struct SignedIn {
    pub account_id: String,
    pub public_key: String,
}

/// The sign-in message for a product.
pub fn message_for(product: &str) -> String {
    format!("Register {product}")
}

/// Starts a sign-in and returns the URL to open in the browser.
pub fn start(site_url: &str, product: &str) -> Result<Pending, String> {
    let mut nonce = [0u8; 32];
    getrandom::getrandom(&mut nonce).map_err(|e| format!("No random source: {e}"))?;
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("Cannot listen: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let message = message_for(product);
    let sign_url = format!(
        "{site_url}/sign.html#nonce={}&port={port}&message={}&recipient={RECIPIENT}",
        B64URL.encode(nonce),
        url_encode(&message)
    );
    Ok(Pending {
        listener,
        nonce,
        message,
        sign_url,
    })
}

/// What one callback request gave.
enum Step {
    /// Not the callback, or not for this sign-in: wait for the next request.
    Ignore,
    Done(Result<SignedIn, String>),
}

impl Pending {
    /// Waits for the browser callback, then checks the signature and the key.
    /// A request with a wrong nonce gets 400 and the sign-in goes on, so that
    /// a stray request cannot cancel it. Blocks: call it from a worker thread.
    pub fn finish(self, near: &Near) -> Result<SignedIn, String> {
        self.finish_within(near, WAIT)
    }

    fn finish_within(self, near: &Near, wait: Duration) -> Result<SignedIn, String> {
        self.listener
            .set_nonblocking(true)
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + wait;
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Step::Done(result) = self.handle(stream, near) {
                        return result;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() > deadline {
                        return Err(format!("Sign-in timed out after {} s", wait.as_secs()));
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(e) => return Err(format!("Callback failed: {e}")),
            }
        }
    }

    fn handle(&self, stream: TcpStream, near: &Near) -> Step {
        if stream.set_nonblocking(false).is_err() {
            return Step::Ignore;
        }
        stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
        let mut first = String::new();
        let read = BufReader::new((&stream).take(MAX_LINE)).read_line(&mut first);
        if read.is_err() || !first.ends_with('\n') {
            respond(&stream, 400, "Bad request");
            return Step::Ignore;
        }
        // GET /callback?a=b&c=d HTTP/1.1
        let target = first.split_whitespace().nth(1).unwrap_or("");
        let Some(query) = target.strip_prefix("/callback?") else {
            // A browser can ask for /favicon.ico first.
            respond(&stream, 404, "Not found");
            return Step::Ignore;
        };
        let params = parse_query(query);
        if !self.nonce_matches(&params) {
            respond(
                &stream,
                400,
                "This request is not for the sign-in in progress.",
            );
            return Step::Ignore;
        }
        let result = self.check(&params, near);
        match &result {
            Ok(s) => respond(
                &stream,
                200,
                &format!(
                    "Registered to {}. You can close this tab and go back to the app.",
                    html_escape(&s.account_id)
                ),
            ),
            Err(e) => respond(&stream, 400, &format!("Sign-in failed: {}", html_escape(e))),
        }
        Step::Done(result)
    }

    fn nonce_matches(&self, p: &HashMap<String, String>) -> bool {
        p.get("nonce")
            .and_then(|n| B64URL.decode(n).ok())
            .is_some_and(|n| n == self.nonce)
    }

    fn check(&self, p: &HashMap<String, String>, near: &Near) -> Result<SignedIn, String> {
        let get = |k: &str| {
            p.get(k)
                .map(String::as_str)
                .ok_or(format!("The callback has no {k}"))
        };
        let account_id = get("accountId")?.to_string();
        let public_key = get("publicKey")?.to_string();
        verify_nep413(&SignedMessage {
            message: &self.message,
            nonce: &self.nonce,
            recipient: RECIPIENT,
            callback_url: p
                .get("callbackUrl")
                .map(String::as_str)
                .filter(|s| !s.is_empty()),
            public_key: &public_key,
            signature: get("signature")?,
        })?;
        if !near.is_full_access_key(&account_id, &public_key)? {
            return Err(format!("The key is not a full-access key of {account_id}"));
        }
        Ok(SignedIn {
            account_id,
            public_key,
        })
    }
}

fn respond(mut stream: &TcpStream, code: u16, text: &str) {
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>BuildDeed</title>\
         <body style=\"font-family:sans-serif;padding:2em\"><p>{text}</p></body>"
    );
    let reason = if code == 200 { "OK" } else { "Error" };
    let _ = write!(
        stream,
        "HTTP/1.1 {code} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn parse_query(q: &str) -> HashMap<String, String> {
    q.split('&')
        .filter_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            Some((url_decode(k), url_decode(v)))
        })
        .collect()
}

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => match (hex_val(b[i + 1]), hex_val(b[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h * 16 + l);
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(c: u8) -> Option<u8> {
    (c as char).to_digit(16).map(|v| v as u8)
}

fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::near::{fake_rpc, nep413_hash};
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::json;

    #[test]
    fn url_round_trip() {
        let s = "Register BuildDeed Checksum/+=&?";
        assert_eq!(url_decode(&url_encode(s)), s);
        assert_eq!(url_decode("a+b%2Fc%"), "a b/c%");
        assert_eq!(url_decode("%e2%82%ac"), "€");
    }

    #[test]
    fn query_parse() {
        let p = parse_query("accountId=alice.testnet&publicKey=ed25519%3AAbc");
        assert_eq!(p["accountId"], "alice.testnet");
        assert_eq!(p["publicKey"], "ed25519:Abc");
    }

    #[test]
    fn html_escape_escapes_markup() {
        assert_eq!(
            html_escape("<a href=\"x\">&</a>"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&lt;/a&gt;"
        );
    }

    /// The parts of a pending sign-in that a test needs.
    struct Parts {
        port: u16,
        nonce_b64: String,
        nonce: [u8; 32],
        message: String,
    }

    fn parts(p: &Pending) -> Parts {
        let q = parse_query(p.sign_url.split_once('#').unwrap().1);
        let nonce_b64 = q["nonce"].clone();
        Parts {
            port: q["port"].parse().unwrap(),
            nonce: B64URL.decode(&nonce_b64).unwrap().try_into().unwrap(),
            nonce_b64,
            message: q["message"].clone(),
        }
    }

    /// Sends one GET request to the app and returns the reply. Write and
    /// read errors are not fatal: the app can close a refused request early.
    fn get(port: u16, path: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let _ = write!(s, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");
        let mut out = String::new();
        let _ = s.read_to_string(&mut out);
        out
    }

    /// Tests wait at most this long for a callback, so that a broken test
    /// fails instead of waiting the 5 minutes of a real sign-in.
    const TEST_WAIT: Duration = Duration::from_secs(10);

    fn signed_query(sk: &SigningKey, account: &str, p: &Parts) -> String {
        let sig = sk.sign(&nep413_hash(&p.message, &p.nonce, RECIPIENT, None));
        let pk = format!(
            "ed25519:{}",
            bs58::encode(sk.verifying_key().to_bytes()).into_string()
        );
        format!(
            "/callback?nonce={}&accountId={}&publicKey={}&signature={}",
            p.nonce_b64,
            url_encode(account),
            url_encode(&pk),
            url_encode(&base64::engine::general_purpose::STANDARD.encode(sig.to_bytes()))
        )
    }

    fn near(url: String) -> Near {
        Near {
            rpc_url: url,
            contract: "demo.builddeed.testnet".into(),
        }
    }

    #[test]
    fn message_names_the_product_only() {
        let p = start("https://example.invalid", "BuildDeed Checksum").unwrap();
        assert_eq!(parts(&p).message, "Register BuildDeed Checksum");
        assert!(p
            .sign_url
            .starts_with("https://example.invalid/sign.html#nonce="));
        assert!(p.sign_url.ends_with("&recipient=builddeed"));
    }

    /// Stray requests (favicon, wrong nonce, junk) do not end the sign-in;
    /// the good callback then succeeds.
    #[test]
    fn stray_requests_do_not_cancel_and_good_callback_signs_in() {
        let rpc = fake_rpc::serve(vec![fake_rpc::ok(
            json!({ "permission": "FullAccess", "nonce": 1 }),
        )]);
        let p = start("https://example.invalid", "BuildDeed Checksum").unwrap();
        let x = parts(&p);
        let sk = SigningKey::from_bytes(&[9u8; 32]);
        let good = signed_query(&sk, "alice.testnet", &x);
        let client = std::thread::spawn(move || {
            let r1 = get(x.port, "/favicon.ico");
            let r2 = get(
                x.port,
                "/callback?nonce=AAAA&accountId=x&publicKey=y&signature=z",
            );
            let r3 = get(x.port, &good);
            (r1, r2, r3)
        });
        let signed = p.finish_within(&near(rpc), TEST_WAIT).expect("sign-in");
        assert_eq!(signed.account_id, "alice.testnet");
        let (r1, r2, r3) = client.join().unwrap();
        assert!(r1.starts_with("HTTP/1.1 404"), "{r1}");
        assert!(
            r2.starts_with("HTTP/1.1 400") && r2.contains("not for the sign-in"),
            "{r2}"
        );
        assert!(
            r3.starts_with("HTTP/1.1 200") && r3.contains("Registered to alice.testnet"),
            "{r3}"
        );
    }

    #[test]
    fn bad_signature_fails_the_sign_in() {
        let p = start("https://example.invalid", "BuildDeed Checksum").unwrap();
        let mut x = parts(&p);
        // Sign a different message with the right nonce.
        x.message = "Register something else".into();
        let q = signed_query(&SigningKey::from_bytes(&[9u8; 32]), "alice.testnet", &x);
        let port = x.port;
        let client = std::thread::spawn(move || get(port, &q));
        let e = p
            .finish_within(&near("http://127.0.0.1:9".into()), TEST_WAIT)
            .expect_err("must fail");
        assert_eq!(e, "The signature does not match");
        assert!(client.join().unwrap().starts_with("HTTP/1.1 400"));
    }

    #[test]
    fn function_call_key_fails_the_sign_in() {
        let rpc = fake_rpc::serve(vec![fake_rpc::ok(
            json!({ "permission": { "FunctionCall": { "allowance": null, "receiver_id": "x", "method_names": [] } } }),
        )]);
        let p = start("https://example.invalid", "BuildDeed Checksum").unwrap();
        let x = parts(&p);
        let q = signed_query(&SigningKey::from_bytes(&[9u8; 32]), "alice.testnet", &x);
        let client = std::thread::spawn(move || get(x.port, &q));
        let e = p
            .finish_within(&near(rpc), TEST_WAIT)
            .expect_err("must fail");
        assert_eq!(e, "The key is not a full-access key of alice.testnet");
        client.join().unwrap();
    }

    #[test]
    fn missing_field_fails_with_its_name() {
        let p = start("https://example.invalid", "BuildDeed Checksum").unwrap();
        let x = parts(&p);
        let q = format!("/callback?nonce={}&accountId=alice.testnet", x.nonce_b64);
        let client = std::thread::spawn(move || get(x.port, &q));
        let e = p
            .finish_within(&near("http://127.0.0.1:9".into()), TEST_WAIT)
            .expect_err("must fail");
        assert_eq!(e, "The callback has no publicKey");
        client.join().unwrap();
    }

    #[test]
    fn long_request_line_is_refused_and_ignored() {
        let rpc = fake_rpc::serve(vec![fake_rpc::ok(
            json!({ "permission": "FullAccess", "nonce": 1 }),
        )]);
        let p = start("https://example.invalid", "BuildDeed Checksum").unwrap();
        let x = parts(&p);
        let good = signed_query(&SigningKey::from_bytes(&[9u8; 32]), "alice.testnet", &x);
        let client = std::thread::spawn(move || {
            let long = format!("/callback?pad={}", "a".repeat(20_000));
            let refused = get(x.port, &long);
            let ok = get(x.port, &good);
            (refused, ok)
        });
        assert!(p.finish_within(&near(rpc), TEST_WAIT).is_ok());
        let (refused, ok) = client.join().unwrap();
        // The app may close the refused request before the client reads it.
        assert!(
            refused.is_empty() || refused.starts_with("HTTP/1.1 400"),
            "{refused}"
        );
        assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
    }

    /// The whole sign-in against NEAR testnet, with a real account key in
    /// place of a wallet. Run it with a key file of a full-access key:
    ///   BD_TEST_KEY=/path/key.json BD_TEST_ACCOUNT=name.testnet cargo test -- --ignored signin_against_testnet
    #[test]
    #[ignore]
    fn signin_against_testnet() {
        let key: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(std::env::var("BD_TEST_KEY").unwrap()).unwrap(),
        )
        .unwrap();
        let account = std::env::var("BD_TEST_ACCOUNT").expect("set BD_TEST_ACCOUNT");
        let secret = bs58::decode(
            key["private_key"]
                .as_str()
                .unwrap()
                .trim_start_matches("ed25519:"),
        )
        .into_vec()
        .unwrap();
        let sk = SigningKey::from_bytes(&secret[..32].try_into().unwrap());
        let n = near("https://rpc.testnet.fastnear.com".into());
        let p = start("https://example.invalid", "BuildDeed Checksum").unwrap();
        let x = parts(&p);
        let q = signed_query(&sk, &account, &x);
        let client = std::thread::spawn(move || get(x.port, &q));
        let signed = p.finish_within(&n, TEST_WAIT).expect("sign-in");
        assert_eq!(signed.account_id, account);
        assert!(client.join().unwrap().contains("Registered to"));
    }
}
