use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_venue_phemex::types::{PhemexApiResponse, PositionsResponse};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::process::Command;

trait Capture {
    fn capture(&self) -> Value;
}
impl Capture for f64 {
    fn capture(&self) -> Value {
        let bits = if self.is_nan() {
            0x7ff8000000000000
        } else {
            self.to_bits()
        };
        json!(["number", format!("{bits:016x}")])
    }
}
impl Capture for String {
    fn capture(&self) -> Value {
        json!(["string", self])
    }
}
impl Capture for bool {
    fn capture(&self) -> Value {
        json!(["boolean", self])
    }
}
fn capture(value: &impl Capture) -> Value {
    value.capture()
}
include!("support/type_fields.rs");

#[test]
fn types_match_live_typescript_fields_and_numeric_ingress() {
    let oracle = Command::new("node")
        .arg(format!(
            "{}/tests/cardinality_oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .expect("execute one TypeScript corpus process");
    assert!(
        oracle.status.success(),
        "type oracle: {}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let corpus: Value = serde_json::from_slice(&oracle.stdout).unwrap();
    eprintln!("TypeScript numeric profile: {}", corpus["profile"]);
    assert_eq!(
        corpus["cases"].as_array().unwrap().len(),
        71,
        "PHEMEX-CARDINALITY source case total"
    );
    assert_eq!(
        corpus["envelopes"].as_array().unwrap().len(),
        74,
        "PHEMEX-CARDINALITY source envelope total"
    );
    let mut comparisons = 0;
    for case_index in 0..71 {
        let case = corpus["cases"]
            .get(case_index)
            .expect("PHEMEX-CARDINALITY fixed cases case");
        let raw = case["raw"].as_str().unwrap();
        let from_text: PositionsResponse = js_from_str(raw).unwrap();
        check_fields(&from_text, &case["expected"]);
        comparisons += 3;
        // Each independently captured ingress path must retain the same bits.
        let wire: Value = serde_json::from_str(raw).unwrap();
        check_fields(
            &js_from_value::<PositionsResponse>(wire.clone()).unwrap(),
            &case["expected"],
        );
        check_fields(
            &js_from_value_ref::<PositionsResponse>(&wire).unwrap(),
            &case["expected"],
        );
    }
    for case_index in 0..74 {
        let case = corpus["envelopes"]
            .get(case_index)
            .expect("PHEMEX-CARDINALITY fixed envelopes case");
        let raw = case["raw"].as_str().unwrap();
        let wire: Value = serde_json::from_str(raw).unwrap();
        let variants: [PhemexApiResponse<PositionsResponse>; 3] = [
            js_from_str(raw).expect("envelope text ingress"),
            js_from_value(wire.clone()).unwrap(),
            js_from_value_ref(&wire).unwrap(),
        ];
        for value in variants {
            assert_eq!(capture(&value.code), case["code"], "native envelope.code");
            assert_eq!(
                value.msg,
                case["msg"].as_str().unwrap(),
                "native envelope.msg"
            );
            check_fields(&value.payload, &case["expected"]);
            comparisons += 1;
        }
    }
    let supplements = corpus["supplement"].as_array().unwrap();
    assert_eq!(
        supplements.len(),
        3,
        "PHEMEX-CARDINALITY supplement case total"
    );
    let mut native_total = 0;
    for (case_index, count) in [0usize, 2, 3].into_iter().enumerate() {
        let case = supplements
            .get(case_index)
            .expect("PHEMEX-CARDINALITY fixed supplement case");
        let expected = case["expected"].as_array().unwrap();
        assert_eq!(
            case["count"].as_u64().unwrap() as usize,
            count,
            "PHEMEX-CARDINALITY source per-case count"
        );
        assert_eq!(
            expected.len(),
            count,
            "PHEMEX-CARDINALITY expected position count"
        );
        let wire: Value = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        assert_eq!(
            wire["positions"].as_array().unwrap().len(),
            count,
            "PHEMEX-CARDINALITY raw source position count"
        );
        let variants: [PositionsResponse; 3] = [
            js_from_str(case["raw"].as_str().unwrap()).unwrap(),
            js_from_value(wire.clone()).unwrap(),
            js_from_value_ref(&wire).unwrap(),
        ];
        for native in variants {
            assert_eq!(
                native.positions.len(),
                expected.len(),
                "PHEMEX-CARDINALITY expected/native positions"
            );
            native_total += native.positions.len();
            let mut case_comparisons = 0;
            for index in 0..count {
                let row = native
                    .positions
                    .get(index)
                    .expect("PHEMEX-CARDINALITY fixed native supplemental position");
                check_fields(
                    &PositionsResponse {
                        account: native.account.clone(),
                        positions: vec![row.clone()],
                    },
                    expected
                        .get(index)
                        .expect("PHEMEX-CARDINALITY fixed expected position"),
                );
                comparisons += 1;
                case_comparisons += 1;
            }
            assert_eq!(
                case_comparisons, count,
                "PHEMEX-CARDINALITY per-case position comparisons"
            );
        }
    }
    assert_eq!(
        comparisons, 450,
        "PHEMEX-CARDINALITY native comparison total"
    );
    assert_eq!(native_total, 15, "PHEMEX-CARDINALITY native position total");
    let base: Value = serde_json::from_str(corpus["cases"][0]["raw"].as_str().unwrap()).unwrap();
    let mut unknown = base.clone();
    unknown["account"]["futureField"] = json!("ignored");
    let value: PositionsResponse = js_from_value(unknown).unwrap();
    check_fields(&value, &corpus["cases"][0]["expected"]);
    for malformed in [
        json!([]),
        json!({"account":[],"positions":[]}),
        json!({"account":base["account"],"positions":[[]]}),
    ] {
        assert!(
            js_from_value::<PositionsResponse>(malformed).is_err(),
            "record arrays refuse"
        );
    }
    // Metadata-reserved payload boundary, deliberately narrower than T & {...}.
    #[derive(Deserialize, Serialize)]
    struct Overlap {
        code: f64,
    }
    impl JsNumberOutput for Overlap {
        fn check_number_output(&self) {
            safe_cex_num::js_number_output!(self, struct { code });
        }
    }
    assert!(
        js_from_str::<PhemexApiResponse<Overlap>>(r#"{"code":7,"msg":"ok"}"#).is_err(),
        "required metadata overlap refuses"
    );
    typed_utf16::assert_projection(
        corpus["surrogateRaw"].as_str().unwrap(),
        &(js_from_str::<PositionsResponse>(corpus["surrogateRaw"].as_str().unwrap()))
            .expect("typed surrogate record is retained"),
    );
    eprintln!(
        "{} response cases x 3 ingress paths; {} envelopes",
        corpus["cases"].as_array().unwrap().len(),
        corpus["envelopes"].as_array().unwrap().len()
    );
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
