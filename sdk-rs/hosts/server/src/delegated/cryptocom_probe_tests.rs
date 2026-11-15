//! Crypto.com startup fixtures verify the signed JSON independently of the venue.
use super::*;

const READS: &[&str] = &[
    "private/get-positions",
    "private/user-balance",
    "private/get-open-orders",
    "private/advanced/get-open-orders",
];

fn valid_signature(frame: &Value) -> bool {
    let (Some(method), Some(id), Some(key), Some(nonce), Some(signature)) = (
        frame["method"].as_str(),
        frame["id"].as_u64(),
        frame["api_key"].as_str(),
        frame["nonce"].as_u64(),
        frame["sig"].as_str(),
    ) else {
        return false;
    };
    let material = match key {
        KEY => SECRET,
        "synthetic-other-key" => "synthetic-other-material",
        _ => return false,
    };
    id == nonce
        && nonce.abs_diff(crate::accounts::now_ms()) <= 60_000
        && signature
            == hmac_sha256(
                material.as_bytes(),
                format!("{method}{id}{key}{nonce}").as_bytes(),
                Encoding::Hex,
            )
}

pub(super) fn response(io: &VenueIo, request: &HttpRequest) -> Value {
    let route = path(request)
        .strip_prefix("/exchange/v1/")
        .expect("native Crypto.com API path");
    assert!(
        request
            .url
            .starts_with("https://api.crypto.com/exchange/v1/")
    );
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/cryptocom-admission-reads.json"
    ))
    .unwrap();
    assert!(
        fixtures.get(route).is_some(),
        "admission never writes: {route}"
    );
    if route.starts_with("public/") {
        assert_eq!(request.method, Method::Get);
        assert!(request.body.is_none());
    } else {
        assert_eq!(request.method, Method::Post);
        assert!(READS.contains(&route), "only private startup reads");
        let frame: Value = serde_json::from_str(request.body.as_deref().unwrap_or("{}"))
            .expect("native JSON envelope");
        assert_eq!(frame["method"], route);
        assert_eq!(frame["params"], json!({}), "no hidden write parameters");
        let mut seen = io.seen.lock().unwrap();
        seen.signed_reads += 1;
        if !valid_signature(&frame) {
            seen.refused += 1;
            return json!({"id":frame["id"],"method":route,"code":40101,"message":"synthetic credential refusal"});
        }
    }
    fixtures[route].clone()
}

pub(super) fn connect(
    io: &VenueIo,
    socket: SocketId,
    url: &str,
    events: Events,
) -> mpsc::UnboundedSender<String> {
    let private = url == "wss://stream.crypto.com/exchange/v1/user";
    assert!(private || url == "wss://stream.crypto.com/exchange/v1/market");
    let seen = io.seen.clone();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    events.send(HostEvent::WsOpened(socket)).unwrap();
    tokio::task::spawn_local(async move {
        let mut authenticated = false;
        while let Some(text) = rx.recv().await {
            let frame: Value = serde_json::from_str(&text).unwrap();
            let method = frame["method"].as_str().unwrap();
            let reply = match method {
                "public/auth" => {
                    assert!(private);
                    assert!(frame.get("params").is_none());
                    authenticated = valid_signature(&frame);
                    if authenticated {
                        seen.lock().unwrap().cryptocom_socket_auths += 1;
                    }
                    json!({"id":frame["id"],"method":method,"code":if authenticated {0} else {40101}})
                }
                "subscribe" | "unsubscribe" => {
                    for channel in frame["params"]["channels"].as_array().unwrap() {
                        let channel = channel.as_str().unwrap();
                        if private {
                            assert!(authenticated, "private feed requires possession proof");
                            assert!(
                                [
                                    "user.order",
                                    "user.advance.order",
                                    "user.positions",
                                    "user.balance",
                                    "user.trade",
                                ]
                                .contains(&channel)
                            );
                        } else {
                            assert!(channel.starts_with("ticker.") || channel.starts_with("book."));
                        }
                    }
                    json!({"id":frame["id"],"method":method,"code":0})
                }
                "public/respond-heartbeat" => continue,
                _ => panic!("admission never writes over websocket: {method}"),
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
async fn cryptocom_native_startup_checks_every_private_read_and_wrong_material() {
    tokio::task::LocalSet::new()
        .run_until(async {
            assert_eq!(
                safe_cex_native::resolve::rest_base_url("cryptocom", false).unwrap(),
                "https://api.crypto.com/exchange/v1"
            );
            for (material, accepted) in [(SECRET, true), ("synthetic-other-material", false)] {
                let (result, seen) = check("cryptocom", local("cryptocom", material)).await;
                let seen = seen.lock().unwrap();
                assert_eq!(
                    result.is_ok(),
                    accepted,
                    "{result:?}; paths={:?}",
                    seen.paths
                );
                assert!(seen.signed_reads > 0);
                if accepted {
                    for route in READS {
                        assert!(
                            seen.paths
                                .iter()
                                .any(|path| path == &format!("/exchange/v1/{route}")),
                            "missing native read {route}"
                        );
                    }
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
                }
            }
        })
        .await;
}

#[tokio::test]
async fn cryptocom_native_remote_proof_checks_different_key_replay_and_foreign_socket() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let prior = Arc::new(Mutex::new(HashMap::new()));
            for answer in [
                DeviceAnswer::Correct,
                DeviceAnswer::DifferentKey,
                DeviceAnswer::Replay,
            ] {
                let (prepared, link, task) = remote("cryptocom", answer, prior.clone()).await;
                let (result, seen) = check("cryptocom", prepared).await;
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
fn cryptocom_unsigned_and_changed_identity_envelopes_cannot_prove_a_key() {
    let nonce = crate::accounts::now_ms();
    let method = "private/user-balance";
    let valid = json!({"method":method,"id":nonce,"nonce":nonce,"api_key":KEY,"params":{},"sig":hmac_sha256(SECRET.as_bytes(),format!("{method}{nonce}{KEY}{nonce}").as_bytes(),Encoding::Hex)});
    assert!(valid_signature(&valid));
    for field in ["sig", "api_key", "nonce", "id", "method"] {
        let mut invalid = valid.clone();
        invalid.as_object_mut().unwrap().remove(field);
        assert!(!valid_signature(&invalid), "{field} required");
    }
    let mut other = valid;
    other["api_key"] = "synthetic-other-key".into();
    assert!(!valid_signature(&other));
}
