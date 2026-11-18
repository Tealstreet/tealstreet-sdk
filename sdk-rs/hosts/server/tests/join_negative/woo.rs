//! Independent WOO legacy v1/v3 read and per-socket authentication fixture.
use super::join_negative::{ACCOUNT, Observed, Venue, WebsocketSession};
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::signing::Algorithm;
use safe_cex_exchange_server::{accounts::now_ms, envelope::InitPayload};
use serde_json::{Value, json};

pub struct Woo<const TESTNET: bool = false>;
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
fn single<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?.to_str().ok()?;
    values.next().is_none().then_some(value)
}
fn fresh(stamp: &str) -> bool {
    stamp
        .parse::<u64>()
        .is_ok_and(|value| stamp == value.to_string() && now_ms().abs_diff(value) < 5_000)
}
fn valid_read(path: &str, query: Option<&str>, headers: &HeaderMap) -> bool {
    (|| {
        let secret = material(single(headers, "x-api-key")?)?;
        let stamp = single(headers, "x-api-timestamp")?;
        if !fresh(stamp) {
            return None;
        }
        let target = query.map_or_else(|| path.into(), |query| format!("{path}?{query}"));
        let bytes = if path.starts_with("/v1/") {
            format!("{}|{stamp}", query.unwrap_or_default())
        } else {
            format!("{stamp}GET{target}")
        };
        Some(
            single(headers, "x-api-signature")?
                == hmac_sha256(secret.as_bytes(), bytes.as_bytes(), Encoding::Hex),
        )
    })() == Some(true)
}
fn valid_auth(request: &Value) -> bool {
    (|| {
        let params = request["params"].as_object()?;
        if request["event"] != "auth" || params.len() != 3 {
            return None;
        }
        let secret = material(params.get("apikey")?.as_str()?)?;
        let number = params.get("timestamp")?.as_f64()?;
        if !number.is_finite() || number.fract() != 0.0 {
            return None;
        }
        let stamp = format!("{number:.0}");
        if !fresh(&stamp) {
            return None;
        }
        Some(
            params.get("sign")?.as_str()?
                == hmac_sha256(
                    secret.as_bytes(),
                    format!("|{stamp}").as_bytes(),
                    Encoding::Hex,
                ),
        )
    })() == Some(true)
}
fn refusal(request: &Value, seen: &mut Observed) -> Value {
    seen.rejections += 1;
    json!({"event":request["event"],"topic":request["topic"],"success":false,"code":-1002})
}
fn ack(request: &Value) -> Value {
    json!({"event":request["event"],"topic":request["topic"],"success":true})
}

impl<const TESTNET: bool> Venue for Woo<TESTNET> {
    const NAME: &'static str = "woo";
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
            .expect("native WOO signing bytes")
            .starts_with('|')
    }
    fn signature(frame: &Value, other: bool) -> (String, Vec<u8>, Value) {
        let text = frame["d"]["wsAuth"]["toSign"]
            .as_str()
            .expect("native WOO signing bytes");
        let slot = if let Some((query, _)) = text.split_once('|') {
            assert!(
                query.is_empty() || query == "page=1&status=INCOMPLETE",
                "only WOO read or WS auth may sign"
            );
            format!("v1:{query}")
        } else {
            let target = text.trim_start_matches(|c: char| c.is_ascii_digit());
            assert!(
                matches!(
                    target,
                    "GET/v3/accountinfo"
                        | "GET/v3/positions"
                        | "GET/v3/algo/orders?status=INCOMPLETE&page=1"
                ),
                "only WOO v3 startup reads may sign: {target}"
            );
            target.into()
        };
        let signature = hmac_sha256(
            if other { OTHER_SECRET } else { SECRET }.as_bytes(),
            text.as_bytes(),
            Encoding::Hex,
        );
        (
            slot,
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
            panic!("join attempted a WOO non-read");
        }
        let path = if let Some(target) = uri.path().strip_prefix("/https://") {
            let slash = target.find('/').expect("forwarded WOO path");
            assert_eq!(
                &target[..slash],
                if TESTNET {
                    "api.staging.woox.io"
                } else {
                    "api.woox.io"
                },
                "wrong WOO upstream authority"
            );
            &target[slash..]
        } else {
            uri.path()
        };
        let private = match path {
            "/v1/public/info" | "/v1/public/futures" | "/v1/public/funding_rates" => false,
            "/v3/accountinfo" | "/v3/positions" => {
                assert!(uri.query().is_none());
                true
            }
            "/v1/orders" => {
                assert_eq!(uri.query(), Some("page=1&status=INCOMPLETE"));
                true
            }
            "/v3/algo/orders" => {
                assert_eq!(uri.query(), Some("status=INCOMPLETE&page=1"));
                true
            }
            other => {
                seen.writes += 1;
                panic!("unhandled WOO operation: {other}");
            }
        };
        if private {
            seen.signed_reads += 1;
            if !valid_read(path, uri.query(), headers) {
                seen.rejections += 1;
                return json!({"success":false,"code":-1002,"message":"synthetic credential refusal"});
            }
        }
        let fixtures: Value =
            serde_json::from_str(include_str!("../fixtures/woo-admission-reads.json")).unwrap();
        fixtures[path].clone()
    }
    fn http_status(reply: &Value) -> StatusCode {
        if reply["code"] == -1002 {
            StatusCode::UNAUTHORIZED
        } else {
            StatusCode::OK
        }
    }
    fn websocket(request: &Value, seen: &mut Observed) -> Value {
        match request["event"].as_str().unwrap() {
            "ping" => json!({"event":"pong"}),
            "pong" => json!({"event":"pong"}),
            "subscribe" | "unsubscribe"
                if matches!(
                    request["topic"].as_str(),
                    Some("tickers" | "bbos" | "markprices")
                ) =>
            {
                ack(request)
            }
            "auth" | "subscribe" | "unsubscribe" => refusal(request, seen),
            other => {
                seen.writes += 1;
                panic!("unhandled WOO socket operation: {other}");
            }
        }
    }
    fn websocket_session() -> WebsocketSession {
        let mut authenticated = false;
        Box::new(move |text, seen| {
            let request: Value = serde_json::from_str(text).expect("native WOO JSON");
            let reply = match request["event"].as_str().unwrap() {
                "auth" => {
                    authenticated = valid_auth(&request);
                    if authenticated {
                        ack(&request)
                    } else {
                        refusal(&request, seen)
                    }
                }
                "subscribe" | "unsubscribe"
                    if matches!(
                        request["topic"].as_str(),
                        Some("executionreport" | "algoexecutionreportv2")
                    ) =>
                {
                    if authenticated {
                        ack(&request)
                    } else {
                        refusal(&request, seen)
                    }
                }
                _ => Self::websocket(&request, seen),
            };
            vec![reply.to_string()]
        })
    }
}

