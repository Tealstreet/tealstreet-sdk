//! Historical orders through the actual Exchange receiver and signed client.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_types::{ExchangeName, OrderSide, OrderStatus, OrderType};
use serde_json::{Value, json};
use support::{harness_as, options};
fn row(id: &str, created: &str) -> Value {
    json!({"orderId":id,"symbol":"BTCUSDT","createdTime":created,"updatedTime":"1700000001000",
        "side":"Buy","orderType":"Limit","orderStatus":"Cancelled","qty":"2.5","cumExecQty":"1.5",
        "price":"0","avgPrice":"123.5","positionIdx":1,"reduceOnly":false})
}
fn history_case(name: ExchangeName) {
    let h = harness_as(name, options(), |wire| {
        wire.ok(
            "GET",
            "/v5/order/history",
            json!({"list":[row("order1","1700000000000")]}),
        );
    });
    let opts = serde_json::from_value(json!({"symbol":"BTCUSDT","from":1,"to":2})).unwrap();
    let rows = h
        .drive(h.venue().fetch_historical_orders(Some(opts)))
        .unwrap();
    assert_eq!(rows.len(), 1, "order history must reach the venue member");
    let r = &rows[0];
    assert_eq!(
        (&*r.id, r.side, r.r#type, r.status),
        (
            "order1",
            OrderSide::Buy,
            OrderType::Limit,
            OrderStatus::Canceled
        )
    );
    assert_eq!(
        (r.amount, r.filled, r.price, r.average_price),
        (2.5, 1.5, None, Some(123.5))
    );
    let calls = h.wire.calls("/v5/order/history");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].params["category"], "linear");
    assert_eq!(calls[0].params["limit"], "50");
    assert_eq!(calls[0].params["startTime"], "1");
    assert_eq!(calls[0].params["endTime"], "2");
    h.venue().dispose().unwrap();
}
#[test]
fn primary_order_history_reaches_the_signed_client() {
    history_case(ExchangeName::Bybit);
}
#[test]
fn demo_order_history_reaches_the_signed_client() {
    history_case(ExchangeName::Bybitdemo);
}
#[test]
fn dedicated_spot_and_inverse_keep_inherited_empty_order_history() {
    for name in [ExchangeName::Bybitspot, ExchangeName::Bybitinverse] {
        let h = harness_as(name, options(), |_| {});
        assert!(
            h.drive(h.venue().fetch_historical_orders(None))
                .unwrap()
                .is_empty()
        );
        assert!(h.wire.calls("/v5/order/history").is_empty());
        h.venue().dispose().unwrap();
    }
}

#[test]
fn order_history_deduplicates_pages_before_sorting_and_limit() {
    let h = harness_as(ExchangeName::Bybit, options(), |wire| {
        wire.ok(
            "GET",
            "/v5/order/history",
            json!({"list":[row("one","100"),row("","150")],"nextPageCursor":"next"}),
        );
        wire.ok(
            "GET",
            "/v5/order/history",
            json!({"list":[row("one","300"),row("two","200")]}),
        );
    });
    let opts = serde_json::from_value(json!({"limit":1})).unwrap();
    let rows = h
        .drive(h.venue().fetch_historical_orders(Some(opts)))
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].id, "two",
        "first occurrence of a repeated ID wins before sorting"
    );
    assert_eq!(
        h.wire.calls("/v5/order/history")[1].params["cursor"],
        "next"
    );
    h.venue().dispose().unwrap();
}

#[test]
fn order_history_scans_both_categories_and_inclusive_windows() {
    let mut opts = options();
    opts.enable_inverse_markets = Some(true);
    let h = harness_as(ExchangeName::Bybitdemo, opts, |wire| {
        wire.ok(
            "GET",
            "/v5/order/history",
            json!({"list":[row("shared","100")]}),
        );
    });
    let opts = serde_json::from_value(json!({"from":0,"to":604800000})).unwrap();
    let rows = h
        .drive(h.venue().fetch_historical_orders(Some(opts)))
        .unwrap();
    assert_eq!(rows.len(), 1);
    let calls = h.wire.calls("/v5/order/history");
    assert_eq!(calls.len(), 4);
    assert_eq!(
        calls
            .iter()
            .map(|r| r.params["category"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!("linear"),
            json!("linear"),
            json!("inverse"),
            json!("inverse")
        ]
    );
    assert_eq!(calls[0].params["startTime"], "0");
    assert_eq!(calls[0].params["endTime"], "604799999");
    assert_eq!(calls[1].params["startTime"], "604800000");
    assert_eq!(calls[1].params["endTime"], "604800000");
    h.venue().dispose().unwrap();
}

#[test]
fn order_history_cap_warns_and_keeps_first_seen_ids() {
    let h = harness_as(ExchangeName::Bybit, options(), |wire| {
        wire.ok(
            "GET",
            "/v5/order/history",
            json!({"list":[row("same","100")],"nextPageCursor":"more"}),
        );
    });
    let rows = h.drive(h.venue().fetch_historical_orders(None)).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(h.wire.calls("/v5/order/history").len(), 20);
    assert!(h.runtime.logs.borrow().iter().any(|m|m=="warning: fetchHistoricalOrders stopped at the 20-page cap; older orders were not fetched"));
    h.venue().dispose().unwrap();
}

#[test]
fn order_history_public_only_returns_without_http() {
    let mut opts = options();
    opts.public_data_only = Some(true);
    let h = harness_as(ExchangeName::Bybit, opts, |_| {});
    assert!(
        h.drive(h.venue().fetch_historical_orders(None))
            .unwrap()
            .is_empty()
    );
    assert!(h.wire.calls("/v5/order/history").is_empty());
    h.venue().dispose().unwrap();
}

#[test]
fn order_history_keeps_rows_with_inherited_optional_function_fields() {
    let h = harness_as(ExchangeName::Bybit, options(), |wire| {
        let mut order = row("optional-prototype", "100");
        order["positionIdx"] = json!("toString");
        wire.ok("GET", "/v5/order/history", json!({"list":[order]}));
    });
    let rows = h.drive(h.venue().fetch_historical_orders(None)).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "optional-prototype");
    assert!(rows[0].position_side.is_none());
    assert!(
        serde_json::to_value(&rows[0])
            .unwrap()
            .get("positionSide")
            .is_none()
    );
    h.venue().dispose().unwrap();
}
