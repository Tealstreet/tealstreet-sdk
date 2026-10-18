use super::*;
use safe_cex_exchange::js_runtime::value::{arg, from_json};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
fn observe(value: &Js) -> Value {
    if value.is_undefined() {
        return json!({"undefined":true});
    }
    if value.is_callable() {
        return json!({"inherited":"function"});
    }
    match value {
        Js::Number(n) => {
            json!({"number":if n.is_nan() { "nan".into() } else { format!("{:016x}",n.to_bits()) }})
        }
        Js::Array(_) => Value::Array(value.array_values().expect("fixture array property reads").unwrap().iter().map(observe).collect()),
        value if value.is_object() => Value::Object(
            value
                .own_entries()
                .unwrap()
                .into_iter()
                .map(|(k, v)| (String::from_utf16(&k).unwrap(), observe(&v)))
                .collect(),
        ),
        _ => safe_cex_exchange::js_runtime::value::to_json(value).unwrap(),
    }
}
#[test]
#[ignore = "requires installed TypeScript; mandatory venue-oracle:bybit"]
fn journal_records_match_source_native_values() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/journal-record-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 595);
    let mut outcomes = [0, 0];
    for (index, case) in cases.iter().enumerate() {
        let trace = Rc::new(RefCell::new(vec![]));
        let deps = Js::object(vec![("account", Js::str("acct"))]);
        let observed = trace.clone();
        deps.define(
            "resolveSettleCurrency",
            Js::function("resolveSettleCurrency", move |_, args| {
                let symbol = arg(&args, 0);
                observed
                    .borrow_mut()
                    .push(Js::array(vec![Js::str("currency"), symbol.clone()]));
                Ok(Js::str(if symbol.strict_equals(&Js::str("BTCUSDC")) {
                    "USDC"
                } else {
                    "USDT"
                }))
            }),
        )
        .unwrap();
        let mode = case["mode"].as_str().unwrap();
        if !matches!(mode, "plain" | "context-only") {
            let observed = trace.clone();
            deps.define(
                "resolveLeg",
                Js::function("resolveLeg", move |_, args| {
                    let id = arg(&args, 0);
                    observed
                        .borrow_mut()
                        .push(Js::array(vec![Js::str("leg"), id.clone()]));
                    Ok(if id.strict_equals(&Js::str("two")) {
                        Js::str("short")
                    } else {
                        Js::Undefined
                    })
                }),
            )
            .unwrap();
        }
        if !matches!(mode, "plain" | "leg-only") {
            let observed = trace.clone();
            let missing = mode == "missing";
            let attributed = mode != "unattributed";
            deps.define(
                "resolveContext",
                Js::function("resolveContext", move |_, args| {
                    let context = arg(&args, 0);
                    observed.borrow_mut().push(Js::array(vec![
                        Js::str("context"),
                        context.get("symbol")?,
                        context.get("positionSide")?,
                        context.get("closedAt")?,
                    ]));
                    Ok(if missing {
                        Js::Undefined
                    } else {
                        Js::object(vec![
                            ("openedAt", Js::Number(1699990000321.)),
                            ("funding", Js::Number(-0.625)),
                            ("fundingAttributed", Js::Bool(attributed)),
                        ])
                    })
                }),
            )
            .unwrap();
        }
        let deps = BybitPositionAdapterDeps(deps);
        let rows = from_json(&case["rows"]);
        let result = if case["op"] == "fold" {
            fold_run(&rows, &deps).map(|v| Js::array(if v.is_nullish() { vec![] } else { vec![v] }))
        } else {
            aggregate_bybit_closed_pnl(&rows, &deps)
        };
        let expected = &case["result"];
        if expected.get("error").is_some() {
            let error = result.unwrap_err().to_exchange_error();
            assert_eq!(
                error.name,
                expected["error"].as_str().unwrap(),
                "case{index}"
            );
            assert_eq!(
                error.message,
                expected["message"].as_str().unwrap(),
                "case{index}"
            );
            outcomes[0] += 1;
        } else {
            let actual =
                result.unwrap_or_else(|e| panic!("case{index}: {}", e.to_exchange_error().message));
            assert_eq!(observe(&actual), expected["ok"], "case{index}");
            outcomes[1] += 1;
        }
        assert_eq!(
            observe(&Js::array(trace.borrow().clone())),
            case["trace"],
            "callback trace{index}"
        );
    }
    assert_eq!(outcomes, [29, 566]);
    let numbers = fixture["numbers"].as_array().unwrap();
    assert_eq!(numbers.len(), 20);
    for case in numbers {
        let value = case["raw"]
            .as_str()
            .map(|s| from_json(&serde_json::from_str::<Value>(s).unwrap()))
            .unwrap_or(Js::Undefined);
        match num(&value) {
            Ok(n) => assert_eq!(observe(&Js::Number(n)), case["result"]["ok"]),
            Err(e) => assert_eq!(
                e.to_exchange_error().message,
                case["result"]["message"].as_str().unwrap()
            ),
        }
    }
}
