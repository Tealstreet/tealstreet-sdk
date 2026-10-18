#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_num::{JsCoercionError, numeric_ingress::JsNumberInput};
use safe_cex_types::{Execution, IncomeCategory, IncomeRecord, OrderSide};
use safe_cex_venue_kraken::{fills, income};
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};

fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/utilities-oracle.cjs"
            ))
            .env("TZ", "UTC")
            .output()
            .expect("utilities actual TypeScript must execute");
        assert!(
            output.status.success(),
            "utilities actual TypeScript: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        eprintln!("utilities capture header: {}", value["header"]);
        assert!(
            value["header"]["node"].as_str().unwrap().starts_with('v'),
            "Node capture version"
        );
        assert!(
            !value["header"]["arch"].as_str().unwrap().is_empty(),
            "CPU capture architecture"
        );
        assert_eq!(value["header"]["tz"], "UTC", "explicit date host");
        cardinality::source_census("utilities", &value);
        value
    })
}
fn number(value: f64) -> Value {
    json!({"number": format!("{:016x}", if value.is_nan() { 0x7ff8000000000000 } else { value.to_bits() })})
}
fn optional(value: Option<f64>) -> Value {
    value
        .map(number)
        .unwrap_or_else(|| json!({"undefined":true}))
}
fn opt_string(value: &Option<String>) -> Value {
    value
        .as_ref()
        .map_or_else(|| json!({"undefined":true}), |v| json!(v))
}
fn caught<T>(value: Result<T, JsCoercionError>, capture: impl FnOnce(T) -> Value) -> Value {
    match value {
        Ok(v) => capture(v),
        Err(e) => json!({"error":e.name,"message":e.message}),
    }
}
fn failure(error: Box<dyn std::error::Error>) -> Value {
    let error = error
        .downcast::<JsCoercionError>()
        .expect("unexpected wire decoder refusal");
    json!({"error":error.name,"message":error.message})
}
fn execution(v: &Execution) -> Value {
    // Observe native fields and enum variants BEFORE any Serialize implementation.
    assert!(
        v.commission.is_none()
            && v.commission_asset.is_none()
            && v.position_side.is_none()
            && v.venue_reported_revenue.is_none()
            && v.venue_reported_revenue_asset.is_none(),
        "fill unreported fields remain absent"
    );
    json!({"id": opt_string(&v.id), "timestamp":number(v.timestamp),"symbol":v.symbol,"orderId":opt_string(&v.order_id),
        "price":number(v.price),"qty":number(v.qty),"side":match v.side {OrderSide::Buy=>"buy",OrderSide::Sell=>"sell"},
        "maker":v.maker,"realizedPnl":optional(v.realized_pnl)})
}
fn record(v: &IncomeRecord) -> Value {
    let category = match v.category.as_ref().expect("income category present") {
        IncomeCategory::RealizedPnl => "realized_pnl",
        IncomeCategory::Funding => "funding",
        IncomeCategory::Commission => "commission",
        IncomeCategory::Liquidation => "liquidation",
        IncomeCategory::Transfer => "transfer",
        IncomeCategory::Other => "other",
        IncomeCategory::Rebate => "rebate",
    };
    json!({"symbol":v.symbol,"incomeType":v.income_type,"asset":v.asset,"timestamp":number(v.timestamp),
        "tradeId":opt_string(&v.trade_id),"tranId":v.tran_id,"income":number(v.income),"category":category})
}
fn raw_value(case: &Value) -> Option<Value> {
    case["raw"]
        .as_str()
        .map(|s| serde_json::from_str(s).unwrap())
}
fn input(case: &Value) -> JsNumberInput {
    serde_json::from_str(case["raw"].as_str().unwrap()).unwrap()
}

