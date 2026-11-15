//! BingX perpetuals: native signed reads prove possession; a listen key alone does not.
use super::*;

const TOKEN: &str = "synthetic-bingx-listen-key";

pub(super) fn response(io: &VenueIo, request: &HttpRequest) -> Value {
    assert!(request.url.starts_with("https://open-api.bingx.com/"));
    let route = path(request).split('?').next().unwrap();
    if route == "/openApi/user/auth/userDataStream" {
        assert_eq!(request.method, Method::Post);
        assert!(matches!(
            header(request, "X-BX-APIKEY"),
            Some(KEY | "synthetic-other-key")
        ));
        // This API-key-only endpoint supplies a stream token, never possession proof.
        return json!({"listenKey":TOKEN});
    }
    assert_eq!(request.method, Method::Get, "proof never trades");
    assert!(request.body.is_none());
    let private = matches!(
        route,
        "/openApi/swap/v1/positionSide/dual"
            | "/openApi/swap/v3/user/balance"
            | "/openApi/swap/v2/user/positions"
            | "/openApi/swap/v2/trade/openOrders"
    );
    if private {
        let signed = request
            .url
            .split_once('?')
            .and_then(|(_, query)| query.rsplit_once("&signature="));
        let material = match header(request, "X-BX-APIKEY") {
            Some(KEY) => Some(SECRET),
            Some("synthetic-other-key") => Some("synthetic-other-material"),
            _ => None,
        };
        let valid = signed.is_some_and(|(payload, signature)| {
            let stamp = payload
                .split('&')
                .find_map(|part| part.strip_prefix("timestamp="))
                .and_then(|stamp| stamp.parse::<u64>().ok());
            stamp.is_some_and(|stamp| stamp.abs_diff(crate::accounts::now_ms()) <= 5000)
                && material.is_some_and(|secret| {
                    signature == hmac_sha256(secret.as_bytes(), payload.as_bytes(), Encoding::Hex)
                })
        });
        let mut seen = io.seen.lock().unwrap();
        seen.signed_reads += 1;
        if !valid {
            seen.refused += 1;
            return json!({"code":100001,"msg":"synthetic signature refusal"});
        }
    }
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/bingx-admission-reads.json"
    ))
    .unwrap();
    if route == "/openApi/swap/v2/server/time" {
        return json!({"code":0,"data":{"serverTime":crate::accounts::now_ms()}});
    }
    fixtures
        .get(route)
        .unwrap_or_else(|| panic!("unexpected BingX startup route: {route}"))
        .clone()
}

pub(super) fn connect(
    io: &VenueIo,
    socket: SocketId,
    url: &str,
    events: Events,
) -> mpsc::UnboundedSender<String> {
    let private = url == format!("wss://open-api-swap.bingx.com/swap-market?listenKey={TOKEN}");
    assert!(
        private || url == "wss://ws-market-swap.qq-os.com/ws",
        "unexpected BingX socket: {url}"
    );
    if private {
        io.seen.lock().unwrap().bingx_private_sockets += 1;
    }
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    events.send(HostEvent::WsOpened(socket)).unwrap();
    tokio::task::spawn_local(async move {
        while let Some(text) = rx.recv().await {
            if text == "Ping" {
                let _ = events.send(HostEvent::WsMessage(socket, "Pong".into()));
            } else {
                assert!(
                    !private,
                    "private stream needs no write/subscription command"
                );
                let frame: Value = serde_json::from_str(&text).unwrap();
                assert_eq!(frame["reqType"], "sub");
                let _ = events.send(HostEvent::WsMessage(
                    socket,
                    json!({"id":frame["id"],"code":0,"msg":""}).to_string(),
                ));
            }
        }
    });
    tx
}

#[tokio::test]
async fn bingx_native_private_startup_validates_key_and_mainnet_origin() {
    tokio::task::LocalSet::new()
        .run_until(async {
            assert_eq!(
                safe_cex_native::resolve::rest_base_url("bingx", false).unwrap(),
                "https://open-api.bingx.com"
            );
            assert!(safe_cex_native::resolve::rest_base_url("bingx", true).is_err());
            for (material, accepted) in [(SECRET, true), ("synthetic-other-material", false)] {
                let (result, seen) = check("bingx", local("bingx", material)).await;
                let seen = seen.lock().unwrap();
                assert_eq!(
                    result.is_ok(),
                    accepted,
                    "{result:?}; paths={:?}",
                    seen.paths
                );
                assert!(seen.signed_reads > 0);
                if accepted {
                    assert_eq!(seen.refused, 0);
                    assert!(seen.bingx_private_sockets > 0);
                } else {
                    assert!(matches!(
                        result,
                        Err(ProbeRefusal {
                            transient: false,
                            ..
                        })
                    ));
                    assert!(seen.refused > 0);
                }
            }
        })
        .await;
}

#[tokio::test]
async fn bingx_native_remote_proof_refuses_wrong_material_replay_and_foreign_socket() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let prior = Arc::new(Mutex::new(HashMap::new()));
            for answer in [
                DeviceAnswer::Correct,
                DeviceAnswer::DifferentKey,
                DeviceAnswer::Replay,
            ] {
                let (prepared, link, task) = remote("bingx", answer, prior.clone()).await;
                let (result, seen) = check("bingx", prepared).await;
                {
                    let seen = seen.lock().unwrap();
                    assert!(seen.signed_reads > 0);
                    if matches!(answer, DeviceAnswer::Correct) {
                        assert!(result.is_ok(), "{result:?}; paths={:?}", seen.paths);
                        assert_eq!(seen.refused, 0);
                    } else {
                        assert!(
                            matches!(
                                result,
                                Err(ProbeRefusal {
                                    transient: false,
                                    ..
                                })
                            ),
                            "{result:?}"
                        );
                        assert!(seen.refused > 0);
                    }
                }
                link.mark_unavailable();
                task.abort();
                if let Err(error) = task.await {
                    assert!(error.is_cancelled(), "device responder failed: {error}");
                }
            }
        })
        .await;
}

#[test]
fn bingx_listen_key_and_public_discovery_cannot_substitute_for_signed_reads() {
    let io = VenueIo {
        venue: "bingx",
        seen: Arc::default(),
    };
    let mut request = HttpRequest {
        id: safe_cex_engine::RequestId(1),
        method: Method::Post,
        route: None,
        timeout_ms: None,
        url: "https://open-api.bingx.com/openApi/user/auth/userDataStream".into(),
        headers: vec![("X-BX-APIKEY", KEY.into())],
        signer_headers: vec![],
        body: None,
    };
    assert_eq!(response(&io, &request)["listenKey"], TOKEN);
    request.method = Method::Get;
    request.url = "https://open-api.bingx.com/openApi/swap/v2/quote/contracts".into();
    assert_eq!(response(&io, &request)["code"], 0);
    assert_eq!(io.seen.lock().unwrap().signed_reads, 0);
    request.url = "https://open-api.bingx.com/openApi/swap/v3/user/balance".into();
    assert_eq!(response(&io, &request)["code"], 100001);
    let seen = io.seen.lock().unwrap();
    assert_eq!(seen.signed_reads, 1);
    assert_eq!(seen.refused, 1);
}
