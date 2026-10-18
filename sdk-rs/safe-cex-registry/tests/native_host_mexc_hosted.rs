//! Production MEXC registry: shared hosted HTTP and private websocket signing.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_exchange::runtime::{Cleanup, RemoteSigner};
use safe_cex_transport::{
    TransportFuture,
    signing::{HttpSigningResponse, RemoteSignerRequest, RemoteSignerResponse, SignedConfig},
};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, rc::Rc, sync::OnceLock};

fn source() -> &'static Value {
    static CAPTURE: OnceLock<Value> = OnceLock::new();
    CAPTURE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/mexc_hosted_source.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "actual MEXC source: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            capture["cases"].as_array().unwrap().len(),
            3,
            "MEXC source outcome census"
        );
        for (index, mode) in ["success", "error", "missing"].into_iter().enumerate() {
            let case = &capture["cases"][index];
            assert_eq!(case["mode"], mode);
            assert_eq!(case["calls"].as_array().unwrap().len(), 2);
            assert_eq!(case["wire"].as_array().unwrap().len(), 2);
            assert_eq!(
                case["frames"].as_array().unwrap().len(),
                if index == 0 { 2 } else { 0 }
            );
            assert_eq!(
                case["errors"].as_array().unwrap().len(),
                if index == 0 { 0 } else { 1 }
            );
        }
        eprintln!(
            "MEXC source Node {} / {} / V8 {}",
            capture["node"], capture["arch"], capture["v8"]
        );
        capture
    })
}
struct HostedMexc {
    mode: usize,
    calls: RefCell<Vec<Value>>,
}
impl safe_cex_transport::signing::RemoteSigner for HostedMexc {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        self.calls
            .borrow_mut()
            .push(serde_json::to_value(&request).unwrap());
        let response = match request {
            RemoteSignerRequest::Http(request) => {
                let mut headers = request.config.headers;
                headers.insert("X-Hosted-Mexc".into(), "callback-proof".into());
                RemoteSignerResponse::Http(HttpSigningResponse {
                    id: request.id,
                    config: Some(SignedConfig {
                        url: None,
                        params: request.config.params,
                        data: request.config.data,
                        headers,
                    }),
                    error: None,
                })
            }
            RemoteSignerRequest::WsAuth { request, .. } => {
                RemoteSignerResponse::WsAuth(safe_cex_types::ws_auth::WsAuthResponse {
                    id: request.id,
                    ws_auth_signature: (self.mode == 0).then(|| "hosted-ws-signature".into()),
                    error: (self.mode == 1).then(|| "signer-offline".into()),
                })
            }
        };
        Box::pin(async move { Ok(response) })
    }
}
impl RemoteSigner for HostedMexc {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> Cleanup {
        Box::new(|| {})
    }
}
fn payload(request: &support::Seen) -> Option<Value> {
    let data = match request.path.as_str() {
        "/api/v1/private/account/assets" => {
            json!([{"currency":"USDT","availableBalance":81,"equity":123,"unrealized":5,"frozenBalance":0}])
        }
        "/api/v1/private/position/position_mode" => json!(1),
        "/api/v1/private/order/list/open_orders"
        | "/api/v1/private/planorder/list/orders"
        | "/api/v1/private/stoporder/list/orders" => json!({"resultList":[],"totalCount":0}),
        _ => json!([]),
    };
    Some(json!({"success":true,"code":0,"data":data}))
}
async fn exercise(mode: usize) {
    let expected = &source()["cases"][mode];
    let signer = Rc::new(HostedMexc {
        mode,
        calls: RefCell::default(),
    });
    let h = support::venue_harness(
        ExchangeName::Mexc,
        Rc::new(payload),
        json!({"key":" hosted-api-key ","secret":""}),
        Some((signer.clone(), Some("mexc-hosted-account".into()))),
        safe_cex_exchange::native::native_runtime(),
    )
    .await;
    assert!(h.exchange.base().options().secret.is_empty());
    assert!(h.exchange.can_sign(), "MEXC HOSTED typed capability");
    let errors = Rc::new(RefCell::new(vec![]));
    let capture = errors.clone();
    h.exchange.base().on_args(
        "error",
        Rc::new(move |_, args| {
            capture.borrow_mut().push(
                safe_cex_exchange::js_runtime::value::to_json(
                    &safe_cex_exchange::js_runtime::value::Js::array(args.to_vec()),
                )
                .unwrap(),
            );
            Ok(())
        }),
    );
    let account = h.exchange.get_account().await.unwrap();
    use sha2::Digest;
    let hash = sha2::Sha256::digest(b" hosted-api-key ");
    let suffix = hash[..12]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(account.user_id, format!("mexc:{suffix}"));
    {
        let calls = signer.calls.borrow();
        assert_eq!(
            calls.len(),
            1,
            "MEXC HOSTED private HTTP reaches shared signer"
        );
        for field in ["exchangeName", "accountName"] {
            assert_eq!(
                calls[0][field], expected["calls"][0][field],
                "MEXC HTTP {field}"
            );
        }
        for field in ["method", "url", "baseURL"] {
            assert_eq!(
                calls[0]["config"][field], expected["calls"][0]["config"][field],
                "MEXC HTTP {field}"
            );
        }
        assert!(
            !calls[0]["config"]["headers"]
                .as_object()
                .unwrap()
                .keys()
                .any(|k| k.eq_ignore_ascii_case("Signature")),
            "MEXC HOSTED skips local signing"
        );
    }
    assert!(h.exchange.fetch_markets().await.unwrap().is_empty());
    assert_eq!(
        signer.calls.borrow().len(),
        1,
        "MEXC HOSTED public HTTP bypass"
    );
    {
        let seen = h.seen.borrow();
        assert_eq!(seen.len(), 2);
        assert_eq!(
            seen[0].headers["x-hosted-mexc"],
            expected["wire"][0]["header"].as_str().unwrap()
        );
        assert!(!seen[1].headers.contains_key("x-hosted-mexc"));
    }
    h.exchange.start().await.unwrap();
    support::eventually("MEXC hosted WS RPC", || {
        signer.calls.borrow().iter().any(|r| r["type"] == "ws-auth")
    })
    .await;
    {
        let calls = signer.calls.borrow();
        let ws: Vec<_> = calls.iter().filter(|r| r["type"] == "ws-auth").collect();
        assert_eq!(ws.len(), 1, "MEXC HOSTED one WS signing request");
        for field in ["exchangeName", "accountName"] {
            assert_eq!(
                ws[0][field], expected["calls"][1][field],
                "MEXC HOSTED WS metadata {field}"
            );
        }
        assert!(!ws[0]["id"].as_str().unwrap().is_empty());
        assert_eq!(
            ws[0]["wsAuth"]["outputFormat"],
            expected["calls"][1]["wsAuth"]["outputFormat"]
        );
        let time = ws[0]["wsAuth"]["extraData"]["reqTime"].as_str().unwrap();
        assert!(time.parse::<u64>().unwrap() > 0);
        assert_eq!(
            ws[0]["wsAuth"]["toSign"],
            format!("hosted-api-key{time}"),
            "MEXC HOSTED trimmed key preimage"
        );
    }
    if mode == 0 {
        support::eventually("MEXC hosted login", || {
            h.received
                .borrow()
                .iter()
                .any(|s| serde_json::from_str::<Value>(s).unwrap()["method"] == "login")
        })
        .await;
        let frames: Vec<Value> = h
            .received
            .borrow()
            .iter()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        let login = frames.iter().find(|f| f["method"] == "login").unwrap();
        assert_eq!(
            login["param"]["apiKey"],
            expected["frames"][0]["param"]["apiKey"]
        );
        assert_eq!(
            login["param"]["signature"], expected["frames"][0]["param"]["signature"],
            "MEXC HOSTED signature reaches native WS"
        );
        {
            let calls = signer.calls.borrow();
            let ws = calls.iter().find(|r| r["type"] == "ws-auth").unwrap();
            assert_eq!(
                login["param"]["reqTime"],
                ws["wsAuth"]["extraData"]["reqTime"]
            );
        }
        assert!(
            !frames.iter().any(|f| f["method"] == "personal.filter"),
            "MEXC HOSTED subscriptions await login reply"
        );
        for peer in h.peers.borrow().iter() {
            peer.push
                .send(json!({"channel":"rs.login","data":"success"}).to_string())
                .unwrap();
        }
        support::eventually("MEXC source private filters", || {
            h.received
                .borrow()
                .iter()
                .any(|s| serde_json::from_str::<Value>(s).unwrap()["method"] == "personal.filter")
        })
        .await;
        let filters: Vec<Value> = h
            .received
            .borrow()
            .iter()
            .map(|s| serde_json::from_str(s).unwrap())
            .filter(|f: &Value| f["method"] == "personal.filter")
            .collect();
        assert_eq!(filters.len(), 1);
        assert_eq!(
            filters[0], expected["frames"][1],
            "MEXC HOSTED seven source private topics"
        );
        assert!(errors.borrow().is_empty());
    } else {
        let message = expected["errors"][0].as_str().unwrap();
        support::eventually("MEXC hosted signer failure log", || {
            errors
                .borrow()
                .iter()
                .any(|args| args[0] == format!("Exchange Error: {message}"))
        })
        .await;
        assert_eq!(
            errors.borrow().len(),
            1,
            "MEXC HOSTED one source signer error"
        );
        assert!(!message.to_ascii_lowercase().contains("auth failed"));
        assert!(
            !h.received.borrow().iter().any(|s| matches!(
                serde_json::from_str::<Value>(s).unwrap()["method"].as_str(),
                Some("login" | "personal.filter")
            )),
            "MEXC HOSTED signer failure sends no login or subscriptions"
        );
    }
    h.exchange.dispose().unwrap();
}
#[test]
fn mexc_hosted_http_and_ws_match_actual_source() {
    support::run_local(exercise(0));
}
#[test]
fn mexc_hosted_ws_error_preserves_source_log_without_login() {
    support::run_local(exercise(1));
}
#[test]
fn mexc_hosted_ws_missing_signature_preserves_source_log_without_login() {
    support::run_local(exercise(2));
}

