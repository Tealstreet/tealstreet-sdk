//! Closed history through the production Exchange receiver and signed HTTP client.
#[path = "support/binding_harness.rs"]
mod support;

use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use support::{harness_as, options};

fn row(symbol: &str, updated: &str) -> Value {
    json!({"symbol":symbol,"createdTime":"1700000000000","updatedTime":updated,
        "closedSize":"1000","avgEntryPrice":"40000","avgExitPrice":"50000",
        "cumExitValue":"50000000","closedPnl":"12.5","openFee":"1.2",
        "closeFee":"0.8","side":"Sell","leverage":"10"})
}

fn closed_pnl_case(name: ExchangeName, symbol: &str, category: &str, inverse: bool) {
    let h = harness_as(name, options(), |wire| {
        wire.ok(
            "GET",
            "/v5/position/closed-pnl",
            json!({"list":[row(symbol,"1700000001000")]}),
        );
    });
    let opts = serde_json::from_value(json!({"symbol":symbol,"from":1700000000000u64,
        "to":1700000002000u64,"accountName":"history account"}))
    .unwrap();
    let rows = h.drive(h.venue().fetch_closed_pnl(opts)).unwrap();
    assert_eq!(rows.len(), 1, "closed PnL must reach the venue HTTP member");
    let projected = serde_json::to_value(&rows[0]).unwrap();
    assert_eq!(projected["symbol"], symbol);
    assert_eq!(projected["provenance"], "venue");
    assert_eq!(projected["side"], "long");
    assert_eq!(projected["account"], "history account");
    assert_eq!(projected["fees"], 2.0);
    assert_eq!(projected["contracts"], if inverse { 0.02 } else { 1000.0 });
    assert_eq!(
        projected["notional"],
        if inverse { 1000.0 } else { 50000000.0 }
    );
    let calls = h.wire.calls("/v5/position/closed-pnl");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].params["category"], category);
    assert_eq!(calls[0].params["symbol"], symbol);
    assert_eq!(calls[0].params["limit"], "100");
    assert_eq!(calls[0].params["startTime"], "1700000000000");
    assert_eq!(calls[0].params["endTime"], "1700000002000");
    assert!(calls[0].params.get("cursor").is_none());
    h.venue().dispose().unwrap();
}

#[test]
fn primary_closed_pnl_reaches_http_and_maps_linear_rows() {
    closed_pnl_case(ExchangeName::Bybit, "BTCUSDT", "linear", false);
}
#[test]
fn demo_closed_pnl_reaches_http_and_maps_inverse_rows() {
    closed_pnl_case(ExchangeName::Bybitdemo, "BTCUSD", "inverse", true);
}
#[test]
fn inverse_closed_pnl_reaches_http_and_maps_coin_contracts() {
    closed_pnl_case(ExchangeName::Bybitinverse, "BTCUSD", "inverse", true);
}
#[test]
fn spot_retains_inherited_empty_closed_pnl() {
    let h = harness_as(ExchangeName::Bybitspot, options(), |_| {});
    let opts = serde_json::from_value(json!({"symbol":"BTCUSDT"})).unwrap();
    assert!(
        h.drive(h.venue().fetch_closed_pnl(opts))
            .unwrap()
            .is_empty()
    );
    assert!(h.wire.calls("/v5/position/closed-pnl").is_empty());
    h.venue().dispose().unwrap();
}

#[test]
fn closed_pnl_keeps_pages_windows_and_descending_order() {
    for name in [
        ExchangeName::Bybit,
        ExchangeName::Bybitdemo,
        ExchangeName::Bybitinverse,
    ] {
        let h = harness_as(name, options(), |wire| {
            wire.ok(
                "GET",
                "/v5/position/closed-pnl",
                json!({"list":[row("BTCUSD","1700000001000")],"nextPageCursor":"next"}),
            );
            wire.ok(
                "GET",
                "/v5/position/closed-pnl",
                json!({"list":[row("BTCUSD","1700000003000")]}),
            );
            wire.ok(
                "GET",
                "/v5/position/closed-pnl",
                json!({"list":[row("BTCUSD","1700000002000")]}),
            );
        });
        let opts =
            serde_json::from_value(json!({"symbol":"BTCUSD","from":0,"to":604800001})).unwrap();
        let rows = h.drive(h.venue().fetch_closed_pnl(opts)).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.closed_time).collect::<Vec<_>>(),
            vec![1700000003000., 1700000002000., 1700000001000.]
        );
        let calls = h.wire.calls("/v5/position/closed-pnl");
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].params["startTime"], "2");
        assert_eq!(calls[0].params["endTime"], "604800001");
        assert_eq!(calls[1].params["cursor"], "next");
        assert_eq!(calls[2].params["startTime"], "0");
        assert_eq!(calls[2].params["endTime"], "1");
        assert!(calls[2].params.get("cursor").is_none());
        h.venue().dispose().unwrap();
    }
}

