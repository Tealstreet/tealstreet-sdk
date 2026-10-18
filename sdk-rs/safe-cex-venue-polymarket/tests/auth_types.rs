use safe_cex_num::numeric_ingress::{
    JsNumberInput, JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_venue_polymarket::types::{
    PolymarketBalanceAllowance, PolymarketClobMarket, PolymarketCredentials,
    PolymarketDeriveKeyRequest, PolymarketDeriveKeyResponse, PolymarketL2Headers,
    PolymarketOrderBook, PolymarketOrderBookLevel, PolymarketOrderResponse, PolymarketPrice,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};

fn oracle() -> &'static Value {
    // Cache errors too: a failing mutation must not launch another Node process
    // when another test asks for the same corpus.
    static CORPUS: OnceLock<Result<Value, String>> = OnceLock::new();
    CORPUS
        .get_or_init(|| {
            eprintln!("polymarket auth oracle: one Node invocation for the entire corpus");
            let output = Command::new("node")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/auth_types_oracle.cjs"
                ))
                .output()
                .map_err(|error| {
                    format!("Node is required for actual TypeScript witnesses: {error}")
                })?;
            if !output.status.success() {
                return Err(format!(
                    "actual_typescript_oracle: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())
        })
        .as_ref()
        .unwrap_or_else(|error| panic!("{error}"))
}

fn shape<T: DeserializeOwned + Serialize + JsNumberOutput>(
    name: &str,
    sample: &Value,
    schema: &Value,
) {
    let fields = schema[name].as_array().unwrap();
    for field in fields {
        let key = field[0].as_str().unwrap();
        let optional = field[1].as_bool().unwrap();
        let mut absent = sample.clone();
        absent.as_object_mut().unwrap().remove(key);
        assert_eq!(
            js_from_value::<T>(absent).is_ok(),
            optional,
            "required_field:{name}.{key}"
        );
        let mut null = sample.clone();
        null[key] = Value::Null;
        assert!(
            js_from_value::<T>(null).is_err(),
            "present_null:{name}.{key}"
        );
        let mut wrong = sample.clone();
        wrong[key] = if field[2] == "boolean" {
            json!("wrong")
        } else {
            json!(false)
        };
        assert!(
            js_from_value::<T>(wrong).is_err(),
            "wrong_field_type:{name}.{key}"
        );
    }
    let positional = Value::Array(
        fields
            .iter()
            .map(|field| sample[field[0].as_str().unwrap()].clone())
            .collect(),
    );
    assert!(
        js_from_value::<T>(positional).is_err(),
        "positional_record_refusal:{name}"
    );
    assert!(
        js_from_value::<T>(Value::Null).is_err(),
        "null_record_refusal:{name}"
    );
    let decoded: T = js_from_value_ref(sample).unwrap();
    assert_eq!(
        serde_json::to_value(decoded).unwrap(),
        *sample,
        "wire_fields:{name}"
    );
}

#[test]
fn auth_records_actual_typescript() {
    let result = oracle();
    let schema = &result["schema"];
    // Hand-maintained expectations independent of the compiler's heritage-aware census.
    assert_eq!(
        schema["PolymarketCredentials"],
        json!([
            ["apiKey", false, "string"],
            ["secret", false, "string"],
            ["passphrase", false, "string"]
        ]),
        "credentials_schema"
    );
    assert_eq!(
        schema["PolymarketDeriveKeyResponse"], schema["PolymarketCredentials"],
        "response_schema"
    );
    assert_eq!(
        schema["PolymarketDeriveKeyRequest"],
        json!([
            ["address", false, "string"],
            ["nonce", false, "number"],
            ["signature", false, "string"],
            ["timestamp", false, "number"]
        ]),
        "request_schema"
    );
    assert_eq!(
        schema["PolymarketL2Headers"],
        json!([
            ["POLY_ADDRESS", false, "string"],
            ["POLY_SIGNATURE", false, "string"],
            ["POLY_TIMESTAMP", false, "string"],
            ["POLY_API_KEY", false, "string"],
            ["POLY_PASSPHRASE", true, "string"]
        ]),
        "headers_schema"
    );
    shape::<PolymarketCredentials>("PolymarketCredentials", &result["credentials"], schema);
    shape::<PolymarketDeriveKeyResponse>(
        "PolymarketDeriveKeyResponse",
        &result["response"],
        schema,
    );
    shape::<PolymarketDeriveKeyRequest>("PolymarketDeriveKeyRequest", &result["request"], schema);
    shape::<PolymarketL2Headers>("PolymarketL2Headers", &result["headers"], schema);

    let credentials: PolymarketCredentials = js_from_value_ref(&result["credentials"]).unwrap();
    assert_eq!(
        credentials.api_key, result["credentials"]["apiKey"],
        "native_credentials_api_key"
    );
    assert_eq!(
        credentials.secret, result["credentials"]["secret"],
        "native_credentials_secret"
    );
    assert_eq!(
        credentials.passphrase, result["credentials"]["passphrase"],
        "native_credentials_passphrase"
    );
    let response: PolymarketDeriveKeyResponse = js_from_value_ref(&result["response"]).unwrap();
    assert_eq!(
        response.api_key, result["response"]["apiKey"],
        "native_response_api_key"
    );
    assert_eq!(
        response.secret, result["response"]["secret"],
        "native_response_secret"
    );
    assert_eq!(
        response.passphrase, result["response"]["passphrase"],
        "native_response_passphrase"
    );
    let request: PolymarketDeriveKeyRequest = js_from_value_ref(&result["request"]).unwrap();
    assert_eq!(
        request.address, result["request"]["address"],
        "native_request_address"
    );
    assert_eq!(
        request.signature, result["request"]["signature"],
        "native_request_signature"
    );
    assert_eq!(
        request.nonce.to_bits(),
        17.25_f64.to_bits(),
        "native_request_nonce"
    );
    assert_eq!(
        request.timestamp.to_bits(),
        1700000123.5_f64.to_bits(),
        "native_request_timestamp"
    );
    let headers: PolymarketL2Headers = js_from_value_ref(&result["headers"]).unwrap();
    assert_eq!(
        headers.poly_address, result["headers"]["POLY_ADDRESS"],
        "native_headers_address"
    );
    assert_eq!(
        headers.poly_signature, result["headers"]["POLY_SIGNATURE"],
        "native_headers_signature"
    );
    assert_eq!(
        headers.poly_timestamp, result["headers"]["POLY_TIMESTAMP"],
        "native_headers_timestamp"
    );
    assert_eq!(
        headers.poly_api_key, result["headers"]["POLY_API_KEY"],
        "native_headers_api_key"
    );
    assert_eq!(
        headers.poly_passphrase.as_deref(),
        result["headers"]["POLY_PASSPHRASE"].as_str(),
        "native_headers_passphrase"
    );
    let absent: PolymarketL2Headers = js_from_value_ref(&result["optionalAbsent"]).unwrap();
    assert!(absent.poly_passphrase.is_none(), "native_headers_absence");
    assert_eq!(
        serde_json::to_value(absent).unwrap(),
        result["optionalAbsent"],
        "optional_wire_omission"
    );
}

#[test]
fn numeric_auth_fields_before_serialization() {
    let result = oracle();
    let cases = result["numeric"].as_array().unwrap();
    assert_eq!(cases.len(), 9, "auth_numeric_source_count");
    let mut comparisons = 0;
    for case in cases {
        let raw = case["raw"].as_str().unwrap();
        let value: Value = serde_json::from_str(raw).unwrap();
        let buffered: JsNumberInput = serde_json::from_str(raw).unwrap();
        let paths: [PolymarketDeriveKeyRequest; 4] = [
            js_from_str(raw).unwrap(),
            js_from_value(value.clone()).unwrap(),
            js_from_value_ref(&value).unwrap(),
            buffered.decode().unwrap(),
        ];
        for (path, decoded) in paths.iter().enumerate() {
            assert_eq!(
                decoded.address, "numeric-wallet",
                "numeric_native_address:{path}"
            );
            assert_eq!(
                decoded.signature, "numeric-signature",
                "numeric_native_signature:{path}"
            );
            assert_eq!(
                format!("{:016x}", decoded.nonce.to_bits()),
                case["nonceBits"],
                "nonce_bits_before_json:{path}:{raw}"
            );
            assert_eq!(
                format!("{:016x}", decoded.timestamp.to_bits()),
                case["timestampBits"],
                "timestamp_bits_before_json:{path}:{raw}"
            );
            comparisons += 2;
        }
    }
    assert_eq!(comparisons, 72, "auth_numeric_native_comparisons");
}

fn decoded_boundary_field(name: &str, raw: &str) -> Result<String, serde_json::Error> {
    match name {
        "PolymarketCredentials" => js_from_str::<PolymarketCredentials>(raw).map(|v| v.api_key),
        "PolymarketDeriveKeyResponse" => {
            js_from_str::<PolymarketDeriveKeyResponse>(raw).map(|v| v.api_key)
        }
        "PolymarketDeriveKeyRequest" => {
            js_from_str::<PolymarketDeriveKeyRequest>(raw).map(|v| v.address)
        }
        "PolymarketL2Headers" => js_from_str::<PolymarketL2Headers>(raw).map(|v| v.poly_address),
        _ => panic!("unregistered boundary record {name}"),
    }
}

#[test]
fn auth_string_boundary() {
    let result = oracle();
    let cases = result["stringBoundary"].as_array().unwrap();
    assert_eq!(cases.len(), 32, "string_boundary_case_count");
    assert_eq!(
        cases.iter().filter(|case| case["accepted"] == true).count(),
        20,
        "string_boundary_case_count_accepted"
    );
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let raw = case["raw"].as_str().unwrap();
        let decoded = decoded_boundary_field(name, raw);
        if case["accepted"].as_bool().unwrap() {
            let source: Value = serde_json::from_str(raw).unwrap();
            let key = match name {
                "PolymarketCredentials" | "PolymarketDeriveKeyResponse" => "apiKey",
                "PolymarketDeriveKeyRequest" => "address",
                "PolymarketL2Headers" => "POLY_ADDRESS",
                _ => unreachable!(),
            };
            assert_eq!(
                decoded.unwrap(),
                source[key],
                "native_unicode_string:{name}:{raw}"
            );
        } else {
            let retained = typed_utf16::retained(raw);
            let key = match name {
                "PolymarketCredentials" | "PolymarketDeriveKeyResponse" => "apiKey",
                "PolymarketDeriveKeyRequest" => "address",
                "PolymarketL2Headers" => "POLY_ADDRESS",
                _ => unreachable!(),
            };
            let units = retained.get(key).unwrap().as_text().unwrap();
            assert_eq!(
                decoded.unwrap(),
                String::from_utf16_lossy(units),
                "typed auth U+FFFD field"
            );
        }
    }
}

