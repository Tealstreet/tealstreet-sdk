//! Live production history readers for delegated assignment rows 6 and 16.
#[path = "support/binding_harness.rs"]
mod support;

use safe_cex_types::{ExchangeName, Execution, ExecutionsOptions};
use serde_json::{Value, json};

fn recovery_row(id: &str, fee_asset: Value) -> Value {
    json!({"orderId":id,"orderStatus":"Filled","symbol":"BTCUSD","side":"Sell",
        "cumExecQty":"2","avgPrice":"100","updatedTime":"1700000001000",
        "cumExecFee":"-0.25","feeCurrency":fee_asset,
        "execQty":"2","execPrice":"100","execTime":"1700000001000","execFee":"-0.25"})
}
fn options() -> ExecutionsOptions {
    serde_json::from_value(json!({"symbol":"BTCUSD",
        "from":1700000000000_u64,"to":1700000002000_u64}))
    .unwrap()
}
fn names() -> [ExchangeName; 3] {
    [
        ExchangeName::Bybit,
        ExchangeName::Bybitdemo,
        ExchangeName::Bybitinverse,
    ]
}
fn read(h: &support::Harness) -> Vec<Execution> {
    h.drive(h.venue().fetch_historical_fills(options()))
        .expect("BYBIT-HISTORY-ROW: malformed output must not reject good neighbors")
}
fn refusals(h: &support::Harness) -> Vec<String> {
    h.runtime
        .logs
        .borrow()
        .iter()
        .filter(|s| s.contains("Refused Bybit execution row"))
        .cloned()
        .collect()
}

fn malformed_history_neighbors(name: ExchangeName) {
    for route in ["history", "execution"] {
        for (input_field, output_field) in
            [("symbol", "symbol"), ("feeCurrency", "commissionAsset")]
        {
            for value in [
                json!({"asset":"unsupported"}),
                json!(["USDT"]),
                json!(true),
                json!(12),
            ] {
                let mut bad = recovery_row("bad", json!("USDT"));
                bad[input_field] = value.clone();
                let h = support::harness_as(name.clone(), support::options(), |wire| {
                    let list = json!([
                        recovery_row("first", json!("USDT")),
                        bad,
                        recovery_row("last", json!("BTC"))
                    ]);
                    wire.ok(
                        "GET",
                        "/v5/execution/list",
                        json!({"list":if route == "execution" { list.clone() } else { json!([]) }}),
                    );
                    wire.ok("GET", "/v5/order/history", json!({"list":list}));
                });
                let rows = read(&h);
                assert_eq!(
                    rows.iter()
                        .map(|r| r.order_id.as_deref().unwrap())
                        .collect::<Vec<_>>(),
                    ["first", "last"],
                    "BYBIT-HISTORY-ROW {name:?}/{route}/{output_field}/{value}"
                );
                assert_eq!(rows[0].commission_asset.as_deref(), Some("USDT"));
                assert_eq!(rows[1].commission_asset.as_deref(), Some("BTC"));
                assert!(rows.iter().all(|r| r.symbol == "BTCUSD"
                    && r.qty == 2.
                    && r.price == 100.
                    && r.timestamp == 1700000001.
                    && r.commission == Some(0.25)));
                let logs = refusals(&h);
                assert_eq!(logs.len(), 1, "BYBIT-HISTORY-ROW one visible refusal");
                assert!(
                    logs[0].contains(&format!("{output_field}={value}: expected string")),
                    "BYBIT-HISTORY-ROW exact field/value: {logs:?}"
                );
                assert_eq!(h.wire.calls("/v5/execution/list").len(), 1);
                assert_eq!(
                    h.wire.calls("/v5/order/history").len(),
                    usize::from(route == "history")
                );
                assert!(h.runtime.reports.borrow().is_empty());
                h.venue().dispose().unwrap();
            }
        }
    }
}

#[test]
fn delegated_primary_history_strict_output_keeps_good_neighbors() {
    malformed_history_neighbors(ExchangeName::Bybit);
}
#[test]
fn delegated_demo_history_strict_output_keeps_good_neighbors() {
    malformed_history_neighbors(ExchangeName::Bybitdemo);
}
#[test]
fn delegated_inverse_history_strict_output_keeps_good_neighbors() {
    malformed_history_neighbors(ExchangeName::Bybitinverse);
}

#[test]
fn delegated_history_all_refused_is_empty_without_an_extra_recovery() {
    for name in names() {
        let h = support::harness_as(name, support::options(), |wire| {
            wire.ok("GET", "/v5/execution/list", json!({"list":[]}));
            wire.ok(
                "GET",
                "/v5/order/history",
                json!({"list":[
                recovery_row("bad1", json!(true)), recovery_row("bad2", json!(["USDT"]))]}),
            );
        });
        assert!(read(&h).is_empty(), "BYBIT-HISTORY-ALL-REFUSED");
        assert_eq!(refusals(&h).len(), 2);
        assert_eq!(
            h.wire.calls("/v5/order/history").len(),
            1,
            "BYBIT-HISTORY-ALL-REFUSED keeps the source fallback decision before admission"
        );
        h.venue().dispose().unwrap();
    }
}

