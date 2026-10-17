//! Income history through the actual receiver and signed client.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_types::ExchangeName;
use serde_json::json;
use support::{harness_as, options};

fn income_case(name: ExchangeName, range: bool) {
    let inverse = name == ExchangeName::Bybitinverse;
    let h = harness_as(name, options(), |wire| {
        wire.ok(
            "GET",
            "/v5/account/transaction-log",
            json!({"list":[{
                "transactionTime":"1700000000000","type":"TRADE","symbol":"BTCUSD",
                "currency":"BTC","tradeId":"fill1","orderId":"order1",
                "cashFlow":"4.5","funding":"-1.25","fee":"0.25"
            }]}),
        );
    });
    let rows = if range {
        h.drive(
            h.venue()
                .fetch_income_history_range(1., 2., Some("COMMISSION".into())),
        )
    } else {
        h.drive(h.venue().fetch_income_history(Some(
            serde_json::from_value(json!({"limit":100,"incomeType":"COMMISSION"})).unwrap(),
        )))
    }
    .unwrap();
    assert_eq!(
        rows.len(),
        3,
        "income must reach transaction log and preserve all three terms"
    );
    assert_eq!(
        rows.iter().map(|r| r.income).collect::<Vec<_>>(),
        vec![4.5, -1.25, -0.25]
    );
    assert_eq!(
        rows.iter()
            .map(|r| r.income_type.as_str())
            .collect::<Vec<_>>(),
        vec!["REALIZED_PNL", "FUNDING_FEE", "COMMISSION"]
    );
    assert!(
        rows.iter()
            .all(|r| r.asset == "BTC" && r.trade_id.as_deref() == Some("fill1"))
    );
    let calls = h.wire.calls("/v5/account/transaction-log");
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].params["category"],
        if inverse { "inverse" } else { "linear" }
    );
    assert_eq!(calls[0].params["accountType"], "UNIFIED");
    assert_eq!(calls[0].params["limit"], "50");
    assert_eq!(calls[0].params["type"], "TRADE");
    if range {
        assert_eq!(calls[0].params["startTime"], "1");
        assert_eq!(calls[0].params["endTime"], "2");
    }
    h.venue().dispose().unwrap();
}
#[test]
fn primary_income_reads_the_transaction_log() {
    income_case(ExchangeName::Bybit, false);
}
#[test]
fn demo_income_reads_the_transaction_log() {
    income_case(ExchangeName::Bybitdemo, false);
}
#[test]
fn inverse_income_reads_the_transaction_log() {
    income_case(ExchangeName::Bybitinverse, false);
}
#[test]
fn primary_income_range_reads_the_transaction_log() {
    income_case(ExchangeName::Bybit, true);
}
#[test]
fn demo_income_range_reads_the_transaction_log() {
    income_case(ExchangeName::Bybitdemo, true);
}
#[test]
fn inverse_income_range_reads_the_transaction_log() {
    income_case(ExchangeName::Bybitinverse, true);
}
#[test]
fn spot_income_retains_both_inherited_defaults() {
    let h = harness_as(ExchangeName::Bybitspot, options(), |_| {});
    assert!(
        h.drive(h.venue().fetch_income_history(None))
            .unwrap()
            .is_empty()
    );
    assert!(
        h.drive(h.venue().fetch_income_history_range(1., 2., None))
            .unwrap()
            .is_empty()
    );
    assert!(h.wire.calls("/v5/account/transaction-log").is_empty());
    h.venue().dispose().unwrap();
}

fn transaction(time: u64, symbol: &str) -> serde_json::Value {
    json!({"transactionTime":time.to_string(),"type":"SETTLEMENT","symbol":symbol,
        "currency":"USDT","funding":"1","cashFlow":"0","fee":"0"})
}

