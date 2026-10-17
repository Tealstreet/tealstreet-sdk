use safe_cex_num::JsCoercionError;
#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_venue_okx::position_adapter::{OkxPositionAdapterDeps, map_okx_position_history};
use serde_json::{Value, json};
use std::cell::RefCell;

#[test]
#[ignore = "requires Node and the isolated TypeScript compiler; mandatory in CI"]
fn maintained_typescript_round_trip_position_adapter() {
    let oracle = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/position-oracle.mjs"
        ))
        .output()
        .expect("Node is required for the live TypeScript oracle");
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&oracle.stdout).unwrap();
    assert_eq!(cases.len(), 310);
    cardinality::assert_groups(
        &cases,
        "position_adapter profiles",
        |case| {
            format!(
                "{}:{}",
                case["known"],
                if case.get("error").is_some_and(|error| !error.is_null()) {
                    "error"
                } else if case["record"].is_null() {
                    "none"
                } else {
                    "record"
                }
            )
        },
        &[
            ("true:record", 206),
            ("true:error", 80),
            ("false:none", 16),
            ("true:none", 8),
        ],
    );
    for (index, case) in cases.iter().enumerate() {
        let calls = RefCell::new(Vec::new());
        let to_base = |n, id: &str| {
            calls.borrow_mut().push(json!(["base", n, id]));
            Ok::<_, JsCoercionError>(n * case["scale"].as_f64().unwrap())
        };
        let symbol = |id: &str| {
            calls.borrow_mut().push(json!(["symbol", id]));
            Ok::<_, JsCoercionError>((case["known"] == true).then(|| "DOGEUSDC".into()))
        };
        let settle = |id: &str| {
            calls.borrow_mut().push(json!(["settle", id]));
            Ok::<_, JsCoercionError>(Some("USDC".into()))
        };
        let deps = OkxPositionAdapterDeps {
            account: "replay",
            to_base: &to_base,
            resolve_symbol: &symbol,
            resolve_settle_currency: &settle,
        };
        if let Some(expected) = case.get("error") {
            let error =
                map_okx_position_history(case["row"].as_object().unwrap(), &deps).unwrap_err();
            assert_eq!(error.name, expected["name"], "position case {index}");
            assert_eq!(error.message, expected["message"], "position case {index}");
            continue;
        }
        let record = map_okx_position_history(case["row"].as_object().unwrap(), &deps)
            .unwrap()
            .map(|record| {
                let native = safe_cex_num::JsJsonValue::Object(record.into_iter().collect());
                serde_json::from_str(&safe_cex_num::js_json_stringify(&native).unwrap().unwrap())
                    .unwrap()
            })
            .unwrap_or(Value::Null);
        assert_eq!(
            serde_json::to_string(&record).unwrap(),
            serde_json::to_string(&case["record"]).unwrap(),
            "position record {index}: {case}"
        );
        let mut actual_calls = serde_json::to_value(calls.into_inner()).unwrap();
        // Callback numbers are exact binary64; canonicalize only their JSON spelling.
        for call in actual_calls.as_array_mut().unwrap() {
            if call[0] == "base" {
                call[1] = serde_json::from_str(&safe_cex_num::js_number_string(
                    call[1].as_f64().unwrap(),
                ))
                .unwrap();
            }
        }
        assert_eq!(actual_calls, case["calls"], "callback order {index}");
    }
}
