#[path = "support/depth_corpus.rs"]
mod depth_corpus;
use depth_corpus::DepthCorpus;
use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_webworker::payload::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

fn number(v: &Value) -> f64 {
    assert!(v.is_number());
    v.as_f64()
        .unwrap_or_else(|| safe_cex_num::number_from_string(&v.to_string()))
}
fn repaired<T: DeserializeOwned + Serialize + JsNumberOutput>(
    source: &Value,
    name: &str,
    template: &str,
    read: impl Fn(&T) -> f64,
) -> usize {
    fn native_zeros(value: &mut Value) {
        match value {
            Value::Number(n)
                if n.as_f64() == Some(-0.0) && n.as_f64().unwrap().is_sign_negative() =>
            {
                *value = Value::from(-0.0)
            }
            Value::Array(a) => a.iter_mut().for_each(native_zeros),
            Value::Object(o) => o.values_mut().for_each(native_zeros),
            _ => {}
        }
    }
    let mut observations = 0;
    for (token, expected) in source.as_object().unwrap() {
        let bits = u64::from_str_radix(expected.as_str().unwrap(), 16).unwrap();
        let raw = template.replace("-0", token);
        let value: Value = serde_json::from_str(&raw).unwrap();
        let mut native = value.clone();
        native_zeros(&mut native);
        let routes: [(&str, Result<T, serde_json::Error>); 9] = [
            ("bare raw str", serde_json::from_str(&raw)),
            ("bare raw slice", serde_json::from_slice(raw.as_bytes())),
            ("bare owned Value", serde_json::from_value(value.clone())),
            ("bare borrowed Value", T::deserialize(&value)),
            ("shared raw", js_from_str(&raw)),
            ("shared owned Value", js_from_value(value.clone())),
            ("shared borrowed Value", js_from_value_ref(&value)),
            (
                "owned Value with native negative zero",
                serde_json::from_value(native.clone()),
            ),
            (
                "borrowed Value with native negative zero",
                T::deserialize(&native),
            ),
        ];
        for (route, decoded) in routes {
            assert!(
                decoded.is_ok(),
                "FIXTURE-NUMERIC ACCEPTANCE {name} {route} {token}: {}",
                decoded.err().map(|e| e.to_string()).unwrap_or_default()
            );
            assert_eq!(
                read(&decoded.unwrap()).to_bits(),
                bits,
                "FIXTURE-NUMERIC BITS {name} {route} {token}"
            );
            observations += 1;
        }
    }
    observations
}
#[test]
fn record_numeric_ingress() {
    let output = std::process::Command::new("node").args(["-e", r#"
const bytes=new DataView(new ArrayBuffer(8));
process.stdout.write(JSON.stringify(Object.fromEntries(['-0','-0.0','0','1e400','-1e400','1e-400','-1e-400','1.25','1e3','-1e-4000'].map(token=>{bytes.setFloat64(0,JSON.parse(token));return [token,bytes.getBigUint64(0).toString(16).padStart(16,'0')];}))));
"#]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut observations = 0;
    observations += repaired::<PerfFixtureFrame>(
        &source,
        "PerfFixtureFrame.atMs",
        r#"{"atMs":-0,"msg":{}}"#,
        |v| v.at_ms,
    );
    observations += repaired::<PerfFixtureFrame>(
        &source,
        "PerfFixtureFrame.msg",
        r#"{"atMs":0,"msg":{"x":-0}}"#,
        |v| number(&v.msg["x"]),
    );
    observations += repaired::<PerfFixture>(
        &source,
        "PerfFixture.recordedAtMs",
        r#"{"version":1,"recordedAtMs":-0,"durationMs":0,"frames":[]}"#,
        |v| v.recorded_at_ms,
    );
    observations += repaired::<PerfFixture>(
        &source,
        "PerfFixture.durationMs",
        r#"{"version":1,"recordedAtMs":0,"durationMs":-0,"frames":[]}"#,
        |v| v.duration_ms,
    );
    observations += repaired::<PerfFixture>(
        &source,
        "PerfFixture.frames.atMs",
        r#"{"version":1,"recordedAtMs":0,"durationMs":0,"frames":[{"atMs":-0,"msg":{}}]}"#,
        |v| v.frames[0].at_ms,
    );
    observations += repaired::<PerfFixture>(
        &source,
        "PerfFixture.frames.msg",
        r#"{"version":1,"recordedAtMs":0,"durationMs":0,"frames":[{"atMs":0,"msg":{"x":-0}}]}"#,
        |v| number(&v.frames[0].msg["x"]),
    );
    observations += repaired::<PerfFixtureConfig>(
        &source,
        "PerfFixtureConfig.intervalMs",
        r#"{"mode":"off","intervalMs":-0}"#,
        |v| v.interval_ms.unwrap(),
    );
    observations += repaired::<PerfFixtureConfig>(
        &source,
        "PerfFixtureConfig.fixture.frames.atMs",
        r#"{"mode":"replay","fixture":{"version":1,"recordedAtMs":0,"durationMs":0,"frames":[{"atMs":-0,"msg":{}}]}}"#,
        |v| v.fixture.as_ref().unwrap().frames[0].at_ms,
    );
    observations += repaired::<PerfFixtureConfig>(
        &source,
        "PerfFixtureConfig.fixture.frames.msg",
        r#"{"mode":"replay","fixture":{"version":1,"recordedAtMs":0,"durationMs":0,"frames":[{"atMs":0,"msg":{"x":-0}}]}}"#,
        |v| number(&v.fixture.as_ref().unwrap().frames[0].msg["x"]),
    );
    assert_eq!(observations, 810, "FIXTURE-NUMERIC path inventory");
    let mut envelope_observations = 0;
    envelope_observations += repaired::<WorkerConfig>(
        &source,
        "WorkerConfig.tickIntervalMultipliers",
        r#"{"tickIntervalMultipliers":{"x":-0}}"#,
        |v| v.tick_interval_multipliers.as_ref().unwrap()["x"],
    );
    envelope_observations += repaired::<WorkerConfig>(
        &source,
        "WorkerConfig.perfFixture.intervalMs",
        r#"{"perfFixture":{"mode":"off","intervalMs":-0}}"#,
        |v| v.perf_fixture.as_ref().unwrap().interval_ms.unwrap(),
    );
    envelope_observations += repaired::<WorkerConfig>(
        &source,
        "WorkerConfig.perfFixture.fixture.frames.atMs",
        r#"{"perfFixture":{"mode":"replay","fixture":{"version":1,"recordedAtMs":0,"durationMs":0,"frames":[{"atMs":-0,"msg":{}}]}}}"#,
        |v| {
            v.perf_fixture
                .as_ref()
                .unwrap()
                .fixture
                .as_ref()
                .unwrap()
                .frames[0]
                .at_ms
        },
    );
    envelope_observations += repaired::<WorkerConfig>(
        &source,
        "WorkerConfig.perfFixture.fixture.frames.msg",
        r#"{"perfFixture":{"mode":"replay","fixture":{"version":1,"recordedAtMs":0,"durationMs":0,"frames":[{"atMs":0,"msg":{"x":-0}}]}}}"#,
        |v| {
            number(
                &v.perf_fixture
                    .as_ref()
                    .unwrap()
                    .fixture
                    .as_ref()
                    .unwrap()
                    .frames[0]
                    .msg["x"],
            )
        },
    );
    envelope_observations += repaired::<StoreWorkerPayload>(
        &source,
        "StoreWorkerPayload.data",
        r#"{"op":"store","data":{"x":-0}}"#,
        |v| number(&v.data.as_ref().unwrap()["x"]),
    );
    assert_eq!(
        envelope_observations, 450,
        "ENVELOPE-NUMERIC path inventory"
    );
    let native = json!(-0.0);
    assert_eq!(native.as_f64().unwrap().to_bits(), (-0.0_f64).to_bits());
}

#[test]
fn fixture_frame_owned_integer_zero_is_preserved() {
    let value: Value = serde_json::from_str(r#"{"atMs":-0,"msg":{}}"#).unwrap();
    let frame: PerfFixtureFrame = serde_json::from_value(value).unwrap();
    assert_eq!(
        frame.at_ms.to_bits(),
        (-0.0_f64).to_bits(),
        "FRAME OWNED INTEGER ZERO"
    );
}
#[test]
fn fixture_raw_nested_integer_zero_is_preserved() {
    let fixture: PerfFixture = serde_json::from_str(
        r#"{"version":1,"recordedAtMs":0,"durationMs":0,"frames":[{"atMs":-0,"msg":{}}]}"#,
    )
    .unwrap();
    assert_eq!(
        fixture.frames[0].at_ms.to_bits(),
        (-0.0_f64).to_bits(),
        "FIXTURE RAW NESTED INTEGER ZERO"
    );
}
#[test]
fn fixture_config_owned_integer_zero_is_preserved() {
    let value: Value = serde_json::from_str(r#"{"mode":"off","intervalMs":-0}"#).unwrap();
    let config: PerfFixtureConfig = serde_json::from_value(value).unwrap();
    assert_eq!(
        config.interval_ms.unwrap().to_bits(),
        (-0.0_f64).to_bits(),
        "FIXTURE CONFIG OWNED INTEGER ZERO"
    );
}

#[test]
fn fixture_ingress_retains_the_shared_chosen_depth_boundary() {
    fn check<T: DeserializeOwned + Serialize + JsNumberOutput>(
        input: Value,
        depth: usize,
        pointer: &str,
        corpus: &mut DepthCorpus,
    ) {
        let raw = serde_json::to_string(&input).unwrap();
        corpus.record(&input, pointer, depth);
        for (path, result) in [
            ("bare owned", serde_json::from_value::<T>(input.clone())),
            ("bare borrowed", T::deserialize(&input)),
            ("shared raw", js_from_str::<T>(&raw)),
            ("shared owned", js_from_value::<T>(input.clone())),
            ("shared borrowed", js_from_value_ref::<T>(&input)),
        ] {
            if depth == 20 {
                assert!(
                    result.is_ok(),
                    "FIXTURE-NUMERIC ordinary nested input {path}"
                );
            } else {
                assert!(
                    result.is_err(),
                    "FIXTURE-NUMERIC shared depth refusal {path}"
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
        let mut msg = json!(1);
        for _ in 0..depth {
            msg = json!([msg]);
        }
        let frame = json!({"atMs":0,"msg":msg});
        let fixture = json!({"version":1,"recordedAtMs":0,"durationMs":0,"frames":[frame.clone()]});
        check::<PerfFixtureFrame>(frame, depth, "/msg", &mut corpus);
        check::<PerfFixture>(fixture.clone(), depth, "/frames/0/msg", &mut corpus);
        check::<PerfFixtureConfig>(
            json!({"mode":"replay","fixture":fixture}),
            depth,
            "/fixture/frames/0/msg",
            &mut corpus,
        );
    }
    corpus.verify();
}

#[test]
fn worker_config_owned_integer_zero_is_preserved() {
    let value: Value = serde_json::from_str(r#"{"tickIntervalMultipliers":{"x":-0}}"#).unwrap();
    let config: WorkerConfig = serde_json::from_value(value).unwrap();
    assert_eq!(
        config.tick_interval_multipliers.unwrap()["x"].to_bits(),
        (-0.0_f64).to_bits(),
        "WORKER CONFIG OWNED INTEGER ZERO"
    );
}
#[test]
fn worker_payload_raw_integer_zero_is_preserved() {
    let payload: StoreWorkerPayload =
        serde_json::from_str(r#"{"op":"store","data":{"x":-0}}"#).unwrap();
    assert_eq!(
        number(&payload.data.unwrap()["x"]).to_bits(),
        (-0.0_f64).to_bits(),
        "WORKER PAYLOAD RAW INTEGER ZERO"
    );
}

#[test]
fn envelope_numeric_presence_and_shared_depth_boundary() {
    fn check<T: DeserializeOwned + Serialize + JsNumberOutput>(
        input: Value,
        depth: usize,
        pointer: &str,
        corpus: &mut DepthCorpus,
    ) {
        let raw = serde_json::to_string(&input).unwrap();
        corpus.record(&input, pointer, depth);
        for (path, result) in [
            ("bare owned", serde_json::from_value::<T>(input.clone())),
            ("bare borrowed", T::deserialize(&input)),
            ("shared raw", js_from_str::<T>(&raw)),
            ("shared owned", js_from_value::<T>(input.clone())),
            ("shared borrowed", js_from_value_ref::<T>(&input)),
        ] {
            if depth == 20 {
                assert!(result.is_ok(), "ENVELOPE-NUMERIC ordinary nested {path}");
            } else {
                assert!(
                    result.is_err(),
                    "ENVELOPE-NUMERIC shared depth refusal {path}"
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
        let mut value = json!(1);
        for _ in 0..depth {
            value = json!([value]);
        }
        check::<WorkerConfig>(
            json!({"unknown":value.clone()}),
            depth,
            "/unknown",
            &mut corpus,
        );
        check::<StoreWorkerPayload>(
            json!({"op":"store","data":value}),
            depth,
            "/data",
            &mut corpus,
        );
    }
    corpus.verify();
    for raw in [r#"{"op":"store"}"#, r#"{"op":"store","data":null}"#] {
        let value: Value = serde_json::from_str(raw).unwrap();
        let expected = value.get("data").cloned();
        for decoded in [
            serde_json::from_str::<StoreWorkerPayload>(raw),
            serde_json::from_value(value.clone()),
            StoreWorkerPayload::deserialize(&value),
            js_from_str(raw),
            js_from_value(value.clone()),
            js_from_value_ref(&value),
        ] {
            assert_eq!(
                decoded.unwrap().data,
                expected,
                "ENVELOPE-NUMERIC absent versus explicit null"
            );
        }
    }
}
