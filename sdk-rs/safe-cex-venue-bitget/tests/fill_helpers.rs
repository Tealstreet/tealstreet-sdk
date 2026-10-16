use safe_cex_num::{JsCoercionError, js_number_string};
use safe_cex_venue_bitget::fills::{leg_and_direction_of, num, total_fee_of};
use serde_json::{Value, json};

// Source captures use tagged numbers to retain signed zero and nonfinite values.
// Undefined members disappear at this JSON ingress, matching optional reads.
fn decode(value: &Value) -> Option<Value> {
    if value.get("$undefined") == Some(&Value::Bool(true)) {
        return None;
    }
    if let Some(text) = value.get("$number").and_then(Value::as_str) {
        return Some(
            serde_json::from_str(match text {
                "Infinity" => "1e400",
                "-Infinity" => "-1e400",
                other => other,
            })
            .expect("JSON number domain"),
        );
    }
    Some(match value {
        Value::Array(a) => {
            Value::Array(a.iter().map(|v| decode(v).unwrap_or(Value::Null)).collect())
        }
        Value::Object(m) => Value::Object(
            m.iter()
                .filter_map(|(k, v)| decode(v).map(|v| (k.clone(), v)))
                .collect(),
        ),
        other => other.clone(),
    })
}
fn number(n: f64) -> Value {
    json!({"$number":if n == 0.0 && n.is_sign_negative() { "-0".into() } else {js_number_string(n)}})
}
fn result<T>(r: Result<T, JsCoercionError>, encode: impl FnOnce(T) -> Value) -> Value {
    match r {
        Ok(v) => json!({"value":encode(v)}),
        Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
    }
}
fn compare(actual: Value, expected: &Value, name: &str) {
    if let Some(error) = expected.get("error") {
        assert_eq!(actual, json!({"error":error}), "{name}");
    } else {
        assert_eq!(actual, json!({"value":expected["value"]}), "{name}");
    }
}

fn execution(e: safe_cex_types::Execution) -> Value {
    json!({"timestamp":number(e.timestamp),"symbol":e.symbol,
        "id":e.id.map(Value::String).unwrap_or(json!({"$undefined":true})),
        "orderId":e.order_id.map(Value::String).unwrap_or(json!({"$undefined":true})),
        "price":number(e.price),"qty":number(e.qty),"side":e.side,
        "commission":number(e.commission.unwrap()),"commissionAsset":e.commission_asset,
        "realizedPnl":number(e.realized_pnl.unwrap()),"positionSide":e.position_side,"maker":e.maker})
}
fn contains_nan(v: &Value) -> bool {
    if v == &json!({"$number":"NaN"}) {
        return true;
    }
    match v {
        Value::Array(a) => a.iter().any(contains_nan),
        Value::Object(m) => m.values().any(contains_nan),
        _ => false,
    }
}
fn verify_fills(data: &Value) {
    for case in data["fillSurrogateCases"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(case["escaped"].as_str().unwrap()).is_err(),
            "fill shared UTF-8 input rejection"
        );
        assert!(
            case["resultText"].as_str().unwrap().contains("\\ud800"),
            "TS retains surrogate in callback or output"
        );
    }
    use safe_cex_exchange::runtime::ExchangeError;
    use safe_cex_venue_bitget::fills::{BitgetFillDeps, map_bitget_fill, map_bitget_fills};
    use std::cell::RefCell;
    let mut nan_witnesses = 0;
    for case in data["fillCases"].as_array().unwrap() {
        let name = format!("mapBitgetFill {}", case["name"]);
        if contains_nan(&case["input"]) {
            assert!(
                serde_json::from_str::<Value>("NaN").is_err(),
                "{name} chosen JSON boundary"
            );
            assert!(
                case["result"].get("value").is_some(),
                "{name} TS accepts NaN"
            );
            nan_witnesses += 1;
            continue;
        }
        let row = decode(&case["input"]);
        let mode = case["mode"].as_str().unwrap();
        let calls = RefCell::new(Vec::new());
        let error = |message: &str| ExchangeError {
            name: "TypeError".into(),
            message: message.into(),
            metadata: Box::default(),
        };
        let mut resolve = |symbol: &str| {
            calls.borrow_mut().push(json!(["resolveSymbol", symbol]));
            if mode == "resolveThrows" {
                return Err(error("fixture resolver failure"));
            }
            Ok(match symbol {
                "BTCUSDT" => Some("BTC/USDT".into()),
                "EMPTY" => Some(String::new()),
                _ => None,
            })
        };
        let mut timestamp = |ms| {
            calls.borrow_mut().push(json!(["toTimestamp", number(ms)]));
            if mode == "timestampThrows" {
                return Err(error("fixture timestamp failure"));
            }
            Ok(ms / 1000.0)
        };
        let mut deps = BitgetFillDeps {
            resolve_symbol: &mut resolve,
            to_timestamp: &mut timestamp,
        };
        let result = if mode == "batch" {
            map_bitget_fills(row.as_ref().unwrap().as_array().unwrap(), &mut deps)
                .map(|v| Value::Array(v.into_iter().map(execution).collect()))
        } else {
            map_bitget_fill(row.as_ref(), &mut deps)
                .map(|v| v.map(execution).unwrap_or(Value::Null))
        };
        let actual = match result {
            Ok(v) => json!({"value":v}),
            Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
        };
        compare(actual, &case["result"], &name);
        assert_eq!(
            Value::Array(calls.into_inner()),
            case["calls"],
            "{name} callback order"
        );
    }
    assert_eq!(nan_witnesses, 9, "fill non-JSON NaN witnesses");
}

