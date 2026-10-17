//! Dynamic history dispatch uses the actual venue receiver and signed HTTP client.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_exchange::capabilities::{BaseCapabilityMethod, is_base_capability_fallback};
use safe_cex_types::ExchangeName;
use serde_json::json;
use support::{harness_as, options};

fn derivatives() -> [ExchangeName; 3] {
    [
        ExchangeName::Bybit,
        ExchangeName::Bybitdemo,
        ExchangeName::Bybitinverse,
    ]
}

#[test]
fn dynamic_closed_pnl_reaches_signed_http() {
    for name in derivatives() {
        let h = harness_as(name, options(), |wire| {
            wire.ok(
                "GET",
                "/v5/position/closed-pnl",
                json!({"list":[{
                    "symbol":"BTCUSD","createdTime":"1000","updatedTime":"2000",
                    "closedSize":"1000","avgEntryPrice":"40000","avgExitPrice":"50000",
                    "cumExitValue":"50000000","closedPnl":"12.5","openFee":"1.2",
                    "closeFee":"0.8","side":"Sell","leverage":"10"
                }]}),
            );
        });
        let venue = h.venue();
        let opts = serde_json::from_value(
            json!({"symbol":"BTCUSD","from":1,"to":2,"accountName":"dynamic"}),
        )
        .unwrap();
        let rows = h
            .drive(venue.capabilities().fetch_closed_pnl(venue, opts))
            .unwrap();
        assert_eq!(rows.len(), 1, "dynamic closed PnL must call the venue");
        assert_eq!(rows[0].account, "dynamic");
        assert_eq!(h.wire.calls("/v5/position/closed-pnl").len(), 1);
        assert!(!is_base_capability_fallback(
            &venue.capabilities().fetch_closed_pnl,
            BaseCapabilityMethod::FetchClosedPnl
        ));
        venue.dispose().unwrap();
    }
}

#[test]
fn dynamic_fills_reach_signed_http() {
    for name in derivatives() {
        let h = harness_as(name, options(), |wire| {
            wire.ok(
                "GET",
                "/v5/execution/list",
                json!({"list":[{
                    "execId":"fill1","orderId":"order1","symbol":"BTCUSD","execType":"Trade",
                    "execQty":"2","execPrice":"100","execTime":"1700000000000",
                    "execFee":"-0.2","feeCurrency":"USDT","isMaker":true,"side":"Buy"
                }]}),
            );
        });
        let venue = h.venue();
        let opts =
            serde_json::from_value(json!({"symbol":"BTCUSD","from":1,"to":2,"limit":10})).unwrap();
        let rows = h
            .drive(venue.capabilities().fetch_historical_fills(venue, opts))
            .unwrap();
        assert_eq!(rows.len(), 1, "dynamic fills must call the venue");
        assert_eq!(rows[0].id.as_deref(), Some("fill1"));
        assert_eq!(h.wire.calls("/v5/execution/list").len(), 1);
        assert!(!is_base_capability_fallback(
            &venue.capabilities().fetch_historical_fills,
            BaseCapabilityMethod::FetchHistoricalFills
        ));
        venue.dispose().unwrap();
    }
}

fn income(range: bool) {
    for name in derivatives() {
        let h = harness_as(name, options(), |wire| {
            wire.ok(
                "GET",
                "/v5/account/transaction-log",
                json!({"list":[{
                    "transactionTime":"1700000000000","type":"SETTLEMENT","symbol":"BTCUSD",
                    "currency":"BTC","funding":"1","cashFlow":"0","fee":"0"
                }]}),
            );
        });
        let venue = h.venue();
        let rows = h
            .drive(if range {
                venue
                    .capabilities()
                    .fetch_income_history_range(venue, 1., 2., None)
            } else {
                venue.capabilities().fetch_income_history(venue, None)
            })
            .unwrap();
        assert_eq!(rows.len(), 1, "dynamic income must call the venue");
        assert_eq!(rows[0].income, 1.);
        let calls = h.wire.calls("/v5/account/transaction-log");
        assert_eq!(calls.len(), 1);
        if range {
            assert_eq!(calls[0].params["startTime"], "1");
            assert_eq!(calls[0].params["endTime"], "2");
        }
        assert!(!is_base_capability_fallback(
            &venue.capabilities().fetch_income_history,
            BaseCapabilityMethod::FetchIncomeHistory
        ));
        assert!(!is_base_capability_fallback(
            &venue.capabilities().fetch_income_history_range,
            BaseCapabilityMethod::FetchIncomeHistoryRange
        ));
        venue.dispose().unwrap();
    }
}
#[test]
fn dynamic_income_reaches_signed_http() {
    income(false);
}
#[test]
fn dynamic_income_range_reaches_signed_http() {
    income(true);
}

