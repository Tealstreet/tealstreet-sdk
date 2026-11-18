//! WOO native startup with independent REST and private socket HMAC checks.
use super::*;

fn material(key: &str) -> Option<&'static str> {
    match key {
        KEY => Some(SECRET),
        "synthetic-other-key" => Some("synthetic-other-material"),
        _ => None,
    }
}

fn fresh(stamp: &str) -> bool {
    stamp
        .parse::<u64>()
        .is_ok_and(|stamp| crate::accounts::now_ms().abs_diff(stamp) < 5_000)
}

pub(super) fn response(io: &VenueIo, request: &HttpRequest) -> Value {
    assert_eq!(request.method, Method::Get, "admission never writes");
    assert!(request.body.is_none());
    assert!(request.url.starts_with("https://api.woox.io/"));
    let route = path(request).split('?').next().unwrap();
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/woo-admission-reads.json"
    ))
    .unwrap();
    let private = match route {
        "/v1/public/info" | "/v1/public/futures" | "/v1/public/funding_rates" => false,
        "/v3/accountinfo" | "/v3/positions" | "/v1/orders" | "/v3/algo/orders" => true,
        _ => panic!("unexpected WOO startup route: {route}"),
    };
    if private {
        let stamp = header(request, "x-api-timestamp").unwrap_or("");
        let key = header(request, "x-api-key").unwrap_or("");
        let preimage = if route.starts_with("/v1/") {
            let query = request.url.split_once('?').map_or("", |(_, query)| query);
            assert_eq!(query, "page=1&status=INCOMPLETE");
            format!("{query}|{stamp}")
        } else {
            format!("{stamp}GET{}", path(request))
        };
        let valid = fresh(stamp)
            && material(key).is_some_and(|secret| {
                header(request, "x-api-signature")
                    == Some(
                        hmac_sha256(secret.as_bytes(), preimage.as_bytes(), Encoding::Hex).as_str(),
                    )
            });
        let mut seen = io.seen.lock().unwrap();
        seen.signed_reads += 1;
        if !valid {
            seen.refused += 1;
            return json!({"success":false,"code":-1002,"message":"synthetic credential refusal"});
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
    assert!(url.starts_with("wss://wss.woox.io/"));
    let private = url.contains("/private/");
    let seen = io.seen.clone();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    events.send(HostEvent::WsOpened(socket)).unwrap();
    tokio::task::spawn_local(async move {
        let mut authenticated = !private;
        while let Some(text) = rx.recv().await {
            let frame: Value = serde_json::from_str(&text).unwrap();
            let reply = match frame["event"].as_str().unwrap() {
                "auth" => {
                    assert!(private);
                    let params = &frame["params"];
                    let stamp = params["timestamp"].as_f64().unwrap().to_string();
                    authenticated = fresh(&stamp)
                        && material(params["apikey"].as_str().unwrap()).is_some_and(|secret| {
                            params["sign"]
                                == hmac_sha256(
                                    secret.as_bytes(),
                                    format!("|{stamp}").as_bytes(),
                                    Encoding::Hex,
                                )
                        });
                    if authenticated {
                        seen.lock().unwrap().woo_socket_auths += 1;
                        json!({"event":"auth","success":true})
                    } else {
                        json!({"event":"auth","success":false,"code":-1002})
                    }
                }
                "subscribe" => {
                    assert!(
                        authenticated,
                        "private subscriptions require possession proof"
                    );
                    assert!(matches!(
                        frame["topic"].as_str().unwrap(),
                        "executionreport"
                            | "algoexecutionreportv2"
                            | "tickers"
                            | "bbos"
                            | "markprices"
                    ));
                    json!({"event":"subscribe","success":true,"topic":frame["topic"]})
                }
                "ping" => json!({"event":"pong"}),
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

#[test]
fn woo_fixture_refuses_unsigned_private_reads() {
    let io = VenueIo {
        venue: "woo",
        seen: Arc::default(),
    };
    for route in [
        "/v3/accountinfo",
        "/v3/positions",
        "/v3/algo/orders?status=INCOMPLETE&page=1",
        "/v1/orders?page=1&status=INCOMPLETE",
    ] {
        let request = HttpRequest {
            id: safe_cex_engine::RequestId(1),
            method: Method::Get,
            route: None,
            timeout_ms: None,
            url: format!("https://api.woox.io{route}"),
            headers: vec![],
            signer_headers: vec![],
            body: None,
        };
        assert_eq!(response(&io, &request)["code"], -1002);
    }
    assert_eq!(io.seen.lock().unwrap().refused, 4);
}

#[tokio::test]
async fn woo_native_private_startup_checks_original_signing_material() {
    tokio::task::LocalSet::new()
        .run_until(async {
            assert_eq!(
                safe_cex_native::resolve::rest_base_url("woo", false).unwrap(),
                "https://api.woox.io"
            );
            for (secret, accepted) in [(SECRET, true), ("synthetic-other-material", false)] {
                let (result, seen) = check("woo", local("woo", secret)).await;
                let seen = seen.lock().unwrap();
                assert_eq!(
                    result.is_ok(),
                    accepted,
                    "{result:?}; paths={:?}",
                    seen.paths
                );
                assert!(seen.signed_reads > 0);
                if accepted {
                    assert!(seen.woo_socket_auths > 0);
                    assert_eq!(seen.refused, 0);
                    for route in [
                        "/v3/accountinfo",
                        "/v3/positions",
                        "/v1/orders",
                        "/v3/algo/orders",
                    ] {
                        assert!(seen.paths.iter().any(|path| path == route), "{route}");
                    }
                } else {
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
async fn woo_native_remote_proof_refuses_wrong_material_replay_and_foreign_socket() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let prior = Arc::new(Mutex::new(HashMap::new()));
            for answer in [
                DeviceAnswer::Correct,
                DeviceAnswer::DifferentKey,
                DeviceAnswer::Replay,
            ] {
                let (prepared, link, task) = remote("woo", answer, prior.clone()).await;
                let (result, seen) = check("woo", prepared).await;
                {
                    let seen = seen.lock().unwrap();
                    assert!(seen.signed_reads > 0);
                    if matches!(answer, DeviceAnswer::Correct) {
                        assert!(result.is_ok(), "{result:?}; paths={:?}", seen.paths);
                        assert!(seen.woo_socket_auths > 0);
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
