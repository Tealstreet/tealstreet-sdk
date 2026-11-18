//! Independent Aster V1 query-HMAC verifier; no wallet proof or live venue I/O.
use super::join_negative::{ACCOUNT, Observed, Venue};
use axum::http::{HeaderMap, Method, Uri};
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::signing::Algorithm;
use safe_cex_exchange_server::{accounts::now_ms, envelope::InitPayload};
use serde_json::{Value, json};

pub struct Aster<const TESTNET: bool>;
const KEY: &str = "synthetic-primary-api-key";
const OTHER_KEY: &str = "synthetic-secondary-api-key";
const SECRET: &str = "synthetic-primary-material";
const OTHER_SECRET: &str = "synthetic-secondary-material";

fn material(key: &str) -> Option<&'static str> {
    match key {
        KEY => Some(SECRET),
        OTHER_KEY => Some(OTHER_SECRET),
        _ => None,
    }
}
fn valid_query(uri: &Uri, headers: &HeaderMap) -> bool {
    (|| {
        let mut keys = headers.get_all("x-mbx-apikey").iter();
        let secret = material(keys.next()?.to_str().ok()?)?;
        if keys.next().is_some() {
            return None;
        }
        let (payload, signature) = uri.query()?.rsplit_once("&signature=")?;
        let mut parameters = std::collections::BTreeMap::new();
        for parameter in payload.split('&') {
            let (name, value) = parameter.split_once('=')?;
            if name == "signature" || parameters.insert(name, value).is_some() {
                return None;
            }
        }
        let stamp = parameters.get("timestamp")?.parse::<u64>().ok()?;
        if parameters.get("recvWindow") != Some(&"20000") || stamp.abs_diff(now_ms()) > 5_000 {
            return None;
        }
        Some(signature == hmac_sha256(secret.as_bytes(), payload.as_bytes(), Encoding::Hex))
    })() == Some(true)
}
impl<const TESTNET: bool> Venue for Aster<TESTNET> {
    const NAME: &'static str = "asterdex";
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
        assert_eq!(frame["d"]["wsAuth"]["outputFormat"], "asterdex-hmac");
        let descriptor: Value =
            serde_json::from_str(frame["d"]["wsAuth"]["toSign"].as_str().unwrap()).unwrap();
        assert_eq!(descriptor["slot"], "primary");
        let payload = descriptor["payload"].as_str().unwrap();
        let signature = hmac_sha256(
            if other { OTHER_SECRET } else { SECRET }.as_bytes(),
            payload.as_bytes(),
            Encoding::Hex,
        );
        let slot = payload
            .split('&')
            .filter(|p| !p.starts_with("timestamp="))
            .collect::<Vec<_>>()
            .join("&");
        let batch = json!({"credential":KEY,"signature":signature}).to_string();
        (
            slot,
            payload.as_bytes().to_vec(),
            json!({"id":frame["d"]["id"],"wsAuthSignature":batch}),
        )
    }
    fn http(
        method: &Method,
        uri: &Uri,
        headers: &HeaderMap,
        body: &Value,
        seen: &mut Observed,
    ) -> Value {
        assert!(body.is_null(), "V1 startup has no order payloads");
        let path = if let Some(target) = uri.path().strip_prefix("/https://") {
            let at = target.find('/').unwrap();
            assert_eq!(
                &target[..at],
                if TESTNET {
                    "fapi.asterdex-testnet.com"
                } else {
                    "fapi.asterdex.com"
                }
            );
            &target[at..]
        } else {
            uri.path()
        };
        let public = matches!(
            path,
            "/fapi/v1/time"
                | "/fapi/v1/exchangeInfo"
                | "/fapi/v1/ticker/24hr"
                | "/fapi/v1/openInterest"
        );
        if path == "/fapi/v1/listenKey" {
            assert!(
                matches!(*method, Method::POST | Method::PUT),
                "only read-feed token requests are allowed"
            );
        } else if *method != Method::GET {
            seen.writes += 1;
            panic!("Aster proof attempted a trading write");
        }
        if !public {
            assert!(
                [
                    "/fapi/v4/account",
                    "/fapi/v1/positionRisk",
                    "/fapi/v1/openOrders",
                    "/fapi/v1/positionSide/dual",
                    "/fapi/v1/leverageBracket",
                    "/fapi/v1/listenKey"
                ]
                .contains(&path),
                "unexpected V1 private read: {path}"
            );
            seen.signed_reads += 1;
            if !valid_query(uri, headers) {
                seen.rejections += 1;
                return json!({"code":-1022,"msg":"synthetic signature refusal"});
            }
        }
        match path {
            "/fapi/v1/time" => json!({"serverTime":now_ms()}),
            "/fapi/v1/exchangeInfo" => {
                json!({"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":2,"quantityPrecision":3,"filters":[{"filterType":"LOT_SIZE","minQty":"0.001","maxQty":"1000","stepSize":"0.001"},{"filterType":"PRICE_FILTER","tickSize":"0.01"}]}]})
            }
            "/fapi/v1/ticker/24hr" => {
                json!([{"symbol":"BTCUSDT","lastPrice":"100","priceChangePercent":"0","volume":"1","quoteVolume":"100"}])
            }
            "/fapi/v4/account" => {
                json!({"totalWalletBalance":"1000","availableBalance":"800","totalInitialMargin":"100","totalOpenOrderInitialMargin":"20","totalUnrealizedProfit":"5"})
            }
            "/fapi/v1/positionSide/dual" => json!({"dualSidePosition":false}),
            "/fapi/v1/listenKey" => json!({"listenKey":"synthetic-aster-listen-key"}),
            "/fapi/v1/leverageBracket"
            | "/fapi/v1/positionRisk"
            | "/fapi/v1/openOrders"
            | "/fapi/v1/openInterest" => json!([]),
            _ => panic!("unexpected V1 read: {path}"),
        }
    }
    fn websocket(request: &Value, seen: &mut Observed) -> Value {
        if !matches!(
            request["method"].as_str(),
            Some("SUBSCRIBE" | "UNSUBSCRIBE" | "LIST_SUBSCRIPTIONS")
        ) {
            seen.writes += 1;
            panic!("unexpected Aster socket operation");
        }
        json!({"id":request["id"],"result":[]})
    }
}

