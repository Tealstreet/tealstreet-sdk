//! Independent synthetic OKX REST and WS-login HMAC verifier.
use super::join_negative::{ACCOUNT, Observed, Venue};
use axum::http::{HeaderMap, Method, Uri};
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::signing::Algorithm;
use safe_cex_exchange_server::envelope::InitPayload;
use serde_json::{Value, json};

pub struct Okx<const KIND: u8>;
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
fn single<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?.to_str().ok()?;
    values.next().is_none().then_some(value)
}
fn valid_rest<const KIND: u8>(path: &str, query: Option<&str>, headers: &HeaderMap) -> bool {
    (|| {
        let material = secret(single(headers, "ok-access-key")?)?;
        if single(headers, "ok-access-passphrase")? != PASS {
            return None;
        }
        let valid_demo = if matches!(KIND, 1 | 3) {
            single(headers, "x-simulated-trading") == Some("1")
        } else {
            headers
                .get_all("x-simulated-trading")
                .iter()
                .next()
                .is_none()
        };
        if !valid_demo {
            return None;
        }
        let stamp = single(headers, "ok-access-timestamp")?;
        if stamp.is_empty() {
            return None;
        }
        let target = match query {
            Some(q) => format!("{path}?{q}"),
            None => path.to_owned(),
        };
        let preimage = format!("{stamp}GET{target}");
        Some(
            single(headers, "ok-access-sign")?
                == hmac_sha256(material.as_bytes(), preimage.as_bytes(), Encoding::Base64),
        )
    })() == Some(true)
}
fn valid_login(request: &Value) -> bool {
    (|| {
        if request["op"] != "login" || request["args"].as_array()?.len() != 1 {
            return None;
        }
        let a = &request["args"][0];
        let material = secret(a["apiKey"].as_str()?)?;
        if a["passphrase"] != PASS {
            return None;
        }
        let stamp = match &a["timestamp"] {
            Value::String(stamp) => stamp.clone(),
            Value::Number(stamp) => stamp.as_u64()?.to_string(),
            _ => return None,
        };
        let parsed = stamp.parse::<u64>().ok()?;
        if stamp != parsed.to_string() {
            return None;
        }
        Some(
            a["sign"]
                == hmac_sha256(
                    material.as_bytes(),
                    format!("{stamp}GET/users/self/verify").as_bytes(),
                    Encoding::Base64,
                ),
        )
    })() == Some(true)
}
impl<const KIND: u8> Venue for Okx<KIND> {
    const NAME: &'static str = match KIND {
        0 => "okx",
        1 => "okxdemo",
        2 => "okxeu",
        3 => "okxeudemo",
        _ => panic!("unknown OKX fixture"),
    };
    fn init() -> InitPayload {
        InitPayload {
            account_name: ACCOUNT.into(),
            exchange_name: Self::NAME.into(),
            key: KEY.into(),
            passphrase: Some(PASS.into()),
            signing_mode: Some("cosigned".into()),
            instance_id: Some("negative-join-registration".into()),
            ..Default::default()
        }
    }
    fn algorithm() -> Algorithm {
        Algorithm::HmacSha256
    }
    fn replayable(frame: &Value) -> bool {
        !frame["d"]["wsAuth"]["toSign"]
            .as_str()
            .unwrap()
            .ends_with("GET/users/self/verify")
    }
    fn credentials(other: bool) -> Value {
        if other {
            json!({"key":OTHER_KEY,"secret":OTHER_SECRET,"passphrase":PASS})
        } else {
            json!({"key":KEY,"secret":SECRET,"passphrase":PASS})
        }
    }
    fn signature(frame: &Value, other: bool) -> (String, Vec<u8>, Value) {
        let preimage = frame["d"]["wsAuth"]["toSign"]
            .as_str()
            .expect("native OKX signing bytes");
        let signature = hmac_sha256(
            if other { OTHER_SECRET } else { SECRET }.as_bytes(),
            preimage.as_bytes(),
            Encoding::Base64,
        );
        let at = preimage.find("GET").expect("read or login preimage");
        (
            preimage[at..].into(),
            preimage.as_bytes().to_vec(),
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
            panic!("OKX join attempted a venue write");
        }
        assert!(body.is_null(), "native OKX startup GET has no JSON body");
        let (host, path) = if let Some(target) = uri.path().strip_prefix("/https://") {
            let at = target.find('/').unwrap();
            (Some(&target[..at]), &target[at..])
        } else {
            (None, uri.path())
        };
        assert!(path.starts_with("/api/v5/"), "unexpected OKX route: {path}");
        if let Some(host) = host {
            assert_eq!(
                host,
                match KIND {
                    0 => "www.okx.com",
                    1 => "openapi.okx.com",
                    2 | 3 => "eea.okx.com",
                    _ => unreachable!(),
                }
            );
        }
        let public = path.starts_with("/api/v5/public/") || path.starts_with("/api/v5/market/");
        if !public {
            assert!(
                [
                    "/account/config",
                    "/account/instruments",
                    "/account/balance",
                    "/account/positions",
                    "/account/account-position-risk",
                    "/trade/orders-pending",
                    "/trade/orders-algo-pending"
                ]
                .iter()
                .any(|suffix| path.ends_with(suffix)),
                "unexpected private OKX read: {path}"
            );
            seen.signed_reads += 1;
            if !valid_rest::<KIND>(path, uri.query(), headers) {
                seen.rejections += 1;
                return json!({"code":"50113","msg":"synthetic signature refusal","data":[]});
            }
        }
        let fixtures: Value =
            serde_json::from_str(include_str!("../fixtures/admission-reads.json")).unwrap();
        let rows = fixtures["okx"].as_object().unwrap();
        // EEA native keyed discovery follows its private config read; it returns the same instrument schema.
        let suffix = if path.ends_with("/account/instruments") {
            "instruments"
        } else {
            path
        };
        rows.iter()
            .find(|(route, _)| route.as_str() != "*" && suffix.ends_with(route.as_str()))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| {
                assert!(!public, "missing public OKX read fixture: {path}");
                json!({"code":"0","msg":"","data":[]})
            })
    }
    fn websocket(request: &Value, seen: &mut Observed) -> Value {
        let op = request["op"].as_str().expect("native OKX socket operation");
        match op {
            "login" => {
                seen.signed_reads += 1;
                let valid = valid_login(request);
                if !valid {
                    seen.rejections += 1;
                }
                json!({"event":"login","code":if valid {"0"}else{"60009"},"msg":""})
            }
            "subscribe" | "unsubscribe" => json!({"event":op,"arg":request["args"][0],"code":"0"}),
            _ => {
                seen.writes += 1;
                panic!("unexpected OKX socket operation: {op}");
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
    fn headers(key: &str, material: &str, demo: bool) -> HeaderMap {
        let stamp = "2026-11-17T00:00:00.000Z";
        let payload = format!("{stamp}GET/api/v5/account/config");
        let signature = hmac_sha256(material.as_bytes(), payload.as_bytes(), Encoding::Base64);
        let mut headers = HeaderMap::new();
        for (name, value) in [
            ("ok-access-key", key),
            ("ok-access-passphrase", PASS),
            ("ok-access-timestamp", stamp),
            ("ok-access-sign", signature.as_str()),
        ] {
            headers.insert(name, value.parse().unwrap());
        }
        if demo {
            headers.insert("x-simulated-trading", "1".parse().unwrap());
        }
        headers
    }
    #[test]
    fn rest_accepts_both_pairs_and_exact_demo_header() {
        for (key, material) in [(KEY, SECRET), (OTHER_KEY, OTHER_SECRET)] {
            assert!(valid_rest::<0>(
                "/api/v5/account/config",
                None,
                &headers(key, material, false)
            ));
            assert!(valid_rest::<3>(
                "/api/v5/account/config",
                None,
                &headers(key, material, true)
            ));
        }
    }
    #[test]
    fn rest_rejects_wrong_key_or_material_and_unsigned_reads() {
        for (key, material) in [
            (KEY, OTHER_SECRET),
            (OTHER_KEY, SECRET),
            ("unknown", SECRET),
        ] {
            assert!(!valid_rest::<0>(
                "/api/v5/account/config",
                None,
                &headers(key, material, false)
            ));
        }
        assert!(!valid_rest::<0>(
            "/api/v5/account/config",
            None,
            &HeaderMap::new()
        ));
    }
    #[test]
    fn rest_rejects_passphrase_demo_and_payload_changes() {
        let base = headers(KEY, SECRET, false);
        for field in [
            "ok-access-passphrase",
            "ok-access-sign",
            "ok-access-timestamp",
        ] {
            let mut bad = base.clone();
            bad.insert(field, "invalid".parse().unwrap());
            assert!(!valid_rest::<0>("/api/v5/account/config", None, &bad));
        }
        assert!(!valid_rest::<0>(
            "/api/v5/account/config",
            Some("tampered=1"),
            &base
        ));
        assert!(!valid_rest::<3>("/api/v5/account/config", None, &base));
        let mut duplicate = headers(KEY, SECRET, true);
        duplicate.append("x-simulated-trading", "1".parse().unwrap());
        assert!(!valid_rest::<3>("/api/v5/account/config", None, &duplicate));
    }
    #[test]
    fn login_accepts_both_pairs_and_rejects_tampering() {
        for (key, material) in [(KEY, SECRET), (OTHER_KEY, OTHER_SECRET)] {
            let payload = "1794873600GET/users/self/verify";
            let signature = hmac_sha256(material.as_bytes(), payload.as_bytes(), Encoding::Base64);
            let good = json!({"op":"login","args":[{"apiKey":key,"passphrase":PASS,"timestamp":"1794873600","sign":signature}]});
            assert!(valid_login(&good));
            for field in ["apiKey", "passphrase", "timestamp", "sign"] {
                let mut bad = good.clone();
                bad["args"][0][field] = "invalid".into();
                assert!(!valid_login(&bad));
            }
        }
    }
}
