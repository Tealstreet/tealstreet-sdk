//! Options admission proves the joined key through its existing native EAPI reads.
use super::*;

const ORIGIN: &str = "https://eapi.binance.com";
const PRIVATE: &[&str] = &[
    "/eapi/v1/marginAccount",
    "/eapi/v1/position",
    "/eapi/v1/openOrders",
];

fn get_request(url: String) -> HttpRequest {
    HttpRequest {
        id: safe_cex_engine::RequestId(1),
        method: Method::Get,
        route: None,
        timeout_ms: None,
        url,
        headers: vec![],
        signer_headers: vec![],
        body: None,
    }
}

fn valid(request: &HttpRequest) -> bool {
    let material = match header(request, "X-MBX-APIKEY") {
        Some(KEY) => SECRET,
        Some("synthetic-other-key") => "synthetic-other-material",
        _ => return false,
    };
    let Some((payload, signature)) = request
        .url
        .split_once('?')
        .and_then(|(_, query)| query.rsplit_once("&signature="))
    else {
        return false;
    };
    let mut parameters = HashMap::new();
    for part in payload.split('&') {
        let Some((name, value)) = part.split_once('=') else {
            return false;
        };
        if parameters.insert(name, value).is_some() {
            return false;
        }
    }
    let stamp = parameters
        .get("timestamp")
        .and_then(|value| value.parse::<u64>().ok());
    let window = parameters
        .get("recvWindow")
        .and_then(|value| value.parse::<u64>().ok());
    let now = crate::accounts::now_ms();
    window == Some(5000)
        && stamp.is_some_and(|stamp| stamp <= now + 1000 && now.saturating_sub(stamp) <= 5000)
        && signature == hmac_sha256(material.as_bytes(), payload.as_bytes(), Encoding::Hex)
}

pub(super) fn response(io: &VenueIo, request: &HttpRequest) -> Value {
    assert!(request.url.starts_with(&format!("{ORIGIN}/eapi/")));
    assert_eq!(request.method, Method::Get, "Options proof never trades");
    assert!(request.body.is_none());
    let route = path(request).split('?').next().unwrap();
    if PRIVATE.contains(&route) {
        let accepted = valid(request);
        let mut seen = io.seen.lock().unwrap();
        seen.signed_reads += 1;
        if !accepted {
            seen.refused += 1;
            return json!({"code":-2015,"msg":"synthetic Options signature refusal"});
        }
    }
    if route == "/eapi/v1/time" {
        return json!({"serverTime":crate::accounts::now_ms()});
    }
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/binanceoptions-admission-reads.json"
    ))
    .unwrap();
    fixtures
        .get(route)
        .unwrap_or_else(|| panic!("unexpected Options startup route: {route}"))
        .clone()
}

pub(super) fn assert_startup(seen: &Seen) {
    for route in PRIVATE {
        assert!(
            seen.urls
                .iter()
                .any(|url| url.split('?').next() == Some(format!("{ORIGIN}{route}").as_str())),
            "missing native Options private route: {route}"
        );
    }
    assert!(seen.urls.iter().all(|url| url.starts_with(ORIGIN)));
    assert!(seen.signed_reads >= PRIVATE.len());
}

#[test]
fn binanceoptions_private_fixture_requires_key_signature_and_fresh_timestamp() {
    let io = VenueIo {
        venue: "binanceoptions",
        seen: Arc::default(),
    };
    for route in PRIVATE {
        for (key, material) in [
            (KEY, SECRET),
            ("synthetic-other-key", "synthetic-other-material"),
        ] {
            let payload = format!("recvWindow=5000&timestamp={}", crate::accounts::now_ms());
            let signature = hmac_sha256(material.as_bytes(), payload.as_bytes(), Encoding::Hex);
            let mut request =
                get_request(format!("{ORIGIN}{route}?{payload}&signature={signature}"));
            request.headers.push(("X-MBX-APIKEY", key.into()));
            assert_ne!(response(&io, &request)["code"], -2015);
            request.headers.clear();
            assert_eq!(response(&io, &request)["code"], -2015);
            request.headers.push(("X-MBX-APIKEY", key.into()));
            request.url = format!("{ORIGIN}{route}?{payload}");
            assert_eq!(response(&io, &request)["code"], -2015);
            let wrong = hmac_sha256(
                b"synthetic-wrong-material",
                payload.as_bytes(),
                Encoding::Hex,
            );
            request.url = format!("{ORIGIN}{route}?{payload}&signature={wrong}");
            assert_eq!(response(&io, &request)["code"], -2015);
            let stale = format!(
                "recvWindow=5000&timestamp={}",
                crate::accounts::now_ms() - 60_000
            );
            let signature = hmac_sha256(material.as_bytes(), stale.as_bytes(), Encoding::Hex);
            request.url = format!("{ORIGIN}{route}?{stale}&signature={signature}");
            assert_eq!(response(&io, &request)["code"], -2015);
        }
    }
}

#[test]
fn binanceoptions_public_discovery_does_not_count_as_key_proof() {
    let io = VenueIo {
        venue: "binanceoptions",
        seen: Arc::default(),
    };
    for route in ["/eapi/v1/exchangeInfo", "/eapi/v1/ticker", "/eapi/v1/mark"] {
        response(&io, &get_request(format!("{ORIGIN}{route}")));
    }
    assert_eq!(io.seen.lock().unwrap().signed_reads, 0);
}

#[tokio::test]
async fn binanceoptions_native_private_startup_validates_the_original_key() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for (material, accepted) in [(SECRET, true), ("synthetic-other-material", false)] {
                let (result, seen) =
                    check("binanceoptions", local("binanceoptions", material)).await;
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
async fn binanceoptions_remote_private_startup_refuses_wrong_key_and_replay() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let prior = Arc::new(Mutex::new(HashMap::new()));
            for answer in [
                DeviceAnswer::Correct,
                DeviceAnswer::DifferentKey,
                DeviceAnswer::Replay,
            ] {
                let (prepared, link, task) = remote("binanceoptions", answer, prior.clone()).await;
                let (result, seen) = check("binanceoptions", prepared).await;
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