fn income_record(r: safe_cex_types::IncomeRecord) -> Value {
    json!({"symbol":r.symbol,"incomeType":r.income_type,"asset":r.asset,
        "timestamp":number(r.timestamp),"tranId":r.tran_id,"income":number(r.income),"category":r.category})
}
fn verify_income(data: &Value) {
    use safe_cex_venue_bitget::income::{map_bitget_bill, map_bitget_bills};
    for case in data["billSurrogateCases"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(case["escaped"].as_str().unwrap()).is_err(),
            "bill UTF-8 boundary"
        );
        assert!(
            case["resultText"].as_str().unwrap().contains("\\ud800"),
            "TS bill retains surrogate"
        );
    }
    let boundaries = data["billPrototypeBoundaries"].as_array().unwrap();
    assert_eq!(boundaries.len(), 12, "income prototype own-name inventory");
    for case in boundaries {
        let key = case["key"].as_str().unwrap();
        assert_eq!(
            case["categoryIdentity"], true,
            "income {key} native identity"
        );
        assert_eq!(case["categoryPresent"], true, "income {key} own category");
        assert_eq!(
            case["categoryType"],
            if key == "__proto__" {
                "object"
            } else {
                "function"
            },
            "income {key} pre-serialization typeof"
        );
        let input = decode(&case["input"]);
        let records = map_bitget_bill(input.as_ref()).unwrap();
        assert_eq!(records.len(), 1, "income {key} retains record");
        assert!(
            records[0].category.is_none(),
            "income {key} typed category refusal"
        );
        let mut source: Value = serde_json::from_str(case["json"].as_str().unwrap()).unwrap();
        let actual = serde_json::to_value(&records).unwrap();
        // Compare parsed numeric values, not serde's integer/f64 spelling or key order.
        for field in ["timestamp", "income"] {
            source[0][field] = json!(source[0][field].as_f64().unwrap());
        }
        assert!(
            actual[0].get("category").is_none(),
            "income {key} Rust optional field omission"
        );
        if key == "__proto__" {
            assert_eq!(
                source[0]["category"],
                json!({}),
                "FINDING income prototype object survives serialization"
            );
            assert_ne!(
                actual, source,
                "FINDING income prototype object is refused only in Rust"
            );
            source[0].as_object_mut().unwrap().remove("category");
        } else {
            assert!(
                source[0].get("category").is_none(),
                "income {key} source callable omission"
            );
        }
        assert_eq!(
            actual, source,
            "income {key} member interpolation and retained record"
        );
    }
    let mut nan_witnesses = 0;
    let mut prototype_witnesses = 0;
    for (batch, key) in [(false, "billCases"), (true, "billBatches")] {
        for (i, case) in data[key].as_array().unwrap().iter().enumerate() {
            let name = format!("mapBitgetBill {key} case {i} {}", case["name"]);
            if contains_nan(&case["input"]) {
                assert!(
                    serde_json::from_str::<Value>("NaN").is_err(),
                    "{name} chosen JSON boundary"
                );
                assert!(
                    case["result"].get("value").is_some(),
                    "{name} TS accepts NaN"
                );
                nan_witnesses += 1;
                continue;
            }
            let input = decode(&case["input"]);
            let result = if batch {
                map_bitget_bills(input.as_ref().unwrap().as_array().unwrap())
            } else {
                map_bitget_bill(input.as_ref())
            };
            let actual = match result {
                Ok(records) => {
                    json!({"value":records.into_iter().map(income_record).collect::<Vec<_>>()})
                }
                Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
            };
            let mut expected = case["result"].clone();
            if let Some(records) = expected["value"].as_array_mut() {
                for record in records {
                    if record["category"].is_object() {
                        // ts-bug: mapBitgetBill inherits Object.prototype values.
                        // Pin the source value before serialization and the typed field refusal.
                        let native = record["category"]
                            .get("$function")
                            .and_then(Value::as_str)
                            .unwrap_or("[object Object]");
                        assert_eq!(
                            record["tranId"],
                            format!("bill-1:{native}"),
                            "{name} inherited source tranId"
                        );
                        assert!(
                            native.contains("[native code]") || record["category"] == json!({}),
                            "{name} inherited category"
                        );
                        record["category"] = Value::Null;
                        // tranId stays the source member-specific string, without projection.
                        prototype_witnesses += 1;
                    }
                }
            }
            compare(actual, &expected, &name);
        }
    }
    assert_eq!(nan_witnesses, 7, "bill non-JSON NaN witnesses");
    assert_eq!(
        prototype_witnesses, 48,
        "all twelve inherited categories across four nonzero inputs"
    );
}