#[test]
fn wire_payload_records_actual_typescript() {
    let result = oracle();
    assert!(
        result["capture"]["architecture"]
            .as_str()
            .is_some_and(|v| !v.is_empty()),
        "capture_architecture"
    );
    assert!(
        result["capture"]["nodeMajor"]
            .as_u64()
            .is_some_and(|v| v > 0),
        "capture_node_major"
    );
    eprintln!("capture header: {}", result["capture"]);
    let schema = &result["schema"];
    assert_eq!(
        schema["PolymarketBalanceAllowance"],
        json!([["balance", false, "string"], ["allowance", false, "string"]]),
        "balance_allowance_schema"
    );
    assert_eq!(
        schema["PolymarketOrderBookLevel"],
        json!([["price", false, "string"], ["size", false, "string"]]),
        "book_level_schema"
    );
    let cases = result["payloadCases"].as_array().unwrap();
    assert_eq!(cases.len(), 36, "payload_case_count");
    assert_eq!(
        cases.iter().filter(|case| case["accepted"] == true).count(),
        28,
        "payload_case_count_accepted"
    );
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let raw = case["raw"].as_str().unwrap();
        let accepted = case["accepted"].as_bool().unwrap();
        match name {
            "PolymarketBalanceAllowance" => {
                let decoded = js_from_str::<PolymarketBalanceAllowance>(raw);
                if accepted {
                    let value: Value = serde_json::from_str(raw).unwrap();
                    let native = decoded.unwrap();
                    assert_eq!(native.balance, value["balance"], "native_balance.balance");
                    assert_eq!(
                        native.allowance, value["allowance"],
                        "native_balance.allowance"
                    );
                    shape::<PolymarketBalanceAllowance>(name, &value, schema);
                    assert_eq!(serde_json::to_value(native).unwrap(), value, "balance_wire");
                } else {
                    typed_utf16::assert_projection(
                        raw,
                        &decoded.expect("typed payload retains the record"),
                    );
                }
            }
            "PolymarketOrderBookLevel" => {
                let decoded = js_from_str::<PolymarketOrderBookLevel>(raw);
                if accepted {
                    let value: Value = serde_json::from_str(raw).unwrap();
                    let native = decoded.unwrap();
                    assert_eq!(native.price, value["price"], "native_level.price");
                    assert_eq!(native.size, value["size"], "native_level.size");
                    shape::<PolymarketOrderBookLevel>(name, &value, schema);
                    assert_eq!(serde_json::to_value(native).unwrap(), value, "level_wire");
                } else {
                    typed_utf16::assert_projection(
                        raw,
                        &decoded.expect("typed payload retains the record"),
                    );
                }
            }
            _ => panic!("unregistered payload {name}"),
        }
    }
}

