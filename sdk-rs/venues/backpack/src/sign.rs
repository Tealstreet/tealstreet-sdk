use base64::{Engine, engine::general_purpose::STANDARD};
use safe_cex_crypto::ed25519;
use std::collections::BTreeMap;

pub type Params = BTreeMap<String, String>;

pub fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(byte as char);
        } else {
            use std::fmt::Write;
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

pub fn query(params: &Params) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{k}={}", encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

pub fn payload(instruction: &str, rows: &[Params], now: f64, window: u32) -> String {
    let mut text = rows
        .iter()
        .map(|row| {
            let q = query(row);
            format!(
                "instruction={instruction}{}",
                if q.is_empty() {
                    String::new()
                } else {
                    format!("&{q}")
                }
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    text.push_str(&format!("&timestamp={now:.0}&window={window}"));
    text
}

pub struct Credentials {
    pub key: String,
    seed: zeroize::Zeroizing<Vec<u8>>,
    pub window: u32,
}

impl Credentials {
    pub fn new(key: String, secret: &str) -> Result<Self, String> {
        let seed = zeroize::Zeroizing::new(
            STANDARD
                .decode(secret)
                .map_err(|_| "Invalid Backpack base64 seed")?,
        );
        if seed.len() != 32 {
            return Err("Ed25519 seed must be 32 bytes".into());
        }
        Ok(Self {
            key,
            seed,
            window: 60000,
        })
    }

    pub fn signature(&self, instruction: &str, rows: &[Params], now: f64) -> String {
        STANDARD.encode(
            ed25519::sign(
                payload(instruction, rows, now, self.window).as_bytes(),
                &self.seed,
            )
            .expect("validated seed"),
        )
    }

    pub fn ws_auth(&self, now: f64) -> [String; 4] {
        let bytes = payload("subscribe", &[Params::new()], now, 5000);
        [
            self.key.clone(),
            STANDARD.encode(ed25519::sign(bytes.as_bytes(), &self.seed).expect("validated seed")),
            format!("{now:.0}"),
            "5000".into(),
        ]
    }

    pub fn headers(
        &self,
        instruction: &str,
        rows: &[Params],
        now: f64,
    ) -> Vec<(&'static str, String)> {
        vec![
            ("X-API-Key", self.key.clone()),
            ("X-Broker-Id", "1200".into()),
            ("X-Signature", self.signature(instruction, rows, now)),
            ("X-Timestamp", format!("{now:.0}")),
            ("X-Window", self.window.to_string()),
            ("Content-Type", "application/json".into()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_batch_matches_documented_signing_example() {
        let row = |price: &str, quantity: &str| {
            [
                ("symbol".into(), "SOL_USDC_PERP".into()),
                ("side".into(), "Bid".into()),
                ("orderType".into(), "Limit".into()),
                ("price".into(), price.into()),
                ("quantity".into(), quantity.into()),
            ]
            .into()
        };
        assert_eq!(
            payload(
                "orderExecute",
                &[row("141", "12"), row("140", "11")],
                1750793021519.0,
                5000
            ),
            "instruction=orderExecute&orderType=Limit&price=141&quantity=12&side=Bid&symbol=SOL_USDC_PERP&instruction=orderExecute&orderType=Limit&price=140&quantity=11&side=Bid&symbol=SOL_USDC_PERP&timestamp=1750793021519&window=5000"
        );
        assert_eq!(
            query(&[("text".into(), "a +/&é".into())].into()),
            "text=a%20%2B%2F%26%C3%A9"
        );
    }
    #[test]
    fn rest_and_ws_use_distinct_auth_windows() {
        let c = Credentials::new("public-test-key".into(), &STANDARD.encode([7u8; 32])).unwrap();
        assert!(
            c.headers("accountQuery", &[Params::new()], 1000.0)
                .iter()
                .any(|(k, v)| *k == "X-Window" && v == "60000")
        );
        assert_eq!(c.ws_auth(1000.0)[3], "5000");
        assert!(Credentials::new("key".into(), "invalid").is_err());
        assert!(Credentials::new("key".into(), &STANDARD.encode([7u8; 64])).is_err());
    }
}
