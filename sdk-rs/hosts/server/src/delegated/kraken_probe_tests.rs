//! Synthetic Futures venue checks exact native REST and socket signing bytes.
use super::*;
use safe_cex_engine::signing::{Algorithm, Operation, PayloadPart, SignRequest, Signer};
use std::sync::atomic::{AtomicU64, Ordering};

fn material(key: &str) -> Option<&'static str> {
    match key {
        KEY => Some(SECRET),
        "synthetic-other-key" => Some("synthetic-other-material"),
        _ => None,
    }
}

fn signature(material: &str, payload: &[u8]) -> String {
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    let (signer, ids) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("synthetic-fixture".into()),
            secret: Secret::new(material.into()),
            ..Default::default()
        }),
        secondary: None,
    })
    .unwrap();
    let result = signer
        .sign_ready(&SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Sign {
                algorithm: Algorithm::HmacSha512,
                key: ids.primary.as_ref().unwrap().signing_ref().unwrap().clone(),
                payload: vec![PayloadPart::Bytes {
                    bytes: safe_cex_crypto::sha256(payload).to_vec(),
                }],
            }],
        })
        .unwrap()
        .unwrap();
    safe_cex_crypto::encode(&result.values[0], Encoding::Base64)
}

pub(crate) fn socket_preimage(descriptor: &str) -> bool {
    serde_json::from_str::<Value>(descriptor).unwrap()["payload"]
        .as_str()
        .unwrap()
        .starts_with("synthetic-kraken-challenge-")
}

pub(super) fn response(io: &VenueIo, request: &HttpRequest) -> Value {
    assert_eq!(request.method, Method::Get, "admission never writes");
    assert!(request.body.is_none());
    assert!(request.url.starts_with("https://futures.kraken.com/"));
    let route = path(request).split('?').next().unwrap();
    let private = match route {
        "/derivatives/api/v3/instruments" | "/derivatives/api/v3/tickers" => false,
        "/derivatives/api/v3/accounts"
        | "/derivatives/api/v3/openorders"
        | "/derivatives/api/v3/openpositions"
        | "/derivatives/api/v3/leveragepreferences" => true,
        _ => panic!("unexpected Kraken startup route: {route}"),
    };
    if private {
        let query = request.url.split_once('?').map_or("", |(_, q)| q);
        let nonce = header(request, "Nonce").unwrap_or("");
        let canonical = format!(
            "{query}{nonce}{}",
            route.strip_prefix("/derivatives").unwrap()
        );
        let authent = header(request, "Authent").unwrap_or("");
        let valid = material(header(request, "APIKey").unwrap_or(""))
            .is_some_and(|secret| authent == signature(secret, canonical.as_bytes()));
        let mut seen = io.seen.lock().unwrap();
        seen.signed_reads += 1;
        seen.kraken_authents.push((route.into(), authent.into()));
        if !valid {
            seen.refused += 1;
            return json!({"result":"error","error":"authenticationError"});
        }
    }
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/kraken-admission-reads.json"
    ))
    .unwrap();
    fixtures[route].clone()
}

