//! Crypto.com fixture independently verifies closed native read/auth envelopes.
use super::join_negative::{ACCOUNT, Observed, Venue, WebsocketSession};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::signing::Algorithm;
use safe_cex_exchange_server::{accounts::now_ms, envelope::InitPayload};
use serde_json::{Value, json};

pub struct CryptoCom<const TESTNET: bool = false>;
const KEY: &str = "synthetic-primary-api-key";
const OTHER_KEY: &str = "synthetic-secondary-api-key";
const SECRET: &str = "synthetic-primary-material";
const OTHER_SECRET: &str = "synthetic-secondary-material";
const READS: [&str; 4] = [
    "private/user-balance",
    "private/get-positions",
    "private/get-open-orders",
    "private/advanced/get-open-orders",
];

fn valid(_request: &Value) -> bool {
    // RED checkpoint intentionally leaves the fixture authentication unchecked.
    true
}
fn response(request: &Value, code: i64) -> Value {
    json!({"id":request["id"],"method":request["method"],"code":code})
}
fn socket_reply(request: &Value, authenticated: bool, seen: &mut Observed) -> Value {
    match request["method"].as_str().unwrap() {
        "subscribe" | "unsubscribe" => {
            let channels = request["params"]["channels"]
                .as_array()
                .expect("native channels");
            assert!(!channels.is_empty());
            let mut private = false;
            for channel in channels {
                let channel = channel.as_str().unwrap();
                if [
                    "user.order",
                    "user.advance.order",
                    "user.positions",
                    "user.balance",
                    "user.trade",
                ]
                .contains(&channel)
                {
                    private = true;
                } else if !channel.starts_with("ticker.") && !channel.starts_with("book.") {
                    seen.writes += 1;
                    panic!("unhandled Crypto.com channel: {channel}");
                }
            }
            if private && !authenticated {
                seen.rejections += 1;
                response(request, 40101)
            } else {
                response(request, 0)
            }
        }
        "public/auth" => {
            seen.rejections += 1;
            response(request, 40101)
        }
        other => {
            seen.writes += 1;
            panic!("join attempted an unknown Crypto.com socket operation: {other}");
        }
    }
}
impl<const TESTNET: bool> Venue for CryptoCom<TESTNET> {
    const NAME: &'static str = "cryptocom";
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
        frame["d"]["wsAuth"]["toSign"]
            .as_str()
            .expect("native Crypto.com signing bytes")
            .starts_with("private/")
    }
    fn signature(frame: &Value, other: bool) -> (String, Vec<u8>, Value) {
        let text = frame["d"]["wsAuth"]["toSign"]
            .as_str()
            .expect("native Crypto.com signing bytes");
        let method = text.split(|c: char| c.is_ascii_digit()).next().unwrap();
        assert!(
            method == "public/auth" || READS.contains(&method),
            "only closed Crypto.com startup reads/auth may sign"
        );
        let signature = hmac_sha256(
            if other { OTHER_SECRET } else { SECRET }.as_bytes(),
            text.as_bytes(),
            Encoding::Hex,
        );
        (
            method.into(),
            text.as_bytes().to_vec(),
            json!({"id":frame["d"]["id"],"wsAuthSignature":signature}),
        )
    }
    fn http(
        method: &Method,
        uri: &Uri,
        _headers: &HeaderMap,
        body: &Value,
        seen: &mut Observed,
    ) -> Value {
        let path = if let Some(target) = uri.path().strip_prefix("/https://") {
            let slash = target.find('/').expect("forwarded Crypto.com path");
            assert_eq!(
                &target[..slash],
                if TESTNET {
                    "uat-api.3ona.co"
                } else {
                    "api.crypto.com"
                },
                "wrong Crypto.com origin"
            );
            &target[slash..]
        } else {
            uri.path()
        };
        let route = path
            .strip_prefix("/exchange/v1/")
            .expect("native Crypto.com API prefix");
        assert!(uri.query().is_none(), "unexpected Crypto.com read query");
        let fixtures: Value =
            serde_json::from_str(include_str!("../fixtures/cryptocom-admission-reads.json"))
                .unwrap();
        if matches!(route, "public/get-instruments" | "public/get-tickers") {
            assert_eq!(method, Method::GET);
            assert!(body.is_null());
            return fixtures[route].clone();
        }
        if method != Method::POST
            || !READS.contains(&route)
            || body["method"] != route
            || body["params"] != json!({})
        {
            seen.writes += 1;
            panic!("join attempted a Crypto.com non-read: {route}");
        }
        seen.signed_reads += 1;
        if !valid(body) {
            seen.rejections += 1;
            return response(body, 40101);
        }
        let mut reply = fixtures[route].clone();
        reply["id"] = body["id"].clone();
        reply["method"] = route.into();
        reply
    }
    fn http_status(reply: &Value) -> StatusCode {
        if reply["code"] == 40101 {
            StatusCode::UNAUTHORIZED
        } else {
            StatusCode::OK
        }
    }
    fn websocket(request: &Value, seen: &mut Observed) -> Value {
        socket_reply(request, false, seen)
    }
    fn websocket_session() -> WebsocketSession {
        let mut authenticated = false;
        Box::new(move |text, seen| {
            let request: Value = serde_json::from_str(text).expect("native Crypto.com socket JSON");
            let reply = match request["method"].as_str().unwrap() {
                "public/respond-heartbeat" => return Vec::new(),
                "public/auth" => {
                    authenticated = valid(&request);
                    if !authenticated {
                        seen.rejections += 1;
                    }
                    response(&request, if authenticated { 0 } else { 40101 })
                }
                _ => socket_reply(&request, authenticated, seen),
            };
            vec![reply.to_string()]
        })
    }
}

