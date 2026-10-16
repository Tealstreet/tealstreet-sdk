use safe_cex_venue_weex::selection::{
    WeexArrayRuntime, extract_array, get_desired_ticker_topics, is_node_or_bun_runtime,
};
use serde_json::Value;
use std::{cell::RefCell, collections::HashMap, path::Path, process::Command};

#[derive(Debug, PartialEq)]
enum NativeValue {
    Undefined,
    RevokedProxy,
    Null,
    Number(i64),
    String(Vec<u16>),
    Boolean(bool),
    Object(usize),
    Array(usize, Vec<i64>),
}

struct Host {
    nodes: Vec<Value>,
    counts: HashMap<(usize, Vec<u16>), usize>,
    trace: Vec<(usize, Vec<u16>)>,
}

fn units(value: &Value) -> Vec<u16> {
    serde_json::from_value(value.clone()).unwrap()
}

fn decode(value: &Value, nodes: &[Value]) -> Result<NativeValue, String> {
    Ok(match value["kind"].as_str().unwrap() {
        "undefined" => NativeValue::Undefined,
        "revoked" => NativeValue::RevokedProxy,
        "null" => NativeValue::Null,
        "number" => NativeValue::Number(value["value"].as_i64().unwrap()),
        "string" => NativeValue::String(units(&value["units"])),
        "boolean" => NativeValue::Boolean(value["value"].as_bool().unwrap()),
        "throw" => return Err(value["name"].as_str().unwrap().into()),
        "ref" => {
            let id = value["id"].as_u64().unwrap() as usize;
            if nodes[id]["kind"] == "array" {
                NativeValue::Array(
                    id,
                    nodes[id]["elements"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|item| item["value"].as_i64().unwrap())
                        .collect(),
                )
            } else {
                NativeValue::Object(id)
            }
        }
        other => panic!("unsupported input {other}"),
    })
}