pub(super) fn connect(
    io: &VenueIo,
    socket: SocketId,
    url: &str,
    events: Events,
) -> mpsc::UnboundedSender<String> {
    assert_eq!(url, "wss://futures.kraken.com/ws/v1");
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let challenge = format!(
        "synthetic-kraken-challenge-{}",
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let seen = io.seen.clone();
    let gate = seen.lock().unwrap().kraken_auth_gate.clone();
    events.send(HostEvent::WsOpened(socket)).unwrap();
    tokio::task::spawn_local(async move {
        let mut challenge_key: Option<String> = None;
        while let Some(text) = rx.recv().await {
            let frame: Value = serde_json::from_str(&text).unwrap();
            let reply = match frame["event"].as_str().unwrap() {
                "challenge" => {
                    challenge_key = frame["api_key"].as_str().map(str::to_owned);
                    json!({"event":"challenge","message":challenge})
                }
                "subscribe" => {
                    let feed = frame["feed"].as_str().unwrap();
                    if matches!(feed, "fills" | "open_orders" | "open_positions") {
                        let valid = challenge_key.is_some()
                            && frame["api_key"].as_str() == challenge_key.as_deref()
                            && frame["original_challenge"] == challenge
                            && material(frame["api_key"].as_str().unwrap_or("")).is_some_and(
                                |secret| {
                                    frame["signed_challenge"]
                                        == signature(secret, challenge.as_bytes())
                                },
                            );
                        if !valid {
                            seen.lock().unwrap().refused += 1;
                            json!({"event":"error","message":"Invalid challenge"})
                        } else {
                            {
                                let mut seen = seen.lock().unwrap();
                                seen.kraken_socket_auths += 1;
                                seen.kraken_challenges.push((
                                    challenge.clone(),
                                    frame["signed_challenge"].as_str().unwrap().into(),
                                ));
                            }
                            if let Some(gate) = &gate {
                                gate.acquire().await.unwrap().forget();
                            }
                            json!({"event":"subscribed","feed":feed})
                        }
                    } else {
                        assert!(matches!(feed, "heartbeat" | "ticker" | "ticker_lite"));
                        json!({"event":"subscribed","feed":feed})
                    }
                }
                event => panic!("admission never writes over websocket: {event}"),
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
async fn kraken_fixture_binds_subscriptions_to_the_challenge_request_key() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for (requested, subscribed, expected) in [
                (KEY, KEY, "subscribed"),
                ("synthetic-other-key", "synthetic-other-key", "subscribed"),
                (KEY, "synthetic-other-key", "error"),
            ] {
                let io = VenueIo {
                    venue: "kraken",
                    seen: Arc::new(Mutex::new(Seen::default())),
                };
                let (events, mut received) = mpsc::unbounded_channel();
                let socket = SocketId(77);
                let control = connect(&io, socket, "wss://futures.kraken.com/ws/v1", events);
                assert!(
                    matches!(received.recv().await, Some(HostEvent::WsOpened(id)) if id == socket)
                );
                control
                    .send(json!({"event":"challenge","api_key":requested}).to_string())
                    .unwrap();
                let event = tokio::time::timeout(Duration::from_secs(3), received.recv())
                    .await
                    .unwrap()
                    .unwrap();
                let HostEvent::WsMessage(id, text) = event else {
                    panic!("expected challenge reply")
                };
                assert_eq!(id, socket);
                let reply: Value = serde_json::from_str(&text).unwrap();
                let challenge = reply["message"].as_str().unwrap();
                let signed = signature(material(subscribed).unwrap(), challenge.as_bytes());
                control
                    .send(
                        json!({"event":"subscribe","feed":"fills","api_key":subscribed,
                "original_challenge":challenge,"signed_challenge":signed})
                        .to_string(),
                    )
                    .unwrap();
                let event = tokio::time::timeout(Duration::from_secs(3), received.recv())
                    .await
                    .unwrap()
                    .unwrap();
                let HostEvent::WsMessage(id, text) = event else {
                    panic!("expected subscription reply")
                };
                assert_eq!(id, socket);
                let reply: Value = serde_json::from_str(&text).unwrap();
                assert_eq!(
                    reply["event"], expected,
                    "a valid MAC cannot change the challenge's key"
                );
                let seen = io.seen.lock().unwrap();
                assert_eq!(seen.refused, usize::from(expected == "error"));
                assert_eq!(
                    seen.kraken_socket_auths,
                    usize::from(expected == "subscribed")
                );
            }
        })
        .await;
}

#[tokio::test]
async fn kraken_loaded_rest_waits_for_the_private_challenge_acknowledgments() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let gate = Arc::new(tokio::sync::Semaphore::new(0));
            let seen = Arc::new(Mutex::new(Seen {
                kraken_auth_gate: Some(gate.clone()),
                ..Default::default()
            }));
            let prepared = local("kraken", SECRET);
            let io = VenueIo {
                venue: "kraken",
                seen: seen.clone(),
            };
            let mut pending = tokio::task::spawn_local(async move {
                run(
                    "kraken",
                    &prepared.options,
                    prepared.resident.provider(),
                    io,
                    ServerConfig::default(),
                    Arc::new(|| true),
                )
                .await
            });
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    let waiting = {
                        let seen = seen.lock().unwrap();
                        seen.signed_reads >= 3 && seen.kraken_socket_auths > 0
                    };
                    if waiting {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("native reads and the signed private handshake reached the fixture");
            assert!(
                tokio::time::timeout(Duration::from_millis(200), &mut pending)
                    .await
                    .is_err(),
                "REST loading alone must not admit before fresh private authentication"
            );
            for acknowledged in 1..3 {
                gate.add_permits(1);
                tokio::time::timeout(Duration::from_secs(3), async {
                    while seen.lock().unwrap().kraken_socket_auths < acknowledged + 1 {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("the next private feed reached its ACK gate");
                assert!(
                    tokio::time::timeout(Duration::from_millis(200), &mut pending)
                        .await
                        .is_err(),
                    "only {acknowledged} private acknowledgments cannot admit the account"
                );
            }
            gate.add_permits(1);
            assert!(
                tokio::time::timeout(Duration::from_secs(3), pending)
                    .await
                    .unwrap()
                    .unwrap()
                    .is_ok()
            );
            assert_eq!(seen.lock().unwrap().kraken_socket_auths, 3);
        })
        .await;
}

#[tokio::test]
async fn kraken_native_private_startup_checks_primary_material() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for (secret, accepted) in [(SECRET, true), ("synthetic-other-material", false)] {
                let (result, seen) = check("kraken", local("kraken", secret)).await;
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
                    for route in ["accounts", "openorders", "openpositions"] {
                        assert!(seen.paths.iter().any(|p| p.ends_with(route)), "{route}");
                    }
                } else {
                    assert!(seen.refused > 0);
                    assert!(matches!(
                        result,
                        Err(ProbeRefusal {
                            transient: false,
                            ..
                        })
                    ));
                }
            }
        })
        .await;
}

