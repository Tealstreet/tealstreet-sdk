use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_bybit::{
    execution_guards::*,
    order_params::{BybitBboSideType, BybitPlaceOrderOpts},
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn waker() -> Waker {
    Waker::noop().clone()
}
fn drive<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let waker = waker();
    let mut context = Context::from_waker(&waker);
    for _ in 0..4 {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
    panic!("GUARD native controlled future did not resolve within four polls");
}
async fn pending_once() {
    let mut pending = true;
    std::future::poll_fn(|context| {
        if pending {
            pending = false;
            context.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;
}
fn token(value: &Value) -> f64 {
    match value.as_str().unwrap() {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        _ => value.as_str().unwrap().parse().unwrap(),
    }
}
fn number(actual: f64, expected: &Value, label: &str) {
    let bits = if actual.is_nan() {
        "nan".into()
    } else {
        format!("{:016x}", actual.to_bits())
    };
    assert_eq!(
        bits,
        expected["number"].as_str().unwrap(),
        "GUARD native number {label}"
    );
}
fn optional(actual: Option<f64>, expected: &Value, label: &str) {
    if expected["undefined"] == true {
        assert!(actual.is_none(), "GUARD native absent {label}");
    } else {
        number(actual.expect(label), expected, label);
    }
}
fn json_value(actual: &Value, expected: &Value) {
    match actual {
        Value::Number(_) => number(
            safe_cex_num::numeric_ingress::js_value_as_f64(actual).unwrap(),
            expected,
            "retained JSON",
        ),
        Value::Array(values) => {
            let expected = expected.as_array().unwrap();
            assert_eq!(values.len(), expected.len(), "GUARD native array count");
            for (value, expected) in values.iter().zip(expected) {
                json_value(value, expected);
            }
        }
        Value::Object(values) => {
            let expected = expected.as_object().unwrap();
            assert_eq!(
                values.len(),
                expected.len(),
                "GUARD native object field count"
            );
            for (key, value) in values {
                json_value(value, expected.get(key).expect(key));
            }
        }
        _ => assert_eq!(actual, expected, "GUARD native JSON scalar"),
    }
}
fn error(name: &str, message: &str) -> Rc<ExchangeError> {
    let mut error = ExchangeError::new(message);
    error.name = name.into();
    error.code = Some(safe_cex_exchange::js_runtime::value::from_json(
        &serde_json::json!(37),
    ));
    Rc::new(error)
}
fn assert_error(actual: &Rc<ExchangeError>, expected: &Value) {
    assert_eq!(
        actual.name,
        expected["error"].as_str().unwrap(),
        "GUARD native error name"
    );
    assert_eq!(
        actual.message,
        expected["message"].as_str().unwrap(),
        "GUARD native error message"
    );
}

#[test]
#[ignore = "requires installed live TypeScript"]
fn execution_guards_native_source_parity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/execution-guard-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let f: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(f["receiverProfiles"], 28, "native receiver source floor");
    assert_eq!(
        f["windows"].as_array().unwrap().len(),
        20,
        "GUARD independent window floor"
    );
    for c in f["windows"].as_array().unwrap() {
        let value = |name| {
            if c[name] == "undefined" {
                None
            } else {
                Some(token(&c[name]))
            }
        };
        let actual =
            get_inverse_execution_query_windows(value("from"), value("to"), token(&c["max"]));
        let wanted = c["result"].as_array().unwrap();
        assert_eq!(actual.len(), wanted.len(), "GUARD native window count");
        for ((from, to), wanted) in actual.into_iter().zip(wanted) {
            optional(from, &wanted["startTime"], "window start");
            optional(to, &wanted["endTime"], "window end");
        }
    }
    assert_eq!(
        f["reinit"].as_array().unwrap().len(),
        8,
        "GUARD independent rejection floor"
    );
    for c in f["reinit"].as_array().unwrap() {
        let flag = Cell::new(c["initially"].as_bool().unwrap());
        let value = Rc::new(serde_json::json!({"marker":73}));
        let original = error("RemoteError", "request failed");
        let resolution = RefCell::new(None::<Result<Rc<Value>, Rc<ExchangeError>>>);
        let request = std::future::poll_fn(|_| {
            resolution
                .borrow_mut()
                .take()
                .map(Poll::Ready)
                .unwrap_or(Poll::Pending)
        });
        let mut future = Box::pin(reinit_on_throw(&flag, request));
        let waker = waker();
        let mut context = Context::from_waker(&waker);
        assert_eq!(
            flag.get(),
            c["before"].as_bool().unwrap(),
            "GUARD native initial rejection flag"
        );
        assert!(
            matches!(future.as_mut().poll(&mut context), Poll::Pending),
            "GUARD native request await"
        );
        *resolution.borrow_mut() = Some(if c["reject"] == true {
            Err(Rc::clone(&original))
        } else {
            Ok(Rc::clone(&value))
        });
        assert_eq!(
            flag.get(),
            c["settled"].as_bool().unwrap(),
            "GUARD native flag before reaction"
        );
        let Poll::Ready(result) = future.as_mut().poll(&mut context) else {
            panic!("GUARD native reaction readiness")
        };
        if c["reject"] == true {
            let actual = result.unwrap_err();
            assert_error(&actual, &c["outcome"]);
            assert!(
                Rc::ptr_eq(&actual, &original),
                "GUARD native rejection identity"
            );
            assert_eq!(c["outcome"]["sameError"], true);
        } else {
            let actual = result.unwrap();
            json_value(&actual, &c["outcome"]["value"]);
            assert!(Rc::ptr_eq(&actual, &value), "GUARD native success identity");
            assert_eq!(c["outcome"]["sameValue"], true);
        }
        assert_eq!(
            flag.get(),
            c["after"].as_bool().unwrap(),
            "GUARD native rejection flag"
        );
    }
    assert_eq!(
        f["index"].as_array().unwrap().len(),
        87,
        "GUARD independent index floor"
    );
    for c in f["index"].as_array().unwrap() {
        let opts = RefCell::new(c["opts"].as_object().unwrap().clone());
        let calls = RefCell::new(Vec::new());
        let original = error("ModeError", "mode request failed");
        let result = if c["family"] == "spot" {
            Ok(drive(spot_get_order_position_idx(&opts)))
        } else {
            drive(get_order_position_idx(&opts, |symbol| {
                calls.borrow_mut().push(symbol);
                async {
                    pending_once().await;
                    opts.borrow_mut()
                        .extend(c["after"].as_object().unwrap().clone());
                    if c["fail"] == true {
                        Err(Rc::clone(&original))
                    } else {
                        Ok(c["modeRaw"]
                            .as_str()
                            .map(|raw| serde_json::from_str::<Value>(raw).unwrap()))
                    }
                }
            }))
        };
        if c["result"].get("error").is_some() {
            let actual = result.unwrap_err();
            assert_error(&actual, &c["result"]);
            assert!(
                Rc::ptr_eq(&actual, &original),
                "GUARD native mode error identity"
            );
            assert_eq!(c["result"]["sameError"], true);
        } else {
            number(result.unwrap(), &c["result"]["value"], "position index");
        }
        let calls = calls.borrow();
        let wanted = c["calls"].as_array().unwrap();
        assert_eq!(calls.len(), wanted.len(), "GUARD native mode call count");
        for (actual, wanted) in calls.iter().zip(wanted) {
            if let Some(actual) = actual {
                json_value(actual, wanted);
            } else {
                assert_eq!(wanted["undefined"], true, "GUARD native absent symbol");
            }
        }
    }
    assert_eq!(
        f["stops"].as_array().unwrap().len(),
        24,
        "GUARD independent stop floor"
    );
    for c in f["stops"].as_array().unwrap() {
        let opts = c["opts"].as_object().unwrap().clone();
        let calls = RefCell::new(Vec::new());
        let original = error("IndexError", "index request failed");
        let actual = drive(get_stop_order_position_idx(&opts, |received| {
            calls.borrow_mut().push(received);
            async {
                pending_once().await;
                if c["token"] == "fail" {
                    Err(Rc::clone(&original))
                } else {
                    Ok(token(&c["token"]))
                }
            }
        }));
        if c["result"].get("error").is_some() {
            let actual = actual.unwrap_err();
            assert_error(&actual, &c["result"]);
            assert!(
                Rc::ptr_eq(&actual, &original),
                "GUARD native stop error identity"
            );
        } else {
            optional(actual.unwrap(), &c["result"]["value"], "stop index");
        }
        assert_eq!(
            opts,
            c["opts"].as_object().unwrap().clone(),
            "GUARD native caller opts untouched"
        );
        let calls = calls.borrow();
        assert_eq!(calls.len(), 1, "GUARD independent stop host call");
        assert!(
            !calls[0].contains_key("reduceOnly"),
            "GUARD native omitted reduceOnly"
        );
        json_value(&Value::Object(calls[0].clone()), &c["calls"][0]);
    }
    assert_eq!(
        f["executions"].as_array().unwrap().len(),
        39,
        "GUARD independent execution floor"
    );
    for c in f["executions"].as_array().unwrap() {
        let input = &c["input"];
        let opts: BybitPlaceOrderOpts = serde_json::from_value(input["opts"].clone()).unwrap();
        let original = error(
            input["errorName"].as_str().unwrap_or("CategoryError"),
            input["errorMessage"].as_str().unwrap_or("category failed"),
        );
        let logging = error("LogError", "logging failed");
        let events = RefCell::new(Vec::<(String, String)>::new());
        let emit = |message: &str| {
            events.borrow_mut().push(("log".into(), message.into()));
            if input["logFail"] == true {
                Err(Rc::clone(&logging))
            } else {
                Ok(())
            }
        };
        let actual = if c["family"] == "spot" {
            spot_get_order_execution_params(&opts, emit)
        } else {
            get_order_execution_params(
                &opts,
                |symbol| {
                    events.borrow_mut().push(("category".into(), symbol.into()));
                    if input["categoryFail"] == true {
                        Err(Rc::clone(&original))
                    } else {
                        Ok(if c["family"] == "inverse" {
                            "inverse"
                        } else {
                            "linear"
                        }
                        .into())
                    }
                },
                emit,
            )
        };
        let expected = &c["result"];
        if expected.get("error").is_some() {
            assert!(actual.is_err(), "GUARD native execution rejection");
            let actual = actual.unwrap_err();
            assert_error(&actual, expected);
            if expected["sameError"] == true {
                let original = if expected["loggingError"] == true {
                    &logging
                } else {
                    &original
                };
                assert!(
                    Rc::ptr_eq(&actual, original),
                    "GUARD native execution error identity"
                );
                assert_eq!(
                    observed_error_code(&actual.code),
                    Some(serde_json::json!(37)),
                    "GUARD native error metadata"
                );
            }
        } else {
            let actual = actual.unwrap();
            let expected = &expected["params"];
            let side = actual.bbo_side_type.map(|side| match side {
                BybitBboSideType::Queue => "Queue",
                BybitBboSideType::Counterparty => "Counterparty",
            });
            assert_eq!(
                side,
                expected["bboSideType"].as_str(),
                "GUARD native BBO side"
            );
            assert_eq!(
                actual.bbo_level.as_deref(),
                expected["bboLevel"].as_str(),
                "GUARD native BBO level"
            );
            assert_eq!(
                actual.slippage_tolerance_type.as_deref(),
                expected["slippageToleranceType"].as_str(),
                "GUARD native slippage type"
            );
            assert_eq!(
                actual.slippage_tolerance.as_deref(),
                expected["slippageTolerance"].as_str(),
                "GUARD native slippage value"
            );
        }
        let wanted: Vec<(String, String)> = c["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| {
                (
                    event["kind"].as_str().unwrap().into(),
                    event["value"].as_str().unwrap().into(),
                )
            })
            .collect();
        assert_eq!(
            *events.borrow(),
            wanted,
            "GUARD native execution callback order"
        );
    }
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
