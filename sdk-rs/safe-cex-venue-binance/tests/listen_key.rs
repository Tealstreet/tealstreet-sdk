use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_binance::listen_key::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::{Future, ready},
    pin::pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn run<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    for _ in 0..3 {
        if let Poll::Ready(result) = future.as_mut().poll(&mut context) {
            return result;
        }
    }
    panic!("authored callback future did not finish within three polls");
}
fn original() -> Rc<ExchangeError> {
    let mut e = ExchangeError::new("fixture error");
    e.name = "RangeError".into();
    e.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "original-code"
    )));
    Rc::new(e)
}
fn failure_fields<T>(
    result: &Result<T, KeepaliveFailure>,
    expected: Option<&str>,
    original: &Rc<ExchangeError>,
) -> (Value, Value, Value) {
    match result {
        Ok(_) => {
            assert_eq!(expected, None, "native expected failure");
            (Value::Null, Value::Null, Value::Null)
        }
        Err(KeepaliveFailure::Error(e)) => {
            if expected == Some("original") {
                assert!(
                    Rc::ptr_eq(e, original),
                    "LISTEN native original error identity"
                );
                assert_eq!(
                    observed_error_code(&e.code),
                    Some(json!("original-code")),
                    "native error metadata"
                );
            } else {
                assert_eq!(expected, Some("generated"));
                assert_eq!(e.name, "TypeError");
            }
            (json!(expected.unwrap()), json!(e.name), json!(e.message))
        }
        Err(KeepaliveFailure::Other(_)) => panic!("unexpected escaped non-Error"),
    }
}
fn source() -> Vec<Value> {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/listen-key.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        rows.len(),
        70,
        "independently authored source floor 2*(14+21)"
    );
    rows
}
fn compare_source(variant: &str, method: &str, native: Vec<Value>) {
    let rows: Vec<_> = source()
        .into_iter()
        .filter(|r| r["variant"] == variant && r["method"] == method)
        .collect();
    assert_eq!(rows.len(), if method == "fetch" { 14 } else { 21 });
    for (index, (mut actual, expected)) in rows.into_iter().zip(native).enumerate() {
        let source_trace = actual["trace"].as_array_mut().unwrap();
        let native_trace = expected["trace"].as_array().unwrap();
        assert_eq!(
            source_trace.len(),
            native_trace.len(),
            "source effect count"
        );
        for (source_effect, native_effect) in source_trace.iter_mut().zip(native_trace) {
            if source_effect[0] == "schedule" && native_effect[0] == "schedule" {
                assert_eq!(
                    source_effect[1].as_f64().unwrap().to_bits(),
                    native_effect[1].as_f64().unwrap().to_bits(),
                    "LISTEN {variant} {method} source delay case {index}"
                );
                // Numeric values were decoded and compared above. Align only
                // serde's integer/float spelling before structural comparison.
                source_effect[1] = native_effect[1].clone();
            }
        }
        assert_eq!(
            actual, expected,
            "LISTEN {variant} {method} source case {index}"
        );
    }
}
fn fetch(variant: &str) {
    let mut native = Vec::new();
    let names = [
        "normal",
        "alias-mutated",
        "key-undefined",
        "key-null",
        "key-object",
        "response-undefined",
        "response-null",
        "data-undefined",
        "data-null",
        "data-primitive",
        "response-primitive",
        "post-sync-fail",
        "post-reject",
        "schedule-fail",
    ];
    let endpoint = if variant == "binance" {
        "/fapi/v1/listenKey"
    } else {
        "/papi/v1/listenKey"
    };
    for (index, name) in names.into_iter().enumerate() {
        let trace = RefCell::new(Vec::<Value>::new());
        let delays = RefCell::new(Vec::<f64>::new());
        let error = original();
        let first = Rc::new(json!({"id":11}));
        let second = Rc::new(json!({"id":22}));
        let key = Rc::new(RefCell::new(match name {
            "key-undefined" | "data-primitive" => None,
            "key-null" => Some(Rc::new(Value::Null)),
            _ => Some(first.clone()),
        }));
        let response = match name {
            "response-undefined" => ListenKeyResponse::Undefined,
            "response-null" => ListenKeyResponse::Null,
            "data-undefined" | "response-primitive" => {
                ListenKeyResponse::Data(ListenKeyData::Undefined)
            }
            "data-null" => ListenKeyResponse::Data(ListenKeyData::Null),
            _ => ListenKeyResponse::Data(ListenKeyData::Present(key.clone())),
        };
        let post = |path| {
            assert_eq!(
                path, endpoint,
                "LISTEN {variant} fetch native endpoint case {index}"
            );
            trace.borrow_mut().push(json!(["post", path]));
            if name == "post-sync-fail" {
                return Err(KeepaliveFailure::Error(error.clone()));
            }
            Ok(ready(if name == "post-reject" {
                Err(KeepaliveFailure::Error(error.clone()))
            } else {
                Ok(response)
            }))
        };
        let schedule = |delay| {
            delays.borrow_mut().push(delay);
            trace.borrow_mut().push(json!(["schedule", delay]));
            if name == "alias-mutated" {
                *key.borrow_mut() = Some(second.clone());
            }
            if name == "schedule-fail" {
                Err(KeepaliveFailure::Error(error.clone()))
            } else {
                Ok(())
            }
        };
        let result = if variant == "binance" {
            run(fetch_binance_listen_key(post, schedule))
        } else {
            run(fetch_binance_pm_listen_key(post, schedule))
        };
        let before_schedule = [
            "response-undefined",
            "response-null",
            "post-sync-fail",
            "post-reject",
        ]
        .contains(&name);
        assert_eq!(
            *delays.borrow(),
            if before_schedule {
                vec![]
            } else {
                vec![1_800_000.0]
            },
            "LISTEN {variant} fetch native delay case {index}"
        );
        assert_eq!(
            trace.borrow().len(),
            if before_schedule { 1 } else { 2 },
            "native independent operation count"
        );
        let expected_failure = match name {
            "response-undefined" | "response-null" | "data-undefined" | "data-null"
            | "response-primitive" => Some("generated"),
            "post-sync-fail" | "post-reject" | "schedule-fail" => Some("original"),
            _ => None,
        };
        if expected_failure == Some("generated") {
            let message = match name {
                "response-undefined" => {
                    "Cannot destructure property 'data' of '(intermediate value)' as it is undefined."
                }
                "response-null" => {
                    "Cannot destructure property 'data' of '(intermediate value)' as it is null."
                }
                "data-null" => "Cannot read properties of null (reading 'listenKey')",
                _ => "Cannot read properties of undefined (reading 'listenKey')",
            };
            let Err(KeepaliveFailure::Error(native_error)) = &result else {
                panic!("expected native TypeError");
            };
            assert_eq!(
                native_error.message, message,
                "native nullish-container error message"
            );
        }
        let (failure, error_name, error_message) =
            failure_fields(&result, expected_failure, &error);
        let mut identity = Value::Null;
        let wire_key = match &result {
            Ok(None) => {
                assert!(["key-undefined", "data-primitive"].contains(&name));
                json!("undefined")
            }
            Ok(Some(value)) => {
                if name == "key-null" {
                    assert!(value.is_null(), "native explicit null key");
                } else {
                    let expected = if name == "alias-mutated" {
                        &second
                    } else {
                        &first
                    };
                    assert!(
                        Rc::ptr_eq(value, expected),
                        "LISTEN {variant} fetch native key identity case {index}"
                    );
                    assert_eq!(
                        value["id"].as_u64(),
                        Some(if name == "alias-mutated" { 22 } else { 11 }),
                        "native decoded key value"
                    );
                    identity = json!(true);
                }
                value.as_ref().clone()
            }
            Err(_) => json!("undefined"),
        };
        native.push(json!({"variant":variant,"method":"fetch","name":name,"trace":*trace.borrow(),"key":wire_key,"identity":identity,"failure":failure,"errorName":error_name,"errorMessage":error_message}));
    }
    assert_eq!(
        native.len(),
        14,
        "native authored fetch floor; stopped schedule-fail"
    );
    compare_source(variant, "fetch", native);
}
fn update(variant: &str) {
    let mut native = Vec::new();
    let names = [
        "disposed",
        "normal",
        "recover",
        "disposed-during-put",
        "flag-during-put",
        "put-sync",
        "put-reject",
        "repeat-error",
        "schedule-success-fail",
        "warning-fail",
        "emit-fail",
        "retry-fail",
        "undefined-error",
        "null-error",
        "number-error",
        "string-error",
        "array-error",
        "object-error",
        "coercion-fail",
        "logger-changes-flag",
        "schedule-changes-flag",
    ];
    let endpoint = if variant == "binance" {
        "/fapi/v1/listenKey"
    } else {
        "/papi/v1/listenKey"
    };
    for (index, name) in names.into_iter().enumerate() {
        let state = KeepaliveState {
            is_disposed: Cell::new(name == "disposed").into(),
            keepalive_failing: Cell::new(
                ["recover", "repeat-error", "warning-fail"].contains(&name),
            ),
        };
        let trace = RefCell::new(Vec::<Value>::new());
        let delays = RefCell::new(Vec::<f64>::new());
        let observed = RefCell::new(Vec::<bool>::new());
        let messages = RefCell::new(Vec::<(bool, String)>::new());
        let error = original();
        let thrown = match name {
            "undefined-error" => KeepaliveFailure::Other(None),
            "null-error" => KeepaliveFailure::Other(Some(Rc::new(Value::Null))),
            "number-error" => KeepaliveFailure::Other(Some(Rc::new(json!(42)))),
            "string-error" => KeepaliveFailure::Other(Some(Rc::new(json!("wire failure")))),
            "array-error" => KeepaliveFailure::Other(Some(Rc::new(json!([1, null, "x"])))),
            "object-error" => KeepaliveFailure::Other(Some(Rc::new(json!({"code":7})))),
            "coercion-fail" => KeepaliveFailure::Other(Some(Rc::new(json!({"toString":0})))),
            _ => KeepaliveFailure::Error(error.clone()),
        };
        let success = [
            "disposed",
            "normal",
            "recover",
            "disposed-during-put",
            "schedule-success-fail",
            "schedule-changes-flag",
        ]
        .contains(&name);
        let polls = Cell::new(0);
        let put = |path| {
            assert_eq!(
                path, endpoint,
                "LISTEN {variant} update native endpoint case {index}"
            );
            trace.borrow_mut().push(json!(["put", path]));
            if name == "put-sync" {
                return Err(thrown.clone());
            }
            Ok(std::future::poll_fn(|_| {
                polls.set(polls.get() + 1);
                if polls.get() == 1 {
                    return Poll::Pending;
                }
                if name == "disposed-during-put" {
                    state.is_disposed.set(true);
                }
                if name == "flag-during-put" {
                    state.keepalive_failing.set(true);
                }
                Poll::Ready(if success { Ok(()) } else { Err(thrown.clone()) })
            }))
        };
        let schedule = |delay| {
            delays.borrow_mut().push(delay);
            trace.borrow_mut().push(json!(["schedule", delay]));
            observed.borrow_mut().push(state.keepalive_failing.get());
            if (name == "schedule-success-fail" && delay == 1_800_000.0)
                || (name == "retry-fail" && delay == 60_000.0)
            {
                return Err(KeepaliveFailure::Error(error.clone()));
            }
            if name == "schedule-changes-flag" {
                state.keepalive_failing.set(true);
            }
            Ok(())
        };
        let logger = |warning: bool, message: &str| {
            messages.borrow_mut().push((warning, message.into()));
            trace.borrow_mut().push(if warning {
                json!(["warning", message, "warning"])
            } else {
                json!(["emit", message])
            });
            observed.borrow_mut().push(state.keepalive_failing.get());
            if (warning && name == "warning-fail") || (!warning && name == "emit-fail") {
                return Err(KeepaliveFailure::Error(error.clone()));
            }
            if name == "logger-changes-flag" {
                state.keepalive_failing.set(false);
            }
            Ok(())
        };
        let warning = |message: &str| logger(true, message);
        let emit = |message: &str| logger(false, message);
        let result = if variant == "binance" {
            run(update_binance_listen_key(
                &state, put, schedule, warning, emit,
            ))
        } else {
            run(update_binance_pm_listen_key(
                &state, put, schedule, warning, emit,
            ))
        };
        let expected_delays = match name {
            "disposed" | "coercion-fail" | "warning-fail" | "emit-fail" => vec![],
            "normal" | "recover" | "disposed-during-put" | "schedule-changes-flag" => {
                vec![1_800_000.0]
            }
            "schedule-success-fail" => vec![1_800_000.0, 60_000.0],
            _ => vec![60_000.0],
        };
        assert_eq!(
            *delays.borrow(),
            expected_delays,
            "LISTEN {variant} update native delay case {index}"
        );
        assert_eq!(
            polls.get(),
            if name == "disposed" || name == "put-sync" {
                0
            } else {
                2
            },
            "native actual await suspension count"
        );
        assert_eq!(
            state.is_disposed.get(),
            name == "disposed" || name == "disposed-during-put",
            "native disposal state"
        );
        let failing = ![
            "disposed",
            "normal",
            "recover",
            "disposed-during-put",
            "schedule-changes-flag",
            "coercion-fail",
            "emit-fail",
        ]
        .contains(&name);
        assert_eq!(
            state.keepalive_failing.get(),
            failing,
            "LISTEN {variant} update native failure flag case {index}"
        );
        let expected_observed = match name {
            "disposed" | "coercion-fail" => vec![],
            "normal" | "disposed-during-put" | "schedule-changes-flag" | "emit-fail" => vec![false],
            "recover" | "warning-fail" => vec![true],
            "flag-during-put" | "repeat-error" => vec![true, true],
            "schedule-success-fail" => vec![false, false, true],
            _ => vec![false, true],
        };
        assert_eq!(
            *observed.borrow(),
            expected_observed,
            "native flag observed before effects"
        );
        let expected_logs = usize::from(
            ![
                "disposed",
                "normal",
                "recover",
                "disposed-during-put",
                "schedule-changes-flag",
                "coercion-fail",
            ]
            .contains(&name),
        );
        assert_eq!(
            messages.borrow().len(),
            expected_logs,
            "native independent logger count"
        );
        if expected_logs == 1 {
            let detail = match name {
                "undefined-error" => "undefined",
                "null-error" => "null",
                "number-error" => "42",
                "string-error" => "wire failure",
                "array-error" => "1,,x",
                "object-error" => "[object Object]",
                _ => "fixture error",
            };
            let warning = ["flag-during-put", "repeat-error", "warning-fail"].contains(&name);
            assert_eq!(
                messages.borrow()[0],
                (
                    warning,
                    format!("Binance user data stream keepalive failed: {detail}")
                ),
                "LISTEN {variant} update native log message case {index}"
            );
        }
        let expected_failure = match name {
            "warning-fail" | "emit-fail" | "retry-fail" => Some("original"),
            "coercion-fail" => Some("generated"),
            _ => None,
        };
        if name == "coercion-fail" {
            let Err(KeepaliveFailure::Error(native_error)) = &result else {
                panic!("expected native TypeError");
            };
            assert_eq!(
                native_error.message, "Cannot convert object to primitive value",
                "native failed JSON coercion"
            );
        }
        assert_eq!(
            trace.borrow().len(),
            usize::from(name != "disposed") + expected_delays.len() + expected_logs,
            "native independent total effect count"
        );
        let (failure, error_name, error_message) =
            failure_fields(&result, expected_failure, &error);
        native.push(json!({"variant":variant,"method":"update","name":name,"trace":*trace.borrow(),"observed":*observed.borrow(),"disposed":state.is_disposed.get(),"failing":state.keepalive_failing.get(),"failure":failure,"errorName":error_name,"errorMessage":error_message}));
    }
    assert_eq!(
        native.len(),
        21,
        "native authored update floor; stopped schedule-changes-flag"
    );
    compare_source(variant, "update", native);
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn futures_fetch() {
    fetch("binance");
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn pm_fetch() {
    fetch("binancepm");
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn futures_update() {
    update("binance");
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn pm_update() {
    update("binancepm");
}

fn observed_error_code(
    code: &Option<safe_cex_exchange::js_runtime::value::Js>,
) -> Option<serde_json::Value> {
    code.as_ref().map(|value| {
        let finite =
            safe_cex_exchange::js_runtime::value::to_json(value).expect("finite JSON code fixture");
        let text = safe_cex_num::js_json_stringify(&finite.into())
            .expect("source JSON code fixture")
            .expect("defined code fixture");
        serde_json::from_str(&text).expect("JSON code observation")
    })
}
