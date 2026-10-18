#![cfg(not(target_arch = "wasm32"))]

use safe_cex_exchange::{
    exchange::Exchange,
    native::{NativeHttpHost, native_runtime},
    native_store::DefaultStore,
    socket_lifecycle::SocketSlot,
    websocket::WsRuntime,
};
use safe_cex_store::{Store, StoreUpdate, virtual_clock::VIRTUAL_CLOCK};
use safe_cex_transport::rate_limit::RateLimitClock;
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::ExchangeName;
use safe_cex_venue_bybit::{BybitDependencies, BybitExchange};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

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
        name.clone(),
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
    let slot = if name == ExchangeName::Bybitinverse {
        SocketSlot::PublicSecondary
    } else {
        SocketSlot::Public
    };
    let socket = venue.base().get_socket(slot).unwrap();
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ticker-prototype-source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut observations = 0;
    for case in cases.as_array().unwrap() {
        if case["variant"] != serde_json::to_value(&name).unwrap() {
            continue;
        }
        let tickers = serde_json::from_value(case["tickers"].clone()).unwrap();
        Store::update(
            store.as_ref(),
            StoreUpdate {
                tickers: Some(tickers),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let count = Rc::new(Cell::new(0));
        let seen = count.clone();
        let previous = RefCell::new(
            store
                .tickers()
                .unwrap()
                .iter()
                .map(|ticker| (ticker.last.to_bits(), ticker.bid.to_bits()))
                .collect::<Vec<_>>(),
        );
        let unsubscribe = Store::subscribe(
            store.as_ref(),
            Box::new(move |state, _| {
                let current: Vec<_> = state
                    .tickers
                    .iter()
                    .map(|ticker| (ticker.last.to_bits(), ticker.bid.to_bits()))
                    .collect();
                // The socket also notifies handler statistics; count ticker commits.
                if current != *previous.borrow() {
                    seen.set(seen.get() + 1);
                    *previous.borrow_mut() = current;
                }
                Ok(())
            }),
        );
        let result = socket.on_message(&SocketFrame::Text(case["frame"].to_string()));
        assert_eq!(
            result.as_ref().err().map(|e| e.name.as_str()),
            case["error"]["name"].as_str(),
            "BYBIT-TICKER-PROTOTYPE error class: {case}"
        );
        assert_eq!(
            result.err().map(|e| e.message),
            case["error"]["message"].as_str().map(str::to_owned),
            "BYBIT-TICKER-PROTOTYPE parse precedes update: {case}"
        );
        assert_eq!(
            count.get(),
            case["notifications"].as_u64().unwrap(),
            "{case}"
        );
        let tickers = store.tickers().unwrap();
        for (ticker, expected) in tickers.iter().zip(case["result"].as_array().unwrap()) {
            assert_eq!(ticker.last, expected["last"].as_f64().unwrap(), "{case}");
            assert_eq!(ticker.bid, expected["bid"].as_f64().unwrap(), "{case}");
        }
        assert_eq!(tickers.len(), case["result"].as_array().unwrap().len());
        drop(tickers);
        unsubscribe();
        observations += 1;
    }
    assert_eq!(observations, 48, "source case census");
    venue.dispose().unwrap();
}

#[test]
fn constructed_public_socket_uses_source_ticker_prototype_lookup() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, async {
        for name in [
            ExchangeName::Bybit,
            ExchangeName::Bybitdemo,
            ExchangeName::Bybitinverse,
            ExchangeName::Bybitspot,
        ] {
            observe(name).await;
        }
    });
}
