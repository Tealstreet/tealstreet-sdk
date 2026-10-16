use safe_cex_num::js_number_string;
use safe_cex_types::OrderSide;
use safe_cex_venue_paradex::{
    bracket_validation::validate_tp_sl_prices,
    lifecycle::dispose,
    sdk_reads::{
        LossFactorRequest, TokenBalanceRequest, get_paraclear_balance, get_socialized_loss_factor,
    },
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
#[derive(Debug)]
struct Marker;
type Ref = Rc<Marker>;
type Error = Rc<Marker>;
struct Held<T> {
    ready: Rc<Cell<bool>>,
    outcome: Option<Result<T, Error>>,
}
impl<T: Unpin> Future for Held<T> {
    type Output = Result<T, Error>;
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self.ready.get() {
            Poll::Ready(self.outcome.take().unwrap())
        } else {
            Poll::Pending
        }
    }
}
fn bits(n: f64) -> String {
    format!(
        "{:016x}",
        if n.is_nan() {
            0x7ff8000000000000
        } else {
            n.to_bits()
        }
    )
}
fn units(text: &str) -> Vec<u16> {
    let mut value: Vec<u16> = text.encode_utf16().collect();
    value.push(0xd800);
    value
}
fn triples() -> [(Option<f64>, Option<f64>, Option<f64>); 20] {
    [
        (None, None, None),
        (Some(0.0), Some(5.0), Some(9.0)),
        (Some(-0.0), Some(5.0), Some(9.0)),
        (Some(f64::NAN), Some(5.0), Some(9.0)),
        (Some(25.0), None, None),
        (Some(25.0), Some(30.0), Some(20.0)),
        (Some(25.0), Some(20.0), Some(30.0)),
        (Some(25.0), Some(25.0), None),
        (Some(25.0), None, Some(25.0)),
        (Some(25.0), None, Some(30.0)),
        (Some(25.0), None, Some(20.0)),
        (Some(25.0), Some(0.0), Some(-0.0)),
        (Some(25.0), Some(f64::NAN), Some(f64::NAN)),
        (Some(25.0), Some(f64::INFINITY), Some(f64::NEG_INFINITY)),
        (Some(f64::INFINITY), Some(f64::INFINITY), None),
        (Some(f64::NEG_INFINITY), None, Some(f64::NEG_INFINITY)),
        (Some(-25.0), Some(-30.0), Some(-20.0)),
        (Some(30.125), Some(31.0625), Some(29.03125)),
        (Some(1e21), Some(1e21), None),
        (Some(1e-7), None, Some(1e-7)),
    ]
}
#[test]
fn actual_typescript_sdk_operations() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/sdk_operations_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "operations_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        s => s,
    };
    assert_eq!(corpus["header"]["arch"], arch);
    assert!(corpus["header"]["nodeMajor"].as_u64().unwrap() > 0);
    let reads = corpus["reads"].as_array().unwrap();
    assert_eq!(reads.len(), 22, "sdk_source_case_count");
    let mut completed = 0;
    for kind in ["balance", "factor"] {
        for mode in [
            "ok",
            "load-sync",
            "load-async",
            "method",
            "config",
            "provider",
            "address",
            "call-sync",
            "call-async",
            "stringify-get",
            "stringify-call",
        ] {
            let source = &reads[completed];
            assert_eq!(source["kind"], kind, "sdk_live_kind");
            assert_eq!(source["mode"], mode, "sdk_live_mode");
            let trace = Rc::new(RefCell::new(Vec::<Value>::new()));
            let error = Rc::new(Marker);
            let sdk = Rc::new(Marker);
            let method = Rc::new(Marker);
            let config = Rc::new(Marker);
            let provider = Rc::new(Marker);
            let value = Rc::new(Marker);
            let address = Rc::new(units("address-"));
            let token = Rc::new(units("token-"));
            let load_ready = Rc::new(Cell::new(false));
            let read_ready = Rc::new(Cell::new(false));
            let load = || {
                trace.borrow_mut().push(json!(["load"]));
                if mode == "load-sync" {
                    Err(error.clone())
                } else {
                    Ok(Held {
                        ready: load_ready.clone(),
                        outcome: Some(if mode == "load-async" {
                            Err(error.clone())
                        } else {
                            Ok(sdk.clone())
                        }),
                    })
                }
            };
            let select = |loaded: Ref| {
                assert!(Rc::ptr_eq(&loaded, &sdk), "sdk_native_loaded_identity");
                trace.borrow_mut().push(json!(["paraclear"]));
                trace.borrow_mut().push(json!(["method", kind]));
                if mode == "method" {
                    Err(error.clone())
                } else {
                    Ok(method.clone())
                }
            };
            let fields = || {
                trace.borrow_mut().push(json!(["config"]));
                if mode == "config" {
                    return Err(error.clone());
                }
                trace.borrow_mut().push(json!(["provider"]));
                if mode == "provider" {
                    return Err(error.clone());
                }
                Ok((config.clone(), provider.clone()))
            };
            let accept =
                |bound: Ref, c: Ref, p: Ref, a: Option<Rc<Vec<u16>>>, t: Option<Rc<Vec<u16>>>| {
                    assert!(Rc::ptr_eq(&bound, &method), "sdk_native_method_identity");
                    if kind == "balance" {
                        assert!(Rc::ptr_eq(&c, &config), "balance_native_config");
                        assert!(Rc::ptr_eq(&p, &provider), "balance_native_provider");
                    } else {
                        assert!(Rc::ptr_eq(&c, &config), "factor_native_config");
                        assert!(Rc::ptr_eq(&p, &provider), "factor_native_provider");
                    }
                    let mut event = vec![
                        json!("call"),
                        json!(true),
                        json!(if kind == "balance" {
                            vec!["config", "provider", "account", "token"]
                        } else {
                            vec!["config", "provider"]
                        }),
                        json!(true),
                        json!(true),
                    ];
                    if kind == "balance" {
                        let a = a.unwrap();
                        let t = t.unwrap();
                        assert!(Rc::ptr_eq(&a, &address), "balance_native_address");
                        assert!(Rc::ptr_eq(&t, &token), "balance_native_token");
                        event.push(json!(*a));
                        event.push(json!(*t));
                    } else {
                        assert!(a.is_none() && t.is_none(), "factor_native_unused_fields");
                    }
                    trace.borrow_mut().push(Value::Array(event));
                    if mode == "call-sync" {
                        Err(error.clone())
                    } else {
                        Ok(Held {
                            ready: read_ready.clone(),
                            outcome: Some(if mode == "call-async" {
                                Err(error.clone())
                            } else {
                                Ok(value.clone())
                            }),
                        })
                    }
                };
            let stringify = |result: Ref| {
                assert!(Rc::ptr_eq(&result, &value), "sdk_native_read_value");
                trace.borrow_mut().push(json!(["stringify-get"]));
                if mode == "stringify-get" {
                    return Err(error.clone());
                }
                trace.borrow_mut().push(json!(["stringify-call", true]));
                if mode == "stringify-call" {
                    Err(error.clone())
                } else {
                    Ok(units(&format!("{kind}-")))
                }
            };
            let mut pending: Pin<Box<dyn Future<Output = Result<Vec<u16>, Error>>>> =
                if kind == "balance" {
                    Box::pin(get_paraclear_balance(
                        load,
                        select,
                        || {
                            let (c, p) = fields()?;
                            trace.borrow_mut().push(json!(["account"]));
                            trace.borrow_mut().push(json!(["address"]));
                            if mode == "address" {
                                Err(error.clone())
                            } else {
                                Ok((c, p, address.clone(), token.clone()))
                            }
                        },
                        |m, request: TokenBalanceRequest<Ref, Ref, Rc<Vec<u16>>, Rc<Vec<u16>>>| {
                            accept(
                                m,
                                request.config,
                                request.provider,
                                Some(request.account),
                                Some(request.token),
                            )
                        },
                        stringify,
                    ))
                } else {
                    Box::pin(get_socialized_loss_factor(
                        &token,
                        load,
                        select,
                        fields,
                        |m, request: LossFactorRequest<Ref, Ref>| {
                            accept(m, request.config, request.provider, None, None)
                        },
                        stringify,
                    ))
                };
            let mut context = Context::from_waker(Waker::noop());
            let mut result = pending.as_mut().poll(&mut context);
            let before = trace.borrow().clone();
            if result.is_pending() {
                load_ready.set(true);
                result = pending.as_mut().poll(&mut context);
            }
            let middle = trace.borrow().clone();
            if result.is_pending() {
                read_ready.set(true);
                result = pending.as_mut().poll(&mut context);
            }
            let Poll::Ready(result) = result else {
                panic!("sdk_native_settlement")
            };
            drop(pending);
            match result {
                Ok(text) => {
                    assert_eq!(
                        source["outcome"]["ok"].as_bool(),
                        Some(true),
                        "sdk_native_expected_success"
                    );
                    let expected: Vec<u16> = source["outcome"]["units"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|unit| u16::try_from(unit.as_u64().unwrap()).unwrap())
                        .collect();
                    assert_eq!(text, expected, "sdk_native_text");
                }
                Err(actual) => {
                    assert_eq!(
                        source["outcome"]["ok"].as_bool(),
                        Some(false),
                        "sdk_native_expected_error"
                    );
                    assert_eq!(source["outcome"]["same"].as_bool(), Some(true));
                    assert!(Rc::ptr_eq(&actual, &error), "sdk_native_error_identity");
                }
            }
            assert_eq!(
                &before,
                source["before"].as_array().unwrap(),
                "sdk_native_before"
            );
            assert_eq!(
                &middle,
                source["middle"].as_array().unwrap(),
                "sdk_native_middle"
            );
            assert_eq!(
                &*trace.borrow(),
                source["trace"].as_array().unwrap(),
                "sdk_native_trace"
            );
            completed += 1;
        }
    }
    assert_eq!(completed, 22, "sdk_completed_cases");
    let disposals = corpus["disposals"].as_array().unwrap();
    assert_eq!(disposals.len(), 6, "dispose_source_count");
    let mut completed = 0;
    for initial in [false, true] {
        for mode in ["ok", "public", "private"] {
            let source = &disposals[completed];
            assert_eq!(source["initial"].as_bool(), Some(initial));
            assert_eq!(source["mode"], mode);
            let disposed = Cell::new(initial);
            let trace = RefCell::new(Vec::<Value>::new());
            let error = Rc::new(Marker);
            let result = dispose(
                &disposed,
                || {
                    assert!(disposed.get(), "dispose_native_flag_before_public");
                    trace.borrow_mut().push(json!(["public", disposed.get()]));
                    if mode == "public" {
                        Err(error.clone())
                    } else {
                        Ok(())
                    }
                },
                || {
                    assert!(disposed.get(), "dispose_native_flag_before_private");
                    trace.borrow_mut().push(json!(["private", disposed.get()]));
                    if mode == "private" {
                        Err(error.clone())
                    } else {
                        Ok(())
                    }
                },
            );
            assert!(disposed.get(), "dispose_native_flag");
            assert_eq!(source["disposed"].as_bool(), Some(true));
            match result {
                Ok(()) => {
                    assert_eq!(source["result"]["ok"].as_bool(), Some(true));
                    assert_eq!(source["result"]["undefined"].as_bool(), Some(true));
                }
                Err(actual) => {
                    assert_eq!(source["result"]["ok"].as_bool(), Some(false));
                    assert_eq!(source["result"]["same"].as_bool(), Some(true));
                    assert!(Rc::ptr_eq(&actual, &error), "dispose_native_error_identity");
                }
            }
            assert_eq!(
                &*trace.borrow(),
                source["trace"].as_array().unwrap(),
                "dispose_native_trace"
            );
            completed += 1;
        }
    }
    assert_eq!(completed, 6, "dispose_completed_cases");
    let validations = corpus["validations"].as_array().unwrap();
    assert_eq!(validations.len(), 44, "bracket_source_count");
    let mut defs = Vec::new();
    for side in [OrderSide::Buy, OrderSide::Sell] {
        for values in triples() {
            defs.push((side, values, false));
        }
    }
    defs.extend([
        (OrderSide::Buy, (Some(25.0), Some(20.0), None), true),
        (OrderSide::Buy, (Some(25.0), None, Some(30.0)), true),
        (OrderSide::Sell, (Some(25.0), Some(30.0), None), true),
        (OrderSide::Sell, (Some(25.0), None, Some(20.0)), true),
    ]);
    assert_eq!(defs.len(), 44, "bracket_independent_count");
    let mut completed = 0;
    for (i, (side, (price, tp, sl), throw_log)) in defs.into_iter().enumerate() {
        let source = &validations[i];
        assert_eq!(
            source["side"],
            if side == OrderSide::Buy {
                "buy"
            } else {
                "sell"
            }
        );
        assert_eq!(source["throwLog"].as_bool(), Some(throw_log));
        assert_eq!(
            source["values"],
            json!([price.map(bits), tp.map(bits), sl.map(bits)]),
            "bracket_live_input"
        );
        let error = Rc::new(Marker);
        let trace = RefCell::new(Vec::<Value>::new());
        let result = validate_tp_sl_prices(side, price, tp, sl, |message, severity| {
            trace.borrow_mut().push(json!([message, severity]));
            if throw_log {
                Err(error.clone())
            } else {
                Ok(())
            }
        });
        match result {
            Ok(value) => {
                assert_eq!(source["result"]["ok"].as_bool(), Some(true));
                assert_eq!(
                    Some(value),
                    source["result"]["value"].as_bool(),
                    "bracket_native_value"
                );
            }
            Err(actual) => {
                assert_eq!(source["result"]["ok"].as_bool(), Some(false));
                assert_eq!(source["result"]["same"].as_bool(), Some(true));
                assert!(Rc::ptr_eq(&actual, &error), "bracket_native_error_identity");
            }
        }
        assert_eq!(
            &*trace.borrow(),
            source["trace"].as_array().unwrap(),
            "bracket_native_log"
        );
        completed += 1;
    }
    assert_eq!(completed, 44, "bracket_completed_cases");
    assert_eq!(js_number_string(1e21), "1e+21");
}
