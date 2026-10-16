use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_num::JsJsonValue;
use safe_cex_venue_kraken::private_auth::{self, FuturesAuthState, SpotAuthState};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    process::Command,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};

fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/private-auth-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["header"]["hashes"].as_object().unwrap().len(), 2);
        assert!(!value["header"]["v8"].as_str().unwrap().is_empty());
        value
    })
}

fn flag(config: &Value, name: &str) -> bool {
    config[name].as_bool().unwrap_or(false)
}
fn token(tag: &str) -> JsJsonValue {
    match tag {
        "undefined" => JsJsonValue::Undefined,
        "null" => JsJsonValue::Null,
        "false" => JsJsonValue::Bool(false),
        "true" => JsJsonValue::Bool(true),
        "zero" => JsJsonValue::Number(0.0),
        "negative-zero" => JsJsonValue::Number(-0.0),
        "nan" => JsJsonValue::Number(f64::NAN),
        "infinity" => JsJsonValue::Number(f64::INFINITY),
        "negative-infinity" => JsJsonValue::Number(f64::NEG_INFINITY),
        "empty" => JsJsonValue::String(String::new()),
        "text" => JsJsonValue::String("private-雪".into()),
        "array" => JsJsonValue::Array(vec![]),
        "object" => JsJsonValue::Object(vec![
            ("value".into(), JsJsonValue::Number(-0.0)),
            ("other".into(), JsJsonValue::Number(17.25)),
        ]),
        _ => panic!("unknown fixture token"),
    }
}
fn assert_token(native: &JsJsonValue, source: &Value) {
    match native {
        JsJsonValue::Undefined => assert_eq!(
            source,
            &json!({"undefined":true}),
            "PRIVATE-AUTH native token"
        ),
        JsJsonValue::Null => assert!(source.is_null(), "PRIVATE-AUTH native token"),
        JsJsonValue::Bool(value) => {
            assert_eq!(source.as_bool(), Some(*value), "PRIVATE-AUTH native token")
        }
        JsJsonValue::String(value) => assert_eq!(
            source.as_str(),
            Some(value.as_str()),
            "PRIVATE-AUTH native token"
        ),
        JsJsonValue::Number(value) => assert_eq!(
            source["number"],
            format!(
                "{:016x}",
                if value.is_nan() {
                    f64::NAN.to_bits()
                } else {
                    value.to_bits()
                }
            ),
            "PRIVATE-AUTH native token"
        ),
        JsJsonValue::Array(values) => {
            assert_eq!(
                values.len(),
                source.as_array().unwrap().len(),
                "PRIVATE-AUTH native token"
            );
            for (index, value) in values.iter().enumerate() {
                assert_token(value, &source[index]);
            }
        }
        JsJsonValue::Object(values) => {
            assert_eq!(
                values.len(),
                source.as_object().unwrap().len(),
                "PRIVATE-AUTH native token"
            );
            for (index, (key, value)) in values.iter().enumerate() {
                assert_eq!(
                    source.as_object().unwrap().iter().nth(index).unwrap().0,
                    key
                );
                assert_token(value, &source[key]);
            }
        }
        JsJsonValue::BigInt(_) => panic!("not in fixed corpus"),
    }
}
fn assert_state(native: &SpotAuthState, source: &Value) {
    assert_eq!(
        native.authenticated,
        source["authenticated"].as_bool().unwrap(),
        "PRIVATE-AUTH native authenticated"
    );
    assert_token(&native.token, &source["token"]);
}
fn snapshot(state: &RefCell<SpotAuthState>) -> SpotAuthState {
    let state = state.borrow();
    SpotAuthState {
        authenticated: state.authenticated,
        token: state.token.clone(),
    }
}
fn callback_error(config: &Value, pointer: &Cell<usize>) -> ExchangeError {
    let mut error = ExchangeError::new(if flag(config, "emptyError") {
        ""
    } else {
        "callback failure"
    });
    error.name = "TransportError".into();
    error.metadata.context = Some(json!({"identity":91}));
    pointer.set((&*error.metadata as *const _) as usize);
    error
}
fn assert_result(result: Result<(), ExchangeError>, expected: &Value, pointer: &Cell<usize>) {
    match result {
        Ok(()) => assert!(expected.is_null(), "PRIVATE-AUTH native rejection"),
        Err(error) => {
            assert!(!expected.is_null(), "PRIVATE-AUTH native rejection");
            assert_eq!(error.name, expected["name"]);
            assert_eq!(error.message, expected["message"]);
            assert_eq!(error.metadata.context, Some(json!({"identity":91})));
            assert_eq!(
                (&*error.metadata as *const _) as usize,
                pointer.get(),
                "PRIVATE-AUTH original error identity"
            );
            assert_eq!(expected["identity"], true);
        }
    }
}

#[test]
fn set_token_live_source() {
    let cases = oracle()["setters"].as_array().unwrap();
    assert_eq!(cases.len(), 13, "PRIVATE-AUTH independent setter cases");
    for case in cases {
        let state = RefCell::new(SpotAuthState {
            authenticated: true,
            token: JsJsonValue::String("old-token".into()),
        });
        private_auth::set_token(&state, token(case["token"].as_str().unwrap()));
        assert_state(&state.borrow(), &case["state"]);
    }
}

