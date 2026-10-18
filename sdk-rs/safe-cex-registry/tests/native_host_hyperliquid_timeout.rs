#![cfg(not(target_arch = "wasm32"))]
mod support;

use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_registry::{native_host::native_venue_host_with, rust_venues::rust_constructor};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_transport::{RestAdapter, RestFuture, RestRequest, RestResponse};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use support::{LocalOrigin, run_local, serve_http};
use tokio::net::TcpListener;

const KEY: &str = "0x0123456789012345678901234567890123456789012345678901234567890123";
const ADDRESS: &str = "0x14791697260e4c9a71f18484c9f997b308e59325";

struct ObserveTimeout {
    inner: LocalOrigin,
    timeouts: Rc<RefCell<Vec<Option<i64>>>>,
}
impl RestAdapter for ObserveTimeout {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.timeouts.borrow_mut().push(request.timeout_ms);
        self.inner.execute(request)
    }
}

#[expect(clippy::arc_with_non_send_sync)]
async fn timeout_for(window: Option<f64>) -> Option<i64> {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let seen = Rc::new(RefCell::new(vec![]));
    tokio::task::spawn_local(serve_http(
        listener,
        Rc::new(|request| {
            assert_eq!(request.path, "/exchange");
            Some(json!({"status":"ok","response":{"type":"default"}}))
        }),
        seen.clone(),
    ));
    let timeouts = Rc::new(RefCell::new(vec![]));
    let observed = timeouts.clone();
    let adapters = Rc::new(move || {
        let inner = safe_cex_transport::native::reqwest_adapter()
            .map_err(|error| ExchangeError::new(error.to_string()))?;
        Ok(Arc::new(ObserveTimeout {
            inner: LocalOrigin {
                inner,
                origin: origin.clone(),
            },
            timeouts: observed.clone(),
        }) as Arc<dyn RestAdapter>)
    });
    let host = native_venue_host_with(
        safe_cex_exchange::native::native_runtime(),
        adapters,
        safe_cex_registry::native_host::hyperliquid_rate_limiter(),
    );
    let mut options = json!({"key":ADDRESS,"secret":KEY,"applicationId":ADDRESS});
    if let Some(window) = window {
        options["extra"] = json!({"recvWindow":window});
    }
    let store: Rc<dyn Store> = Rc::new(DefaultStore::new());
    let exchange = rust_constructor(&ExchangeName::Hyperliquid, host)
        .unwrap()
        .construct(
            ExchangeName::Hyperliquid,
            serde_json::from_value(options).unwrap(),
            store.clone(),
        )
        .unwrap();
    store
        .update(
            StoreUpdate {
                markets: Some(vec![
                    serde_json::from_value(json!({
                        "id":"BTC","symbol":"BTC-PERP","base":"BTC","quote":"USDC","active":true,
                        "precision":{"amount":0.00001,"price":1},
                        "limits":{"amount":{"min":0.00001,"max":100},"leverage":{"min":1,"max":40}},
                        "info":{"hyperliquidId":0}
                    }))
                    .unwrap(),
                ]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    exchange
        .set_leverage("BTC-PERP".into(), 7.0, None)
        .await
        .unwrap();
    exchange.dispose().unwrap();
    let seen = seen.borrow();
    assert_eq!(
        seen.len(),
        1,
        "the real signed write reached HTTP exactly once"
    );
    let body: Value = serde_json::from_str(&seen[0].body).unwrap();
    assert_eq!(body["action"]["type"], "updateLeverage");
    assert_eq!(body["action"]["leverage"].as_f64(), Some(7.0));
    let signature = serde_json::to_value(
        safe_cex_venue_hyperliquid::signing::sign_l1_action(
            KEY,
            &body["action"],
            None,
            &body["nonce"],
            true,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(body["signature"]["r"], signature["r"]);
    assert_eq!(body["signature"]["s"], signature["s"]);
    assert_eq!(body["signature"]["v"].as_f64(), signature["v"].as_f64());
    let timeouts = timeouts.borrow();
    assert_eq!(timeouts.len(), 1);
    timeouts[0]
}

#[test]
fn short_recv_window_does_not_shorten_the_native_http_timeout() {
    run_local(async {
        assert_eq!(timeout_for(Some(5000.0)).await, Some(20_000));
    });
}
#[test]
fn long_recv_window_does_not_extend_the_native_http_timeout() {
    run_local(async {
        assert_eq!(timeout_for(Some(60_000.0)).await, Some(20_000));
    });
}
#[test]
fn absent_recv_window_uses_the_native_factory_timeout() {
    run_local(async {
        assert_eq!(timeout_for(None).await, Some(20_000));
    });
}
