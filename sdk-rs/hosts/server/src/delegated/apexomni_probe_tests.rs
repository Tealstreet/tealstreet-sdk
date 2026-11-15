//! ApeX startup fixtures verify the native API HMAC, never OmniKey possession.
use super::*;

fn material(key: &str) -> Option<&'static str> {
    match key {
        KEY => Some(SECRET),
        "synthetic-other-key" => Some("synthetic-other-material"),
        _ => None,
    }
}

fn valid(key: &str, passphrase: &str, timestamp: &str, signature: &str, suffix: &str) -> bool {
    let now = crate::accounts::now_ms();
    passphrase == PASS
        && timestamp
            .parse::<u64>()
            .is_ok_and(|stamp| now.abs_diff(stamp) <= 5000)
        && material(key).is_some_and(|secret| {
            let key = safe_cex_crypto::encode(secret.as_bytes(), Encoding::Base64);
            signature
                == hmac_sha256(
                    key.as_bytes(),
                    format!("{timestamp}{suffix}").as_bytes(),
                    Encoding::Base64,
                )
        })
}

pub(super) fn response(io: &VenueIo, request: &HttpRequest) -> Value {
    assert_eq!(request.method, Method::Get, "startup proof never writes");
    assert!(request.body.is_none());
    assert!(
        request
            .url
            .starts_with("https://omni.apex.exchange/api/v3/")
    );
    let target = path(request);
    assert!(!target.contains('?'), "startup reads have no query scope");
    let name = target.strip_prefix("/api/v3/").unwrap();
    assert!(["symbols", "account", "account-balance", "open-orders"].contains(&name));
    let accepted = valid(
        header(request, "APEX-API-KEY").unwrap_or(""),
        header(request, "APEX-PASSPHRASE").unwrap_or(""),
        header(request, "APEX-TIMESTAMP").unwrap_or(""),
        header(request, "APEX-SIGNATURE").unwrap_or(""),
        &format!("GET{target}"),
    );
    // Symbols are signed by native startup, but public discovery is not key proof.
    if name != "symbols" {
        let mut seen = io.seen.lock().unwrap();
        seen.signed_reads += 1;
        if !accepted {
            seen.refused += 1;
        }
    }
    if !accepted && name != "symbols" {
        return json!({"code":401,"msg":"synthetic signature refusal"});
    }
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/apexomni-admission-reads.json"
    ))
    .unwrap();
    fixtures[name].clone()
}

pub(super) fn connect(
    io: &VenueIo,
    socket: SocketId,
    url: &str,
    events: Events,
) -> mpsc::UnboundedSender<String> {
    assert!(url.starts_with("wss://quote.omni.apex.exchange/realtime_"));
    let private = url.contains("/realtime_private?");
    let seen = io.seen.clone();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    events.send(HostEvent::WsOpened(socket)).unwrap();
    tokio::task::spawn_local(async move {
        let mut authenticated = false;
        while let Some(text) = rx.recv().await {
            let frame: Value = serde_json::from_str(&text).unwrap();
            let reply = match frame["op"].as_str().unwrap() {
                "login" => {
                    assert!(private);
                    let login: Value =
                        serde_json::from_str(frame["args"][0].as_str().unwrap()).unwrap();
                    assert_eq!(login["type"], "login");
                    assert_eq!(login["httpMethod"], "GET");
                    assert_eq!(login["requestPath"], "/ws/accounts");
                    authenticated = valid(
                        login["apiKey"].as_str().unwrap(),
                        login["passphrase"].as_str().unwrap(),
                        &login["timestamp"].as_u64().unwrap().to_string(),
                        login["signature"].as_str().unwrap(),
                        "GET/ws/accounts",
                    );
                    if !authenticated {
                        seen.lock().unwrap().refused += 1;
                    }
                    json!({"op":"login","success":authenticated,"ret_msg":if authenticated {""} else {"synthetic signature refusal"}})
                }
                "subscribe" => {
                    if private {
                        assert!(authenticated, "private subscriptions follow signed login");
                        assert_eq!(
                            frame["args"],
                            json!(["ws_zk_accounts_v3", "ws_user_v1", "ws_notify_v1"])
                        );
                    }
                    json!({"op":"subscribe","success":true})
                }
                "ping" => json!({"op":"pong"}),
                "pong" => continue,
                op => panic!("startup never writes over websocket: {op}"),
            };
            if events
                .send(HostEvent::WsMessage(socket, reply.to_string()))
                .is_err()
            {
                break;
            }
        }
    });
    tx
}

#[tokio::test]
async fn apexomni_batch_bridge_local_provider_authenticates_all_private_reads() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for (material, accepted) in [(SECRET, true), ("synthetic-other-material", false)] {
                let (result, seen) = check("apexomni", local("apexomni", material)).await;
                let seen = seen.lock().unwrap();
                assert_eq!(
                    result.is_ok(),
                    accepted,
                    "{result:?}; paths={:?}",
                    seen.paths
                );
                if accepted {
                    for route in [
                        "/api/v3/account",
                        "/api/v3/account-balance",
                        "/api/v3/open-orders",
                    ] {
                        assert!(
                            seen.paths.iter().any(|path| path == route),
                            "missing private read: {route}"
                        );
                    }
                    assert!(seen.signed_reads >= 3);
                    assert_eq!(seen.refused, 0);
                } else {
                    assert!(seen.refused > 0);
                }
            }
        })
        .await;
}
