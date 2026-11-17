//! Independent KuCoin Spot request/passphrase MACs and exact batch reply.
use super::join_negative::{ACCOUNT, Observed, Venue};
use axum::http::{HeaderMap, Method, Uri};
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::signing::Algorithm;
use safe_cex_exchange_server::{accounts::now_ms, envelope::InitPayload};
use serde_json::{Value, json};
pub struct KucoinSpot<const TESTNET: bool>;
const KEY: &str = "synthetic-primary-api-key";
const OTHER_KEY: &str = "synthetic-secondary-api-key";
const SECRET: &str = "synthetic-primary-material";
const OTHER_SECRET: &str = "synthetic-secondary-material";
const PASS: &str = "synthetic-passphrase";
fn material(key: &str) -> Option<&'static str> {
    match key {
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
fn mac(secret: &str, text: &str) -> String {
    hmac_sha256(secret.as_bytes(), text.as_bytes(), Encoding::Base64)
}
fn valid(method: &Method, target: &str, h: &HeaderMap) -> bool {
    (|| {
        let secret = material(single(h, "KC-API-KEY")?)?;
        let stamp = single(h, "KC-API-TIMESTAMP")?;
        if single(h, "KC-API-KEY-VERSION")? != "2"
            || stamp.parse::<u64>().ok()?.abs_diff(now_ms()) > 5_000
        {
            return None;
        }
        Some(
            single(h, "KC-API-SIGN")? == mac(secret, &format!("{stamp}{method}{target}"))
                && single(h, "KC-API-PASSPHRASE")? == mac(secret, PASS),
        )
    })() == Some(true)
}

impl<const TESTNET: bool> Venue for KucoinSpot<TESTNET> {
    const NAME: &'static str = "kucoinspot";
    fn algorithm() -> Algorithm {
        Algorithm::HmacSha256
    }
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
    fn credentials(other: bool) -> Value {
        if other {
            json!({"key":OTHER_KEY,"secret":OTHER_SECRET,"passphrase":PASS})
        } else {
            json!({"key":KEY,"secret":SECRET,"passphrase":PASS})
        }
    }
    fn signature(f: &Value, other: bool) -> (String, Vec<u8>, Value) {
        assert_eq!(f["d"]["wsAuth"]["outputFormat"], "kucoin-batch");
        let raw = f["d"]["wsAuth"]["toSign"].as_str().unwrap();
        let descriptor: Value = serde_json::from_str(raw).unwrap();
        assert!(
            descriptor["brokerPrefix"].is_null(),
            "no broker material in synthetic account"
        );
        assert!(
            descriptor["slot"].is_null() || descriptor["slot"] == "primary",
            "synthetic account uses its primary slot"
        );
        let payload = descriptor["payload"].as_str().unwrap();
        let secret = if other { OTHER_SECRET } else { SECRET };
        let batch =
            json!({"credential":KEY,"macs":[mac(secret,payload),mac(secret,PASS)]}).to_string();
        (
            payload
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .into(),
            payload.as_bytes().to_vec(),
            json!({"id":f["d"]["id"],"wsAuthSignature":batch}),
        )
    }
    fn http(method: &Method, uri: &Uri, h: &HeaderMap, body: &Value, seen: &mut Observed) -> Value {
        assert!(
            !TESTNET,
            "KuCoin hosted testnet must refuse before venue I/O"
        );
        assert!(body.is_null(), "startup never sends order payloads");
        let path = if let Some(t) = uri.path().strip_prefix("/https://") {
            let at = t.find('/').unwrap();
            assert_eq!(&t[..at], "api.kucoin.com");
            &t[at..]
        } else {
            uri.path()
        };
        let public = matches!(
            path,
            "/api/v2/symbols" | "/api/v1/market/allTickers" | "/api/v1/bullet-public"
        );
        if matches!(path, "/api/v1/bullet-private" | "/api/v1/bullet-public") {
            assert_eq!(
                method,
                Method::POST,
                "only read-feed token POSTs are allowed"
            );
        } else if *method != Method::GET {
            seen.writes += 1;
            panic!("KuCoin join attempted a write");
        }
        if !public {
            assert!(
                [
                    "/api/v1/bullet-private",
                    "/api/v1/accounts",
                    "/api/v1/hf/orders/active/symbols",
                    "/api/v1/hf/orders/active/page"
                ]
                .contains(&path),
                "unexpected KuCoin Spot private read: {path}"
            );
            let target = uri
                .query()
                .map_or_else(|| path.to_owned(), |q| format!("{path}?{q}"));
            seen.signed_reads += 1;
            if !valid(method, &target, h) {
                seen.rejections += 1;
                return json!({"code":"400003","msg":"synthetic proof refusal","data":null});
            }
        }
        let fixtures: Value =
            serde_json::from_str(include_str!("../fixtures/kucoinspot-admission-reads.json"))
                .unwrap();
        fixtures
            .get(path)
            .expect("exact KuCoin Spot startup fixture")
            .clone()
    }

    fn initial_websocket_frames() -> Vec<String> {
        vec![json!({"type":"welcome"}).to_string()]
    }
    fn websocket(v: &Value, seen: &mut Observed) -> Value {
        match v["type"].as_str().unwrap() {
            "ping" => json!({"type":"pong","id":v["id"]}),
            "subscribe" | "unsubscribe" => {
                let topic = v["topic"].as_str().unwrap();
                let private = ["/spotMarket/tradeOrdersV2", "/account/balance"].contains(&topic);
                let public = topic == "/market/ticker:all";
                assert!(private || public, "unknown KuCoin topic: {topic}");
                assert_eq!(v["privateChannel"], private);
                json!({"type":"ack","id":v["id"]})
            }
            other => {
                seen.writes += 1;
                panic!("unexpected KuCoin socket operation: {other}");
            }
        }
    }
}
#[cfg(test)]
mod controls {
    use super::*;
    fn headers(key: &str, secret: &str, target: &str) -> HeaderMap {
        let t = now_ms().to_string();
        let mut h = HeaderMap::new();
        for (n, v) in [
            ("KC-API-KEY", key.to_owned()),
            ("KC-API-TIMESTAMP", t.clone()),
            ("KC-API-KEY-VERSION", "2".into()),
            ("KC-API-SIGN", mac(secret, &format!("{t}GET{target}"))),
            ("KC-API-PASSPHRASE", mac(secret, PASS)),
        ] {
            h.insert(n, v.parse().unwrap());
        }
        h
    }
    #[test]
    fn both_pairs_are_genuinely_valid() {
        for (k, s) in [(KEY, SECRET), (OTHER_KEY, OTHER_SECRET)] {
            assert!(valid(
                &Method::GET,
                "/api/v1/accounts?type=trade",
                &headers(k, s, "/api/v1/accounts?type=trade")
            ));
        }
    }
    #[test]
    fn wrong_pair_and_unsigned_are_refused() {
        assert!(!valid(
            &Method::GET,
            "/api/v1/accounts",
            &headers(KEY, OTHER_SECRET, "/api/v1/accounts")
        ));
        assert!(!valid(&Method::GET, "/api/v1/accounts", &HeaderMap::new()));
    }
    #[test]
    fn actual_target_and_passphrase_are_bound() {
        let mut h = headers(KEY, SECRET, "/api/v1/accounts?type=trade");
        assert!(!valid(&Method::GET, "/api/v1/accounts?type=margin", &h));
        h.insert("KC-API-PASSPHRASE", PASS.parse().unwrap());
        assert!(!valid(&Method::GET, "/api/v1/accounts?type=trade", &h));
    }
    #[test]
    fn version_and_singular_authority_are_required() {
        let mut h = headers(KEY, SECRET, "/api/v1/accounts");
        h.insert("KC-API-KEY-VERSION", "1".parse().unwrap());
        assert!(!valid(&Method::GET, "/api/v1/accounts", &h));
        h.insert("KC-API-KEY-VERSION", "2".parse().unwrap());
        h.append("KC-API-KEY", OTHER_KEY.parse().unwrap());
        assert!(!valid(&Method::GET, "/api/v1/accounts", &h));
    }
}
