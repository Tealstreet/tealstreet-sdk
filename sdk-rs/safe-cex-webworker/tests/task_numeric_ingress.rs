#[path = "support/depth_corpus.rs"]
mod depth_corpus;
use depth_corpus::DepthCorpus;
use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_webworker::task_types::{ChaserOrderState, ChaserTerminate};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[test]
fn terminate_owned_integer_zero_is_preserved() {
    let input: Value =
        serde_json::from_str(r#"{"type":"price","value":-0,"anchorPrice":0}"#).unwrap();
    let output: ChaserTerminate = serde_json::from_value(input).unwrap();
    assert_eq!(
        output.value.to_bits(),
        (-0.0_f64).to_bits(),
        "TERMINATE OWNED INTEGER ZERO"
    );
}
#[test]
fn state_owned_integer_zero_is_preserved() {
    let input: Value = serde_json::from_str(
        r#"{"attempts":-0,"isStopped":false,"stopReason":"","currentOrdersIds":[]}"#,
    )
    .unwrap();
    let output: ChaserOrderState = serde_json::from_value(input).unwrap();
    assert_eq!(
        output.attempts.to_bits(),
        (-0.0_f64).to_bits(),
        "CHASER STATE OWNED INTEGER ZERO"
    );
}

fn check_numeric<T: DeserializeOwned + Serialize + JsNumberOutput>(
    source: &Value,
    raw: &str,
    field: &str,
    read: impl Fn(&T) -> f64,
) -> usize {
    let mut count = 0;
    for (token, bits) in source.as_object().unwrap() {
        let expected = u64::from_str_radix(bits.as_str().unwrap(), 16).unwrap();
        let raw = raw.replace("-0", token);
        let value: Value = serde_json::from_str(&raw).unwrap();
        let mut native = value.clone();
        if expected == (-0.0_f64).to_bits() {
            native[field] = json!(-0.0);
        }
        for (route, output) in [
            ("bare raw", serde_json::from_str::<T>(&raw)),
            ("bare slice", serde_json::from_slice(raw.as_bytes())),
            ("bare owned", serde_json::from_value(value.clone())),
            ("bare borrowed", T::deserialize(&value)),
            ("shared raw", js_from_str(&raw)),
            ("shared owned", js_from_value(value.clone())),
            ("shared borrowed", js_from_value_ref(&value)),
            (
                "owned native negative zero",
                serde_json::from_value(native.clone()),
            ),
            ("borrowed native negative zero", T::deserialize(&native)),
        ] {
            assert!(
                output.is_ok(),
                "TASK-NUMERIC ACCEPTANCE {} {field} {token} {route}: {}",
                std::any::type_name::<T>(),
                output.err().map(|e| e.to_string()).unwrap_or_default()
            );
            assert_eq!(
                read(&output.unwrap()).to_bits(),
                expected,
                "TASK-NUMERIC BITS {} {field} {token} {route}",
                std::any::type_name::<T>()
            );
            count += 1;
        }
    }
    count
}
#[test]
fn primitive_task_numeric_ingress_matches_live_node_bits() {
    let output=std::process::Command::new("node").args(["-e",r#"
const b=new DataView(new ArrayBuffer(8));
process.stdout.write(JSON.stringify(Object.fromEntries(['-0','-0.0','0','1e400','-1e400','1e-400','-1e-400','1.25','1e3','-1e-4000'].map(token=>{b.setFloat64(0,JSON.parse(token));return[token,b.getBigUint64(0).toString(16).padStart(16,'0')]}))));
"#]).output().unwrap();
    assert!(output.status.success());
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut n = 0;
    n += check_numeric::<ChaserTerminate>(
        &source,
        r#"{"type":"price","value":-0,"anchorPrice":0}"#,
        "value",
        |v| v.value,
    );
    n += check_numeric::<ChaserTerminate>(
        &source,
        r#"{"type":"percent","value":0,"anchorPrice":-0}"#,
        "anchorPrice",
        |v| v.anchor_price,
    );
    n += check_numeric::<ChaserOrderState>(
        &source,
        r#"{"attempts":-0,"isStopped":false,"stopReason":"","currentOrdersIds":[]}"#,
        "attempts",
        |v| v.attempts,
    );
    assert_eq!(n, 270, "TASK-NUMERIC field/path inventory");
}
#[test]
fn primitive_task_shared_depth_boundary() {
    fn check<T: DeserializeOwned + Serialize + JsNumberOutput>(
        mut value: Value,
        depth: usize,
        corpus: &mut DepthCorpus,
    ) {
        let mut extra = json!(1);
        for _ in 0..depth {
            extra = json!([extra]);
        }
        value["ignored"] = extra;
        let raw = serde_json::to_string(&value).unwrap();
        corpus.record(&value, "/ignored", depth);
        for (path, result) in [
            ("bare owned", serde_json::from_value::<T>(value.clone())),
            ("bare borrowed", T::deserialize(&value)),
            ("shared raw", js_from_str::<T>(&raw)),
            ("shared owned", js_from_value::<T>(value.clone())),
            ("shared borrowed", js_from_value_ref::<T>(&value)),
        ] {
            if depth == 20 {
                assert!(
                    result.is_ok(),
                    "TASK-NUMERIC ordinary depth {} {path}",
                    std::any::type_name::<T>()
                );
            } else {
                assert!(
                    result.is_err(),
                    "TASK-NUMERIC shared depth refusal {} {path}",
                    std::any::type_name::<T>()
                );
                assert!(
                    result
                        .err()
                        .unwrap()
                        .to_string()
                        .contains("recursion limit")
                );
            }
        }
    }
    let mut corpus = DepthCorpus::default();
    for depth in [140, 20] {
        check::<ChaserTerminate>(
            json!({"type":"price","value":0,"anchorPrice":0}),
            depth,
            &mut corpus,
        );
        check::<ChaserOrderState>(
            json!({"attempts":0,"isStopped":false,"stopReason":"","currentOrdersIds":[]}),
            depth,
            &mut corpus,
        );
    }
    corpus.verify();
}
