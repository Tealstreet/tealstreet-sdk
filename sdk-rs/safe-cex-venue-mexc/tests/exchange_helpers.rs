mod support;
use safe_cex_num::JsJsonValue;
use safe_cex_types::{MarginType, Position, PositionSide};
use safe_cex_venue_mexc::{
    exchange_helpers::*, symbol_config::format_change_leverage_payload, utils::*,
};
use serde_json::{Value, json};
use support::{decode_number, encode, numeric};
fn undefined(v: &Value) -> bool {
    v.get("$undefined") == Some(&Value::Bool(true))
}
fn decoded(v: &Value) -> Value {
    if v.get("$number").is_some() {
        return json!(decode_number(v));
    }
    match v {
        Value::Array(a) => Value::Array(a.iter().map(decoded).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .filter(|(_, v)| !undefined(v))
                .map(|(k, v)| (k.clone(), decoded(v)))
                .collect(),
        ),
        _ => v.clone(),
    }
}

fn encode_js_json(value: &JsJsonValue) -> Value {
    match value {
        JsJsonValue::Undefined => json!({"$undefined":true}),
        JsJsonValue::Null => Value::Null,
        JsJsonValue::Bool(value) => json!(value),
        JsJsonValue::Number(value) => numeric(*value),
        JsJsonValue::String(value) => json!(value),
        JsJsonValue::Array(values) => Value::Array(values.iter().map(encode_js_json).collect()),
        JsJsonValue::Object(entries) => Value::Object(
            entries
                .iter()
                .filter(|(_, value)| !matches!(value, JsJsonValue::Undefined))
                .map(|(key, value)| (key.clone(), encode_js_json(value)))
                .collect(),
        ),
        JsJsonValue::BigInt(value) => json!({"$bigint":value.to_string()}),
    }
}

fn positions(rows: &Value) -> Vec<Position> {
    let market = normalize_market(
        &serde_json::Value::Object(
            (json!({"symbol":"BTC_USDT","contractSize":1})
                .as_object()
                .unwrap())
            .clone(),
        )
        .into(),
    )
    .unwrap();
    let base = fake_positions_for_market(&market, None, None, None).remove(0);
    rows.as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let mut position = base.clone();
            position.id = row["id"].as_str().unwrap().into();
            position.symbol = row["symbol"].as_str().unwrap().into();
            position.side = serde_json::from_value(row["side"].clone()).unwrap();
            position.contracts = decode_number(&row["contracts"]);
            position.info = (row.get("info").map(decoded).map(Into::into)).into();
            position
        })
        .collect()
}