#[cfg(test)]
mod controls {
    use super::*;
    fn signed(method: &str, key: &str, secret: &str, nonce: u64) -> Value {
        let mut frame = json!({"id":nonce,"method":method,"api_key":key,"nonce":nonce,"sig":hmac_sha256(secret.as_bytes(), format!("{method}{nonce}{key}{nonce}").as_bytes(), Encoding::Hex)});
        if method != "public/auth" {
            frame["params"] = json!({});
        }
        frame
    }
    fn http(frame: &Value) -> Value {
        CryptoCom::<false>::http(
            &Method::POST,
            &format!("/exchange/v1/{}", frame["method"].as_str().unwrap())
                .parse()
                .unwrap(),
            &HeaderMap::new(),
            frame,
            &mut Observed::default(),
        )
    }
    #[test]
    fn http_both_genuine_pairs_pass() {
        for method in READS {
            for (key, secret) in [(KEY, SECRET), (OTHER_KEY, OTHER_SECRET)] {
                let frame = signed(method, key, secret, now_ms());
                assert_eq!(http(&frame)["code"], 0);
                assert_eq!(http(&frame)["id"], frame["id"]);
            }
        }
    }
    #[test]
    fn http_wrong_material_refuses() {
        for method in READS {
            assert_eq!(
                http(&signed(method, KEY, OTHER_SECRET, now_ms()))["code"],
                40101
            );
        }
    }
    #[test]
    fn http_unknown_key_or_missing_auth_fields_refuse() {
        for method in READS {
            assert_eq!(
                http(&signed(method, "synthetic-unknown-key", SECRET, now_ms()))["code"],
                40101
            );
            for field in ["id", "api_key", "nonce", "sig"] {
                let mut frame = signed(method, KEY, SECRET, now_ms());
                frame.as_object_mut().unwrap().remove(field);
                assert_eq!(http(&frame)["code"], 40101);
            }
        }
    }
    #[test]
    fn http_stale_nonce_or_mismatched_id_refuses() {
        for method in READS {
            assert_eq!(http(&signed(method, KEY, SECRET, 1))["code"], 40101);
            let mut frame = signed(method, KEY, SECRET, now_ms());
            frame["id"] = json!(1);
            assert_eq!(http(&frame)["code"], 40101);
        }
    }
}
