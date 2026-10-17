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