#[test]
fn mexc_exchange_helpers_live_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/exchange_helpers_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 552);
    for (i, case) in cases.iter().enumerate() {
        let args = case["args"].as_array().unwrap();
        let raw = args.first().filter(|v| !undefined(v)).map(decoded);
        let name = case["name"].as_str().unwrap();
        let actual: Result<Value, safe_cex_num::JsCoercionError> = (|| {
            Ok(match name {
                "findOpenPosition" => {
                    let side: PositionSide = serde_json::from_value(args[1].clone()).unwrap();
                    find_open_position(args[0].as_str().unwrap(), side, &positions(&args[2]))
                        .map(|position| encode(&position.id))
                        .unwrap_or(json!({"$undefined":true}))
                }
                "formatChangeLeveragePayload" => {
                    let market_raw = decoded(&case["marketRaw"]);
                    let market = normalize_market(
                        &serde_json::Value::Object((market_raw.as_object().unwrap()).clone())
                            .into(),
                    )?;
                    let positions = positions(&args[6]);
                    let margin_type: MarginType = serde_json::from_value(args[4].clone()).unwrap();
                    encode_js_json(&format_change_leverage_payload(
                        args[0].as_str().unwrap(),
                        &market,
                        decode_number(&args[2]) as u8,
                        decode_number(&args[3]),
                        margin_type,
                        args[5]["includeOpenTypeWithPositionId"].as_bool().unwrap(),
                        &positions,
                    )?)
                }
                "extractData" => extract_data(raw.as_ref())
                    .map(encode)
                    .unwrap_or(json!({"$undefined":true})),
                "extractRows" => encode(&extract_rows(
                    raw.as_ref(),
                    &serde_json::from_value::<Vec<String>>(args[1].clone()).unwrap(),
                )),
                "orderListEndpoint" => {
                    encode(&order_list_endpoint(raw.as_ref().and_then(Value::as_str)))
                }
                "nativePositionId" => {
                    let mut p = normalize_position(
                        json!({"symbol":"BTC_USDT"}).as_object().unwrap(),
                        None,
                        None,
                        &|ms| ms,
                    )?
                    .unwrap();
                    p.info = (raw.as_ref().unwrap().get("info").cloned().map(Into::into)).into();
                    encode(&native_position_id(&p)?)
                }
                "planOrderTriggerType" | "stopOrderTrend" | "isRegularPlaceOrder" => {
                    let mut input =
                        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1});
                    input
                        .as_object_mut()
                        .unwrap()
                        .extend(raw.as_ref().unwrap().as_object().unwrap().clone());
                    let opts: safe_cex_types::PlaceOrderOpts =
                        serde_json::from_value(input).unwrap();
                    match name {
                        "planOrderTriggerType" => encode(&plan_order_trigger_type(&opts)),
                        "stopOrderTrend" => encode(&stop_order_trend(&opts)),
                        _ => encode(&is_regular_place_order(&opts)),
                    }
                }
                _ => {
                    let mut order = normalize_order(
                        json!({"id":"1","symbol":"BTC_USDT","side":1})
                            .as_object()
                            .unwrap(),
                        None,
                        &|ms| ms,
                    )?
                    .unwrap();
                    let value = raw.as_ref().unwrap();
                    if let Some(t) = value.get("type") {
                        order.r#type = serde_json::from_value(t.clone()).unwrap();
                    }
                    for (key, target) in [
                        ("amount", &mut order.amount),
                        ("remaining", &mut order.remaining),
                    ] {
                        if let Some(number) = args[0].get(key) {
                            *target = decode_number(number);
                        }
                    }
                    order.info = (value.get("info").cloned().map(Into::into)).into();
                    if let Some(number) = args[0]
                        .get("extra")
                        .and_then(|v| v.get("remainingNative"))
                        .filter(|v| !undefined(v))
                    {
                        order.extra.as_mut().unwrap().remaining_native =
                            Some(decode_number(number));
                    } else {
                        order.extra = (None).into();
                    }
                    match name {
                        "isStopLikeOrder" => encode(&is_stop_like_order(&order)),
                        "isStopOrder" => encode(&is_stop_order(&order)),
                        "isPlanOrder" => encode(&is_plan_order(&order)),
                        "getReplacementOrderAmount" => {
                            encode(&get_replacement_order_amount(&order)?)
                        }
                        _ => panic!("unknown {name}"),
                    }
                }
            })
        })();
        match actual {
            Ok(value) => assert_eq!(value, case["output"], "helper {i}: {case}"),
            Err(e) => assert_eq!(
                json!({"name":e.name,"message":e.message}),
                case["error"],
                "helper {i}: {case}"
            ),
        }
    }
}

#[test]
fn mexc_exchange_helpers_typescript_control_is_observed() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_exchange_helpers_live_typescript",
            "--nocapture",
        ])
        .env("MEXC_TS_CONTROL", "plan-classification")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("mexc_exchange_helpers_live_typescript ... FAILED")
            && text.contains("isPlanOrder")
            && text.contains("assertion `left == right` failed"),
        "{text}"
    );
}

#[test]
fn mexc_find_open_position_typescript_control_is_observed() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_exchange_helpers_live_typescript",
            "--nocapture",
        ])
        .env("MEXC_TS_CONTROL", "positive-open-position")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("mexc_exchange_helpers_live_typescript ... FAILED")
            && text.contains("findOpenPosition")
            && text.contains("assertion `left == right` failed"),
        "{text}"
    );
}

#[test]
fn mexc_format_change_leverage_payload_typescript_control_is_observed() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_exchange_helpers_live_typescript",
            "--nocapture",
        ])
        .env("MEXC_TS_CONTROL", "leverage-payload-open-type")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("mexc_exchange_helpers_live_typescript ... FAILED")
            && text.contains("formatChangeLeveragePayload")
            && text.contains("assertion `left == right` failed"),
        "{text}"
    );
}