#[test]
fn price_record_actual_typescript() {
    let result = oracle();
    assert_eq!(
        result["schema"]["PolymarketPrice"],
        json!([
            ["token_id", false, "string"],
            ["price", false, "string"],
            ["timestamp", true, "number"]
        ]),
        "price_schema"
    );
    let sample = &result["priceSample"];
    let native: PolymarketPrice = js_from_value_ref(sample).unwrap();
    assert_eq!(native.token_id, sample["token_id"], "native_price.token_id");
    assert_eq!(native.price, sample["price"], "native_price.price");
    assert_eq!(
        native.timestamp.unwrap().to_bits(),
        17.125_f64.to_bits(),
        "native_price.timestamp"
    );
    shape::<PolymarketPrice>("PolymarketPrice", sample, &result["schema"]);
    let cases = result["priceCases"].as_array().unwrap();
    assert_eq!(cases.len(), 18, "price_string_case_count");
    assert_eq!(
        cases.iter().filter(|case| case["accepted"] == true).count(),
        14,
        "price_string_case_count_accepted"
    );
    for case in cases {
        let raw = case["raw"].as_str().unwrap();
        let decoded = js_from_str::<PolymarketPrice>(raw);
        if case["accepted"].as_bool().unwrap() {
            let source: Value = serde_json::from_str(raw).unwrap();
            let native = decoded.unwrap();
            assert_eq!(
                native.token_id, source["token_id"],
                "native_price_unicode_token"
            );
            assert_eq!(native.price, source["price"], "native_price_unicode_price");
            assert!(native.timestamp.is_none(), "price_optional_absence");
            assert_eq!(
                serde_json::to_value(native).unwrap(),
                source,
                "price_timestamp_wire_omission"
            );
        } else {
            typed_utf16::assert_projection(raw, &decoded.expect("typed record retains the frame"));
        }
    }
    let numeric = result["priceNumeric"].as_array().unwrap();
    assert_eq!(numeric.len(), 9, "price_numeric_case_count");
    for case in numeric {
        let raw = case["raw"].as_str().unwrap();
        let value: Value = serde_json::from_str(raw).unwrap();
        let buffered: JsNumberInput = serde_json::from_str(raw).unwrap();
        let paths: [PolymarketPrice; 4] = [
            js_from_str(raw).unwrap(),
            js_from_value(value.clone()).unwrap(),
            js_from_value_ref(&value).unwrap(),
            buffered.decode().unwrap(),
        ];
        for native in paths {
            assert_eq!(
                native.token_id, "numeric-token",
                "price_numeric_native_token"
            );
            assert_eq!(native.price, "0.012300", "price_numeric_native_string");
            assert_eq!(
                format!("{:016x}", native.timestamp.unwrap().to_bits()),
                case["bits"],
                "price_timestamp_bits_before_json"
            );
        }
    }
}

