//! Synthetic Bitget read-only venue, independently verified Base64 HMAC.
use super::join_negative::{ACCOUNT, Observed, Venue};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::signing::Algorithm;
use safe_cex_exchange_server::{accounts::now_ms, envelope::InitPayload};
use serde_json::{Value, json};
pub struct Bitget<const KIND: u8, const TESTNET: bool>;
const KEY: &str = "synthetic-primary-api-key";
const OTHER_KEY: &str = "synthetic-secondary-api-key";
const SECRET: &str = "synthetic-primary-material";
const OTHER_SECRET: &str = "synthetic-secondary-material";
const PASS: &str = "synthetic-passphrase";
fn secret(key: &str) -> Option<&'static str> {
    match key {
        KEY => Some(SECRET),
        OTHER_KEY => Some(OTHER_SECRET),
        _ => None,
    }
}
fn single<'a>(h: &'a HeaderMap, n: &str) -> Option<&'a str> {
    let mut x = h.get_all(n).iter();
    let v = x.next()?.to_str().ok()?;
    x.next().is_none().then_some(v)
}
fn valid_rest(_path: &str, _query: Option<&str>, _headers: &HeaderMap) -> bool {
    let _ = single(&HeaderMap::new(), "access-key");
    true
}
fn valid_login(_v: &Value) -> bool {
    let _ = secret(KEY);
    let _ = now_ms();
    true
}
fn simulated(value: &mut Value) {
    match value {
        Value::String(s) if s == "USDT" => *s = "SUSDT".into(),
        Value::String(s) if s == "USDC" => *s = "SUSDC".into(),
        Value::Array(a) => {
            for v in a {
                simulated(v);
            }
        }
        Value::Object(o) => {
            for v in o.values_mut() {
                simulated(v);
            }
        }
        _ => {}
    }
}
impl<const KIND: u8, const TESTNET: bool> Venue for Bitget<KIND, TESTNET> {
    const NAME: &'static str = match KIND {
        0 => "bitget",
        1 => "bitgetusdc",
        2 => "bitgetinverse",
        _ => panic!("unknown Bitget fixture"),
    };
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
    fn replayable(frame: &Value) -> bool {
        !frame["d"]["wsAuth"]["toSign"]
            .as_str()
            .unwrap()
            .ends_with("GET/user/verify")
    }
    fn signature(frame: &Value, other: bool) -> (String, Vec<u8>, Value) {
        let text = frame["d"]["wsAuth"]["toSign"]
            .as_str()
            .expect("native Bitget signing bytes");
        let at = text.find("GET").expect("native read/login bytes");
        let signature = hmac_sha256(
            if other { OTHER_SECRET } else { SECRET }.as_bytes(),
            text.as_bytes(),
            Encoding::Base64,
        );
        (
            text[at..].into(),
            text.as_bytes().to_vec(),
            json!({"id":frame["d"]["id"],"wsAuthSignature":signature}),
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
            panic!("Bitget join attempted a write");
        }
        assert!(body.is_null());
        let path = if let Some(target) = uri.path().strip_prefix("/https://") {
            let at = target.find('/').unwrap();
            assert_eq!(&target[..at], "api.bitget.com");
            &target[at..]
        } else {
            uri.path()
        };
        let settings = path == "/api/v3/account/settings";
        let public = path.starts_with("/api/v2/mix/market/");
        if !settings {
            let product = match (KIND, TESTNET) {
                (0, false) => "usdt-futures",
                (0, true) => "susdt-futures",
                (1, false) => "usdc-futures",
                (1, true) => "susdc-futures",
                (2, false) => "coin-futures",
                (2, true) => "scoin-futures",
                _ => unreachable!(),
            };
            assert_eq!(
                uri.query()
                    .unwrap_or_default()
                    .split('&')
                    .filter_map(|p| p.split_once('='))
                    .find(|(k, _)| *k == "productType")
                    .map(|(_, v)| v),
                Some(product),
                "wrong Bitget product scope"
            );
        }
        if !public {
            assert!(
                settings
                    || [
                        "/account/accounts",
                        "/position/all-position",
                        "/order/orders-pending",
                        "/order/orders-plan-pending",
                        "/account/account"
                    ]
                    .iter()
                    .any(|p| path.ends_with(p)),
                "unexpected private Bitget read: {path}"
            );
            seen.signed_reads += 1;
            if !valid_rest(path, uri.query(), headers) {
                seen.rejections += 1;
                return json!({"code":"40006","msg":"synthetic proof refusal","data":null});
            }
        }
        let mut reply = if KIND == 2 {
            let rows: Value = serde_json::from_str(include_str!(
                "../fixtures/bitgetinverse-admission-reads.json"
            ))
            .unwrap();
            rows.get(path)
                .unwrap_or_else(|| panic!("unexpected inverse read:{path}"))
                .clone()
        } else {
            let rows: Value = if KIND == 0 {
                serde_json::from_str(include_str!("../fixtures/admission-reads.json")).unwrap()
            } else {
                serde_json::from_str(include_str!("../fixtures/admission-variant-reads.json"))
                    .unwrap()
            };
            let rows = rows[Self::NAME].as_object().unwrap();
            rows.iter()
                .find(|(k, _)| k.as_str() != "*" && path.ends_with(k.as_str()))
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| {
                    assert!(!public, "missing public Bitget fixture");
                    json!({"code":"00000","data":[]})
                })
        };
        if TESTNET {
            simulated(&mut reply);
        }
        reply
    }
    fn http_status(reply: &Value) -> StatusCode {
        if reply["code"] == "40006" {
            StatusCode::BAD_REQUEST
        } else {
            StatusCode::OK
        }
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
                json!({"event":"login","code":if valid{"0"}else{"30012"},"msg":""})
            }
            "subscribe" | "unsubscribe" => json!({"event":op,"arg":v["args"][0],"code":"0"}),
            _ => {
                seen.writes += 1;
                panic!("unexpected Bitget socket operation: {op}");
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
    fn headers(key: &str, material: &str) -> HeaderMap {
        let stamp = now_ms().to_string();
        let payload = format!("{stamp}GET/api/v3/account/settings");
        let sig = hmac_sha256(material.as_bytes(), payload.as_bytes(), Encoding::Base64);
        let mut h = HeaderMap::new();
        for (n, v) in [
            ("access-key", key),
            ("access-passphrase", PASS),
            ("access-timestamp", stamp.as_str()),
            ("access-sign", sig.as_str()),
        ] {
            h.insert(n, v.parse().unwrap());
        }
        h
    }
    #[test]
    fn rest_accepts_both_valid_pairs() {
        for (k, s) in [(KEY, SECRET), (OTHER_KEY, OTHER_SECRET)] {
            assert!(valid_rest("/api/v3/account/settings", None, &headers(k, s)));
        }
    }
    #[test]
    fn rest_rejects_unsigned_wrong_key_and_wrong_material() {
        assert!(!valid_rest(
            "/api/v3/account/settings",
            None,
            &HeaderMap::new()
        ));
        for (k, s) in [
            (KEY, OTHER_SECRET),
            (OTHER_KEY, SECRET),
            ("unknown", SECRET),
        ] {
            assert!(!valid_rest(
                "/api/v3/account/settings",
                None,
                &headers(k, s)
            ));
        }
    }
    #[test]
    fn rest_rejects_passphrase_and_payload_tampering() {
        let base = headers(KEY, SECRET);
        for field in ["access-passphrase", "access-timestamp", "access-sign"] {
            let mut bad = base.clone();
            bad.insert(field, "invalid".parse().unwrap());
            assert!(!valid_rest("/api/v3/account/settings", None, &bad));
        }
        assert!(!valid_rest(
            "/api/v3/account/settings",
            Some("tampered=1"),
            &base
        ));
    }
    #[test]
    fn login_accepts_both_pairs_and_rejects_tampering() {
        for (k, s) in [(KEY, SECRET), (OTHER_KEY, OTHER_SECRET)] {
            let stamp = "1794873600";
            let sig = hmac_sha256(
                s.as_bytes(),
                format!("{stamp}GET/user/verify").as_bytes(),
                Encoding::Base64,
            );
            let good = json!({"op":"login","args":[{"apiKey":k,"passphrase":PASS,"timestamp":stamp,"sign":sig}]});
            assert!(valid_login(&good));
            for field in ["apiKey", "passphrase", "timestamp", "sign"] {
                let mut bad = good.clone();
                bad["args"][0][field] = "invalid".into();
                assert!(!valid_login(&bad));
            }
        }
    }
}
