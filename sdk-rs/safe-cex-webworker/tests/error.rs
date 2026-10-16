use safe_cex_webworker::error::serialize_worker_error_for_sentry;
use serde_json::{Value, json};

#[test]
fn sentry_plain_objects_preserve_nonempty_strings_without_coercion_or_trim() {
    for (input, expected) in [
        (
            json!({"message":"failure","name":"VenueError","stack":"trace","code":42}),
            json!({"message":"failure","name":"VenueError","stack":"trace"}),
        ),
        (
            json!({"message":"","name":42,"stack":false}),
            json!({"message":"fallback","name":"Error"}),
        ),
        (
            json!({"message":" \u{feff}","name":" ","stack":"\n"}),
            json!({"message":" \u{feff}","name":" ","stack":"\n"}),
        ),
        (
            json!({"message":["failure"],"name":null,"stack":""}),
            json!({"message":"fallback","name":"Error"}),
        ),
    ] {
        let actual = serialize_worker_error_for_sentry(&input, "fallback", None);
        assert_eq!(
            actual.message,
            expected["message"].as_str().unwrap(),
            "ERROR-IDENTITY native message"
        );
        assert_eq!(
            actual.name,
            expected["name"].as_str().unwrap(),
            "ERROR-IDENTITY native name"
        );
        assert_eq!(
            actual.stack.as_deref(),
            expected.get("stack").and_then(Value::as_str),
            "ERROR-IDENTITY native stack"
        );
        assert_eq!(serde_json::to_value(actual).unwrap(), expected);
    }
}

#[test]
fn sentry_primitives_and_explicit_empty_fallback_name() {
    for input in [
        Value::Null,
        json!(false),
        json!(42),
        json!("failure"),
        json!([]),
    ] {
        let actual = serialize_worker_error_for_sentry(&input, "fallback", Some(""));
        assert_eq!(
            actual.message, "fallback",
            "ERROR-IDENTITY primitive message"
        );
        assert_eq!(actual.name, "", "ERROR-IDENTITY primitive name");
        assert_eq!(actual.stack, None, "ERROR-IDENTITY primitive stack");
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            json!({"message":"fallback","name":""})
        );
    }
}

#[test]
fn live_typescript_pins_error_materialization_missing_at_json_boundary() {
    let output = std::process::Command::new("node")
        .args(["--input-type=module", "-e", r#"
import {loadStandalone} from './sdk-rs/safe-cex-webworker/tests/source_loader.mjs';
const {serializeWorkerErrorForSentry: serialize} = await loadStandalone('./packages/safe-cex/src/webworker/workerErrorSerialization.ts');
const error = new Error('worker failed'); error.stack = 'trace';
const dom = new DOMException('read failed', 'NotReadableError');
const inherited = Object.create({message:'inherited', name:'InheritedError'});
process.stdout.write(JSON.stringify([serialize(error, 'fallback'), serialize(dom, 'fallback'), serialize(inherited, 'fallback'), JSON.parse(JSON.stringify(error))]));
"#])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let values: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        values[0],
        json!({"message":"worker failed","name":"Error","stack":"trace"})
    );
    assert_eq!(values[1]["message"], "read failed");
    assert_eq!(values[1]["name"], "NotReadableError");
    assert_eq!(
        values[2],
        json!({"message":"inherited","name":"InheritedError"})
    );
    assert_eq!(values[3], json!({}));
    let rust = serialize_worker_error_for_sentry(&values[3], "fallback", None);
    assert_eq!(rust.message, "fallback");
}