fn assert_book_source_count(source: &Value, expected: usize) {
    for side in ["bids", "asks"] {
        assert_eq!(
            source[side].as_array().unwrap().len(),
            expected,
            "book_source.{side}.length"
        );
    }
}

fn assert_book_native(native: &PolymarketOrderBook, source: &Value) -> usize {
    let mut comparisons = 0;
    assert_eq!(native.asset_id, source["asset_id"], "native_book.asset_id");
    assert_eq!(
        native.hash.as_deref(),
        source["hash"].as_str(),
        "native_book.hash"
    );
    for (name, levels) in [("bids", &native.bids), ("asks", &native.asks)] {
        let expected = source[name].as_array().unwrap();
        assert_eq!(levels.len(), expected.len(), "native_book.{name}.length");
        for (index, (actual, expected)) in levels.iter().zip(expected).enumerate() {
            assert_eq!(
                actual.price, expected["price"],
                "native_book.{name}[{index}].price"
            );
            assert_eq!(
                actual.size, expected["size"],
                "native_book.{name}[{index}].size"
            );
            comparisons += 1;
        }
    }
    assert_eq!(
        comparisons,
        source["bids"].as_array().unwrap().len() + source["asks"].as_array().unwrap().len(),
        "native_book.level_comparison_count"
    );
    comparisons
}

