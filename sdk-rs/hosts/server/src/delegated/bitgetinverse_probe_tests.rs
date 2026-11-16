//! Offline Coin-M startup proof, using existing native requests and real HMACs.
//! Shapes: https://www.bitget.com/docs/catalog/classic-contract-account/classic-contract-account
//! MACs: https://www.bitget.com/docs/classic/rest-api
use super::*;

const VENUE: &str = "bitgetinverse";
const ORIGIN: &str = "https://api.bitget.com";
const SETTINGS: &str = "/api/v3/account/settings";
const ACCOUNT: &str = "/api/v2/mix/account/account";
const CONTRACTS: &str = "/api/v2/mix/market/contracts";
const TICKERS: &str = "/api/v2/mix/market/tickers";
const PRIVATE: &[&str] = &[
    SETTINGS,
    "/api/v2/mix/account/accounts",
    ACCOUNT,
    "/api/v2/mix/position/all-position",
    "/api/v2/mix/order/orders-pending",
    "/api/v2/mix/order/orders-plan-pending",
];

fn material(key: &str) -> Option<&'static str> {
    match key {
        KEY => Some(SECRET),
        "synthetic-other-key" => Some("synthetic-other-material"),
        _ => None,
    }
}

fn fresh(stamp: u64) -> bool {
    let now = crate::accounts::now_ms();
    stamp <= now.saturating_add(1000) && now.saturating_sub(stamp) <= 30_000
}

fn product_scope(request: &HttpRequest) -> bool {
    let target = path(request);
    if target == SETTINGS {
        return true;
    }
    let Some((route, query)) = target.split_once('?') else {
        return false;
    };
    let mut parameters = HashMap::new();
    for part in query.split('&') {
        let Some((name, value)) = part.split_once('=') else {
            return false;
        };
        if parameters.insert(name, value).is_some() {
            return false;
        }
    }
    parameters
        .get("productType")
        .is_some_and(|value| matches!(*value, "coin-futures" | "scoin-futures"))
        && parameters
            .get("marginCoin")
            .is_none_or(|coin| *coin == "BTC")
        && parameters
            .get("symbol")
            .map_or(route != ACCOUNT, |symbol| *symbol == "BTCUSD")
}

fn only_header<'a>(request: &'a HttpRequest, name: &str) -> Option<&'a str> {
    let mut fields = request
        .headers
        .iter()
        .filter(|(field, _)| field.eq_ignore_ascii_case(name));
    let value = fields.next()?.1.as_str();
    fields.next().is_none().then_some(value)
}

fn valid(request: &HttpRequest) -> bool {
    let Some(material) = only_header(request, "ACCESS-KEY").and_then(material) else {
        return false;
    };
    let Some(stamp) = only_header(request, "ACCESS-TIMESTAMP") else {
        return false;
    };
    request.method == Method::Get
        && request.body.is_none()
        && product_scope(request)
        && stamp.parse::<u64>().is_ok_and(fresh)
        && only_header(request, "ACCESS-PASSPHRASE") == Some(PASS)
        && only_header(request, "ACCESS-SIGN")
            == Some(
                hmac_sha256(
                    material.as_bytes(),
                    format!("{stamp}GET{}", path(request)).as_bytes(),
                    Encoding::Base64,
                )
                .as_str(),
            )
}

pub(super) fn response(io: &VenueIo, request: &HttpRequest) -> Value {
    assert!(request.url.starts_with(&format!("{ORIGIN}/")));
    assert_eq!(request.method, Method::Get, "inverse proof never trades");
    assert!(request.body.is_none());
    let route = path(request).split('?').next().unwrap();
    assert!(
        PRIVATE.contains(&route) || matches!(route, CONTRACTS | TICKERS),
        "unexpected inverse startup route: {route}"
    );
    if PRIVATE.contains(&route) {
        let accepted = valid(request);
        let mut seen = io.seen.lock().unwrap();
        seen.signed_reads += 1;
        if !accepted || seen.refused_private_path == Some(route) {
            seen.refused += 1;
            return json!({"code":"40006","msg":"synthetic inverse signature refusal","data":null});
        }
        if route == SETTINGS && seen.bitget_inverse_uta {
            return json!({"code":"00000","data":{"holdMode":"one_way_mode"}});
        }
    } else {
        assert!(product_scope(request), "public discovery must use Coin-M");
    }
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/bitgetinverse-admission-reads.json"
    ))
    .unwrap();
    fixtures
        .get(route)
        .expect("every startup route has a documented response shape")
        .clone()
}

