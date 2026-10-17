use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{
    Market, MarketLimitsAmount, MarketLimitsLeverage, MarketPrecision, Order, StoreDataLoaded,
};
use safe_cex_venue_binance::{
    account_leverage_overlay::*, exchange_error::*, orders_retry::*,
    reconciliation::pause_background_reconciliation, request_weight::*,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn run<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..3 {
        if let Poll::Ready(value) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return value;
        }
    }
    panic!("fixture exceeded three polls");
}
fn error() -> Failure {
    let mut e = ExchangeError::new("rest fixture");
    e.name = "RangeError".into();
    e.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "original-code"
    )));
    Rc::new(e)
}
fn fields(r: &Result<(), Failure>, e: &Failure) -> Value {
    match r {
        Ok(_) => json!({"failure":null,"errorName":null,"errorMessage":null}),
        Err(f) => {
            let original = Rc::ptr_eq(f, e);
            if original {
                assert_eq!(
                    observed_error_code(&f.code),
                    Some(json!("original-code")),
                    "REST native original error metadata"
                );
            }
            json!({"failure":if original{"original"}else{"generated"},"errorName":f.name,"errorMessage":f.message})
        }
    }
}
enum ExpectedError<'a> {
    None,
    Original,
    Generated(&'a str, String),
}
fn assert_error(
    result: &Result<(), Failure>,
    expected: ExpectedError<'_>,
    original: &Failure,
    label: &str,
) {
    match expected {
        ExpectedError::None => assert!(result.is_ok(), "{label} native expected success"),
        ExpectedError::Original => {
            let error = result.as_ref().expect_err("native expected callback error");
            assert!(
                Rc::ptr_eq(error, original),
                "{label} native original error identity"
            );
            assert_eq!(error.name, "RangeError", "{label} native error name");
            assert_eq!(
                error.message, "rest fixture",
                "{label} native error message"
            );
            assert_eq!(
                observed_error_code(&error.code),
                Some(json!("original-code")),
                "{label} native error code"
            );
        }
        ExpectedError::Generated(name, message) => {
            let error = result
                .as_ref()
                .expect_err("native expected generated error");
            assert!(
                !Rc::ptr_eq(error, original),
                "{label} native generated error identity"
            );
            assert_eq!(error.name, name, "{label} native generated error name");
            assert_eq!(
                error.message, message,
                "{label} native generated error message"
            );
        }
    }
}
fn row(mut value: Value, r: &Result<(), Failure>, e: &Failure) -> Value {
    for (k, v) in fields(r, e).as_object().unwrap() {
        value[k] = v.clone();
    }
    value
}
fn number(actual: f64, expected: f64, label: &str) {
    if expected.is_nan() {
        assert!(actual.is_nan(), "{label} native NaN");
    } else {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "{label} native value {actual:?} versus{expected:?}"
        );
    }
}
fn encode(n: f64) -> Value {
    if n.is_nan() {
        json!("nan")
    } else if n == f64::INFINITY {
        json!("infinity")
    } else if n == f64::NEG_INFINITY {
        json!("-infinity")
    } else if n == 0.0 && n.is_sign_negative() {
        json!("-zero")
    } else {
        json!(n)
    }
}
fn compare(family: &str, native: Vec<Value>, count: usize) {
    assert_eq!(
        native.len(),
        count,
        "REST independently authored native floor"
    );
    // Native values/counts/identity for this entire family finish before source.
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/rest-reconciliation.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        source.len(),
        133,
        "REST independent source floor15+21+11+12+36+38"
    );
    let source: Vec<_> = source
        .into_iter()
        .filter(|r| r["family"] == family)
        .collect();
    assert_eq!(source.len(), count);
    for (i, (a, b)) in native.iter().zip(&source).enumerate() {
        values(a, b, &format!("REST {family} source case{i}"));
    }
}
fn values(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "{path} decoded number {:?} versus{:?}",
            a.as_f64(),
            b.as_f64()
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path} array count");
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                values(x, y, &format!("{path}/{i}"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path} object count");
            for (k, v) in a {
                values(v, b.get(k).expect("source field"), &format!("{path}/{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
const HEADERS: [&str; 15] = [
    "lower",
    "exact",
    "upper",
    "nulls",
    "falsy",
    "null-upper",
    "missing",
    "no-headers",
    "getter",
    "getter-missing",
    "getter-fail",
    "member-fail",
    "changed-getter",
    "noncallable-second",
    "property-fail",
];
#[test]
#[ignore = "live source oracle"]
fn response_header() {
    let mut native = vec![];
    for (index, name) in HEADERS.into_iter().enumerate() {
        let e = error();
        let trace = Rc::new(RefCell::new(vec![]));
        let reads = Cell::new(0);
        let mut record = json!({"retry-after":"3.6","RETRY-AFTER":"4.2"});
        match name {
            "exact" => record["Retry-After"] = json!("2.5"),
            "upper" => {
                record.as_object_mut().unwrap().remove("retry-after");
            }
            "nulls" => {
                record["Retry-After"] = Value::Null;
                record["retry-after"] = Value::Null;
            }
            "falsy" => record["Retry-After"] = json!(0),
            "null-upper" => {
                record["retry-after"] = Value::Null;
                record["RETRY-AFTER"] = Value::Null;
            }
            "missing" => {
                record.as_object_mut().unwrap().clear();
            }
            _ => {}
        }
        let member = || {
            reads.set(reads.get() + 1);
            trace.borrow_mut().push(json!(["get-member", reads.get()]));
            if name == "member-fail" {
                return Err(e.clone());
            }
            if [
                "getter",
                "getter-missing",
                "getter-fail",
                "changed-getter",
                "noncallable-second",
            ]
            .contains(&name)
            {
                if name == "noncallable-second" && reads.get() == 2 {
                    return Ok(HeaderGetMember::NotCallable);
                }
                let id = if name == "changed-getter" {
                    reads.get()
                } else {
                    1
                };
                let (t, e) = (trace.clone(), e.clone());
                Ok(HeaderGetMember::Callable(Rc::new(move |n| {
                    assert_eq!(n, "Retry-After", "REST native original get header name");
                    t.borrow_mut().push(json!(["get-call", id, n, true]));
                    if name == "getter-fail" {
                        Err(e.clone())
                    } else {
                        Ok(if name == "getter-missing" {
                            None
                        } else {
                            Some(json!(if id == 2 { "8.5" } else { "7.5" }))
                        })
                    }
                })))
            } else {
                Ok(HeaderGetMember::NotCallable)
            }
        };
        let property = |key: &str| {
            trace.borrow_mut().push(json!(["property", key]));
            if name == "property-fail" {
                Err(e.clone())
            } else {
                Ok(record.get(key).cloned())
            }
        };
        let result = get_response_header(name != "no-headers", "Retry-After", member, property);
        let expected = match name {
            "lower" => Some(json!("3.6")),
            "exact" => Some(json!("2.5")),
            "upper" | "nulls" => Some(json!("4.2")),
            "falsy" => Some(json!(0)),
            "null-upper" => Some(Value::Null),
            "getter" => Some(json!("7.5")),
            "changed-getter" => Some(json!("8.5")),
            _ => None,
        };
        let fails = [
            "getter-fail",
            "member-fail",
            "noncallable-second",
            "property-fail",
        ]
        .contains(&name);
        assert_eq!(result.is_err(), fails, "REST header native error floor");
        if let Ok(v) = &result {
            assert_eq!(*v, expected, "REST header native result case{index}");
        }
        assert_eq!(
            reads.get(),
            if name == "no-headers" {
                0
            } else if [
                "getter",
                "getter-missing",
                "getter-fail",
                "changed-getter",
                "noncallable-second"
            ]
            .contains(&name)
            {
                2
            } else {
                1
            },
            "REST native get member reads"
        );
        let returned = result.as_ref().ok().and_then(Clone::clone);
        let status = result.as_ref().map(|_| ()).map_err(Clone::clone);
        assert_error(
            &status,
            if name == "noncallable-second" {
                ExpectedError::Generated("TypeError", "headers.get is not a function".into())
            } else if fails {
                ExpectedError::Original
            } else {
                ExpectedError::None
            },
            &e,
            "REST header",
        );
        native.push(row(json!({"family":"header","name":name,"trace":*trace.borrow(),"returned":returned,"undefined":result.as_ref().map_or(true,Option::is_none)}),&status,&e));
    }
    compare("header", native, 15);
}
const WEIGHTS: [&str; 21] = [
    "429-retry",
    "418-retry",
    "429-zero",
    "429-negative",
    "429-missing",
    "429-infinity",
    "429-overflow",
    "429-null",
    "429-array",
    "429-hex",
    "429-coercion-fail",
    "below",
    "threshold",
    "above",
    "string-status",
    "missing-status",
    "missing-weight",
    "weight-infinity",
    "weight-coercion-fail",
    "header-fail",
    "pause-fail",
];
#[test]
#[ignore = "live source oracle"]
fn response_weight() {
    let mut native = vec![];
    for (index, name) in WEIGHTS.into_iter().enumerate() {
        let e = error();
        let trace = RefCell::new(vec![]);
        let durations = RefCell::new(vec![]);
        let until = Cell::new(2000.0);
        let mut status = Some(json!(if name.starts_with("429") {
            429
        } else if name == "418-retry" {
            418
        } else {
            200
        }));
        let mut retry = Some(json!("2.5"));
        let mut used = Some(json!("100"));
        match name {
            "429-zero" => retry = Some(json!(0)),
            "429-negative" => retry = Some(json!("-1")),
            "429-missing" => retry = None,
            "429-infinity" => retry = Some(json!("Infinity")),
            "429-overflow" => retry = Some(json!("1e308")),
            "429-null" => retry = Some(Value::Null),
            "429-array" => retry = Some(json!(["3.6"])),
            "429-hex" => retry = Some(json!("0x10")),
            "429-coercion-fail" => retry = Some(json!({"toString":0})),
            "threshold" => used = Some(json!("1920")),
            "above" | "pause-fail" => used = Some(json!("1921")),
            "below" => used = Some(json!("1919")),
            "string-status" => {
                status = Some(json!("429"));
                used = Some(json!("1921"));
            }
            "missing-status" => status = None,
            "missing-weight" => used = None,
            "weight-infinity" => used = Some(json!("Infinity")),
            "weight-coercion-fail" => used = Some(json!({"toString":0})),
            _ => {}
        }
        let result = observe_request_weight(
            status.as_ref(),
            |key| {
                trace.borrow_mut().push(json!(["header", key]));
                if name == "header-fail" {
                    Err(e.clone())
                } else {
                    Ok(if key == "retry-after" {
                        retry.clone()
                    } else {
                        used.clone()
                    })
                }
            },
            |duration| {
                trace.borrow_mut().push(json!(["pause", encode(duration)]));
                durations.borrow_mut().push(duration);
                if name == "pause-fail" {
                    return Err(e.clone());
                }
                trace.borrow_mut().push(json!(["now"]));
                let mut value = until.get();
                pause_background_reconciliation(&mut value, 1000.0, duration);
                until.set(value);
                Ok(())
            },
        );
        let expected = match name {
            "429-retry" | "418-retry" => Some(2500.0),
            "429-zero" | "429-negative" | "429-missing" | "429-infinity" | "429-null"
            | "threshold" | "above" | "string-status" | "pause-fail" => Some(60000.0),
            "429-array" => Some(3600.0),
            "429-hex" => Some(16000.0),
            "429-overflow" => Some(f64::INFINITY),
            _ => None,
        };
        assert_eq!(
            durations.borrow().len(),
            usize::from(expected.is_some()),
            "REST weight native independent pause count case{index}"
        );
        if let Some(expected) = expected {
            number(
                durations.borrow()[0],
                expected,
                &format!("REST weight native duration case{index}"),
            );
        }
        number(
            until.get(),
            if name == "pause-fail" {
                2000.0
            } else {
                expected.map_or(2000.0, |d| 2000.0f64.max(1000.0 + d))
            },
            "REST native paused until",
        );
        assert_eq!(
            result.is_err(),
            [
                "429-coercion-fail",
                "weight-coercion-fail",
                "header-fail",
                "pause-fail"
            ]
            .contains(&name)
        );
        assert_error(
            &result,
            if ["429-coercion-fail", "weight-coercion-fail"].contains(&name) {
                ExpectedError::Generated(
                    "TypeError",
                    "Cannot convert object to primitive value".into(),
                )
            } else if ["header-fail", "pause-fail"].contains(&name) {
                ExpectedError::Original
            } else {
                ExpectedError::None
            },
            &e,
            "REST weight",
        );
        native.push(row(json!({"family":"weight","name":name,"trace":*trace.borrow(),"durations":durations.borrow().iter().map(|v|encode(*v)).collect::<Vec<_>>(),"until":encode(until.get())}),&result,&e));
    }
    compare("weight", native, 21);
}
const INSTALL: [&str; 11] = [
    "normal",
    "equal-client-values",
    "empty",
    "register-fail-first",
    "register-fail-second",
    "fulfilled-fail",
    "rejected-observe-fail",
    "get-observe-fail",
    "error-response-fail",
    "late-method",
    "error-getter-changes-method",
];
type Response = Rc<RefCell<Value>>;
type HttpError = Rc<Value>;
#[test]
#[ignore = "live source oracle"]
fn weight_interceptors() {
    let mut native = vec![];
    for (index, name) in INSTALL.into_iter().enumerate() {
        let e = error();
        let trace = Rc::new(RefCell::new(vec![]));
        let current = Rc::new(Cell::new(1));
        let registered = RefCell::new(vec![]);
        let (t, c, e1) = (trace.clone(), current.clone(), e.clone());
        let get_observe: ObserveGetter<Response> = Rc::new(move || {
            t.borrow_mut().push(json!(["get-observe", c.get()]));
            if name == "get-observe-fail" {
                return Err(e1.clone());
            }
            let id = c.get();
            let (t, e) = (t.clone(), e1.clone());
            Ok(Rc::new(move |r| {
                if let Some(r) = r.as_ref() {
                    number(
                        r.borrow()["quantity"].as_f64().unwrap(),
                        3.6,
                        "REST native decoded observed quantity",
                    );
                }
                t.borrow_mut().push(json!([
                    "observe",
                    id,
                    r.map(|r| r.borrow()["status"].clone())
                ]));
                if ["fulfilled-fail", "rejected-observe-fail"].contains(&name) {
                    Err(e.clone())
                } else {
                    Ok(())
                }
            }))
        });
        let response = Rc::new(RefCell::new(json!({"status":200,"quantity":3.6})));
        let http_error = Rc::new(json!({"marker":7}));
        let (t, c, r, e1) = (trace.clone(), current.clone(), response.clone(), e.clone());
        let error_response: Rc<dyn Fn(&HttpError) -> Result<Option<Response>, Failure>> =
            Rc::new(move |error| {
                assert_eq!(
                    error["marker"].as_u64(),
                    Some(7),
                    "REST native decoded rejection marker"
                );
                t.borrow_mut().push(json!(["error-response"]));
                if name == "error-response-fail" {
                    return Err(e1.clone());
                }
                if name == "error-getter-changes-method" {
                    c.set(3);
                }
                Ok(Some(r.clone()))
            });
        let a = Rc::new(1);
        let b = Rc::new(if name == "equal-client-values" { 1 } else { 2 });
        let clients = if name == "empty" {
            vec![]
        } else {
            vec![a.clone(), a.clone(), b.clone(), a.clone()]
        };
        let result = install_request_weight_guards(
            &clients,
            get_observe,
            error_response,
            |client, ok, bad| {
                trace.borrow_mut().push(json!(["register", **client]));
                assert!(
                    Rc::ptr_eq(client, &a) || Rc::ptr_eq(client, &b),
                    "REST native original client identity"
                );
                registered.borrow_mut().push((ok, bad));
                if name == "register-fail-first" && **client == 1
                    || name == "register-fail-second" && **client == 2
                {
                    Err(e.clone())
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(
            registered.borrow().len(),
            if name == "empty" {
                0
            } else if name == "register-fail-first" {
                1
            } else {
                2
            },
            "REST install native register count case{index}"
        );
        let mut callback_result = Ok(());
        let mut returned_same = false;
        let mut rejected = false;
        if result.is_ok() && !registered.borrow().is_empty() {
            if name == "late-method" {
                current.set(2);
            }
            let pair = &registered.borrow()[0];
            if [
                "normal",
                "fulfilled-fail",
                "late-method",
                "get-observe-fail",
            ]
            .contains(&name)
            {
                callback_result = (pair.0)(response.clone()).map(|r| {
                    returned_same = Rc::ptr_eq(&r, &response);
                    assert!(returned_same, "REST native original fulfilled response");
                });
            } else {
                match (pair.1)(http_error.clone()) {
                    Err(error) => callback_result = Err(error),
                    Ok(future) => match run(future) {
                        Err(error) => {
                            assert!(
                                Rc::ptr_eq(&error, &http_error),
                                "REST native original rejected error"
                            );
                            rejected = true;
                        }
                        Ok(_) => panic!("native rejection resolved"),
                    },
                }
            }
        }
        assert_eq!(result.is_err(), name.starts_with("register-fail"));
        assert_eq!(
            callback_result.is_err(),
            [
                "fulfilled-fail",
                "rejected-observe-fail",
                "get-observe-fail",
                "error-response-fail"
            ]
            .contains(&name)
        );
        let observed: Vec<_> = trace
            .borrow()
            .iter()
            .filter(|r| r[0] == "observe")
            .map(|r| r[1].as_i64().unwrap())
            .collect();
        assert_eq!(
            observed,
            if [
                "empty",
                "register-fail-first",
                "register-fail-second",
                "get-observe-fail",
                "error-response-fail"
            ]
            .contains(&name)
            {
                vec![]
            } else {
                vec![if name == "late-method" { 2 } else { 1 }]
            },
            "REST native captured observer before error getter"
        );
        assert_error(
            &result,
            if name.starts_with("register-fail") {
                ExpectedError::Original
            } else {
                ExpectedError::None
            },
            &e,
            "REST install registration",
        );
        assert_error(
            &callback_result,
            if [
                "fulfilled-fail",
                "rejected-observe-fail",
                "get-observe-fail",
                "error-response-fail",
            ]
            .contains(&name)
            {
                ExpectedError::Original
            } else {
                ExpectedError::None
            },
            &e,
            "REST install callback",
        );
        native.push(json!({"family":"install","name":name,"trace":*trace.borrow(),"registered":registered.borrow().len(),"returnedSame":returned_same,"rejected":rejected,"failure":result.is_err().then_some("original"),"callbackFailure":callback_result.is_err().then_some("original")}));
    }
    compare("install", native, 11);
}
const EMIT: [&str; 12] = [
    "error-response",
    "error-message",
    "error-empty",
    "error-zero",
    "error-array",
    "error-object",
    "error-coercion-fail",
    "plain-response",
    "plain-message",
    "plain-falsy",
    "null",
    "emit-fail",
];
#[test]
#[ignore = "live source oracle"]
fn exchange_error() {
    let mut native = vec![];
    for (index, name) in EMIT.into_iter().enumerate() {
        let e = error();
        let trace = RefCell::new(vec![]);
        let mut properties = json!({"message":"fallback","marker":7});
        match name {
            "error-response" | "plain-response" | "emit-fail" => {
                properties["response"] = json!({"data":{"msg":"venue"}})
            }
            "error-empty" => properties["message"] = json!(""),
            "error-zero" => properties["message"] = json!(0),
            "error-array" => properties["response"] = json!({"data":{"msg":[]}}),
            "error-object" => properties["response"] = json!({"data":{"msg":{}}}),
            "error-coercion-fail" => {
                properties["response"] = json!({"data":{"msg":{"toString":0}}})
            }
            "plain-falsy" => {
                properties["response"] = json!({"data":{"msg":0}});
                properties["message"] = json!(false);
            }
            _ => {}
        }
        let original = Rc::new(properties.clone());
        let instance = name.starts_with("error");
        let exception = if instance {
            ExceptionValue::Error {
                original: original.clone(),
                properties,
            }
        } else {
            ExceptionValue::Other(Some(if name == "null" {
                Value::Null
            } else {
                properties
            }))
        };
        let observed = RefCell::new(None);
        let result = emit_binance_exchange_error(&exception, |emission| {
            trace.borrow_mut().push(json!(["emit"]));
            match emission {
                ErrorEmission::Error(error, prefix) => {
                    assert!(
                        Rc::ptr_eq(&error, &original),
                        "REST native original exception identity"
                    );
                    assert_eq!(
                        error["marker"].as_u64(),
                        Some(7),
                        "REST native decoded exception marker"
                    );
                    observed.replace(Some(("error", Some(true), None, prefix)));
                }
                ErrorEmission::Message(message) => {
                    observed.replace(Some(("message", None, message, None)));
                }
            }
            if name == "emit-fail" {
                Err(e.clone())
            } else {
                Ok(())
            }
        });
        assert_eq!(
            observed.borrow().is_some(),
            name != "error-coercion-fail",
            "REST native emission count"
        );
        let expected_prefix = match name {
            "error-response" => Some("Exchange Error: venue"),
            "error-message" => Some("Exchange Error: fallback"),
            "error-array" => Some("Exchange Error: "),
            "error-object" => Some("Exchange Error: [object Object]"),
            _ => None,
        };
        if let Some((kind, _, message, prefix)) = observed.borrow().as_ref() {
            assert_eq!(*kind, if instance { "error" } else { "message" });
            assert_eq!(
                prefix.as_deref(),
                expected_prefix,
                "REST emit native prefix case{index}"
            );
            if !instance {
                assert_eq!(
                    *message,
                    match name {
                        "plain-response" | "emit-fail" => Some(json!("venue")),
                        "plain-message" => Some(json!("fallback")),
                        "plain-falsy" => Some(json!(false)),
                        _ => None,
                    },
                    "REST native decoded error message"
                );
            }
        }
        assert_eq!(
            result.is_err(),
            ["error-coercion-fail", "emit-fail"].contains(&name)
        );
        let observed = observed.into_inner();
        let (kind, identity, message, prefix) =
            observed.map_or((None, None, None, None), |(k, i, m, p)| (Some(k), i, m, p));
        assert_error(
            &result,
            if name == "error-coercion-fail" {
                ExpectedError::Generated(
                    "TypeError",
                    "Cannot convert object to primitive value".into(),
                )
            } else if name == "emit-fail" {
                ExpectedError::Original
            } else {
                ExpectedError::None
            },
            &e,
            "REST emit",
        );
        native.push(row(json!({"family":"emit","name":name,"trace":*trace.borrow(),"kind":kind,"identity":identity,"message":message,"undefined":message.is_none(),"prefix":prefix}),&result,&e));
    }
    compare("emit", native, 12);
}
const RETRY: [&str; 18] = [
    "boundary",
    "early",
    "overdue",
    "nan-now",
    "nan-last",
    "disposed",
    "stale",
    "first-clock-fail",
    "second-clock-fail",
    "fetch-sync-fail",
    "fetch-reject",
    "disposed-during-fetch",
    "stale-during-fetch",
    "update-getter-fail",
    "loaded-getter-fail",
    "apply-fail",
    "update-mutates-loaded",
    "loaded-disposes",
];
fn retry(family: &str) {
    let mut native = vec![];
    for (index, name) in RETRY.into_iter().enumerate() {
        let e = error();
        let trace = Rc::new(RefCell::new(vec![]));
        let state = Rc::new(OrdersRetryState {
            last_retry_at: Cell::new(if name == "nan-last" { f64::NAN } else { 0.0 }),
            is_disposed: Cell::new(name == "disposed").into(),
            snapshot_stale: Cell::new(name == "stale").into(),
        });
        let clock_reads = Cell::new(0);
        let fetches = Cell::new(0);
        let applies = Cell::new(0);
        let patch = RefCell::new(None);
        let orders=Rc::new(vec![serde_json::from_value::<Order>(json!({"id":"first","symbol":"BTCUSDT","type":"limit","side":"buy","status":"open","price":12.3,"amount":3.6,"filled":0,"remaining":3.6,"reduceOnly":false,"lastUpdate":99})).unwrap(),serde_json::from_value::<Order>(json!({"id":"second","symbol":"ETHUSDT","type":"limit","side":"sell","status":"open","price":14.2,"amount":2.1,"filled":0,"remaining":2.1,"reduceOnly":false,"lastUpdate":99})).unwrap()]);
        let loaded = RefCell::new(StoreDataLoaded {
            balance: true,
            orders: false,
            markets: true,
            tickers: false,
            positions: true,
        });
        let now = || {
            clock_reads.set(clock_reads.get() + 1);
            trace.borrow_mut().push(json!(["now", clock_reads.get()]));
            if name == "first-clock-fail" && clock_reads.get() == 1
                || name == "second-clock-fail" && clock_reads.get() == 2
            {
                return Err(e.clone());
            }
            Ok(if name == "early" {
                14999.0
            } else if name == "overdue" {
                15001.0
            } else if name == "nan-now" {
                f64::NAN
            } else if clock_reads.get() == 1 {
                15000.0
            } else {
                15007.0
            })
        };
        let fetch = || {
            fetches.set(fetches.get() + 1);
            trace.borrow_mut().push(json!(["fetch"]));
            if name == "fetch-sync-fail" {
                return Err(e.clone());
            }
            let (s, t, o, e) = (state.clone(), trace.clone(), orders.clone(), e.clone());
            Ok(async move {
                t.borrow_mut().push(json!(["resolve"]));
                if name == "disposed-during-fetch" {
                    s.is_disposed.set(true);
                }
                if name == "stale-during-fetch" {
                    s.snapshot_stale.set(true);
                }
                if name == "fetch-reject" {
                    Err(e)
                } else {
                    Ok(o)
                }
            })
        };
        let get_update = || {
            trace.borrow_mut().push(json!(["get-update"]));
            if name == "update-getter-fail" {
                return Err(e.clone());
            }
            if name == "update-mutates-loaded" {
                loaded.borrow_mut().tickers = true;
            }
            Ok(|o: Rc<Vec<Order>>, l: StoreDataLoaded| {
                trace.borrow_mut().push(json!(["apply"]));
                applies.set(applies.get() + 1);
                assert!(
                    Rc::ptr_eq(&o, &orders),
                    "REST native original orders payload identity"
                );
                assert_eq!(o.len(), 2, "REST native independently authored order count");
                number(
                    (o[0].price).to_number(),
                    12.3,
                    "REST native decoded first price",
                );
                number(o[0].amount, 3.6, "REST native decoded first amount");
                number(
                    (o[1].price).to_number(),
                    14.2,
                    "REST native decoded second price",
                );
                number(o[1].amount, 2.1, "REST native decoded second amount");
                assert_eq!(o[0].id, "first");
                assert_eq!(o[1].id, "second");
                assert!(l.orders, "REST native loaded.orders");
                assert!(l.balance && l.markets && l.positions);
                assert_eq!(l.tickers, name == "update-mutates-loaded");
                patch.replace(Some(json!({"loaded":l,"ordersIdentity":true})));
                if name == "apply-fail" {
                    Err(e.clone())
                } else {
                    Ok(())
                }
            })
        };
        let get_loaded = || {
            trace.borrow_mut().push(json!(["get-loaded"]));
            if name == "loaded-getter-fail" {
                return Err(e.clone());
            }
            if name == "loaded-disposes" {
                state.is_disposed.set(true);
            }
            Ok(loaded.borrow().clone())
        };
        let result = run(if family == "retry_futures" {
            Box::pin(retry_futures_orders_snapshot(
                &state, now, fetch, get_update, get_loaded,
            )) as Pin<Box<dyn Future<Output = Result<(), Failure>>>>
        } else {
            Box::pin(retry_pm_orders_snapshot(
                &state, now, fetch, get_update, get_loaded,
            ))
        });
        let expect_fetch = !["early", "first-clock-fail", "second-clock-fail"].contains(&name);
        let expect_apply = ![
            "early",
            "disposed",
            "stale",
            "first-clock-fail",
            "second-clock-fail",
            "fetch-sync-fail",
            "fetch-reject",
            "disposed-during-fetch",
            "stale-during-fetch",
            "update-getter-fail",
            "loaded-getter-fail",
        ]
        .contains(&name);
        assert_eq!(
            fetches.get(),
            usize::from(expect_fetch),
            "REST {family} native fetch count case{index}"
        );
        assert_eq!(
            applies.get(),
            usize::from(expect_apply),
            "REST native independent apply count"
        );
        assert_eq!(
            clock_reads.get(),
            if ["early", "first-clock-fail"].contains(&name) {
                1
            } else {
                2
            }
        );
        number(
            state.last_retry_at.get(),
            if name == "nan-last" {
                15007.0
            } else if name == "nan-now" {
                f64::NAN
            } else if ["early", "first-clock-fail", "second-clock-fail"].contains(&name) {
                0.0
            } else if name == "overdue" {
                15001.0
            } else {
                15007.0
            },
            "REST native second clock stored",
        );
        assert!(
            !loaded.borrow().orders,
            "REST native original loaded snapshot unmodified"
        );
        assert_eq!(
            result.is_err(),
            [
                "first-clock-fail",
                "second-clock-fail",
                "fetch-sync-fail",
                "fetch-reject",
                "update-getter-fail",
                "loaded-getter-fail",
                "apply-fail"
            ]
            .contains(&name)
        );
        assert_error(
            &result,
            if [
                "first-clock-fail",
                "second-clock-fail",
                "fetch-sync-fail",
                "fetch-reject",
                "update-getter-fail",
                "loaded-getter-fail",
                "apply-fail",
            ]
            .contains(&name)
            {
                ExpectedError::Original
            } else {
                ExpectedError::None
            },
            &e,
            "REST retry",
        );
        native.push(row(json!({"family":family,"name":name,"trace":*trace.borrow(),"clockReads":clock_reads.get(),"fetches":fetches.get(),"applies":applies.get(),"last":encode(state.last_retry_at.get()),"disposed":state.is_disposed.get(),"stale":state.snapshot_stale.get(),"patch":patch.into_inner(),"loaded":loaded.into_inner()}),&result,&e));
    }
    compare(family, native, 18);
}
#[test]
#[ignore = "live source oracle"]
fn futures_orders_retry() {
    retry("retry_futures");
}
#[test]
#[ignore = "live source oracle"]
fn pm_orders_retry() {
    retry("retry_pm");
}
const OVERLAY: [&str; 19] = [
    "normal",
    "duplicates",
    "nonpositive",
    "infinity",
    "missing-brackets",
    "object-brackets",
    "primitive-row",
    "null-row",
    "primitive-bracket",
    "string-brackets",
    "bad-coercion",
    "malformed-data",
    "undefined-data",
    "empty-data",
    "get-sync-fail",
    "get-reject",
    "no-load",
    "late-client",
    "overlay-repeat",
];
fn brackets(name: &str) -> Option<Value> {
    Some(match name {
        "duplicates" => {
            json!([{"symbol":"BTCUSDT","brackets":[{"initialLeverage":20}]},{"symbol":"BTCUSDT","brackets":[{"initialLeverage":70}]},{"symbol":"BTCUSDT","brackets":[{"initialLeverage":0}]}])
        }
        "nonpositive" => {
            json!([{"symbol":"BTCUSDT","brackets":[{"initialLeverage":-1}]},{"symbol":"ETHUSDT","brackets":[{"initialLeverage":"no"}]}])
        }
        "infinity" => json!([{"symbol":"BTCUSDT","brackets":[{"initialLeverage":"Infinity"}]}]),
        "missing-brackets" => json!([{"symbol":"BTCUSDT"}]),
        "object-brackets" => {
            json!([{"symbol":"BTCUSDT","brackets":{"0":{"initialLeverage":"30"}}}])
        }
        "primitive-row" => json!([7]),
        "null-row" => json!([null]),
        "primitive-bracket" => json!([{"symbol":"BTCUSDT","brackets":[7]}]),
        "string-brackets" => json!([{"symbol":"BTCUSDT","brackets":"A"}]),
        "bad-coercion" => {
            json!([{"symbol":"BTCUSDT","brackets":[{"initialLeverage":{"toString":0}}]}])
        }
        "malformed-data" => json!({"code":-1,"msg":"bad"}),
        "undefined-data" => return None,
        "empty-data" => json!([]),
        _ => {
            json!([{"symbol":"BTCUSDT","brackets":[{"initialLeverage":"125"}]},{"symbol":"ETHUSDT","brackets":[{"initial_leverage":"20"}]}])
        }
    })
}
struct Limits {
    amount: Rc<RefCell<MarketLimitsAmount>>,
    leverage: Rc<RefCell<MarketLimitsLeverage>>,
}
struct Record {
    symbol: String,
    precision: Rc<RefCell<MarketPrecision>>,
    limits: Rc<Limits>,
}
#[derive(Clone)]
struct ReferenceMarket(Rc<Record>);
impl std::ops::Deref for ReferenceMarket {
    type Target = Rc<Record>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl LeverageMarket for ReferenceMarket {
    fn symbol(&self) -> &str {
        &self.symbol
    }
    fn with_leverage_max(&self, max: f64) -> Self {
        let mut leverage = self.limits.leverage.borrow().clone();
        leverage.max = max;
        ReferenceMarket(Rc::new(Record {
            symbol: self.symbol.clone(),
            precision: self.precision.clone(),
            limits: Rc::new(Limits {
                amount: self.limits.amount.clone(),
                leverage: Rc::new(RefCell::new(leverage)),
            }),
        }))
    }
}
fn overlay(family: &str) {
    let mut native = vec![];
    let futures = family == "overlay_futures";
    for (index, name) in OVERLAY.into_iter().enumerate() {
        let e = error();
        let trace = Rc::new(RefCell::new(vec![]));
        let gets = Rc::new(Cell::new(0));
        let current = Rc::new(Cell::new(1));
        let ep = Rc::new(RefCell::new(None));
        let saved = RefCell::new(None);
        let (t, n, c, p, e1) = (
            trace.clone(),
            gets.clone(),
            current.clone(),
            ep.clone(),
            e.clone(),
        );
        let get = move |endpoint: &'static str| {
            t.borrow_mut().push(json!(["get", c.get()]));
            n.set(n.get() + 1);
            p.replace(Some(endpoint));
            if name == "get-sync-fail" {
                return Err(e1.clone());
            }
            let e = e1.clone();
            Ok(async move {
                if name == "get-reject" {
                    Err(e)
                } else {
                    Ok(brackets(name))
                }
            })
        };
        let marker = Rc::new(7);
        let load = |loader| {
            trace.borrow_mut().push(json!(["load"]));
            saved.replace(Some(loader));
            marker.clone()
        };
        let returned = if futures {
            load_futures_account_max_leverage(get, load)
        } else {
            load_pm_account_max_leverage(get, load)
        };
        assert!(
            Rc::ptr_eq(&returned, &marker),
            "REST native original overlay return"
        );
        assert_eq!(
            gets.get(),
            0,
            "REST native no eager GET before loader invocation"
        );
        if name == "late-client" {
            current.set(2);
        }
        let mut result = Ok(());
        let mut output_values = None;
        let mut identity = None;
        if name != "no-load" {
            let future = saved.borrow_mut().as_mut().unwrap()();
            assert_eq!(
                gets.get(),
                1,
                "REST native GET starts when loader invoked before poll"
            );
            match run(future) {
                Err(error) => result = Err(error),
                Ok(overlay) => {
                    let markets: Vec<_> = ["BTCUSDT", "ETHUSDT", "DOGEUSDT"]
                        .into_iter()
                        .enumerate()
                        .map(|(i, symbol)| {
                            ReferenceMarket(Rc::new(Record {
                                symbol: symbol.into(),
                                precision: Rc::new(RefCell::new(
                                    serde_json::from_value(json!({"amount":3.6,"price":12.3}))
                                        .unwrap(),
                                )),
                                limits: Rc::new(Limits {
                                    amount: Rc::new(RefCell::new(
                                        serde_json::from_value(json!({"min":0.1,"max":100}))
                                            .unwrap(),
                                    )),
                                    leverage: Rc::new(RefCell::new(
                                        serde_json::from_value(json!({"min":1,"max":i+1})).unwrap(),
                                    )),
                                }),
                            }))
                        })
                        .collect();
                    let expected = match name {
                        "duplicates" => [70.0, 2.0, 3.0],
                        "nonpositive" | "missing-brackets" | "primitive-row" | "empty-data" => {
                            [1.0, 2.0, 3.0]
                        }
                        "infinity" => [f64::INFINITY, 2.0, 3.0],
                        "object-brackets" => [30.0, 2.0, 3.0],
                        _ => [125.0, 20.0, 3.0],
                    };
                    let out = overlay.apply(markets.clone());
                    assert_eq!(out.len(), 3, "REST native independent market count");
                    let mut ids = vec![];
                    for (n, (before, after)) in markets.iter().zip(&out).enumerate() {
                        number(
                            after.limits.leverage.borrow().max,
                            expected[n],
                            &format!("REST {family} native max case{index} row{n}"),
                        );
                        number(
                            before.limits.leverage.borrow().max,
                            (n + 1) as f64,
                            "REST native original leverage unmodified",
                        );
                        number(
                            after.precision.borrow().amount,
                            3.6,
                            "REST native decoded precision",
                        );
                        number(
                            after.limits.amount.borrow().min,
                            0.1,
                            "REST native decoded amount min",
                        );
                        assert_eq!(after.symbol, before.symbol);
                        let unchanged = expected[n] == (n + 1) as f64;
                        let flags = [
                            Rc::ptr_eq(before, after),
                            Rc::ptr_eq(&before.precision, &after.precision),
                            Rc::ptr_eq(&before.limits, &after.limits),
                            Rc::ptr_eq(&before.limits.amount, &after.limits.amount),
                            Rc::ptr_eq(&before.limits.leverage, &after.limits.leverage),
                        ];
                        assert_eq!(
                            flags,
                            [unchanged, true, unchanged, true, unchanged],
                            "REST native shallow-copy referent topology"
                        );
                        ids.push(flags);
                    }
                    if name == "overlay-repeat" {
                        let again = overlay.apply(markets.clone());
                        for (n, m) in again.iter().enumerate() {
                            number(
                                m.limits.leverage.borrow().max,
                                expected[n],
                                "REST native immutable overlay repeat",
                            );
                        }
                    }
                    let dto:Market=serde_json::from_value(json!({"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":3.6,"price":12.3},"limits":{"amount":{"min":0.1,"max":100},"leverage":{"min":1,"max":1}}})).unwrap();
                    let typed = overlay.apply(vec![dto]);
                    number(
                        typed[0].limits.leverage.max,
                        expected[0],
                        "REST native production Market DTO max",
                    );
                    number(
                        typed[0].precision.amount,
                        3.6,
                        "REST native production Market DTO precision",
                    );
                    output_values = Some(
                        out.iter()
                            .map(|m| encode(m.limits.leverage.borrow().max))
                            .collect::<Vec<_>>(),
                    );
                    identity = Some(ids);
                }
            }
        }
        let fails = [
            "null-row",
            "primitive-bracket",
            "string-brackets",
            "bad-coercion",
            "malformed-data",
            "undefined-data",
            "get-sync-fail",
            "get-reject",
        ]
        .contains(&name);
        assert_eq!(result.is_err(), fails, "REST native overlay error floor");
        assert_eq!(gets.get(), usize::from(name != "no-load"));
        assert_eq!(
            *ep.borrow(),
            if name == "no-load" {
                None
            } else {
                Some(if futures {
                    "/fapi/v1/leverageBracket"
                } else {
                    "/papi/v1/um/leverageBracket"
                })
            },
            "REST native leverage endpoint"
        );
        let expected_error = match name {
            "null-row" => ExpectedError::Generated(
                "TypeError",
                "Cannot read properties of null (reading 'symbol')".into(),
            ),
            "primitive-bracket" => ExpectedError::Generated(
                "TypeError",
                "Cannot use 'in' operator to search for 'initialLeverage' in 7".into(),
            ),
            "string-brackets" => ExpectedError::Generated(
                "TypeError",
                "Cannot use 'in' operator to search for 'initialLeverage' in A".into(),
            ),
            "bad-coercion" => ExpectedError::Generated(
                "TypeError",
                "Cannot convert object to primitive value".into(),
            ),
            "malformed-data" | "undefined-data" => {
                let venue = if futures { "Binance" } else { "Binance PM" };
                let endpoint = if futures {
                    "/fapi/v1/leverageBracket"
                } else {
                    "/papi/v1/um/leverageBracket"
                };
                let detail = if name == "malformed-data" {
                    "object(code,msg) (code=-1 bad)"
                } else {
                    "undefined"
                };
                ExpectedError::Generated(
                    "Error",
                    format!(
                        "{venue} {endpoint} response malformed: expected array, received {detail}"
                    ),
                )
            }
            "get-sync-fail" | "get-reject" => ExpectedError::Original,
            _ => ExpectedError::None,
        };
        assert_error(&result, expected_error, &e, "REST overlay");
        native.push(row(json!({"family":family,"name":name,"trace":*trace.borrow(),"getCalls":gets.get(),"endpoint":*ep.borrow(),"values":output_values,"identity":identity}),&result,&e));
    }
    compare(family, native, 19);
}
#[test]
#[ignore = "live source oracle"]
fn futures_leverage_overlay() {
    overlay("overlay_futures");
}
#[test]
#[ignore = "live source oracle"]
fn pm_leverage_overlay() {
    overlay("overlay_pm");
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
