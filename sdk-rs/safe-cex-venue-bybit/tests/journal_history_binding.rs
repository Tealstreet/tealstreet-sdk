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
    let records = h
        .drive(
            h.venue()
                .fetch_historical_positions(HistoricalPositionOptions {
                    account: "account-a".into(),
                    from: Some(0.),
                    to: Some(10000.),
                }),
        )
        .unwrap();
    assert_eq!(
        records.len(),
        2,
        "order hedge legs must keep otherwise identical closes separate"
    );
    for (record, leg, size, pnl) in [
        (&records[0], "long", 1., 4.),
        (&records[1], "short", 2., 8.),
    ] {
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
    assert_eq!(h.wire.calls("/v5/order/history").len(), 1);
    h.venue().dispose().unwrap();
}
#[test]
fn primary_journal_history_joins_the_real_order_axis() {
    journal_case(ExchangeName::Bybit);
}
#[test]
fn demo_journal_history_joins_the_real_order_axis() {
    journal_case(ExchangeName::Bybitdemo);
}
#[test]
fn dedicated_spot_and_inverse_retain_source_journal_refusal() {
    for name in [ExchangeName::Bybitspot, ExchangeName::Bybitinverse] {
        let h = harness_as(name, options(), |_| {});
        let error = h
            .drive(
                h.venue()
                    .fetch_historical_positions(HistoricalPositionOptions {
                        account: "account-a".into(),
                        from: Some(0.),
                        to: Some(10000.),
                    }),
            )
            .unwrap_err();
        assert!(error.message.contains("no journal records path"));
        assert!(h.wire.requests.borrow().is_empty());
        h.venue().dispose().unwrap();
    }
}

fn closed_row(id: &str, symbol: &str, size: &str, at: &str) -> serde_json::Value {
    json!({"orderId":id,"symbol":symbol,"side":"Sell","avgEntryPrice":"100",
        "avgExitPrice":"104","closedSize":size,"closedPnl":"2","updatedTime":at,"execType":"Trade"})
}
#[test]
fn journal_pages_closed_rows_before_reading_orders_and_folding_partials() {
    let h = harness_as(ExchangeName::Bybit, options(), |wire| {
        wire.ok(
            "GET",
            "/v5/position/closed-pnl",
            json!({"list":[closed_row("one","BTCUSDT","1","1000")],"nextPageCursor":"next"}),
        );
        wire.ok(
            "GET",
            "/v5/position/closed-pnl",
            json!({"list":[closed_row("two","BTCUSDT","2","2000")]}),
        );
        wire.ok("GET", "/v5/order/history", json!({"list":[]}));
    });
    let rows = h
        .drive(
            h.venue()
                .fetch_historical_positions(HistoricalPositionOptions {
                    account: "account-a".into(),
                    from: Some(0.),
                    to: Some(10000.),
                }),
        )
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["closedSize"], 3.);
    assert_eq!(rows[0]["closeCount"], 2.);
    assert_eq!(rows[0]["positionSide"], "both");
    assert_eq!(rows[0]["sourceAxis"], "closed-positions");
    let calls = h.wire.calls("/v5/position/closed-pnl");
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].params["limit"], "100");
    assert_eq!(calls[0].params["category"], "linear");
    assert!(calls[0].params.get("symbol").is_none());
    assert!(calls[0].params.get("cursor").is_none());
    assert_eq!(calls[1].params["cursor"], "next");
    assert_eq!(h.wire.calls("/v5/order/history").len(), 1);
    h.venue().dispose().unwrap();
}
#[test]
fn journal_closed_page_failure_discards_partial_records_and_skips_order_join() {
    let h = harness_as(ExchangeName::Bybit, options(), |wire| {
        wire.ok(
            "GET",
            "/v5/position/closed-pnl",
            json!({"list":[closed_row("one","BTCUSDT","1","1000")],"nextPageCursor":"next"}),
        );
        wire.route(
            "GET",
            "/v5/position/closed-pnl",
            200,
            json!({"retCode":1,"retMsg":"closed-axis rejected"}),
        );
    });
    let error = h
        .drive(
            h.venue()
                .fetch_historical_positions(HistoricalPositionOptions {
                    account: "account-a".into(),
                    from: Some(0.),
                    to: Some(10000.),
                }),
        )
        .unwrap_err();
    assert!(error.message.contains("closed-axis rejected"));
    assert_eq!(h.wire.calls("/v5/position/closed-pnl").len(), 2);
    assert!(h.wire.calls("/v5/order/history").is_empty());
    h.venue().dispose().unwrap();
}
#[test]
fn journal_public_only_returns_empty_without_http() {
    let mut opts = options();
    opts.public_data_only = Some(true);
    let h = harness_as(ExchangeName::Bybit, opts, |_| {});
    assert!(
        h.drive(
            h.venue()
                .fetch_historical_positions(HistoricalPositionOptions {
                    account: "account-a".into(),
                    from: Some(0.),
                    to: Some(10000.),
                })
        )
        .unwrap()
        .is_empty()
    );
    assert!(h.wire.requests.borrow().is_empty());
    h.venue().dispose().unwrap();
}
#[test]
fn journal_resolves_each_symbol_quote_from_the_live_store() {
    let h = harness_as(ExchangeName::Bybit, options(), |wire| {
        wire.ok("GET","/v5/position/closed-pnl",json!({"list":[closed_row("one","BTCUSDT","1","1000"),closed_row("two","BTCUSDC","1","2000")]}));
        wire.ok("GET", "/v5/order/history", json!({"list":[]}));
    });
    let market = serde_json::from_value(json!({"id":"BTCUSDC","symbol":"BTCUSDC","base":"BTC","quote":"USDC","active":true,
        "precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":100},"leverage":{"min":1,"max":100}}})).unwrap();
    safe_cex_store::Store::update(
        h.store.as_ref(),
        safe_cex_store::StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let rows = h
        .drive(
            h.venue()
                .fetch_historical_positions(HistoricalPositionOptions {
                    account: "account-a".into(),
                    from: Some(0.),
                    to: Some(10000.),
                }),
        )
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["settleCurrency"], "USDT");
    assert_eq!(rows[1]["settleCurrency"], "USDC");
    h.venue().dispose().unwrap();
}
#[test]
fn journal_keeps_rows_with_inherited_close_reason_functions() {
    let h = harness_as(ExchangeName::Bybit, options(), |wire| {
        let mut inherited = closed_row("inherited", "BTCUSDT", "1", "1000");
        inherited["execType"] = json!("constructor");
        wire.ok(
            "GET",
            "/v5/position/closed-pnl",
            json!({"list":[inherited,closed_row("valid","BTCUSDC","1","2000")]}),
        );
        wire.ok("GET", "/v5/order/history", json!({"list":[]}));
    });
    let rows = h
        .drive(
            h.venue()
                .fetch_historical_positions(HistoricalPositionOptions {
                    account: "account-a".into(),
                    from: Some(0.),
                    to: Some(10000.),
                }),
        )
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["venueId"], "inherited");
    assert!(!rows[0].as_object().unwrap().contains_key("closeReason"));
    assert_eq!(rows[1]["closeReason"], "user");
    h.venue().dispose().unwrap();
}