struct SpotEvent {
    kind: &'static str,
    state: SpotAuthState,
    log: Option<(String, LogSeverity)>,
    wire: Option<String>,
}
#[test]
fn spot_open_live_source() {
    let cases = oracle()["spot"].as_array().unwrap();
    assert_eq!(cases.len(), 24, "PRIVATE-AUTH independent spot cases");
    let mut events = 0;
    let mut rejected = 0;
    for case in cases {
        let config = &case["config"];
        let state = RefCell::new(SpotAuthState {
            authenticated: true,
            token: JsJsonValue::String("old-token".into()),
        });
        let disposed = Cell::new(flag(config, "disposed"));
        let released = Cell::new(false);
        let error_pointer = Cell::new(0);
        let trace = RefCell::new(Vec::new());
        let sends = Cell::new(0);
        let fetch = || {
            trace.borrow_mut().push(SpotEvent {
                kind: "fetch",
                state: snapshot(&state),
                log: None,
                wire: None,
            });
            if flag(config, "fetchSync") {
                return Err(callback_error(config, &error_pointer));
            }
            Ok(poll_fn(|_| {
                if !released.get() {
                    Poll::Pending
                } else if flag(config, "fetchFail") {
                    Poll::Ready(Err(callback_error(config, &error_pointer)))
                } else {
                    Poll::Ready(Ok(token(config["token"].as_str().unwrap_or("undefined"))))
                }
            }))
        };
        let send = |wire| {
            sends.set(sends.get() + 1);
            trace.borrow_mut().push(SpotEvent {
                kind: "send",
                state: snapshot(&state),
                log: None,
                wire: Some(wire),
            });
            if sends.get() == 1 {
                if let Some(tag) = config["retoken"].as_str() {
                    private_auth::set_token(&state, token(tag));
                }
                if flag(config, "disposeSend") {
                    disposed.set(true);
                }
            }
            if Some(sends.get()) == config["sendFail"].as_u64() {
                Err(callback_error(config, &error_pointer))
            } else {
                Ok(())
            }
        };
        let log = |message, severity| {
            trace.borrow_mut().push(SpotEvent {
                kind: "log",
                state: snapshot(&state),
                log: Some((message, severity)),
                wire: None,
            });
            if flag(config, "logFail") {
                Err(callback_error(config, &error_pointer))
            } else {
                Ok(())
            }
        };
        let mut future = Box::pin(private_auth::spot_on_open(
            &state,
            || disposed.get(),
            fetch,
            send,
            log,
        ));
        let mut context = Context::from_waker(Waker::noop());
        let first = future.as_mut().poll(&mut context);
        assert_state(&state.borrow(), &case["prefix"]);
        assert_eq!(
            trace.borrow().len(),
            case["prefixTrace"].as_u64().unwrap() as usize
        );
        if flag(config, "disposeAwait") {
            disposed.set(true);
        }
        if flag(config, "setAwait") {
            private_auth::set_token(&state, JsJsonValue::String("during-await".into()));
        }
        released.set(true);
        let result = match first {
            Poll::Ready(result) => result,
            Poll::Pending => match future.as_mut().poll(&mut context) {
                Poll::Ready(result) => result,
                Poll::Pending => panic!("unsettled token fetch"),
            },
        };
        rejected += usize::from(result.is_err());
        assert_result(result, &case["failure"], &error_pointer);
        assert_state(&state.borrow(), &case["state"]);
        let trace = trace.borrow();
        assert_eq!(trace.len(), case["trace"].as_array().unwrap().len());
        events += trace.len();
        for (native, expected) in trace.iter().zip(case["trace"].as_array().unwrap()) {
            assert_eq!(native.kind, expected["kind"]);
            assert_state(&native.state, &expected["state"]);
            if let Some((message, severity)) = &native.log {
                assert_eq!(message, &expected["message"]);
                assert_eq!(severity.as_str(), expected["severity"]);
            }
        }
        for (native, expected) in trace.iter().zip(case["trace"].as_array().unwrap()) {
            if let Some(wire) = &native.wire {
                assert_eq!(wire, &expected["wire"], "PRIVATE-AUTH spot wire");
            }
        }
    }
    assert_eq!(events, 55, "PRIVATE-AUTH independent spot events");
    assert_eq!(rejected, 4, "PRIVATE-AUTH independent spot rejections");
}

