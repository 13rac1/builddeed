//! Sign-in: the app proves that the user controls a NEAR account.
//!
//! 1. The app makes a random nonce and listens on 127.0.0.1 on a random port.
//! 2. The app opens the browser at the website's sign page.
//! 3. The wallet signs a NEP-413 message. The page sends the result to
//!    http://127.0.0.1:<port>/callback.
//! 4. The app checks the signature, then checks on chain that the key is a
//!    full-access key of the account and that the account owns a license.

use crate::near::{verify_nep413, Near, SignedMessage};
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

pub const RECIPIENT: &str = "builddeed";
const WAIT: Duration = Duration::from_secs(300);

pub struct Pending {
    listener: TcpListener,
    nonce: [u8; 32],
    message: String,
    pub sign_url: String,
}

pub struct SignedIn {
    pub account_id: String,
    pub public_key: String,
}

/// Starts a sign-in and returns the URL to open in the browser.
pub fn start(site_url: &str, product: &str) -> Result<Pending, String> {
    let mut nonce = [0u8; 32];
    getrandom::getrandom(&mut nonce).map_err(|e| format!("No random source: {e}"))?;
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("Cannot listen: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let message = format!("Register {product} on this computer");
    let sign_url = format!(
        "{site_url}/sign.html#nonce={}&port={port}&message={}&recipient={RECIPIENT}",
        B64URL.encode(nonce),
        url_encode(&message)
    );
    Ok(Pending { listener, nonce, message, sign_url })
}

impl Pending {
    /// Waits for the browser callback, then checks the signature and the key.
    /// Blocks: call it from a worker thread.
    pub fn finish(self, near: &Near) -> Result<SignedIn, String> {
        self.listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let deadline = Instant::now() + WAIT;
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).map_err(|e| e.to_string())?;
                    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
                    let mut reader = BufReader::new(&stream);
                    let mut first = String::new();
                    reader.read_line(&mut first).map_err(|e| e.to_string())?;
                    // GET /callback?a=b&c=d HTTP/1.1
                    let target = first.split_whitespace().nth(1).unwrap_or("");
                    let Some(query) = target.strip_prefix("/callback?") else {
                        // A browser can ask for /favicon.ico first. Wait for the callback.
                        respond(&stream, 404, "Not found");
                        continue;
                    };
                    let params = parse_query(query);
                    let result = self.check(&params, near);
                    match &result {
                        Ok(s) => respond(&stream, 200, &format!(
                            "Registered to {}. You can close this tab and go back to the app.",
                            html_escape(&s.account_id)
                        )),
                        Err(e) => respond(&stream, 400, &format!("Sign-in failed: {}", html_escape(e))),
                    }
                    return result;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() > deadline {
                        return Err("Sign-in timed out after 5 minutes".into());
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(e) => return Err(format!("Callback failed: {e}")),
            }
        }
    }

    fn check(&self, p: &HashMap<String, String>, near: &Near) -> Result<SignedIn, String> {
        let get = |k: &str| p.get(k).map(String::as_str).ok_or(format!("The callback has no {k}"));
        let nonce_back = B64URL
            .decode(get("nonce")?)
            .map_err(|_| "The nonce is not valid")?;
        if nonce_back != self.nonce {
            return Err("The nonce does not match this sign-in".into());
        }
        let account_id = get("accountId")?.to_string();
        let public_key = get("publicKey")?.to_string();
        verify_nep413(&SignedMessage {
            message: &self.message,
            nonce: &self.nonce,
            recipient: RECIPIENT,
            callback_url: p.get("callbackUrl").map(String::as_str).filter(|s| !s.is_empty()),
            public_key: &public_key,
            signature_b64: get("signature")?,
        })?;
        if !near.is_full_access_key(&account_id, &public_key)? {
            return Err(format!("The key is not a full-access key of {account_id}"));
        }
        Ok(SignedIn { account_id, public_key })
    }
}

fn respond(mut stream: &std::net::TcpStream, code: u16, text: &str) {
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
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_round_trip() {
        let s = "Register BuildDeed Checksum on this computer/+=&?";
        assert_eq!(url_decode(&url_encode(s)), s);
        assert_eq!(url_decode("a+b%2Fc%"), "a b/c%");
    }

    #[test]
    fn query_parse() {
        let p = parse_query("accountId=alice.testnet&publicKey=ed25519%3AAbc");
        assert_eq!(p["accountId"], "alice.testnet");
        assert_eq!(p["publicKey"], "ed25519:Abc");
    }
}
