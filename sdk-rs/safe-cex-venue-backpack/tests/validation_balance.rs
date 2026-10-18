use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsJsonValue as J, numeric_ingress::js_from_str};
use safe_cex_venue_backpack::account_reads::{
    cancel_all_orders, cancel_orders, fetch_balance, validate_account,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn oracle() -> Value {
    let o = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/validation_balance_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).unwrap()
}
fn bits(v: f64) -> String {
    format!(
        "{:016x}",
        if v.is_nan() {
            0x7ff8000000000000
        } else {
            v.to_bits()
        }
    )
}
fn ready<F: Future>(future: F) -> F::Output {
    let mut f = std::pin::pin!(future);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture must complete"),
    }
}
fn string(v: &str) -> J {
    J::String(v.into())
}
fn object(v: Vec<(&str, J)>) -> J {
    J::Object(v.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn probe(v: &J) -> Value {
    match v {
        J::Undefined => json!(["undefined"]),
        J::Null => json!(["null"]),
        J::Bool(v) => json!(["boolean", v]),
        J::Number(v) => json!(["number", bits(*v)]),
        J::String(v) => json!(["string", v]),
        J::BigInt(v) => json!(["bigint", v.to_string()]),
        J::Array(v) => json!(["array", v.iter().map(probe).collect::<Vec<_>>()]),
        J::Object(v) => json!([
            "object",
            v.iter()
                .map(|(k, v)| json!([k, probe(v)]))
                .collect::<Vec<_>>()
        ]),
    }
}
fn error(name: &str, message: &str) -> ExchangeError {
    let mut e = ExchangeError::new(message);
    e.name = name.into();
    e.store_subscriber_ids = vec![17];
    e
}
fn balance_inputs() -> Vec<J> {
    let mut v = vec![object(vec![
        ("netEquity", string("101.125")),
        ("netEquityAvailable", string("37.25")),
        ("netEquityLocked", string("63.875")),
        ("pnlUnrealized", string("-4.5")),
    ])];
    for input in [
        J::Undefined,
        J::Null,
        string(""),
        string("  "),
        string("-0"),
        J::Number(0.0),
        J::Number(-0.0),
        J::Number(f64::NAN),
        J::Number(f64::INFINITY),
        J::Number(f64::NEG_INFINITY),
        string("1e309"),
        string("2.75x"),
        J::Bool(true),
        J::Bool(false),
        J::Array(vec![]),
        J::Array(vec![J::Number(3.0)]),
        J::Object(vec![]),
        object(vec![("toString", J::Number(0.0))]),
        J::BigInt(5.into()),
    ] {
        v.push(object(vec![
            ("netEquity", input.clone()),
            ("netEquityAvailable", input.clone()),
            ("netEquityLocked", input.clone()),
            ("pnlUnrealized", input),
        ]));
    }
    v.extend([J::Undefined, J::Null, J::Number(7.0), J::Object(vec![])]);
    v
}
#[test]
fn actual_typescript_fetch_balance() {
    let c = oracle();
    let cases = c["balance"].as_array().unwrap();
    let inputs = balance_inputs();
    assert_eq!(cases.len(), 48, "FETCH-BALANCE independent cases");
    assert_eq!(inputs.len(), 24, "FETCH-BALANCE independent inputs");
    let mut completed = [0; 2];
    for (i, case) in cases.iter().enumerate() {
        let index = i / 2;
        let throws = i % 2 == 1;
        assert_eq!(case["index"], index);
        assert_eq!(
            case["input"],
            probe(&inputs[index]),
            "FETCH-BALANCE independent input {i}"
        );
        assert_eq!(case["logThrows"], throws);
        let trace = RefCell::new(Vec::new());
        let logger = error("LoggerError", "logger failed");
        let pending = fetch_balance(
            |endpoint| {
                trace.borrow_mut().push(json!(["get", endpoint]));
                Ok(std::future::ready(Ok(inputs[index].clone())))
            },
            |message, severity| {
                trace.borrow_mut().push(json!(["log", message, severity]));
                if throws { Err(logger.clone()) } else { Ok(()) }
            },
        );
        assert_eq!(
            trace.borrow().as_slice(),
            [json!(["get", "/api/v1/capital/collateral"])],
            "FETCH-BALANCE native eager request {i}"
        );
        assert_eq!(
            case["atConstruction"],
            json!([["get", "/api/v1/capital/collateral"]])
        );
        let result = ready(pending);
        let rejected = [18, 20, 21].contains(&index);
        assert_eq!(case["outcome"]["ok"], !rejected);
        assert_eq!(
            trace.borrow().len(),
            if rejected { 2 } else { 1 },
            "FETCH-BALANCE independent native effects {i}"
        );
        assert_eq!(
            trace.borrow().as_slice(),
            case["trace"].as_array().unwrap(),
            "FETCH-BALANCE native trace {i}"
        );
        if rejected {
            let e = result.unwrap_err();
            assert_eq!(
                e.name, case["outcome"]["name"],
                "FETCH-BALANCE native error name {i}"
            );
            assert_eq!(
                e.message, case["outcome"]["message"],
                "FETCH-BALANCE native error message {i}"
            );
            if throws {
                assert_eq!(e, logger);
            }
            completed[1] += 1;
        } else {
            let b = result.unwrap();
            assert_eq!(
                bits(b.total),
                case["outcome"]["values"][0],
                "FETCH-BALANCE native total bits case {i}"
            );
            assert_eq!(
                bits(b.free),
                case["outcome"]["values"][1],
                "FETCH-BALANCE native free bits {i}"
            );
            assert_eq!(
                bits(b.used),
                case["outcome"]["values"][2],
                "FETCH-BALANCE native used bits {i}"
            );
            assert_eq!(
                bits(b.upnl),
                case["outcome"]["values"][3],
                "FETCH-BALANCE native upnl bits {i}"
            );
            assert!(b.info.is_none() && b.currencies.is_none() && b.coins.is_none());
            assert_eq!(
                case["outcome"]["fields"],
                json!(["total", "free", "used", "upnl"])
            );
            completed[0] += 1;
        }
    }
    assert_eq!(
        completed,
        [42, 6],
        "FETCH-BALANCE completed native comparisons"
    );
    let failures = c["balanceFailures"].as_array().unwrap();
    assert_eq!(failures.len(), 4);
    for (i, case) in failures.iter().enumerate() {
        let sync = i >= 2;
        let throws = i % 2 == 1;
        assert_eq!(case["sync"], sync);
        assert_eq!(case["logThrows"], throws);
        let trace = RefCell::new(Vec::new());
        let e = error("TransportError", "network failed");
        let logger = error("LoggerError", "logger failed");
        let pending = fetch_balance(
            |endpoint| {
                trace.borrow_mut().push(json!(["get", endpoint]));
                if sync {
                    Err(e.clone())
                } else {
                    Ok(std::future::ready(Err(e.clone())))
                }
            },
            |message, severity| {
                trace.borrow_mut().push(json!(["log", message, severity]));
                if throws { Err(logger.clone()) } else { Ok(()) }
            },
        );
        assert_eq!(
            trace.borrow().len(),
            if sync { 2 } else { 1 },
            "FETCH-BALANCE native sync effects {i}"
        );
        assert_eq!(
            trace.borrow().as_slice(),
            case["atConstruction"].as_array().unwrap()
        );
        assert_eq!(
            ready(pending).unwrap_err(),
            if throws { logger } else { e },
            "FETCH-BALANCE native original failure {i}"
        );
        assert_eq!(trace.borrow().as_slice(), case["trace"].as_array().unwrap());
    }
    let raw = c["balanceBoundary"]["raw"].as_str().unwrap();
    let native = js_from_str::<String>(raw).unwrap();
    assert_eq!(native, "2\u{fffd}");
    typed_utf16::assert_projection(raw, &native);
    assert_eq!(c["balanceBoundary"]["total"], bits(2.0));
}
fn validation_inputs() -> Vec<J> {
    vec![
        J::Undefined,
        J::Null,
        J::Object(vec![]),
        object(vec![("message", string(""))]),
        object(vec![("message", J::Number(0.0))]),
        object(vec![("message", J::Number(f64::NAN))]),
        object(vec![("message", string("message λ"))]),
        object(vec![("message", J::Number(12.0))]),
        object(vec![("message", J::Number(f64::INFINITY))]),
        object(vec![("message", J::BigInt(5.into()))]),
        object(vec![("message", J::Array(vec![]))]),
        object(vec![("message", object(vec![("tag", string("object"))]))]),
        response(string("response λ"), string("fallback")),
        response(J::Number(0.0), string("fallback")),
        response(J::Bool(false), J::Number(0.0)),
        response(J::Number(f64::NAN), string("fallback")),
        response(J::Array(vec![]), string("fallback")),
        response(
            object(vec![("tag", string("response"))]),
            string("fallback"),
        ),
    ]
}
fn response(msg: J, message: J) -> J {
    object(vec![
        (
            "response",
            object(vec![("data", object(vec![("msg", msg)]))]),
        ),
        ("message", message),
    ])
}
#[test]
fn actual_typescript_validate_account() {
    let c = oracle();
    let cases = c["validation"].as_array().unwrap();
    assert_eq!(cases.len(), 110, "VALIDATE independent source cases");
    let inputs = validation_inputs();
    assert_eq!(inputs.len(), 18);
    let mut completed = [0; 2];
    for (i, case) in cases.iter().enumerate() {
        let stage = if i < 2 {
            ["ok", "balance async"][i]
        } else {
            ["capital sync", "capital async", "balance sync"][(i - 2) / 36]
        };
        let input = if i < 2 {
            object(vec![("message", string("balance failure"))])
        } else {
            inputs[((i - 2) % 36) / 2].clone()
        };
        let throws = i >= 2 && (i - 2) % 2 == 1;
        assert_eq!(case["stage"], stage);
        assert_eq!(
            case["input"],
            probe(&input),
            "VALIDATE independent input {i}"
        );
        assert_eq!(case["emitThrows"], throws);
        let shared = Rc::new(RefCell::new(input));
        let trace = RefCell::new(Vec::new());
        let token = Rc::new(7u32);
        let returned = token.clone();
        let replacement = error("EmitError", "emit failed");
        let pending = validate_account(
            |endpoint| {
                trace.borrow_mut().push(json!(["get", endpoint]));
                if stage == "capital sync" {
                    Err(shared.clone())
                } else {
                    Ok(std::future::ready(if stage == "capital async" {
                        Err(shared.clone())
                    } else {
                        Ok(())
                    }))
                }
            },
            || {
                trace.borrow_mut().push(json!(["balance"]));
                if stage == "balance sync" {
                    Err(shared.clone())
                } else {
                    Ok(std::future::ready(if stage == "balance async" {
                        Err(shared.clone())
                    } else {
                        Ok(returned)
                    }))
                }
            },
            |value| {
                trace.borrow_mut().push(json!(["emit", probe(value)]));
                if throws {
                    Err(replacement.clone())
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(
            trace.borrow().len(),
            if stage == "capital sync" { 2 } else { 1 },
            "VALIDATE independent eager effects {i}"
        );
        assert_eq!(
            trace.borrow().as_slice(),
            case["atConstruction"].as_array().unwrap()
        );
        let result = ready(pending);
        assert_eq!(
            trace.borrow().len(),
            if i < 2 {
                2
            } else if stage == "balance sync" {
                3
            } else {
                2
            },
            "VALIDATE independent final effects {i}"
        );
        assert_eq!(
            trace.borrow().as_slice(),
            case["trace"].as_array().unwrap(),
            "VALIDATE native trace {i}"
        );
        if throws {
            assert_eq!(
                result.err().unwrap(),
                replacement,
                "VALIDATE native emit rejection {i}"
            );
            assert_eq!(case["outcome"]["sameReplacement"], true);
            completed[1] += 1;
        } else {
            let actual = result.unwrap();
            assert_eq!(
                probe(&actual.error),
                case["outcome"]["error"],
                "VALIDATE native error value {i}"
            );
            assert_eq!(
                actual.balance.is_some(),
                i < 2,
                "VALIDATE native balance presence {i}"
            );
            assert_eq!(case["outcome"]["hasBalance"], i < 2);
            if i == 0 {
                let p = actual.balance.unwrap().unwrap();
                assert!(Rc::ptr_eq(&p, &token));
                assert_eq!(case["outcome"]["sameBalance"], true);
            } else if i == 1 {
                assert!(actual.balance.unwrap().is_none());
                assert_eq!(case["outcome"]["balanceUndefined"], true);
            }
            completed[0] += 1;
        }
    }
    assert_eq!(completed, [56, 54], "VALIDATE completed comparisons");
    let shared = Rc::new(RefCell::new(object(vec![("message", string("before"))])));
    let returned = shared.clone();
    let result = ready(validate_account(
        |_| Ok(std::future::ready(Err(returned))),
        || Ok(std::future::ready(Ok(()))),
        |value| {
            assert_eq!(probe(value), probe(&string("before")));
            *shared.borrow_mut() = object(vec![("message", string("after"))]);
            Ok::<_, ExchangeError>(())
        },
    ))
    .unwrap();
    assert_eq!(
        probe(&result.error),
        probe(&string("after")),
        "VALIDATE native post-emission reread"
    );
    assert_eq!(c["validationMutation"]["error"], "after");
}
type Item = Rc<String>;
type ItemFuture<E> = Pin<Box<dyn Future<Output = Result<Item, E>>>>;
const MODES: [&str; 11] = [
    "empty",
    "dense",
    "cancel sync",
    "cancel async",
    "sparse",
    "append",
    "shrink",
    "get throw",
    "element reject",
    "length throw",
    "logger",
];
fn items(mode: &str) -> Rc<RefCell<Vec<Option<Item>>>> {
    Rc::new(RefCell::new(if mode == "empty" {
        vec![]
    } else {
        vec![
            Some(Rc::new("A".into())),
            if mode == "sparse" {
                None
            } else {
                Some(Rc::new("B".into()))
            },
            Some(Rc::new("C".into())),
        ]
    }))
}
#[test]
fn actual_typescript_cancel_orders() {
    let c = oracle();
    let cases = c["series"].as_array().unwrap();
    assert_eq!(cases.len(), 22, "SERIES independent cases");
    for (i, mode) in MODES.iter().enumerate() {
        let case = &cases[i];
        assert_eq!(case["method"], "cancelOrders");
        assert_eq!(case["mode"], *mode);
        let list = items(mode);
        let trace = Rc::new(RefCell::new(Vec::new()));
        let opaque = Rc::new(vec![17u8]);
        let calls = Cell::new(0);
        let action = cancel_orders(
            || {
                if *mode == "length throw" {
                    Err(opaque.clone())
                } else {
                    Ok(list.borrow().len())
                }
            },
            |index| {
                calls.set(calls.get() + 1);
                if index == 1 && ["get throw", "logger"].contains(mode) {
                    return Err(opaque.clone());
                }
                Ok(list.borrow().get(index).and_then(Clone::clone).map(|item| {
                    Box::pin(std::future::ready(
                        if index == 1 && *mode == "element reject" {
                            Err(opaque.clone())
                        } else {
                            Ok(item)
                        },
                    )) as ItemFuture<_>
                }))
            },
            |item| {
                assert!(
                    list.borrow().iter().flatten().any(|v| Rc::ptr_eq(v, &item)),
                    "SERIES native item identity {i}"
                );
                trace.borrow_mut().push(json!(["invoke", item.as_str()]));
                if *mode == "append" && item.as_str() == "A" {
                    list.borrow_mut().push(Some(Rc::new("D".into())));
                }
                if *mode == "shrink" && item.as_str() == "A" {
                    list.borrow_mut().truncate(1);
                }
                if *mode == "cancel sync" && item.as_str() == "B" {
                    return Err(opaque.clone());
                }
                Ok(std::future::ready(
                    if *mode == "cancel async" && item.as_str() == "B" {
                        Err(opaque.clone())
                    } else {
                        Ok(())
                    },
                ))
            },
        );
        assert!(
            trace.borrow().is_empty(),
            "SERIES native deferred cancel {i}"
        );
        assert_eq!(case["atConstruction"], json!([]));
        assert_eq!(
            calls.get(),
            if ["empty", "length throw"].contains(mode) {
                0
            } else {
                1
            },
            "SERIES native eager first member {i}"
        );
        let result = ready(action);
        let counts = [0, 3, 3, 3, 2, 4, 1, 1, 1, 0, 1];
        assert_eq!(
            trace.borrow().len(),
            counts[i],
            "SERIES independent native cancellations {i}"
        );
        assert_eq!(
            trace.borrow().as_slice(),
            case["trace"].as_array().unwrap(),
            "SERIES native sequential trace {i}"
        );
        if ["get throw", "element reject", "length throw", "logger"].contains(mode) {
            assert!(
                Rc::ptr_eq(&result.unwrap_err(), &opaque),
                "SERIES native opaque original rejection {i}"
            );
            assert_eq!(case["outcome"]["sameFailure"], true);
        } else {
            result.unwrap();
            assert_eq!(case["outcome"]["ok"], true);
        }
    }
}
#[test]
fn actual_typescript_cancel_all_orders() {
    let c = oracle();
    let cases = c["series"].as_array().unwrap();
    assert_eq!(cases.len(), 22);
    for (i, mode) in MODES.iter().enumerate() {
        let case = &cases[11 + i];
        assert_eq!(case["method"], "cancelAllOrders");
        assert_eq!(case["mode"], *mode);
        let list = items(mode);
        let trace = RefCell::new(Vec::new());
        let failure = error("SeriesError", "series failed");
        let logger = error("LoggerError", "logger failed");
        let action = cancel_all_orders(
            || {
                if *mode == "length throw" {
                    Err(failure.clone())
                } else {
                    Ok(list.borrow().len())
                }
            },
            |index| {
                if index == 1 && ["get throw", "logger"].contains(mode) {
                    return Err(failure.clone());
                }
                Ok(list.borrow().get(index).and_then(Clone::clone).map(|item| {
                    Box::pin(std::future::ready(
                        if index == 1 && *mode == "element reject" {
                            Err(failure.clone())
                        } else {
                            Ok(item)
                        },
                    )) as ItemFuture<_>
                }))
            },
            |item| {
                assert!(list.borrow().iter().flatten().any(|v| Rc::ptr_eq(v, &item)));
                Ok((*item).clone())
            },
            |endpoint, body| {
                assert_eq!(endpoint, "/api/v1/orders");
                let J::Object(fields) = body else {
                    panic!("ALL-CANCEL native config")
                };
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].0, "data");
                let J::Object(fields) = &fields[0].1 else {
                    panic!("ALL-CANCEL native data")
                };
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].0, "symbol", "ALL-CANCEL native symbol field {i}");
                let J::String(id) = &fields[0].1 else {
                    panic!("ALL-CANCEL native symbol")
                };
                trace.borrow_mut().push(json!(["invoke", id]));
                if *mode == "append" && id == "A" {
                    list.borrow_mut().push(Some(Rc::new("D".into())));
                }
                if *mode == "shrink" && id == "A" {
                    list.borrow_mut().truncate(1);
                }
                if *mode == "cancel sync" && id == "B" {
                    return Err(failure.clone());
                }
                Ok(std::future::ready(
                    if *mode == "cancel async" && id == "B" {
                        Err(failure.clone())
                    } else {
                        Ok(())
                    },
                ))
            },
            |message, severity| {
                trace.borrow_mut().push(json!(["log", message, severity]));
                if *mode == "logger" {
                    Err(logger.clone())
                } else {
                    Ok(())
                }
            },
        );
        assert!(
            trace.borrow().is_empty(),
            "ALL-CANCEL native deferred writes {i}"
        );
        assert_eq!(case["atConstruction"], json!([]));
        let result = ready(action);
        assert_eq!(
            trace.borrow().len(),
            [0, 3, 3, 3, 2, 4, 1, 2, 2, 1, 2][i],
            "ALL-CANCEL independent effects {i}"
        );
        assert_eq!(
            trace.borrow().as_slice(),
            case["trace"].as_array().unwrap(),
            "ALL-CANCEL native full trace {i}"
        );
        if ["get throw", "element reject", "length throw", "logger"].contains(mode) {
            assert_eq!(
                result.unwrap_err(),
                if *mode == "logger" { logger } else { failure },
                "ALL-CANCEL native error precedence {i}"
            );
        } else {
            result.unwrap();
            assert_eq!(case["outcome"]["ok"], true);
        }
    }
}
#[test]
fn actual_typescript_serial_cancellation_waits() {
    let c = oracle();
    let cases = c["waits"].as_array().unwrap();
    assert_eq!(cases.len(), 2, "SERIES-WAIT independent methods");
    for (method, case) in cases.iter().enumerate() {
        assert_eq!(
            case["method"],
            ["cancelOrders", "cancelAllOrders"][method],
            "SERIES-WAIT independent method identity {method}"
        );
        let item_ready = Cell::new(false);
        let cancel_ready = Cell::new(false);
        let trace = RefCell::new(Vec::new());
        let element = |index| -> Result<
            Option<Pin<Box<dyn Future<Output = Result<Item, ExchangeError>> + '_>>>,
            ExchangeError,
        > {
            let id = Rc::new(if index == 0 { "A" } else { "B" }.to_string());
            let item_ready = &item_ready;
            Ok(Some(Box::pin(std::future::poll_fn(move |_| {
                if index == 0 && !item_ready.get() {
                    Poll::Pending
                } else {
                    Poll::Ready(Ok(id.clone()))
                }
            }))))
        };
        let effect = |id: String| -> Result<
            Pin<Box<dyn Future<Output = Result<(), ExchangeError>> + '_>>,
            ExchangeError,
        > {
            trace.borrow_mut().push(json!(["start", id]));
            let cancel_ready = &cancel_ready;
            let trace = &trace;
            Ok(Box::pin(std::future::poll_fn(move |_| {
                if id == "A" && !cancel_ready.get() {
                    Poll::Pending
                } else {
                    trace.borrow_mut().push(json!(["finish", id]));
                    Poll::Ready(Ok(()))
                }
            })))
        };
        let mut action = if method == 0 {
            cancel_orders(|| Ok(2), element, |id| effect((*id).clone()))
        } else {
            cancel_all_orders(
                || Ok(2),
                element,
                |id| Ok((*id).clone()),
                |endpoint, body| {
                    assert_eq!(endpoint, "/api/v1/orders");
                    let J::Object(root) = body else { panic!() };
                    let J::Object(data) = &root[0].1 else {
                        panic!()
                    };
                    let J::String(id) = &data[0].1 else { panic!() };
                    effect(id.clone())
                },
                |_, _| panic!("unexpected native wait log"),
            )
        };
        let mut context = Context::from_waker(Waker::noop());
        assert!(trace.borrow().is_empty());
        assert_eq!(case["before"], json!([]));
        assert!(
            matches!(action.as_mut().poll(&mut context), Poll::Pending),
            "SERIES-WAIT native unresolved element method{method}"
        );
        assert!(trace.borrow().is_empty());
        item_ready.set(true);
        assert!(
            matches!(action.as_mut().poll(&mut context), Poll::Pending),
            "SERIES-WAIT native after element resolution method{method}"
        );
        assert_eq!(trace.borrow().as_slice(), [json!(["start", "A"])]);
        assert_eq!(case["afterItem"], json!([["start", "A"]]));
        assert_eq!(case["whileCancelPending"], json!([["start", "A"]]));
        assert!(matches!(action.as_mut().poll(&mut context), Poll::Pending));
        assert_eq!(
            trace.borrow().len(),
            1,
            "SERIES-WAIT independent no overlapping callback {method}"
        );
        cancel_ready.set(true);
        assert!(
            matches!(action.as_mut().poll(&mut context), Poll::Ready(Ok(()))),
            "SERIES-WAIT native completed method{method}"
        );
        assert_eq!(
            trace.borrow().as_slice(),
            [
                json!(["start", "A"]),
                json!(["finish", "A"]),
                json!(["start", "B"]),
                json!(["finish", "B"])
            ]
        );
        assert_eq!(trace.borrow().as_slice(), case["after"].as_array().unwrap());
    }
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