fn valid_login(value: &Value) -> bool {
    let Some(args) = value["args"].as_array().filter(|args| args.len() == 1) else {
        return false;
    };
    let arg = &args[0];
    let Some(material) = arg["apiKey"].as_str().and_then(material) else {
        return false;
    };
    let Some(stamp) = arg["timestamp"].as_str() else {
        return false;
    };
    value["op"] == "login"
        && arg["passphrase"] == PASS
        && stamp
            .parse::<u64>()
            .ok()
            .and_then(|seconds| seconds.checked_mul(1000))
            .is_some_and(fresh)
        && arg["sign"]
            == hmac_sha256(
                material.as_bytes(),
                format!("{stamp}GET/user/verify").as_bytes(),
                Encoding::Base64,
            )
}

pub(super) fn connect(
    io: &VenueIo,
    socket: SocketId,
    url: &str,
    events: Events,
) -> mpsc::UnboundedSender<String> {
    assert!(matches!(
        url,
        "wss://ws.bitget.com/v2/ws/public" | "wss://ws.bitget.com/v2/ws/private"
    ));
    let private = url.ends_with("/private");
    let seen = io.seen.clone();
    let (send, mut receive) = mpsc::unbounded_channel::<String>();
    let _ = events.send(HostEvent::WsOpened(socket));
    tokio::task::spawn_local(async move {
        while let Some(text) = receive.recv().await {
            if text == "ping" {
                let _ = events.send(HostEvent::WsMessage(socket, "pong".into()));
                continue;
            }
            let value: Value = serde_json::from_str(&text).unwrap();
            let reply = if private && value["op"] == "login" {
                if valid_login(&value) {
                    json!({"event":"login","code":"0","msg":""})
                } else {
                    seen.lock().unwrap().refused += 1;
                    json!({"event":"error","code":"30011","msg":"synthetic inverse login refusal"})
                }
            } else {
                assert_eq!(value["op"], "subscribe", "proof may only open read feeds");
                for arg in value["args"].as_array().unwrap() {
                    assert!(matches!(
                        arg["instType"].as_str(),
                        Some("COIN-FUTURES" | "SCOIN-FUTURES")
                    ));
                }
                json!({"event":"subscribe","code":"0","arg":value["args"][0]})
            };
            let _ = events.send(HostEvent::WsMessage(socket, reply.to_string()));
        }
    });
    send
}

pub(super) fn assert_startup(seen: &Seen) {
    for route in PRIVATE {
        assert!(
            seen.urls
                .iter()
                .any(|url| url.split('?').next() == Some(format!("{ORIGIN}{route}").as_str())),
            "missing native inverse private read: {route}"
        );
    }
    assert!(
        seen.urls
            .iter()
            .all(|url| url.starts_with(&format!("{ORIGIN}/")))
    );
    assert!(
        seen.urls
            .iter()
            .any(|url| url.contains("productType=coin-futures"))
    );
    assert!(seen.urls.iter().any(|url| url.contains("marginCoin=BTC")));
    assert!(seen.signed_reads >= PRIVATE.len());
}

fn signed(target: &str, key: &str, material: &str, stamp: u64) -> HttpRequest {
    HttpRequest {
        id: safe_cex_engine::RequestId(1),
        method: Method::Get,
        route: None,
        timeout_ms: None,
        url: format!("{ORIGIN}{target}"),
        headers: vec![
            ("ACCESS-KEY", key.into()),
            ("ACCESS-TIMESTAMP", stamp.to_string()),
            ("ACCESS-PASSPHRASE", PASS.into()),
            (
                "ACCESS-SIGN",
                hmac_sha256(
                    material.as_bytes(),
                    format!("{stamp}GET{target}").as_bytes(),
                    Encoding::Base64,
                ),
            ),
        ],
        signer_headers: vec![],
        body: None,
    }
}

