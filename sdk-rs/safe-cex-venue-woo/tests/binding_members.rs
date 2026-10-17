//! WOO members bound in binding/hosts/members.rs, driven through the production
//! WooExchange: transaction-history fills and their symbol/side normalisation.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)] // Shared support: each test crate uses a subset.
mod binding_harness;
use binding_harness::{harness, options};
use safe_cex_types::{ExchangeName, ExchangeOptions, ExecutionsOptions};
use serde_json::{Value, json};

fn executions(symbol: Option<&str>) -> ExecutionsOptions {
    serde_json::from_value(symbol.map_or(json!({}), |s| json!({"symbol": s}))).unwrap()
}

fn row(id: u64, side: &str, ts: f64) -> Value {
    json!({
        "id": id, "symbol": "PERP_BTC_USDT", "side": side, "executedPrice": 50000.5,
        "executedQuantity": 0.25, "executedTimestamp": ts, "orderId": 77, "fee": 0.1,
        "feeAsset": "USDT", "isMaker": 1, "realizedPnl": null
    })
}

#[test]
fn fetch_historical_fills_reads_transaction_history_for_the_venue_symbol() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.ok(
            "GET",
            "/v3/trade/transactionHistory",
            json!({"rows": [row(2, "SELL", 1_700_000_002_000.0), row(1, "BUY", 1_700_000_001_000.0)]}),
        );
    });
    let fills = h
        .drive(
            h.venue()
                .fetch_historical_fills(executions(Some("BTCUSDT"))),
        )
        .unwrap();
    let calls = h.http.calls("/v3/trade/transactionHistory");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].method, "GET");
    assert_eq!(
        calls[0].params,
        json!({"limit": "500", "symbol": "PERP_BTC_USDT"})
    );
    let fills = serde_json::to_value(fills).unwrap();
    assert_eq!(fills.as_array().unwrap().len(), 2);
    // ts: sorted newest first (b.timestamp - a.timestamp).
    let first = &fills[1];
    assert_eq!(fills[0]["id"], "2");
    assert_eq!(fills[0]["timestamp"], 1_700_000_002.0);
    assert_eq!(first["id"], "1");
    assert_eq!(first["symbol"], "BTCUSDT");
    assert_eq!(first["side"], "buy");
    assert_eq!(first["timestamp"], 1_700_000_001.0);
    assert_eq!(first["price"], 50000.5);
    assert_eq!(first["qty"], 0.25);
    assert_eq!(first["orderId"], "77");
    assert_eq!(first["maker"], true);
    assert!(first.get("realizedPnl").is_none_or(Value::is_null));
    assert_eq!(fills[0]["side"], "sell");
}

#[test]
fn hedge_side_strings_carry_side_and_position_side() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.ok(
            "GET",
            "/v3/trade/transactionHistory",
            json!({"rows": [row(3, "OPEN SHORT", 1_700_000_003_000.0)]}),
        );
    });
    let fills = h
        .drive(h.venue().fetch_historical_fills(executions(None)))
        .unwrap();
    let fills = serde_json::to_value(fills).unwrap();
    assert_eq!(fills[0]["side"], "sell");
    assert_eq!(fills[0]["positionSide"], "short");
    assert_eq!(
        h.http.calls("/v3/trade/transactionHistory")[0].params,
        json!({"limit": "500"})
    );
}

#[test]
fn a_failed_history_read_is_reported_and_yields_no_fills() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.route(
            "GET",
            "/v3/trade/transactionHistory",
            500.0,
            json!({"success": false, "message": "internal error"}),
        );
    });
    let errors = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
    let sink = errors.clone();
    h.venue().base().on(
        "error",
        std::rc::Rc::new(move |_, payload| {
            sink.borrow_mut().push(payload.to_display());
            Ok(())
        }),
    );
    let fills = h
        .drive(h.venue().fetch_historical_fills(executions(None)))
        .unwrap();
    assert!(fills.is_empty());
    assert!(
        errors.borrow().iter().any(|m| m.contains("internal error")),
        "{:?}",
        errors.borrow()
    );
}

#[test]
fn public_data_only_returns_no_fills_without_a_request() {
    let mut opts: ExchangeOptions = options();
    opts.key = String::new();
    opts.secret = String::new();
    let h = harness(ExchangeName::Woo, opts, |_| {});
    let fills = h
        .drive(h.venue().fetch_historical_fills(executions(None)))
        .unwrap();
    assert!(fills.is_empty());
    assert!(h.http.requests.borrow().is_empty());
}