#[test]
fn order_book_record_actual_typescript() {
    let result = oracle();
    assert_eq!(
        result["schema"]["PolymarketOrderBook"],
        json!([
            ["asset_id", false, "string"],
            ["bids", false, "PolymarketOrderBookLevel[]"],
            ["asks", false, "PolymarketOrderBookLevel[]"],
            ["timestamp", true, "number"],
            ["hash", true, "string"]
        ]),
        "book_schema"
    );
    let sample = &result["bookSample"];
    assert_book_source_count(sample, 2);
    let native: PolymarketOrderBook = js_from_value_ref(sample).unwrap();
    let mut comparisons = assert_book_native(&native, sample);
    assert_eq!(
        native.timestamp.unwrap().to_bits(),
        123.625_f64.to_bits(),
        "native_book.timestamp"
    );
    shape::<PolymarketOrderBook>("PolymarketOrderBook", sample, &result["schema"]);
    let absent: PolymarketOrderBook = js_from_value_ref(&result["bookAbsent"]).unwrap();
    assert_book_source_count(&result["bookAbsent"], 0);
    comparisons += assert_book_native(&absent, &result["bookAbsent"]);
    assert!(
        absent.timestamp.is_none() && absent.hash.is_none(),
        "book_optional_absence"
    );
    assert_eq!(
        serde_json::to_value(absent).unwrap(),
        result["bookAbsent"],
        "book_optional_wire_omission"
    );
    // Array membership/order and strict nested record decoding are separate from the outer schema.
    for side in ["bids", "asks"] {
        for invalid in [
            Value::Null,
            json!(["0.1", "2.0"]),
            json!({"price":"0.1"}),
            json!({"price":false,"size":"2"}),
        ] {
            let mut bad = sample.clone();
            bad[side] = json!([invalid]);
            assert!(
                js_from_value::<PolymarketOrderBook>(bad).is_err(),
                "book_nested_record_refusal:{side}"
            );
        }
    }
    let cases = result["bookCases"].as_array().unwrap();
    assert_eq!(cases.len(), 54, "book_string_case_count");
    assert_eq!(
        cases.iter().filter(|case| case["accepted"] == true).count(),
        42,
        "book_string_case_count_accepted"
    );
    for case in cases {
        let raw = case["raw"].as_str().unwrap();
        let native = js_from_str::<PolymarketOrderBook>(raw);
        if case["accepted"].as_bool().unwrap() {
            let source: Value = serde_json::from_str(raw).unwrap();
            let native = native.unwrap();
            assert_book_source_count(&source, 2);
            comparisons += assert_book_native(&native, &source);
            assert_eq!(
                native.timestamp.unwrap().to_bits(),
                123.625_f64.to_bits(),
                "book_unicode_native_timestamp"
            );
            assert_eq!(
                serde_json::to_value(native).unwrap(),
                source,
                "book_unicode_wire"
            );
        } else {
            typed_utf16::assert_projection(raw, &native.expect("typed record retains the frame"));
        }
    }
    let numeric = result["bookNumeric"].as_array().unwrap();
    assert_eq!(numeric.len(), 9, "book_numeric_case_count");
    for case in numeric {
        let raw = case["raw"].as_str().unwrap();
        let value: Value = serde_json::from_str(raw).unwrap();
        let buffered: JsNumberInput = serde_json::from_str(raw).unwrap();
        let paths: [PolymarketOrderBook; 4] = [
            js_from_str(raw).unwrap(),
            js_from_value(value.clone()).unwrap(),
            js_from_value_ref(&value).unwrap(),
            buffered.decode().unwrap(),
        ];
        for native in paths {
            assert_book_source_count(&value, 1);
            comparisons += assert_book_native(&native, &value);
            assert_eq!(
                format!("{:016x}", native.timestamp.unwrap().to_bits()),
                case["bits"],
                "book_timestamp_bits_before_json"
            );
        }
    }
    assert_eq!(comparisons, 244, "book_total_native_level_comparisons");
}

