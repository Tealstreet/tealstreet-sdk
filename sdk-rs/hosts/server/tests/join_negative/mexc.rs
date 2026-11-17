//! Independent MEXC futures wire verification; public discovery is not proof.
use super::join_negative::{ACCOUNT, Observed, Venue};
use axum::http::{HeaderMap, Method, Uri};
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::signing::Algorithm;
use safe_cex_exchange_server::{accounts::now_ms, envelope::InitPayload};
use serde_json::{Value, json};
pub struct Mexc<const TESTNET: bool>;
const KEY: &str = "synthetic-primary-api-key";
const OTHER_KEY: &str = "synthetic-secondary-api-key";
const SECRET: &str = "synthetic-primary-material";
const OTHER_SECRET: &str = "synthetic-secondary-material";
const PRIVATE: &[&str] = &[
    "/api/v1/private/position/position_mode",
    "/api/v1/private/account/assets",
    "/api/v1/private/position/open_positions",
    "/api/v1/private/order/list/open_orders",
    "/api/v1/private/planorder/list/orders",
    "/api/v1/private/stoporder/list/orders",
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
fn valid(key: Option<&str>, stamp: Option<&str>, signature: Option<&str>, query: &str) -> bool {
    // RED: exercise the real wire with synthetic authentication deliberately unchecked.
    let _ = (key.and_then(material), stamp, signature, query);
    true
}

fn valid_rest(uri: &Uri, headers: &HeaderMap) -> bool {
    valid(
        single(headers, "ApiKey"),
        single(headers, "Request-Time"),
        single(headers, "Signature"),
        uri.query().unwrap_or_default(),
    )
}
fn parts(text: &str) -> (&str, &str) {
    let tail = text
        .strip_prefix(KEY)
        .or_else(|| text.strip_prefix(OTHER_KEY))
        .expect("native MEXC key prefix");
    let at = tail
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(tail.len());
    (&tail[..at], &tail[at..])
}
impl<const TESTNET: bool> Venue for Mexc<TESTNET> {
    const NAME: &'static str = "mexc";
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
    fn replayable(f: &Value) -> bool {
        // Empty payload can be either WS login or REST. Only query-bearing private
        // reads are replayed; current valid signatures service both prerequisites.
        !parts(f["d"]["wsAuth"]["toSign"].as_str().unwrap())
            .1
            .is_empty()
    }
    fn signature(f: &Value, other: bool) -> (String, Vec<u8>, Value) {
        let text = f["d"]["wsAuth"]["toSign"]
            .as_str()
            .expect("native MEXC signing bytes");
        let (_, query) = parts(text);
        let operation = query
            .split('&')
            .filter(|s| !s.starts_with("start_time=") && !s.starts_with("end_time="))
            .collect::<Vec<_>>()
            .join("&");
        let signature = hmac_sha256(
            if other { OTHER_SECRET } else { SECRET }.as_bytes(),
            text.as_bytes(),
            Encoding::Hex,
        );
        (
            operation,
            text.as_bytes().to_vec(),
            json!({"id":f["d"]["id"],"wsAuthSignature":signature}),
        )
    }
    fn http(
        method: &Method,
        uri: &Uri,
        headers: &HeaderMap,
        body: &Value,
        seen: &mut Observed,
    ) -> Value {
        assert!(!TESTNET, "MEXC hosted testnet must refuse before venue I/O");
        if *method != Method::GET {
            seen.writes += 1;
            panic!("MEXC join attempted a write");
        }
        assert!(body.is_null());
        let path = if let Some(t) = uri.path().strip_prefix("/https://") {
            let at = t.find('/').unwrap();
            assert_eq!(&t[..at], "api.mexc.com");
            &t[at..]
        } else {
            uri.path()
        };
        if PRIVATE.contains(&path) {
            seen.signed_reads += 1;
            if !valid_rest(uri, headers) {
                seen.rejections += 1;
                return json!({"success":false,"code":602,"message":"synthetic proof refusal"});
            }
        } else {
            assert!(
                [
                    "/api/v1/contract/detail",
                    "/api/v1/contract/ticker",
                    "/api/v1/contract/ping"
                ]
                .contains(&path),
                "unexpected MEXC read: {path}"
            );
        }
        if path == "/api/v1/contract/ping" {
            return json!({"success":true,"code":0,"data":now_ms()});
        }
        let fixtures: Value =
            serde_json::from_str(include_str!("../fixtures/mexc-admission-reads.json")).unwrap();
        fixtures.get(path).expect("MEXC startup fixture").clone()
    }
    fn websocket(v: &Value, seen: &mut Observed) -> Value {
        assert!(!TESTNET, "MEXC hosted testnet socket must remain unopened");
        let method = v["method"].as_str().unwrap();
        match method {
            "login" => {
                let p = &v["param"];
                seen.signed_reads += 1;
                let accepted = valid(
                    p["apiKey"].as_str(),
                    p["reqTime"].as_str(),
                    p["signature"].as_str(),
                    "",
                );
                if !accepted {
                    seen.rejections += 1;
                }
                json!({"channel":"rs.login","success":accepted,"data":if accepted{"success"}else{"refused"}})
            }
            "ping" => json!({"channel":"pong","data":now_ms()}),
            "personal.filter" => json!({"channel":"rs.personal.filter","data":"success"}),
            "sub.tickers" | "sub.depth" | "sub.kline" | "sub.deal" => {
                json!({"channel":format!("rs.{method}"),"data":"success"})
            }
            _ => {
                seen.writes += 1;
                panic!("unexpected MEXC socket operation: {method}");
            }
        }
    }
}
#[cfg(test)]
mod controls {
    use super::*;
    fn signature(key: &str, secret: &str, stamp: &str, query: &str) -> String {
        hmac_sha256(
            secret.as_bytes(),
            format!("{key}{stamp}{query}").as_bytes(),
            Encoding::Hex,
        )
    }
    #[test]
    fn both_pairs_are_genuinely_valid() {
        for (k, s) in [(KEY, SECRET), (OTHER_KEY, OTHER_SECRET)] {
            let t = now_ms().to_string();
            assert!(valid(
                Some(k),
                Some(&t),
                Some(&signature(k, s, &t, "page_num=1")),
                "page_num=1"
            ));
        }
    }
    #[test]
    fn wrong_pair_and_unsigned_reads_are_refused() {
        let t = now_ms().to_string();
        assert!(!valid(
            Some(KEY),
            Some(&t),
            Some(&signature(KEY, OTHER_SECRET, &t, "page_num=1")),
            "page_num=1"
        ));
        assert!(!valid(None, None, None, ""));
    }
    #[test]
    fn signed_query_and_clock_are_bound() {
        let t = now_ms().to_string();
        let sig = signature(KEY, SECRET, &t, "page_num=1");
        assert!(!valid(Some(KEY), Some(&t), Some(&sig), "page_num=2"));
        assert!(!valid(
            Some(KEY),
            Some("1"),
            Some(&signature(KEY, SECRET, "1", "")),
            ""
        ));
    }
    #[test]
    fn duplicate_authority_headers_are_refused() {
        let t = now_ms().to_string();
        let mut h = HeaderMap::new();
        h.insert("ApiKey", KEY.parse().unwrap());
        h.insert("Request-Time", t.parse().unwrap());
        h.insert(
            "Signature",
            signature(KEY, SECRET, &t, "page_num=1").parse().unwrap(),
        );
        h.append("ApiKey", OTHER_KEY.parse().unwrap());
        assert!(!valid_rest(
            &"/api/v1/private/order/list/open_orders?page_num=1"
                .parse()
                .unwrap(),
            &h
        ));
    }
}
