//! Account refresh through each constructed Exchange and signed HTTP host.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};

fn position(symbol: &str) -> Value {
    json!({"symbol":symbol,"side":"Buy","size":"2","avgPrice":"80","positionValue":"160","leverage":"5","unrealisedPnl":"3","curRealisedPnl":"-1","liqPrice":"50","positionIdx":0,"positionIM":"32","updatedTime":"1700000000000","seq":7,"positionStatus":"Normal"})
}
fn snapshot(name: ExchangeName) {
    let symbol = if name == ExchangeName::Bybitinverse {
        "BTCUSD"
    } else {
        "BTCUSDT"
    };
    let h = support::harness_as(name.clone(), support::options(), |wire| {
        wire.ok(
            "GET",
            "/v5/order/realtime",
            json!({"list":[],"nextPageCursor":""}),
        );
        wire.ok(
            "GET",
            "/v5/position/list",
            json!({"list":[position(symbol)],"nextPageCursor":""}),
        );
        wire.ok("GET", "/v5/account/wallet-balance", json!({"list":[{"accountType":"UNIFIED","totalEquity":"100","totalWalletBalance":"100","totalAvailableBalance":"80","totalInitialMargin":"15","totalMaintenanceMargin":"5","totalPerpUPL":"3","coin":[]}]}));
    });
    Store::update(h.store.as_ref(), StoreUpdate {
        markets: Some(vec![serde_json::from_value(json!({"id":symbol,"symbol":symbol,"base":"BTC","quote":"USD","active":true,"precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":1000},"leverage":{"min":1,"max":100}}})).unwrap()]),
        ..Default::default()
    }, false).unwrap();
    h.drive(h.venue().fetch_and_set_private_snapshot(10))
        .unwrap();
    assert_eq!(
        h.wire.calls("/v5/account/wallet-balance").len(),
        1,
        "venue resync must refresh balance"
    );
    assert_eq!(
        h.store.balance().total,
        if name == ExchangeName::Bybitinverse {
            97.0
        } else {
            100.0
        }
    );
    assert!(h.store.loaded().orders && h.store.loaded().positions && h.store.loaded().balance);
    if name == ExchangeName::Bybitspot {
        assert!(h.wire.calls("/v5/position/list").is_empty());
        assert!(h.store.positions().is_empty());
    } else {
        assert!(!h.wire.calls("/v5/position/list").is_empty());
        assert!(
            h.store
                .positions()
                .iter()
                .any(|p| p.symbol == symbol && p.entry_price == 80.0)
        );
    }
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "{:?}",
        h.runtime.reports.borrow()
    );
    h.venue().dispose().unwrap();
}
#[test]
fn primary_snapshot_refreshes_orders_positions_balance() {
    snapshot(ExchangeName::Bybit);
}
#[test]
fn demo_snapshot_refreshes_orders_positions_balance() {
    snapshot(ExchangeName::Bybitdemo);
}
#[test]
fn inverse_snapshot_refreshes_orders_positions_balance() {
    snapshot(ExchangeName::Bybitinverse);
}
#[test]
fn spot_snapshot_refreshes_orders_and_balance() {
    snapshot(ExchangeName::Bybitspot);
}