fn assert_order_response_native(native: &PolymarketOrderResponse, source: &Value) -> usize {
    assert_eq!(
        native.success,
        source["success"].as_bool().unwrap(),
        "native_order_response.success"
    );
    assert_eq!(
        native.error_msg.as_deref(),
        source["errorMsg"].as_str(),
        "native_order_response.error_msg"
    );
    assert_eq!(
        native.order_id.as_deref(),
        source["orderID"].as_str(),
        "native_order_response.order_id"
    );
    assert_eq!(
        native.status.as_deref(),
        source["status"].as_str(),
        "native_order_response.status"
    );
    assert_eq!(
        native.taking_amount.as_deref(),
        source["takingAmount"].as_str(),
        "native_order_response.taking_amount"
    );
    assert_eq!(
        native.making_amount.as_deref(),
        source["makingAmount"].as_str(),
        "native_order_response.making_amount"
    );
    let hashes = source["transactionsHashes"].as_array().map(|values| {
        values
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        native.transactions_hashes.as_ref().map(Vec::len),
        hashes.as_ref().map(Vec::len),
        "native_order_response.transactions_hashes.length"
    );
    assert_eq!(
        native.transactions_hashes, hashes,
        "native_order_response.transactions_hashes"
    );
    native.transactions_hashes.as_ref().map_or(0, Vec::len)
}

#[test]
fn order_response_record_actual_typescript() {
    let result = oracle();
    assert_eq!(
        result["schema"]["PolymarketOrderResponse"],
        json!([
            ["success", false, "boolean"],
            ["errorMsg", true, "string"],
            ["orderID", true, "string"],
            ["transactionsHashes", true, "string[]"],
            ["status", true, "string"],
            ["takingAmount", true, "string"],
            ["makingAmount", true, "string"]
        ]),
        "order_response_schema"
    );
    let sample = &result["orderResponseSample"];
    assert_eq!(
        sample["transactionsHashes"].as_array().unwrap().len(),
        2,
        "response_source_hash_count"
    );
    let native: PolymarketOrderResponse = js_from_value_ref(sample).unwrap();
    let mut comparisons = assert_order_response_native(&native, sample);
    shape::<PolymarketOrderResponse>("PolymarketOrderResponse", sample, &result["schema"]);
    for key in ["orderResponseAbsent", "orderResponseEmptyHashes"] {
        let native: PolymarketOrderResponse = js_from_value_ref(&result[key]).unwrap();
        assert!(
            result[key]["transactionsHashes"]
                .as_array()
                .is_none_or(Vec::is_empty),
            "response_source_empty_hashes"
        );
        comparisons += assert_order_response_native(&native, &result[key]);
        assert_eq!(
            serde_json::to_value(native).unwrap(),
            result[key],
            "order_response_optional_wire"
        );
    }
    for invalid in [Value::Null, json!(1), json!(false), json!({"hash":"x"})] {
        let mut bad = sample.clone();
        bad["transactionsHashes"] = json!([invalid]);
        assert!(
            js_from_value::<PolymarketOrderResponse>(bad).is_err(),
            "order_response_hash_element_refusal"
        );
    }
    let cases = result["orderResponseCases"].as_array().unwrap();
    assert_eq!(cases.len(), 54, "order_response_string_case_count");
    assert_eq!(
        cases.iter().filter(|case| case["accepted"] == true).count(),
        42,
        "order_response_string_case_count_accepted"
    );
    for case in cases {
        let raw = case["raw"].as_str().unwrap();
        let decoded = js_from_str::<PolymarketOrderResponse>(raw);
        if case["accepted"].as_bool().unwrap() {
            let source: Value = serde_json::from_str(raw).unwrap();
            let native = decoded.unwrap();
            assert_eq!(
                source["transactionsHashes"].as_array().unwrap().len(),
                2,
                "response_source_case_hash_count"
            );
            comparisons += assert_order_response_native(&native, &source);
            assert_eq!(
                serde_json::to_value(native).unwrap(),
                source,
                "order_response_unicode_wire"
            );
        } else {
            typed_utf16::assert_projection(raw, &decoded.expect("typed record retains the frame"));
        }
    }
    assert_eq!(comparisons, 86, "response_total_native_hash_comparisons");
}

fn assert_clob_native(native: &PolymarketClobMarket, source: &Value) {
    assert_eq!(
        native.condition_id, source["condition_id"],
        "native_clob.condition_id"
    );
    assert_eq!(native.token_id, source["token_id"], "native_clob.token_id");
    assert_eq!(native.outcome, source["outcome"], "native_clob.outcome");
    assert_eq!(
        native.minimum_order_size, source["minimum_order_size"],
        "native_clob.minimum_order_size"
    );
    assert_eq!(
        native.minimum_tick_size, source["minimum_tick_size"],
        "native_clob.minimum_tick_size"
    );
    assert_eq!(
        native.active,
        source["active"].as_bool().unwrap(),
        "native_clob.active"
    );
    assert_eq!(
        native.closed,
        source["closed"].as_bool().unwrap(),
        "native_clob.closed"
    );
}

#[test]
fn clob_market_record_actual_typescript() {
    let result = oracle();
    assert_eq!(
        result["schema"]["PolymarketClobMarket"],
        json!([
            ["condition_id", false, "string"],
            ["token_id", false, "string"],
            ["outcome", false, "string"],
            ["minimum_order_size", false, "string"],
            ["minimum_tick_size", false, "string"],
            ["active", false, "boolean"],
            ["closed", false, "boolean"]
        ]),
        "clob_market_schema"
    );
    let sample = result["clobSample"].as_array().unwrap();
    assert_eq!(sample.len(), 2, "clob_source_sample_count");
    let native: Vec<PolymarketClobMarket> = js_from_value_ref(&result["clobSample"]).unwrap();
    assert_eq!(native.len(), sample.len(), "native_clob.batch_length");
    let mut comparisons = 0;
    for (native, source) in native.iter().zip(sample) {
        assert_clob_native(native, source);
        comparisons += 1;
    }
    assert_eq!(comparisons, 2, "native_clob.sample_comparison_count");
    shape::<PolymarketClobMarket>("PolymarketClobMarket", &sample[0], &result["schema"]);
    let cases = result["clobCases"].as_array().unwrap();
    assert_eq!(cases.len(), 45, "clob_string_case_count");
    assert_eq!(
        cases.iter().filter(|c| c["accepted"] == true).count(),
        35,
        "clob_string_accepted_count"
    );
    for case in cases {
        let raw = case["raw"].as_str().unwrap();
        let native = js_from_str::<PolymarketClobMarket>(raw);
        if case["accepted"] == true {
            let source: Value = serde_json::from_str(raw).unwrap();
            assert_clob_native(&native.unwrap(), &source);
            comparisons += 1;
        } else {
            typed_utf16::assert_projection(raw, &native.expect("typed record retains the frame"));
        }
    }
    let flags = result["clobFlagCases"].as_array().unwrap();
    assert_eq!(flags.len(), 4, "clob_boolean_case_count");
    for (index, source) in flags.iter().enumerate() {
        assert_eq!(
            source["active"],
            index >= 2,
            "clob_source_active_truth_table"
        );
        assert_eq!(
            source["closed"],
            index % 2 == 1,
            "clob_source_closed_truth_table"
        );
        let native: PolymarketClobMarket = js_from_value_ref(source).unwrap();
        assert_clob_native(&native, source);
        comparisons += 1;
    }
    assert_eq!(comparisons, 41, "native_clob.total_record_comparisons");
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
