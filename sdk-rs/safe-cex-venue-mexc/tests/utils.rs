mod support;
use safe_cex_num::JsCoercionError;
use safe_cex_types::{MarginType, OrderSide, PositionMode};
use safe_cex_venue_mexc::utils::*;
use serde_json::{Value, json};
use support::{decode_number, encode};

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
fn run(case: &Value) -> Result<Value, JsCoercionError> {
    let raw_args = case["args"].as_array().unwrap();
    let args: Vec<_> = raw_args.iter().map(decoded).collect();
    let arg = |i: usize| {
        if raw_args.get(i).is_none_or(undefined) {
            None
        } else {
            args.get(i)
        }
    };
    let raw = case.get("marketRaw").filter(|v| !undefined(v));
    let market = raw
        .map(|v| {
            normalize_market(&serde_json::Value::Object((v.as_object().unwrap()).clone()).into())
        })
        .transpose()?;
    let name = case["name"].as_str().unwrap();
    Ok(match name {
        "normalizeTimestampToMs" => encode(&normalize_timestamp_to_ms(arg(0), &|ms| ms)?),
        "normalizeTimestampToSeconds" => encode(&normalize_timestamp_to_seconds(arg(0), &|ms| ms)?),
        "normalizeTicker" => encode(&normalize_ticker(
            arg(0).unwrap().as_object().unwrap(),
            market.as_ref().unwrap(),
            &|ms| ms,
        )?),
        "normalizeCandle" => encode(&normalize_candle(arg(0).unwrap(), &|ms| ms)?),
        "normalizeKlineResponse" => encode(&normalize_kline_response(arg(0).unwrap(), &|ms| ms)?),
        "toNumber" => encode(&to_number(arg(0), None)?),
        "toIntegerString" => encode(&to_integer_string(arg(0))?),
        "fromMexcSymbol" => encode(&from_mexc_symbol(arg(0))?),
        "toMexcSymbol" => encode(&to_mexc_symbol(arg(0).unwrap().as_str().unwrap())),
        "normalizePositionSide" => encode(&normalize_position_side(arg(0))?),
        "normalizePositionMode" => encode(&normalize_position_mode(arg(0))?),
        "normalizeMarginType" => encode(&normalize_margin_type(arg(0))?),
        "normalizePercentage" => encode(&normalize_percentage(arg(0))?),
        "stepFromScale" => encode(&step_from_scale(
            arg(0).cloned().map(Into::into).as_ref(),
            arg(1).unwrap().as_f64().unwrap(),
        )?),
        "normalizeMarket" => encode(&normalize_market(
            &serde_json::Value::Object((arg(0).unwrap().as_object().unwrap()).clone()).into(),
        )?),
        "isTradableMexcMarket" => encode(&is_tradable_mexc_market(
            &serde_json::Value::Object((arg(0).unwrap().as_object().unwrap()).clone()).into(),
        )?),
        "getContractSize" => encode(&get_contract_size(market.as_ref())),
        "nativeVolumeToAmount" => encode(&native_volume_to_amount(arg(0), market.as_ref())?),
        "amountToNativeVolume" => encode(&amount_to_native_volume(
            decode_number(&raw_args[0]),
            market.as_ref(),
        )),
        "normalizeOrderBook" => encode(&normalize_order_book(
            arg(0).unwrap().as_object().unwrap(),
            market.as_ref().unwrap(),
        )?),
        "normalizeBalance" => encode(&normalize_balance(arg(0).unwrap())?),
        "normalizeTriggerOrderStatus" => encode(&normalize_trigger_order_status(
            arg(0).unwrap().as_object().unwrap(),
        )?),
        "normalizePlanOrderType" => encode(&normalize_plan_order_type(
            arg(0).unwrap().as_object().unwrap(),
            if arg(1).unwrap() == "sell" {
                OrderSide::Sell
            } else {
                OrderSide::Buy
            },
        )?),
        "normalizeStopOrderType" => encode(&normalize_stop_order_type(
            arg(0).unwrap().as_object().unwrap(),
        )?),
        "fakePositionsForMarket" => {
            let mode = arg(0).map(|v| {
                if v == "oneway" {
                    PositionMode::OneWay
                } else {
                    PositionMode::Hedged
                }
            });
            let margin = arg(1).map(|v| {
                if v == "isolated" {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                }
            });
            let leverage = raw_args.get(2).filter(|v| !undefined(v)).map(|v| {
                if v.get("$number").is_some() {
                    MexcLeverage::Number(decode_number(v))
                } else {
                    MexcLeverage::Rails {
                        long: v.get("long").map(decode_number),
                        short: v.get("short").map(decode_number),
                    }
                }
            });
            encode(&fake_positions_for_market(
                market.as_ref().unwrap(),
                mode,
                margin,
                leverage.as_ref(),
            ))
        }
        _ => panic!("unhandled function {name}"),
    })
}
#[test]
fn mexc_utils_live_typescript_every_value() {
    let output = std::process::Command::new("node")
        .env("TZ", "UTC")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/utils_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 595, "live utility corpus census");
    let mut precision_identity_cases = 0;
    for (index, case) in cases.iter().enumerate() {
        if case["name"] == "normalizeMarket" && case.get("error").is_none() {
            let raw = decoded(&case["args"][0]);
            let market = normalize_market(
                &serde_json::Value::Object((raw.as_object().unwrap()).clone()).into(),
            )
            .unwrap();
            let expected = &case["output"]["precision"];
            let expected_amount = decode_number(&expected["amount"]);
            let expected_price = decode_number(&expected["price"]);
            assert_eq!(
                market.precision.amount.to_bits(),
                expected_amount.to_bits(),
                "CHASER-PRECISION native amount step case {index}"
            );
            assert_eq!(
                market.precision.price.to_bits(),
                expected_price.to_bits(),
                "CHASER-PRECISION native price step case {index}"
            );
            precision_identity_cases += 1;
        }
        match run(case) {
            Ok(actual) => assert_eq!(
                actual, case["output"],
                "case {index} {} args {}",
                case["name"], case["args"]
            ),
            Err(error) => assert_eq!(
                json!({"name":error.name,"message":error.message}),
                case["error"],
                "case {index} {} args {}",
                case["name"],
                case["args"]
            ),
        }
    }
    assert!(
        precision_identity_cases >= 4,
        "native market precision identity cases: {precision_identity_cases}"
    );
    println!(
        "{} pure utility cases, {} native market precision identity cases",
        cases.len(),
        precision_identity_cases
    );
}
#[test]
fn numeric_encoder_preserves_nonfinite_and_negative_zero() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0, 0.0, 1e308] {
        let actual = encode(&vec![value]);
        assert_eq!(actual[0], support::numeric(value));
    }
    assert_ne!(encode(&f64::INFINITY), encode(&f64::NAN));
    assert_ne!(encode(&0.0), encode(&-0.0));
}

