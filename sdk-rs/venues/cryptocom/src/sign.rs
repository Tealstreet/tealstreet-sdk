//! The venue signs sorted parameter names and their unseparated values.
use safe_cex_crypto::{Encoding, hmac_sha256};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Serialize)]
#[serde(untagged)]
pub enum Param {
    Text(String),
    Number(u64),
    Strings(Vec<String>),
}

#[derive(Clone, Default, Serialize)]
#[serde(transparent)]
pub struct Params(BTreeMap<&'static str, Param>);
impl Params {
    pub fn text(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.0.insert(key, Param::Text(value.into()));
        self
    }
    pub fn number(mut self, key: &'static str, value: u64) -> Self {
        self.0.insert(key, Param::Number(value));
        self
    }
    pub fn strings(mut self, key: &'static str, values: Vec<String>) -> Self {
        self.0.insert(key, Param::Strings(values));
        self
    }
    fn signing_text(&self) -> String {
        let mut text = String::new();
        for (key, value) in &self.0 {
            text.push_str(key);
            match value {
                Param::Text(value) => text.push_str(value),
                Param::Number(value) => text.push_str(&value.to_string()),
                Param::Strings(values) => {
                    for value in values {
                        text.push_str(value);
                    }
                }
            }
        }
        text
    }
}

pub struct Credentials {
    pub key: String,
    pub secret: String,
}
impl Credentials {
    pub fn envelope(&self, method: &str, params: &Params, now: f64, auth: bool) -> String {
        let nonce = now as u64;
        let payload = format!(
            "{method}{nonce}{}{}{nonce}",
            self.key,
            params.signing_text()
        );
        let sig = hmac_sha256(self.secret.as_bytes(), payload.as_bytes(), Encoding::Hex);
        #[derive(Serialize)]
        struct Request<'a> {
            id: u64,
            method: &'a str,
            api_key: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            params: Option<&'a Params>,
            sig: String,
            nonce: u64,
        }
        serde_json::to_string(&Request {
            id: nonce,
            method,
            api_key: &self.key,
            params: (!auth).then_some(params),
            sig,
            nonce,
        })
        .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sorted_fields_and_array_values_match_hmac_oracle() {
        let params = Params::default()
            .text("type", "MARKET")
            .text("side", "SELL")
            .text("quantity", "0.01")
            .text("instrument_name", "BTCUSD-PERP")
            .strings("exec_inst", vec!["REDUCE_ONLY".into()]);
        let credentials = Credentials {
            key: "replay-key".into(),
            secret: "replay-secret".into(),
        };
        let body = credentials.envelope("private/create-order", &params, 1610000000000.0, false);
        // Independent Python hmac.sha256 oracle, using the documented signing formula.
        assert!(body.contains("e9f8f06c03ce0214c03b5157d0a71fe3407d018fd0d96966a17c250bf93bd8bf"));
        assert!(body.contains(r#""nonce":1610000000000"#));
        assert!(
            !credentials
                .envelope("public/auth", &Params::default(), 1.0, true)
                .contains("params")
        );
    }
}
