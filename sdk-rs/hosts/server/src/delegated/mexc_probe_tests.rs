//! MEXC futures proof uses the engine's signed private reads, never public discovery.
use super::*;

const PRIVATE: &[&str] = &[
    "/api/v1/private/position/position_mode",
    "/api/v1/private/account/assets",
    "/api/v1/private/position/open_positions",
    "/api/v1/private/order/list/open_orders",
    "/api/v1/private/planorder/list/orders",
    "/api/v1/private/stoporder/list/orders",
];

fn material(key: Option<&str>) -> Option<&'static str> {
    match key {
        Some(KEY) => Some(SECRET),
        Some("synthetic-other-key") => Some("synthetic-other-material"),
        _ => None,
    }
}

fn valid(key: Option<&str>, stamp: Option<&str>, signature: Option<&str>, payload: &str) -> bool {
    let (Some(key), Some(stamp), Some(signature)) = (key, stamp, signature) else {
        return false;
    };
    let Some(material) = material(Some(key)) else {
        return false;
    };
    stamp
        .parse::<u64>()
        .is_ok_and(|stamp| stamp.abs_diff(crate::accounts::now_ms()) <= 20_000)
        && signature
            == hmac_sha256(
                material.as_bytes(),
                format!("{key}{stamp}{payload}").as_bytes(),
                Encoding::Hex,
            )
}

pub(super) fn response(io: &VenueIo, request: &HttpRequest) -> Value {
    assert!(request.url.starts_with("https://api.mexc.com/"));
    assert_eq!(request.method, Method::Get, "proof never trades");
    assert!(request.body.is_none());
    let route = path(request).split('?').next().unwrap();
    if PRIVATE.contains(&route) {
        let query = request.url.split_once('?').map_or("", |(_, query)| query);
        let accepted = valid(
            header(request, "ApiKey"),
            header(request, "Request-Time"),
            header(request, "Signature"),
            query,
        );
        let mut seen = io.seen.lock().unwrap();
        seen.signed_reads += 1;
        if !accepted {
            seen.refused += 1;
            return json!({"success":false,"code":602,"message":"synthetic signature refusal"});
        }
    }
    if route == "/api/v1/contract/ping" {
        return json!({"success":true,"code":0,"data":crate::accounts::now_ms()});
    }
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/mexc-admission-reads.json"
    ))
    .unwrap();
    fixtures
        .get(route)
        .unwrap_or_else(|| panic!("unexpected MEXC startup route: {route}"))
        .clone()
}

pub(super) fn connect(
    io: &VenueIo,
    socket: SocketId,
    url: &str,
    events: Events,
) -> mpsc::UnboundedSender<String> {
    assert_eq!(url, "wss://contract.mexc.com/edge");
    let seen = io.seen.clone();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    events.send(HostEvent::WsOpened(socket)).unwrap();
    tokio::task::spawn_local(async move {
        while let Some(text) = rx.recv().await {
            let frame: Value = serde_json::from_str(&text).unwrap();
            let reply = match frame["method"].as_str().unwrap() {
                "login" => {
                    let param = &frame["param"];
                    let accepted = valid(
                        param["apiKey"].as_str(),
                        param["reqTime"].as_str(),
                        param["signature"].as_str(),
                        "",
                    );
                    seen.lock().unwrap().mexc_socket_auths += 1;
                    json!({"channel":"rs.login","success":accepted,"data":if accepted { "success" } else { "refused" }})
                }
                "ping" => json!({"channel":"pong","data":crate::accounts::now_ms()}),
                "personal.filter" => json!({"channel":"rs.personal.filter","data":"success"}),
                method if method.starts_with("sub.") => {
                    json!({"channel":format!("rs.{method}"),"data":"success"})
                }
                _ => panic!("unexpected MEXC startup socket method"),
            };
            let _ = events.send(HostEvent::WsMessage(socket, reply.to_string()));
        }
    });
    tx
}

// Only timestamp/window bounds vary between attempts; retain the read's parameters.
pub(super) fn operation(canonical: &str) -> String {
    canonical
        .strip_prefix(KEY)
        .or_else(|| canonical.strip_prefix("synthetic-other-key"))
        .expect("synthetic key prefixes the actual MEXC preimage")
        .trim_start_matches(|c: char| c.is_ascii_digit())
        .split('&')
        .filter(|part| !part.starts_with("start_time=") && !part.starts_with("end_time="))
        .collect::<Vec<_>>()
        .join("&")
}

pub(super) fn assert_startup(seen: &Seen) {
    for route in PRIVATE {
        assert!(
            seen.urls.iter().any(|url| url.split('?').next()
                == Some(format!("https://api.mexc.com{route}").as_str())),
            "missing native MEXC private route: {route}"
        );
    }
    assert!(seen.signed_reads >= PRIVATE.len());
    assert!(seen.mexc_socket_auths > 0);
}

#[tokio::test]
async fn mexc_native_private_startup_validates_the_original_key() {
    tokio::task::LocalSet::new()
        .run_until(async {
            assert_eq!(
                safe_cex_native::resolve::rest_base_url("mexc", false).unwrap(),
                "https://api.mexc.com"
            );
            assert!(safe_cex_native::resolve::rest_base_url("mexc", true).is_err());
            for (material, accepted) in [(SECRET, true), ("synthetic-other-material", false)] {
                let (result, seen) = check("mexc", local("mexc", material)).await;
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
                    assert_startup(&seen);
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

#[test]
fn mexc_public_discovery_and_unsigned_private_reads_cannot_prove_possession() {
    let io = VenueIo {
        venue: "mexc",
        seen: Arc::default(),
    };
    let mut request = HttpRequest {
        id: safe_cex_engine::RequestId(1),
        method: Method::Get,
        route: None,
        timeout_ms: None,
        url: "https://api.mexc.com/api/v1/contract/detail".into(),
        headers: vec![],
        signer_headers: vec![],
        body: None,
    };
    assert_eq!(response(&io, &request)["success"], true);
    assert_eq!(io.seen.lock().unwrap().signed_reads, 0);
    for route in PRIVATE {
        request.url = format!("https://api.mexc.com{route}");
        assert_eq!(response(&io, &request)["code"], 602);
    }
    let seen = io.seen.lock().unwrap();
    assert_eq!(seen.signed_reads, PRIVATE.len());
    assert_eq!(seen.refused, PRIVATE.len());
}
