#![cfg(not(target_arch = "wasm32"))]

use safe_cex_exchange::{
    exchange::Exchange,
    native::{NativeHttpHost, native_runtime},
    native_store::DefaultStore,
    socket_lifecycle::SocketSlot,
    websocket::WsRuntime,
};
use safe_cex_store::{Store, virtual_clock::VIRTUAL_CLOCK};
use safe_cex_transport::rate_limit::RateLimitClock;
use safe_cex_types::{ExchangeName, StoreSubscriberError};
use safe_cex_venue_bybit::{BybitDependencies, BybitExchange, ws_private, ws_public};
use serde_json::json;
use std::{cell::Cell, rc::Rc, sync::Arc};

struct RuntimeClock(Rc<dyn WsRuntime>);
impl RateLimitClock for RuntimeClock {
    fn now_ms(&self) -> u64 {
        self.0.now_ms() as u64
    }
    fn sleep_ms(&self, ms: u64) -> safe_cex_transport::TransportFuture<'_, ()> {
        Box::pin(async move {
            self.0
                .sleep(ms)
                .await
                .map_err(|error| safe_cex_transport::TransportError::Host(error.message))
        })
    }
}

#[allow(
    clippy::arc_with_non_send_sync,
    reason = "native rate clock runs on the LocalSet"
)]
async fn observe(name: ExchangeName) {
    let runtime = native_runtime();
    let store = Rc::new(DefaultStore::new());
    let venue = BybitExchange::create(
        name,
        serde_json::from_value(json!({"key":"", "secret":"", "tuning":{"consoleLogging":false}}))
            .unwrap(),
        store.clone(),
        BybitDependencies {
            runtime: runtime.clone(),
            remote_signer: None,
            wire: Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
            http_host: Rc::new(NativeHttpHost::default()),
            rate_limit_clock: Arc::new(RuntimeClock(runtime.clone())),
            retry_clock: Rc::new(RuntimeClock(runtime.clone())),
            clock: VIRTUAL_CLOCK.clone(),
            external_public_data: None,
            relay_enabled: false,
        },
    )
    .unwrap();
    let public = venue.base().get_socket(SocketSlot::Public).unwrap();
    let private = venue.base().get_socket(SocketSlot::Private).unwrap();
    assert!(!public.has_websocket() && !private.has_websocket());
    let first = Rc::new(Cell::new(0));
    let second = Rc::new(Cell::new(0));
    let seen = first.clone();
    let unsubscribe_first = Store::subscribe(
        store.as_ref(),
        Box::new(move |_, _| {
            seen.set(seen.get() + 1);
            Err(StoreSubscriberError::new(
                "native reconnect subscriber refused",
            ))
        }),
    );
    let seen = second.clone();
    let unsubscribe_second = Store::subscribe(
        store.as_ref(),
        Box::new(move |_, _| {
            seen.set(seen.get() + 1);
            Ok(())
        }),
    );
    let topics = json!({"tickers":(0..11).map(|i| format!("tickers.{i}")).collect::<Vec<_>>()});
    ws_public::subscribe(&public, topics.as_object().unwrap(), runtime.as_ref())
        .await
        .expect("BYBIT-DETACHED public subscribe must finish both chunks");
    assert_eq!(
        first.get(),
        2,
        "two chunks commit before their detached rejections"
    );
    ws_private::subscribe(&private).expect("BYBIT-DETACHED private subscribe must return normally");
    ws_public::send_ohlcv_diagnostic_payload(&public, "unsubscribe", &["kline.1.BTCUSDT".into()])
        .expect("BYBIT-DETACHED diagnostic caller must return normally");
    assert_eq!(first.get(), 4);
    assert_eq!(
        second.get(),
        0,
        "throwing subscriber stops later subscribers"
    );
    assert!(
        store
            .socket_health()
            .unwrap()
            .as_ref()
            .and_then(safe_cex_types::retained_json::RetainedJsonInput::as_object)
            .is_some_and(|health| !health.is_empty()),
        "health commits before notification throws"
    );
    // Both no-socket ping paths return before reconnect or Store notification.
    ws_public::ping(&public).unwrap();
    ws_private::ping(&private).unwrap();
    assert_eq!(first.get(), 4);
    for _ in 0..4 {
        tokio::task::yield_now().await;
    }
    unsubscribe_first();
    unsubscribe_second();
    venue.dispose().unwrap();
    println!("BYBIT-DETACHED native observations PASS");
}

#[test]
fn constructed_bybit_callers_continue_and_native_reporter_receives_rejections() {
    if let Ok(name) = std::env::var("BYBIT_DETACHED_NATIVE_CHILD") {
        let name = serde_json::from_value(json!(name)).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&runtime, observe(name));
        return;
    }
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/check-store-notification.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    for name in ["bybit", "bybitdemo"] {
        let native = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "constructed_bybit_callers_continue_and_native_reporter_receives_rejections",
                "--nocapture",
            ])
            .env("BYBIT_DETACHED_NATIVE_CHILD", name)
            .output()
            .unwrap();
        let stderr = String::from_utf8(native.stderr).unwrap();
        assert!(
            native.status.success(),
            "{name}: {stderr}\n{}",
            String::from_utf8_lossy(&native.stdout)
        );
        assert_eq!(
            stderr
                .matches("[safe-cex uncaught] Error: native reconnect subscriber refused")
                .count(),
            4,
            "{name}: {stderr}"
        );
        assert_eq!(
            stderr
                .matches("\"operation\":\"websocket unawaited reconnect\"")
                .count(),
            4,
            "{name}: {stderr}"
        );
    }
}