#[test]
fn mexc_private_open_posts_the_worker_reconnect_packet_without_data() {
    support::run_local(async {
        let h = support::venue_harness(
            safe_cex_types::ExchangeName::Mexc,
            Rc::new(|request| {
                let data = match request.path.as_str() {
                    "/api/v1/private/account/assets" => json!([]),
                    "/api/v1/private/position/position_mode" => json!(1),
                    "/api/v1/private/order/list/open_orders"
                    | "/api/v1/private/planorder/list/orders"
                    | "/api/v1/private/stoporder/list/orders" => {
                        json!({"resultList":[],"totalCount":0})
                    }
                    _ => json!([]),
                };
                Some(json!({"success":true,"code":0,"data":data}))
            }),
            json!({"key":"reconnect-key","secret":"reconnect-secret"}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        let packets = Rc::new(RefCell::new(vec![]));
        let captured = packets.clone();
        h.exchange.base().on_args(
            "ws-reconnected",
            Rc::new(move |event, args| {
                assert_eq!(args.len(), 1);
                let data = safe_cex_registry::native_value::NativeValue::from_js(&args[0])?;
                assert_eq!(
                    data,
                    safe_cex_registry::native_value::NativeValue::string("")
                );
                captured.borrow_mut().push(
                    safe_cex_registry::native_account::native_account_event(event, &data),
                );
                Ok(())
            }),
        );
        h.exchange
            .base()
            .get_socket(safe_cex_exchange::socket_lifecycle::SocketSlot::Private)
            .expect("the actual MEXC constructor registered its private socket")
            .connect_and_subscribe(false)
            .unwrap();
        support::eventually("real MEXC private login and reconnect notification", || {
            !packets.borrow().is_empty()
                && h.received.borrow().iter().any(|frame| {
                    serde_json::from_str::<serde_json::Value>(frame).unwrap()["method"] == "login"
                })
        })
        .await;
        assert!(
            !h.peers.borrow().is_empty(),
            "the native socket physically opened"
        );
        assert_eq!(packets.borrow().len(), 1);
        assert_eq!(
            packets.borrow()[0],
            safe_cex_registry::native_value::NativeValue::object([(
                "event",
                safe_cex_registry::native_value::NativeValue::string("ws-reconnected")
            )]),
            "ExchangeWorker.ts forwards ws-reconnected without an own data property"
        );
        h.exchange.dispose().unwrap();
    });
}