#[test]
fn delegated_history_source_failure_precedes_refusal_and_discards_the_recovery_prefix() {
    for name in names() {
        let h = support::harness_as(name, support::options(), |wire| {
            wire.ok("GET", "/v5/execution/list", json!({"list":[]}));
            wire.ok("GET", "/v5/order/history", json!({"list":[
                recovery_row("valid", json!("BTC")), recovery_row("bad", json!({"asset":"unsupported"}))
            ],"nextPageCursor":"later"}));
            wire.ok("GET", "/v5/order/history", json!({"list":[null]}));
        });
        assert!(
            read(&h).is_empty(),
            "BYBIT-HISTORY-SOURCE-ERROR priority over strict output"
        );
        assert_eq!(h.wire.calls("/v5/order/history").len(), 2);
        assert!(
            refusals(&h).is_empty(),
            "BYBIT-HISTORY-SOURCE-ERROR no premature refusal"
        );
        assert!(h.runtime.logs.borrow().iter().any(|s| s.starts_with(
            "warning: Failed order-history fallback fetch executions: Cannot read properties of null")));
        h.venue().dispose().unwrap();
    }
}

#[test]
fn delegated_history_omitted_asset_and_normalized_ids_remain_admitted() {
    for name in names() {
        let mut row = recovery_row("ignored", Value::Null);
        row["orderId"] = json!(42);
        let h = support::harness_as(name, support::options(), |wire| {
            wire.ok("GET", "/v5/execution/list", json!({"list":[]}));
            wire.ok("GET", "/v5/order/history", json!({"list":[row]}));
        });
        let rows = read(&h);
        assert_eq!(
            rows.len(),
            1,
            "BYBIT-HISTORY-ABSENT native omission remains admitted"
        );
        assert_eq!(rows[0].order_id.as_deref(), Some("42"));
        assert_eq!(rows[0].commission_asset, None);
        assert!(refusals(&h).is_empty());
        h.venue().dispose().unwrap();
    }
}

#[test]
fn delegated_history_mapper_keeps_raw_identity_and_the_typed_guard() {
    use safe_cex_exchange::js_runtime::{
        globals::install_builtins,
        value::{Js, from_json, to_typed},
    };
    use safe_cex_venue_bybit::history_mapping::map_order_history_rows_to_executions;
    let globals = Js::object(vec![]);
    install_builtins::<()>(&globals, std::rc::Weak::new());
    for (input_field, output_field) in [("symbol", "symbol"), ("feeCurrency", "commissionAsset")] {
        for raw in [json!({"asset":"unsupported"}), json!(["USDT"])] {
            let row = from_json(&recovery_row("identity", json!("BTC")));
            let alias = from_json(&raw);
            row.set(input_field, alias.clone()).unwrap();
            let mapped = map_order_history_rows_to_executions(
                &globals,
                &Js::array(vec![row]),
                Js::str("BTCUSD"),
                Js::Undefined,
                Js::Undefined,
                Js::Undefined,
            )
            .unwrap();
            let projected = mapped.get("0").unwrap().get(output_field).unwrap();
            assert!(
                projected.strict_equals(&alias),
                "BYBIT-HISTORY-IDENTITY retained raw alias"
            );
            if raw.is_object() {
                alias.set("mutated", Js::Number(-0.)).unwrap();
                assert_eq!(
                    projected
                        .get("mutated")
                        .unwrap()
                        .to_number()
                        .unwrap()
                        .to_bits(),
                    (-0_f64).to_bits()
                );
            } else {
                alias.set("0", Js::str("changed")).unwrap();
                assert!(
                    projected
                        .get("0")
                        .unwrap()
                        .strict_equals(&Js::str("changed"))
                );
            }
            assert!(
                to_typed::<Vec<Execution>>(&mapped).is_err(),
                "BYBIT-HISTORY-IDENTITY strict typed guard stays closed"
            );
        }
    }
}

#[test]
fn delegated_history_refusal_listener_error_is_not_swallowed_by_source_fallback() {
    use safe_cex_exchange::{js_runtime::value::Js, runtime::ExchangeError};
    use std::{cell::Cell, rc::Rc};
    for name in names() {
        let h = support::harness_as(name, support::options(), |wire| {
            wire.ok("GET", "/v5/execution/list", json!({"list":[]}));
            wire.ok(
                "GET",
                "/v5/order/history",
                json!({"list":[recovery_row("bad", json!(true))]}),
            );
        });
        let code = Js::object(vec![("marker", Js::Number(-0.))]);
        let captured = code.clone();
        let called = Rc::new(Cell::new(0));
        let observer = called.clone();
        h.venue().base().on(
            "log",
            Rc::new(move |_, value| {
                let message = value.to_js_string().unwrap();
                if String::from_utf16_lossy(&message).contains("Refused Bybit execution row") {
                    observer.set(observer.get() + 1);
                    let mut error = ExchangeError::new("refusal listener failed");
                    error.name = "RefusalObserverError".into();
                    error.code = Some(captured.clone());
                    return Err(error);
                }
                Ok(())
            }),
        );
        let error = h
            .drive(h.venue().fetch_historical_fills(options()))
            .unwrap_err();
        assert_eq!(
            called.get(),
            1,
            "BYBIT-HISTORY-LISTENER refusal was emitted"
        );
        assert_eq!(
            error.message, "refusal listener failed",
            "BYBIT-HISTORY-LISTENER source catch must not swallow it"
        );
        assert_eq!(error.name, "RefusalObserverError");
        assert!(
            error.code.as_ref().unwrap().strict_equals(&code),
            "BYBIT-HISTORY-LISTENER native error code identity"
        );
        assert_eq!(h.wire.calls("/v5/order/history").len(), 1);
        h.venue().dispose().unwrap();
    }
}
