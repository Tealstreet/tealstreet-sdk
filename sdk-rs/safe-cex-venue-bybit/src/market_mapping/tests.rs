use super::*;
use serde_json::Value;
fn binary64(value: &Value) -> Value {
    match value {
        Value::Number(n) => serde_json::json!(n.as_f64().unwrap()),
        Value::Array(items) => Value::Array(items.iter().map(binary64).collect()),
        Value::Object(items) => Value::Object(
            items
                .iter()
                .map(|(k, v)| (k.clone(), binary64(v)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn market_mapping_matches_each_live_family_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/market-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let mut regular = 0;
    let mut asset_refusals = 0;
    let mut symbol_refusals = 0;
    let mut projection_errors = 0;
    for (index, case) in cases.iter().enumerate() {
        let family = match case["variant"].as_str().unwrap() {
            "bybit" => Family::Linear,
            "bybitinverse" => Family::Inverse,
            "bybitspot" => Family::Spot,
            _ => unreachable!(),
        };
        let result = map_market(
            &case["raw"].clone().into(),
            &case["fees"]
                .as_array()
                .unwrap()
                .iter()
                .cloned()
                .map(Into::into)
                .collect::<Vec<_>>(),
            case["category"].as_str().unwrap(),
            family,
        );
        if case["symbolRefusal"] == true {
            assert!(case.get("error").is_none(), "source retains raw symbol");
            assert_eq!(case["ownSymbols"], serde_json::json!([true, true]));
            assert_eq!(case["symbolIdentity"], serde_json::json!([true, true]));
            let refusal = result
                .expect("source evaluation completes")
                .expect_err("strict symbol refusal");
            assert!(refusal.contains("symbol="));
            symbol_refusals += 1;
            continue;
        }
        if case["projectionError"] == true {
            assert_eq!(result.unwrap_err(), case["error"].as_str().unwrap());
            projection_errors += 1;
            continue;
        }
        if case["assetRefusal"] == true {
            assert!(
                case.get("error").is_none(),
                "source keeps the malformed asset row: {case}"
            );
            assert_eq!(case["ownAssets"], serde_json::json!([true, true]));
            assert_eq!(case["assetIdentity"], serde_json::json!([true, true]));
            let refusal = result
                .expect("source evaluation completes")
                .expect_err("strict asset refusal");
            assert!(refusal.contains("Refused Bybit market row"));
            asset_refusals += 1;
            continue;
        }
        regular += 1;
        if let Some(error) = case.get("error") {
            assert_eq!(
                result.unwrap_err(),
                error.as_str().unwrap(),
                "case {index}: {case}"
            );
            continue;
        }
        let market = result
            .unwrap_or_else(|error| panic!("case {index}: {error}: {case}"))
            .unwrap_or_else(|error| panic!("case {index}: {error}: {case}"));
        assert_eq!(
            binary64(&serde_json::to_value(&market).unwrap()),
            binary64(&case["result"]),
            "case {index}: {case}"
        );
        let mut values = vec![
            market.precision.amount,
            market.precision.price,
            market.limits.amount.min,
            safe_cex_num::numeric_value_to_number(&market.limits.amount.max),
            market.limits.leverage.min,
            market.limits.leverage.max,
        ];
        if let Some(taker) = market.taker_fee {
            values.extend([taker, market.maker_fee.unwrap()]);
        }
        let markers: Vec<_> = values
            .into_iter()
            .map(|value| {
                if value.is_nan() {
                    "NaN"
                } else if value == f64::INFINITY {
                    "Infinity"
                } else if value == f64::NEG_INFINITY {
                    "-Infinity"
                } else if value == 0. && value.is_sign_negative() {
                    "-0"
                } else {
                    "finite"
                }
            })
            .collect();
        assert_eq!(
            serde_json::json!(markers),
            case["markers"],
            "number classes case {index}"
        );
    }
    assert_eq!(regular, 549);
    assert_eq!(asset_refusals, 30);
    assert_eq!(symbol_refusals, 21);
    assert_eq!(projection_errors, 6);
    assert_eq!(cases.len(), 606);
}