fn verify(data: Value) {
    verify_income(&data);
    verify_fills(&data);
    for case in data["surrogateCases"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(case["escaped"].as_str().unwrap()).is_err(),
            "shared UTF-8 rejection witness"
        );
        assert!(
            case["result"].get("value").is_some(),
            "TS accepts lone surrogates"
        );
    }
    let mut nan_witnesses = 0;
    for (i, case) in data["numbers"].as_array().unwrap().iter().enumerate() {
        if case["input"] == json!({"$number":"NaN"}) {
            // NaN is not a JSON value. Pin the limitation rather than sanitize it.
            assert!(serde_json::from_str::<Value>("NaN").is_err());
            assert_eq!(case["fills"]["value"], number(0.0));
            nan_witnesses += 1;
            continue;
        }
        let input = decode(&case["input"]);
        compare(
            result(safe_cex_venue_bitget::income::num(input.as_ref()), number),
            &case["income"],
            &format!("income num case {i}"),
        );
        compare(
            result(num(input.as_ref()), number),
            &case["fills"],
            &format!("num case {i}"),
        );
    }
    assert_eq!(nan_witnesses, 1, "explicit non-JSON NaN witness");
    for (i, case) in data["feeCases"].as_array().unwrap().iter().enumerate() {
        let input = decode(&case["input"]);
        compare(
            result(total_fee_of(input.as_ref()), number),
            &case["result"],
            &format!("totalFeeOf case {i}"),
        );
    }
    for (i, case) in data["legCases"].as_array().unwrap().iter().enumerate() {
        let input = decode(&case["input"]);
        let actual = result(leg_and_direction_of(input.as_ref()), |leg| {
            leg.map(|l| json!({"positionSide":l.position_side,"direction":l.direction}))
                .unwrap_or(Value::Null)
        });
        compare(
            actual,
            &case["result"],
            &format!("legAndDirectionOf case {i}"),
        );
    }
}
#[test]
fn frozen_fill_helpers() {
    verify(serde_json::from_str(include_str!("history-reference.json")).unwrap());
}
#[test]
#[ignore = "requires Node and TypeScript"]
fn live_fill_helpers() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/history-capture.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    verify(serde_json::from_slice(&output.stdout).unwrap());
}

#[test]
#[ignore = "requires Node and TypeScript"]
fn live_fill_oracle_rejects_source_correction() {
    for (mutation, name) in [
        ("commission-abs", "mapBitgetFill"),
        ("income-fee", "mapBitgetBill"),
        ("income-own-only", "income constructor native identity"),
    ] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["live_fill_helpers", "--exact", "--ignored", "--nocapture"])
            .env("BITGET_HISTORY_MUTATION", mutation)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(101),
            "source correction must fail Rust comparison: {mutation}"
        );
        let diagnostics = String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr);
        assert!(
            diagnostics.contains(name),
            "child must fail the named mapper assertion: {diagnostics}"
        );
    }
}
