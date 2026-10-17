use safe_cex_num::{JsJsonValue, math_round};
use safe_cex_venue_apexomni::{
    api::*, exchange_utils::*, order_records::Iso8601, utils::hmac_sha256_base64,
};
use serde_json::Value;
use std::{process::Command, sync::OnceLock};

fn corpus() -> &'static Value {
    static CORPUS: OnceLock<Value> = OnceLock::new();
    CORPUS.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/pure.cjs"))
            .env("SAFE_CEX_RUST_ARCH", std::env::consts::ARCH)
            .env("LC_ALL", "en_US.UTF-8")
            .output()
            .expect("actual source pure helpers");
        assert!(
            output.status.success(),
            "pure source oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn rows(key: &str, count: usize) -> &'static [Value] {
    let rows = corpus()[key].as_array().unwrap();
    assert_eq!(rows.len(), count, "{key} independent source floor");
    rows
}
fn units(value: &Value) -> Vec<u16> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn number(value: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value.as_str().unwrap(), 16).unwrap())
}
fn input(value: &Value) -> JsJsonValue {
    match value["kind"].as_str().unwrap() {
        "undefined" => JsJsonValue::Undefined,
        "null" => JsJsonValue::Null,
        "boolean" => JsJsonValue::Bool(value["value"].as_bool().unwrap()),
        "string" => JsJsonValue::String(value["value"].as_str().unwrap().to_owned()),
        "number" => JsJsonValue::Number(number(&value["bits"])),
        "bigint" => JsJsonValue::BigInt(value["value"].as_str().unwrap().parse().unwrap()),
        "array" => JsJsonValue::Array(
            value["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(input)
                .collect(),
        ),
        "object" => JsJsonValue::Object(
            value["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| (v[0].as_str().unwrap().to_owned(), input(&v[1])))
                .collect(),
        ),
        _ => panic!("unclassified actual native input"),
    }
}
fn result(actual: Result<String, String>, expected: &Value, label: &str) {
    if expected["ok"] == true {
        assert_eq!(
            actual.unwrap(),
            expected["value"].as_str().unwrap(),
            "{label}"
        );
    } else {
        assert_eq!(
            actual.unwrap_err(),
            expected["message"].as_str().unwrap(),
            "{label}"
        );
    }
}

#[test]
fn hash_client_order_id_native() {
    for row in rows("hashes", 12) {
        let native = hash_client_order_id(&units(&row["units"]));
        let expected: Vec<u8> = row["bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| u8::try_from(v.as_u64().unwrap()).unwrap())
            .collect();
        assert_eq!(
            native.as_slice(),
            expected,
            "hashClientOrderId native digest bytes"
        );
    }
}
#[test]
fn hmac_sha256_native() {
    for row in rows("hmacs", 144) {
        assert_eq!(
            hmac_sha256_base64(&units(&row["message"]), &units(&row["key"])),
            row["output"].as_str().unwrap(),
            "hmacSha256Base64 native signature"
        );
    }
}
#[test]
fn serialize_primary_native() {
    for row in rows("queries", 13) {
        result(
            serialize_apex_payload(&input(&row["input"])),
            &row["primary"],
            "serializeApexPayload native result",
        );
    }
}
#[test]
fn serialize_legacy_native() {
    for row in rows("queries", 13) {
        result(
            serialize_apex_payload_legacy(&input(&row["input"])),
            &row["legacy"],
            "serializeApexPayloadLegacy native result",
        );
    }
}
#[test]
fn fee_helper_native() {
    for row in rows("fees", 180) {
        let actual = fee_helper(
            &Iso8601::String(row["input"].as_str().unwrap().to_owned()),
            Some(number(&row["digits"])),
            |value| {
                Ok(match row["operation"].as_str().unwrap() {
                    "ceil" => value.ceil(),
                    "floor" => value.floor(),
                    "round" => math_round(value),
                    _ => panic!("source Math operation"),
                })
            },
        );
        result(actual, &row["result"], "feeHelper native result");
    }
}

#[path = "support/surrogate_observability.rs"]
mod surrogate_observability;