#[test]
fn bitgetinverse_private_fixture_checks_key_passphrase_mac_time_and_product() {
    for route in PRIVATE {
        let mut target = if *route == SETTINGS {
            route.to_string()
        } else {
            format!("{route}?productType=coin-futures&marginCoin=BTC")
        };
        if *route == ACCOUNT {
            target.push_str("&symbol=BTCUSD");
        }
        for (key, material) in [
            (KEY, SECRET),
            ("synthetic-other-key", "synthetic-other-material"),
        ] {
            let request = signed(&target, key, material, crate::accounts::now_ms());
            assert!(valid(&request));
            for name in [
                "ACCESS-KEY",
                "ACCESS-TIMESTAMP",
                "ACCESS-PASSPHRASE",
                "ACCESS-SIGN",
            ] {
                let mut absent = request.clone();
                absent.headers.retain(|(field, _)| *field != name);
                assert!(!valid(&absent), "missing {name}");
            }
            let mut duplicated = request.clone();
            duplicated.headers.push(("ACCESS-KEY", key.into()));
            assert!(!valid(&duplicated));
            let mut wrong_passphrase = request.clone();
            wrong_passphrase
                .headers
                .iter_mut()
                .find(|(name, _)| *name == "ACCESS-PASSPHRASE")
                .unwrap()
                .1 = "wrong".into();
            assert!(!valid(&wrong_passphrase));
            assert!(!valid(&signed(
                &target,
                key,
                "wrong-material",
                crate::accounts::now_ms()
            )));
            assert!(!valid(&signed(
                &target,
                key,
                material,
                crate::accounts::now_ms() - 60_000
            )));
            if *route != SETTINGS {
                assert!(!valid(&signed(
                    &target.replace("coin-futures", "usdt-futures"),
                    key,
                    material,
                    crate::accounts::now_ms()
                )));
                assert!(!valid(&signed(
                    &format!("{target}&productType=usdc-futures"),
                    key,
                    material,
                    crate::accounts::now_ms()
                )));
                if *route == ACCOUNT {
                    assert!(!valid(&signed(
                        &target.replace("BTCUSD", "ETHUSDT"),
                        key,
                        material,
                        crate::accounts::now_ms()
                    )));
                }
            }
        }
    }
}

#[test]
fn bitgetinverse_discovery_is_not_possession_proof() {
    let io = VenueIo {
        venue: VENUE,
        seen: Arc::default(),
    };
    for route in [CONTRACTS, TICKERS] {
        let mut request = signed(
            &format!("{route}?productType=coin-futures"),
            KEY,
            SECRET,
            crate::accounts::now_ms(),
        );
        request.headers.clear();
        response(&io, &request);
    }
    assert_eq!(io.seen.lock().unwrap().signed_reads, 0);
}

#[test]
fn bitgetinverse_private_read_feed_login_uses_independent_fresh_hmac() {
    for (key, material) in [
        (KEY, SECRET),
        ("synthetic-other-key", "synthetic-other-material"),
    ] {
        let stamp = (crate::accounts::now_ms() / 1000).to_string();
        let value = json!({"op":"login","args":[{"apiKey":key,"passphrase":PASS,"timestamp":stamp,"sign":hmac_sha256(material.as_bytes(),format!("{stamp}GET/user/verify").as_bytes(),Encoding::Base64)}]});
        assert!(valid_login(&value));
        for field in ["apiKey", "passphrase", "timestamp", "sign"] {
            let mut invalid = value.clone();
            invalid["args"][0][field] = "wrong".into();
            assert!(!valid_login(&invalid), "{field}");
        }
    }
}

