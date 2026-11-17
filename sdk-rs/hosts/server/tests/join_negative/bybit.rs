use super::join_negative::{ACCOUNT, Observed, Venue};
use axum::http::{HeaderMap, Method, Uri};
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::signing::Algorithm;
use safe_cex_exchange_server::{accounts::now_ms, envelope::InitPayload};
use serde_json::{Value, json};

pub struct BybitCase<const VARIANT: u8 = 0, const TESTNET: bool = false, const BRAZIL: bool = false>;
pub type Bybit = BybitCase<0, false, false>;

const KEY: &str = "synthetic-primary-api-key";
const OTHER_KEY: &str = "synthetic-secondary-api-key";
const SECRET: &str = "synthetic-primary-material";
const OTHER_SECRET: &str = "synthetic-secondary-material";

fn dated(mut reply: Value) -> Value {
    // Native Bybit calibrates its signing clock from the venue response time.
    reply["time"] = now_ms().into();
    reply
}

impl<const VARIANT: u8, const TESTNET: bool, const BRAZIL: bool> Venue
    for BybitCase<VARIANT, TESTNET, BRAZIL>
{
    const NAME: &'static str = match VARIANT {
        0 => "bybit",
        1 => "bybitinverse",
        2 => "bybitdemo",
        3 => "bybitspot",
        _ => panic!("invalid Bybit test variant"),
    };
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
        let mut init = InitPayload {
            account_name: ACCOUNT.into(),
            exchange_name: Self::NAME.into(),
            key: KEY.into(),
            signing_mode: Some("cosigned".into()),
            instance_id: Some("negative-join-registration".into()),
            ..Default::default()
        };
        if TESTNET {
            init.testnet = Some(true);
        }
        if BRAZIL {
            init.bybit_site_id = Some("BRA_BTL".into());
        }
        init
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
        if VARIANT != 0 {
            let captures: Value = serde_json::from_str(if VARIANT == 1 {
                include_str!("../fixtures/admission-reads.json")
            } else {
                include_str!("../fixtures/admission-variant-reads.json")
            })
            .unwrap();
            let bodies = captures[Self::NAME]
                .as_object()
                .expect("exact Bybit product fixture");
            let response = if path.ends_with("/market/instruments-info")
                && uri.query().is_some_and(|q| q.contains("PreLaunch"))
            {
                bodies["*"].clone()
            } else {
                bodies
                    .iter()
                    .find(|(suffix, _)| suffix.as_str() != "*" && path.ends_with(suffix.as_str()))
                    .map_or(&bodies["*"], |(_, body)| body)
                    .clone()
            };
            return dated(response);
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

#[cfg(test)]
mod websocket_controls {
    use super::*;
    use crate::join_negative::Fixture;
    use futures_util::{SinkExt, StreamExt};
    use std::time::Duration;
    use tokio_tungstenite::tungstenite::Message;

    fn auth<V: Venue>(other: bool, expiry: &str, numeric: bool) -> Value {
        let credentials = V::credentials(other);
        let signature = hmac_sha256(
            credentials["secret"].as_str().unwrap().as_bytes(),
            format!("GET/realtime{expiry}").as_bytes(),
            Encoding::Hex,
        );
        let mut request = json!({"op":"auth","args":[credentials["key"],expiry,signature]});
        if numeric {
            request["args"][1] = expiry.parse::<u64>().unwrap().into();
        }
        request
    }

    async fn exchange(f: &Fixture, request: &Value) -> Value {
        tokio::time::timeout(Duration::from_secs(2), async {
            let (mut socket, _) = tokio_tungstenite::connect_async(&f.websocket_url)
                .await
                .unwrap();
            socket
                .send(Message::text(request.to_string()))
                .await
                .unwrap();
            let reply = socket.next().await.unwrap().unwrap();
            let reply = serde_json::from_str(reply.to_text().unwrap()).unwrap();
            socket.close(None).await.unwrap();
            reply
        })
        .await
        .expect("loopback Bybit auth response")
    }

    async fn native_strings<V: Venue>() {
        let f = Fixture::new::<V>().await;
        let expiry = (now_ms() + 30_000).to_string();
        for other in [false, true] {
            let request = auth::<V>(other, &expiry, false);
            assert!(
                request["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(Value::is_string)
            );
            assert_eq!(
                exchange(&f, &request).await["success"],
                true,
                "native string expiry rejected for {} testnet={:?} site={:?} other={other}",
                V::NAME,
                V::init().testnet,
                V::init().bybit_site_id
            );
        }
        let valid = auth::<V>(false, &expiry, false);
        let other = V::credentials(true);
        let wrong_mac = hmac_sha256(
            other["secret"].as_str().unwrap().as_bytes(),
            format!("GET/realtime{expiry}").as_bytes(),
            Encoding::Hex,
        );
        let mut bad = Vec::new();
        for (field, value) in [
            (0, other["key"].clone()),
            (0, json!("synthetic-unrecognized-key")),
            (0, Value::Null),
            (1, json!((expiry.parse::<u64>().unwrap() + 1).to_string())),
            (1, Value::Null),
            (1, json!("")),
            (1, json!(true)),
            (1, json!({"expiry":expiry})),
            (2, json!(wrong_mac)),
            (2, Value::Null),
            (2, json!("")),
        ] {
            let mut request = valid.clone();
            request["args"][field] = value;
            bad.push(request);
        }
        for args in [
            Value::Null,
            json!([]),
            json!([valid["args"][0], expiry]),
            json!([valid["args"][0], expiry, valid["args"][2], "extra"]),
        ] {
            bad.push(json!({"op":"auth","args":args}));
        }
        for request in &bad {
            assert_eq!(
                exchange(&f, request).await["success"],
                false,
                "invalid authentication accepted for {}",
                V::NAME
            );
        }
        let seen = f.seen.lock().unwrap();
        assert_eq!(seen.rejections, bad.len());
        assert_eq!(seen.writes, 0);
    }

    async fn numeric_compatibility<V: Venue>() {
        let f = Fixture::new::<V>().await;
        let expiry = (now_ms() + 30_000).to_string();
        for other in [false, true] {
            assert_eq!(
                exchange(&f, &auth::<V>(other, &expiry, true)).await["success"],
                true
            );
        }
        let seen = f.seen.lock().unwrap();
        assert_eq!(seen.rejections, 0);
        assert_eq!(seen.writes, 0);
    }

    async fn quoted_mac_is_not_native<V: Venue>() {
        let f = Fixture::new::<V>().await;
        let expiry = (now_ms() + 30_000).to_string();
        let mut request = auth::<V>(false, &expiry, false);
        let signature = hmac_sha256(
            SECRET.as_bytes(),
            format!("GET/realtime{}", json!(expiry)).as_bytes(),
            Encoding::Hex,
        );
        request["args"][2] = signature.into();
        assert_eq!(
            exchange(&f, &request).await["success"],
            false,
            "JSON-quoted expiry is not the native signing preimage"
        );
        assert_eq!(f.seen.lock().unwrap().rejections, 1);
        assert_eq!(f.seen.lock().unwrap().writes, 0);
    }

    macro_rules! all_scopes {
        ($check:ident) => {{
            $check::<Bybit>().await;
            $check::<BybitCase<0, true>>().await;
            $check::<BybitCase<1>>().await;
            $check::<BybitCase<1, true>>().await;
            $check::<BybitCase<2>>().await;
            $check::<BybitCase<2, true>>().await;
            $check::<BybitCase<3>>().await;
            $check::<BybitCase<3, true>>().await;
            $check::<BybitCase<1, false, true>>().await;
        }};
    }

    #[tokio::test]
    async fn native_string_expiry_and_invalid_auth_controls() {
        all_scopes!(native_strings);
    }
    #[tokio::test]
    async fn existing_numeric_expiry_uses_the_same_unquoted_preimage() {
        all_scopes!(numeric_compatibility);
    }
    #[tokio::test]
    async fn signatures_over_json_quotes_are_refused() {
        all_scopes!(quoted_mac_is_not_native);
    }
}
