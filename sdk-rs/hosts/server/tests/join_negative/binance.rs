//! Independent synthetic Binance query-HMAC/Spot WS verifier; no live I/O.
use super::join_negative::{ACCOUNT, Observed, Venue};
use axum::http::{HeaderMap, Method, Uri};
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::signing::Algorithm;
use safe_cex_exchange_server::{accounts::now_ms, envelope::InitPayload};
use serde_json::{Value, json};

pub struct Binance<const KIND: u8, const TESTNET: bool>;
const KEY: &str = "synthetic-primary-api-key";
const OTHER_KEY: &str = "synthetic-secondary-api-key";
const SECRET: &str = "synthetic-primary-material";
const OTHER_SECRET: &str = "synthetic-secondary-material";

fn secret(key: &str) -> Option<&'static str> {
    match key {
        KEY => Some(SECRET),
        OTHER_KEY => Some(OTHER_SECRET),
        _ => None,
    }
}
fn valid_query(_uri: &Uri, _headers: &HeaderMap) -> bool {
    // RED checkpoint: controls below must reject an unsigned or forged private read.
    true
}
fn refusal(seen: &mut Observed) -> Value {
    seen.rejections += 1;
    json!({"code":-2015,"msg":"synthetic Binance signature refusal"})
}
impl<const KIND: u8, const TESTNET: bool> Venue for Binance<KIND, TESTNET> {
    const NAME: &'static str = match KIND {
        0 => "binance",
        1 => "binancepm",
        2 => "binanceinverse",
        3 => "binancespot",
        4 => "binanceoptions",
        _ => panic!("unknown Binance fixture"),
    };
    fn init() -> InitPayload {
        InitPayload {
            account_name: ACCOUNT.into(),
            exchange_name: Self::NAME.into(),
            key: KEY.into(),
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
            json!({"key":OTHER_KEY,"secret":OTHER_SECRET})
        } else {
            json!({"key":KEY,"secret":SECRET})
        }
    }
    fn signature(frame: &Value, other: bool) -> (String, Vec<u8>, Value) {
        let preimage = frame["d"]["wsAuth"]["toSign"]
            .as_str()
            .expect("native Binance signing bytes");
        let signature = hmac_sha256(
            if other { OTHER_SECRET } else { SECRET }.as_bytes(),
            preimage.as_bytes(),
            Encoding::Hex,
        );
        let slot = preimage
            .split('&')
            .filter(|p| !p.starts_with("timestamp="))
            .collect::<Vec<_>>()
            .join("&");
        (
            slot,
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
        assert!(
            !(KIND == 4 && TESTNET),
            "Options testnet must refuse before transport"
        );
        assert!(body.is_null(), "Binance startup has no JSON body");
        let path = uri.path();
        let public = [
            "/time",
            "/exchangeInfo",
            "/ticker/24hr",
            "/ticker/bookTicker",
            "/premiumIndex",
            "/ticker",
            "/mark",
        ]
        .iter()
        .any(|suffix| path.ends_with(suffix));
        let feed_token = path.ends_with("/listenKey");
        if feed_token {
            assert!(
                matches!(*method, Method::POST | Method::PUT),
                "listen-key acquisition/renewal only"
            );
            if headers
                .get("x-mbx-apikey")
                .and_then(|v| v.to_str().ok())
                .and_then(secret)
                .is_none()
            {
                return refusal(seen);
            }
        } else {
            if *method != Method::GET {
                seen.writes += 1;
                panic!("Binance join attempted a venue write");
            }
            if !public {
                assert!(
                    [
                        "/account",
                        "/balance",
                        "/marginAccount",
                        "/position",
                        "/positionRisk",
                        "/positionSide/dual",
                        "/leverageBracket",
                        "/symbolConfig",
                        "/openOrders",
                        "/openAlgoOrders",
                        "/algo/openAlgoOrders"
                    ]
                    .iter()
                    .any(|suffix| path.ends_with(suffix)),
                    "unexpected private startup route: {path}"
                );
                seen.signed_reads += 1;
                if !valid_query(uri, headers) {
                    return refusal(seen);
                }
            }
        }
        if path.ends_with("/time") {
            return json!({"serverTime":now_ms()});
        }
        if KIND == 4 {
            let fixtures: Value = serde_json::from_str(include_str!(
                "../fixtures/binanceoptions-admission-reads.json"
            ))
            .unwrap();
            return fixtures
                .get(path)
                .unwrap_or_else(|| panic!("unexpected EAPI route: {path}"))
                .clone();
        }
        let fixtures: Value = if KIND == 0 {
            serde_json::from_str(include_str!("../fixtures/admission-reads.json")).unwrap()
        } else {
            serde_json::from_str(include_str!("../fixtures/admission-variant-reads.json")).unwrap()
        };
        let rows = fixtures[Self::NAME].as_object().unwrap();
        rows.iter()
            .find(|(route, _)| route.as_str() != "*" && path.ends_with(route.as_str()))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| {
                assert!(!public, "missing public fixture: {path}");
                json!([])
            })
    }
    fn websocket(request: &Value, seen: &mut Observed) -> Value {
        assert!(KIND != 4, "Options startup is REST only");
        if request["method"] == "userDataStream.subscribe.signature" {
            assert_eq!(KIND, 3, "signed WS startup is Spot only");
            seen.signed_reads += 1;
            let p = &request["params"];
            let valid = (|| {
                let material = secret(p["apiKey"].as_str()?)?;
                let stamp = p["timestamp"].as_u64()?;
                let window = p["recvWindow"].as_u64()?;
                if window == 0
                    || window > 20_000
                    || stamp > now_ms() + 1000
                    || now_ms().saturating_sub(stamp) > window
                {
                    return None;
                }
                let preimage = format!(
                    "apiKey={}&recvWindow={window}&timestamp={stamp}",
                    p["apiKey"].as_str()?
                );
                Some(
                    p["signature"]
                        == hmac_sha256(material.as_bytes(), preimage.as_bytes(), Encoding::Hex),
                )
            })() == Some(true);
            if valid {
                json!({"id":request["id"],"status":200,"result":{"subscriptionId":1}})
            } else {
                seen.rejections += 1;
                json!({"id":request["id"],"status":401,"error":{"code":-2015,"msg":"synthetic WS signature refusal"}})
            }
        } else {
            assert!(
                matches!(
                    request["method"].as_str(),
                    Some("SUBSCRIBE" | "UNSUBSCRIBE" | "LIST_SUBSCRIPTIONS")
                ),
                "unexpected Binance socket operation: {request}"
            );
            json!({"id":request["id"],"result":null})
        }
    }
}

