use super::join_negative::{ACCOUNT, Observed, Venue};
use axum::http::{HeaderMap, Method, Uri};
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::signing::Algorithm;
use safe_cex_exchange_server::{accounts::now_ms, envelope::InitPayload};
use serde_json::{Value, json};

pub struct Bybit;
const KEY: &str = "synthetic-primary-api-key";
const OTHER_KEY: &str = "synthetic-secondary-api-key";
const SECRET: &str = "synthetic-primary-material";
const OTHER_SECRET: &str = "synthetic-secondary-material";

fn dated(mut reply: Value) -> Value {
    // Native Bybit calibrates its signing clock from the venue response time.
    reply["time"] = now_ms().into();
    reply
}

impl Venue for Bybit {
    const NAME: &'static str = "bybit";
    fn algorithm() -> Algorithm {
        Algorithm::HmacSha256
    }
    fn replayable(frame: &Value) -> bool {
        // REST private reads are the proof; socket authentication is a separate startup prerequisite.
        !frame["d"]["wsAuth"]["toSign"]
            .as_str()
            .unwrap()
            .starts_with("GET/realtime")
    }
    fn init() -> InitPayload {
        InitPayload {
            account_name: ACCOUNT.into(),
            exchange_name: Self::NAME.into(),
            key: KEY.into(),
            signing_mode: Some("cosigned".into()),
            instance_id: Some("negative-join-registration".into()),
            ..Default::default()
        }
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
            .expect("native Bybit sign preimage");
        let signature = hmac_sha256(
            if other { OTHER_SECRET } else { SECRET }.as_bytes(),
            preimage.as_bytes(),
            Encoding::Hex,
        );
        let operation = preimage
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .to_owned();
        (
            operation,
            preimage.as_bytes().to_vec(),
            json!({"id":frame["d"]["id"],"wsAuthSignature":signature}),
        )
    }
    fn http(
        method: &Method,
        uri: &Uri,
        headers: &HeaderMap,
        _: &Value,
        seen: &mut Observed,
    ) -> Value {
        let path = uri.path();
        if path.contains("/order/create")
            || path.contains("/order/amend")
            || path.contains("/order/cancel")
        {
            seen.writes += 1;
            panic!("join proof attempted a venue write");
        }
        let header = |name| headers.get(name).and_then(|h| h.to_str().ok());
        if let Some(key) = header("x-bapi-api-key") {
            if method != Method::GET {
                seen.writes += 1;
                panic!("join proof attempted a venue write");
            }
            seen.signed_reads += 1;
            let material = match key {
                KEY => SECRET,
                OTHER_KEY => OTHER_SECRET,
                _ => "",
            };
            let preimage = format!(
                "{}{}{}{}",
                header("x-bapi-timestamp").unwrap_or_default(),
                key,
                header("x-bapi-recv-window").unwrap_or_default(),
                uri.query().unwrap_or_default()
            );
            let expected = hmac_sha256(material.as_bytes(), preimage.as_bytes(), Encoding::Hex);
            let valid_stamp = header("x-bapi-timestamp")
                .and_then(|value| value.parse::<u64>().ok())
                .is_some();
            let valid_window = header("x-bapi-recv-window")
                .and_then(|value| value.parse::<u64>().ok())
                .is_some_and(|window| window > 0);
            if material.is_empty()
                || !valid_stamp
                || !valid_window
                || header("x-bapi-sign") != Some(expected.as_str())
            {
                seen.rejections += 1;
                return json!({"retCode":10003,"retMsg":"synthetic proof refusal","result":{}});
            }
        } else {
            assert_eq!(method, Method::GET, "unsigned write");
            if !path.ends_with("/market/instruments-info") && !path.ends_with("/market/tickers") {
                seen.rejections += 1;
                return json!({"retCode":10003,"retMsg":"synthetic proof refusal","result":{}});
            }
        }
        let fixtures: Value =
            serde_json::from_str(include_str!("../fixtures/delegated-bybit-public.json")).unwrap();
        if path.ends_with("/market/instruments-info")
            && !uri.query().is_some_and(|q| q.contains("PreLaunch"))
        {
            return dated(fixtures["instruments"].clone());
        }
        if path.ends_with("/market/tickers") {
            return dated(fixtures["tickers"].clone());
        }
        let result = if path.ends_with("/account/info") {
            json!({"marginMode":"REGULAR_MARGIN","unifiedMarginStatus":4})
        } else if path.ends_with("/account/wallet-balance") {
            json!({"list":[{"accountType":"UNIFIED","totalEquity":"1000","totalWalletBalance":"1000","totalMarginBalance":"1000","totalAvailableBalance":"1000","totalPerpUPL":"0","coin":[]}]})
        } else {
            json!({"category":"linear","list":[],"nextPageCursor":""})
        };
        dated(json!({"retCode":0,"retMsg":"OK","result":result}))
    }
    fn websocket(request: &Value, seen: &mut Observed) -> Value {
        let op = request["op"].as_str().expect("native Bybit socket op");
        assert!(
            matches!(op, "auth" | "subscribe" | "unsubscribe" | "ping"),
            "write socket op"
        );
        let valid = if op == "auth" {
            let key = request["args"][0].as_str().unwrap();
            let secret = match key {
                KEY => SECRET,
                OTHER_KEY => OTHER_SECRET,
                _ => "",
            };
            let stamp = &request["args"][1];
            let expected = hmac_sha256(
                secret.as_bytes(),
                format!("GET/realtime{stamp}").as_bytes(),
                Encoding::Hex,
            );
            !secret.is_empty() && request["args"][2] == expected
        } else {
            true
        };
        if !valid {
            seen.rejections += 1;
        }
        json!({"op":op,"success":valid,"ret_msg":"","conn_id":"negative-join-socket"})
    }
}
