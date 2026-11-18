//! Bitunix proof uses real native private reads, verified by the synthetic venue.
use super::*;
use safe_cex_crypto::{hex, sha256};

fn valid_signature(key: &str, nonce: &str, stamp: &str, query: &str, signature: &str) -> bool {
    let material = match key {
        KEY => SECRET,
        "synthetic-other-key" => "synthetic-other-material",
        _ => return false,
    };
    let now = crate::accounts::now_ms();
    if nonce.is_empty()
        || !stamp
            .parse::<u64>()
            .is_ok_and(|stamp| stamp.abs_diff(now) <= 10_000)
    {
        return false;
    }
    let inner = hex(&sha256(format!("{nonce}{stamp}{key}{query}").as_bytes()));
    signature == hex(&sha256(format!("{inner}{material}").as_bytes()))
}

pub(super) fn response(io: &VenueIo, request: &HttpRequest) -> Value {
    assert_eq!(request.method, Method::Get, "proof never writes");
    assert!(request.body.is_none());
    assert!(request.url.starts_with("https://fapi.bitunix.com/"));
    let (route, query) = path(request).split_once('?').unwrap_or((path(request), ""));
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/bitunix-admission-reads.json"
    ))
    .unwrap();
    let fixture = fixtures.get(route).expect("only native startup reads");
    if route.starts_with("/api/v1/futures/market/") {
        return fixture.clone();
    }
    let mut pairs: Vec<_> = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| pair.split_once('=').expect("native query pair"))
        .collect();
    pairs.sort_by_key(|(key, _)| *key);
    let signed_query: String = pairs
        .iter()
        .map(|(key, value)| format!("{key}{value}"))
        .collect();
    let valid = valid_signature(
        header(request, "api-key").unwrap_or(""),
        header(request, "nonce").unwrap_or(""),
        header(request, "timestamp").unwrap_or(""),
        &signed_query,
        header(request, "sign").unwrap_or(""),
    );
    let mut seen = io.seen.lock().unwrap();
    seen.signed_reads += 1;
    if !valid {
        seen.refused += 1;
        return json!({"code":10007,"msg":"synthetic signature refusal","data":null});
    }
    fixture.clone()
}

pub(super) fn connect(
    io: &VenueIo,
    socket: SocketId,
    url: &str,
    events: Events,
) -> mpsc::UnboundedSender<String> {
    assert!(url.starts_with("wss://"));
    let private = url.contains("/private/");
    let seen = io.seen.clone();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    events.send(HostEvent::WsOpened(socket)).unwrap();
    tokio::task::spawn_local(async move {
        while let Some(text) = rx.recv().await {
            let frame: Value = serde_json::from_str(&text).unwrap();
            let reply = match frame["op"].as_str() {
                Some("login") => {
                    assert!(private);
                    let args = &frame["args"][0];
                    let valid = valid_signature(
                        args["apiKey"].as_str().unwrap_or(""),
                        args["nonce"].as_str().unwrap_or(""),
                        &args["timestamp"].to_string(),
                        "",
                        args["sign"].as_str().unwrap_or(""),
                    );
                    if valid {
                        seen.lock().unwrap().bitunix_socket_auths += 1;
                    }
                    json!({"op":"login","data":{"result":valid}})
                }
                Some("ping") => json!({"op":"pong","pong":frame["ping"]}),
                Some("subscribe") => {
                    assert!(!private);
                    json!({"op":"subscribe","code":0})
                }
                None if frame.get("ping").is_some() => json!({"pong":frame["ping"]}),
                None if frame["event"] == "sub" => {
                    assert!(!private);
                    json!({"event":"sub","code":0})
                }
                _ => panic!("admission never writes over websocket: {frame}"),
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
async fn bitunix_native_private_startup_requires_the_original_key() {
    tokio::task::LocalSet::new()
        .run_until(async {
            assert_eq!(
                safe_cex_native::resolve::rest_base_url("bitunix", false).unwrap(),
                "https://fapi.bitunix.com"
            );
            for (material, accepted) in [(SECRET, true), ("synthetic-other-material", false)] {
                let (result, seen) = check("bitunix", local("bitunix", material)).await;
                let seen = seen.lock().unwrap();
                assert_eq!(
                    result.is_ok(),
                    accepted,
                    "{result:?}; paths={:?}",
                    seen.paths
                );
                assert!(
                    seen.signed_reads > 0,
                    "a public response is not possession proof"
                );
                if accepted {
                    assert_eq!(seen.refused, 0);
                    for route in [
                        "/api/v1/futures/account",
                        "/api/v1/futures/position/get_pending_positions",
                        "/api/v1/futures/trade/get_pending_orders",
                        "/api/v1/futures/tpsl/get_pending_orders",
                    ] {
                        assert!(
                            seen.paths.iter().any(|path| path == route),
                            "missing native private read {route}"
                        );
                    }
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
async fn bitunix_native_remote_round_trip_refuses_wrong_material_replay_and_foreign_socket() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let prior = Arc::new(Mutex::new(HashMap::new()));
            for answer in [
                DeviceAnswer::Correct,
                DeviceAnswer::DifferentKey,
                DeviceAnswer::Replay,
            ] {
                let (prepared, link, task) = remote("bitunix", answer, prior.clone()).await;
                let (result, seen) = check("bitunix", prepared).await;
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
