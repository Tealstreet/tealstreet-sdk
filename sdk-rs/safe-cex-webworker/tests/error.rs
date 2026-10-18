use safe_cex_exchange::js_runtime::value::{Js, error_object, from_json};
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
        let actual =
            serialize_worker_error_for_sentry(&from_json(&input), "fallback", None).unwrap();
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
        let actual =
            serialize_worker_error_for_sentry(&from_json(&input), "fallback", Some("")).unwrap();
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
fn live_typescript_preserves_native_error_diagnostics() {
    let output = std::process::Command::new("node")
        .args(["--input-type=module", "-e", r#"
import {loadStandalone} from './sdk-rs/safe-cex-webworker/tests/source_loader.mjs';
const {serializeWorkerErrorForSentry: serialize} = await loadStandalone('./packages/safe-cex/src/webworker/workerErrorSerialization.ts');
const error = new Error('worker failed'); error.stack = 'trace';
const dom = new DOMException('read failed', 'NotReadableError'); dom.stack = 'domtrace';
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
    let error = error_object("Error", "worker failed");
    error.set("stack", Js::str("trace")).unwrap();
    let rust = serialize_worker_error_for_sentry(&error, "fallback", None).unwrap();
    assert_eq!(
        serde_json::to_value(rust).unwrap(),
        values[0],
        "WORKER_ERROR_NATIVE hidden message and inherited name"
    );
    let globals = Js::object(vec![]);
    safe_cex_exchange::js_runtime::globals::install_builtins::<()>(&globals, std::rc::Weak::new());
    let object = globals.get("Object").unwrap();
    let inherited = object
        .call_method(
            "setPrototypeOf",
            vec![
                Js::object(vec![]),
                Js::object(vec![
                    ("message", Js::str("inherited")),
                    ("name", Js::str("InheritedError")),
                ]),
            ],
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(
            serialize_worker_error_for_sentry(&inherited, "fallback", None).unwrap()
        )
        .unwrap(),
        values[2]
    );
    let dom = object
        .call_method(
            "setPrototypeOf",
            vec![
                Js::object(vec![]),
                Js::object(vec![("name", Js::str("NotReadableError"))]),
            ],
        )
        .unwrap();
    for (key, value) in [("message", "read failed"), ("stack", "domtrace")] {
        object
            .call_method(
                "defineProperty",
                vec![
                    dom.clone(),
                    Js::str(key),
                    Js::object(vec![("value", Js::str(value))]),
                ],
            )
            .unwrap();
    }
    assert_eq!(
        serde_json::to_value(serialize_worker_error_for_sentry(&dom, "fallback", None).unwrap())
            .unwrap(),
        values[1]
    );
}

fn property_source() -> Value {
    let output = std::process::Command::new("node")
        .args(["--input-type=module", "-e", r#"
import {loadStandalone} from './sdk-rs/safe-cex-webworker/tests/source_loader.mjs';
const {serializeWorkerErrorForSentry: serialize} = await loadStandalone('./packages/safe-cex/src/webworker/workerErrorSerialization.ts');
const profiles = {};
for (const stop of ['none', 'message', 'name', 'stack']) {
  const trace = [], cause = {tag:'original'}, proto = {}, value = Object.create(proto);
  for (const key of ['message','name','stack']) Object.defineProperty(proto, key, {get() {
    trace.push([key, this === value]);
    if (key === stop) throw cause;
    return key + ': exact';
  }});
  try { profiles[stop] = {result:serialize(value,'fallback'), trace}; }
  catch (error) { profiles[stop] = {same:error === cause, trace}; }
}
const callable = function() {};
for (const key of ['message','name','stack']) Object.defineProperty(callable,key,{get(){throw Error('function getter reached')}});
profiles.callable = serialize(callable,'fallback');
const surrogate = {message:'m\ud800!',name:'n\udc00!',stack:'s\ud800\udc00!'};
profiles.surrogate = Object.fromEntries(Object.entries(serialize(surrogate,'fallback')).map(([key,value]) => [key, [...Array(value.length)].map((_,i)=>value.charCodeAt(i))]));
process.stdout.write(JSON.stringify(profiles));
"#])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn sentry_inherited_getters_preserve_receiver_order_and_original_failure() {
    use safe_cex_exchange::js_runtime::{
        globals::install_builtins,
        value::{Js, Thrown},
    };
    use std::{cell::RefCell, rc::Rc};
    let profiles = property_source();
    let globals = Js::object(vec![]);
    install_builtins::<()>(&globals, std::rc::Weak::new());
    let object = globals.get("Object").unwrap();
    for stop in ["none", "message", "name", "stack"] {
        let proto = Js::object(vec![]);
        let value = object
            .call_method("setPrototypeOf", vec![Js::object(vec![]), proto.clone()])
            .unwrap();
        let Js::Object(data) = &value else {
            panic!("Object.setPrototypeOf returned non-object")
        };
        let expected_receiver = Rc::downgrade(data);
        let cause = Js::object(vec![("tag", Js::str("original"))]);
        let trace = Rc::new(RefCell::new(Vec::new()));
        for key in ["message", "name", "stack"] {
            let trace = trace.clone();
            let cause = cause.clone();
            let expected_receiver = expected_receiver.clone();
            proto
                .define_accessor(
                    key,
                    Some(Rc::new(move |receiver| {
                        trace.borrow_mut().push(json!([
                            key,
                            receiver
                                .same_reference(&Js::Object(expected_receiver.upgrade().unwrap()))
                        ]));
                        if key == stop {
                            return Err(Thrown::new(cause.clone()));
                        }
                        Ok(Js::str(&format!("{key}: exact")))
                    })),
                    None,
                )
                .unwrap();
        }
        let result = serialize_worker_error_for_sentry(&value, "fallback", None);
        assert_eq!(
            json!(*trace.borrow()),
            profiles[stop]["trace"],
            "WORKER_ERROR_NATIVE getter trace {stop}"
        );
        if stop == "none" {
            assert_eq!(
                serde_json::to_value(result.unwrap()).unwrap(),
                profiles[stop]["result"]
            );
        } else {
            assert!(profiles[stop]["same"].as_bool().unwrap());
            assert!(
                result.unwrap_err().value.same_reference(&cause),
                "WORKER_ERROR_NATIVE arbitrary cause {stop}"
            );
        }
    }
    let callable = Js::function("ignored", |_, _| panic!("serializer called function"));
    for key in ["message", "name", "stack"] {
        callable
            .define_accessor(
                key,
                Some(Rc::new(|_| panic!("function property read"))),
                None,
            )
            .unwrap();
    }
    assert_eq!(
        serde_json::to_value(
            serialize_worker_error_for_sentry(&callable, "fallback", None).unwrap()
        )
        .unwrap(),
        profiles["callable"]
    );
}

#[test]
fn sentry_string_dto_replaces_only_isolated_surrogates() {
    use safe_cex_exchange::js_runtime::value::Js;
    let profiles = property_source();
    let error = Js::object(vec![]);
    for key in ["message", "name", "stack"] {
        let units = profiles["surrogate"][key]
            .as_array()
            .unwrap()
            .iter()
            .map(|unit| unit.as_u64().unwrap() as u16)
            .collect::<Vec<_>>();
        error.set(key, Js::String(units.into())).unwrap();
    }
    let actual = serialize_worker_error_for_sentry(&error, "fallback", None).unwrap();
    assert_eq!(actual.message, "m\u{fffd}!");
    assert_eq!(actual.name, "n\u{fffd}!");
    assert_eq!(actual.stack.as_deref(), Some("s\u{10000}!"));
    assert_eq!(
        error.get("message").unwrap().as_str().unwrap().as_ref(),
        &[109, 0xd800, 33]
    );
    assert_eq!(
        error.get("name").unwrap().as_str().unwrap().as_ref(),
        &[110, 0xdc00, 33]
    );
}