#[tokio::test]
async fn kraken_two_private_startup_attempts_require_different_signed_bytes() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (first, first_seen) = check("kraken", local("kraken", SECRET)).await;
            let (second, second_seen) = check("kraken", local("kraken", SECRET)).await;
            assert!(first.is_ok() && second.is_ok(), "positive startup controls");
            let first = first_seen.lock().unwrap();
            let second = second_seen.lock().unwrap();
            let route = "/derivatives/api/v3/accounts";
            let authent = |seen: &Seen| {
                seen.kraken_authents
                    .iter()
                    .find(|(p, _)| p == route)
                    .unwrap()
                    .1
                    .clone()
            };
            assert_eq!(authent(&first), authent(&second), "REST remains nonce-free");
            assert_eq!(first.kraken_socket_auths, 3);
            assert_eq!(second.kraken_socket_auths, 3);
            let (first_challenge, first_signature) = &first.kraken_challenges[0];
            let (second_challenge, second_signature) = &second.kraken_challenges[0];
            assert_ne!(first_challenge, second_challenge);
            assert_ne!(
                first_signature, second_signature,
                "fresh proofs must not reuse signed private challenge bytes"
            );
        })
        .await;
}

#[tokio::test]
async fn kraken_native_remote_startup_checks_primary_batch_and_socket_binding() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let prior = Arc::new(Mutex::new(HashMap::new()));
            for answer in [
                DeviceAnswer::Correct,
                DeviceAnswer::DifferentKey,
                DeviceAnswer::Replay,
            ] {
                let (prepared, link, task) = remote("kraken", answer, prior.clone()).await;
                let (result, seen) = check("kraken", prepared).await;
                {
                    let seen = seen.lock().unwrap();
                    assert!(seen.signed_reads > 0);
                    assert_eq!(
                        result.is_ok(),
                        matches!(answer, DeviceAnswer::Correct),
                        "{result:?}"
                    );
                    if matches!(answer, DeviceAnswer::Correct) {
                        assert_eq!(seen.refused, 0);
                    } else {
                        assert!(seen.refused > 0);
                        assert!(matches!(
                            result,
                            Err(ProbeRefusal {
                                transient: false,
                                ..
                            })
                        ));
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
fn kraken_fixture_binds_key_nonce_and_exact_primary_read() {
    let io = VenueIo {
        venue: "kraken",
        seen: Arc::default(),
    };
    let mut request = HttpRequest {
        id: safe_cex_engine::RequestId(1),
        method: Method::Get,
        route: None,
        timeout_ms: None,
        url: "https://futures.kraken.com/derivatives/api/v3/accounts".into(),
        headers: vec![("APIKey", KEY.into())],
        signer_headers: vec![],
        body: None,
    };
    assert_eq!(response(&io, &request)["result"], "error");
    request
        .headers
        .push(("Authent", signature(SECRET, b"/api/v3/accounts")));
    assert_eq!(response(&io, &request)["result"], "success");
    request.headers[0].1 = "synthetic-other-key".into();
    assert_eq!(response(&io, &request)["result"], "error");
    request.headers[1].1 = signature("synthetic-other-material", b"/api/v3/accounts");
    assert_eq!(
        response(&io, &request)["result"],
        "success",
        "other credential is valid only for its own key"
    );
    request.headers.push(("Nonce", "123".into()));
    assert_eq!(response(&io, &request)["result"], "error");
    request.headers[1].1 = signature("synthetic-other-material", b"123/api/v3/accounts");
    assert_eq!(response(&io, &request)["result"], "success");
    request.headers[2].1 = "124".into();
    assert_eq!(
        response(&io, &request)["result"],
        "error",
        "old signature cannot satisfy a new nonce"
    );
}