#[test]
fn closed_pnl_page_cap_warns_with_symbol_and_keeps_twenty_pages() {
    let h = harness_as(ExchangeName::Bybit, options(), |wire| {
        wire.ok(
            "GET",
            "/v5/position/closed-pnl",
            json!({"list":[row("BTCUSDT","1700000001000")],"nextPageCursor":"more"}),
        );
    });
    let opts = serde_json::from_value(json!({"symbol":"BTCUSDT","from":1,"to":2})).unwrap();
    assert_eq!(h.drive(h.venue().fetch_closed_pnl(opts)).unwrap().len(), 20);
    assert_eq!(h.wire.calls("/v5/position/closed-pnl").len(), 20);
    assert!(h.runtime.logs.borrow().iter().any(|s| s == "warning: Closed-PnL paging hit the 20-page cap for BTCUSDT; older rows in this window were not fetched"));
    h.venue().dispose().unwrap();
}

#[test]
fn closed_pnl_rejects_options_before_http_but_public_only_returns_empty() {
    let symbol = "BTC-27DEC30-50000-C";
    for name in [ExchangeName::Bybit, ExchangeName::Bybitdemo] {
        let h = harness_as(name, options(), |_| {});
        let opts = serde_json::from_value(json!({"symbol":symbol})).unwrap();
        let err = h.drive(h.venue().fetch_closed_pnl(opts)).unwrap_err();
        assert_eq!(
            err.message,
            format!("Bybit has no closed PnL for options ({symbol})")
        );
        assert!(h.wire.calls("/v5/position/closed-pnl").is_empty());
        h.venue().dispose().unwrap();
    }
    for name in [
        ExchangeName::Bybit,
        ExchangeName::Bybitdemo,
        ExchangeName::Bybitinverse,
    ] {
        let mut opts = options();
        opts.public_data_only = Some(true);
        let h = harness_as(name, opts, |_| {});
        let opts = serde_json::from_value(json!({"symbol":symbol})).unwrap();
        assert!(
            h.drive(h.venue().fetch_closed_pnl(opts))
                .unwrap()
                .is_empty()
        );
        assert!(h.wire.calls("/v5/position/closed-pnl").is_empty());
        h.venue().dispose().unwrap();
    }
}

#[test]
fn closed_pnl_malformed_rows_emit_error_and_discard_partial_results() {
    let h = harness_as(ExchangeName::Bybit, options(), |wire| {
        wire.ok(
            "GET",
            "/v5/position/closed-pnl",
            json!({"list":[row("BTCUSDT","1700000001000"),null]}),
        );
    });
    let opts = serde_json::from_value(json!({"symbol":"BTCUSDT","from":1,"to":2})).unwrap();
    assert!(
        h.drive(h.venue().fetch_closed_pnl(opts))
            .unwrap()
            .is_empty()
    );
    assert_eq!(h.wire.calls("/v5/position/closed-pnl").len(), 1);
    assert!(
        h.runtime
            .logs
            .borrow()
            .iter()
            .any(|s| s.contains("Cannot read properties of null"))
    );
    h.venue().dispose().unwrap();
}

#[test]
fn closed_pnl_default_bounds_read_the_signing_virtual_clock() {
    let h = harness_as(ExchangeName::Bybit, options(), |wire| {
        wire.ok("GET", "/v5/position/closed-pnl", json!({"list":[]}));
    });
    h.runtime.now.set(support::START_MS + 12345.);
    let opts = serde_json::from_value(json!({"symbol":"BTCUSDT"})).unwrap();
    assert!(
        h.drive(h.venue().fetch_closed_pnl(opts))
            .unwrap()
            .is_empty()
    );
    let calls = h.wire.calls("/v5/position/closed-pnl");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].params["endTime"], "1700000000000");
    assert_eq!(calls[0].params["startTime"], "1699395200001");
    h.venue().dispose().unwrap();
}
