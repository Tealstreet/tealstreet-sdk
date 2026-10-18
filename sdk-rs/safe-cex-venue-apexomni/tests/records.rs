use safe_cex_venue_apexomni::config_records::*;
use safe_cex_venue_apexomni::order_records::*;
use safe_cex_venue_apexomni::private_records::*;
use safe_cex_venue_apexomni::public_records::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::process::Command;

fn oracle() -> &'static Value {
    static CORPUS: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    CORPUS.get_or_init(capture_oracle)
}
fn capture_oracle() -> Value {
    let result = Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/records.cjs"))
        .env("SAFE_CEX_RUST_ARCH", std::env::consts::ARCH)
        .output()
        .expect("live TypeScript record oracle must run");
    assert!(
        result.status.success(),
        "record oracle failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let corpus: Value = serde_json::from_slice(&result.stdout).expect("record oracle JSON");
    numeric_corpus_cardinality(&corpus);
    corpus
}
fn sample(schema: &Value, seed: usize) -> Value {
    match schema["kind"].as_str().unwrap() {
        "null" | "undefined" => Value::Null,
        "literal" => schema["value"].clone(),
        "union" => sample(
            schema["choices"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["kind"] != "null" && c["kind"] != "undefined")
                .unwrap(),
            seed,
        ),
        "any" => json!({"opaque": format!("opaque-{seed}"), "number": seed as f64 + 0.5}),
        "string" => json!(format!("field-{seed}-é-😀-\u{feff}")),
        "number" => json!(seed as f64 + 0.25),
        "boolean" => json!(true),
        "array" => json!([
            sample(&schema["item"], seed),
            sample(&schema["item"], seed + 100)
        ]),
        "record" => Value::Object(
            schema["fields"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .filter(|(_, field)| field["type"]["kind"] != "undefined")
                .map(|(i, field)| {
                    (
                        field["name"].as_str().unwrap().to_owned(),
                        sample(&field["type"], seed + i),
                    )
                })
                .collect(),
        ),
        kind => panic!("unclassified record schema: {kind}"),
    }
}
fn schema_accepts(schema: &Value, value: &Value) -> bool {
    match schema["kind"].as_str().unwrap() {
        "any" => true,
        "undefined" => false,
        "null" => value.is_null(),
        "literal" => value == &schema["value"],
        "union" => schema["choices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| schema_accepts(c, value)),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "array" => value
            .as_array()
            .is_some_and(|xs| xs.iter().all(|x| schema_accepts(&schema["item"], x))),
        "record" => value.as_object().is_some_and(|object| {
            schema["fields"].as_array().unwrap().iter().all(|f| {
                let key = f["name"].as_str().unwrap();
                object
                    .get(key)
                    .map_or(f["optional"] == true, |v| schema_accepts(&f["type"], v))
            })
        }),
        kind => panic!("unclassified acceptance schema: {kind}"),
    }
}
fn union_samples(schema: &Value, seed: usize) -> Vec<Value> {
    if schema["kind"] == "union" {
        return schema["choices"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|c| union_samples(c, seed))
            .collect();
    }
    if schema["kind"] == "undefined" {
        return vec![];
    }
    vec![sample(schema, seed)]
}
fn array_samples(schema: &Value) -> Vec<Value> {
    if schema["kind"] == "any" {
        return vec![
            Value::Null,
            json!(false),
            json!(2.25),
            json!("any-text"),
            json!([]),
            json!({"key": true}),
        ];
    }
    if schema["kind"] != "array" {
        return vec![sample(schema, 3)];
    }
    let children = array_samples(&schema["item"]);
    let mut result = vec![json!([])];
    for child in &children {
        result.push(json!([child]));
        result.push(json!([child, child, child]));
    }
    result.push(Value::Array(children));
    result
}
fn malformed_nested(schema: &Value, positive: &Value) -> Vec<Value> {
    let mut result = Vec::new();
    match schema["kind"].as_str().unwrap() {
        "record" => {
            for field in schema["fields"].as_array().unwrap() {
                let key = field["name"].as_str().unwrap();
                if field["optional"] != true {
                    let mut missing = positive.clone();
                    missing.as_object_mut().unwrap().remove(key);
                    result.push(missing);
                }
                for child in malformed_nested(&field["type"], &positive[key]) {
                    let mut wrong = positive.clone();
                    wrong[key] = child;
                    result.push(wrong);
                }
            }
            result.push(Value::Array(
                positive.as_object().unwrap().values().cloned().collect(),
            ));
        }
        "array" => {
            for child in malformed_nested(&schema["item"], &positive[0]) {
                let mut wrong = positive.clone();
                wrong[0] = child;
                result.push(wrong);
            }
        }
        "any" => {}
        "union" | "literal" | "null" | "undefined" => {
            if let Some(wrong) = [
                Value::Null,
                json!({}),
                json!([]),
                json!(false),
                json!(17),
                json!("bad"),
            ]
            .into_iter()
            .find(|v| !schema_accepts(schema, v))
            {
                result.push(wrong);
            }
        }
        _ => result.push(Value::Null),
    }
    result
}
fn check<T: DeserializeOwned + Serialize>(
    name: &str,
    schema: &Value,
    numbers: &Value,
    concrete: impl Fn(Value) -> bool,
) {
    let positive = sample(schema, 1);
    let value: T = serde_json::from_value(positive.clone())
        .unwrap_or_else(|e| panic!("ApexOmni record schema {name}: {e}"));
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        positive,
        "ApexOmni record schema {name}: serialized fields"
    );
    assert!(
        concrete(positive.clone()),
        "ApexOmni record schema {name}: concrete positive"
    );
    fn replace_first_string(value: &mut Value) -> bool {
        match value {
            Value::String(text) => {
                *text = "SURROGATE_WITNESS".into();
                true
            }
            Value::Array(values) => values.iter_mut().any(replace_first_string),
            Value::Object(values) => values.values_mut().any(replace_first_string),
            _ => false,
        }
    }
    let mut boundary = positive.clone();
    assert!(
        replace_first_string(&mut boundary),
        "{name}: string boundary witness"
    );
    let raw = serde_json::to_string(&boundary)
        .unwrap()
        .replace("\"SURROGATE_WITNESS\"", r#""\ud800""#);
    if name == "MultiChainObject" {
        // Its opaque `chains: Value` reader still refuses this boundary before
        // any typed String is read. This is not the shared String expectation.
        assert!(
            serde_json::from_str::<T>(&raw).is_err(),
            "opaque chains Value boundary"
        );
        typed_utf16::retained(&raw);
    } else {
        typed_utf16::assert_projection(&raw, &serde_json::from_str::<T>(&raw).unwrap());
    }
    for field in schema["fields"].as_array().unwrap() {
        let key = field["name"].as_str().unwrap();
        let mut missing = positive.clone();
        missing.as_object_mut().unwrap().remove(key);
        if field["optional"] == true {
            let actual: T = serde_json::from_value(missing.clone())
                .unwrap_or_else(|e| panic!("ApexOmni record schema {name}: optional {key}: {e}"));
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                missing,
                "{name}: omitted {key} stays absent"
            );
            assert!(concrete(missing), "{name}: concrete optional {key}");
        } else {
            assert!(
                serde_json::from_value::<T>(missing.clone()).is_err(),
                "ApexOmni record schema {name}: missing {key}"
            );
            assert!(
                !concrete(missing),
                "ApexOmni record schema {name}: concrete missing {key}"
            );
        }
        let kind = field["type"]["kind"].as_str().unwrap();
        if kind == "union" || kind == "literal" {
            for variant in union_samples(&field["type"], 97) {
                let mut candidate = positive.clone();
                candidate[key] = variant;
                let actual: T = serde_json::from_value(candidate.clone()).unwrap_or_else(|e| {
                    panic!("ApexOmni record schema {name}: union member {key}: {e}")
                });
                assert_eq!(
                    serde_json::to_value(actual).unwrap(),
                    candidate,
                    "{name}.{key}: every actual TS union member"
                );
                assert!(concrete(candidate), "{name}.{key}: concrete union member");
            }
        }
        for wrong in [
            Value::Null,
            json!(true),
            json!({}),
            json!([]),
            json!(17),
            json!("text"),
        ] {
            if kind == "any" {
                let mut candidate = positive.clone();
                candidate[key] = wrong;
                let actual: T = serde_json::from_value(candidate.clone()).unwrap();
                assert_eq!(
                    serde_json::to_value(actual).unwrap(),
                    candidate,
                    "{name}: any field {key} accepts JSON shapes"
                );
                assert!(concrete(candidate), "{name}: concrete any field {key}");
                continue;
            }
            if (kind == "union" || kind == "literal" || kind == "null")
                && schema_accepts(&field["type"], &wrong)
            {
                continue;
            }
            if (kind == "string" && wrong.is_string())
                || (kind == "number" && wrong.is_number())
                || (kind == "boolean" && wrong.is_boolean())
                || (kind == "array" && wrong.is_array())
            {
                continue;
            }
            let mut malformed = positive.clone();
            malformed[key] = wrong;
            assert!(
                serde_json::from_value::<T>(malformed.clone()).is_err(),
                "ApexOmni record schema {name}: wrong field {key}"
            );
            assert!(
                !concrete(malformed),
                "ApexOmni record schema {name}: concrete wrong field {key}"
            );
        }
        if kind == "array" {
            for wrong in [json!([1]), json!([[1]]), json!([null]), json!([[null]])] {
                if field["type"]["item"]["kind"] == "array"
                    && field["type"]["item"]["item"]["kind"] == "any"
                    && wrong.as_array().unwrap().iter().all(Value::is_array)
                {
                    continue;
                }
                let mut malformed = positive.clone();
                malformed[key] = wrong;
                assert!(
                    serde_json::from_value::<T>(malformed.clone()).is_err(),
                    "ApexOmni record schema {name}: nested field {key}"
                );
                assert!(
                    !concrete(malformed),
                    "ApexOmni record schema {name}: concrete nested field {key}"
                );
            }
        }
    }
    for malformed in malformed_nested(schema, &positive) {
        assert!(
            serde_json::from_value::<T>(malformed.clone()).is_err(),
            "ApexOmni record schema {name}: recursive required fields and map shapes"
        );
        assert!(
            !concrete(malformed),
            "ApexOmni record schema {name}: concrete recursive fields"
        );
    }
    for field in schema["fields"].as_array().unwrap() {
        if field["type"]["kind"] != "array" {
            continue;
        }
        let key = field["name"].as_str().unwrap();
        for array in array_samples(&field["type"]) {
            let mut candidate = positive.clone();
            candidate[key] = array;
            let actual: T = serde_json::from_value(candidate.clone()).unwrap_or_else(|e| {
                panic!("ApexOmni record schema {name}: variable array length {e}")
            });
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                candidate,
                "{name}: array output"
            );
            assert!(
                concrete(candidate),
                "{name}: concrete variable array length"
            );
        }
    }
    for field in schema["fields"].as_array().unwrap() {
        if field["type"]["kind"] != "number" {
            continue;
        }
        let key = field["name"].as_str().unwrap();
        let original = serde_json::to_string(&positive).unwrap();
        let pair = format!("{}:{}", serde_json::to_string(key).unwrap(), positive[key]);
        let mut numeric_comparisons = 0;
        for case in numbers.as_array().unwrap() {
            numeric_comparisons += 1;
            let raw = original.replace(
                &pair,
                &format!(
                    "{}:{}",
                    serde_json::to_string(key).unwrap(),
                    case["raw"].as_str().unwrap()
                ),
            );
            let expected = original.replace(
                &pair,
                &format!(
                    "{}:{}",
                    serde_json::to_string(key).unwrap(),
                    case["numberJson"].as_str().unwrap()
                ),
            );
            let actual: T = serde_json::from_str(&raw).unwrap();
            assert_eq!(
                wire_json(&actual),
                expected,
                "{name}.{key}: live JS numeric output"
            );
        }
        assert_eq!(
            numeric_comparisons, 14,
            "APEX-CARDINALITY native numeric comparisons"
        );
    }
    for field in schema["fields"].as_array().unwrap() {
        if field["type"]["kind"] != "boolean" {
            continue;
        }
        let key = field["name"].as_str().unwrap();
        let mut candidate = positive.clone();
        candidate[key] = json!(false);
        let actual: T = serde_json::from_value(candidate.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            candidate,
            "{name}.{key}: false remains present"
        );
        assert!(concrete(candidate), "{name}.{key}: concrete false");
    }
    let mut required_only = positive.clone();
    for field in schema["fields"].as_array().unwrap() {
        if field["optional"] == true {
            required_only
                .as_object_mut()
                .unwrap()
                .remove(field["name"].as_str().unwrap());
        }
    }
    let actual: T = serde_json::from_value(required_only.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        required_only,
        "{name}: all optionals absent"
    );
    assert!(
        concrete(required_only),
        "{name}: concrete all optionals absent"
    );
    let sequence = Value::Array(
        schema["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| positive[f["name"].as_str().unwrap()].clone())
            .collect(),
    );
    for wrong in [
        sequence,
        json!([]),
        json!(null),
        json!(false),
        json!(1),
        json!("record"),
    ] {
        assert!(
            serde_json::from_value::<T>(wrong.clone()).is_err(),
            "ApexOmni record schema {name}: map-only shape"
        );
        assert!(
            !concrete(wrong),
            "ApexOmni record schema {name}: concrete map-only shape"
        );
    }
}
#[test]
fn public_records_match_live_typescript() {
    let schemas = oracle();
    assert_eq!(
        schemas["$stringBoundary"]["units"],
        json!([55296]),
        "TypeScript retains valid JSON lone surrogate"
    );
    assert_eq!(
        schemas["$stringBoundary"]["serialized"],
        json!(r#""\ud800""#)
    );
    check::<TickerObject>(
        "TickerObject",
        &schemas["TickerObject"],
        &schemas["$numbers"],
        |v| TickerObject::deserialize(v).is_ok(),
    );
    check::<DepthObject>(
        "DepthObject",
        &schemas["DepthObject"],
        &schemas["$numbers"],
        |v| DepthObject::deserialize(v).is_ok(),
    );
    check::<TradeObject>(
        "TradeObject",
        &schemas["TradeObject"],
        &schemas["$numbers"],
        |v| TradeObject::deserialize(v).is_ok(),
    );
    check::<KlineObject>(
        "KlineObject",
        &schemas["KlineObject"],
        &schemas["$numbers"],
        |v| KlineObject::deserialize(v).is_ok(),
    );
    check::<HistoryFundingObject>(
        "HistoryFundingObject",
        &schemas["HistoryFundingObject"],
        &schemas["$numbers"],
        |v| HistoryFundingObject::deserialize(v).is_ok(),
    );
    check::<PerpetualCurrencyObject>(
        "PerpetualCurrencyObject",
        &schemas["PerpetualCurrencyObject"],
        &schemas["$numbers"],
        |v| PerpetualCurrencyObject::deserialize(v).is_ok(),
    );
    check::<CurrencyObject>(
        "CurrencyObject",
        &schemas["CurrencyObject"],
        &schemas["$numbers"],
        |v| CurrencyObject::deserialize(v).is_ok(),
    );
    check::<GlobalObject>(
        "GlobalObject",
        &schemas["GlobalObject"],
        &schemas["$numbers"],
        |v| GlobalObject::deserialize(v).is_ok(),
    );
    check::<PerpetualContractObject>(
        "PerpetualContractObject",
        &schemas["PerpetualContractObject"],
        &schemas["$numbers"],
        |v| PerpetualContractObject::deserialize(v).is_ok(),
    );
    check::<GetHistoryFundingOptions>(
        "GetHistoryFundingOptions",
        &schemas["GetHistoryFundingOptions"],
        &schemas["$numbers"],
        |v| GetHistoryFundingOptions::deserialize(v).is_ok(),
    );
    check::<KeyPair>("KeyPair", &schemas["KeyPair"], &schemas["$numbers"], |v| {
        KeyPair::deserialize(v).is_ok()
    });
    check::<ApiKeyCredentials>(
        "ApiKeyCredentials",
        &schemas["ApiKeyCredentials"],
        &schemas["$numbers"],
        |v| ApiKeyCredentials::deserialize(v).is_ok(),
    );
}
#[test]
fn changed_typescript_record_fails_named_assertion() {
    for mutation in [
        "record",
        "record-boolean",
        "record-optional",
        "record-nested",
        "record-inherited",
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "public_records_match_live_typescript",
                "--nocapture",
            ])
            .env("APEXOMNI_MUTATE_TS", mutation)
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{text}");
        assert!(
            text.contains("public_records_match_live_typescript")
                && text.contains("FAILED")
                && text.contains("ApexOmni record schema"),
            "{text}"
        );
    }
}

#[test]
fn raw_numbers_preserve_sign_and_use_javascript_json_spelling() {
    let reference = oracle();
    let mut numeric_comparisons = 0;
    for case in reference["$numbers"].as_array().unwrap() {
        numeric_comparisons += 1;
        let raw = format!(
            r#"{{"a":[],"b":[],"s":"BTC-USDT","u":{}}}"#,
            case["raw"].as_str().unwrap()
        );
        let record: DepthObject = serde_json::from_str(&raw).unwrap();
        let tag = if record.u == f64::INFINITY {
            "Infinity".to_owned()
        } else if record.u == f64::NEG_INFINITY {
            "-Infinity".to_owned()
        } else if record.u == 0.0 && record.u.is_sign_negative() {
            "-0".to_owned()
        } else {
            safe_cex_types::js_coercion::js_number_string(record.u)
        };
        assert_eq!(
            tag,
            case["tag"].as_str().unwrap(),
            "raw decoded numeric value"
        );
        assert_eq!(
            wire_json(&record),
            case["serialized"].as_str().unwrap(),
            "raw numeric serialized spelling"
        );
    }
    assert_eq!(
        numeric_comparisons, 14,
        "APEX-CARDINALITY native numeric comparisons"
    );
}

#[test]
fn every_numeric_field_preserves_raw_and_existing_value_bits() {
    let reference = oracle();
    let mut losses = Vec::new();
    macro_rules! check_bits {
        ($ty:ident, $wire:literal, $field:ident, $read:expr) => {{
            let positive = sample(&reference[stringify!($ty)], 1);
            let mut numeric_comparisons = 0;
            for case in reference["$numbers"].as_array().unwrap() {
                numeric_comparisons += 1;
                let raw = case["raw"].as_str().unwrap();
                let expected_bits =
                    u64::from_str_radix(case["bits"].as_str().unwrap(), 16).unwrap();
                let expected = f64::from_bits(expected_bits);
                let mut input = positive.clone();
                input[$wire] = serde_json::from_str(raw).unwrap();
                assert_eq!(
                    safe_cex_num::numeric_ingress::js_value_as_f64(&input[$wire])
                        .unwrap()
                        .to_bits(),
                    expected.to_bits(),
                    "input retains source bits"
                );
                let record: $ty = serde_json::from_value(input.clone()).unwrap();
                let read: fn($ty) -> f64 = $read;
                if read(record).to_bits() != expected.to_bits() {
                    losses.push(format!(
                        "{}.{} existing Value {raw}",
                        stringify!($ty),
                        $wire
                    ));
                }
                let record: $ty =
                    serde_json::from_str(&serde_json::to_string(&input).unwrap()).unwrap();
                if read(record).to_bits() != expected.to_bits() {
                    losses.push(format!("{}.{} raw JSON {raw}", stringify!($ty), $wire));
                }
                for record in [
                    safe_cex_num::numeric_ingress::js_from_value::<$ty>(input.clone()).unwrap(),
                    safe_cex_num::numeric_ingress::js_from_value_ref::<$ty>(&input).unwrap(),
                    safe_cex_num::numeric_ingress::js_from_str::<$ty>(
                        &serde_json::to_string(&input).unwrap(),
                    )
                    .unwrap(),
                ] {
                    assert_eq!(
                        read(record).to_bits(),
                        expected_bits,
                        "{}.{} shared ingress bits {raw}",
                        stringify!($ty),
                        $wire
                    );
                }
                if expected == 0.0 && expected.is_sign_negative() {
                    input[$wire] = json!(-0.0_f64);
                    let record: $ty = serde_json::from_value(input).unwrap();
                    if read(record).to_bits() != expected.to_bits() {
                        losses.push(format!(
                            "{}.{} native Value negative zero",
                            stringify!($ty),
                            $wire
                        ));
                    }
                }
            }
            assert_eq!(
                numeric_comparisons, 14,
                "APEX-CARDINALITY native numeric comparisons"
            );
        }};
    }
    check_bits!(DepthObject, "u", u, |v| v.u);
    check_bits!(TradeObject, "t", t, |v| v.t);
    check_bits!(KlineObject, "t", t, |v| v.t);
    check_bits!(HistoryFundingObject, "fundingTime", funding_time, |v| v
        .funding_time);
    check_bits!(
        HistoryFundingObject,
        "fundingTimestamp",
        funding_timestamp,
        |v| v.funding_timestamp
    );
    check_bits!(
        GlobalObject,
        "starkExFundingValidityPeriod",
        stark_ex_funding_validity_period,
        |v| v.stark_ex_funding_validity_period
    );
    check_bits!(
        GlobalObject,
        "starkExOrdersTreeHeight",
        stark_ex_orders_tree_height,
        |v| v.stark_ex_orders_tree_height
    );
    check_bits!(
        GlobalObject,
        "starkExPositionsTreeHeight",
        stark_ex_positions_tree_height,
        |v| v.stark_ex_positions_tree_height
    );
    check_bits!(
        GlobalObject,
        "starkExPriceValidityPeriod",
        stark_ex_price_validity_period,
        |v| v.stark_ex_price_validity_period
    );
    check_bits!(GlobalObject, "registerEnvId", register_env_id, |v| v
        .register_env_id);
    check_bits!(PerpetualContractObject, "crossId", cross_id, |v| v.cross_id);
    check_bits!(
        PerpetualContractObject,
        "crossSymbolId",
        cross_symbol_id,
        |v| v.cross_symbol_id
    );
    check_bits!(GetHistoryFundingOptions, "page", page, |v| v.page.unwrap());
    check_bits!(
        GetHistoryFundingOptions,
        "beginTimeInclusive",
        begin_time_inclusive,
        |v| v.begin_time_inclusive.unwrap()
    );
    check_bits!(
        GetHistoryFundingOptions,
        "endTimeExclusive",
        end_time_exclusive,
        |v| v.end_time_exclusive.unwrap()
    );
    check_bits!(GetHistoryFundingOptions, "limit", limit, |v| v
        .limit
        .unwrap());
    check_bits!(OpenPositionObject, "createdAt", created_at, |v| v
        .created_at);
    check_bits!(OpenPositionObject, "updatedTime", updated_time, |v| v
        .updated_time);
    check_bits!(AccountsItem, "createdAt", created_at, |v| v.created_at);
    check_bits!(AccountObject, "createdAt", created_at, |v| v.created_at);
    check_bits!(FundingRateObject, "fundingTime", funding_time, |v| v
        .funding_time);
    check_bits!(HistoricalPNLObject, "createdAt", created_at, |v| v
        .created_at);
    check_bits!(SymbolInfoObject, "rankIdx", rank_idx, |v| v
        .rank_idx
        .unwrap());
    check_bits!(SymbolInfoObject, "pricePrecision", price_precision, |v| v
        .price_precision
        .unwrap());
    check_bits!(SymbolInfoObject, "priceStep", price_step, |v| v
        .price_step
        .unwrap());
    check_bits!(SymbolInfoObject, "sizePrecision", size_precision, |v| v
        .size_precision
        .unwrap());
    check_bits!(SymbolInfoObject, "sizeStep", size_step, |v| v
        .size_step
        .unwrap());
    check_bits!(
        SymbolInfoObject,
        "baseCoinPrecision",
        base_coin_precision,
        |v| v.base_coin_precision.unwrap()
    );
    check_bits!(
        SymbolInfoObject,
        "baseCoinRealPrecision",
        base_coin_real_precision,
        |v| v.base_coin_real_precision.unwrap()
    );
    check_bits!(
        SymbolInfoObject,
        "currentCoinPrecision",
        current_coin_precision,
        |v| v.current_coin_precision.unwrap()
    );
    check_bits!(SymbolInfoObject, "crossId", cross_id, |v| v.cross_id);
    check_bits!(SymbolInfoObject, "crossSymbolId", cross_symbol_id, |v| v
        .cross_symbol_id);
    check_bits!(OrderObject, "createdAt", created_at, |v| v.created_at);
    check_bits!(OrderObject, "updatedTime", updated_time, |v| v.updated_time);
    assert!(losses.is_empty(), "numeric ingress losses: {losses:#?}");
}

// JSON projection is explicit: Serialize retains native numeric events for ingress.
fn wire_json<T: Serialize>(value: &T) -> String {
    safe_cex_num::js_json_stringify(&serde_json::to_value(value).unwrap().into())
        .unwrap()
        .unwrap()
}

#[test]
fn decoded_fields_match_typescript_schema_individually() {
    let reference = oracle();
    macro_rules! fields {
        ($ty:ident, $( $field:ident => $wire:literal ),+ $(,)?) => {{
            let mut native_names = [$($wire),+];
            native_names.sort_unstable();
            let mut source_names: Vec<_> = reference[stringify!($ty)]["fields"].as_array().unwrap()
                .iter().map(|field| field["name"].as_str().unwrap()).collect();
            source_names.sort_unstable();
            assert_eq!(source_names, native_names, "{} native field inventory", stringify!($ty));
            let input = sample(&reference[stringify!($ty)], 1);
            let mut cases = vec![input.clone()];
            for field in reference[stringify!($ty)]["fields"].as_array().unwrap() {
                if field["type"]["kind"] == "boolean" {
                    let mut one_false = input.clone();
                    one_false[field["name"].as_str().unwrap()] = json!(false);
                    cases.push(one_false);
                }
            }
            for input in cases {
                let actual: $ty = serde_json::from_value(input.clone()).unwrap();
                $(assert_eq!(serde_json::to_value(&actual.$field).unwrap(), input[$wire],
                    "{}.{} decoded native field", stringify!($ty), $wire);)+
            }
        }};
    }
    fields!(TickerObject, funding_rate => "fundingRate", high_price24h => "highPrice24h", index_price => "indexPrice", last_price => "lastPrice", low_price24h => "lowPrice24h", next_funding_time => "nextFundingTime", open_interest => "openInterest", oracle_price => "oraclePrice", predicted_funding_rate => "predictedFundingRate", price24h_pcnt => "price24hPcnt", symbol => "symbol", trade_count => "tradeCount", turnover24h => "turnover24h", volume24h => "volume24h");
    fields!(DepthObject, a => "a", b => "b", s => "s", u => "u");
    fields!(TradeObject, i => "i", p => "p", side => "S", v => "v", s => "s", t => "t");
    fields!(KlineObject, s => "s", i => "i", t => "t", c => "c", h => "h", l => "l", o => "o", v => "v", tr => "tr");
    fields!(HistoryFundingObject, symbol => "symbol", rate => "rate", price => "price", funding_time => "fundingTime", funding_timestamp => "fundingTimestamp");
    fields!(CurrencyObject, id => "id", stark_ex_asset_id => "starkExAssetId", stark_ex_resolution => "starkExResolution", step_size => "stepSize", show_step => "showStep", icon_url => "iconUrl");
    fields!(GlobalObject, fee_account_id => "feeAccountId", fee_account_l2_key => "feeAccountL2Key", stark_ex_collateral_currency_id => "starkExCollateralCurrencyId", stark_ex_funding_validity_period => "starkExFundingValidityPeriod", stark_ex_max_funding_rate => "starkExMaxFundingRate", stark_ex_orders_tree_height => "starkExOrdersTreeHeight", stark_ex_positions_tree_height => "starkExPositionsTreeHeight", stark_ex_price_validity_period => "starkExPriceValidityPeriod", stark_ex_contract_address => "starkExContractAddress", register_env_id => "registerEnvId", cross_chain_account_id => "crossChainAccountId", cross_chain_l2_key => "crossChainL2Key", fast_withdraw_account_id => "fastWithdrawAccountId", fast_withdraw_fact_register_address => "fastWithdrawFactRegisterAddress", fast_withdraw_l2_key => "fastWithdrawL2Key", fast_withdraw_max_amount => "fastWithdrawMaxAmount", bybit_withdraw_account_id => "bybitWithdrawAccountId", bybit_withdraw_l2_key => "bybitWithdrawL2Key");
    fields!(PerpetualContractObject, baseline_position_value => "baselinePositionValue", cross_id => "crossId", cross_symbol_id => "crossSymbolId", cross_symbol_name => "crossSymbolName", digit_merge => "digitMerge", display_max_leverage => "displayMaxLeverage", display_min_leverage => "displayMinLeverage", enable_display => "enableDisplay", enable_open_position => "enableOpenPosition", enable_trade => "enableTrade", funding_impact_margin_notional => "fundingImpactMarginNotional", funding_interest_rate => "fundingInterestRate", incremental_initial_margin_rate => "incrementalInitialMarginRate", incremental_maintenance_margin_rate => "incrementalMaintenanceMarginRate", incremental_position_value => "incrementalPositionValue", initial_margin_rate => "initialMarginRate", maintenance_margin_rate => "maintenanceMarginRate", max_order_size => "maxOrderSize", max_position_size => "maxPositionSize", min_order_size => "minOrderSize", max_market_price_range => "maxMarketPriceRange", settle_currency_id => "settleCurrencyId", stark_ex_oracle_price_quorum => "starkExOraclePriceQuorum", stark_ex_resolution => "starkExResolution", stark_ex_risk_factor => "starkExRiskFactor", stark_ex_synthetic_asset_id => "starkExSyntheticAssetId", step_size => "stepSize", symbol => "symbol", symbol_display_name => "symbolDisplayName", symbol_display_name2 => "symbolDisplayName2", tick_size => "tickSize", underlying_currency_id => "underlyingCurrencyId", max_maintenance_margin_rate => "maxMaintenanceMarginRate", max_position_value => "maxPositionValue");
    fields!(GetHistoryFundingOptions, symbol => "symbol", page => "page", begin_time_inclusive => "beginTimeInclusive", end_time_exclusive => "endTimeExclusive", limit => "limit");
    fields!(KeyPair, public_key => "publicKey", public_key_y_coordinate => "publicKeyYCoordinate", private_key => "privateKey");
    fields!(ApiKeyCredentials, key => "key", secret => "secret", passphrase => "passphrase");
    fields!(PerpetualCurrencyObject, usdc => "usdc", usdt => "usdt");
}

#[test]
fn private_records_match_live_typescript() {
    let schemas = oracle();
    let mut failures = Vec::new();
    if std::panic::catch_unwind(|| {
        check::<WalletObject>(
            "WalletObject",
            &schemas["WalletObject"],
            &schemas["$numbers"],
            |v| WalletObject::deserialize(v).is_ok(),
        )
    })
    .is_err()
    {
        failures.push("WalletObject");
    }
    if std::panic::catch_unwind(|| {
        check::<OpenPositionObject>(
            "OpenPositionObject",
            &schemas["OpenPositionObject"],
            &schemas["$numbers"],
            |v| OpenPositionObject::deserialize(v).is_ok(),
        )
    })
    .is_err()
    {
        failures.push("OpenPositionObject");
    }
    if std::panic::catch_unwind(|| {
        check::<AccountsItem>(
            "AccountsItem",
            &schemas["AccountsItem"],
            &schemas["$numbers"],
            |v| AccountsItem::deserialize(v).is_ok(),
        )
    })
    .is_err()
    {
        failures.push("AccountsItem");
    }
    if std::panic::catch_unwind(|| {
        check::<AccountObject>(
            "AccountObject",
            &schemas["AccountObject"],
            &schemas["$numbers"],
            |v| AccountObject::deserialize(v).is_ok(),
        )
    })
    .is_err()
    {
        failures.push("AccountObject");
    }
    if std::panic::catch_unwind(|| {
        check::<WorstPriceObject>(
            "WorstPriceObject",
            &schemas["WorstPriceObject"],
            &schemas["$numbers"],
            |v| WorstPriceObject::deserialize(v).is_ok(),
        )
    })
    .is_err()
    {
        failures.push("WorstPriceObject");
    }
    if std::panic::catch_unwind(|| {
        check::<FundingRateObject>(
            "FundingRateObject",
            &schemas["FundingRateObject"],
            &schemas["$numbers"],
            |v| FundingRateObject::deserialize(v).is_ok(),
        )
    })
    .is_err()
    {
        failures.push("FundingRateObject");
    }
    if std::panic::catch_unwind(|| {
        check::<HistoricalPNLObject>(
            "HistoricalPNLObject",
            &schemas["HistoricalPNLObject"],
            &schemas["$numbers"],
            |v| HistoricalPNLObject::deserialize(v).is_ok(),
        )
    })
    .is_err()
    {
        failures.push("HistoricalPNLObject");
    }
    if std::panic::catch_unwind(|| {
        check::<AccountBalanceObject>(
            "AccountBalanceObject",
            &schemas["AccountBalanceObject"],
            &schemas["$numbers"],
            |v| AccountBalanceObject::deserialize(v).is_ok(),
        )
    })
    .is_err()
    {
        failures.push("AccountBalanceObject");
    }
    assert!(
        failures.is_empty(),
        "private record row failures: {failures:?}"
    );
}

#[test]
fn private_decoded_fields_match_typescript() {
    let reference = oracle();
    macro_rules! fields {
        ($ty:ident, $( $field:ident => $wire:literal ),+ $(,)?) => {{
            let mut native_names = [$($wire),+];
            native_names.sort_unstable();
            let mut source_names: Vec<_> = reference[stringify!($ty)]["fields"].as_array().unwrap()
                .iter().map(|field| field["name"].as_str().unwrap()).collect();
            source_names.sort_unstable();
            assert_eq!(source_names, native_names, "{} native field inventory", stringify!($ty));
            let input = sample(&reference[stringify!($ty)], 1);
            let mut cases = vec![input.clone()];
            for field in reference[stringify!($ty)]["fields"].as_array().unwrap() {
                if field["type"]["kind"] == "boolean" {
                    let mut one_false = input.clone();
                    one_false[field["name"].as_str().unwrap()] = json!(false);
                    cases.push(one_false);
                }
            }
            for input in cases {
                let actual: $ty = serde_json::from_value(input.clone()).unwrap();
                assert_eq!(serde_json::to_value(&actual).unwrap(), input, "{} structural roundtrip before native assertions", stringify!($ty));
                $(assert_eq!(serde_json::to_value(&actual.$field).unwrap(), input[$wire],
                    "{}.{} decoded native field", stringify!($ty), $wire);)+
            }
        }};
    }
    let mut failures = Vec::new();
    if std::panic::catch_unwind(|| { fields!(WalletObject, user_id => "userId", account_id => "accountId", balance => "balance", asset => "asset", pending_deposit_amount => "pendingDepositAmount", pending_withdraw_amount => "pendingWithdrawAmount", pending_transfer_out_amount => "pendingTransferOutAmount", pending_transfer_in_amount => "pendingTransferInAmount"); }).is_err() { failures.push("WalletObject"); }
    if std::panic::catch_unwind(|| { fields!(OpenPositionObject, user_id => "userId", account_id => "accountId", symbol => "symbol", status => "status", side => "side", size => "size", max_size => "maxSize", entry_price => "entryPrice", exit_price => "exitPrice", unrealized_pnl => "unrealizedPnl", realized_pnl => "realizedPnl", created_at => "createdAt", updated_time => "updatedTime", net_funding => "netFunding", sum_open => "sumOpen", sum_close => "sumClose", fee => "fee", funding_fee => "fundingFee", light_numbers => "lightNumbers", custom_initial_margin_rate => "customInitialMarginRate"); }).is_err() { failures.push("OpenPositionObject"); }
    if std::panic::catch_unwind(|| { fields!(AccountsItem, created_at => "createdAt", taker_fee_rate => "takerFeeRate", maker_fee_rate => "makerFeeRate", min_initial_margin_rate => "minInitialMarginRate", status => "status", token => "token", unrealize_pnl_price_type => "unrealizePnlPriceType"); }).is_err() { failures.push("AccountsItem"); }
    if std::panic::catch_unwind(|| { fields!(AccountObject, stark_key => "starkKey", position_id => "positionId", quote_balance => "quoteBalance", pending_deposits => "pendingDeposits", pending_withdrawals => "pendingWithdrawals", created_at => "createdAt", account_number => "accountNumber", admin_signature => "adminSignature", ethereum_address => "ethereumAddress", id => "id", taker_fee_rate => "takerFeeRate", maker_fee_rate => "makerFeeRate", wallets => "wallets", open_positions => "openPositions", accounts => "accounts"); }).is_err() { failures.push("AccountObject"); }
    if std::panic::catch_unwind(|| { fields!(WorstPriceObject, worst_price => "worstPrice", bid_one_price => "bidOnePrice", ask_one_price => "askOnePrice"); }).is_err() { failures.push("WorstPriceObject"); }
    if std::panic::catch_unwind(|| { fields!(FundingRateObject, id => "id", symbol => "symbol", rate => "rate", position_size => "positionSize", price => "price", funding_time => "fundingTime", side => "side", funding_value => "fundingValue", status => "status", transaction_id => "transactionId"); }).is_err() { failures.push("FundingRateObject"); }
    if std::panic::catch_unwind(|| { fields!(HistoricalPNLObject, symbol => "symbol", size => "size", exit_price => "exitPrice", price => "price", side => "side", total_pnl => "totalPnl", created_at => "createdAt", r#type => "type", is_liquidate => "isLiquidate", is_deleverage => "isDeleverage", fee => "fee", close_shared_open_fee => "closeSharedOpenFee", liquidate_fee => "liquidateFee"); }).is_err() { failures.push("HistoricalPNLObject"); }
    if std::panic::catch_unwind(|| { fields!(AccountBalanceObject, total_equity_value => "totalEquityValue", available_balance => "availableBalance", initial_margin => "initialMargin", maintenance_margin => "maintenanceMargin"); }).is_err() { failures.push("AccountBalanceObject"); }
    assert!(
        failures.is_empty(),
        "private native row failures: {failures:?}"
    );
}

#[test]
fn config_records_match_live_typescript() {
    let schemas = oracle();
    let mut failures = Vec::new();
    if std::panic::catch_unwind(|| {
        check::<MultiChainObject>(
            "MultiChainObject",
            &schemas["MultiChainObject"],
            &schemas["$numbers"],
            |v| MultiChainObject::deserialize(v).is_ok(),
        )
    })
    .is_err()
    {
        failures.push("MultiChainObject");
    }
    if std::panic::catch_unwind(|| {
        check::<SymbolObject>(
            "SymbolObject",
            &schemas["SymbolObject"],
            &schemas["$numbers"],
            |v| SymbolObject::deserialize(v).is_ok(),
        )
    })
    .is_err()
    {
        failures.push("SymbolObject");
    }
    if std::panic::catch_unwind(|| {
        check::<PerpetualObject>(
            "PerpetualObject",
            &schemas["PerpetualObject"],
            &schemas["$numbers"],
            |v| PerpetualObject::deserialize(v).is_ok(),
        )
    })
    .is_err()
    {
        failures.push("PerpetualObject");
    }
    if std::panic::catch_unwind(|| {
        check::<SymbolInfoObject>(
            "SymbolInfoObject",
            &schemas["SymbolInfoObject"],
            &schemas["$numbers"],
            |v| SymbolInfoObject::deserialize(v).is_ok(),
        )
    })
    .is_err()
    {
        failures.push("SymbolInfoObject");
    }
    if std::panic::catch_unwind(|| {
        check::<UserObject>(
            "UserObject",
            &schemas["UserObject"],
            &schemas["$numbers"],
            |v| UserObject::deserialize(v).is_ok(),
        )
    })
    .is_err()
    {
        failures.push("UserObject");
    }
    if std::panic::catch_unwind(|| {
        check::<OrderObject>(
            "OrderObject",
            &schemas["OrderObject"],
            &schemas["$numbers"],
            |v| OrderObject::deserialize(v).is_ok(),
        )
    })
    .is_err()
    {
        failures.push("OrderObject");
    }
    assert!(
        failures.is_empty(),
        "config record row failures: {failures:?}"
    );
}

#[test]
fn config_decoded_fields_match_typescript() {
    let reference = oracle();
    macro_rules! fields {
        ($ty:ident, $( $field:ident => $wire:literal ),+ $(,)?) => {{
            let mut native_names = [$($wire),+];
            native_names.sort_unstable();
            let mut source_names: Vec<_> = reference[stringify!($ty)]["fields"].as_array().unwrap()
                .iter().map(|field| field["name"].as_str().unwrap()).collect();
            source_names.sort_unstable();
            assert_eq!(source_names, native_names, "{} native field inventory", stringify!($ty));
            let input = sample(&reference[stringify!($ty)], 1);
            let mut cases = vec![input.clone()];
            for field in reference[stringify!($ty)]["fields"].as_array().unwrap() {
                if field["type"]["kind"] == "boolean" {
                    let mut one_false = input.clone();
                    one_false[field["name"].as_str().unwrap()] = json!(false);
                    cases.push(one_false);
                }
            }
            for input in cases {
                let actual: $ty = serde_json::from_value(input.clone()).unwrap();
                $(assert_eq!(serde_json::to_value(&actual.$field).unwrap(), input[$wire],
                    "{}.{} decoded native field", stringify!($ty), $wire);)+
            }
        }};
    }
    let mut failures = Vec::new();
    if std::panic::catch_unwind(|| { fields!(MultiChainObject, chains => "chains", currency => "currency", max_withdraw => "maxWithdraw", min_deposit => "minDeposit", min_withdraw => "minWithdraw"); }).is_err() { failures.push("MultiChainObject"); }
    if std::panic::catch_unwind(|| { fields!(SymbolObject, currency => "currency", global => "global", perpetual_contract => "perpetualContract", multi_chain => "multiChain"); }).is_err() { failures.push("SymbolObject"); }
    if std::panic::catch_unwind(|| {
        fields!(PerpetualObject, usdc_config => "usdcConfig", usdt_config => "usdtConfig");
    })
    .is_err()
    {
        failures.push("PerpetualObject");
    }
    if std::panic::catch_unwind(|| { fields!(SymbolInfoObject, rank_idx => "rankIdx", price_precision => "pricePrecision", price_step => "priceStep", size_precision => "sizePrecision", size_step => "sizeStep", base_coin => "baseCoin", current_coin => "currentCoin", base_coin_precision => "baseCoinPrecision", base_coin_real_precision => "baseCoinRealPrecision", current_coin_precision => "currentCoinPrecision", base_coin_icon => "baseCoinIcon", current_coin_icon => "currentCoinIcon", baseline_position_value => "baselinePositionValue", cross_id => "crossId", cross_symbol_id => "crossSymbolId", cross_symbol_name => "crossSymbolName", digit_merge => "digitMerge", display_max_leverage => "displayMaxLeverage", display_min_leverage => "displayMinLeverage", enable_display => "enableDisplay", enable_open_position => "enableOpenPosition", enable_trade => "enableTrade", funding_impact_margin_notional => "fundingImpactMarginNotional", funding_interest_rate => "fundingInterestRate", incremental_initial_margin_rate => "incrementalInitialMarginRate", incremental_maintenance_margin_rate => "incrementalMaintenanceMarginRate", incremental_position_value => "incrementalPositionValue", initial_margin_rate => "initialMarginRate", maintenance_margin_rate => "maintenanceMarginRate", max_order_size => "maxOrderSize", max_position_size => "maxPositionSize", min_order_size => "minOrderSize", max_market_price_range => "maxMarketPriceRange", settle_currency_id => "settleCurrencyId", stark_ex_oracle_price_quorum => "starkExOraclePriceQuorum", stark_ex_resolution => "starkExResolution", stark_ex_risk_factor => "starkExRiskFactor", stark_ex_synthetic_asset_id => "starkExSyntheticAssetId", step_size => "stepSize", symbol => "symbol", symbol_display_name => "symbolDisplayName", symbol_display_name2 => "symbolDisplayName2", tick_size => "tickSize", underlying_currency_id => "underlyingCurrencyId", max_maintenance_margin_rate => "maxMaintenanceMarginRate", max_position_value => "maxPositionValue"); }).is_err() { failures.push("SymbolInfoObject"); }
    if std::panic::catch_unwind(|| { fields!(UserObject, ethereum_address => "ethereumAddress", is_registered => "isRegistered", email => "email", username => "username", referred_by_affiliate_link => "referredByAffiliateLink", affiliate_link => "affiliateLink", maker_volume30_d => "makerVolume30D", taker_volume30_d => "takerVolume30D", fees30_d => "fees30D", user_data => "userData", apex_token_balance => "apexTokenBalance", staked_apex_token_balance => "stakedApexTokenBalance", is_email_verified => "isEmailVerified", is_sharing_username => "isSharingUsername", is_sharing_address => "isSharingAddress", country => "country", id => "id", avatar_url => "avatarUrl", email_notify_general_enable => "emailNotifyGeneralEnable", email_notify_trading_enable => "emailNotifyTradingEnable", email_notify_account_enable => "emailNotifyAccountEnable"); }).is_err() { failures.push("UserObject"); }
    if std::panic::catch_unwind(|| { fields!(OrderObject, id => "id", client_id => "clientId", client_order_id => "clientOrderId", account_id => "accountId", symbol => "symbol", side => "side", price => "price", limit_fee => "limitFee", fee => "fee", liquidate_fee => "liquidateFee", size => "size", r#type => "type", direction => "direction", created_at => "createdAt", updated_time => "updatedTime", status => "status", time_in_force => "timeInForce", match_fill_id => "matchFillId", reduce_only => "reduceOnly", is_position_tpsl => "isPositionTpsl", order_id => "orderId", order_type => "orderType", exit_type => "exitType", latest_match_fill_price => "latestMatchFillPrice", cum_match_fill_size => "cumMatchFillSize", cum_match_fill_value => "cumMatchFillValue", cum_match_fill_fee => "cumMatchFillFee", cum_success_fill_size => "cumSuccessFillSize", cum_success_fill_value => "cumSuccessFillValue", cum_success_fill_fee => "cumSuccessFillFee", trigger_price_type => "triggerPriceType", is_open_tpsl_order => "isOpenTpslOrder", is_set_open_tp => "isSetOpenTp", is_set_open_sl => "isSetOpenSl", open_tp_param => "openTpParam", open_sl_param => "openSlParam"); }).is_err() { failures.push("OrderObject"); }
    assert!(
        failures.is_empty(),
        "config native row failures: {failures:?}"
    );
}

#[test]
fn opaque_fields_preserve_numeric_ingress_bits() {
    use safe_cex_num::numeric_ingress::{JsNumberOutput, js_value_as_f64};
    fn opaque<T: DeserializeOwned + Serialize + JsNumberOutput>(
        name: &str,
        key: &str,
        chains: bool,
        read: impl Fn(&T) -> &Value,
    ) {
        let reference = oracle();
        assert_eq!(
            reference[name]["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["name"] == key)
                .unwrap()["type"]["kind"],
            if chains { "array" } else { "any" },
            "{name}.{key}: actual TS opaque type"
        );
        let mut numeric_comparisons = 0;
        for case in reference["$numbers"].as_array().unwrap() {
            numeric_comparisons += 1;
            let expected = u64::from_str_radix(case["bits"].as_str().unwrap(), 16).unwrap();
            let number: Value = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
            let mut input = sample(&reference[name], 1);
            let payload = json!({"n": number, "nested": [null, true, "payload"]});
            input[key] = if chains { json!([[payload]]) } else { payload };
            let raw = serde_json::to_string(&input).unwrap();
            for actual in [
                serde_json::from_str::<T>(&raw).unwrap(),
                serde_json::from_value::<T>(input.clone()).unwrap(),
                T::deserialize(&input).unwrap(),
                safe_cex_num::numeric_ingress::js_from_str::<T>(&raw).unwrap(),
                safe_cex_num::numeric_ingress::js_from_value::<T>(input.clone()).unwrap(),
                safe_cex_num::numeric_ingress::js_from_value_ref::<T>(&input).unwrap(),
            ] {
                let value = read(&actual);
                let value = if chains {
                    &value[0][0]["n"]
                } else {
                    &value["n"]
                };
                assert_eq!(
                    js_value_as_f64(value).unwrap().to_bits(),
                    expected,
                    "{name}.{key} opaque native number bits for {}",
                    case["raw"]
                );
                assert_eq!(
                    wire_json(&actual),
                    safe_cex_num::js_json_stringify(&input.clone().into())
                        .unwrap()
                        .unwrap(),
                    "{name}.{key} opaque JS wire projection"
                );
            }
        }
        assert_eq!(
            numeric_comparisons, 14,
            "APEX-CARDINALITY native numeric comparisons"
        );
    }
    opaque::<MultiChainObject>("MultiChainObject", "chains", true, |v| &v.chains);
    opaque::<UserObject>("UserObject", "userData", false, |v| {
        v.user_data.as_ref().unwrap()
    });
    opaque::<OrderObject>("OrderObject", "openTpParam", false, |v| &v.open_tp_param);
    opaque::<OrderObject>("OrderObject", "openSlParam", false, |v| &v.open_sl_param);
}

#[test]
fn changed_typescript_config_fails_named_assertion() {
    for (mutation, expected_row) in [
        ("config-inherited", "SymbolInfoObject"),
        ("config-any", "UserObject"),
        ("config-chains", "MultiChainObject"),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "config_records_match_live_typescript",
                "--nocapture",
            ])
            .env("APEXOMNI_MUTATE_TS", mutation)
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{text}");
        assert!(
            text.contains("test result: FAILED")
                && text.contains(&format!("ApexOmni record schema {expected_row}:")),
            "{mutation} must fail its specific row {expected_row}: {text}"
        );
    }
}

#[test]
fn config_mutation_axes_match_live_typescript() {
    let reference = oracle();
    let mut failures = Vec::new();
    macro_rules! axis {
        ($ty:ident, $label:literal, $read:expr, $expected:expr) => {{
            let input = sample(&reference[stringify!($ty)], 1);
            let actual: $ty = serde_json::from_value(input.clone()).unwrap();
            let read: fn(&$ty) -> Value = $read;
            let expected: fn(&Value) -> &Value = $expected;
            if std::panic::catch_unwind(|| assert_eq!(read(&actual), *expected(&input), $label))
                .is_err()
            {
                failures.push($label);
            }
        }};
    }
    axis!(
        MultiChainObject,
        "MultiChainObject.currency independent native axis",
        |v| json!(v.currency),
        |v| &v["currency"]
    );
    axis!(
        SymbolObject,
        "SymbolObject.global.feeAccountId independent native axis",
        |v| json!(v.global.fee_account_id),
        |v| &v["global"]["feeAccountId"]
    );
    axis!(
        PerpetualObject,
        "PerpetualObject.usdcConfig.currency.id independent native axis",
        |v| json!(v.usdc_config.currency[0].id),
        |v| &v["usdcConfig"]["currency"][0]["id"]
    );
    axis!(
        SymbolInfoObject,
        "SymbolInfoObject.rankIdx independent native axis",
        |v| json!(v.rank_idx),
        |v| &v["rankIdx"]
    );
    axis!(
        UserObject,
        "UserObject.ethereumAddress independent native axis",
        |v| json!(v.ethereum_address),
        |v| &v["ethereumAddress"]
    );
    axis!(
        OrderObject,
        "OrderObject.id independent native axis",
        |v| json!(v.id),
        |v| &v["id"]
    );
    assert!(
        failures.is_empty(),
        "config independent native axes: {failures:?}"
    );
}

#[test]
fn inherited_control_rejects_blind_symbol_info_census() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "changed_typescript_config_fails_named_assertion",
            "--nocapture",
        ])
        .env("APEXOMNI_BLIND_SYMBOL_INFO_CONTROL", "1")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("test result: FAILED")
            && text.contains("config-inherited must fail its specific row SymbolInfoObject"),
        "{text}"
    );
}

#[test]
fn order_records_match_live_typescript() {
    let schemas = oracle();
    let mut failures = Vec::new();
    if std::panic::catch_unwind(|| {
        check::<WithPrice>(
            "WithPrice",
            &schemas["WithPrice"],
            &schemas["$numbers"],
            |v| WithPrice::deserialize(v).is_ok(),
        )
    })
    .is_err()
    {
        failures.push("WithPrice");
    }
    if std::panic::catch_unwind(|| {
        check::<OrderWithClientId>(
            "OrderWithClientId",
            &schemas["OrderWithClientId"],
            &schemas["$numbers"],
            |v| OrderWithClientId::deserialize(v).is_ok(),
        )
    })
    .is_err()
    {
        failures.push("OrderWithClientId");
    }
    if std::panic::catch_unwind(|| {
        check::<CreateOrderOptionsObject>(
            "CreateOrderOptionsObject",
            &schemas["CreateOrderOptionsObject"],
            &schemas["$numbers"],
            |v| CreateOrderOptionsObject::deserialize(v).is_ok(),
        )
    })
    .is_err()
    {
        failures.push("CreateOrderOptionsObject");
    }
    if std::panic::catch_unwind(|| {
        check::<OrderResponseObject>(
            "OrderResponseObject",
            &schemas["OrderResponseObject"],
            &schemas["$numbers"],
            |v| OrderResponseObject::deserialize(v).is_ok(),
        )
    })
    .is_err()
    {
        failures.push("OrderResponseObject");
    }
    assert!(
        failures.is_empty(),
        "order record row failures: {failures:?}"
    );
}

#[test]
fn order_decoded_fields_match_typescript() {
    let reference = oracle();
    macro_rules! fields {
        ($ty:ident, $( $field:ident => $wire:literal ),+ $(,)?) => {{
            let mut native_names = [$($wire),+];
            native_names.sort_unstable();
            let mut source_names: Vec<_> = reference[stringify!($ty)]["fields"].as_array().unwrap()
                .iter().map(|field| field["name"].as_str().unwrap()).collect();
            source_names.sort_unstable();
            assert_eq!(source_names, native_names, "{} native field inventory", stringify!($ty));
            let input = sample(&reference[stringify!($ty)], 1);
            let mut cases = vec![input.clone()];
            for field in reference[stringify!($ty)]["fields"].as_array().unwrap() {
                if field["type"]["kind"] == "boolean" {
                    let mut one_false = input.clone();
                    one_false[field["name"].as_str().unwrap()] = json!(false);
                    cases.push(one_false);
                }
            }
            for input in cases {
                let actual: $ty = serde_json::from_value(input.clone()).unwrap();
                $(assert_eq!(serde_json::to_value(&actual.$field).unwrap(), input[$wire],
                    "{}.{} decoded native field", stringify!($ty), $wire);)+
            }
        }};
    }
    let mut failures = Vec::new();
    if std::panic::catch_unwind(|| {
        fields!(WithPrice, human_price => "humanPrice", human_quote_amount => "humanQuoteAmount");
    })
    .is_err()
    {
        failures.push("WithPrice");
    }
    if std::panic::catch_unwind(|| { fields!(OrderWithClientId, position_id => "positionId", human_size => "humanSize", limit_fee => "limitFee", symbol => "symbol", side => "side", expiration_iso_timestamp => "expirationIsoTimestamp", human_price => "humanPrice", human_quote_amount => "humanQuoteAmount", client_id => "clientId", nonce => "nonce"); }).is_err() { failures.push("OrderWithClientId"); }
    if std::panic::catch_unwind(|| { fields!(CreateOrderOptionsObject, client_order_id => "clientOrderId", position_id => "positionId", symbol => "symbol", side => "side", r#type => "type", size => "size", price => "price", limit_fee => "limitFee", time_in_force => "timeInForce", trigger_price => "triggerPrice", trailing_percent => "trailingPercent", reduce_only => "reduceOnly", broker_id => "brokerId"); }).is_err() { failures.push("CreateOrderOptionsObject"); }
    if std::panic::catch_unwind(|| { fields!(OrderResponseObject, id => "id", client_id => "clientId", account_id => "accountId", symbol => "symbol", side => "side", price => "price", trigger_price => "triggerPrice", trailing_percent => "trailingPercent", size => "size", remaining_size => "remainingSize", r#type => "type", created_at => "createdAt", unfillable_at => "unfillableAt", expires_at => "expiresAt", status => "status", time_in_force => "timeInForce", post_only => "postOnly", cancel_reason => "cancelReason"); }).is_err() { failures.push("OrderResponseObject"); }
    assert!(
        failures.is_empty(),
        "order native row failures: {failures:?}"
    );
}

#[test]
fn iso8601_accepted_contract_observes_the_real_timestamp_decoder() {
    let reference = oracle();
    let mut observed = Vec::new();
    for raw in [
        "\"2026-10-18T12:34:56.789Z\"",
        "\"not-a-date\"",
        "\"\"",
        "\"é-😀\"",
        "1700000000000",
        "-0",
        "1.25",
        "null",
        "true",
        "[]",
        "{}",
        "{\"String\":\"time\"}",
        "not-json",
    ] {
        let actual = match serde_json::from_str::<Iso8601>(raw) {
            Ok(Iso8601::String(value)) => {
                json!({"ok":true,"variant":"String","value":value})
            }
            Ok(Iso8601::Number(value)) => {
                json!({"ok":true,"variant":"Number","value":format!("{:016x}",value.to_bits())})
            }
            Err(_) => json!({"ok":false}),
        };
        let source_accepts = serde_json::from_str::<Value>(raw)
            .is_ok_and(|value| schema_accepts(&reference["ISO8601"], &value));
        assert_eq!(actual["ok"], source_accepts, "TS timestamp domain {raw}");
        observed.push((raw, actual));
    }
    let contracts: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../safe-cex-parity/accepted-set/contracts/safe-cex-venue-apexomni.json"
    )))
    .unwrap();
    let contract = &contracts["safe_cex_venue_apexomni::order_records::Iso8601"];
    assert_eq!(
        contract["nativeObservation"]["nativeFields"],
        json!(["variant", "value"]),
        "TimestampVisitor needs direct native timestamp observations"
    );
    let cases = contract["nativeObservation"]["cases"].as_array().unwrap();
    assert_eq!(cases.len(), observed.len());
    for (raw, actual) in observed {
        let expected = cases.iter().find(|case| case["input"] == raw).unwrap();
        assert_eq!(
            actual, expected["output"],
            "native timestamp contract {raw}"
        );
    }
}

#[test]
fn iso8601_preserves_live_typescript_union_and_numeric_bits() {
    let reference = oracle();
    assert_eq!(
        reference["ISO8601"],
        json!({"kind":"union","choices":[{"kind":"string"},{"kind":"number"}]}),
        "ISO8601 actual TypeScript union"
    );
    for input in [
        json!("2026-10-13T00:00:00Z"),
        json!(""),
        json!("not-a-date"),
        json!("é-😀"),
    ] {
        let value: Iso8601 = serde_json::from_value(input.clone()).unwrap();
        assert!(
            matches!(&value, Iso8601::String(s) if s == input.as_str().unwrap()),
            "ISO8601 decoded string variant"
        );
        assert_eq!(serde_json::to_value(value).unwrap(), input);
    }
    let mut numeric_comparisons = 0;
    for case in reference["$numbers"].as_array().unwrap() {
        numeric_comparisons += 1;
        let raw = case["raw"].as_str().unwrap();
        let expected = u64::from_str_radix(case["bits"].as_str().unwrap(), 16).unwrap();
        let input: Value = serde_json::from_str(raw).unwrap();
        for value in [
            serde_json::from_str::<Iso8601>(raw).unwrap(),
            serde_json::from_value::<Iso8601>(input.clone()).unwrap(),
            Iso8601::deserialize(&input).unwrap(),
            safe_cex_num::numeric_ingress::js_from_str::<Iso8601>(raw).unwrap(),
            safe_cex_num::numeric_ingress::js_from_value::<Iso8601>(input.clone()).unwrap(),
            safe_cex_num::numeric_ingress::js_from_value_ref::<Iso8601>(&input).unwrap(),
        ] {
            let Iso8601::Number(number) = value else {
                panic!("ISO8601 decoded numeric variant");
            };
            assert_eq!(
                number.to_bits(),
                expected,
                "ISO8601 decoded numeric bits {raw}"
            );
            assert_eq!(
                wire_json(&value),
                case["numberJson"].as_str().unwrap(),
                "ISO8601 JS wire number"
            );
        }
    }
    assert_eq!(
        numeric_comparisons, 14,
        "APEX-CARDINALITY native numeric comparisons"
    );
    for bad in [
        Value::Null,
        json!(true),
        json!([]),
        json!({"Number":1}),
        json!({"String":"time"}),
    ] {
        assert!(
            serde_json::from_value::<Iso8601>(bad.clone()).is_err(),
            "ISO8601 rejects non-union shape"
        );
        assert!(
            Iso8601::deserialize(bad).is_err(),
            "ISO8601 concrete rejects non-union shape"
        );
    }
}

#[test]
fn order_row_controls_absence_null_and_undefined() {
    let schemas = oracle();
    let mut price: WithPrice = serde_json::from_value(sample(&schemas["WithPrice"], 1)).unwrap();
    assert!(
        price.human_quote_amount.is_none(),
        "WithPrice absent undefined field"
    );
    assert_eq!(
        schemas["$undefined"][1]["ownPrice"], true,
        "actual JS native undefined is present"
    );
    assert_eq!(
        schemas["$undefined"][1]["json"], "{}",
        "actual JS native undefined is omitted on wire"
    );
    price.human_quote_amount = Some(());
    assert!(
        serde_json::to_value(price)
            .unwrap()
            .get("humanQuoteAmount")
            .is_none(),
        "WithPrice present undefined property omitted"
    );
    let mut client: OrderWithClientId =
        serde_json::from_value(sample(&schemas["OrderWithClientId"], 1)).unwrap();
    assert!(
        client.human_quote_amount.is_none() && client.nonce.is_none(),
        "OrderWithClientId absent undefined fields"
    );
    client.human_quote_amount = Some(());
    client.nonce = Some(());
    let encoded = serde_json::to_value(client).unwrap();
    assert!(
        encoded.get("humanQuoteAmount").is_none() && encoded.get("nonce").is_none(),
        "OrderWithClientId present undefined fields omitted from JSON"
    );
    let input = sample(&schemas["OrderResponseObject"], 1);
    for key in [
        "triggerPrice",
        "trailingPercent",
        "unfillableAt",
        "cancelReason",
    ] {
        let mut absent = input.clone();
        absent.as_object_mut().unwrap().remove(key);
        let absent: OrderResponseObject = serde_json::from_value(absent).unwrap();
        assert!(
            match key {
                "triggerPrice" => absent.trigger_price.is_none(),
                "trailingPercent" => absent.trailing_percent.is_none(),
                "unfillableAt" => absent.unfillable_at.is_none(),
                "cancelReason" => absent.cancel_reason.is_none(),
                _ => unreachable!(),
            },
            "OrderResponseObject.{key} native absence"
        );
        let mut present = input.clone();
        present[key] = Value::Null;
        let present: OrderResponseObject = serde_json::from_value(present).unwrap();
        assert!(
            match key {
                "triggerPrice" => present.trigger_price == Some(None),
                "trailingPercent" => present.trailing_percent == Some(None),
                "unfillableAt" => present.unfillable_at == Some(None),
                "cancelReason" => present.cancel_reason == Some(None),
                _ => unreachable!(),
            },
            "OrderResponseObject.{key} native present null"
        );
    }
}

#[test]
fn response_timestamps_preserve_numeric_bits() {
    let reference = oracle();
    for key in ["createdAt", "unfillableAt", "expiresAt"] {
        let mut numeric_comparisons = 0;
        for case in reference["$numbers"].as_array().unwrap() {
            numeric_comparisons += 1;
            let mut input = sample(&reference["OrderResponseObject"], 1);
            input[key] = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
            let raw = serde_json::to_string(&input).unwrap();
            let expected = u64::from_str_radix(case["bits"].as_str().unwrap(), 16).unwrap();
            for actual in [
                serde_json::from_str::<OrderResponseObject>(&raw).unwrap(),
                serde_json::from_value::<OrderResponseObject>(input.clone()).unwrap(),
                OrderResponseObject::deserialize(&input).unwrap(),
                safe_cex_num::numeric_ingress::js_from_str::<OrderResponseObject>(&raw).unwrap(),
                safe_cex_num::numeric_ingress::js_from_value::<OrderResponseObject>(input.clone())
                    .unwrap(),
                safe_cex_num::numeric_ingress::js_from_value_ref::<OrderResponseObject>(&input)
                    .unwrap(),
            ] {
                let time = match key {
                    "createdAt" => &actual.created_at,
                    "unfillableAt" => actual.unfillable_at.as_ref().unwrap().as_ref().unwrap(),
                    "expiresAt" => actual.expires_at.as_ref().unwrap(),
                    _ => unreachable!(),
                };
                let Iso8601::Number(value) = time else {
                    panic!("OrderResponseObject.{key} numeric variant");
                };
                assert_eq!(
                    value.to_bits(),
                    expected,
                    "OrderResponseObject.{key} decoded numeric bits"
                );
                assert_eq!(
                    wire_json(&actual),
                    safe_cex_num::js_json_stringify(&input.clone().into())
                        .unwrap()
                        .unwrap(),
                    "OrderResponseObject.{key} JS wire number"
                );
            }
        }
        assert_eq!(
            numeric_comparisons, 14,
            "APEX-CARDINALITY native numeric comparisons"
        );
    }
}

#[test]
fn create_order_variants_match_live_typescript() {
    let reference = oracle();
    let fields = reference["CreateOrderOptionsObject"]["fields"]
        .as_array()
        .unwrap();
    let kinds = fields.iter().find(|f| f["name"] == "type").unwrap()["type"]["choices"]
        .as_array()
        .unwrap();
    let expected = [
        CreateOrderKind::Limit,
        CreateOrderKind::Market,
        CreateOrderKind::StopLimit,
        CreateOrderKind::StopMarket,
        CreateOrderKind::TakeProfitLimit,
        CreateOrderKind::TakeProfitMarket,
    ];
    assert_eq!(
        kinds.len(),
        expected.len(),
        "CreateOrderOptionsObject type member census"
    );
    for (wire, native) in [
        ("LIMIT", CreateOrderKind::Limit),
        ("MARKET", CreateOrderKind::Market),
        ("STOP_LIMIT", CreateOrderKind::StopLimit),
        ("STOP_MARKET", CreateOrderKind::StopMarket),
        ("TAKE_PROFIT_LIMIT", CreateOrderKind::TakeProfitLimit),
        ("TAKE_PROFIT_MARKET", CreateOrderKind::TakeProfitMarket),
    ] {
        assert!(
            kinds.iter().any(|v| v["value"] == wire),
            "actual TS create order kind {wire}"
        );
        assert_eq!(
            serde_json::from_value::<CreateOrderKind>(json!(wire)).unwrap(),
            native,
            "CreateOrderOptionsObject.type decoded native variant {wire}"
        );
    }
    let policies = fields.iter().find(|f| f["name"] == "timeInForce").unwrap()["type"]["choices"]
        .as_array()
        .unwrap();
    assert_eq!(
        policies.len(),
        4,
        "CreateOrderOptionsObject timeInForce member census"
    );
    for (wire, native) in [
        ("GOOD_TIL_CANCEL", CreateTimeInForce::GoodTilCancel),
        ("FILL_OR_KILL", CreateTimeInForce::FillOrKill),
        ("IMMEDIATE_OR_CANCEL", CreateTimeInForce::ImmediateOrCancel),
        ("POST_ONLY", CreateTimeInForce::PostOnly),
    ] {
        assert!(
            policies.iter().any(|v| v["value"] == wire),
            "actual TS create time policy {wire}"
        );
        assert_eq!(
            serde_json::from_value::<CreateTimeInForce>(json!(wire)).unwrap(),
            native,
            "CreateOrderOptionsObject.timeInForce decoded native variant {wire}"
        );
    }
}

#[test]
fn order_row_controls_native_axes() {
    let reference = oracle();
    let mut failures = Vec::new();
    macro_rules! axis {
        ($ty:ident, $label:literal, $field:ident, $wire:literal) => {{
            let input = sample(&reference[stringify!($ty)], 1);
            let actual: $ty = serde_json::from_value(input.clone()).unwrap();
            if std::panic::catch_unwind(|| {
                assert_eq!(
                    serde_json::to_value(&actual.$field).unwrap(),
                    input[$wire],
                    $label
                )
            })
            .is_err()
            {
                failures.push($label);
            }
        }};
    }
    let input = sample(&reference["ISO8601"], 1);
    let actual: Iso8601 = serde_json::from_value(input.clone()).unwrap();
    if std::panic::catch_unwind(|| {
        let Iso8601::String(value) = actual else {
            panic!("ISO8601 independent native axis");
        };
        assert_eq!(json!(value), input, "ISO8601 independent native axis");
    })
    .is_err()
    {
        failures.push("ISO8601 independent native axis");
    }
    axis!(
        WithPrice,
        "WithPrice.humanPrice independent native axis",
        human_price,
        "humanPrice"
    );
    axis!(
        OrderWithClientId,
        "OrderWithClientId.positionId independent native axis",
        position_id,
        "positionId"
    );
    axis!(
        CreateOrderOptionsObject,
        "CreateOrderOptionsObject.clientOrderId independent native axis",
        client_order_id,
        "clientOrderId"
    );
    axis!(
        OrderResponseObject,
        "OrderResponseObject.id independent native axis",
        id,
        "id"
    );
    assert!(
        failures.is_empty(),
        "order independent native axes: {failures:?}"
    );
}

struct NativeNumberEvent(f64);
impl<'de> serde::Deserializer<'de> for NativeNumberEvent {
    type Error = serde_json::Error;
    fn deserialize_any<V: serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_f64(self.0)
    }
    fn deserialize_newtype_struct<V: serde::de::Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_newtype_struct(self)
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf option unit unit_struct seq tuple tuple_struct map struct enum identifier ignored_any
    }
}

#[test]
fn timestamp_native_numbers_canonicalize_nan_only() {
    let reference = oracle();
    assert!(
        !reference["$runtime"]["arch"].as_str().unwrap().is_empty(),
        "Node witness architecture is recorded"
    );
    assert_eq!(
        reference["$runtime"]["rustArch"],
        std::env::consts::ARCH,
        "Rust witness architecture is recorded"
    );
    assert!(
        reference["$runtime"]["nodeMajor"].as_u64().unwrap() > 0,
        "Node major is recorded"
    );
    let mut numeric_comparisons = 0;
    for case in reference["$nativeNumbers"].as_array().unwrap() {
        numeric_comparisons += 1;
        let input = match case["tag"].as_str().unwrap() {
            "NaN" => f64::NAN,
            "negative NaN" => -f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            "-0" => -0.0,
            "0" => 0.0,
            "fraction" => 1.25,
            _ => unreachable!(),
        };
        assert_eq!(case["type"], "number", "actual JS typeof before stringify");
        let value = Iso8601::deserialize(NativeNumberEvent(input)).unwrap();
        let Iso8601::Number(number) = value else {
            panic!("ISO8601 native numeric variant");
        };
        let bits = if number.is_nan() {
            0x7ff8_0000_0000_0000
        } else {
            number.to_bits()
        };
        assert_eq!(
            format!("{bits:016x}"),
            case["bits"].as_str().unwrap(),
            "ISO8601 native bits canonicalize NaN only"
        );
        assert_eq!(
            wire_json(&value),
            case["numberJson"].as_str().unwrap(),
            "ISO8601 native number JSON spelling"
        );
    }
    assert_eq!(
        numeric_comparisons, 7,
        "APEX-CARDINALITY native numeric comparisons"
    );
    assert_eq!(
        reference["$undefined"],
        json!([
            {"ownPrice":false,"ownNonce":false,"priceType":"undefined","nonceType":"undefined","json":"{}"},
            {"ownPrice":true,"ownNonce":true,"priceType":"undefined","nonceType":"undefined","json":"{}"},
        ]),
        "actual undefined presence and wire omission"
    );
}

#[test]
fn changed_typescript_order_rows_fail_named_assertions() {
    for (mutation, test, label) in [
        (
            "order-iso",
            "iso8601_preserves_live_typescript_union_and_numeric_bits",
            "ISO8601 actual TypeScript union",
        ),
        (
            "order-price",
            "order_records_match_live_typescript",
            "ApexOmni record schema WithPrice:",
        ),
        (
            "order-client",
            "order_records_match_live_typescript",
            "ApexOmni record schema OrderWithClientId:",
        ),
        (
            "order-create",
            "order_records_match_live_typescript",
            "ApexOmni record schema CreateOrderOptionsObject:",
        ),
        (
            "order-response",
            "order_records_match_live_typescript",
            "ApexOmni record schema OrderResponseObject:",
        ),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--nocapture"])
            .env("APEXOMNI_MUTATE_TS", mutation)
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{mutation}: {text}");
        assert!(
            text.contains("test result: FAILED") && text.contains(label),
            "{mutation} must fail {label}: {text}"
        );
    }
}

fn numeric_corpus_cardinality(corpus: &Value) {
    for (key, count) in [("$numbers", 14), ("$nativeNumbers", 7), ("$undefined", 2)] {
        assert_eq!(
            corpus[key].as_array().unwrap().len(),
            count,
            "APEX-CARDINALITY source {key} total"
        );
    }
}
#[test]
fn numeric_source_cardinality_rejects_empty_and_subset() {
    for key in ["$numbers", "$nativeNumbers", "$undefined"] {
        let original = oracle();
        for keep in [0, original[key].as_array().unwrap().len() - 1] {
            let mut shortened = original.clone();
            shortened[key].as_array_mut().unwrap().truncate(keep);
            let error = std::panic::catch_unwind(|| numeric_corpus_cardinality(&shortened))
                .expect_err("source omission must fail");
            assert!(
                error
                    .downcast_ref::<String>()
                    .unwrap()
                    .contains(&format!("APEX-CARDINALITY source {key} total"))
            );
        }
    }
}

// A supplement: original oracle values and scripts remain byte-identical.
fn collection_source_count(expected: &Value, name: &str) {
    assert_eq!(
        expected.as_array().unwrap().len(),
        2,
        "APEX-CARDINALITY {name} source per-case count"
    );
}
fn collection_native_count(expected: &Value, native_len: usize, name: &str) {
    assert_eq!(
        native_len,
        expected.as_array().unwrap().len(),
        "APEX-CARDINALITY {name} expected/native length"
    );
    assert_eq!(
        native_len, 2,
        "APEX-CARDINALITY {name} native comparison total"
    );
}
fn collection_values(expected: &Value, actual: &Value, native_len: usize, name: &str) {
    collection_source_count(expected, name);
    collection_native_count(expected, native_len, name);
    if expected[0].is_array() {
        let mut comparisons = 0;
        for i in 0..2 {
            let inner = format!("{name}[{i}]");
            collection_source_count(&expected[i], &inner);
            collection_native_count(&expected[i], actual[i].as_array().unwrap().len(), &inner);
            comparisons += 1;
        }
        assert_eq!(comparisons, 2, "APEX-CARDINALITY inner comparisons");
    }
    assert_eq!(
        actual, expected,
        "APEX-CARDINALITY {name} whole native value"
    );
}
#[test]
fn collection_cardinality_native_supplement() {
    let reference = oracle();
    let mut attempts = 0;
    let mut failures = Vec::new();
    macro_rules! axis {
        ($ty:ident, $wire:literal, $field:ident, $projection:expr, $opaque:expr) => {{
            attempts += 1;
            let name = concat!(stringify!($ty), ".", $wire);
            if std::panic::catch_unwind(|| {
                let mut input = sample(&reference[stringify!($ty)], 1);
                if $opaque {
                    input[$wire] = json!([["first", 1.25], ["second", 9.5]]);
                }
                let native: $ty = serde_json::from_value(input.clone()).unwrap();
                let (length, value): (usize, Value) = ($projection)(&native.$field);
                collection_values(&input[$wire], &value, length, name);
            })
            .is_err()
            {
                failures.push(name);
            }
        }};
    }
    macro_rules! vector {
        ($ty:ident, $wire:literal, $field:ident) => {
            axis!(
                $ty,
                $wire,
                $field,
                |v: &Vec<_>| (v.len(), serde_json::to_value(v).unwrap()),
                false
            );
        };
    }
    vector!(DepthObject, "a", a);
    vector!(DepthObject, "b", b);
    vector!(PerpetualCurrencyObject, "usdc", usdc);
    vector!(PerpetualCurrencyObject, "usdt", usdt);
    vector!(AccountObject, "wallets", wallets);
    vector!(AccountObject, "openPositions", open_positions);
    axis!(
        AccountObject,
        "accounts",
        accounts,
        |v: &Option<Vec<AccountsItem>>| {
            let v = v.as_ref().unwrap();
            (v.len(), serde_json::to_value(v).unwrap())
        },
        false
    );
    vector!(SymbolObject, "currency", currency);
    vector!(SymbolObject, "perpetualContract", perpetual_contract);
    axis!(
        MultiChainObject,
        "chains",
        chains,
        |v: &Value| (v.as_array().unwrap().len(), v.clone()),
        false
    );
    axis!(
        UserObject,
        "userData",
        user_data,
        |v: &Option<Value>| {
            let v = v.as_ref().unwrap();
            (v.as_array().unwrap().len(), v.clone())
        },
        true
    );
    axis!(
        OrderObject,
        "openTpParam",
        open_tp_param,
        |v: &Value| (v.as_array().unwrap().len(), v.clone()),
        true
    );
    axis!(
        OrderObject,
        "openSlParam",
        open_sl_param,
        |v: &Value| (v.as_array().unwrap().len(), v.clone()),
        true
    );
    assert_eq!(attempts, 13, "APEX-CARDINALITY collection axis total");
    assert!(
        failures.is_empty(),
        "APEX-CARDINALITY collection failures: {failures:?}"
    );
}
#[test]
fn collection_cardinality_guards_are_independent() {
    let input = sample(&oracle()["DepthObject"], 1);
    let native: DepthObject = serde_json::from_value(input.clone()).unwrap();
    for keep in [0, 1] {
        let mut omitted = input["a"].clone();
        omitted.as_array_mut().unwrap().truncate(keep);
        for (result, label) in [
            (
                std::panic::catch_unwind(|| collection_source_count(&omitted, "DepthObject.a")),
                "source per-case count",
            ),
            // Deliberately bypass the source guard; retain the real native output.
            (
                std::panic::catch_unwind(|| {
                    collection_native_count(&omitted, native.a.len(), "DepthObject.a")
                }),
                "expected/native length",
            ),
        ] {
            let error = result.expect_err("omission must fail each layer independently");
            assert!(error.downcast_ref::<String>().unwrap().contains(label));
        }
    }
}

#[path = "support/signer_records.rs"]
mod signer_records;

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