#[test]
fn scalar_utilities_live_oracle() {
    for case in cardinality::cases("utilities", "scalars", oracle()) {
        let v = raw_value(case);
        assert_eq!(
            caught(fills::num(v.as_ref()), number),
            case["num"],
            "fills.num raw={}",
            case["raw"]
        );
        assert_eq!(
            caught(fills::optional_num(v.as_ref()), optional),
            case["optional"],
            "fills.optionalNum raw={}",
            case["raw"]
        );
        assert_eq!(
            caught(income::num(v.as_ref()), optional),
            case["income"],
            "income.num raw={}",
            case["raw"]
        );
    }
    for case in cardinality::cases("utilities", "dates", oracle()) {
        let v = raw_value(case);
        assert_eq!(
            caught(fills::to_seconds(v.as_ref(), |ms| ms), number),
            case["seconds"],
            "fills.toSeconds raw={}",
            case["raw"]
        );
        assert_eq!(
            caught(income::to_ms(v.as_ref(), |ms| ms), number),
            case["ms"],
            "income.toMs raw={}",
            case["raw"]
        );
    }
    for case in cardinality::cases("utilities", "legs", oracle()) {
        let v = raw_value(case).unwrap();
        assert_eq!(
            caught(income::is_position_size_leg(&v), |b| json!(b)),
            case["result"],
            "income.isPositionSizeLeg raw={}",
            case["raw"]
        );
    }
}

#[test]
fn mapped_utilities_live_oracle() {
    for (corpus, name) in [
        ("fills", "fills.mapKrakenFill"),
        ("income", "income.mapKrakenAccountLogRow"),
        ("fillBatches", "fills.mapKrakenFills"),
        ("logBatches", "income.mapKrakenAccountLog"),
    ] {
        for case in cardinality::cases("utilities", corpus, oracle()) {
            let mut calls = Vec::new();
            let mut resolve = |s: &str| {
                calls.push(s.to_owned());
                match s {
                    "PF_XRPUSD" => Ok(Some("XRP/USD".to_owned())),
                    "EMPTY" => Ok(Some(String::new())),
                    "THROW" => Err(Box::new(JsCoercionError {
                        name: "Error",
                        message: "resolver throws",
                    }) as Box<dyn std::error::Error>),
                    _ => Ok(None),
                }
            };
            let mut utc = |ms| ms;
            let result = match corpus {
                "fills" => fills::map_kraken_fill(input(case), &mut resolve, &mut utc)
                    .map(|v| v.as_ref().map_or(Value::Null, execution)),
                "income" => income::map_kraken_account_log_row(input(case), &mut resolve, &mut utc)
                    .map(|v| {
                        cardinality::native_len(
                            v.len(),
                            &case["result"],
                            &format!("{name} decoded native fields"),
                        );
                        Value::Array(v.iter().map(record).collect())
                    }),
                "fillBatches" => {
                    fills::map_kraken_fills(input(case), &mut resolve, &mut utc).map(|v| {
                        cardinality::native_len(
                            v.len(),
                            &case["result"],
                            &format!("{name} decoded native fields"),
                        );
                        Value::Array(v.iter().map(execution).collect())
                    })
                }
                "logBatches" => income::map_kraken_account_log(input(case), &mut resolve, &mut utc)
                    .map(|v| {
                        cardinality::native_len(
                            v.len(),
                            &case["result"],
                            &format!("{name} decoded native fields"),
                        );
                        Value::Array(v.iter().map(record).collect())
                    }),
                _ => unreachable!(),
            }
            .unwrap_or_else(failure);
            assert_eq!(
                result, case["result"],
                "{name} decoded native fields raw={}",
                case["raw"]
            );
            assert_eq!(
                json!(calls),
                case["calls"],
                "{name} resolver calls raw={}",
                case["raw"]
            );
        }
    }
}

