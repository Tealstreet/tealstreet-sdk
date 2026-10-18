use safe_cex_num::numeric_ingress::{
    JsNumberInput, JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_types::object_map::JsObjectMap;
use safe_cex_venue_lighter::sdk_batches::{
    BatchCreateOrder, BatchCreateOrdersParams, BatchTransaction, BatchTxReq, BatchTxResponse,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{collections::BTreeMap, process::Command, sync::OnceLock};

fn corpus() -> &'static Value {
    static CAPTURE: OnceLock<Value> = OnceLock::new();
    CAPTURE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/sdk-batches-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "batch source oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            value["header"]["identitySha256"],
            "e4a67a59eae708a9b14e37450fa04bc58c6ff0c51040f83fc8193b07060e2c62",
            "batch complete source identities"
        );
        for (key, count) in [
            ("numeric", 340),
            ("strings", 81),
            ("depth", 40),
            ("optionalNull", 4),
            ("retainedExtras", 6),
            ("errorKeyProbes", 2),
            ("nonJsonExtras", 8),
        ] {
            assert_eq!(
                value[key].as_array().unwrap().len(),
                count,
                "batch {key} inventory"
            );
        }
        assert_eq!(
            value["schemas"].as_object().unwrap().len(),
            9,
            "batch schema inventory"
        );
        assert_eq!(
            value["unions"].as_object().unwrap().len(),
            2,
            "batch union inventory"
        );
        for (name, count) in [
            ("BatchTxReq", 5),
            ("BatchCreateOrdersParams", 5),
            ("BatchTxResponse", 4),
        ] {
            assert_eq!(
                value["cases"][name].as_array().unwrap().len(),
                count,
                "{name} source case inventory"
            );
        }
        value
    })
}
#[derive(Debug, PartialEq)]
enum Identity {
    Null,
    Bool(bool),
    Number(u64),
    String(String),
    Array(Vec<Identity>),
    Object(BTreeMap<String, Identity>),
}
fn expected(value: &Value) -> Identity {
    match value {
        Value::Null => Identity::Null,
        Value::Bool(v) => Identity::Bool(*v),
        Value::Number(v) => Identity::Number(v.as_f64().unwrap().to_bits()),
        Value::String(v) => Identity::String(v.clone()),
        Value::Array(v) => Identity::Array(v.iter().map(expected).collect()),
        Value::Object(v) => {
            Identity::Object(v.iter().map(|(k, v)| (k.clone(), expected(v))).collect())
        }
    }
}
fn own_keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
    keys.sort_by_key(|key| match key.parse::<u32>() {
        Ok(index) if index != u32::MAX && index.to_string() == *key => (false, index),
        _ => (true, 0),
    });
    keys
}
fn retained(input: &JsNumberInput, shape: &Value, label: &str) -> Identity {
    match shape {
        Value::Null => {
            assert_eq!(
                input.clone().decode::<Option<bool>>().unwrap(),
                None,
                "{label} retained null"
            );
            Identity::Null
        }
        Value::Bool(_) => Identity::Bool(input.clone().decode::<bool>().unwrap()),
        Value::Number(_) => Identity::Number(input.clone().decode::<f64>().unwrap().to_bits()),
        Value::String(_) => Identity::String(input.clone().decode::<String>().unwrap()),
        Value::Array(values) => {
            let actual = input.clone().decode::<Vec<JsNumberInput>>().unwrap();
            assert_eq!(
                actual.len(),
                values.len(),
                "{label} retained nested array length"
            );
            Identity::Array(
                actual
                    .iter()
                    .enumerate()
                    .map(|(i, v)| retained(v, &values[i], label))
                    .collect(),
            )
        }
        Value::Object(values) => {
            let actual = input
                .clone()
                .decode::<JsObjectMap<JsNumberInput>>()
                .unwrap();
            assert_eq!(actual.len(), values.len(), "{label} retained own key count");
            assert_eq!(
                actual.keys().cloned().collect::<Vec<_>>(),
                own_keys(shape),
                "{label} retained native own-key order"
            );
            Identity::Object(
                actual
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            retained(
                                v,
                                values
                                    .get(k)
                                    .expect("retained extra key must exist in source"),
                                label,
                            ),
                        )
                    })
                    .collect(),
            )
        }
    }
}
fn native_length(actual: usize, expected: usize, label: &str) {
    assert_eq!(actual, expected, "BATCH-CARDINALITY {label} native length");
}
fn comparison_count(actual: usize, expected: usize, label: &str) {
    assert_eq!(
        actual, expected,
        "BATCH-CARDINALITY {label} comparison count"
    );
}
fn array_identity(actual: Vec<Identity>, source: &Value, label: &str) {
    let source = source.as_array().unwrap();
    native_length(actual.len(), source.len(), label);
    let mut compared = 0;
    for (i, value) in actual.iter().enumerate() {
        compared += 1;
        assert_eq!(
            Some(value),
            source.get(i).map(expected).as_ref(),
            "BATCH-CARDINALITY {label} native item {i}"
        );
    }
    comparison_count(compared, source.len(), label);
    assert_eq!(
        Identity::Array(actual),
        Identity::Array(source.iter().map(expected).collect()),
        "BATCH-CARDINALITY {label} native collection identity"
    );
}
fn paths<T: DeserializeOwned + Serialize + JsNumberOutput>(
    raw: &str,
    value: &Value,
) -> Vec<(&'static str, Result<T, String>)> {
    vec![
        (
            "bare raw",
            serde_json::from_str(raw).map_err(|e| e.to_string()),
        ),
        (
            "bare owned",
            T::deserialize(value.clone()).map_err(|e| e.to_string()),
        ),
        (
            "bare borrowed",
            T::deserialize(value).map_err(|e| e.to_string()),
        ),
        ("shared raw", js_from_str(raw).map_err(|e| e.to_string())),
        (
            "shared owned",
            js_from_value(value.clone()).map_err(|e| e.to_string()),
        ),
        (
            "shared borrowed",
            js_from_value_ref(value).map_err(|e| e.to_string()),
        ),
    ]
}
enum Batch {
    Tx(BatchTxReq),
    Orders(BatchCreateOrdersParams),
    Response(BatchTxResponse),
}
fn batch_paths(name: &str, raw: &str, value: &Value) -> Vec<(&'static str, Result<Batch, String>)> {
    match name {
        "BatchTxReq" => paths::<BatchTxReq>(raw, value)
            .into_iter()
            .map(|(p, v)| (p, v.map(Batch::Tx)))
            .collect(),
        "BatchCreateOrdersParams" => paths::<BatchCreateOrdersParams>(raw, value)
            .into_iter()
            .map(|(p, v)| (p, v.map(Batch::Orders)))
            .collect(),
        "BatchTxResponse" => paths::<BatchTxResponse>(raw, value)
            .into_iter()
            .map(|(p, v)| (p, v.map(Batch::Response)))
            .collect(),
        _ => panic!("unknown batch record {name}"),
    }
}
fn optional_number(actual: Option<f64>, input: &Value, key: &str, label: &str) {
    assert_eq!(
        actual.map(f64::to_bits),
        input.get(key).map(|v| v.as_f64().unwrap().to_bits()),
        "{label}.{key} native identity"
    );
}
fn check_native(value: &Batch, input: &Value, label: &str) {
    match value {
        Batch::Tx(v) => {
            native_length(
                v.transactions.len(),
                input["transactions"].as_array().unwrap().len(),
                &format!("BatchTxReq.transactions {label}"),
            );
            array_identity(
                v.transactions
                    .iter()
                    .enumerate()
                    .map(|(i, x)| retained(x.retained(), &input["transactions"][i], label))
                    .collect(),
                &input["transactions"],
                &format!("BatchTxReq.transactions {label}"),
            );
            optional_number(v.account_index, input, "accountIndex", label);
            optional_number(v.api_key_index, input, "apiKeyIndex", label);
        }
        Batch::Orders(v) => {
            native_length(
                v.orders.len(),
                input["orders"].as_array().unwrap().len(),
                &format!("BatchCreateOrdersParams.orders {label}"),
            );
            array_identity(
                v.orders
                    .iter()
                    .enumerate()
                    .map(|(i, x)| retained(x.retained(), &input["orders"][i], label))
                    .collect(),
                &input["orders"],
                &format!("BatchCreateOrdersParams.orders {label}"),
            );
            optional_number(v.account_index, input, "accountIndex", label);
            optional_number(v.api_key_index, input, "apiKeyIndex", label);
        }
        Batch::Response(v) => {
            assert_eq!(
                v.errors.keys().cloned().collect::<Vec<_>>(),
                own_keys(&input["errors"]),
                "{label} errors native own-key order"
            );
            array_identity(
                v.tx_hashes.iter().cloned().map(Identity::String).collect(),
                &input["txHashes"],
                &format!("BatchTxResponse.txHashes {label}"),
            );
            array_identity(
                v.success_indices
                    .iter()
                    .map(|v| Identity::Number(v.to_bits()))
                    .collect(),
                &input["successIndices"],
                &format!("BatchTxResponse.successIndices {label}"),
            );
            array_identity(
                v.failed_indices
                    .iter()
                    .map(|v| Identity::Number(v.to_bits()))
                    .collect(),
                &input["failedIndices"],
                &format!("BatchTxResponse.failedIndices {label}"),
            );
            assert_eq!(
                Identity::Object(
                    v.errors
                        .iter()
                        .map(|(k, v)| (k.clone(), Identity::String(v.clone())))
                        .collect()
                ),
                expected(&input["errors"]),
                "{label} errors native identity"
            );
        }
    }
}
fn collection(record: &str, field: &str, count: usize) {
    let input = &corpus()["cases"][record][0]["input"];
    assert_eq!(
        input[field].as_array().unwrap().len(),
        count,
        "BATCH-CARDINALITY {record}.{field} fixed source length"
    );
    for (path, value) in batch_paths(record, &input.to_string(), input) {
        let value = value.unwrap_or_else(|e| {
            panic!("{record}.{field} {path} must decode before comparison: {e}")
        });
        let actual_len = match (&value, field) {
            (Batch::Tx(v), "transactions") => v.transactions.len(),
            (Batch::Orders(v), "orders") => v.orders.len(),
            (Batch::Response(v), "txHashes") => v.tx_hashes.len(),
            (Batch::Response(v), "successIndices") => v.success_indices.len(),
            (Batch::Response(v), "failedIndices") => v.failed_indices.len(),
            _ => unreachable!(),
        };
        native_length(actual_len, count, &format!("{record}.{field} {path}"));
        match (&value, field) {
            (Batch::Tx(v), "transactions") => array_identity(
                v.transactions
                    .iter()
                    .enumerate()
                    .map(|(i, x)| retained(x.retained(), &input[field][i], field))
                    .collect(),
                &input[field],
                &format!("{record}.{field} {path}"),
            ),
            (Batch::Orders(v), "orders") => array_identity(
                v.orders
                    .iter()
                    .enumerate()
                    .map(|(i, x)| retained(x.retained(), &input[field][i], field))
                    .collect(),
                &input[field],
                &format!("{record}.{field} {path}"),
            ),
            (Batch::Response(v), "txHashes") => array_identity(
                v.tx_hashes.iter().cloned().map(Identity::String).collect(),
                &input[field],
                &format!("{record}.{field} {path}"),
            ),
            (Batch::Response(v), "successIndices") => array_identity(
                v.success_indices
                    .iter()
                    .map(|v| Identity::Number(v.to_bits()))
                    .collect(),
                &input[field],
                &format!("{record}.{field} {path}"),
            ),
            (Batch::Response(v), "failedIndices") => array_identity(
                v.failed_indices
                    .iter()
                    .map(|v| Identity::Number(v.to_bits()))
                    .collect(),
                &input[field],
                &format!("{record}.{field} {path}"),
            ),
            _ => unreachable!(),
        }
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_batch_collection_transactions() {
    collection("BatchTxReq", "transactions", 4);
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_batch_collection_orders() {
    collection("BatchCreateOrdersParams", "orders", 2);
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_batch_collection_tx_hashes() {
    collection("BatchTxResponse", "txHashes", 3);
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_batch_collection_success_indices() {
    collection("BatchTxResponse", "successIndices", 2);
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_batch_collection_failed_indices() {
    collection("BatchTxResponse", "failedIndices", 1);
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_batches_source_shapes_and_retained_identity() {
    for (record, field_count) in [
        ("BatchTxReq", 3),
        ("BatchCreateOrdersParams", 3),
        ("BatchTxResponse", 4),
    ] {
        assert_eq!(
            corpus()["schemas"][record].as_array().unwrap().len(),
            field_count,
            "{record} schema field count"
        );
    }
    macro_rules! field_type {
        ($value:expr, $expected:ty, $name:literal) => {
            assert_eq!(
                std::any::type_name_of_val(&$value),
                std::any::type_name::<$expected>(),
                concat!($name, " public field type")
            );
        };
    }
    let tx: BatchTxReq =
        serde_json::from_value(corpus()["cases"]["BatchTxReq"][0]["input"].clone()).unwrap();
    field_type!(
        tx.transactions,
        Vec<BatchTransaction>,
        "BatchTxReq.transactions"
    );
    field_type!(tx.account_index, Option<f64>, "BatchTxReq.accountIndex");
    field_type!(tx.api_key_index, Option<f64>, "BatchTxReq.apiKeyIndex");
    let orders: BatchCreateOrdersParams =
        serde_json::from_value(corpus()["cases"]["BatchCreateOrdersParams"][0]["input"].clone())
            .unwrap();
    field_type!(
        orders.orders,
        Vec<BatchCreateOrder>,
        "BatchCreateOrdersParams.orders"
    );
    field_type!(
        orders.account_index,
        Option<f64>,
        "BatchCreateOrdersParams.accountIndex"
    );
    field_type!(
        orders.api_key_index,
        Option<f64>,
        "BatchCreateOrdersParams.apiKeyIndex"
    );
    let response: BatchTxResponse =
        serde_json::from_value(corpus()["cases"]["BatchTxResponse"][0]["input"].clone()).unwrap();
    field_type!(response.tx_hashes, Vec<String>, "BatchTxResponse.txHashes");
    field_type!(
        response.success_indices,
        Vec<f64>,
        "BatchTxResponse.successIndices"
    );
    field_type!(
        response.failed_indices,
        Vec<f64>,
        "BatchTxResponse.failedIndices"
    );
    field_type!(
        response.errors,
        JsObjectMap<String>,
        "BatchTxResponse.errors"
    );
    for (index, row) in corpus()["nonJsonExtras"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let record = if index < 4 {
            "BatchTxReq"
        } else {
            "BatchCreateOrdersParams"
        };
        let kind = ["undefined", "bigint", "function", "symbol"][index % 4];
        let expression = ["undefined", "17n", "() => 17", "Symbol('batch-extra')"][index % 4];
        assert_eq!(
            row,
            &json!({"record":record,"kind":kind,"accepted":true,"ownPresent":true,"typeof":kind,"expression":expression}),
            "{record} {kind} measured nonJSON extra FINDING"
        );
    }
    assert!(
        BatchTxReq::deserialize(json!([[], 101.25, 102.5])).is_err(),
        "BatchTxReq qualified map-only decoder"
    );
    assert!(
        BatchCreateOrdersParams::deserialize(json!([[], 201.25, 202.5])).is_err(),
        "BatchCreateOrdersParams qualified map-only decoder"
    );
    assert!(
        BatchTxResponse::deserialize(json!([[], [], [], {}])).is_err(),
        "BatchTxResponse qualified map-only decoder"
    );
    for record in ["BatchTxReq", "BatchCreateOrdersParams", "BatchTxResponse"] {
        for row in corpus()["cases"][record].as_array().unwrap() {
            let input = &row["input"];
            let accepted = row["accepted"] == true && row["name"] != "nonnumeric-error-value";
            for (path, value) in batch_paths(record, &input.to_string(), input) {
                assert_eq!(
                    value.is_ok(),
                    accepted,
                    "{record} {} {path} admission",
                    row["name"]
                );
                if let Ok(value) = value {
                    check_native(&value, input, &format!("{record} {} {path}", row["name"]));
                }
            }
        }
        let base = corpus()["cases"][record][0]["input"].clone();
        for invalid in [
            json!(null),
            json!(false),
            json!(12),
            json!("record"),
            json!([]),
            Value::Array(base.as_object().unwrap().values().cloned().collect()),
        ] {
            for (path, result) in batch_paths(record, &invalid.to_string(), &invalid) {
                assert!(result.is_err(), "{record} {path} map-only shape");
            }
        }
        for field in corpus()["schemas"][record].as_array().unwrap() {
            let key = field["name"].as_str().unwrap();
            for replacement in [None, Some(Value::Null)] {
                let mut input = base.clone();
                if let Some(v) = replacement.clone() {
                    input[key] = v;
                } else {
                    input.as_object_mut().unwrap().remove(key);
                }
                let accepted = replacement.is_none() && field["optional"] == true;
                for (path, result) in batch_paths(record, &input.to_string(), &input) {
                    assert_eq!(
                        result.is_ok(),
                        accepted,
                        "{record}.{key} {path} required/present-null"
                    );
                    if let Ok(v) = result {
                        check_native(&v, &input, record);
                    }
                }
            }
        }
    }
    assert_eq!(
        corpus()["errorKeyProbes"][0],
        json!({"key":"foo","value":123,"accepted":true}),
        "numeric index signature nonnumeric extra FINDING"
    );
    assert_eq!(
        corpus()["errorKeyProbes"][1],
        json!({"key":"1","value":123,"accepted":false}),
        "numeric index signature numeric key"
    );
}
fn child_at(input: &JsNumberInput, path: &[Value]) -> JsNumberInput {
    let mut value = input.clone();
    for key in path {
        value = if let Some(key) = key.as_str() {
            value
                .decode::<JsObjectMap<JsNumberInput>>()
                .unwrap()
                .get(key)
                .unwrap()
                .clone()
        } else {
            value.decode::<Vec<JsNumberInput>>().unwrap()[key.as_u64().unwrap() as usize].clone()
        };
    }
    value
}
fn leaf(value: &Batch, path: &[Value]) -> JsNumberInput {
    match value {
        Batch::Tx(v) => child_at(
            v.transactions[path[1].as_u64().unwrap() as usize].retained(),
            &path[2..],
        ),
        Batch::Orders(v) => child_at(
            v.orders[path[1].as_u64().unwrap() as usize].retained(),
            &path[2..],
        ),
        Batch::Response(_) => unreachable!(),
    }
}
fn number(value: &Batch, path: &[Value]) -> f64 {
    match (value, path[0].as_str().unwrap()) {
        (Batch::Tx(v), "accountIndex") => v.account_index.unwrap(),
        (Batch::Tx(v), "apiKeyIndex") => v.api_key_index.unwrap(),
        (Batch::Orders(v), "accountIndex") => v.account_index.unwrap(),
        (Batch::Orders(v), "apiKeyIndex") => v.api_key_index.unwrap(),
        (Batch::Response(v), "successIndices") => {
            v.success_indices[path[1].as_u64().unwrap() as usize]
        }
        (Batch::Response(v), "failedIndices") => {
            v.failed_indices[path[1].as_u64().unwrap() as usize]
        }
        _ => leaf(value, path).decode::<f64>().unwrap(),
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_batches_numeric_native_bits() {
    let mut inventory = BTreeMap::<String, Vec<String>>::new();
    let mut expected_paths = Vec::new();
    for record in ["BatchTxReq", "BatchCreateOrdersParams"] {
        for key in ["accountIndex", "apiKeyIndex"] {
            expected_paths.push(format!("{record}:{:?}", vec![json!(key)]));
        }
    }
    for (record, array, children) in [
        (
            "BatchTxReq",
            "transactions",
            vec![
                vec![
                    "type",
                    "side",
                    "timeInForce",
                    "nonce",
                    "expiry",
                    "accountIndex",
                    "apiKeyIndex",
                ],
                vec!["nonce", "expiry", "accountIndex", "apiKeyIndex"],
                vec!["nonce", "expiry", "accountIndex", "apiKeyIndex"],
                vec!["nonce", "expiry", "accountIndex", "apiKeyIndex"],
            ],
        ),
        (
            "BatchCreateOrdersParams",
            "orders",
            vec![
                vec![
                    "side",
                    "timeInForce",
                    "accountIndex",
                    "apiKeyIndex",
                    "expiry",
                ],
                vec!["side", "accountIndex", "apiKeyIndex"],
            ],
        ),
    ] {
        for (index, fields) in children.iter().enumerate() {
            for field in fields {
                expected_paths.push(format!(
                    "{record}:{:?}",
                    vec![json!(array), json!(index), json!(field)]
                ));
            }
        }
    }
    for (field, count) in [("successIndices", 2), ("failedIndices", 1)] {
        for index in 0..count {
            expected_paths.push(format!(
                "BatchTxResponse:{:?}",
                vec![json!(field), json!(index)]
            ));
        }
    }
    expected_paths.sort();
    for row in corpus()["numeric"].as_array().unwrap() {
        let record = row["record"].as_str().unwrap();
        let path = row["path"].as_array().unwrap();
        let raw = row["encoded"].as_str().unwrap();
        inventory
            .entry(format!("{record}:{path:?}"))
            .or_default()
            .push(row["raw"].as_str().unwrap().to_owned());
        let input: Value = serde_json::from_str(raw).unwrap();
        let bits = u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap();
        if row["raw"] == "-0" {
            let mut native = input.clone();
            let mut cursor = &mut native;
            for key in &path[..path.len() - 1] {
                cursor = if let Some(key) = key.as_str() {
                    &mut cursor[key]
                } else {
                    &mut cursor[key.as_u64().unwrap() as usize]
                };
            }
            let key = path.last().unwrap();
            if let Some(key) = key.as_str() {
                cursor[key] = json!(-0.0);
            } else {
                cursor[key.as_u64().unwrap() as usize] = json!(-0.0);
            }
            for (entry, value) in batch_paths(record, &native.to_string(), &native) {
                assert_eq!(
                    number(&value.unwrap(), path).to_bits(),
                    (-0.0_f64).to_bits(),
                    "{record} {path:?} {entry} existing Value negative zero"
                );
            }
        }
        for (entry, value) in batch_paths(record, raw, &input) {
            let value = value.unwrap_or_else(|e| panic!("{record} {path:?} {entry}: {e}"));
            assert_eq!(
                number(&value, path).to_bits(),
                bits,
                "{record} {path:?} {entry} numeric native bits {} ",
                row["raw"]
            );
        }
    }
    assert_eq!(inventory.len(), 34, "batch numeric distinct field paths");
    assert_eq!(
        inventory.keys().cloned().collect::<Vec<_>>(),
        expected_paths,
        "batch exact numeric field paths"
    );
    for (path, lexemes) in inventory {
        assert_eq!(
            lexemes,
            [
                "-0",
                "-0.0",
                "-0e0",
                "0",
                "1e3",
                "1e400",
                "-1e400",
                "-1e-4000",
                "16777217.25",
                "9007199254740993"
            ],
            "{path} exact numeric lexemes"
        );
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_batches_unicode_and_depth() {
    let mut inventory = BTreeMap::<String, Vec<Vec<u16>>>::new();
    for row in corpus()["strings"].as_array().unwrap() {
        let record = row["record"].as_str().unwrap();
        let path = row["path"].as_array().unwrap();
        let raw = row["encoded"].as_str().unwrap();
        let units: Vec<u16> = row["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_u64().unwrap() as u16)
            .collect();
        inventory
            .entry(format!("{record}:{path:?}"))
            .or_default()
            .push(units.clone());
        let representable = String::from_utf16(&units).ok();
        assert_eq!(
            representable.is_some(),
            row["representable"] == true,
            "Unicode source classification"
        );
        if let Some(text) = representable {
            let input: Value = serde_json::from_str(raw).unwrap();
            for (entry, value) in batch_paths(record, raw, &input) {
                let value = value
                    .unwrap_or_else(|e| panic!("{record} {path:?} {entry} Unicode admission: {e}"));
                if path[0] != "extra" {
                    let actual = match &value {
                        Batch::Response(v) => {
                            if path[0] == "txHashes" {
                                v.tx_hashes[path[1].as_u64().unwrap() as usize].clone()
                            } else {
                                v.errors.get(path[1].as_str().unwrap()).unwrap().clone()
                            }
                        }
                        _ => leaf(&value, path).decode::<String>().unwrap(),
                    };
                    assert_eq!(
                        actual, text,
                        "{record} {path:?} {entry} positive Unicode native identity"
                    );
                }
            }
        } else {
            typed_utf16::retained(raw);
            let values = match record {
                "BatchTxReq" => vec![
                    Batch::Tx(serde_json::from_str(raw).unwrap()),
                    Batch::Tx(js_from_str(raw).unwrap()),
                ],
                "BatchCreateOrdersParams" => vec![
                    Batch::Orders(serde_json::from_str(raw).unwrap()),
                    Batch::Orders(js_from_str(raw).unwrap()),
                ],
                _ => vec![
                    Batch::Response(serde_json::from_str(raw).unwrap()),
                    Batch::Response(js_from_str(raw).unwrap()),
                ],
            };
            for value in values {
                if path[0] == "extra" {
                    continue;
                }
                let actual = match &value {
                    Batch::Response(v) if path[0] == "txHashes" => {
                        v.tx_hashes[path[1].as_u64().unwrap() as usize].clone()
                    }
                    Batch::Response(v) => v.errors.get(path[1].as_str().unwrap()).unwrap().clone(),
                    _ => {
                        let payload = leaf(&value, path);
                        let original: safe_cex_types::retained_json::RetainedJsonInput =
                            payload.clone().decode().unwrap();
                        assert_eq!(
                            original.as_text().unwrap(),
                            units,
                            "batch retained payload native units"
                        );
                        payload.decode::<String>().unwrap()
                    }
                };
                assert_eq!(
                    actual,
                    String::from_utf16_lossy(&units),
                    "batch typed String replacement"
                );
            }
        }
    }
    assert_eq!(inventory.len(), 9, "batch Unicode field inventory");
    let units = vec![
        vec![],
        vec![112, 108, 97, 105, 110],
        vec![233],
        vec![55357, 56425],
        vec![65279],
        vec![133],
        vec![55296],
        vec![57343],
        vec![120, 55296, 121],
    ];
    for (path, actual) in inventory {
        assert_eq!(actual, units, "{path} exact Unicode witnesses");
    }
    let mut depths = BTreeMap::<String, Vec<u64>>::new();
    for row in corpus()["depth"].as_array().unwrap() {
        let record = row["record"].as_str().unwrap();
        let location = row["location"].as_str().unwrap();
        let depth = row["depth"].as_u64().unwrap();
        depths
            .entry(format!("{record}:{location}"))
            .or_default()
            .push(depth);
        let mut input = corpus()["cases"][record][0]["input"].clone();
        let mut extra = json!(17);
        for _ in 0..depth {
            extra = json!([extra]);
        }
        if location == "root" {
            input["extra"] = extra;
        } else {
            input[if record == "BatchTxReq" {
                "transactions"
            } else {
                "orders"
            }][0]["extra"] = extra;
        }
        let accepted = depth <= if location == "root" { 126 } else { 124 };
        for (entry, result) in batch_paths(record, row["encoded"].as_str().unwrap(), &input) {
            assert_eq!(
                result.is_ok(),
                accepted,
                "{record} {location} depth {depth} {entry} bounded acceptance"
            );
            if location == "child"
                && let Ok(value) = &result
            {
                let field = if record == "BatchTxReq" {
                    "transactions"
                } else {
                    "orders"
                };
                let mut extra = leaf(value, &[json!(field), json!(0), json!("extra")]);
                for level in 0..depth {
                    let mut children = extra.decode::<Vec<JsNumberInput>>().unwrap();
                    assert_eq!(
                        children.len(),
                        1,
                        "{record} {entry} retained depth {depth} level {level} native cardinality"
                    );
                    extra = children.pop().unwrap();
                }
                assert_eq!(
                    extra.decode::<f64>().unwrap().to_bits(),
                    17_f64.to_bits(),
                    "{record} {entry} retained depth {depth} native leaf"
                );
            }
            if let Err(error) = result {
                assert!(
                    error.contains("recursion") || error.contains("depth"),
                    "{record} expected depth refusal: {error}"
                );
            }
        }
    }
    assert_eq!(depths.len(), 5, "batch depth location inventory");
    for (location, values) in depths {
        assert_eq!(
            values,
            [0, 2, 123, 124, 125, 126, 127, 140],
            "{location} exact depth witnesses"
        );
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_batches_source_controls() {
    for (mutation, name) in [
        (
            "heritage-BatchTxReq",
            "BatchTxReq unsupported interface heritage",
        ),
        (
            "heritage-BatchTxResponse",
            "BatchTxResponse unsupported interface heritage",
        ),
        (
            "heritage-BatchCreateOrdersParams",
            "BatchCreateOrdersParams unsupported interface heritage",
        ),
        ("transaction-union", "BatchTxReq source schema"),
        ("order-union", "BatchCreateOrdersParams source schema"),
        ("response-index-string", "BatchTxResponse source schema"),
        (
            "missing-import",
            "batch source program diagnostics must be empty: 21",
        ),
        (
            "discriminating-content",
            "batch independent complete fixture contents",
        ),
    ] {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/sdk-batches-oracle.cjs"
            ))
            .env("LIGHTER_SDK_BATCHES_MUTATION", mutation)
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "{mutation} source mutation must fail"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(name),
            "{mutation} named source failure: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