#[cfg(test)]
mod controls {
    use super::*;
    fn request(key: &str, secret: &str, stamp: u64) -> (Uri, HeaderMap) {
        let payload = format!("recvWindow=20000&timestamp={stamp}");
        let signature = hmac_sha256(secret.as_bytes(), payload.as_bytes(), Encoding::Hex);
        let uri = format!("/fapi/v4/account?{payload}&signature={signature}")
            .parse()
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-mbx-apikey", key.parse().unwrap());
        (uri, headers)
    }
    #[test]
    fn both_pairs_are_genuinely_valid() {
        for (key, secret) in [(KEY, SECRET), (OTHER_KEY, OTHER_SECRET)] {
            let (uri, headers) = request(key, secret, now_ms());
            assert!(valid_query(&uri, &headers));
        }
    }
    #[test]
    fn wrong_pair_and_missing_authority_are_refused() {
        let (uri, headers) = request(KEY, OTHER_SECRET, now_ms());
        assert!(!valid_query(&uri, &headers));
        let (uri, _) = request(KEY, SECRET, now_ms());
        assert!(!valid_query(&uri, &HeaderMap::new()));
    }
    #[test]
    fn duplicate_and_tampered_query_or_header_are_refused() {
        let (uri, mut headers) = request(KEY, SECRET, now_ms());
        for query in [
            format!("timestamp=1&{}", uri.query().unwrap()),
            uri.query()
                .unwrap()
                .replace("recvWindow=20000", "recvWindow=19999"),
        ] {
            assert!(!valid_query(
                &format!("/fapi/v4/account?{query}").parse().unwrap(),
                &headers
            ));
        }
        headers.append("x-mbx-apikey", OTHER_KEY.parse().unwrap());
        assert!(!valid_query(&uri, &headers));
    }
    #[test]
    fn signed_stale_or_future_requests_are_refused() {
        for stamp in [now_ms() - 30_000, now_ms() + 30_000] {
            let (uri, headers) = request(KEY, SECRET, stamp);
            assert!(!valid_query(&uri, &headers));
        }
    }
}
