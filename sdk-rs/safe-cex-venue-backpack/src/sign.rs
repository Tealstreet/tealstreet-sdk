#![allow(dead_code)]
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
    seed: Vec<u8>,
    pub window: u32,
}

impl Credentials {
    pub fn new(key: String, secret: &str) -> Result<Self, String> {
        let seed = STANDARD.decode(secret).map_err(|e| e.to_string())?;
        if seed.len() != 32 {
            return Err("Ed25519 seed must be 32 bytes".into());
        }
        Ok(Self {
            key,
            seed,
            window: 5000,
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

    pub fn headers(
        &self,
        instruction: &str,
        rows: &[Params],
        now: f64,
    ) -> Vec<(&'static str, String)> {
        vec![
            ("X-API-Key", self.key.clone()),
            ("X-Signature", self.signature(instruction, rows, now)),
            ("X-Timestamp", format!("{now:.0}")),
            ("X-Window", self.window.to_string()),
            ("Content-Type", "application/json".into()),
        ]
    }
}
