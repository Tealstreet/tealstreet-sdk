//! Synthetic Blofin protocol: base64 of lowercase-hex HMAC, never raw-MAC base64.
use super::join_negative::{ACCOUNT, Observed, Venue};
use axum::http::{HeaderMap, Method, Uri};
use safe_cex_crypto::{Encoding, encode, hmac_sha256};
use safe_cex_engine::signing::Algorithm;
use safe_cex_exchange_server::{accounts::now_ms, envelope::InitPayload};
use serde_json::{Value, json};
pub struct Blofin<const TESTNET: bool>;
const KEY: &str = "synthetic-primary-api-key";
const OTHER_KEY: &str = "synthetic-secondary-api-key";
const SECRET: &str = "synthetic-primary-material";
const OTHER_SECRET: &str = "synthetic-secondary-material";
const PASS: &str = "synthetic-passphrase";
fn secret(k: &str) -> Option<&'static str> {
    match k {
        KEY => Some(SECRET),
        OTHER_KEY => Some(OTHER_SECRET),
        _ => None,
    }
}
fn mac(material: &str, text: &str) -> String {
    let hex = hmac_sha256(material.as_bytes(), text.as_bytes(), Encoding::Hex);
    encode(hex.as_bytes(), Encoding::Base64)
}
fn single<'a>(h: &'a HeaderMap, n: &str) -> Option<&'a str> {
    let mut i = h.get_all(n).iter();
    let v = i.next()?.to_str().ok()?;
    i.next().is_none().then_some(v)
}
fn valid_rest(path: &str, query: Option<&str>, h: &HeaderMap) -> bool {
    (|| {
        let material = secret(single(h, "access-key")?)?;
        if single(h, "access-passphrase")? != PASS {
            return None;
        }
        let stamp = single(h, "access-timestamp")?;
        let time = stamp.parse::<u64>().ok()?;
        if time.abs_diff(now_ms()) > 30_000 {
            return None;
        }
        let nonce = single(h, "access-nonce")?;
        if nonce.is_empty() || !nonce.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        let target = match query {
            Some(q) => format!("{path}?{q}"),
            None => path.to_owned(),
        };
        Some(single(h, "access-sign")? == mac(material, &format!("{target}GET{stamp}{nonce}")))
    })() == Some(true)
}
fn valid_login(v: &Value) -> bool {
    (|| {
        if v["op"] != "login" || v["args"].as_array()?.len() != 1 {
            return None;
        }
        let a = &v["args"][0];
        let material = secret(a["apiKey"].as_str()?)?;
        if a["passphrase"] != PASS {
            return None;
        }
        let stamp = a["timestamp"].as_u64()?;
        let nonce = a["nonce"].as_str()?;
        if nonce.is_empty() || !nonce.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        Some(a["sign"] == mac(material, &format!("/users/self/verifyGET{stamp}{nonce}")))
    })() == Some(true)
}
impl<const TESTNET: bool> Venue for Blofin<TESTNET> {
    const NAME: &'static str = "blofin";
    fn init() -> InitPayload {
        InitPayload {
            account_name: ACCOUNT.into(),
            exchange_name: Self::NAME.into(),
            key: KEY.into(),
            passphrase: Some(PASS.into()),
            testnet: Some(TESTNET),
            signing_mode: Some("cosigned".into()),
            instance_id: Some("negative-join-registration".into()),
            ..Default::default()
        }
    }
    fn algorithm() -> Algorithm {
        Algorithm::HmacSha256
    }
    fn credentials(other: bool) -> Value {
        if other {
            json!({"key":OTHER_KEY,"secret":OTHER_SECRET,"passphrase":PASS})
        } else {
            json!({"key":KEY,"secret":SECRET,"passphrase":PASS})
        }
    }
    fn replayable(f: &Value) -> bool {
        !f["d"]["wsAuth"]["toSign"]
            .as_str()
            .unwrap()
            .starts_with("/users/self/verifyGET")
    }
    fn signature(f: &Value, other: bool) -> (String, Vec<u8>, Value) {
        let text = f["d"]["wsAuth"]["toSign"]
            .as_str()
            .expect("native Blofin signing bytes");
        let at = text.find("GET").unwrap() + 3;
        let sig = mac(if other { OTHER_SECRET } else { SECRET }, text);
        (
            text[..at].into(),
            text.as_bytes().to_vec(),
            json!({"id":f["d"]["id"],"wsAuthSignature":sig}),
        )
    }
    fn http(
        method: &Method,
        uri: &Uri,
        headers: &HeaderMap,
        body: &Value,
        seen: &mut Observed,
    ) -> Value {
        if *method != Method::GET {
            seen.writes += 1;
            panic!("Blofin join attempted a write");
        }
        assert!(body.is_null());
        let path = if let Some(t) = uri.path().strip_prefix("/https://") {
            let at = t.find('/').unwrap();
            assert_eq!(
                &t[..at],
                if TESTNET {
                    "demo-trading-openapi.blofin.com"
                } else {
                    "openapi.blofin.com"
                }
            );
            &t[at..]
        } else {
            uri.path()
        };
        let public = path.starts_with("/api/v1/market/");
        if !public {
            assert!(
                [
                    "/account/margin-mode",
                    "/account/position-mode",
                    "/account/positions",
                    "/account/batch-leverage-info",
                    "/asset/balances",
                    "/trade/orders-pending",
                    "/trade/orders-tpsl-pending",
                    "/trade/orders-algo-pending"
                ]
                .iter()
                .any(|p| path.ends_with(p)),
                "unexpected private Blofin read: {path}"
            );
            seen.signed_reads += 1;
            if !valid_rest(path, uri.query(), headers) {
                seen.rejections += 1;
                return json!({"code":"152401","msg":"synthetic proof refusal","data":[]});
            }
        }
        let fixtures: Value =
            serde_json::from_str(include_str!("../fixtures/admission-reads.json")).unwrap();
        let rows = fixtures["blofin"].as_object().unwrap();
        rows.iter()
            .find(|(k, _)| k.as_str() != "*" && path.ends_with(k.as_str()))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| {
                assert!(!public, "missing public Blofin fixture");
                json!({"code":"0","data":[]})
            })
    }
    fn websocket(v: &Value, seen: &mut Observed) -> Value {
        let op = v["op"].as_str().unwrap();
        match op {
            "login" => {
                seen.signed_reads += 1;
                let valid = valid_login(v);
                if !valid {
                    seen.rejections += 1;
                }
                json!({"event":"login","code":if valid{"0"}else{"152401"},"msg":""})
            }
            "subscribe" | "unsubscribe" => json!({"event":op,"arg":v["args"][0],"code":"0"}),
            _ => {
                seen.writes += 1;
                panic!("unexpected Blofin socket operation: {op}");
            }
        }
    }
    fn websocket_text(text: &str, seen: &mut Observed) -> Option<String> {
        match text {
            "ping" => Some("pong".into()),
            "pong" => None,
            _ => Some(Self::websocket(&serde_json::from_str(text).unwrap(), seen).to_string()),
        }
    }
}
#[cfg(test)]
mod controls {
    use super::*;
    fn headers(k: &str, s: &str) -> HeaderMap {
        let stamp = now_ms().to_string();
        let nonce = "syntheticnonce";
        let sig = mac(
            s,
            &format!("/api/v1/account/position-modeGET{stamp}{nonce}"),
        );
        let mut h = HeaderMap::new();
        for (n, v) in [
            ("access-key", k),
            ("access-passphrase", PASS),
            ("access-timestamp", stamp.as_str()),
            ("access-nonce", nonce),
            ("access-sign", sig.as_str()),
        ] {
            h.insert(n, v.parse().unwrap());
        }
        h
    }
    #[test]
    fn rest_accepts_both_valid_pairs() {
        for (k, s) in [(KEY, SECRET), (OTHER_KEY, OTHER_SECRET)] {
            assert!(valid_rest(
                "/api/v1/account/position-mode",
                None,
                &headers(k, s)
            ));
        }
    }
    #[test]
    fn rest_rejects_unsigned_wrong_key_and_wrong_material() {
        assert!(!valid_rest(
            "/api/v1/account/position-mode",
            None,
            &HeaderMap::new()
        ));
        for (k, s) in [
            (KEY, OTHER_SECRET),
            (OTHER_KEY, SECRET),
            ("unknown", SECRET),
        ] {
            assert!(!valid_rest(
                "/api/v1/account/position-mode",
                None,
                &headers(k, s)
            ));
        }
    }
    #[test]
    fn rest_rejects_passphrase_nonce_and_payload_tampering() {
        let base = headers(KEY, SECRET);
        for field in [
            "access-passphrase",
            "access-timestamp",
            "access-nonce",
            "access-sign",
        ] {
            let mut bad = base.clone();
            bad.insert(field, "invalid".parse().unwrap());
            assert!(!valid_rest("/api/v1/account/position-mode", None, &bad));
        }
        assert!(!valid_rest(
            "/api/v1/account/position-mode",
            Some("tampered=1"),
            &base
        ));
    }
    #[test]
    fn login_accepts_both_pairs_and_rejects_tampering() {
        for (k, s) in [(KEY, SECRET), (OTHER_KEY, OTHER_SECRET)] {
            let stamp = now_ms();
            let nonce = "syntheticnonce";
            let sig = mac(s, &format!("/users/self/verifyGET{stamp}{nonce}"));
            let good = json!({"op":"login","args":[{"apiKey":k,"passphrase":PASS,"timestamp":stamp,"nonce":nonce,"sign":sig}]});
            assert!(valid_login(&good));
            for field in ["apiKey", "passphrase", "timestamp", "nonce", "sign"] {
                let mut bad = good.clone();
                bad["args"][0][field] = "invalid".into();
                assert!(!valid_login(&bad));
            }
        }
    }
}
