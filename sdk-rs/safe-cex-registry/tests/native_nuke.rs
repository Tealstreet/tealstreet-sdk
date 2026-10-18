//! Shared nuke through the registry's real OKX binding, native host and loopback HTTP.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_store::StoreUpdate;
use safe_cex_types::{ExchangeName, Market, Position};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
use support::{Harness, Seen, run_local, venue_harness};

fn market(size: f64, inverse: bool) -> Market {
    serde_json::from_value(json!({
        "id":"BTC-USDT-SWAP","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,
        "contractSize":size,"isInverse":inverse,"precision":{"amount":1,"price":0.1},
        "limits":{"amount":{"min":1,"max":100000},"leverage":{"min":1,"max":100}}
    }))
    .unwrap()
}
fn position(symbol: &str, contracts: f64, notional: f64, native: Option<f64>) -> Position {
    let mut value = json!({"id":format!("{symbol}:long"),"symbol":symbol,"side":"long",
        "entryPrice":50000,"notional":notional,"leverage":10,"unrealizedPnl":0,
        "contracts":contracts,"liquidationPrice":0});
    if let Some(native) = native {
        value["extra"] = json!({"contractsNative":native});
    }
    serde_json::from_value(value).unwrap()
}
async fn fixture(cancel_error: bool) -> Harness {
    venue_harness(ExchangeName::Okx, Rc::new(move |request: &Seen| {
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/api/v5/account/config") => Some(json!({"code":"0","msg":"","data":[{"posMode":"net_mode","acctLv":"2"}]})),
            ("POST", "/api/v5/trade/batch-orders") => Some(json!({"code":"0","msg":"","data":[{"ordId":"close-1","sCode":"0","sMsg":""}]})),
            ("POST", "/api/v5/trade/cancel-batch-orders") => Some(if cancel_error {
                json!({"code":"1","msg":"cancel refused","data":[{"ordId":"resting","sCode":"51400","sMsg":"cancel refused"}]})
            } else { json!({"code":"0","msg":"","data":[{"ordId":"resting","sCode":"0","sMsg":""}]}) }),
            _ => panic!("unexpected real OKX request: {request:?}"),
        }
    }), json!({"key":"fixture-key","secret":"fixture-secret","passphrase":"fixture-passphrase","tuning":{"consoleLogging":false}}), None,
    safe_cex_exchange::native::native_runtime()).await
}
fn seed(h: &Harness, market: Market, positions: Vec<Position>, resting: bool) {
    h.store.update(StoreUpdate { markets:Some(vec![market]), positions:Some(positions),
        orders:Some(if resting {vec![serde_json::from_value(json!({"id":"resting","symbol":"BTCUSDT","status":"open","type":"limit","side":"buy","price":40000,"amount":1,"filled":0,"remaining":1,"reduceOnly":false,"lastUpdate":0})).unwrap()]} else {vec![]}), ..StoreUpdate::default() }, false).unwrap();
}
fn placed(h: &Harness) -> Vec<Value> {
    h.seen
        .borrow()
        .iter()
        .filter(|r| r.path == "/api/v5/trade/batch-orders")
        .flat_map(|r| serde_json::from_str::<Vec<Value>>(&r.body).unwrap())
        .collect()
}
fn logs(h: &Harness) -> Rc<RefCell<Vec<String>>> {
    let rows = Rc::new(RefCell::new(vec![]));
    let captured = rows.clone();
    h.exchange.base().on(
        "log",
        Rc::new(move |_, message| {
            captured
                .borrow_mut()
                .push(String::from_utf16(&message.to_js_string()?).unwrap());
            Ok(())
        }),
    );
    rows
}

