#![cfg(not(target_arch = "wasm32"))]
#[path = "support/native_account.rs"]
mod account;
mod support;
use safe_cex_registry::native_account::native_account_watch;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc, time::Duration};

#[test]
fn startup_forces_the_complete_worker_snapshot_and_order_updates_strip_info() {
    account::run(async {
        let h = account::opened(json!({"hostedMode":true})).await;
        let posts = Rc::new(RefCell::new(Vec::<Value>::new()));
        let captured = posts.clone();
        let key_orders = Rc::new(RefCell::new(Vec::new()));
        let capture_keys = key_orders.clone();
        let unwatch = native_account_watch(
            &h.account,
            Rc::new(|_, _| {}),
            Rc::new(move |data| {
                if let safe_cex_registry::native_value::NativeValue::Object(fields) = &data {
                    capture_keys.borrow_mut().push(
                        fields
                            .iter()
                            .map(|(key, _)| String::from_utf16(key).unwrap())
                            .collect::<Vec<_>>(),
                    );
                }
                captured.borrow_mut().push(support::boundary_json(data));
            }),
        );
        h.ok("start", json!([])).await;
        {
            let posts = posts.borrow();
            let snapshot = posts.last().expect("post-start Store snapshot");
            let source = std::process::Command::new("node")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/native_store_order_source.mjs"
                ))
                .output()
                .unwrap();
            assert!(
                source.status.success(),
                "{}",
                String::from_utf8_lossy(&source.stderr)
            );
            let source: Value = serde_json::from_slice(&source.stdout).unwrap();
            assert_eq!(
                key_orders
                    .borrow()
                    .last()
                    .unwrap()
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                source["snapshot"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect::<Vec<_>>(),
                "STORE-CONSTRUCTION-ORDER actual account full snapshot"
            );
            assert_eq!(snapshot["markets"][0]["symbol"], "BTCUSDT");
            assert_eq!(snapshot["tickers"][0]["last"], 10.0);
            for key in [
                "orders",
                "positions",
                "displayedPositions",
                "activeChasers",
                "activeTWAPs",
            ] {
                assert!(snapshot[key].is_array(), "{key}: {snapshot}");
            }
            assert!(snapshot["socketHealth"]["publicHealthy"].is_boolean());
            assert!(snapshot["messageHandlerStats"].is_object());
            assert_eq!(snapshot["loaded"]["orders"], true);
        }
        h.ok("fetchAndSetOrders", json!([])).await;
        tokio::time::sleep(Duration::from_millis(350)).await;
        {
            let posts = posts.borrow();
            let snapshot = posts
                .iter()
                .rev()
                .find(|post| post["orders"][0]["id"] == "77")
                .expect("native signed HTTP order reaches Store delivery");
            assert!(snapshot["orders"][0].get("info").is_none());
            assert!(snapshot.get("markets").is_none());
            assert_eq!(snapshot["tickers"][0]["symbol"], "BTCUSDT");
        }
        unwatch();
        h.ok("dispose", json!([])).await;
    });
}

#[test]
fn gateway_bursts_are_throttled_and_latest_ticker_snapshot_is_delivered() {
    account::run(async {
        let h = account::opened(json!({"tuning":{"dataUpdateThrottleMultiplier":2}})).await;
        let posts = Rc::new(RefCell::new(Vec::<Value>::new()));
        let captured = posts.clone();
        let unwatch = native_account_watch(
            &h.account,
            Rc::new(|_, _| {}),
            Rc::new(move |data| {
                captured.borrow_mut().push(support::boundary_json(data));
            }),
        );
        h.ok("start", json!([])).await;
        let initial = posts.borrow().len();
        for last in 11..=16 {
            h.peers.borrow()[0].push.send(json!({"type":"data","exchangeId":"binance","channel":"tickers","data":[{
                "id":"BTCUSDT","symbol":"BTCUSDT","bid":9.9,"ask":10.1,"last":last,"mark":10,"index":10,
                "percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":1,"quoteVolume":10
            }]}).to_string()).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(120)).await;
        assert_eq!(posts.borrow().len() - initial, 1, "leading update only");
        tokio::time::sleep(Duration::from_millis(600)).await;
        {
            let posts = posts.borrow();
            assert_eq!(posts.len() - initial, 2, "one latest-state trailing update");
            let latest = posts.last().unwrap();
            assert_eq!(latest["tickers"][0]["last"], 16.0);
            for omitted in ["markets", "orders", "positions"] {
                assert!(latest.get(omitted).is_none(), "{omitted}");
            }
        }
        unwatch();
        h.ok("dispose", json!([])).await;
    });
}
