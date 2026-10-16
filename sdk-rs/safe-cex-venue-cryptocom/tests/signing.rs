use safe_cex_num::numeric_ingress::{JsNumberOutput, js_from_str};
use safe_cex_venue_cryptocom::signing::params_to_string;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::process::Command;

#[derive(Deserialize, Serialize)]
struct Input {
    #[serde(deserialize_with = "safe_cex_num::numeric_ingress::js_deserialize_passthrough")]
    value: Value,
}
impl JsNumberOutput for Input {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct {} passthrough { value })
    }
}

#[test]
fn live_params() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/signing_oracle.cjs"
        ))
        .output()
        .expect("one Node corpus");
    assert!(
        output.status.success(),
        "signing_source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).expect("corpus JSON");
    eprintln!(
        "signing capture {} Rust arch {}",
        corpus["header"],
        std::env::consts::ARCH
    );
    assert!(
        corpus["header"]["nodeMajor"].as_u64().is_some(),
        "capture_node_major"
    );
    assert!(
        corpus["header"]["architecture"].as_str().is_some(),
        "capture_architecture"
    );
    let names = [
        "missing",
        "null",
        "false",
        "true",
        "empty_string",
        "unicode",
        "negative_zero",
        "rounded_integer",
        "overflow",
        "negative_overflow",
        "subnormal",
        "fixed",
        "exponential",
        "empty_array",
        "array",
        "nested_array",
        "empty_object",
        "sorted_keys",
        "integer_keys",
        "utf16_keys",
        "prototype_keys",
        "depth_object",
        "depth_array",
        "coercion_error",
        "array_coercion_error",
        "duplicates",
        "empty_key",
        "number_array",
    ];
    let levels = [
        None,
        Some("-1"),
        Some("0"),
        Some("2"),
        Some("3"),
        Some("4"),
        Some("NaN"),
        Some("Infinity"),
        Some("-Infinity"),
        Some("2.5"),
    ];
    let rows = corpus["rows"].as_array().expect("source rows");
    assert_eq!(rows.len(), 280, "params_source_count");
    let admission: Value =
        serde_json::from_str(include_str!("signing_input_inventory.json")).unwrap();
    assert_eq!(
        admission.as_array().unwrap().len(),
        280,
        "params_inventory_count"
    );
    let mut comparisons = 0;
    let mut outcomes = [0; 2];
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(
            row.get("raw").expect("params_raw_presence"),
            admission[index].get("raw").unwrap(),
            "params_input_content"
        );
        assert_eq!(row["name"], names[index / 10], "params_case_identity");
        assert_eq!(
            row.get("level").expect("params_level_presence").as_str(),
            levels[index % 10],
            "params_level_identity"
        );
        let input: Option<Input> = row["raw"].as_str().map(|raw| {
            js_from_str(&format!("{{\"value\":{raw}}}")).expect("retained numeric ingress")
        });
        let level = row["level"]
            .as_str()
            .map(|level| level.parse::<f64>().expect("numeric level"));
        let native = params_to_string(input.as_ref().map(|input| &input.value), level);
        let error_expected = (index / 10 == 20 && matches!(index % 10, 4 | 5 | 7))
            || (index / 10 == 24 && matches!(index % 10, 0 | 2 | 3 | 4 | 5 | 7 | 9));
        if !error_expected {
            let expected = row.get("value").expect("params_source_value_presence");
            assert!(row.get("error").is_none(), "params_source_success");
            outcomes[0] += 1;
            assert_eq!(
                native.as_ref().expect("source succeeded"),
                expected.as_str().unwrap(),
                "params_native_value:{}:{:?}",
                names[index / 10],
                levels[index % 10]
            );
        } else {
            outcomes[1] += 1;
            assert!(row.get("error").is_some(), "params_source_error_presence");
            let error = native.expect_err("source error");
            assert_eq!(error.name, row["error"][0], "params_native_error_name");
            assert_eq!(
                error.message, row["error"][1],
                "params_native_error_message"
            );
        }
        comparisons += 1;
    }
    assert_eq!(comparisons, 280, "params_comparisons_executed");
    assert_eq!(outcomes, [270, 10], "params_outcome_counts");
    let excluded = &corpus["excluded"];
    assert_eq!(
        excluded["nestedUndefined"], "aundefined",
        "domain_nested_undefined"
    );
    assert_eq!(excluded["bigint"], "123", "domain_bigint");
    assert_eq!(excluded["symbol"], "Symbol(x)", "domain_symbol");
    assert_eq!(excluded["hole"], "last", "domain_hole");
    assert_eq!(excluded["getter"], "agetter", "domain_getter");
    assert_eq!(excluded["getterCalls"], 1, "domain_getter_calls");
    assert_eq!(excluded["customPrototype"], "owndata", "domain_prototype");
    assert_eq!(
        excluded["callable"], "function witness() {}",
        "domain_callable"
    );
    assert_eq!(excluded["surrogateCode"], 0xd800, "domain_utf16");
    assert!(
        js_from_str::<Input>(r#"{"value":"\ud800"}"#).is_err(),
        "domain_utf8_refusal"
    );
}
