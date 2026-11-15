//! V1 proof uses signed private reads, never public discovery or a feed token alone.
use super::*;

const TOKEN: &str = "synthetic-aster-listen-key";

pub(crate) async fn device_signature(request: &Value, key: &str, material: &str) -> String {
    let mut options = serde_json::from_value(json!({"key":key,"secret":material})).unwrap();
    let (provider, ids) =
        safe_cex_venues::prepare_local_signer_for("asterdex", &mut options).unwrap();
    assert_eq!(request["wsAuth"]["outputFormat"], "asterdex-hmac");
    safe_cex_venues::hosted_wallet::sign(
        "asterdex",
        &ids,
        &Default::default(),
        &provider,
        request["wsAuth"]["toSign"].as_str().unwrap(),
        "asterdex-hmac",
        crate::accounts::now_ms() as f64,
    )
    .unwrap()
}

pub(super) fn response(io: &VenueIo, request: &HttpRequest) -> Value {
    assert!(
        request.url.starts_with("https://fapi.asterdex.com/")
            || request
                .url
                .starts_with("https://fapi.asterdex-testnet.com/")
    );
    assert!(request.body.is_none());
    let route = path(request).split('?').next().unwrap();
    let private = matches!(
        route,
        "/fapi/v4/account"
            | "/fapi/v1/positionRisk"
            | "/fapi/v1/openOrders"
            | "/fapi/v1/positionSide/dual"
            | "/fapi/v1/leverageBracket"
            | "/fapi/v1/listenKey"
    );
    assert!(
        request.method == Method::Get
            || request.method == Method::Post && route == "/fapi/v1/listenKey",
        "proof never trades"
    );
    if private {
        let material = match header(request, "X-MBX-APIKEY") {
            Some(KEY) => Some(SECRET),
            Some("synthetic-other-key") => Some("synthetic-other-material"),
            _ => None,
        };
        let valid = request
            .url
            .split_once('?')
            .and_then(|(_, query)| query.rsplit_once("&signature="))
            .is_some_and(|(payload, signature)| {
                let stamp = payload
                    .split('&')
                    .find_map(|part| part.strip_prefix("timestamp="))
                    .and_then(|s| s.parse::<u64>().ok());
                payload.split('&').any(|part| part == "recvWindow=20000")
                    && stamp.is_some_and(|s| s.abs_diff(crate::accounts::now_ms()) <= 5000)
                    && material.is_some_and(|m| {
                        signature == hmac_sha256(m.as_bytes(), payload.as_bytes(), Encoding::Hex)
                    })
            });
        let mut seen = io.seen.lock().unwrap();
        seen.signed_reads += 1;
        if !valid {
            seen.refused += 1;
            return json!({"code":-1022,"msg":"synthetic signature refusal"});
        }
    }
    match route {
        "/fapi/v1/time" => json!({"serverTime":crate::accounts::now_ms()}),
        "/fapi/v1/exchangeInfo" => {
            json!({"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":2,"quantityPrecision":3,"filters":[{"filterType":"LOT_SIZE","minQty":"0.001","maxQty":"1000","stepSize":"0.001"},{"filterType":"PRICE_FILTER","tickSize":"0.01"}]}]})
        }
        "/fapi/v1/ticker/24hr" => {
            json!([{"symbol":"BTCUSDT","lastPrice":"100","priceChangePercent":"0","volume":"1","quoteVolume":"100"}])
        }
        "/fapi/v4/account" => {
            json!({"totalWalletBalance":"1000","availableBalance":"800","totalInitialMargin":"100","totalOpenOrderInitialMargin":"20","totalUnrealizedProfit":"5"})
        }
        "/fapi/v1/positionSide/dual" => json!({"dualSidePosition":false}),
        "/fapi/v1/listenKey" => json!({"listenKey":TOKEN}),
        "/fapi/v1/leverageBracket"
        | "/fapi/v1/positionRisk"
        | "/fapi/v1/openOrders"
        | "/fapi/v1/openInterest" => json!([]),
        _ => panic!("unexpected Aster V1 startup route: {route}"),
    }
}