#[tokio::test]
async fn bitgetinverse_native_private_startup_proves_the_key_on_coin_reads() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for (material, accepted) in [(SECRET, true), ("different-material", false)] {
                let (result, seen) = check(VENUE, local(VENUE, material)).await;
                assert_eq!(result.is_ok(), accepted, "{result:?}");
                let seen = seen.lock().unwrap();
                if accepted {
                    assert_startup(&seen);
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
async fn bitgetinverse_remote_private_startup_refuses_wrong_key_and_replay() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let prior = Arc::new(Mutex::new(HashMap::new()));
            for answer in [
                DeviceAnswer::Correct,
                DeviceAnswer::DifferentKey,
                DeviceAnswer::Replay,
            ] {
                let (prepared, link, task) = remote(VENUE, answer, prior.clone()).await;
                let (result, seen) = check(VENUE, prepared).await;
                {
                    let seen = seen.lock().unwrap();
                    if matches!(answer, DeviceAnswer::Correct) {
                        assert!(result.is_ok(), "{result:?}");
                        assert_startup(&seen);
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
                link.mark_unavailable();
                task.abort();
                if let Err(error) = task.await {
                    assert!(error.is_cancelled(), "device responder failed: {error}");
                }
            }
        })
        .await;
}

#[tokio::test]
async fn bitgetinverse_simulated_startup_keeps_its_native_coin_product() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let mut prepared = local(VENUE, SECRET);
            let mut options: Value = serde_json::from_str(&prepared.options).unwrap();
            options["testnet"] = true.into();
            prepared.options = options.to_string();
            let (result, seen) = check(VENUE, prepared).await;
            assert!(result.is_ok(), "{result:?}");
            let seen = seen.lock().unwrap();
            assert!(
                seen.urls
                    .iter()
                    .any(|url| url.contains("productType=scoin-futures"))
            );
            assert!(
                !seen
                    .urls
                    .iter()
                    .any(|url| url.contains("productType=coin-futures"))
            );
            assert!(
                seen.urls
                    .iter()
                    .all(|url| url.starts_with(&format!("{ORIGIN}/")))
            );
            assert!(seen.signed_reads >= PRIVATE.len());
            assert_eq!(seen.refused, 0);
        })
        .await;
}

#[tokio::test]
async fn bitgetinverse_uta_account_refuses_before_classic_private_reads() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let prepared = local(VENUE, SECRET);
            let seen = Arc::new(Mutex::new(Seen {
                bitget_inverse_uta: true,
                ..Default::default()
            }));
            let io = VenueIo {
                venue: VENUE,
                seen: seen.clone(),
            };
            let result = tokio::time::timeout(
                Duration::from_secs(3),
                run(
                    VENUE,
                    &prepared.options,
                    prepared.resident.provider(),
                    io,
                    ServerConfig::default(),
                    Arc::new(|| true),
                ),
            )
            .await
            .expect("unsupported UTA inverse startup must settle");
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
            let seen = seen.lock().unwrap();
            assert!(seen.urls.iter().any(|url| url.ends_with(SETTINGS)));
            assert!(!seen.urls.iter().any(|url| {
                PRIVATE[1..]
                    .iter()
                    .any(|route| url.split('?').next() == Some(format!("{ORIGIN}{route}").as_str()))
            }));
        })
        .await;
}

#[tokio::test]
async fn bitgetinverse_public_or_frame_fed_options_cannot_prove_possession() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for field in ["publicDataOnly", "frameFed", "publicMode"] {
                let mut prepared = local(VENUE, SECRET);
                let mut options: Value = serde_json::from_str(&prepared.options).unwrap();
                options[field] = if field == "publicMode" {
                    json!({"requested":true})
                } else {
                    true.into()
                };
                prepared.options = options.to_string();
                let (result, seen) = check(VENUE, prepared).await;
                assert!(matches!(
                    result,
                    Err(ProbeRefusal {
                        transient: false,
                        ..
                    })
                ));
                assert_eq!(seen.lock().unwrap().signed_reads, 0);
            }
        })
        .await;
}
