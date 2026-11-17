//! HyroTrader's native Bybit-linear protocol, with independent REST/WS checks.
use super::join_negative::{ACCOUNT, Observed, Venue, WebsocketSession};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::signing::Algorithm;
use safe_cex_exchange_server::{accounts::now_ms, envelope::InitPayload};
use serde_json::{Value, json};

pub struct HyroTrader<const TESTNET: bool = false>;
const KEY: &str = "synthetic-primary-api-key";
const OTHER_KEY: &str = "synthetic-secondary-api-key";
const SECRET: &str = "synthetic-primary-material";
const OTHER_SECRET: &str = "synthetic-secondary-material";

fn valid_read(_headers: &HeaderMap, _query: &str) -> bool {
    // Committed RED intentionally leaves fixture authentication unchecked.
    true
}
fn valid_auth(_request: &Value) -> bool {
    true
}
fn socket_reply(request: &Value, accepted: bool, seen: &mut Observed) -> Value {
    if !accepted {
        seen.rejections += 1;
    }
    json!({"op":request["op"],"req_id":request["req_id"],"success":accepted,"ret_code":if accepted {0} else {10003},"ret_msg":"","conn_id":"synthetic-hyro-socket"})
}
fn subscription(request: &Value, authenticated: bool, seen: &mut Observed) -> Value {
    let args = request["args"]
        .as_array()
        .expect("native Hyro subscription topics");
    assert!(!args.is_empty());
    let mut private = false;
    for topic in args {
        let topic = topic.as_str().unwrap();
        if ["order", "position", "execution", "wallet"].contains(&topic) {
            private = true;
        } else if !topic.starts_with("tickers.") && !topic.starts_with("orderbook.") {
            seen.writes += 1;
            panic!("unhandled Hyro subscription: {topic}");
        }
    }
    socket_reply(request, !private || authenticated, seen)
}
impl<const TESTNET: bool> Venue for HyroTrader<TESTNET> {
    const NAME: &'static str = "hyrotrader";
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
    fn replayable(frame: &Value) -> bool {
        !frame["d"]["wsAuth"]["toSign"]
            .as_str()
            .expect("native Hyro signing bytes")
            .starts_with("GET/realtime")
    }
    fn signature(frame: &Value, other: bool) -> (String, Vec<u8>, Value) {
        let text = frame["d"]["wsAuth"]["toSign"]
            .as_str()
            .expect("native Hyro signing bytes");
        let socket = text.starts_with("GET/realtime");
        assert!(
            socket
                || text
                    .trim_start_matches(|c: char| c.is_ascii_digit())
                    .starts_with(KEY),
            "unexpected Hyro signing recipe"
        );
        // Isolate wrong REST material/replay behind a fresh valid socket handshake,
        // as the native probe does. Bad socket MACs have independent wire controls.
        let secret = if other && !socket {
            OTHER_SECRET
        } else {
            SECRET
        };
        let signature = hmac_sha256(secret.as_bytes(), text.as_bytes(), Encoding::Hex);
        (
            text.trim_start_matches(|c: char| c.is_ascii_digit()).into(),
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
        if method != Method::GET || !body.is_null() {
            seen.writes += 1;
            panic!("join attempted a Hyro non-read");
        }
        let path = if let Some(target) = uri.path().strip_prefix("/https://") {
            let slash = target.find('/').expect("forwarded Hyro path");
            assert_eq!(
                &target[..slash],
                "sim-tealstreet.hyro.site",
                "wrong Hyro upstream authority"
            );
            &target[slash..]
        } else {
            uri.path()
        };
        let private = match path {
            "/v5/market/instruments-info" | "/v5/market/tickers" => false,
            "/v5/account/info"
            | "/v5/account/wallet-balance"
            | "/v5/position/list"
            | "/v5/order/realtime" => true,
            other => {
                seen.writes += 1;
                panic!("unhandled Hyro operation: {other}");
            }
        };
        if private {
            seen.signed_reads += 1;
            if !valid_read(headers, uri.query().unwrap_or_default()) {
                seen.rejections += 1;
                return json!({"retCode":10003,"retMsg":"synthetic credential refusal","result":{},"time":now_ms()});
            }
        }
        // Reuse only the established linear response corpus, never its venue origin.
        let corpus: Value =
            serde_json::from_str(include_str!("../fixtures/admission-variant-reads.json")).unwrap();
        let bodies = corpus["bybitdemo"].as_object().unwrap();
        let mut reply = if path.ends_with("instruments-info")
            && uri.query().is_some_and(|q| q.contains("PreLaunch"))
        {
            bodies["*"].clone()
        } else {
            bodies
                .iter()
                .find(|(suffix, _)| suffix.as_str() != "*" && path.ends_with(suffix.as_str()))
                .map_or(&bodies["*"], |(_, v)| v)
                .clone()
        };
        // Native Bybit clocks calibrate from this response time, so it must advance.
        reply["time"] = now_ms().into();
        reply
    }
    fn http_status(reply: &Value) -> StatusCode {
        if reply["retCode"] == 10003 {
            StatusCode::UNAUTHORIZED
        } else {
            StatusCode::OK
        }
    }
    fn websocket(request: &Value, seen: &mut Observed) -> Value {
        match request["op"].as_str().unwrap() {
            "ping" => json!({"op":"pong","success":true}),
            "subscribe" | "unsubscribe" => subscription(request, false, seen),
            "auth" => socket_reply(request, false, seen),
            other => {
                seen.writes += 1;
                panic!("unknown Hyro socket operation: {other}");
            }
        }
    }
    fn websocket_session() -> WebsocketSession {
        let mut authenticated = false;
        Box::new(move |text, seen| {
            let request: Value = serde_json::from_str(text).expect("native Hyro JSON");
            let reply = match request["op"].as_str().unwrap() {
                "auth" => {
                    authenticated = valid_auth(&request);
                    socket_reply(&request, authenticated, seen)
                }
                "subscribe" | "unsubscribe" => subscription(&request, authenticated, seen),
                _ => Self::websocket(&request, seen),
            };
            vec![reply.to_string()]
        })
    }
}

#[cfg(test)]
mod controls {
    use super::*;
    fn headers(key: &str, secret: &str, stamp: &str, window: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-bapi-api-key", key.parse().unwrap());
        headers.insert("x-bapi-timestamp", stamp.parse().unwrap());
        headers.insert("x-bapi-recv-window", window.parse().unwrap());
        headers.insert(
            "x-bapi-sign",
            hmac_sha256(
                secret.as_bytes(),
                format!("{stamp}{key}{window}accountType=UNIFIED").as_bytes(),
                Encoding::Hex,
            )
            .parse()
            .unwrap(),
        );
        headers
    }
    fn http(headers: &HeaderMap) -> Value {
        HyroTrader::<false>::http(
            &Method::GET,
            &"/v5/account/wallet-balance?accountType=UNIFIED"
                .parse()
                .unwrap(),
            headers,
            &Value::Null,
            &mut Observed::default(),
        )
    }
    #[test]
    fn http_both_genuine_pairs_pass() {
        for (key, secret) in [(KEY, SECRET), (OTHER_KEY, OTHER_SECRET)] {
            assert_eq!(
                http(&headers(key, secret, &now_ms().to_string(), "5000"))["retCode"],
                0
            );
        }
    }
    #[test]
    fn http_wrong_material_refuses() {
        assert_eq!(
            http(&headers(KEY, OTHER_SECRET, &now_ms().to_string(), "5000"))["retCode"],
            10003
        );
    }
    #[test]
    fn http_unknown_missing_or_duplicate_headers_refuse() {
        let good = headers(KEY, SECRET, &now_ms().to_string(), "5000");
        assert_eq!(
            http(&headers(
                "synthetic-unknown-key",
                SECRET,
                &now_ms().to_string(),
                "5000"
            ))["retCode"],
            10003
        );
        for name in [
            "x-bapi-api-key",
            "x-bapi-timestamp",
            "x-bapi-recv-window",
            "x-bapi-sign",
        ] {
            let mut missing = good.clone();
            missing.remove(name);
            assert_eq!(http(&missing)["retCode"], 10003);
            let mut duplicate = good.clone();
            duplicate.append(name, good[name].clone());
            assert_eq!(http(&duplicate)["retCode"], 10003);
        }
    }
    #[test]
    fn http_stale_stamp_or_invalid_window_refuses() {
        assert_eq!(http(&headers(KEY, SECRET, "1", "5000"))["retCode"], 10003);
        for window in ["0", "bad", "60001"] {
            assert_eq!(
                http(&headers(KEY, SECRET, &now_ms().to_string(), window))["retCode"],
                10003
            );
        }
    }
}
