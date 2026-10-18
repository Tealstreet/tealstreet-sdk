//! Closed String projections reached through constructed clients and native inputs.
use super::accepted_read_receivers::terminal::{Harness, harness, options};
use crate::{private_accounts::IncomeHost, rest_account::ReadHost};
use safe_cex_exchange::js_runtime::{
    promise::await_js,
    value::{Js, from_retained_input, to_retained_input},
};
use safe_cex_types::{ExchangeName, retained_json::RetainedJsonInput};
use serde_json::Value;
use std::{process::Command, rc::Rc};
fn source(script: &str) -> Value {
    let output = Command::new("node")
        .arg(format!("{}/tests/{script}", env!("CARGO_MANIFEST_DIR")))
        .env("TZ", "UTC")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn parse(raw: &str) -> Js {
    from_retained_input(serde_json::from_str::<RetainedJsonInput>(raw).unwrap())
}
fn member(h: &Harness, name: &str) -> Js {
    let pending = h.exchange.rt.this.call_method(name, vec![]).unwrap();
    let runtime: Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> = h.runtime.clone();
    h.drive(Box::pin(async move {
        await_js(&runtime, pending)
            .await
            .map_err(|e| e.to_exchange_error())
    }))
    .unwrap()
}
fn assert_rows(actual: &[Js], expected: &[Value], closed_strings: bool) {
    assert_eq!(actual.len(), expected.len());
    for (row, source) in actual.iter().zip(expected) {
        for (key, expected) in source.as_object().unwrap() {
            let field = row.get(key).unwrap();
            if let Some(bits) = expected.get("number") {
                assert_eq!(
                    format!("{:016x}", field.as_f64().unwrap().to_bits()),
                    bits.as_str().unwrap(),
                    "native {key}"
                );
            } else if let Some(units) = expected.get("units") {
                let units: Vec<u16> = units
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
                    .collect();
                let projected: Vec<u16> = if closed_strings {
                    String::from_utf16_lossy(&units).encode_utf16().collect()
                } else {
                    units
                };
                assert_eq!(
                    field.as_str().unwrap().as_ref(),
                    projected.as_slice(),
                    "typed {key}"
                );
            } else if expected.get("undefined") == Some(&Value::Bool(true)) {
                assert!(field.is_undefined(), "absent {key}");
            } else {
                let Js::Bool(actual) = field else {
                    panic!("native {key} must remain Boolean");
                };
                assert_eq!(actual, expected.as_bool().unwrap(), "native {key}");
            }
        }
    }
}
#[test]
fn token_and_leverage_bindings_project_only_the_closed_output() {
    let source = source("reads-cardinality-oracle.cjs");
    let token_body = parse(source["boundaries"]["surrogate"]["raw"].as_str().unwrap());
    assert_eq!(
        source["boundaries"]["surrogate"]["units"],
        serde_json::json!([55296])
    );
    let h = harness(ExchangeName::Krakenspot, options(), |http| {
        http.route_native(
            "POST",
            "/0/private/GetWebSocketsToken",
            200.0,
            token_body.clone(),
        );
    });
    assert_eq!(
        member(&h, "fetchWsToken").as_str().unwrap().as_ref(),
        &[0xfffd]
    );
    let raw = to_retained_input(&token_body).unwrap();
    assert_eq!(
        raw.get("result").unwrap().get("token").unwrap().as_text(),
        Some([0xd800].as_slice())
    );
    assert_eq!(h.http.calls("/0/private/GetWebSocketsToken").len(), 1);
    assert!(h.runtime.reports.borrow().is_empty());
    let leverage_body = parse(source["leverageBoundary"]["raw"].as_str().unwrap());
    assert_eq!(
        source["leverageBoundary"]["symbolUnits"],
        serde_json::json!([55296])
    );
    let h = harness(ExchangeName::Kraken, options(), |http| {
        http.route_native(
            "GET",
            "/v3/leveragepreferences",
            200.0,
            leverage_body.clone(),
        );
    });
    let cache = h.exchange.rt.this.get("leverageSettings").unwrap();
    cache
        .call_method("set", vec![Js::str("OLD"), Js::Number(2.5)])
        .unwrap();
    assert!(member(&h, "fetchLeverageSettings").is_undefined());
    assert!(
        cache
            .call_method("get", vec![Js::str("OLD")])
            .unwrap()
            .is_undefined()
    );
    assert_eq!(
        cache
            .call_method("get", vec![Js::str("\u{fffd}")])
            .unwrap()
            .as_f64()
            .unwrap()
            .to_bits(),
        17.25f64.to_bits()
    );
    assert!(
        cache
            .call_method("get", vec![Js::utf16(&[0xd800])])
            .unwrap()
            .is_undefined()
    );
    let raw = to_retained_input(&leverage_body).unwrap();
    assert_eq!(
        raw.get("leveragePreferences").unwrap().as_array().unwrap()[0]
            .get("symbol")
            .unwrap()
            .as_text(),
        Some([0xd800].as_slice())
    );
    assert_eq!(h.http.calls("/v3/leveragepreferences").len(), 1);
    assert!(h.runtime.console.borrow().is_empty());
    assert!(h.runtime.reports.borrow().is_empty());
}
#[test]
fn fill_and_income_bindings_keep_raw_units_before_typed_model_projection() {
    let source = source("utilities-oracle.cjs");
    let boundary = |name: &str| {
        source["boundaries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == name)
            .unwrap()
    };
    let fills = boundary("fills.mapKrakenFills");
    let input = parse(fills["raw"].as_str().unwrap());
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    // The actual Scope host called by rest_account::fetch_historical_fills.
    let actual = ReadHost::map_fills(&h.exchange.rt, Rc::new(input.clone())).unwrap();
    assert_rows(
        &actual.iter().map(|row| (**row).clone()).collect::<Vec<_>>(),
        fills["output"].as_array().unwrap(),
        false,
    );
    assert_eq!(
        actual[0].get("id").unwrap().as_str().unwrap().as_ref(),
        &[0xd800],
        "the native mapper preserves source units before the Exchange trait output"
    );
    let typed: safe_cex_types::Execution =
        safe_cex_exchange::js_runtime::value::to_typed(&actual[0]).unwrap();
    assert_eq!(
        typed.id.as_deref(),
        Some("\u{fffd}"),
        "the actual invoke projection closes Execution strings"
    );
    let retained = to_retained_input(&input).unwrap();
    assert_eq!(
        retained.as_array().unwrap()[0]
            .get("fill_id")
            .unwrap()
            .as_text(),
        Some([0xd800].as_slice())
    );
    let income = boundary("income.mapKrakenAccountLog");
    let logs = parse(income["raw"].as_str().unwrap());
    let body = Js::object(vec![("logs", logs)]);
    let h = harness(ExchangeName::Kraken, options(), |http| {
        http.route_native("GET", "/history/v3/account-log", 200.0, body.clone());
    });
    let pending = IncomeHost::account_log_page(
        &h.exchange.rt,
        Rc::new(Js::object(vec![
            ("startTime", Js::Number(1.0)),
            ("limit", Js::Number(37.0)),
        ])),
    )
    .unwrap();
    let actual = h
        .drive(Box::pin(async {
            pending.await.map_err(|e| e.to_exchange_error())
        }))
        .unwrap();
    assert_rows(
        &actual
            .array_values()
            .expect("fixture array property reads")
            .unwrap(),
        income["output"].as_array().unwrap(),
        true,
    );
    assert_eq!(
        actual
            .array_values()
            .expect("fixture array property reads")
            .unwrap()
            .len(),
        4
    );
    let raw = to_retained_input(&body).unwrap();
    assert_eq!(
        raw.get("logs").unwrap().as_array().unwrap()[0]
            .get("booking_uid")
            .unwrap()
            .as_text(),
        Some([0xd800].as_slice())
    );
    let requests = h.http.calls("/history/v3/account-log");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url, "/history/v3/account-log?count=37&since=1");
    assert!(h.runtime.reports.borrow().is_empty());
}
