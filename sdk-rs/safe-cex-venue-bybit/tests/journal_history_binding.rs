//! Journal history through the actual Exchange, receiver, clients and order join.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_exchange::journal::HistoricalPositionOptions;
use safe_cex_types::ExchangeName;
use serde_json::json;
use support::{harness_as, options};
fn journal_case(name: ExchangeName) {
    let h = harness_as(name, options(), |wire| {
        wire.ok("GET", "/v5/position/closed-pnl", json!({"list":[
            {"orderId":"long-close", "symbol":"BTCUSDT", "side":"Sell", "avgEntryPrice":"100", "avgExitPrice":"104", "closedSize":"1", "openFee":"0.1", "closeFee":"0.2", "closedPnl":"3.7", "updatedTime":"2000", "execType":"Trade"},
            {"orderId":"short-close", "symbol":"BTCUSDT", "side":"Sell", "avgEntryPrice":"100", "avgExitPrice":"104", "closedSize":"2", "openFee":"0.2", "closeFee":"0.4", "closedPnl":"7.4", "updatedTime":"3000", "execType":"Trade"}
        ]}));
        wire.ok("GET", "/v5/order/history", json!({"list":[
            {"orderId":"long-close", "symbol":"BTCUSDT", "side":"Sell", "orderType":"Market", "orderStatus":"Filled", "qty":"1", "cumExecQty":"1", "positionIdx":1, "createdTime":"1000"},
            {"orderId":"short-close", "symbol":"BTCUSDT", "side":"Sell", "orderType":"Market", "orderStatus":"Filled", "qty":"2", "cumExecQty":"2", "positionIdx":2, "createdTime":"1000"}
        ]}));
    });
    let records = h.drive(h.venue().fetch_historical_positions(HistoricalPositionOptions {
        account: "account-a".into(), from: Some(0.), to: Some(10000.),
    })).unwrap();
    assert_eq!(records.len(), 2, "order hedge legs must keep otherwise identical closes separate");
    for (record, leg, size, pnl) in [(&records[0], "long", 1., 4.), (&records[1], "short", 2., 8.)] {
        assert_eq!(record["account"], "account-a");
        assert_eq!(record["symbol"], "BTCUSDT");
        assert_eq!(record["settleCurrency"], "USDT");
        assert_eq!(record["positionSide"], leg);
        assert_eq!(record["side"], "long");
        assert_eq!(record["sourceAxis"], "closed-positions+orders");
        assert_eq!(record["closedSize"].as_f64().unwrap(), size);
        assert_eq!(record["realizedPnl"].as_f64().unwrap(), pnl);
        assert_eq!(record["closeCount"].as_f64().unwrap(), 1.);
    }
    assert_eq!(h.wire.calls("/v5/position/closed-pnl").len(), 1);
    assert_eq!(h.wire.calls("/v5/order/history").len(), 2);
    h.venue().dispose().unwrap();
}
#[test]
fn primary_journal_history_joins_the_real_order_axis() { journal_case(ExchangeName::Bybit); }
#[test]
fn demo_journal_history_joins_the_real_order_axis() { journal_case(ExchangeName::Bybitdemo); }
#[test]
fn dedicated_spot_and_inverse_retain_source_journal_refusal() {
    for name in [ExchangeName::Bybitspot, ExchangeName::Bybitinverse] {
        let h = harness_as(name, options(), |_| {});
        let error = h.drive(h.venue().fetch_historical_positions(HistoricalPositionOptions {
            account: "account-a".into(), from: Some(0.), to: Some(10000.),
        })).unwrap_err();
        assert!(error.message.contains("no journal records path"));
        assert!(h.wire.requests.borrow().is_empty());
        h.venue().dispose().unwrap();
    }
}
