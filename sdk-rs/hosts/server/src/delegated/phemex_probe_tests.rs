//! Phemex native startup: independently verified REST and private socket HMACs.
use super::*;

fn material(key: &str) -> Option<&'static str> {
    match key {
        KEY => Some(SECRET),
        "synthetic-other-key" => Some("synthetic-other-material"),
        _ => None,
    }
}

fn valid_expiry(expiry: &str) -> bool {
    let now = crate::accounts::now_ms() / 1000;
    expiry
        .parse::<u64>()
        .is_ok_and(|expiry| expiry > now && expiry <= now + 65)
}

pub(super) fn response(io: &VenueIo, request: &HttpRequest) -> Value {
    assert_eq!(request.method, Method::Get, "proof never writes");
    assert!(request.body.is_none());
    let route = path(request).split('?').next().unwrap();
    assert!(request.url.starts_with("https://api.phemex.com/"));
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/phemex-admission-reads.json"
    ))
    .unwrap();
    match route {
        "/public/products" => fixtures["products"].clone(),
        "/md/v3/ticker/24hr/all" => fixtures["tickers"].clone(),
        "/g-accounts/accountPositions" => {
            let query = request.url.split_once('?').unwrap().1;
            let currency = match query {
                "currency=USDT" => "USDT",
                "currency=USDC" => "USDC",
                _ => panic!("unexpected private read scope: {query}"),
            };
            let key = header(request, "X-PHEMEX-ACCESS-TOKEN").unwrap_or("");
            let expiry = header(request, "X-PHEMEX-REQUEST-EXPIRY").unwrap_or("");
            let signature = header(request, "X-PHEMEX-REQUEST-SIGNATURE").unwrap_or("");
            let valid = valid_expiry(expiry)
                && material(key).is_some_and(|secret| {
                    signature
                        == hmac_sha256(
                            secret.as_bytes(),
                            format!("{route}{query}{expiry}").as_bytes(),
                            Encoding::Hex,
                        )
                });
            let mut seen = io.seen.lock().unwrap();
            seen.signed_reads += 1;
            if !valid {
                seen.refused += 1;
                return json!({"code":401,"msg":"synthetic credential refusal"});
            }
            fixtures[currency].clone()
        }
        _ => panic!("unexpected Phemex startup route: {route}"),
    }
}

pub(super) fn connect(
    io: &VenueIo,
    socket: SocketId,
    url: &str,
    events: Events,
) -> mpsc::UnboundedSender<String> {
    assert!(
        url.starts_with("wss://ws.phemex.com"),
        "native socket origin: {url}"
    );
    let seen = io.seen.clone();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    events.send(HostEvent::WsOpened(socket)).unwrap();
    tokio::task::spawn_local(async move {
        let mut authenticated = false;
        while let Some(text) = rx.recv().await {
            let frame: Value = serde_json::from_str(&text).unwrap();
            let reply = match frame["method"].as_str().unwrap() {
                "user.auth" => {
                    assert_eq!(frame["params"][0], "API");
                    let key = frame["params"][1].as_str().unwrap();
                    let signature = frame["params"][2].as_str().unwrap();
                    let expiry = frame["params"][3].as_u64().unwrap().to_string();
                    authenticated = valid_expiry(&expiry)
                        && material(key).is_some_and(|secret| {
                            signature
                                == hmac_sha256(
                                    secret.as_bytes(),
                                    format!("{key}{expiry}").as_bytes(),
                                    Encoding::Hex,
                                )
                        });
                    if authenticated {
                        seen.lock().unwrap().phemex_socket_auths += 1;
                        json!({"id":frame["id"],"result":{"status":"success"}})
                    } else {
                        json!({"id":frame["id"],"error":{"code":401,"message":"synthetic credential refusal"}})
                    }
                }
                "aop_p.subscribe" => {
                    assert!(
                        authenticated,
                        "private snapshot requires real socket authentication"
                    );
                    events
                        .send(HostEvent::WsMessage(
                            socket,
                            json!({"id":frame["id"],"result":{"status":"success"}}).to_string(),
                        ))
                        .unwrap();
                    json!({"type":"snapshot","sequence":1,"orders_p":[]})
                }
                "server.ping" => json!({"id":frame["id"],"result":"pong"}),
                "perp_market24h_pack_p.subscribe" => {
                    json!({"id":frame["id"],"result":{"status":"success"}})
                }
                method => panic!("admission never writes over websocket: {method}"),
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
async fn phemex_native_private_reads_and_socket_require_the_original_key() {
    tokio::task::LocalSet::new()
        .run_until(async {
            assert_eq!(
                safe_cex_native::resolve::rest_base_url("phemex", false).unwrap(),
                "https://api.phemex.com"
            );
            assert_eq!(
                safe_cex_native::resolve::rest_base_url("phemex", true).unwrap(),
                "https://testnet-api.phemex.com"
            );
            for (secret, accepted) in [(SECRET, true), ("synthetic-other-material", false)] {
                let (result, seen) = check("phemex", local("phemex", secret)).await;
                let seen = seen.lock().unwrap();
                assert_eq!(
                    result.is_ok(),
                    accepted,
                    "{result:?}; paths={:?}",
                    seen.paths
                );
                assert!(
                    seen.signed_reads >= 2,
                    "both currency reads must really be signed"
                );
                if accepted {
                    assert!(seen.phemex_socket_auths > 0);
                    assert_eq!(seen.refused, 0);
                } else {
                    assert!(matches!(
                        result,
                        Err(ProbeRefusal {
                            transient: false,
                            ..
                        })
                    ));
                    assert!(seen.refused > 0);
                    assert_eq!(seen.phemex_socket_auths, 0);
                }
            }
        })
        .await;
}

#[tokio::test]
async fn phemex_native_remote_proof_refuses_wrong_material_replay_and_foreign_socket() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let prior = Arc::new(Mutex::new(HashMap::new()));
            for answer in [
                DeviceAnswer::Correct,
                DeviceAnswer::DifferentKey,
                DeviceAnswer::Replay,
            ] {
                if matches!(answer, DeviceAnswer::Replay) {
                    // Expiry is in whole seconds; never pretend identical bytes are a stale proof.
                    tokio::time::sleep(Duration::from_millis(1100)).await;
                }
                let (prepared, link, task) = remote("phemex", answer, prior.clone()).await;
                let (result, seen) = check("phemex", prepared).await;
                {
                    let seen = seen.lock().unwrap();
                    assert!(seen.signed_reads > 0);
                    if matches!(answer, DeviceAnswer::Correct) {
                        assert!(result.is_ok(), "{result:?}; paths={:?}", seen.paths);
                        assert!(seen.phemex_socket_auths > 0);
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
