//! Hosted HTTP and the source's secretless private-WS failure are separate paths.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{Cleanup, RemoteSigner},
};
use safe_cex_transport::{
    TransportFuture,
    signing::{HttpSigningResponse, RemoteSignerRequest, RemoteSignerResponse, SignedConfig},
};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    process::Command,
    rc::Rc,
    sync::{Arc, OnceLock},
};

fn source() -> &'static Value {
    static CAPTURE: OnceLock<Value> = OnceLock::new();
    CAPTURE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/backpack_hosted_source.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "actual Backpack source: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
        eprintln!(
            "Backpack hosted source: Node {} / {} / V8 {}",
            capture["node"], capture["arch"], capture["v8"]
        );
        assert_eq!(
            capture["cases"].as_array().unwrap().len(),
            6,
            "Backpack actual source WS census"
        );
        assert_eq!(capture["calls"].as_array().unwrap().len(), 1);
        assert_eq!(capture["wire"].as_array().unwrap().len(), 2);
        capture
    })
}
#[derive(Default)]
struct HostedBackpack {
    calls: RefCell<Vec<Value>>,
}
impl safe_cex_transport::signing::RemoteSigner for HostedBackpack {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        self.calls
            .borrow_mut()
            .push(serde_json::to_value(&request).unwrap());
        let RemoteSignerRequest::Http(request) = request else {
            panic!("Backpack TS does not use hosted private WS auth")
        };
        let mut headers = request.config.headers;
        headers.insert("X-Hosted-Backpack".into(), "callback-proof".into());
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
impl RemoteSigner for HostedBackpack {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> Cleanup {
        Box::new(|| {})
    }
}
#[test]
fn backpack_registry_hosted_http_matches_actual_source_and_public_bypass() {
    support::run_local(async {
        let signer = Rc::new(HostedBackpack::default());
        let h = support::venue_harness(
            ExchangeName::Backpack,
            Rc::new(|request| match request.path.as_str() {
                "/api/v1/account" => Some(json!({"accountId":"hosted-backpack"})),
                "/api/v1/markets" => Some(json!([])),
                _ => None,
            }),
            json!({"key":"hosted-public-key","secret":""}),
            Some((signer.clone(), Some("backpack-hosted-account".into()))),
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        assert!(h.exchange.base().options().secret.is_empty());
        assert!(
            h.exchange.can_sign(),
            "BACKPACK HOSTED capability reaches typed base"
        );
        assert_eq!(
            h.exchange
                .get_account()
                .await
                .expect("BACKPACK HOSTED private HTTP uses shared signer")
                .user_id,
            "hosted-backpack"
        );
        assert!(h.exchange.fetch_markets().await.unwrap().is_empty());
        let calls = signer.calls.borrow();
        assert_eq!(calls.len(), 1, "BACKPACK HOSTED private RPC only");
        for field in ["exchangeName", "accountName"] {
            assert_eq!(calls[0][field], source()["calls"][0][field]);
        }
        for field in ["method", "url", "baseURL"] {
            assert_eq!(
                calls[0]["config"][field],
                source()["calls"][0]["config"][field]
            );
        }
        let seen = h.seen.borrow();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].path, "/api/v1/account");
        assert_eq!(seen[0].headers["x-hosted-backpack"], "callback-proof");
        assert_eq!(seen[1].path, "/api/v1/markets");
        assert!(!seen[1].headers.contains_key("x-hosted-backpack"));
        h.exchange.dispose().unwrap();
    });
}
#[test]
fn backpack_secretless_private_auth_preserves_actual_source_failure_and_log() {
    support::run_local(async {
        let runtime = safe_cex_exchange::native::native_runtime();
        let inputs = safe_cex_venue_backpack::native_host::native_inputs(
            runtime.clone(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        );
        let signer = Rc::new(HostedBackpack::default());
        let mut base = BaseExchange::new(
            ExchangeName::Backpack,
            serde_json::from_value(json!({"key":"hosted-public-key","secret":""})).unwrap(),
            Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
            runtime,
            ExchangeSettings::default(),
        );
        base.signers.remote_signer = Some(signer.clone());
        let venue = safe_cex_venue_backpack::adapter::BackpackExchange::new(
            base,
            inputs.build_http,
            inputs.install_transport,
            inputs.runtime,
            inputs.date_parse,
        )
        .unwrap();
        let errors = Rc::new(RefCell::new(vec![]));
        let capture = errors.clone();
        venue.base.on_args(
            "error",
            Rc::new(move |_, value| {
                capture.borrow_mut().push(
                    safe_cex_exchange::js_runtime::value::to_json(
                        &safe_cex_exchange::js_runtime::value::Js::array(value.to_vec()),
                    )
                    .unwrap(),
                );
                Ok(())
            }),
        );
        let socket = venue.private_websocket.get().unwrap();
        socket.authenticate().await.unwrap();
        socket.subscribe_to_order_updates(None).await.unwrap();
        socket.subscribe_to_position_updates().await.unwrap();
        assert!(socket.state.subscriptions.borrow().values().is_empty());
        assert!(
            signer.calls.borrow().is_empty(),
            "BACKPACK HOSTED private WS follows local TS signer"
        );
        let native = errors.borrow();
        assert_eq!(native.len(), 3, "BACKPACK HOSTED native WS failure census");
        for (index, method) in [
            "authenticate",
            "subscribeToOrderUpdates",
            "subscribeToPositionUpdates",
        ]
        .iter()
        .enumerate()
        {
            let expected = &source()["cases"][index];
            assert_eq!(expected["method"], *method);
            assert_eq!(expected["local"], false);
            assert_eq!(expected["reads"], 0);
            assert_eq!(expected["signCalls"], 0);
            assert_eq!(expected["frames"].as_array().unwrap().len(), 0);
            let message = expected["errors"][0].as_str().unwrap();
            assert_eq!(
                native[index][0],
                json!(format!("Exchange Error: {message}")),
                "BACKPACK HOSTED source error identity and log {method}"
            );
            let positive = &source()["cases"][index + 3];
            assert_eq!(positive["method"], *method);
            assert_eq!(positive["local"], true);
            assert_eq!(positive["frames"].as_array().unwrap().len(), 1);
            assert!(positive["errors"].as_array().unwrap().is_empty());
        }
        venue.dispose().unwrap();
    });
}

#[test]
fn backpack_constructed_subscription_timer_drains_fires_and_cancels_on_replay_clock() {
    use safe_cex_exchange::{
        native::{native_replay_runtime, replay_raw_tasks},
        replay_clock,
    };
    support::run_local(async {
        let runtime = native_replay_runtime(1_800_000_000_000.0);
        let h = support::venue_harness(ExchangeName::Backpack, Rc::new(|r| match r.path.as_str() {
            "/api/v1/markets" => Some(json!([{"symbol":"BTC_USDC","marketType":"SPOT","visible":true,"filters":{"price":{"tickSize":"0.1","minPrice":"0.1","maxPrice":"1000000"},"quantity":{"stepSize":"0.001","minQuantity":"0.001","maxQuantity":"100"}}}])),
            "/api/v1/tickers" => Some(json!([{"symbol":"BTC_USDC","lastPrice":"100","priceChangePercent":"0","volume":"1","quoteVolume":"100"}])),
            "/api/v1/markPrices" => Some(json!([])),
            _ => None,
        }), json!({"key":"","secret":"","publicDataOnly":true}), None, runtime.clone()).await;
        h.exchange.start().await.unwrap();
        support::eventually("Backpack socket", || !h.peers.borrow().is_empty()).await;
        for _ in 0..32 {
            tokio::task::yield_now().await;
        }
        let receipt = replay_raw_tasks(&runtime);
        assert_eq!(
            receipt["unknown"], 0,
            "Backpack constructed startup timer must use core ownership: {receipt}"
        );
        assert_eq!(receipt["blocking"], 0, "Backpack startup drain: {receipt}");
        replay_clock::activate(runtime.replay_clock.as_ref().unwrap()).unwrap();
        replay_clock::advance(runtime.replay_clock.as_ref().unwrap(), 2000.0)
            .await
            .unwrap();
        support::eventually("Backpack delayed market subscription", || {
            h.received
                .borrow()
                .iter()
                .any(|s| s.contains("ticker.BTC_USDC"))
        })
        .await;
        h.exchange.dispose().unwrap();
        for _ in 0..32 {
            tokio::task::yield_now().await;
        }
        let before = h.received.borrow().len();
        replay_clock::advance(runtime.replay_clock.as_ref().unwrap(), 12_000.0)
            .await
            .unwrap();
        for _ in 0..32 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            h.received.borrow().len(),
            before,
            "disposed subscription timer never runs"
        );
        assert_eq!(replay_raw_tasks(&runtime)["blocking"], 0);
    });
}
