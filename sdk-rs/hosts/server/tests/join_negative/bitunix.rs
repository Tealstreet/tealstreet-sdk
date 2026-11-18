//! Independent Bitunix double-SHA256 verification; no production auth shortcut.
use super::join_negative::{ACCOUNT, Observed, Venue};
use axum::http::{HeaderMap, Method, Uri};
use safe_cex_crypto::{hex, sha256};
use safe_cex_engine::signing::Algorithm;
use safe_cex_exchange_server::{accounts::now_ms, envelope::InitPayload};
use serde_json::{Value, json};
pub struct Bitunix<const SIMULATED: bool>;
const KEY: &str = "synthetic-primary-api-key";
const OTHER_KEY: &str = "synthetic-secondary-api-key";
const SECRET: &str = "synthetic-primary-material";
const OTHER_SECRET: &str = "synthetic-secondary-material";
const PRIVATE: &[&str] = &[
    "/api/v1/futures/account",
    "/api/v1/futures/position/get_pending_positions",
    "/api/v1/futures/trade/get_pending_orders",
    "/api/v1/futures/tpsl/get_pending_orders",
];
fn material(key: &str) -> Option<&'static str> {
    match key {
        KEY => Some(SECRET),
        OTHER_KEY => Some(OTHER_SECRET),
        _ => None,
    }
}
fn signature(text: &str, secret: &str) -> String {
    hex(&sha256(
        format!("{}{secret}", hex(&sha256(text.as_bytes()))).as_bytes(),
    ))
}
fn single<'a>(h: &'a HeaderMap, n: &str) -> Option<&'a str> {
    let mut it = h.get_all(n).iter();
    let v = it.next()?.to_str().ok()?;
    it.next().is_none().then_some(v)
}
fn valid(key: &str, nonce: &str, stamp: &str, query: &str, sig: &str) -> bool {
    let Some(secret) = material(key) else {
        return false;
    };
    if nonce.is_empty()
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || !stamp
            .parse::<u64>()
            .is_ok_and(|t| t.abs_diff(now_ms()) <= 10_000)
    {
        return false;
    }
    sig == signature(&format!("{nonce}{stamp}{key}{query}"), secret)
}