#[test]
fn numeric_capture_and_boundary() {
    assert_eq!(
        number(-0.0),
        oracle()["capture"]["negativeZero"],
        "signed zero capture"
    );
    assert_eq!(number(f64::NAN), oracle()["capture"]["nan"], "NaN capture");
    assert_eq!(
        number(-f64::NAN),
        oracle()["capture"]["negativeNan"],
        "negative NaN capture"
    );
    let raw = r#"{"fill_id":"\ud800"}"#;
    let retained: safe_cex_types::retained_json::RetainedJsonInput =
        serde_json::from_str(raw).unwrap();
    assert_eq!(
        retained.get("fill_id").unwrap().as_text(),
        Some([0xd800].as_slice())
    );
    let input: JsNumberInput = serde_json::from_str(raw).unwrap();
    assert!(
        fills::map_kraken_fill(input, &mut |_| Ok(None), &mut |ms| ms).is_err(),
        "legacy fill wire refuses only when consumed"
    );
}

#[test]
fn chosen_boundary_live_witnesses() {
    let boundaries = oracle()["boundaries"].as_array().unwrap();
    assert_eq!(boundaries.len(), 12, "boundary row census");
    for case in cardinality::cases("utilities", "boundaries", oracle()) {
        let raw = case["raw"].as_str().unwrap();
        let name = case["name"].as_str().unwrap();
        if name.ends_with(".depth") {
            assert!(
                serde_json::from_str::<JsNumberInput>(raw).is_err(),
                "depth refusal {name}"
            );
        } else {
            let input: JsNumberInput = serde_json::from_str(raw).unwrap();
            let retained: safe_cex_types::retained_json::RetainedJsonInput =
                serde_json::from_str(raw).unwrap();
            let row = retained.as_array().map_or(&retained, |rows| &rows[0]);
            let unit = if row.as_text().is_some() {
                row
            } else {
                let key = match name {
                    "income.isPositionSizeLeg" => "asset",
                    "fills.mapKrakenFill" | "fills.mapKrakenFills" => "fill_id",
                    _ => "booking_uid",
                };
                row.get(key).unwrap()
            };
            assert_eq!(
                unit.as_text(),
                Some([0xd800].as_slice()),
                "native units {name}"
            );
            match name {
                "fills.mapKrakenFill" => {
                    assert!(fills::map_kraken_fill(input, &mut |_| Ok(None), &mut |ms| ms).is_err())
                }
                "fills.mapKrakenFills" => assert!(
                    fills::map_kraken_fills(input, &mut |_| Ok(None), &mut |ms| ms).is_err()
                ),
                "income.mapKrakenAccountLogRow" => assert!(
                    income::map_kraken_account_log_row(input, &mut |_| Ok(None), &mut |ms| ms)
                        .is_err()
                ),
                "income.mapKrakenAccountLog" => assert!(
                    income::map_kraken_account_log(input, &mut |_| Ok(None), &mut |ms| ms).is_err()
                ),
                "income.isPositionSizeLeg" => assert!(
                    input
                        .decode::<std::collections::BTreeMap<String, String>>()
                        .is_err(),
                    "closed leg text admission"
                ),
                _ => assert!(
                    input.decode::<String>().is_err(),
                    "closed scalar text admission {name}"
                ),
            }
        }
        match case["name"].as_str().unwrap() {
            "fills.num" | "fills.toSeconds" | "income.toMs" => {
                assert_eq!(case["output"], number(0.0), "TS surrogate numeric output")
            }
            "fills.optionalNum" | "income.num" => assert_eq!(
                case["output"],
                optional(None),
                "TS surrogate optional output"
            ),
            "income.isPositionSizeLeg" => {
                assert_eq!(case["output"], true, "TS surrogate leg equality")
            }
            "fills.mapKrakenFill" => assert_eq!(
                case["output"]["id"],
                json!({"units":[55296]}),
                "TS surrogate fill id"
            ),
            "fills.mapKrakenFills" => assert_eq!(
                case["output"][0]["id"],
                json!({"units":[55296]}),
                "TS surrogate batch fill id"
            ),
            "income.mapKrakenAccountLogRow" | "income.mapKrakenAccountLog" => assert_eq!(
                case["output"][0]["tranId"]["units"][0], 55296,
                "TS surrogate income id"
            ),
            "fills.mapKrakenFill.depth" => assert_eq!(
                case["output"]["price"],
                number(3.125),
                "TS deep ignored field fill acceptance"
            ),
            "income.mapKrakenAccountLogRow.depth" => assert_eq!(
                case["output"].as_array().unwrap().len(),
                4,
                "TS deep ignored field income acceptance"
            ),
            _ => panic!("unknown boundary witness"),
        }
    }
    assert_eq!(
        oracle()["nonJson"]["bigint"],
        number(9007199254740992.0),
        "TS explicit BigInt Number accepts"
    );
    assert_eq!(
        oracle()["nonJson"]["object"],
        number(4.125),
        "TS executable Number hook accepts"
    );
    assert_eq!(
        oracle()["nonJson"]["hooks"],
        json!(["number"]),
        "TS number-hint callback"
    );
    assert_eq!(
        oracle()["nonJson"]["resolverThrow"]["error"],
        "Error",
        "TS resolver exception"
    );
    for case in cardinality::cases("utilities", "localDates", oracle()) {
        let value = raw_value(case);
        assert_eq!(
            caught(
                fills::to_seconds(value.as_ref(), |ms| ms - 7_200_000.0),
                number
            ),
            case["seconds"],
            "fills.toSeconds non-UTC adapter"
        );
        assert_eq!(
            caught(income::to_ms(value.as_ref(), |ms| ms - 7_200_000.0), number),
            case["ms"],
            "income.toMs non-UTC adapter"
        );
    }
}

