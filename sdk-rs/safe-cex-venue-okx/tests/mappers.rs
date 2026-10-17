use safe_cex_num::JsCoercionError;
#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_types::Market;
use safe_cex_venue_okx::{closed_pnl, income, order_history};
use serde_json::{Value, json};

fn canonical_numbers(value: Value) -> Value {
    match value {
        Value::Number(number) => {
            serde_json::from_str(&safe_cex_num::js_number_string(number.as_f64().unwrap())).unwrap()
        }
        Value::Array(values) => Value::Array(values.into_iter().map(canonical_numbers).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(k, v)| (k, canonical_numbers(v)))
                .collect(),
        ),
        value => value,
    }
}
fn convert(native: f64, market: &Market, price: Option<f64>) -> Result<f64, JsCoercionError> {
    Ok(if market.is_inverse.unwrap_or(false) {
        price
            .filter(|p| *p > 0.0)
            .map_or(0.0, |p| native * market.contract_size.unwrap() / p)
    } else {
        native * market.contract_size.unwrap()
    })
}

#[test]
#[ignore = "requires Node and the isolated TypeScript compiler; mandatory in CI"]
fn maintained_typescript_ledger_history_and_closed_mappers() {
    let oracle = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/mappers-oracle.mjs"
        ))
        .output()
        .expect("Node is required for the live TypeScript oracle");
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&oracle.stdout).unwrap();
    assert_eq!(cases.len(), 1220);
    cardinality::assert_groups(
        &cases,
        "mappers operations",
        |case| case["op"].as_str().unwrap().to_owned(),
        &[
            ("bill_type_codes", 1),
            ("number", 28),
            ("income", 108),
            ("closed", 288),
            ("history", 608),
            ("coercion_error", 5),
            ("income_error", 45),
            ("closed_error", 50),
            ("history_error", 65),
            ("history_effects", 22),
        ],
    );
    let mut corrections = 0;
    for (index, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let expected = case["expected"].clone();
        let actual = match case["op"].as_str().unwrap() {
            "bill_type_codes" => Value::Object(
                income::OKX_BILL_TYPE_BY_INCOME_TYPE
                    .into_iter()
                    .map(|(key, value)| (key.to_owned(), json!(value)))
                    .collect(),
            ),
            "coercion_error" => {
                let error = income::to_number(input).unwrap_err();
                assert_eq!(error.name, case["expected"]["name"]);
                assert_eq!(error.message, case["expected"]["message"]);
                assert_eq!(closed_pnl::to_number(input).unwrap_err(), error);
                assert_eq!(order_history::to_number(input).unwrap_err(), error);
                assert_eq!(order_history::to_optional_number(input).unwrap_err(), error);
                continue;
            }
            "history_effects" => {
                let market: Market = serde_json::from_value(input["market"].clone()).unwrap();
                let calls = std::cell::RefCell::new(Vec::new());
                let error = order_history::map_okx_historical_order(
                    input["row"].as_object().unwrap(),
                    Some(&market),
                    |n, m, p| {
                        calls.borrow_mut().push(json!({"native":n,"reference":p}));
                        convert(n, m, p)
                    },
                )
                .unwrap_err();
                assert_eq!(
                    canonical_numbers(json!(calls.into_inner())),
                    canonical_numbers(case["calls"].clone()),
                    "history conversion effects before {} error",
                    case["field"]
                );
                assert_eq!(
                    error.name, expected["name"],
                    "history exact error name {index}"
                );
                assert_eq!(
                    error.message, expected["message"],
                    "history exact error message {index}"
                );
                continue;
            }
            "income_error" | "closed_error" | "history_error" => {
                let row = input["row"].as_object().unwrap();
                let market: Market = serde_json::from_value(input["market"].clone()).unwrap();
                let error = match case["op"].as_str().unwrap() {
                    "income_error" => income::map_okx_bill_to_income_records(row, |_| {
                        Ok::<_, JsCoercionError>(Some("DOGEUSDT".into()))
                    })
                    .unwrap_err(),
                    "closed_error" => closed_pnl::map_okx_closed_position(
                        row,
                        Some(&market),
                        |n, m, p| convert(n, m, Some(p)),
                        "replay",
                    )
                    .unwrap_err(),
                    _ => order_history::map_okx_historical_order(row, Some(&market), convert)
                        .unwrap_err(),
                };
                assert_eq!(error.name, expected["name"], "case {index}: {case}");
                assert_eq!(error.message, expected["message"], "case {index}: {case}");
                continue;
            }
            "number" => {
                assert_eq!(
                    income::to_number(input).unwrap(),
                    closed_pnl::to_number(input).unwrap()
                );
                assert_eq!(
                    income::to_number(input).unwrap(),
                    order_history::to_number(input).unwrap()
                );
                let mut result = json!({"number":income::to_number(input).unwrap()});
                if let Some(optional) = order_history::to_optional_number(input).unwrap() {
                    result["optional"] = json!(optional);
                }
                result
            }
            "income" => {
                let result =
                    income::map_okx_bill_to_income_records(input.as_object().unwrap(), |id| {
                        Ok::<_, JsCoercionError>(
                            (id == "DOGE-USDT-SWAP").then(|| "DOGEUSDT".into()),
                        )
                    });
                if case["corrected"] == true {
                    let bad = case["sourcePrototype"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|record| record["categoryType"] != "string")
                        .count();
                    if bad > 0 {
                        let Err(error) = result else {
                            panic!(
                                "inherited category must refuse typed output after source computations"
                            );
                        };
                        assert_eq!(error.name, "RepresentationError");
                        assert_eq!(
                            error.message,
                            "OKX inherited income category cannot cross the closed typed output boundary"
                        );
                        corrections += bad;
                        continue;
                    }
                }
                let records = result.unwrap();
                serde_json::to_value(records).unwrap()
            }
            "closed" => {
                let market = (!input["market"].is_null())
                    .then(|| serde_json::from_value::<Market>(input["market"].clone()).unwrap());
                serde_json::to_value(
                    closed_pnl::map_okx_closed_position(
                        input["row"].as_object().unwrap(),
                        market.as_ref(),
                        |n, m, p| convert(n, m, Some(p)),
                        "replay",
                    )
                    .unwrap(),
                )
                .unwrap()
            }
            "history" => {
                let market = (!input["market"].is_null())
                    .then(|| serde_json::from_value::<Market>(input["market"].clone()).unwrap());
                if case["corrected"] == true {
                    let error = order_history::map_okx_historical_order(
                        input["row"].as_object().unwrap(),
                        market.as_ref(),
                        convert,
                    )
                    .expect_err("history inherited output must refuse at closed boundary");
                    assert_eq!(
                        error.name, "RepresentationError",
                        "history closed-output refusal name {index}"
                    );
                    assert_eq!(
                        error.message,
                        "OKX inherited history field cannot cross the closed typed output boundary",
                        "history closed-output refusal message {index}"
                    );
                    let prototype = &case["sourcePrototype"];
                    assert!(
                        prototype["type"] == "function"
                            || prototype["status"] == "function"
                            || prototype["typePrototype"] == true
                            || prototype["statusPrototype"] == true,
                        "source pre-serialization inherited history witness {index}"
                    );
                    corrections += 1;
                    continue;
                }
                serde_json::to_value(
                    order_history::map_okx_historical_order(
                        input["row"].as_object().unwrap(),
                        market.as_ref(),
                        convert,
                    )
                    .unwrap(),
                )
                .unwrap()
            }
            operation => panic!("unknown oracle operation {operation}"),
        };
        assert_eq!(
            canonical_numbers(actual),
            canonical_numbers(expected),
            "mapper case {index}: {case}"
        );
    }
    assert_eq!(corrections, 153);
}