#[test]
fn native_nuke_positions_sizes_linear_and_inverse_closes_in_contracts() {
    run_local(async {
        for (inverse, size, contracts, notional, native, expected) in [
            (false, 0.0001, 0.0012, 60.0, None, "12"),
            (true, 100.0, 0.2, 10000.0, Some(90.0), "100"),
            (false, 0.01, 0.2, 10000.0, Some(30.0), "30"),
        ] {
            let h = fixture(false).await;
            seed(
                &h,
                market(size, inverse),
                vec![position("BTCUSDT", contracts, notional, native)],
                true,
            );
            h.exchange.nuke_positions(3).await.unwrap();
            let orders = placed(&h);
            assert_eq!(
                orders.len(),
                1,
                "one real close reaches OKX for {inverse}/{size}"
            );
            assert_eq!(
                orders[0]["sz"], expected,
                "native contract count, not base units"
            );
            assert_eq!(orders[0]["side"], "sell");
            assert_eq!(orders[0]["ordType"], "market");
            assert_eq!(orders[0]["reduceOnly"], true);
            assert!(
                h.seen.borrow().iter().all(|r| !r.path.contains("cancel")),
                "nukePositions does not cancel"
            );
            assert_eq!(
                h.store.positions().len(),
                1,
                "shared nuke waits for source Store updates"
            );
            h.exchange.dispose().unwrap();
        }
    });
}
#[test]
fn native_nuke_cancels_resting_orders_before_any_close() {
    run_local(async {
        let h = fixture(false).await;
        seed(
            &h,
            market(1.0, false),
            vec![position("BTCUSDT", 3.0, 150000.0, None)],
            true,
        );
        h.exchange.nuke(3).await.unwrap();
        let paths: Vec<_> = h
            .seen
            .borrow()
            .iter()
            .filter(|r| r.method == "POST")
            .map(|r| r.path.clone())
            .collect();
        assert_eq!(
            paths,
            vec![
                "/api/v5/trade/cancel-batch-orders",
                "/api/v5/trade/batch-orders"
            ],
            "cancel must settle before close can reach the network"
        );
        assert_eq!(placed(&h)[0]["sz"], "3");
        h.exchange.dispose().unwrap();
    });
}
#[test]
fn native_nuke_logs_cancel_failure_and_still_closes() {
    run_local(async {
        let h = fixture(true).await;
        seed(
            &h,
            market(1.0, false),
            vec![position("BTCUSDT", 3.0, 150000.0, None)],
            true,
        );
        let messages = logs(&h);
        h.exchange
            .nuke(3)
            .await
            .expect("failed cancellation must not abort closes");
        assert_eq!(placed(&h).len(), 1);
        assert!(
            messages
                .borrow()
                .iter()
                .any(|m| m.contains("Nuke: failed to cancel orders:")
                    && m.contains("cancel refused")),
            "cancel failure must be visible: {:?}",
            messages.borrow()
        );
        let paths: Vec<_> = h
            .seen
            .borrow()
            .iter()
            .filter(|r| r.method == "POST")
            .map(|r| r.path.clone())
            .collect();
        assert_eq!(
            paths,
            vec![
                "/api/v5/trade/cancel-batch-orders",
                "/api/v5/trade/batch-orders"
            ]
        );
        h.exchange.dispose().unwrap();
    });
}
#[test]
fn native_nuke_missing_market_logs_and_keeps_good_siblings() {
    run_local(async {
        let h = fixture(false).await;
        seed(
            &h,
            market(1.0, false),
            vec![
                position("UNLOADED", 2.0, 1.0, None),
                position("BTCUSDT", 3.0, 150000.0, None),
            ],
            false,
        );
        let messages = logs(&h);
        h.exchange.nuke_positions(3).await.unwrap();
        assert!(
            messages.borrow().iter().any(
                |m| m.contains("Nuke: failed to close position on UNLOADED: market not loaded")
            ),
            "missing market refusal: {:?}",
            messages.borrow()
        );
        assert_eq!(placed(&h).len(), 1);
        assert_eq!(placed(&h)[0]["instId"], "BTC-USDT-SWAP");
        h.exchange.dispose().unwrap();
    });
}

#[test]
fn native_nuke_source_oracle() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/nuke-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual maintained TS: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("6 cases PASS"));
}

#[test]
fn native_nuke_dropped_close_does_not_latch_cancels() {
    run_local(async {
        let h = fixture(false).await;
        seed(
            &h,
            market(1.0, false),
            vec![position("BTCUSDT", 3.0, 150000.0, None)],
            true,
        );
        let mut close = Box::pin(h.exchange.nuke_positions(3));
        assert!(
            std::future::Future::poll(
                close.as_mut(),
                &mut std::task::Context::from_waker(std::task::Waker::noop())
            )
            .is_pending(),
            "real HTTP close remains pending on first poll"
        );
        drop(close);
        h.exchange
            .base()
            .nuke_orders(h.exchange.as_ref())
            .await
            .unwrap();
        assert_eq!(
            h.seen
                .borrow()
                .iter()
                .filter(|r| r.path == "/api/v5/trade/cancel-batch-orders")
                .count(),
            1,
            "dropping a pending close must release NukeGuard for later public calls"
        );
        h.exchange.dispose().unwrap();
    });
}