pub(super) fn connect(
    io: &VenueIo,
    socket: SocketId,
    url: &str,
    events: Events,
) -> mpsc::UnboundedSender<String> {
    assert!(
        url.starts_with("wss://fstream.asterdex.com/")
            || url.starts_with("wss://fstream5.asterdex-testnet.com/"),
        "unexpected Aster socket: {url}"
    );
    if url.contains(TOKEN) {
        io.seen.lock().unwrap().aster_private_sockets += 1;
    }
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    events.send(HostEvent::WsOpened(socket)).unwrap();
    tokio::task::spawn_local(async move {
        while let Some(text) = rx.recv().await {
            let frame: Value = serde_json::from_str(&text).unwrap();
            assert!(matches!(
                frame["method"].as_str(),
                Some("SUBSCRIBE" | "LIST_SUBSCRIPTIONS")
            ));
            let _ = events.send(HostEvent::WsMessage(
                socket,
                json!({"id":frame["id"],"result":[]}).to_string(),
            ));
        }
    });
    tx
}

#[tokio::test]
async fn asterdex_v1_delegated_private_startup_checks_the_handed_over_pair() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for (material, accepted, testnet) in [
                (SECRET, true, false),
                (SECRET, true, true),
                ("synthetic-other-material", false, false),
                ("synthetic-other-material", false, true),
            ] {
                let init = crate::envelope::InitPayload {
                    account_name: "fixture".into(),
                    exchange_name: "asterdex".into(),
                    key: KEY.into(),
                    signing_mode: Some("delegated".into()),
                    testnet: Some(testnet),
                    ..Default::default()
                };
                let prepared = crate::delegated::prepare_delegated(
                    &init,
                    crate::delegated::DelegatedPair::new(KEY.into(), material.into(), None),
                )
                .unwrap();
                let (result, seen) = check("asterdex", prepared).await;
                let seen = seen.lock().unwrap();
                assert_eq!(result.is_ok(), accepted, "{result:?}; {:?}", seen.paths);
                assert!(seen.signed_reads > 0);
                if accepted {
                    assert!(seen.aster_private_sockets > 0);
                    assert_eq!(seen.refused, 0);
                    for route in [
                        "/fapi/v4/account",
                        "/fapi/v1/positionRisk",
                        "/fapi/v1/openOrders",
                    ] {
                        assert!(
                            seen.paths.iter().any(|path| path == route),
                            "missing native private read: {route}"
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
async fn asterdex_v1_remote_private_startup_checks_key_replay_and_socket() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let prior = Arc::new(Mutex::new(HashMap::new()));
            for answer in [
                DeviceAnswer::Correct,
                DeviceAnswer::DifferentKey,
                DeviceAnswer::Replay,
            ] {
                if matches!(answer, DeviceAnswer::Replay) {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                let (prepared, link, task) = remote("asterdex", answer, prior.clone()).await;
                let (result, seen) = check("asterdex", prepared).await;
                {
                    let seen = seen.lock().unwrap();
                    assert!(seen.signed_reads > 0);
                    if matches!(answer, DeviceAnswer::Correct) {
                        assert!(result.is_ok(), "{result:?}; {:?}", seen.paths);
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
fn asterdex_v1_public_discovery_never_proves_the_key() {
    let io = VenueIo {
        venue: "asterdex",
        seen: Arc::default(),
    };
    let mut request = HttpRequest {
        id: safe_cex_engine::RequestId(1),
        method: Method::Get,
        route: None,
        timeout_ms: None,
        url: "https://fapi.asterdex.com/fapi/v1/exchangeInfo".into(),
        headers: vec![],
        signer_headers: vec![],
        body: None,
    };
    assert!(response(&io, &request)["symbols"].is_array());
    assert_eq!(io.seen.lock().unwrap().signed_reads, 0);
    for route in [
        "/fapi/v4/account",
        "/fapi/v1/positionRisk",
        "/fapi/v1/openOrders",
        "/fapi/v1/positionSide/dual",
    ] {
        request.url = format!("https://fapi.asterdex.com{route}");
        assert_eq!(response(&io, &request)["code"], -1022);
    }
    assert_eq!(io.seen.lock().unwrap().refused, 4);
}
