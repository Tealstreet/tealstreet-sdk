//! Independent API-key proof for ApeX monitoring; not OmniKey/L2 possession.
use super::join_negative::{ACCOUNT, Observed, Venue, WebsocketSession};
use axum::http::{HeaderMap, Method, Uri};
use safe_cex_crypto::{Encoding, encode, hmac_sha256};
use safe_cex_engine::signing::Algorithm;
use safe_cex_exchange_server::{accounts::now_ms, envelope::InitPayload};
use serde_json::{Value, json};

pub struct Apex<const TESTNET: bool>;
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
fn single<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?.to_str().ok()?;
    values.next().is_none().then_some(value)
}
fn mac(secret: &str, payload: &[u8]) -> String {
    // Native ApeX uses the base64 TEXT of the raw secret as its HMAC key.
    hmac_sha256(
        encode(secret.as_bytes(), Encoding::Base64).as_bytes(),
        payload,
        Encoding::Base64,
    )
}
fn valid(key: &str, pass: &str, stamp: &str, signature: &str, suffix: &str) -> bool {
    pass == PASS
        && stamp
            .parse::<u64>()
            .is_ok_and(|value| value.abs_diff(now_ms()) <= 5_000)
        && material(key)
            .is_some_and(|secret| signature == mac(secret, format!("{stamp}{suffix}").as_bytes()))
}
fn signed_bytes(frame: &Value) -> Vec<u8> {
    assert_eq!(frame["d"]["wsAuth"]["outputFormat"], "apexomni-batch");
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Part {
        bytes: Vec<u8>,
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Batch {
        contracts: Vec<Vec<u8>>,
        payload: Vec<Part>,
    }
    let batch: Batch =
        serde_json::from_str(frame["d"]["wsAuth"]["toSign"].as_str().unwrap()).unwrap();
    assert!(
        batch.contracts.is_empty(),
        "admission never signs order contracts"
    );
    assert!(!batch.payload.is_empty());
    batch.payload.into_iter().flat_map(|p| p.bytes).collect()
}
impl<const TESTNET: bool> Venue for Apex<TESTNET> {
    const NAME: &'static str = "apexomni";
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
    fn algorithm() -> Algorithm {
        Algorithm::HmacSha256
    }
    fn credentials(other: bool) -> Value {
        if other {
            json!({"key":OTHER_KEY,"secret":OTHER_SECRET,"passphrase":PASS})
        } else {
            json!({"key":KEY,"secret":SECRET,"passphrase":PASS})
        }
    }
    fn replayable(frame: &Value) -> bool {
        // Replay private reads; answer setup/login freshly so an unrelated
        // prerequisite failure cannot masquerade as the intended REST rejection.
        let bytes = signed_bytes(frame);
        let text = std::str::from_utf8(&bytes).unwrap();
        matches!(
            text.trim_start_matches(|c: char| c.is_ascii_digit()),
            "GET/api/v3/account" | "GET/api/v3/account-balance" | "GET/api/v3/open-orders"
        )
    }
    fn signature(frame: &Value, other: bool) -> (String, Vec<u8>, Value) {
        let bytes = signed_bytes(frame);
        let text = std::str::from_utf8(&bytes).unwrap();
        let slot = text
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .to_owned();
        let answer = json!({"credential":KEY,"contracts":[],"signature":mac(if other { OTHER_SECRET } else { SECRET }, &bytes)}).to_string();
        (
            slot,
            bytes,
            json!({"id":frame["d"]["id"],"wsAuthSignature":answer}),
        )
    }
    fn http(
        method: &Method,
        uri: &Uri,
        headers: &HeaderMap,
        body: &Value,
        seen: &mut Observed,
    ) -> Value {
        if *method != Method::GET {
            seen.writes += 1;
            panic!("ApeX join attempted a trading write");
        }
        assert!(body.is_null(), "admission never sends order payloads");
        assert!(
            uri.query().is_none(),
            "these native startup reads have no query"
        );
        let path = if let Some(target) = uri.path().strip_prefix("/https://") {
            let at = target.find('/').unwrap();
            assert_eq!(
                &target[..at],
                if TESTNET {
                    "testnet.omni.apex.exchange"
                } else {
                    "omni.apex.exchange"
                }
            );
            &target[at..]
        } else {
            uri.path()
        };
        let name = path.strip_prefix("/api/v3/").expect("native ApeX API base");
        assert!(
            ["symbols", "account", "account-balance", "open-orders"].contains(&name),
            "unexpected ApeX startup read: {path}"
        );
        if name != "symbols" {
            seen.signed_reads += 1;
            let accepted = (|| {
                Some(valid(
                    single(headers, "APEX-API-KEY")?,
                    single(headers, "APEX-PASSPHRASE")?,
                    single(headers, "APEX-TIMESTAMP")?,
                    single(headers, "APEX-SIGNATURE")?,
                    &format!("{method}{path}"),
                ))
            })() == Some(true);
            if !accepted {
                seen.rejections += 1;
                return json!({"code":401,"msg":"synthetic signature refusal"});
            }
        }
        let fixtures: Value =
            serde_json::from_str(include_str!("../fixtures/apexomni-admission-reads.json"))
                .unwrap();
        fixtures[name].clone()
    }
    fn websocket(_request: &Value, _seen: &mut Observed) -> Value {
        panic!("ApeX requires a per-connection login state");
    }
    fn websocket_session() -> WebsocketSession {
        let mut authenticated = false;
        Box::new(move |text, seen| {
            let frame: Value = serde_json::from_str(text).unwrap();
            let reply = match frame["op"].as_str().unwrap() {
                "login" => {
                    let login: Value =
                        serde_json::from_str(frame["args"][0].as_str().unwrap()).unwrap();
                    assert_eq!(login["type"], "login");
                    assert_eq!(login["httpMethod"], "GET");
                    assert_eq!(login["requestPath"], "/ws/accounts");
                    seen.signed_reads += 1;
                    authenticated = valid(
                        login["apiKey"].as_str().unwrap(),
                        login["passphrase"].as_str().unwrap(),
                        &login["timestamp"].as_u64().unwrap().to_string(),
                        login["signature"].as_str().unwrap(),
                        "GET/ws/accounts",
                    );
                    if !authenticated {
                        seen.rejections += 1;
                    }
                    json!({"op":"login","success":authenticated,"ret_msg":if authenticated {""} else {"synthetic signature refusal"}})
                }
                "subscribe" | "unsubscribe" => {
                    if frame["args"] == json!(["instrumentInfo.all"]) {
                        json!({"op":frame["op"],"success":true})
                    } else {
                        assert!(
                            authenticated,
                            "private subscriptions require this socket's login"
                        );
                        assert_eq!(
                            frame["args"],
                            json!(["ws_zk_accounts_v3", "ws_user_v1", "ws_notify_v1"])
                        );
                        json!({"op":frame["op"],"success":true})
                    }
                }
                "ping" => json!({"op":"pong"}),
                "pong" => return vec![],
                other => {
                    seen.writes += 1;
                    panic!("unexpected ApeX socket operation: {other}");
                }
            };
            vec![reply.to_string()]
        })
    }
}
#[cfg(test)]
mod controls {
    use super::*;
    #[test]
    fn both_pairs_are_genuinely_valid() {
        for (key, secret) in [(KEY, SECRET), (OTHER_KEY, OTHER_SECRET)] {
            let stamp = now_ms().to_string();
            assert!(valid(
                key,
                PASS,
                &stamp,
                &mac(secret, format!("{stamp}GET/api/v3/account").as_bytes()),
                "GET/api/v3/account"
            ));
        }
    }
    #[test]
    fn wrong_material_and_passphrase_are_refused() {
        let stamp = now_ms().to_string();
        let payload = format!("{stamp}GET/api/v3/account");
        assert!(!valid(
            KEY,
            PASS,
            &stamp,
            &mac(OTHER_SECRET, payload.as_bytes()),
            "GET/api/v3/account"
        ));
        assert!(!valid(
            KEY,
            "wrong-passphrase",
            &stamp,
            &mac(SECRET, payload.as_bytes()),
            "GET/api/v3/account"
        ));
    }
    #[test]
    fn exact_method_path_and_clock_are_bound() {
        let stamp = now_ms().to_string();
        let signature = mac(SECRET, format!("{stamp}GET/api/v3/account").as_bytes());
        for suffix in ["POST/api/v3/account", "GET/api/v3/open-orders"] {
            assert!(!valid(KEY, PASS, &stamp, &signature, suffix));
        }
        for clock in [now_ms() - 30_000, now_ms() + 30_000] {
            let stamp = clock.to_string();
            assert!(!valid(
                KEY,
                PASS,
                &stamp,
                &mac(SECRET, format!("{stamp}GET/api/v3/account").as_bytes()),
                "GET/api/v3/account"
            ));
        }
    }
    #[test]
    fn private_http_requires_singular_headers() {
        let uri: Uri = "/api/v3/account".parse().unwrap();
        let stamp = now_ms().to_string();
        let mut headers = HeaderMap::new();
        for (name, value) in [
            ("APEX-API-KEY", KEY.into()),
            ("APEX-PASSPHRASE", PASS.into()),
            ("APEX-TIMESTAMP", stamp.clone()),
            (
                "APEX-SIGNATURE",
                mac(SECRET, format!("{stamp}GET/api/v3/account").as_bytes()),
            ),
        ] {
            headers.insert(name, value.parse().unwrap());
        }
        assert_eq!(
            Apex::<false>::http(
                &Method::GET,
                &uri,
                &headers,
                &Value::Null,
                &mut Observed::default()
            )["code"],
            0
        );
        headers.append("APEX-API-KEY", OTHER_KEY.parse().unwrap());
        assert_eq!(
            Apex::<false>::http(
                &Method::GET,
                &uri,
                &headers,
                &Value::Null,
                &mut Observed::default()
            )["code"],
            401
        );
        assert_eq!(
            Apex::<false>::http(
                &Method::GET,
                &uri,
                &HeaderMap::new(),
                &Value::Null,
                &mut Observed::default()
            )["code"],
            401
        );
    }
    #[test]
    fn socket_authentication_is_local_to_each_connection() {
        let mut first = Apex::<false>::websocket_session();
        let mut second = Apex::<false>::websocket_session();
        let stamp = now_ms();
        let login = json!({"op":"login","args":[json!({"type":"login","httpMethod":"GET","requestPath":"/ws/accounts","apiKey":KEY,"passphrase":PASS,"timestamp":stamp,"signature":mac(SECRET,format!("{stamp}GET/ws/accounts").as_bytes())}).to_string()]}).to_string();
        let reply: Value =
            serde_json::from_str(&first(&login, &mut Observed::default())[0]).unwrap();
        assert_eq!(reply["success"], true);
        let mut wrong: Value = serde_json::from_str(&login).unwrap();
        let mut fields: Value = serde_json::from_str(wrong["args"][0].as_str().unwrap()).unwrap();
        fields["signature"] =
            mac(OTHER_SECRET, format!("{stamp}GET/ws/accounts").as_bytes()).into();
        wrong["args"][0] = fields.to_string().into();
        let reply: Value =
            serde_json::from_str(&second(&wrong.to_string(), &mut Observed::default())[0]).unwrap();
        assert_eq!(
            reply["success"], false,
            "a different key cannot authenticate this socket"
        );
        fields["timestamp"] = (stamp + 5).into();
        fields["signature"] = mac(SECRET, format!("{stamp}GET/ws/accounts").as_bytes()).into();
        wrong["args"][0] = fields.to_string().into();
        assert_ne!(
            format!("{stamp}GET/ws/accounts"),
            format!("{}GET/ws/accounts", stamp + 5)
        );
        let reply: Value =
            serde_json::from_str(&second(&wrong.to_string(), &mut Observed::default())[0]).unwrap();
        assert_eq!(
            reply["success"], false,
            "old login MAC cannot be rebound to new bytes"
        );
        let subscription =
            json!({"op":"subscribe","args":["ws_zk_accounts_v3","ws_user_v1","ws_notify_v1"]})
                .to_string();
        first(&subscription, &mut Observed::default());
        let mut seen = Observed::default();
        let refused: Value = serde_json::from_str(&second(&subscription, &mut seen)[0]).unwrap();
        assert_eq!(refused["success"], false, "pipelined private subscriptions must return an auth refusal without panicking");
        assert_eq!(seen.writes, 0);
    }
}
