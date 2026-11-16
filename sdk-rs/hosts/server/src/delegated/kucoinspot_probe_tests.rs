//! Exact KuCoin Spot startup through the shared probe, without venue writes.
use super::*;

pub(super) fn response(io: &VenueIo, request: &HttpRequest) -> Value {
    assert!(request.url.starts_with("https://api.kucoin.com/"));
    assert!(
        request.body.is_none(),
        "startup does not send order payloads"
    );
    let route = path(request).split('?').next().unwrap();
    let private = match path(request) {
        "/api/v2/symbols" | "/api/v1/market/allTickers" => {
            assert_eq!(request.method, Method::Get);
            false
        }
        "/api/v1/bullet-public" => {
            assert_eq!(request.method, Method::Post);
            false
        }
        "/api/v1/bullet-private" => {
            assert_eq!(request.method, Method::Post);
            true
        }
        "/api/v1/accounts?type=trade"
        | "/api/v1/hf/orders/active/symbols"
        | "/api/v1/hf/orders/active/page?symbol=BTC-USDT&pageNum=1&pageSize=50" => {
            assert_eq!(request.method, Method::Get, "private proof only reads");
            true
        }
        unexpected => panic!("unexpected KuCoin Spot startup route: {unexpected}"),
    };
    if private {
        // Verify the actual dispatch-time request MAC and separate passphrase MAC.
        let stamp = header(request, "KC-API-TIMESTAMP").and_then(|s| s.parse::<u64>().ok());
        let valid = io.valid_private_signature(request)
            && header(request, "KC-API-KEY-VERSION") == Some("2")
            && stamp.is_some_and(|s| s.abs_diff(crate::accounts::now_ms()) <= 5000);
        let mut seen = io.seen.lock().unwrap();
        seen.signed_reads += 1;
        if !valid {
            seen.refused += 1;
            return json!({"code":"400003","msg":"synthetic signature refusal","data":null});
        }
    }
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/kucoinspot-admission-reads.json"
    ))
    .unwrap();
    fixtures
        .get(route)
        .unwrap_or_else(|| panic!("missing KuCoin Spot fixture: {route}"))
        .clone()
}

pub(super) fn connect(
    socket: SocketId,
    url: &str,
    events: Events,
) -> mpsc::UnboundedSender<String> {
    let private =
        url.starts_with("wss://kucoinspot.example.test?token=synthetic-private-token&connectId=");
    assert!(
        private
            || url.starts_with(
                "wss://kucoinspot.example.test?token=synthetic-public-token&connectId=",
            )
    );
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    events.send(HostEvent::WsOpened(socket)).unwrap();
    events
        .send(HostEvent::WsMessage(
            socket,
            json!({"type":"welcome"}).to_string(),
        ))
        .unwrap();
    tokio::task::spawn_local(async move {
        while let Some(text) = rx.recv().await {
            let frame: Value = serde_json::from_str(&text).unwrap();
            let kind = match frame["type"].as_str().unwrap() {
                "ping" => "pong",
                "subscribe" | "unsubscribe" => {
                    assert_eq!(frame["privateChannel"], private);
                    let topic = frame["topic"].as_str().unwrap();
                    assert!(
                        if private {
                            matches!(topic, "/spotMarket/tradeOrdersV2" | "/account/balance")
                        } else {
                            topic == "/market/ticker:all"
                        },
                        "proof sockets carry only read feeds"
                    );
                    "ack"
                }
                unexpected => panic!("unexpected KuCoin Spot socket command: {unexpected}"),
            };
            let _ = events.send(HostEvent::WsMessage(
                socket,
                json!({"type":kind,"id":frame["id"]}).to_string(),
            ));
        }
    });
    tx
}

#[tokio::test]
async fn kucoinspot_native_private_startup_validates_key_and_spot_origin() {
    tokio::task::LocalSet::new()
        .run_until(async {
            assert_eq!(
                safe_cex_native::resolve::rest_base_url("kucoinspot", false).unwrap(),
                "https://api.kucoin.com"
            );
            assert!(safe_cex_native::resolve::rest_base_url("kucoinspot", true).is_err());
            for (material, accepted) in [(SECRET, true), ("synthetic-other-material", false)] {
                let (result, seen) = check("kucoinspot", local("kucoinspot", material)).await;
                let seen = seen.lock().unwrap();
                assert_eq!(result.is_ok(), accepted, "{result:?}; {:?}", seen.paths);
                assert!(seen.signed_reads > 0);
                if accepted {
                    assert_eq!(seen.refused, 0);
                    assert_variant_startup("kucoinspot", &seen);
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
async fn kucoinspot_remote_proof_refuses_wrong_material_replay_and_foreign_socket() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let prior = Arc::new(Mutex::new(HashMap::new()));
            for answer in [
                DeviceAnswer::Correct,
                DeviceAnswer::DifferentKey,
                DeviceAnswer::Replay,
            ] {
                let (prepared, link, task) = remote("kucoinspot", answer, prior.clone()).await;
                let capture = matches!(answer, DeviceAnswer::Correct)
                    .then(|| Arc::new(KucoinStartupCapture::spot(prior.clone())));
                let (result, seen) = check_captured("kucoinspot", prepared, capture).await;
                {
                    let seen = seen.lock().unwrap();
                    assert!(seen.signed_reads > 0);
                    if matches!(answer, DeviceAnswer::Correct) {
                        assert!(result.is_ok(), "{result:?}; {:?}", seen.paths);
                        assert_eq!(seen.refused, 0);
                        assert_variant_startup("kucoinspot", &seen);
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
fn kucoinspot_public_discovery_cannot_substitute_for_signed_account_reads() {
    let io = VenueIo {
        venue: "kucoinspot",
        seen: Arc::default(),
    };
    let mut request = HttpRequest {
        id: safe_cex_engine::RequestId(1),
        method: Method::Get,
        route: None,
        timeout_ms: None,
        url: "https://api.kucoin.com/api/v2/symbols".into(),
        headers: vec![],
        signer_headers: vec![],
        body: None,
    };
    assert_eq!(response(&io, &request)["code"], "200000");
    assert_eq!(io.seen.lock().unwrap().signed_reads, 0);
    request.url = "https://api.kucoin.com/api/v1/accounts?type=trade".into();
    assert_eq!(response(&io, &request)["code"], "400003");
    let seen = io.seen.lock().unwrap();
    assert_eq!(seen.signed_reads, 1);
    assert_eq!(seen.refused, 1);
}