fn get_property(host: &mut Host, value: &NativeValue, key: &[u16]) -> Result<NativeValue, String> {
    let id = match value {
        NativeValue::Null | NativeValue::Undefined | NativeValue::RevokedProxy => {
            return Err("TypeError".into());
        }
        NativeValue::Object(id) | NativeValue::Array(id, _) => *id,
        _ => return Ok(NativeValue::Undefined),
    };
    let property = host.nodes[id]["properties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|property| units(&property["key"]) == key);
    let Some(property) = property else {
        return Ok(NativeValue::Undefined);
    };
    host.trace.push((id, key.to_vec()));
    let count = host.counts.entry((id, key.to_vec())).or_default();
    let values = property["values"].as_array().unwrap();
    let value = &values[(*count).min(values.len() - 1)];
    *count += 1;
    decode(value, &host.nodes)
}

fn assert_native(value: NativeValue, expected: &Value, label: &str) {
    match value {
        NativeValue::RevokedProxy => panic!("unexpected revoked proxy {label}"),
        NativeValue::Undefined => assert_eq!(expected["kind"], "undefined", "{label}"),
        NativeValue::Null => assert_eq!(expected["kind"], "null", "{label}"),
        NativeValue::Number(number) => {
            assert_eq!(expected["kind"], "number", "{label}");
            assert_eq!(number, expected["value"].as_i64().unwrap(), "{label}");
        }
        NativeValue::String(string) => {
            assert_eq!(expected["kind"], "string", "{label}");
            assert_eq!(string, units(&expected["units"]), "{label}");
        }
        NativeValue::Boolean(boolean) => {
            assert_eq!(expected["kind"], "boolean", "{label}");
            assert_eq!(boolean, expected["value"].as_bool().unwrap(), "{label}");
        }
        NativeValue::Object(id) => {
            assert_eq!(expected["kind"], "object", "{label}");
            assert_eq!(id as i64, expected["id"].as_i64().unwrap(), "{label}");
        }
        NativeValue::Array(id, elements) => {
            assert_eq!(expected["kind"], "array", "{label}");
            let native_id = if id == usize::MAX { -1 } else { id as i64 };
            assert_eq!(native_id, expected["id"].as_i64().unwrap(), "{label}");
            let expected_elements: Vec<i64> = expected["elements"]
                .as_array()
                .unwrap()
                .iter()
                .map(|element| {
                    assert_eq!(element["kind"], "number", "{label}");
                    element["value"].as_i64().unwrap()
                })
                .collect();
            assert_eq!(elements, expected_elements, "{label}");
        }
    }
}

fn truthy(value: &Value) -> bool {
    match value["kind"].as_str().unwrap() {
        "undefined" | "null" | "nan" => false,
        "boolean" => value["value"].as_bool().unwrap(),
        "number" => value["value"].as_i64().unwrap() != 0,
        "string" => !units(&value["units"]).is_empty(),
        "object" => true,
        other => panic!("unsupported truthiness {other}"),
    }
}

#[test]
fn selection_matches_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/selection-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let array_ids = [
        "payload_array",
        "data_before_keys",
        "root_key",
        "nested_before_later_root",
        "first_key_order",
        "empty_keys",
        "missing",
        "undefined",
        "null",
        "number",
        "string",
        "boolean",
        "null_data",
        "changing_data",
        "changing_root",
        "changing_nested_null",
        "throw_data",
        "throw_root",
        "throw_second_data",
        "lone_surrogate_key",
        "revoked_payload",
        "revoked_data",
        "revoked_nested",
    ];
    let arrays = corpus["arrays"].as_array().unwrap();
    assert_eq!(arrays.len(), 23, "selection::array_count");
    for (index, case) in arrays.iter().enumerate() {
        let id = array_ids[index];
        assert_eq!(case["id"], id, "selection::array_identity");
        let mut host = Host {
            nodes: case["nodes"].as_array().unwrap().clone(),
            counts: HashMap::new(),
            trace: Vec::new(),
        };
        let payload = decode(&case["payload"], &host.nodes).unwrap();
        if id == "payload_array" {
            assert_eq!(
                payload,
                NativeValue::Array(2, vec![101, 102]),
                "selection::decoded_payload_array"
            );
        }
        let keys: Vec<Vec<u16>> = case["keys"].as_array().unwrap().iter().map(units).collect();
        let runtime = WeexArrayRuntime {
            is_array: |value| {
                if matches!(value, NativeValue::RevokedProxy) {
                    Err("TypeError".into())
                } else {
                    Ok(matches!(value, NativeValue::Array(..)))
                }
            },
            is_nullish: |value| matches!(value, NativeValue::Null | NativeValue::Undefined),
            undefined: || NativeValue::Undefined,
            get_property,
            empty_array: |_| NativeValue::Array(usize::MAX, Vec::new()),
        };
        let label = format!("selection::array::{id}");
        match extract_array(&mut host, payload, &keys, &runtime) {
            Ok(value) => {
                assert!(case["result"].get("error").is_none(), "{label}");
                assert_native(value, &case["result"]["ok"], &label);
            }
            Err(error) => assert_eq!(
                Some(error.as_str()),
                case["result"]["error"].as_str(),
                "{label}"
            ),
        }
        let expected_trace: Vec<(usize, Vec<u16>)> = case["trace"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| (entry[0].as_u64().unwrap() as usize, units(&entry[1])))
            .collect();
        assert_eq!(host.trace, expected_trace, "selection::array_trace::{id}");
    }
    let runtime_ids = [
        "absent",
        "node_short_circuit",
        "bun",
        "neither",
        "empty_node",
        "zero_node",
        "nan_node",
        "object_node",
        "node_error",
        "bun_error",
    ];
    let runtimes = corpus["runtimes"].as_array().unwrap();
    assert_eq!(runtimes.len(), 10, "selection::runtime_count");
    for (index, case) in runtimes.iter().enumerate() {
        let id = runtime_ids[index];
        assert_eq!(case["id"], id, "selection::runtime_identity");
        let trace = RefCell::new(Vec::new());
        let callback = |name: &'static str, field: &str| -> Result<bool, String> {
            trace.borrow_mut().push(name);
            if let Some(error) = case[format!("{field}Error")].as_str() {
                return Err(error.into());
            }
            Ok(truthy(&case[field]))
        };
        let result = is_node_or_bun_runtime(
            case["defined"].as_bool().unwrap(),
            || callback("node", "node"),
            || callback("bun", "bun"),
        );
        match result {
            Ok(value) => {
                assert_eq!(
                    case["result"]["error"].as_str(),
                    None,
                    "selection::runtime::{id}"
                );
                assert_eq!(
                    value,
                    case["result"]["ok"].as_bool().unwrap(),
                    "selection::runtime::{id}"
                );
            }
            Err(error) => assert_eq!(
                Some(error.as_str()),
                case["result"]["error"].as_str(),
                "selection::runtime::{id}"
            ),
        }
        let expected_trace: Vec<&str> = case["trace"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_str().unwrap())
            .collect();
        assert_eq!(
            *trace.borrow(),
            expected_trace,
            "selection::runtime_trace::{id}"
        );
    }
    let topic_ids = [
        "all",
        "active_market_order",
        "duplicates",
        "unavailable",
        "empty_markets",
        "utf16",
        "capacity_0",
        "capacity_9",
        "capacity_10",
        "capacity_11",
        "capacity_98",
        "capacity_99",
        "capacity_100",
        "capacity_101",
    ];
    let topics = corpus["topics"].as_array().unwrap();
    assert_eq!(topics.len(), 14, "selection::topic_count");
    for (index, case) in topics.iter().enumerate() {
        let id = topic_ids[index];
        assert_eq!(case["id"], id, "selection::topic_identity");
        let available: Vec<Vec<u16>> = case["available"]
            .as_array()
            .unwrap()
            .iter()
            .map(units)
            .collect();
        let active: Vec<Vec<u16>> = case["active"]
            .as_array()
            .unwrap()
            .iter()
            .map(units)
            .collect();
        let result = get_desired_ticker_topics(
            &available,
            &active,
            case["count"].as_u64().unwrap() as usize,
        );
        let expected: Vec<Vec<u16>> = case["result"]
            .as_array()
            .unwrap()
            .iter()
            .map(units)
            .collect();
        assert_eq!(result, expected, "selection::topics::{id}");
        if id == "capacity_0" {
            assert_eq!(result.len(), 90, "selection::ticker_cap");
        }
        if id == "duplicates" {
            assert_eq!(
                result,
                vec!["BTC@ticker".encode_utf16().collect::<Vec<_>>(); 2],
                "selection::duplicates_retained"
            );
        }
    }
}