#[cfg(test)]
mod controls {
    use super::*;
    fn request(key: &str, material: &str) -> (Uri, HeaderMap) {
        let payload = format!("recvWindow=20000&timestamp={}", now_ms());
        let signature = hmac_sha256(material.as_bytes(), payload.as_bytes(), Encoding::Hex);
        let uri = format!("/fapi/v3/account?{payload}&signature={signature}")
            .parse()
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-mbx-apikey", key.parse().unwrap());
        (uri, headers)
    }
    #[test]
    fn rest_verifier_accepts_both_genuinely_valid_pairs() {
        for (key, material) in [(KEY, SECRET), (OTHER_KEY, OTHER_SECRET)] {
            let (u, h) = request(key, material);
            assert!(valid_query(&u, &h));
        }
    }
    #[test]
    fn rest_verifier_rejects_wrong_material_and_unknown_key() {
        for (key, material) in [
            (KEY, OTHER_SECRET),
            (OTHER_KEY, SECRET),
            ("unknown", SECRET),
        ] {
            let (u, h) = request(key, material);
            assert!(!valid_query(&u, &h));
        }
    }
    #[test]
    fn rest_verifier_rejects_unsigned_private_reads() {
        let (u, mut h) = request(KEY, SECRET);
        h.clear();
        assert!(!valid_query(&u, &h));
        let (_, h) = request(KEY, SECRET);
        assert!(!valid_query(
            &"/fapi/v3/account?timestamp=1".parse().unwrap(),
            &h
        ));
    }
    #[test]
    fn rest_verifier_rejects_tampered_bytes_and_duplicate_parameters() {
        let (u, h) = request(KEY, SECRET);
        for query in [
            format!("{}&timestamp=1", u.query().unwrap()),
            u.query()
                .unwrap()
                .replace("recvWindow=20000", "recvWindow=19999"),
        ] {
            assert!(!valid_query(
                &format!("/fapi/v3/account?{query}").parse().unwrap(),
                &h
            ));
        }
    }
}