#[test]
fn mexc_utils_typescript_negative_control_is_observed() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_utils_live_typescript_every_value",
            "--nocapture",
        ])
        .env("MEXC_TS_CONTROL", "contract-size")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        text.contains("mexc_utils_live_typescript_every_value ... FAILED"),
        "{text}"
    );
    assert!(
        text.contains("getContractSize") || text.contains("normalizeMarket"),
        "{text}"
    );
    assert!(text.contains("assertion `left == right` failed"), "{text}");
}

// The former shared ordering FINDING flipped after JsObjectMap landed.
// Keep positive live-source parity for this observable key order.
#[test]
fn mexc_balance_shared_currency_order_parity() {
    let output = std::process::Command::new("node")
        .env("TZ", "UTC")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/utils_oracle.mjs"
        ))
        .arg("--currency-order")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ts_keys: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    let balance = normalize_balance(
        &json!([{ "currency":"USDT", "free":1 }, { "currency":"USDC", "free":2 }]),
    )
    .unwrap();
    let rust_keys: Vec<_> = balance.currencies.unwrap().keys().cloned().collect();
    assert_eq!(ts_keys, ["USDT", "USDC"]);
    // The committed ordering FINDING flipped after the shared JsObjectMap repair.
    assert_eq!(
        rust_keys, ts_keys,
        "shared Balance currency order matches live source"
    );
}

#[test]
fn mexc_timestamp_host_timezone_adapter() {
    for zone in ["UTC", "Asia/Singapore", "America/New_York"] {
        for source in [
            "2024-01-02T03:04:05",
            "2024-03-10T02:30:00",
            "2024-11-03T01:30:00",
        ] {
            let output = std::process::Command::new("node")
                .args(["-e", "const s=process.argv[1];process.stdout.write(JSON.stringify([Date.parse(s),Date.parse(s+'Z')]));", source])
                .env("TZ", zone).output().unwrap();
            assert!(output.status.success());
            let expected: Vec<f64> = serde_json::from_slice(&output.stdout).unwrap();
            let called = std::cell::Cell::new(false);
            let adapter = |wall: f64| {
                assert_eq!(wall, expected[1], "wall clock input {zone} {source}");
                called.set(true);
                expected[0]
            };
            assert_eq!(
                normalize_timestamp_to_ms(Some(&json!(source)), &adapter).unwrap(),
                expected[0]
            );
            assert!(called.get());
        }
    }
    for source in [
        "2024-01-02",
        "2024-01-02T03:04:05Z",
        "2024-01-02T03:04:05+08:00",
    ] {
        normalize_timestamp_to_ms(Some(&json!(source)), &|_| {
            panic!("explicit UTC bypasses host adapter")
        })
        .unwrap();
    }
}

#[test]
fn mexc_timestamp_typescript_negative_control_is_observed() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_utils_live_typescript_every_value",
            "--nocapture",
        ])
        .env("MEXC_TS_CONTROL", "timestamp-zero")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        text.contains("mexc_utils_live_typescript_every_value ... FAILED"),
        "{text}"
    );
    assert!(text.contains("normalizeTimestampToMs"), "{text}");
    assert!(text.contains("assertion `left == right` failed"), "{text}");
}
