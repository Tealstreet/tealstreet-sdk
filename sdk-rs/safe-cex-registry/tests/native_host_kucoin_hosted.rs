//! Native hosted account HTTP and private token startup for both KuCoin names.
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
                "/tests/kucoin_hosted_source.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "actual KuCoin source: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            capture["cases"].as_array().unwrap().len(),
            2,
            "KuCoin source family census"
        );
        for (index, name) in ["kucoin", "kucoinspot"].into_iter().enumerate() {
            let case = &capture["cases"][index];
            assert_eq!(case["name"], name);
            assert_eq!(
                case["calls"].as_array().unwrap().len(),
                if index == 0 { 5 } else { 4 }
            );
            assert_eq!(
                case["wire"].as_array().unwrap().len(),
                if index == 0 { 5 } else { 4 }
            );
            assert_eq!(
                case["frames"].as_array().unwrap().len(),
                if index == 0 { 7 } else { 3 }
            );
        }
        eprintln!(
            "KuCoin source Node {} / {} / V8 {}",
            capture["node"], capture["arch"], capture["v8"]
        );
        capture
    })
}
struct HostedKucoin {
    name: &'static str,
    calls: RefCell<Vec<Value>>,
}
impl safe_cex_transport::signing::RemoteSigner for HostedKucoin {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        self.calls
            .borrow_mut()
            .push(serde_json::to_value(&request).unwrap());
        let RemoteSignerRequest::Http(request) = request else {
            panic!("KuCoin WS authenticates through HTTP bullet-private")
        };
        let mut headers = request.config.headers;
        headers.insert("X-Hosted-Kucoin".into(), self.name.into());
        let response = RemoteSignerResponse::Http(HttpSigningResponse {
            id: request.id,
            config: Some(SignedConfig {
                url: None,
                params: request.config.params,
                data: request.config.data,
                headers,
            }),
            error: None,
        });
        Box::pin(async move { Ok(response) })
    }
}
impl RemoteSigner for HostedKucoin {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> Cleanup {
        Box::new(|| {})
    }
}
fn payload(request: &support::Seen, name: &str) -> Option<Value> {
    let data = match request.path.as_str() {
        "/api/v1/user-info" | "/api/v2/user-info" => {
            json!({"userId":format!("{name}-hosted"),"subUserId":"hosted-sub","referrerUserId":null})
        }
        "/api/v1/bullet-private" | "/api/v1/bullet-public" => {
            json!({"token":if request.path.ends_with("private") { "hosted-private-token" } else { "hosted-public-token" }, "instanceServers":[{"endpoint":"wss://kucoin-hosted.invalid/private","pingInterval":5000}]})
        }
        "/api/v1/market/allTickers" => json!({"ticker":[]}),
        "/api/v1/account-overview" => {
            json!({"availableBalance":0,"orderMargin":0,"accountEquity":0,"unrealisedPNL":0})
        }
        "/api/v1/orders" | "/api/v1/stopOrders" | "/api/v1/hf/orders/active/page" => {
            json!({"items":[]})
        }
        _ => json!([]),
    };
    Some(json!({"code":"200000","data":data}))
}
async fn exercise(name: ExchangeName, index: usize) {
    let label = if index == 0 { "kucoin" } else { "kucoinspot" };
    let account_path = if index == 0 {
        "/api/v1/user-info"
    } else {
        "/api/v2/user-info"
    };
    let signer = Rc::new(HostedKucoin {
        name: label,
        calls: RefCell::default(),
    });
    let h = support::venue_harness(
        name,
        Rc::new(move |request| payload(request, label)),
        json!({"key":"hosted-api-key","secret":"","passphrase":""}),
        Some((signer.clone(), Some(format!("{label}-hosted-account")))),
        safe_cex_exchange::native::native_runtime(),
    )
    .await;
    assert!(h.exchange.base().options().secret.is_empty());
    assert!(
        h.exchange.can_sign(),
        "KUCOIN HOSTED typed capability {label}"
    );
    let account = h.exchange.get_account().await.unwrap();
    assert_eq!(
        account.user_id,
        format!("{label}-hosted"),
        "KUCOIN HOSTED account read {label}"
    );
    assert_eq!(account.sub_id, Some(Some("hosted-sub".into())));
    {
        let calls = signer.calls.borrow();
        assert_eq!(
            calls.len(),
            1,
            "KUCOIN HOSTED account RPC reaches shared signer {label}"
        );
        let headers = calls[0]["config"]["headers"].as_object().unwrap();
        assert!(
            !headers
                .keys()
                .any(|key| key.eq_ignore_ascii_case("KC-API-SIGN")),
            "KUCOIN HOSTED skips local empty-secret signing {label}"
        );
    }
    h.exchange.start().await.unwrap();
    h.exchange.cancel_all_orders().await.unwrap();
    let expected = &source()["cases"][index];
    let topics: Vec<&str> = expected["frames"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|frame| frame["type"] == "subscribe")
        .map(|frame| frame["topic"].as_str().unwrap())
        .collect();
    assert_eq!(topics.len(), if index == 0 { 6 } else { 2 });
    support::eventually("hosted KuCoin private subscriptions", || {
        let frames: Vec<Value> = h
            .received
            .borrow()
            .iter()
            .map(|text| serde_json::from_str(text).unwrap())
            .collect();
        topics.iter().all(|topic| {
            frames.iter().any(|frame| {
                frame["type"] == "subscribe"
                    && frame["topic"] == *topic
                    && frame["privateChannel"] == true
                    && frame["response"] == true
            })
        })
    })
    .await;
    assert!(
        h.peers.borrow().iter().any(|peer| peer
            .path
            .starts_with("/private?token=hosted-private-token&connectId=")),
        "KUCOIN HOSTED token reaches native WS {label}"
    );
    let calls = signer.calls.borrow();
    let mut checked = vec![(account_path, 0), ("/api/v1/bullet-private", 2)];
    if index == 0 {
        checked.extend([("/api/v3/orders", 3), ("/api/v1/stopOrders", 4)]);
    } else {
        checked.push(("/api/v1/hf/orders/cancelAll", 3));
    }
    for (path, source_index) in checked {
        let selected: Vec<_> = calls
            .iter()
            .filter(|call| {
                call["config"]["url"] == path
                    && call["config"]["method"]
                        == expected["calls"][source_index]["config"]["method"]
            })
            .collect();
        assert_eq!(
            selected.len(),
            1,
            "KUCOIN HOSTED source HTTP call census {label} {path}"
        );
        let call = selected[0];
        for field in ["exchangeName", "accountName"] {
            assert_eq!(
                call[field], expected["calls"][source_index][field],
                "{label} {path} {field}"
            );
        }
        for field in ["method", "url", "baseURL"] {
            assert_eq!(
                call["config"][field], expected["calls"][source_index]["config"][field],
                "{label} {path} {field}"
            );
        }
        let wire = h.seen.borrow();
        let request = wire
            .iter()
            .find(|request| {
                request.path == path
                    && request.method
                        == expected["calls"][source_index]["config"]["method"]
                            .as_str()
                            .unwrap()
            })
            .unwrap();
        assert_eq!(
            request.headers["x-hosted-kucoin"], label,
            "KUCOIN HOSTED callback header reaches wire {label} {path}"
        );
    }
    let frames: Vec<Value> = h
        .received
        .borrow()
        .iter()
        .map(|text| serde_json::from_str(text).unwrap())
        .collect();
    for topic in &topics {
        assert_eq!(
            frames
                .iter()
                .filter(|frame| frame["type"] == "subscribe" && frame["topic"] == *topic)
                .count(),
            1,
            "KUCOIN HOSTED one source private topic {label} {topic}"
        );
    }
    h.exchange.dispose().unwrap();
}
#[test]
fn kucoin_futures_hosted_account_and_private_token_match_actual_source() {
    support::run_local(exercise(ExchangeName::Kucoin, 0));
}
#[test]
fn kucoin_spot_hosted_account_and_private_token_match_actual_source() {
    support::run_local(exercise(ExchangeName::Kucoinspot, 1));
}