fn valid_rest<const SIMULATED: bool>(uri: &Uri, h: &HeaderMap) -> bool {
    (|| {
        let simulation = single(h, "x-simulated-trading");
        if (SIMULATED && simulation != Some("1"))
            || (!SIMULATED && h.contains_key("x-simulated-trading"))
        {
            return None;
        }
        let mut pairs = std::collections::BTreeMap::new();
        for p in uri
            .query()
            .unwrap_or_default()
            .split('&')
            .filter(|p| !p.is_empty())
        {
            let (k, v) = p.split_once('=')?;
            if pairs.insert(k, v).is_some() {
                return None;
            }
        }
        let query = pairs
            .iter()
            .map(|(k, v)| format!("{k}{v}"))
            .collect::<String>();
        Some(valid(
            single(h, "api-key")?,
            single(h, "nonce")?,
            single(h, "timestamp")?,
            &query,
            single(h, "sign")?,
        ))
    })() == Some(true)
}
impl<const SIMULATED: bool> Venue for Bitunix<SIMULATED> {
    const NAME: &'static str = "bitunix";
    fn init() -> InitPayload {
        InitPayload {
            account_name: ACCOUNT.into(),
            exchange_name: Self::NAME.into(),
            key: KEY.into(),
            testnet: Some(SIMULATED),
            signing_mode: Some("cosigned".into()),
            instance_id: Some("negative-join-registration".into()),
            ..Default::default()
        }
    }
    fn algorithm() -> Algorithm {
        Algorithm::Sha256SecretSuffix
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
            .expect("native Bitunix signing bytes");
        let query = text
            .split_once(KEY)
            .or_else(|| text.split_once(OTHER_KEY))
            .expect("presented Bitunix key")
            .1;
        let sig = signature(text, if other { OTHER_SECRET } else { SECRET });
        (
            query.into(),
            text.as_bytes().to_vec(),
            json!({"id":f["d"]["id"],"wsAuthSignature":sig}),
        )
    }
    fn http(method: &Method, uri: &Uri, h: &HeaderMap, body: &Value, seen: &mut Observed) -> Value {
        if *method != Method::GET {
            seen.writes += 1;
            panic!("Bitunix join attempted a write");
        }
        assert!(body.is_null());
        let path = if let Some(t) = uri.path().strip_prefix("/https://") {
            let at = t.find('/').unwrap();
            assert_eq!(&t[..at], "fapi.bitunix.com");
            &t[at..]
        } else {
            uri.path()
        };
        if PRIVATE.contains(&path) {
            seen.signed_reads += 1;
            if !valid_rest::<SIMULATED>(uri, h) {
                seen.rejections += 1;
                return json!({"code":10007,"msg":"synthetic proof refusal","data":null});
            }
        } else {
            assert!(
                [
                    "/api/v1/futures/market/trading_pairs",
                    "/api/v1/futures/market/tickers"
                ]
                .contains(&path),
                "unexpected Bitunix read: {path}"
            );
        }
        let fixtures: Value =
            serde_json::from_str(include_str!("../fixtures/bitunix-admission-reads.json")).unwrap();
        fixtures.get(path).expect("native startup fixture").clone()
    }
    fn websocket(v: &Value, seen: &mut Observed) -> Value {
        match v["op"].as_str() {
            Some("login") => {
                let a = &v["args"][0];
                seen.signed_reads += 1;
                let good = valid(
                    a["apiKey"].as_str().unwrap(),
                    a["nonce"].as_str().unwrap(),
                    &a["timestamp"]
                        .as_u64()
                        .expect("native numeric WS timestamp")
                        .to_string(),
                    "",
                    a["sign"].as_str().unwrap(),
                );
                if !good {
                    seen.rejections += 1;
                }
                json!({"op":"login","data":{"result":good}})
            }
            Some("ping") => json!({"op":"pong","pong":v["ping"]}),
            Some("subscribe") => {
                for a in v["args"].as_array().unwrap() {
                    assert!(
                        matches!(a["ch"].as_str(), Some("ticker" | "price")),
                        "only public startup channels"
                    );
                }
                json!({"op":"subscribe","code":0})
            }
            None if v.get("ping").is_some() => json!({"pong":v["ping"]}),
            None if v["event"] == "sub" => {
                assert!(matches!(
                    v["params"]["channel"].as_str(),
                    Some("system_notice" | "reconnected" | "onconnected")
                ));
                json!({"event":"sub","code":0})
            }
            _ => {
                seen.writes += 1;
                panic!("unexpected Bitunix socket operation: {v}");
            }
        }
    }
}
#[cfg(test)]
mod controls {
    use super::*;
    fn proof(k: &str, s: &str, stamp: &str, query: &str) -> String {
        signature(&format!("syntheticnonce{stamp}{k}{query}"), s)
    }
    #[test]
    fn both_pairs_are_genuinely_valid() {
        let t = now_ms().to_string();
        for (k, s) in [(KEY, SECRET), (OTHER_KEY, OTHER_SECRET)] {
            assert!(valid(
                k,
                "syntheticnonce",
                &t,
                "limit100",
                &proof(k, s, &t, "limit100")
            ));
        }
    }
    #[test]
    fn wrong_pair_and_unsigned_are_refused() {
        let t = now_ms().to_string();
        assert!(!valid(
            KEY,
            "syntheticnonce",
            &t,
            "limit100",
            &proof(KEY, OTHER_SECRET, &t, "limit100")
        ));
        assert!(!valid("", "", "", "", ""));
    }
    #[test]
    fn nonce_query_and_clock_are_bound() {
        let t = now_ms().to_string();
        let sig = proof(KEY, SECRET, &t, "limit100");
        assert!(!valid(KEY, "syntheticnonce", &t, "limit101", &sig));
        assert!(!valid(KEY, "othernonce", &t, "limit100", &sig));
        assert!(!valid(
            KEY,
            "syntheticnonce",
            "1",
            "",
            &proof(KEY, SECRET, "1", "")
        ));
    }
    #[test]
    fn raw_digest_or_single_hash_is_not_the_wire() {
        let t = now_ms().to_string();
        let text = format!("syntheticnonce{t}{KEY}limit100");
        let wrong = hex(&sha256(format!("{text}{SECRET}").as_bytes()));
        assert!(!valid(KEY, "syntheticnonce", &t, "limit100", &wrong));
    }
}
