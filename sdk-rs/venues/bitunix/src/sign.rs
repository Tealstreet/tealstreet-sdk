//! Bitunix signs compact body bytes and sorted, delimiter-free query parameters.
use safe_cex_crypto::{hex, sha256, uuid::uuid};
use safe_cex_engine::num;
#[derive(Clone)]
pub struct Credentials {
    pub key: String,
    pub secret: String,
}
pub fn signature(c: &Credentials, nonce: &str, timestamp: &str, query: &str, body: &str) -> String {
    let digest = hex(&sha256(
        format!("{nonce}{timestamp}{}{query}{body}", c.key).as_bytes(),
    ));
    hex(&sha256(format!("{digest}{}", c.secret).as_bytes()))
}
#[derive(Default, Clone)]
pub struct Params(Vec<(String, String)>);
impl Params {
    pub fn str(mut self, k: &str, v: &str) -> Self {
        self.0.push((k.into(), v.into()));
        self
    }
    pub fn query(&self) -> String {
        let mut rows = self.0.clone();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        rows.iter()
            .map(|(k, v)| format!("{k}={}", encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    }
    fn signing(&self) -> String {
        let mut rows = self.0.clone();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        rows.iter()
            .map(|(k, v)| format!("{k}{v}").replace(['=', '&'], ""))
            .collect()
    }
}
pub fn encode(v: &str) -> String {
    let mut out = String::new();
    for b in v.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
pub fn headers(
    c: &Credentials,
    params: &Params,
    body: &str,
    now: f64,
) -> Result<Vec<(&'static str, String)>, String> {
    let nonce = uuid()?;
    let timestamp = num::format(now.floor());
    let sign = signature(c, &nonce, &timestamp, &params.signing(), body);
    Ok(vec![
        ("api-key", c.key.clone()),
        ("nonce", nonce),
        ("timestamp", timestamp),
        ("sign", sign),
        ("Content-Type", "application/json".into()),
    ])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn official_example_and_sorted_query() {
        // bitunix.com/api-docs/futures/common/sign.html
        let c = Credentials {
            key: "yourApiKey".into(),
            secret: "yourSecretKey".into(),
        };
        let body = r#"{"uid":"2899","arr":[{"id":1,"name":"maple"},{"id":2,"name":"lily"}]}"#;
        assert_eq!(
            signature(&c, "123456", "20241120123045", "id1uid200", body),
            "00397cd1e52c7dce3258067324363b6361fabc9178a0912b330c138db8745655"
        );
        assert_eq!(
            Params::default().str("uid", "200").str("id", "1").signing(),
            "id1uid200"
        );
    }
}
