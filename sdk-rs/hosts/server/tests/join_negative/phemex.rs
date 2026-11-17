//! Independent synthetic Phemex private-read and per-socket authentication fixture.
use super::join_negative::{ACCOUNT, Observed, Venue, WebsocketSession};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::signing::Algorithm;
use safe_cex_exchange_server::{accounts::now_ms, envelope::InitPayload};
use serde_json::{Value, json};
use std::time::Duration;

pub struct Phemex<const TESTNET: bool = false>;
const KEY: &str = "synthetic-primary-api-key";
const OTHER_KEY: &str = "synthetic-secondary-api-key";
const SECRET: &str = "synthetic-primary-material";
const OTHER_SECRET: &str = "synthetic-secondary-material";

// Red checkpoint: the negative controls must detect these unchecked verifiers.
fn valid_read(_path: &str, _query: Option<&str>, _headers: &HeaderMap) -> bool {
    true
}
fn valid_auth(_request: &Value) -> bool {
    true
}
fn refusal(id: &Value, seen: &mut Observed) -> Value {
    seen.rejections += 1;
    json!({"id":id,"error":{"code":401,"message":"synthetic credential refusal"}})
}
fn ack(id: &Value) -> Value {
    json!({"id":id,"result":{"status":"success"}})
}

impl<const TESTNET: bool> Venue for Phemex<TESTNET> {
    const NAME: &'static str = "phemex";
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
    fn replay_delay() -> Duration {
        Duration::from_millis(1100)
    }
    fn replayable(frame: &Value) -> bool {
        frame["d"]["wsAuth"]["toSign"]
            .as_str()
            .expect("native Phemex signing bytes")
            .starts_with('/')
    }
    fn signature(frame: &Value, other: bool) -> (String, Vec<u8>, Value) {
        let preimage = frame["d"]["wsAuth"]["toSign"]
            .as_str()
            .expect("native Phemex signing bytes");
        assert!(
            preimage.starts_with("/g-accounts/accountPositionscurrency=")
                || preimage.starts_with("/public/products")
                || preimage.starts_with("/md/v3/ticker/24hr/all")
                || preimage.starts_with(KEY),
            "only native Phemex private reads or login may sign"
        );
        let signature = hmac_sha256(
            if other { OTHER_SECRET } else { SECRET }.as_bytes(),
            preimage.as_bytes(),
            Encoding::Hex,
        );
        (
            preimage
                .trim_end_matches(|c: char| c.is_ascii_digit())
                .into(),
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
        if method != Method::GET || !body.is_null() {
            seen.writes += 1;
            panic!("join attempted a non-read Phemex request");
        }
        // Native proxy routes preserve the upstream URL in the loopback path.
        // Strip only that envelope and keep the original signed query unchanged.
        let path = if let Some(target) = uri.path().strip_prefix("/https://") {
            let slash = target.find('/').expect("forwarded Phemex path");
            assert_eq!(
                &target[..slash],
                if TESTNET {
                    "testnet-api.phemex.com"
                } else {
                    "api.phemex.com"
                },
                "wrong forwarded Phemex origin"
            );
            &target[slash..]
        } else {
            uri.path()
        };
        let fixtures: Value =
            serde_json::from_str(include_str!("../fixtures/phemex-admission-reads.json")).unwrap();
        match path {
            "/public/products" => fixtures["products"].clone(),
            "/md/v3/ticker/24hr/all" => fixtures["tickers"].clone(),
            "/g-accounts/accountPositions" => {
                let currency = match uri.query() {
                    Some("currency=USDT") => "USDT",
                    Some("currency=USDC") => "USDC",
                    other => panic!("unexpected Phemex private read scope: {other:?}"),
                };
                seen.signed_reads += 1;
                if !valid_read(path, uri.query(), headers) {
                    seen.rejections += 1;
                    return json!({"code":401,"msg":"synthetic credential refusal"});
                }
                fixtures[currency].clone()
            }
            other => {
                seen.writes += 1;
                panic!("unhandled Phemex operation: {other}");
            }
        }
    }
    fn http_status(reply: &Value) -> StatusCode {
        if reply["code"] == 401 {
            StatusCode::UNAUTHORIZED
        } else {
            StatusCode::OK
        }
    }
    fn websocket(request: &Value, seen: &mut Observed) -> Value {
        match request["method"].as_str().unwrap() {
            "server.ping" => json!({"id":request["id"],"result":"pong"}),
            "perp_market24h_pack_p.subscribe" | "perp_market24h_pack_p.unsubscribe" => {
                ack(&request["id"])
            }
            "user.auth" | "aop_p.subscribe" | "aop_p.unsubscribe" => {
                // A stateless fallback must never expose private snapshot data.
                refusal(&request["id"], seen)
            }
            method => {
                seen.writes += 1;
                panic!("join attempted an unknown Phemex socket operation: {method}");
            }
        }
    }
    fn websocket_session() -> WebsocketSession {
        let mut authenticated = false;
        Box::new(move |text, seen| {
            let request: Value = serde_json::from_str(text).expect("native Phemex socket JSON");
            let replies = match request["method"].as_str().unwrap() {
                "user.auth" => {
                    authenticated = valid_auth(&request);
                    vec![if authenticated {
                        ack(&request["id"])
                    } else {
                        refusal(&request["id"], seen)
                    }]
                }
                "aop_p.subscribe" if authenticated => vec![
                    ack(&request["id"]),
                    json!({"type":"snapshot","sequence":1,"orders_p":[]}),
                ],
                "aop_p.unsubscribe" if authenticated => vec![ack(&request["id"])],
                "aop_p.subscribe" | "aop_p.unsubscribe" => {
                    vec![refusal(&request["id"], seen)]
                }
                _ => vec![Self::websocket(&request, seen)],
            };
            replies.into_iter().map(|reply| reply.to_string()).collect()
        })
    }
}

#[cfg(test)]
mod controls {
    use super::*;
    fn headers(key: &str, material: &str, expiry: &str) -> HeaderMap {
        let signature = hmac_sha256(
            material.as_bytes(),
            format!("/g-accounts/accountPositionscurrency=USDT{expiry}").as_bytes(),
            Encoding::Hex,
        );
        let mut headers = HeaderMap::new();
        headers.insert("x-phemex-access-token", key.parse().unwrap());
        headers.insert("x-phemex-request-expiry", expiry.parse().unwrap());
        headers.insert("x-phemex-request-signature", signature.parse().unwrap());
        headers
    }
    fn response(headers: &HeaderMap) -> Value {
        Phemex::<false>::http(
            &Method::GET,
            &"/g-accounts/accountPositions?currency=USDT"
                .parse()
                .unwrap(),
            headers,
            &Value::Null,
            &mut Observed::default(),
        )
    }
    #[test]
    fn http_both_genuine_pairs_pass() {
        let expiry = (now_ms() / 1000 + 60).to_string();
        for (key, material) in [(KEY, SECRET), (OTHER_KEY, OTHER_SECRET)] {
            assert_eq!(response(&headers(key, material, &expiry))["code"], 0);
            for testnet in [false, true] {
                let origin = if testnet {
                    "testnet-api.phemex.com"
                } else {
                    "api.phemex.com"
                };
                let uri = format!("/https://{origin}/g-accounts/accountPositions?currency=USDT")
                    .parse()
                    .unwrap();
                let response = if testnet {
                    Phemex::<true>::http(
                        &Method::GET,
                        &uri,
                        &headers(key, material, &expiry),
                        &Value::Null,
                        &mut Observed::default(),
                    )
                } else {
                    Phemex::<false>::http(
                        &Method::GET,
                        &uri,
                        &headers(key, material, &expiry),
                        &Value::Null,
                        &mut Observed::default(),
                    )
                };
                assert_eq!(response["code"], 0);
            }
        }
    }
    #[test]
    fn http_wrong_material_refuses() {
        let expiry = (now_ms() / 1000 + 60).to_string();
        assert_eq!(response(&headers(KEY, OTHER_SECRET, &expiry))["code"], 401);
    }
    #[test]
    fn http_unknown_or_duplicate_key_refuses() {
        let expiry = (now_ms() / 1000 + 60).to_string();
        assert_eq!(
            response(&headers("synthetic-unregistered-key", SECRET, &expiry))["code"],
            401
        );
        let mut duplicate = headers(KEY, SECRET, &expiry);
        duplicate.append("x-phemex-access-token", KEY.parse().unwrap());
        assert_eq!(response(&duplicate)["code"], 401);
    }
    #[test]
    fn http_expired_or_malformed_expiry_refuses() {
        for expiry in [
            "1".into(),
            (now_ms() / 1000 + 120).to_string(),
            "bad".into(),
        ] {
            assert_eq!(response(&headers(KEY, SECRET, &expiry))["code"], 401);
        }
    }
}
