//! MEXC signs the exact POST body, or the sorted GET query, in REST headers.
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::num;
use std::collections::BTreeMap;

pub struct Credentials {
    pub key: String,
    pub secret: String,
}
impl Credentials {
    pub fn signature(&self, now: f64, payload: &str) -> String {
        hmac_sha256(
            self.secret.as_bytes(),
            format!("{}{}{payload}", self.key, num::format(now)).as_bytes(),
            Encoding::Hex,
        )
    }
    pub fn headers(&self, now: f64, payload: &str) -> Vec<(&'static str, String)> {
        vec![
            ("ApiKey", self.key.clone()),
            ("Request-Time", num::format(now)),
            ("Signature", self.signature(now, payload)),
            ("Recv-Window", "20000".into()),
            ("Content-Type", "application/json".into()),
        ]
    }
}
pub fn query(params: impl IntoIterator<Item = (&'static str, String)>) -> String {
    params
        .into_iter()
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .map(|(k, v)| format!("{k}={}", escape(&v)))
        .collect::<Vec<_>>()
        .join("&")
}
fn escape(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signs_exact_post_bytes_and_sorted_get_parameters() {
        let c = Credentials {
            key: "replay-key".into(),
            secret: "replay-secret".into(),
        };
        assert_eq!(
            query([("symbol", "BTC_USDT".into()), ("page_num", "1".into())]),
            "page_num=1&symbol=BTC_USDT"
        );
        assert_eq!(
            c.signature(1700000000000.0, "[\"739113577038255616\"]"),
            "59e9ebb06b0e27069eec29158b3682a9f279f1e3790b0fa09a403d4965270557"
        );
        assert_ne!(
            c.signature(1.0, r#"{"vol":1}"#),
            c.signature(1.0, r#"{ "vol":1}"#)
        );
    }
}