#[cfg(test)]
mod controls {
    use super::*;
    fn headers(key: &str, secret: &str, stamp: &str, path: &str, query: Option<&str>) -> HeaderMap {
        let target = query.map_or_else(|| path.into(), |query| format!("{path}?{query}"));
        let bytes = if path.starts_with("/v1/") {
            format!("{}|{stamp}", query.unwrap_or_default())
        } else {
            format!("{stamp}GET{target}")
        };
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", key.parse().unwrap());
        headers.insert("x-api-timestamp", stamp.parse().unwrap());
        headers.insert(
            "x-api-signature",
            hmac_sha256(secret.as_bytes(), bytes.as_bytes(), Encoding::Hex)
                .parse()
                .unwrap(),
        );
        headers
    }
    fn response(headers: &HeaderMap, path: &str, query: Option<&str>) -> Value {
        let target = query.map_or_else(|| path.into(), |query| format!("{path}?{query}"));
        Woo::<false>::http(
            &Method::GET,
            &target.parse().unwrap(),
            headers,
            &Value::Null,
            &mut Observed::default(),
        )
    }
    fn routes() -> [(&'static str, Option<&'static str>); 4] {
        [
            ("/v3/accountinfo", None),
            ("/v3/positions", None),
            ("/v1/orders", Some("page=1&status=INCOMPLETE")),
            ("/v3/algo/orders", Some("status=INCOMPLETE&page=1")),
        ]
    }
    #[test]
    fn http_both_genuine_pairs_pass() {
        for (path, query) in routes() {
            for (key, secret) in [(KEY, SECRET), (OTHER_KEY, OTHER_SECRET)] {
                let headers = headers(key, secret, &now_ms().to_string(), path, query);
                assert_eq!(response(&headers, path, query)["success"], true);
            }
        }
    }
    #[test]
    fn http_wrong_material_refuses() {
        for (path, query) in routes() {
            assert_eq!(
                response(
                    &headers(KEY, OTHER_SECRET, &now_ms().to_string(), path, query),
                    path,
                    query
                )["code"],
                -1002
            );
        }
    }
    #[test]
    fn http_missing_unknown_or_duplicate_headers_refuse() {
        for (path, query) in routes() {
            let good = headers(KEY, SECRET, &now_ms().to_string(), path, query);
            assert_eq!(
                response(
                    &headers(
                        "synthetic-unregistered-key",
                        SECRET,
                        &now_ms().to_string(),
                        path,
                        query
                    ),
                    path,
                    query
                )["code"],
                -1002
            );
            for name in ["x-api-key", "x-api-timestamp", "x-api-signature"] {
                let mut missing = good.clone();
                missing.remove(name);
                assert_eq!(response(&missing, path, query)["code"], -1002);
                let mut duplicate = good.clone();
                duplicate.append(name, good[name].clone());
                assert_eq!(response(&duplicate, path, query)["code"], -1002);
            }
        }
    }
    #[test]
    fn http_stale_or_malformed_timestamp_refuses() {
        for (path, query) in routes() {
            for stamp in ["1".into(), "bad".into(), (now_ms() + 60_000).to_string()] {
                assert_eq!(
                    response(&headers(KEY, SECRET, &stamp, path, query), path, query)["code"],
                    -1002
                );
            }
        }
    }
}
