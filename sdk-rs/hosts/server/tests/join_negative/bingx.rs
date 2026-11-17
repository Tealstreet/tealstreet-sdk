//! Independent BingX signed-read proof; a listen token is not possession.
use super::join_negative::{ACCOUNT, Observed, Venue};
use axum::http::{HeaderMap, Method, Uri};
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::signing::Algorithm;
use safe_cex_exchange_server::{accounts::now_ms, envelope::InitPayload};
use serde_json::{Value, json};
pub struct Bingx<const TESTNET: bool>;
const KEY: &str = "synthetic-primary-api-key";
const OTHER_KEY: &str = "synthetic-secondary-api-key";
const SECRET: &str = "synthetic-primary-material";
const OTHER_SECRET: &str = "synthetic-secondary-material";
const TOKEN: &str = "synthetic-bingx-listen-token";
const PRIVATE: &[&str] = &[
    "/openApi/swap/v1/positionSide/dual",
    "/openApi/swap/v3/user/balance",
    "/openApi/swap/v2/user/positions",
    "/openApi/swap/v2/trade/openOrders",
];
fn material(k: &str) -> Option<&'static str> {
    match k {
        KEY => Some(SECRET),
        OTHER_KEY => Some(OTHER_SECRET),
        _ => None,
    }
}
fn single<'a>(h: &'a HeaderMap, n: &str) -> Option<&'a str> {
    let mut i = h.get_all(n).iter();
    let v = i.next()?.to_str().ok()?;
    i.next().is_none().then_some(v)
}
fn valid(query: &str, headers: &HeaderMap) -> bool {
    (|| {
        let secret = material(single(headers, "X-BX-APIKEY")?)?;
        let (payload, signature) = query.rsplit_once("&signature=")?;
        let mut names = std::collections::BTreeSet::new();
        let mut stamp = None;
        for pair in payload.split('&') {
            let (key, value) = pair.split_once('=')?;
            if !names.insert(key) || key == "signature" {
                return None;
            }
            if key == "timestamp" {
                stamp = Some(value.parse::<u64>().ok()?);
            }
        }
        if stamp?.abs_diff(now_ms()) > 5_000 {
            return None;
        }
        Some(signature == hmac_sha256(secret.as_bytes(), payload.as_bytes(), Encoding::Hex))
    })() == Some(true)
}

impl<const TESTNET: bool> Venue for Bingx<TESTNET> {
    const NAME: &'static str = "bingx";
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
    fn signature(f: &Value, other: bool) -> (String, Vec<u8>, Value) {
        let text = f["d"]["wsAuth"]["toSign"]
            .as_str()
            .expect("native BingX signing bytes");
        let operation = text
            .split('&')
            .filter(|p| !p.starts_with("timestamp="))
            .collect::<Vec<_>>()
            .join("&");
        let sig = hmac_sha256(
            if other { OTHER_SECRET } else { SECRET }.as_bytes(),
            text.as_bytes(),
            Encoding::Hex,
        );
        (
            operation,
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
        assert!(
            !TESTNET,
            "BingX hosted testnet must refuse before venue I/O"
        );
        let path = if let Some(t) = uri.path().strip_prefix("/https://") {
            let at = t.find('/').unwrap();
            assert_eq!(&t[..at], "open-api.bingx.com");
            &t[at..]
        } else {
            uri.path()
        };
        if path == "/openApi/user/auth/userDataStream" {
            assert_eq!(
                method,
                Method::POST,
                "only token acquisition is a startup POST"
            );
            assert!(single(headers, "X-BX-APIKEY").and_then(material).is_some());
            // API-key-only acquisition releases no trading write and is not proof.
            return json!({"listenKey":TOKEN});
        }
        if *method != Method::GET {
            seen.writes += 1;
            panic!("BingX join attempted a write");
        }
        assert!(body.is_null());
        if PRIVATE.contains(&path) {
            seen.signed_reads += 1;
            if !valid(uri.query().unwrap_or_default(), headers) {
                seen.rejections += 1;
                return json!({"code":100001,"msg":"synthetic proof refusal"});
            }
        } else {
            assert!(
                [
                    "/openApi/swap/v2/quote/contracts",
                    "/openApi/swap/v2/quote/ticker",
                    "/openApi/swap/v2/quote/premiumIndex",
                    "/openApi/swap/v2/server/time"
                ]
                .contains(&path),
                "unexpected BingX read: {path}"
            );
        }
        if path == "/openApi/swap/v2/server/time" {
            return json!({"code":0,"data":{"serverTime":now_ms()}});
        }
        let fixtures: Value =
            serde_json::from_str(include_str!("../fixtures/bingx-admission-reads.json")).unwrap();
        fixtures.get(path).expect("BingX startup fixture").clone()
    }
    fn websocket(v: &Value, seen: &mut Observed) -> Value {
        assert_eq!(
            v["reqType"], "sub",
            "BingX startup only subscribes public topics"
        );
        let kind = v["dataType"].as_str().unwrap();
        assert!(
            kind == "market.contracts"
                || kind.starts_with("market.depth.")
                || kind.starts_with("market.kline."),
            "unexpected BingX topic: {kind}"
        );
        let _ = seen;
        json!({"id":v["id"],"code":0,"msg":""})
    }
    fn websocket_text(text: &str, seen: &mut Observed) -> Option<String> {
        match text {
            "Ping" => Some("Pong".into()),
            "Pong" => None,
            _ => Some(Self::websocket(&serde_json::from_str(text).unwrap(), seen).to_string()),
        }
    }
}
#[cfg(test)]
mod controls {
    use super::*;
    fn headers(key: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("X-BX-APIKEY", key.parse().unwrap());
        h
    }
    fn signed(payload: &str, secret: &str) -> String {
        format!(
            "{payload}&signature={}",
            hmac_sha256(secret.as_bytes(), payload.as_bytes(), Encoding::Hex)
        )
    }
    #[test]
    fn both_pairs_are_genuinely_valid() {
        let q = format!("symbol=BTC-USDT&timestamp={}", now_ms());
        for (k, s) in [(KEY, SECRET), (OTHER_KEY, OTHER_SECRET)] {
            assert!(valid(&signed(&q, s), &headers(k)));
        }
    }
    #[test]
    fn wrong_pair_and_unsigned_reads_are_refused() {
        let q = format!("timestamp={}", now_ms());
        assert!(!valid(&signed(&q, OTHER_SECRET), &headers(KEY)));
        assert!(!valid("", &HeaderMap::new()));
    }
    #[test]
    fn signed_query_and_clock_are_bound() {
        let q = format!("symbol=BTC-USDT&timestamp={}", now_ms());
        assert!(!valid(
            &signed(&q, SECRET).replace("BTC-USDT", "ETH-USDT"),
            &headers(KEY)
        ));
        assert!(!valid(&signed("timestamp=1", SECRET), &headers(KEY)));
    }
    #[test]
    fn duplicate_authority_is_refused() {
        let q = format!("timestamp={}", now_ms());
        let mut h = headers(KEY);
        h.append("X-BX-APIKEY", OTHER_KEY.parse().unwrap());
        assert!(!valid(&signed(&q, SECRET), &h));
        assert!(!valid(
            &signed(&format!("{q}&timestamp=1"), SECRET),
            &headers(KEY)
        ));
    }
}