struct FuturesEvent {
    kind: &'static str,
    waiting: bool,
    log: Option<(String, LogSeverity)>,
    wire: Option<String>,
}
#[test]
fn futures_auth_live_source() {
    let cases = oracle()["auth"].as_array().unwrap();
    assert_eq!(cases.len(), 11, "PRIVATE-AUTH independent auth cases");
    let mut events = 0;
    let mut rejected = 0;
    for case in cases {
        let config = &case["config"];
        let state = RefCell::new(FuturesAuthState {
            waiting_challenge: flag(config, "waiting"),
        });
        let trace = RefCell::new(Vec::new());
        let error_pointer = Cell::new(0);
        let send = |wire| {
            trace.borrow_mut().push(FuturesEvent {
                kind: "send",
                waiting: state.borrow().waiting_challenge,
                log: None,
                wire: Some(wire),
            });
            if flag(config, "resetSend") {
                state.borrow_mut().waiting_challenge = false;
            }
            if flag(config, "sendFail") {
                Err(callback_error(config, &error_pointer))
            } else {
                Ok(())
            }
        };
        let log = |message, severity| {
            trace.borrow_mut().push(FuturesEvent {
                kind: "log",
                waiting: state.borrow().waiting_challenge,
                log: Some((message, severity)),
                wire: None,
            });
            if flag(config, "logFail") {
                Err(callback_error(config, &error_pointer))
            } else {
                Ok(())
            }
        };
        let mut future = Box::pin(private_auth::authenticate(
            &state,
            config["key"].as_str(),
            config["secret"].as_str(),
            flag(config, "remote"),
            send,
            log,
        ));
        let result = match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("auth has no await"),
        };
        rejected += usize::from(result.is_err());
        assert_result(result, &case["failure"], &error_pointer);
        assert_eq!(
            state.borrow().waiting_challenge,
            case["waiting"].as_bool().unwrap(),
            "PRIVATE-AUTH native waiting challenge"
        );
        let trace = trace.borrow();
        assert_eq!(trace.len(), case["trace"].as_array().unwrap().len());
        events += trace.len();
        for (native, expected) in trace.iter().zip(case["trace"].as_array().unwrap()) {
            assert_eq!(native.kind, expected["kind"]);
            assert_eq!(
                native.waiting,
                expected["waiting"].as_bool().unwrap(),
                "PRIVATE-AUTH native waiting challenge"
            );
            if let Some((message, severity)) = &native.log {
                assert_eq!(message, &expected["message"]);
                assert_eq!(severity.as_str(), expected["severity"]);
            }
        }
        for (native, expected) in trace.iter().zip(case["trace"].as_array().unwrap()) {
            if let Some(wire) = &native.wire {
                assert_eq!(wire, &expected["wire"], "PRIVATE-AUTH challenge wire");
            }
        }
    }
    assert_eq!(events, 14, "PRIVATE-AUTH independent auth events");
    assert_eq!(rejected, 2, "PRIVATE-AUTH independent auth rejections");
}

#[test]
fn futures_open_live_source() {
    let cases = oracle()["open"].as_array().unwrap();
    assert_eq!(
        cases.len(),
        6,
        "PRIVATE-AUTH independent futures open cases"
    );
    let mut events = 0;
    let mut rejected = 0;
    for case in cases {
        let config = &case["config"];
        let released = Cell::new(false);
        let error_pointer = Cell::new(0);
        let trace = RefCell::new(Vec::new());
        let auth = || {
            trace.borrow_mut().push(("auth", None));
            if flag(config, "syncFail") {
                return Err(callback_error(config, &error_pointer));
            }
            Ok(poll_fn(|_| {
                if !released.get() {
                    Poll::Pending
                } else if flag(config, "fail") {
                    Poll::Ready(Err(callback_error(config, &error_pointer)))
                } else {
                    Poll::Ready(Ok(()))
                }
            }))
        };
        let log = |message, severity| {
            trace.borrow_mut().push(("log", Some((message, severity))));
            if flag(config, "logFail") {
                Err(callback_error(config, &error_pointer))
            } else {
                Ok(())
            }
        };
        let mut future = Box::pin(private_auth::futures_on_open(
            || flag(config, "disposed"),
            auth,
            log,
        ));
        let mut context = Context::from_waker(Waker::noop());
        let first = future.as_mut().poll(&mut context);
        assert_eq!(
            trace.borrow().len(),
            case["prefixTrace"].as_u64().unwrap() as usize
        );
        released.set(true);
        let result = match first {
            Poll::Ready(result) => result,
            Poll::Pending => match future.as_mut().poll(&mut context) {
                Poll::Ready(result) => result,
                Poll::Pending => panic!("unsettled authentication"),
            },
        };
        rejected += usize::from(result.is_err());
        assert_result(result, &case["failure"], &error_pointer);
        let trace = trace.borrow();
        assert_eq!(trace.len(), case["trace"].as_array().unwrap().len());
        events += trace.len();
        for ((kind, log), expected) in trace.iter().zip(case["trace"].as_array().unwrap()) {
            assert_eq!(kind, &expected["kind"]);
            if let Some((message, severity)) = log {
                assert_eq!(message, &expected["message"]);
                assert_eq!(
                    severity.as_str(),
                    expected["severity"],
                    "PRIVATE-AUTH native open severity"
                );
            }
        }
    }
    assert_eq!(events, 9, "PRIVATE-AUTH independent futures open events");
    assert_eq!(
        rejected, 1,
        "PRIVATE-AUTH independent futures open rejections"
    );
}
