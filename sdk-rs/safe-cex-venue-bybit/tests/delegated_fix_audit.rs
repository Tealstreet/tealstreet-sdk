//! Real production bindings for the twelve delegated non-UTF-8 audit rows.
#[path = "support/binding_harness.rs"]
mod support;

use safe_cex_types::ExchangeName;
use serde_json::{Value, json};

fn recovery_row(id: &str, fee_asset: Value) -> Value {
    json!({"orderId":id,"orderStatus":"Filled","symbol":"BTCUSD","side":"Sell",
        "cumExecQty":"2","avgPrice":"100","updatedTime":"1700000001000",
        "cumExecFee":"-0.25","feeCurrency":fee_asset})
}

#[test]
fn delegated_history_strict_output_refuses_one_row_and_keeps_good_neighbors() {
    for name in [ExchangeName::Bybit, ExchangeName::Bybitinverse] {
        let h = support::harness_as(name.clone(), support::options(), |wire| {
            wire.ok("GET", "/v5/execution/list", json!({"list":[]}));
            wire.ok(
                "GET",
                "/v5/order/history",
                json!({"list":[
                    recovery_row("first", json!("USDT")),
                    recovery_row("bad", json!({"asset":"unsupported"})),
                    recovery_row("last", json!("BTC"))
                ]}),
            );
        });
        let options = serde_json::from_value(json!({"symbol":"BTCUSD",
            "from":1700000000000_u64,"to":1700000002000_u64}))
        .unwrap();
        let rows = h
            .drive(h.venue().fetch_historical_fills(options))
            .expect("BYBIT-HISTORY-ROW: malformed output must not reject good neighbors");
        assert_eq!(
            rows.iter()
                .map(|r| r.order_id.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["first", "last"],
            "BYBIT-HISTORY-ROW {name:?}"
        );
        assert!(
            h.runtime
                .logs
                .borrow()
                .iter()
                .any(|s| s.contains("commissionAsset") && s.contains("unsupported")),
            "BYBIT-HISTORY-ROW visible field/value refusal {name:?}"
        );
        assert_eq!(h.wire.calls("/v5/order/history").len(), 1);
        h.venue().dispose().unwrap();
    }
}