#[test]
fn income_combines_categories_then_sorts_and_applies_the_primary_limit() {
    let mut opts = options();
    opts.enable_inverse_markets = Some(true);
    let h = harness_as(ExchangeName::Bybit, opts, |wire| {
        wire.route_when(
            "GET",
            "/v5/account/transaction-log",
            "category",
            "linear",
            json!({"retCode":0,"retMsg":"OK","result":{"list":[transaction(100,"BTCUSDT")]}}),
        );
        wire.route_when(
            "GET",
            "/v5/account/transaction-log",
            "category",
            "inverse",
            json!({"retCode":0,"retMsg":"OK","result":{"list":[transaction(200,"BTCUSD")]}}),
        );
    });
    let opts = serde_json::from_value(json!({"limit":1})).unwrap();
    let rows = h.drive(h.venue().fetch_income_history(Some(opts))).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].timestamp, 200.);
    assert_eq!(rows[0].symbol, "BTCUSD");
    assert_eq!(
        h.wire
            .calls("/v5/account/transaction-log")
            .iter()
            .map(|r| r.params["category"].clone())
            .collect::<Vec<_>>(),
        vec![json!("linear"), json!("inverse")]
    );
    h.venue().dispose().unwrap();
}

#[test]
fn single_category_income_does_not_sort_or_truncate_a_mapped_page() {
    for name in [ExchangeName::Bybit, ExchangeName::Bybitinverse] {
        let h = harness_as(name, options(), |wire| {
            wire.ok("GET", "/v5/account/transaction-log", json!({"list":[transaction(100,"BTCUSD"),transaction(200,"BTCUSD")],"nextPageCursor":"more"}));
        });
        let opts = serde_json::from_value(json!({"limit":1})).unwrap();
        let rows = h.drive(h.venue().fetch_income_history(Some(opts))).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.timestamp).collect::<Vec<_>>(),
            vec![100., 200.]
        );
        assert_eq!(h.wire.calls("/v5/account/transaction-log").len(), 1);
        h.venue().dispose().unwrap();
    }
}

#[test]
fn income_range_windows_advance_without_deduplicating_shared_ids() {
    let h = harness_as(ExchangeName::Bybitinverse, options(), |wire| {
        wire.ok(
            "GET",
            "/v5/account/transaction-log",
            json!({"list":[transaction(100,"BTCUSD")]}),
        );
    });
    let rows = h
        .drive(h.venue().fetch_income_history_range(0., 604800002., None))
        .unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|r| r.tran_id == rows[0].tran_id));
    let calls = h.wire.calls("/v5/account/transaction-log");
    assert_eq!(calls.len(), 3);
    let bounds = calls
        .iter()
        .map(|r| (r.params["startTime"].clone(), r.params["endTime"].clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        bounds,
        vec![
            (json!("0"), json!("604799999")),
            (json!("604800000"), json!("604800000")),
            (json!("604800001"), json!("604800002"))
        ]
    );
    h.venue().dispose().unwrap();
}

#[test]
fn income_keeps_source_paging_and_reports_malformed_rows() {
    let h = harness_as(ExchangeName::Bybit, options(), |wire| {
        wire.ok(
            "GET",
            "/v5/account/transaction-log",
            json!({"list":[transaction(100,"BTCUSDT")],"nextPageCursor":"next"}),
        );
        wire.ok(
            "GET",
            "/v5/account/transaction-log",
            json!({"list":[transaction(200,"ETHUSDT")]}),
        );
    });
    let rows = h.drive(h.venue().fetch_income_history(None)).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        h.wire.calls("/v5/account/transaction-log")[1].params["cursor"],
        "next"
    );
    h.venue().dispose().unwrap();
    let h = harness_as(ExchangeName::Bybit, options(), |wire| {
        wire.ok("GET", "/v5/account/transaction-log", json!({"list":[null]}));
    });
    assert!(
        h.drive(h.venue().fetch_income_history(None))
            .unwrap()
            .is_empty()
    );
    assert!(h.runtime.logs.borrow().iter().any(|m| {
        m.starts_with("error: Error fetching income history: Cannot read properties of null")
    }));
    h.venue().dispose().unwrap();
}
