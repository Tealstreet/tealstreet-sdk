//! Bitunix signs compact body bytes and sorted, delimiter-free query parameters.
pub mod dispatch;
use safe_cex_engine::num;
#[derive(Default, Clone)]
pub struct Params(Vec<(String, String)>);
impl Params {
    pub fn str(mut self, k: &str, v: &str) -> Self {
        self.0.push((k.into(), v.into()));
        self
    }
    pub fn num(self, k: &str, v: f64) -> Self {
        self.str(k, &num::format(v))
    }
    pub fn opt_str(self, k: &str, v: Option<&str>) -> Self {
        if let Some(v) = v {
            self.str(k, v)
        } else {
            self
        }
    }
    pub fn opt_num(self, k: &str, v: Option<f64>) -> Self {
        if let Some(v) = v {
            self.num(k, v)
        } else {
            self
        }
    }
    pub fn query(&self) -> String {
        let mut rows = self.0.clone();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        rows.iter()
            .map(|(k, v)| format!("{k}={}", encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    }
    pub fn signing(&self) -> String {
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
#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_crypto::{hex, sha256};
    #[test]
    fn official_example_and_sorted_query() {
        // bitunix.com/api-docs/futures/common/sign.html
        let body = r#"{"uid":"2899","arr":[{"id":1,"name":"maple"},{"id":2,"name":"lily"}]}"#;
        assert_eq!(
            {
                let inner = hex(&sha256(
                    format!("12345620241120123045yourApiKeyid1uid200{body}").as_bytes(),
                ));
                hex(&sha256(format!("{inner}yourSecretKey").as_bytes()))
            },
            "00397cd1e52c7dce3258067324363b6361fabc9178a0912b330c138db8745655"
        );
        assert_eq!(
            Params::default().str("uid", "200").str("id", "1").signing(),
            "id1uid200"
        );
    }
}