#[test]
fn consuming_fills_decode_cardinality() {
    let case = &oracle()["fillBatches"][0];
    let mut resolve = |symbol: &str| -> Result<Option<String>, Box<dyn std::error::Error>> {
        Ok(match symbol {
            "PF_XRPUSD" => Some("XRP/USD".to_owned()),
            "EMPTY" => Some(String::new()),
            _ => None,
        })
    };
    let values = fills::map_kraken_fills(input(case), &mut resolve, &mut |ms| ms).unwrap();
    assert_eq!(
        values.len(),
        2,
        "KRAKEN-CONSUMING fills independent native length"
    );
    cardinality::native_len(
        values.len(),
        &case["result"],
        "KRAKEN-CONSUMING fills source/native length",
    );
    let mut comparisons = 0;
    for index in 0..2 {
        let value = values
            .get(index)
            .expect("KRAKEN-CONSUMING fills native index coverage");
        assert_eq!(
            execution(value),
            case["result"][index],
            "KRAKEN-CONSUMING fills native record"
        );
        comparisons += 1;
    }
    assert_eq!(comparisons, 2, "KRAKEN-CONSUMING fills comparison count");
}

#[test]
fn consuming_income_decode_cardinality() {
    let case = &oracle()["logBatches"][0];
    let mut resolve = |symbol: &str| -> Result<Option<String>, Box<dyn std::error::Error>> {
        Ok(match symbol {
            "PF_XRPUSD" => Some("XRP/USD".to_owned()),
            "EMPTY" => Some(String::new()),
            _ => None,
        })
    };
    let values = income::map_kraken_account_log(input(case), &mut resolve, &mut |ms| ms).unwrap();
    assert_eq!(
        values.len(),
        5,
        "KRAKEN-CONSUMING income independent native length"
    );
    cardinality::native_len(
        values.len(),
        &case["result"],
        "KRAKEN-CONSUMING income source/native length",
    );
    let mut comparisons = 0;
    for index in 0..5 {
        let value = values
            .get(index)
            .expect("KRAKEN-CONSUMING income native index coverage");
        assert_eq!(
            record(value),
            case["result"][index],
            "KRAKEN-CONSUMING income native record"
        );
        comparisons += 1;
    }
    assert_eq!(comparisons, 5, "KRAKEN-CONSUMING income comparison count");
}
