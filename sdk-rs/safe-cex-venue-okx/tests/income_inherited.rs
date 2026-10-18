use safe_cex_num::JsCoercionError;
use safe_cex_venue_okx::income::{map_okx_bill_to_income_records, okx_bill_category};
use serde_json::Value;

fn canonical_numbers(value: Value) -> Value {
    match value {
        Value::Number(number) => {
            serde_json::from_str(&safe_cex_num::js_number_string(number.as_f64().unwrap())).unwrap()
        }
        Value::Array(values) => Value::Array(values.into_iter().map(canonical_numbers).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, canonical_numbers(value)))
                .collect(),
        ),
        value => value,
    }
}

#[test]
fn inherited_income_refuses_only_emitted_category_after_source_effects() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/income_inherited_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 60, "income::independent source floor");
    let mut compared = 0;
    for case in cases {
        let key = case["key"].as_str().unwrap();
        assert_eq!(case["identity"], true, "income::source identity {key}");
        assert_eq!(
            case["type"],
            if key == "__proto__" {
                "object"
            } else {
                "function"
            },
            "income::source type {key}"
        );
        assert_eq!(
            okx_bill_category(&serde_json::json!(key))
                .expect_err("income::helper inherited output must refuse")
                .name,
            "RepresentationError",
            "income::helper inherited refusal {key}"
        );
        let calls = std::cell::Cell::new(0);
        let result = map_okx_bill_to_income_records(case["row"].as_object().unwrap(), |_| {
            calls.set(calls.get() + 1);
            Ok::<_, JsCoercionError>(Some("BTCUSDT".into()))
        });
        assert_eq!(
            calls.get(),
            1,
            "income::native resolver precedes refusal {key}"
        );
        match case["mode"].as_str().unwrap() {
            "empty" => assert!(
                result
                    .expect("income::empty inherited bill must succeed")
                    .is_empty(),
                "income::empty inherited bill succeeds {key}"
            ),
            "fee" => {
                let records = result.expect("income::fee-only inherited bill must succeed");
                assert_eq!(records.len(), 1, "income::fee record floor {key}");
                assert_eq!(
                    records[0].category,
                    Some(safe_cex_types::IncomeCategory::Commission),
                    "income::fee bypasses inherited category {key}"
                );
                assert_eq!(
                    records[0].income.to_bits(),
                    (-1.0f64).to_bits(),
                    "income::native fee value {key}"
                );
                assert_eq!(
                    records[0].timestamp.to_bits(),
                    123.0f64.to_bits(),
                    "income::native fee timestamp {key}"
                );
                assert_eq!(
                    records[0].symbol, "BTCUSDT",
                    "income::native fee symbol {key}"
                );
                assert_eq!(
                    records[0].income_type, "COMMISSION",
                    "income::native fee income type {key}"
                );
                assert_eq!(records[0].asset, "USDT", "income::native fee asset {key}");
                assert_eq!(
                    records[0].tran_id, "bill:fee",
                    "income::native fee transaction identity {key}"
                );
                assert_eq!(
                    records[0].trade_id, None,
                    "income::native fee trade absence {key}"
                );
                assert_eq!(
                    canonical_numbers(serde_json::to_value(records).unwrap()),
                    canonical_numbers(case["records"].clone()),
                    "income::fee wire after native assertions {key}"
                );
            }
            "error" => {
                let error = result.expect_err("income::native inherited bill must refuse");
                assert_eq!(
                    error.name, "TypeError",
                    "income::later source coercion wins {key}"
                );
                assert_eq!(
                    error.message, case["error"]["message"],
                    "income::exact source error {key}"
                );
            }
            "pnl" | "adj" => {
                let error = result.expect_err("income::native inherited bill must refuse");
                assert_eq!(
                    error.name, "RepresentationError",
                    "income::emitted inherited category refuses {key}"
                );
                assert_eq!(
                    error.message,
                    "OKX inherited income category cannot cross the closed typed output boundary",
                    "income::refusal message {key}"
                );
            }
            _ => unreachable!(),
        }
        compared += 1;
    }
    assert_eq!(compared, 60, "income::independent native comparison floor");
}
