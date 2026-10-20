use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::num;

pub fn signature(secret: &str, timestamp: &str, method: &str, path: &str, body: &str) -> String {
    hmac_sha256(
        secret.as_bytes(),
        format!("{timestamp}{method}{path}{body}").as_bytes(),
        Encoding::Base64,
    )
}

pub fn headers(
    key: &str,
    secret: &str,
    passphrase: &str,
    now: f64,
    method: &str,
    path: &str,
    body: &str,
) -> Vec<(&'static str, String)> {
    let timestamp = num::format(now.floor());
    vec![
        ("ACCESS-KEY", key.to_owned()),
        ("ACCESS-PASSPHRASE", passphrase.to_owned()),
        (
            "ACCESS-SIGN",
            signature(secret, &timestamp, method, path, body),
        ),
        ("ACCESS-TIMESTAMP", timestamp),
        ("X-CHANNEL-API-CODE", "Tealstreet".to_owned()),
        ("Content-Type", "application/json".to_owned()),
    ]
}

pub fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte))
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