#[test]
fn dynamic_orders_reach_signed_http() {
    for name in [ExchangeName::Bybit, ExchangeName::Bybitdemo] {
        let h = harness_as(name, options(), |wire| {
            wire.ok("GET", "/v5/order/history", json!({"list":[{
                "orderId":"order1","symbol":"BTCUSDT","createdTime":"1700000000000","updatedTime":"1700000001000",
                "side":"Buy","orderType":"Limit","orderStatus":"Cancelled","qty":"2.5","cumExecQty":"1.5",
                "price":"0","avgPrice":"123.5","positionIdx":1,"reduceOnly":false
            }]}));
        });
        let venue = h.venue();
        let opts = serde_json::from_value(json!({"symbol":"BTCUSDT","from":1,"to":2})).unwrap();
        let rows = h
            .drive(
                venue
                    .capabilities()
                    .fetch_historical_orders(venue, Some(opts)),
            )
            .unwrap();
        assert_eq!(rows.len(), 1, "dynamic orders must call the venue");
        assert_eq!(rows[0].id, "order1");
        assert_eq!(h.wire.calls("/v5/order/history").len(), 1);
        assert!(!is_base_capability_fallback(
            &venue.capabilities().fetch_historical_orders,
            BaseCapabilityMethod::FetchHistoricalOrders
        ));
        venue.dispose().unwrap();
    }
}

#[test]
fn dedicated_families_retain_inherited_history_capabilities() {
    let h = harness_as(ExchangeName::Bybitspot, options(), |_| {});
    let venue = h.venue();
    let caps = venue.capabilities();
    let opts = serde_json::from_value(json!({"symbol":"BTCUSDT"})).unwrap();
    assert!(
        h.drive(caps.fetch_closed_pnl(venue, opts))
            .unwrap()
            .is_empty()
    );
    let opts = serde_json::from_value(json!({"symbol":"BTCUSDT"})).unwrap();
    assert!(
        h.drive(caps.fetch_historical_fills(venue, opts))
            .unwrap()
            .is_empty()
    );
    assert!(
        h.drive(caps.fetch_income_history(venue, None))
            .unwrap()
            .is_empty()
    );
    assert!(
        h.drive(caps.fetch_income_history_range(venue, 1., 2., None))
            .unwrap()
            .is_empty()
    );
    assert!(is_base_capability_fallback(
        &caps.fetch_closed_pnl,
        BaseCapabilityMethod::FetchClosedPnl
    ));
    assert!(is_base_capability_fallback(
        &caps.fetch_historical_fills,
        BaseCapabilityMethod::FetchHistoricalFills
    ));
    assert!(is_base_capability_fallback(
        &caps.fetch_income_history,
        BaseCapabilityMethod::FetchIncomeHistory
    ));
    assert!(is_base_capability_fallback(
        &caps.fetch_income_history_range,
        BaseCapabilityMethod::FetchIncomeHistoryRange
    ));
    venue.dispose().unwrap();
    for name in [ExchangeName::Bybitspot, ExchangeName::Bybitinverse] {
        let h = harness_as(name, options(), |_| {});
        let venue = h.venue();
        assert!(
            h.drive(venue.capabilities().fetch_historical_orders(venue, None))
                .unwrap()
                .is_empty()
        );
        assert!(is_base_capability_fallback(
            &venue.capabilities().fetch_historical_orders,
            BaseCapabilityMethod::FetchHistoricalOrders
        ));
        assert!(h.wire.calls("/v5/order/history").is_empty());
        venue.dispose().unwrap();
    }
}
