use crate::binding_test_harness::{harness, options};
use safe_cex_exchange::js_runtime::{promise::await_js, value::Js};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, rc::Rc};

fn observe(value: &Js) -> Value {
    if let Some(number) = value.as_f64() {
        return json!({"$bits":format!("{:016x}", number.to_bits())});
    }
    if value.is_array() {
        return Value::Array(
            value
                .array_values()
                .unwrap()
                .expect("native array")
                .iter()
                .map(observe)
                .collect(),
        );
    }
    if value.is_object() {
        return Value::Object(
            value
                .own_keys()
                .into_iter()
                .map(|key| {
                    let name = String::from_utf16(&key).unwrap();
                    (name.clone(), observe(&value.get(&name).unwrap()))
                })
                .collect(),
        );
    }
    if value.is_undefined() {
        return json!({"$undefined":true});
    }
    safe_cex_exchange::js_runtime::value::to_json(value).unwrap()
}

#[test]
fn history_null_and_array_rows_do_not_discard_valid_fills() {
    assert_rows(false);
}

#[test]
fn history_primitive_rows_keep_the_source_lookup_diagnostic() {
    assert_rows(true);
}

fn assert_rows(primitives: bool) {
    let inputs = json!([null, [], [1, 2], {}, 7, false, "oops"]);
    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/history-row-source.cjs"
        ))
        .arg(inputs.to_string())
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(cases.len(), 14);
    for case in cases {
        let extra = &case["extra"];
        if (!extra.is_null() && !extra.is_array() && !extra.is_object()) != primitives {
            continue;
        }
        let path = "/v3/trade/transactionHistory";
        let h = harness(ExchangeName::Woo, options(), |http| {
            http.ok("GET", path, json!({"rows":[extra, {"id":"fill", "orderId":"order", "symbol":"PERP_BTC_USDT", "side":"BUY", "executedPrice":17.125, "executedQuantity":3.75, "executedTimestamp":1700000000123_i64, "fee":-0.0, "realizedPnl":0, "isMaker":0}]}));
        });
        let logs = Rc::new(RefCell::new(Vec::<String>::new()));
        let record = logs.clone();
        h.venue().base().on(
            "error",
            Rc::new(move |_, value| {
                record
                    .borrow_mut()
                    .push(String::from_utf16(value.as_str().unwrap()).unwrap());
                Ok(())
            }),
        );
        let value = h
            .exchange
            .rt
            .this
            .call_method(
                case["method"].as_str().unwrap(),
                vec![Js::object(vec![("symbol", Js::str("BTCUSDT"))])],
            )
            .unwrap();
        let runtime = h.exchange.rt.exchange_runtime();
        let actual = h
            .drive(Box::pin(async move {
                await_js(&runtime, value)
                    .await
                    .map_err(|error| error.to_exchange_error())
            }))
            .unwrap();
        assert_eq!(observe(&actual), case["rows"], "source output {case}");
        let expected: Vec<String> = case["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|message| format!("Exchange Error: {}", message.as_str().unwrap()))
            .collect();
        assert_eq!(&*logs.borrow(), &expected, "source lookup failure {case}");
        let calls = h.http.calls(path);
        assert_eq!(calls.len(), 1, "one signed request {case}");
        assert!(
            !calls[0].headers["x-api-signature"]
                .as_str()
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn history_ignores_unread_utf16_and_preserves_selected_number_coercion() {
    use safe_cex_exchange::js_runtime::value::from_retained_input;
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/history-row-source.cjs"
        ))
        .arg("--retained")
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(cases.len(), 64);
    for case in cases {
        let path = "/v3/trade/transactionHistory";
        let raw = case["raw"].as_str().unwrap();
        let legacy = case["legacy"].as_bool().unwrap();
        let mut data = Input::parse_json(&raw.encode_utf16().collect::<Vec<_>>()).unwrap();
        if case["ownUndefined"] == true {
            let row_path = match case["shape"].as_str().unwrap() {
                "nestedRows" => data.get_mut("data").unwrap().get_mut("rows").unwrap(),
                "rows" => data.get_mut("rows").unwrap(),
                "nestedArray" => data.get_mut("data").unwrap(),
                _ => &mut data,
            };
            row_path.as_array_mut().unwrap()[1]
                .as_object_mut()
                .unwrap()
                .push(("executedPrice".encode_utf16().collect(), Input::Undefined));
        }
        let h = harness(ExchangeName::Woo, options(), |http| {
            if legacy {
                http.ok("GET", path, json!({"rows":[]}));
            }
            http.route_native(
                "GET",
                if legacy { "/v1/client/trades" } else { path },
                200.0,
                from_retained_input(data),
            );
        });
        let errors = Rc::new(RefCell::new(Vec::new()));
        let sink = errors.clone();
        h.venue().base().on(
            "error",
            Rc::new(move |_, value| {
                sink.borrow_mut().push(value.to_display());
                Ok(())
            }),
        );
        let value = h
            .exchange
            .rt
            .this
            .call_method(
                case["method"].as_str().unwrap(),
                vec![Js::object(vec![("symbol", Js::str("BTCUSDT"))])],
            )
            .unwrap();
        let runtime = h.exchange.rt.exchange_runtime();
        let actual = h
            .drive(Box::pin(async move {
                await_js(&runtime, value)
                    .await
                    .map_err(|e| e.to_exchange_error())
            }))
            .unwrap();
        assert_eq!(
            observe(&actual),
            case["rows"],
            "retained signed history {case}"
        );
        assert!(
            errors.borrow().is_empty(),
            "unread metadata must not report: {:?}",
            errors.borrow()
        );
        let calls = h
            .http
            .calls(if legacy { "/v1/client/trades" } else { path });
        assert_eq!(calls.len(), 1, "no metadata-induced fallback");
        if legacy {
            assert_eq!(h.http.calls(path).len(), 5);
        }
        assert!(
            !calls[0].headers["x-api-signature"]
                .as_str()
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn history_options_match_source_coercion() {
    assert_history_boundary("options");
}

#[test]
fn history_selected_asset_refuses_only_its_row() {
    assert_history_boundary("asset");
}

#[test]
fn history_typed_strings_do_not_discard_siblings() {
    assert_history_boundary("text");
}

fn assert_history_boundary(group: &str) {
    use safe_cex_exchange::js_runtime::value::from_retained_input;
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    fn unpack(value: &Value) -> Js {
        if let Some(units) = value.get("$units").and_then(Value::as_array) {
            return Js::utf16(
                &units
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u16)
                    .collect::<Vec<_>>(),
            );
        }
        if value.get("$undefined") == Some(&Value::Bool(true)) {
            return Js::Undefined;
        }
        if let Some(bits) = value.get("$bits").and_then(Value::as_str) {
            return Js::Number(f64::from_bits(u64::from_str_radix(bits, 16).unwrap()));
        }
        match value {
            Value::Array(items) => Js::array(items.iter().map(unpack).collect()),
            Value::Object(items) => {
                Js::object(items.iter().map(|(k, v)| (k.as_str(), unpack(v))).collect())
            }
            _ => safe_cex_exchange::js_runtime::value::from_json(value),
        }
    }
    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/history-row-source.cjs"
        ))
        .arg("--boundary")
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(cases.len(), 28);
    for case in cases {
        let profile = case["profile"].as_str().unwrap();
        let selected = match profile {
            "bad-asset" => "asset",
            "utf16-text" | "unknown-side" | "unknown-position" => "text",
            _ => "options",
        };
        if selected != group {
            continue;
        }
        let raw = case["raw"].as_str().unwrap();
        let data = Input::parse_json(&raw.encode_utf16().collect::<Vec<_>>()).unwrap();
        let path = "/v3/trade/transactionHistory";
        let h = harness(ExchangeName::Woo, options(), |http| {
            http.route_native("GET", path, 200.0, from_retained_input(data));
        });
        let errors = Rc::new(RefCell::new(Vec::new()));
        let sink = errors.clone();
        h.venue().base().on(
            "error",
            Rc::new(move |_, value| {
                sink.borrow_mut().push(value.to_display());
                Ok(())
            }),
        );
        let value = h
            .exchange
            .rt
            .this
            .call_method(
                case["method"].as_str().unwrap(),
                vec![unpack(&case["opts"])],
            )
            .unwrap();
        let runtime = h.exchange.rt.exchange_runtime();
        let actual = h.drive(Box::pin(async move {
            await_js(&runtime, value)
                .await
                .map_err(|e| e.to_exchange_error())
        }));
        let actual = actual.unwrap_or_else(|error| {
            panic!(
                "history boundary {} {}: {error:?}",
                case["method"], case["profile"]
            )
        });
        fn typed_text(value: Value) -> Value {
            if let Some(units) = value.get("$units").and_then(Value::as_array) {
                return Value::String(String::from_utf16_lossy(
                    &units
                        .iter()
                        .map(|v| v.as_u64().unwrap() as u16)
                        .collect::<Vec<_>>(),
                ));
            }
            match value {
                Value::Array(items) => Value::Array(items.into_iter().map(typed_text).collect()),
                Value::Object(items) => {
                    Value::Object(items.into_iter().map(|(k, v)| (k, typed_text(v))).collect())
                }
                other => other,
            }
        }
        let mut expected = typed_text(case["rows"].clone());
        if case["profile"] == "bad-asset" {
            expected
                .as_array_mut()
                .unwrap()
                .retain(|row| row["id"] != "middle");
            assert_eq!(errors.borrow().len(), 1, "exactly one row refusal");
            let message = &errors.borrow()[0];
            assert!(
                message.contains("commissionAsset")
                    && message.contains("middle")
                    && message.contains("invalid"),
                "{message}"
            );
        } else {
            assert!(errors.borrow().is_empty(), "{:?}", errors.borrow());
        }
        assert_eq!(
            observe(&actual),
            expected,
            "history output {} {}",
            case["method"],
            case["profile"]
        );
        let calls = h.http.calls(path);
        assert_eq!(calls.len(), 1, "one signed history read");
        assert!(
            !calls[0].headers["x-api-signature"]
                .as_str()
                .unwrap()
                .is_empty()
        );
        h.venue().dispose().unwrap();
    }
}
