use safe_cex_venue_okx::{public_lifecycle::*, types::OkxInstrumentType};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};
#[path = "../../safe-cex-venue-kucoin/tests/support/public_open.rs"]
mod support;
use support::error;

fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}
fn pack(value: f64) -> Value {
    if value.is_nan() {
        json!({"nan":true})
    } else {
        json!({"bits":bits(value)})
    }
}
#[test]
fn public_lifecycle_native_before_source() {
    let (open, native_open) = support::open_cases(
        "okx",
        &[
            "orderBookSequenceByHandler.clear",
            "recoveringOrderBooks.clear",
            "subscribe",
            "ping",
            "emitter.off:markets:changed",
            "emitter.on:markets:changed",
        ],
        |disposed, invoke, enabled| on_open_public(disposed, invoke, enabled),
    );
    let instrument_fixtures = [
        vec![],
        vec![OkxInstrumentType::Swap],
        vec![OkxInstrumentType::Swap, OkxInstrumentType::Futures],
        vec![
            OkxInstrumentType::Swap,
            OkxInstrumentType::Swap,
            OkxInstrumentType::Futures,
        ],
    ];
    let mut subscriptions = vec![];
    let mut native_subscriptions = vec![];
    for business in [false, true] {
        for instruments in &instrument_fixtures {
            for count in [0, 1, 100, 101, 201] {
                for fail_at in [-2, -1, 0, 1, 2, 3] {
                    let error = error();
                    let trace = RefCell::new(vec![]);
                    let frames = RefCell::new(vec![]);
                    let send_index = Cell::new(0);
                    let live_topics = RefCell::new(Vec::<Rc<RefCell<Vec<u16>>>>::new());
                    let snapshot = RefCell::new(Vec::<Rc<RefCell<Vec<u16>>>>::new());
                    let instrument_types = || {
                        trace.borrow_mut().push("instruments".to_owned());
                        if fail_at == -2 {
                            Err(error.clone())
                        } else {
                            Ok(instruments.clone())
                        }
                    };
                    let flatten = || {
                        trace.borrow_mut().push("topics".to_owned());
                        if fail_at == -1 {
                            return Err(error.clone());
                        }
                        let topics = live_topics.borrow().clone();
                        *snapshot.borrow_mut() = topics.clone();
                        Ok(topics)
                    };
                    let send = |operation: &str, args: &[SubscriptionTopic<RefCell<Vec<u16>>>]| {
                        let i = send_index.get();
                        send_index.set(i + 1);
                        trace.borrow_mut().push(format!("send:{i}"));
                        assert_eq!(operation, "subscribe", "OKX-SUB native operation");
                        let mut captured = vec![];
                        if i == 0 {
                            assert_eq!(
                                args.len(),
                                instruments.len() * 2 + usize::from(!business),
                                "OKX-SUB native header length"
                            );
                            let mut offset = 0;
                            if !business {
                                assert!(
                                    matches!(args[0], SubscriptionTopic::Account),
                                    "OKX-SUB native account first"
                                );
                                captured.push(json!({"channel":"account"}));
                                offset = 1;
                            }
                            for (j, instrument) in instruments.iter().enumerate() {
                                for (k, channel) in (if business {
                                    ["orders-algo", "algo-advance"]
                                } else {
                                    ["positions", "orders"]
                                })
                                .iter()
                                .enumerate()
                                {
                                    let SubscriptionTopic::Product {
                                        channel: actual,
                                        instrument_type,
                                    } = args[offset + 2 * j + k]
                                    else {
                                        panic!("native product shape")
                                    };
                                    assert_eq!(
                                        actual, *channel,
                                        "OKX-SUB native product channel/order"
                                    );
                                    assert_eq!(
                                        instrument_type, *instrument,
                                        "OKX-SUB native instrument type"
                                    );
                                    captured
                                        .push(json!({"channel":actual,"instType":instrument_type}));
                                }
                            }
                        } else {
                            let start = (i - 1) as usize * 100;
                            let expected_len = (count - start).min(100);
                            assert_eq!(args.len(), expected_len, "OKX-SUB native batch floor");
                            for (j, arg) in args.iter().enumerate() {
                                let SubscriptionTopic::Existing(topic) = arg else {
                                    panic!("native existing shape")
                                };
                                assert!(
                                    Rc::ptr_eq(topic, &snapshot.borrow()[start + j]),
                                    "OKX-SUB native topic pointer"
                                );
                                let expected = if i > 1 && start + j == count - 1 {
                                    vec![0xd800, 77]
                                } else {
                                    format!("S{}", start + j).encode_utf16().collect()
                                };
                                assert_eq!(
                                    *topic.borrow(),
                                    expected,
                                    "OKX-SUB native live topic units"
                                );
                                captured.push(json!({"channel":"books","instId":*topic.borrow()}));
                            }
                        }
                        frames
                            .borrow_mut()
                            .push(json!({"op":operation,"args":captured}));
                        if i == fail_at {
                            return Err(error.clone());
                        }
                        if business && i == 0 {
                            *live_topics.borrow_mut() = (0..count)
                                .map(|j| {
                                    Rc::new(RefCell::new(format!("S{j}").encode_utf16().collect()))
                                })
                                .collect();
                        }
                        if business && i == 1 && count > 100 {
                            *snapshot.borrow()[count - 1].borrow_mut() = vec![0xd800, 77];
                        }
                        Ok(())
                    };
                    let result = if business {
                        subscribe_business(instrument_types, flatten, send)
                    } else {
                        subscribe_private(instrument_types, send)
                    };
                    let batch_count = count.div_ceil(100);
                    let attempted = if !business { 1 } else { 1 + batch_count };
                    let mut expected_trace = vec!["instruments".to_owned()];
                    let mut rejected = fail_at == -2;
                    if !rejected {
                        expected_trace.push("send:0".into());
                        rejected = fail_at == 0;
                        if business && !rejected {
                            expected_trace.push("topics".into());
                            rejected = fail_at == -1;
                            if !rejected {
                                for i in 1..attempted {
                                    expected_trace.push(format!("send:{i}"));
                                    if i as i32 == fail_at {
                                        rejected = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    assert_eq!(
                        *trace.borrow(),
                        expected_trace,
                        "OKX-SUB native late topic snapshot and send order"
                    );
                    assert_eq!(
                        result.is_err(),
                        rejected,
                        "native subscription error branch"
                    );
                    let expected_frames = if fail_at == -2 {
                        0
                    } else if !business || fail_at == 0 || fail_at == -1 {
                        1
                    } else if fail_at > 0 && (fail_at as usize) < attempted {
                        fail_at as usize + 1
                    } else {
                        attempted
                    };
                    assert_eq!(
                        frames.borrow().len(),
                        expected_frames,
                        "OKX-SUB native frame count"
                    );
                    if let Err(actual) = &result {
                        assert!(
                            Rc::ptr_eq(actual, &error),
                            "native subscription error pointer"
                        );
                        assert_eq!(actual, &error, "native complete subscription error");
                    }
                    subscriptions.push(json!({"business":business,"instruments":instruments,"count":count,"failAt":fail_at}));
                    native_subscriptions.push(json!({"trace":*trace.borrow(),"frames":*frames.borrow(),"error":result.err().map(|e|json!({"name":e.name,"message":e.message,"same":true}))}));
                }
            }
        }
    }
    assert_eq!(subscriptions.len(), 240, "declared subscription floor");
    let numeric = [
        (3., Some(2.), false, false, 7., 42.),
        (3., Some(2.), true, false, 7., 6.),
        (3., Some(2.), true, true, 7., 42.),
        (3., Some(0.), false, false, 7., 21.),
        (3., Some(f64::NAN), false, false, 7., 21.),
        (-0., Some(2.), false, false, 7., -0.),
        (f64::INFINITY, Some(2.), true, false, 7., f64::INFINITY),
        (3., Some(f64::INFINITY), false, false, 0., f64::NAN),
        (1e308, Some(1e-308), false, false, 2., f64::INFINITY),
        (f64::NAN, Some(2.), true, false, 7., f64::NAN),
        (3., Some(-2.), false, false, 7., -42.),
        (3., None, true, false, 7., 3.),
    ];
    let mut notional = vec![];
    let mut native_notional = vec![];
    for (contracts, size, inverse, different, mark, expected) in numeric {
        for fail_at in [-1, 0, 1, 2, 3] {
            let trace = RefCell::new(vec![]);
            let error = error();
            let ccy = if different {
                vec![66, 84, 67]
            } else {
                vec![0xd800, 65]
            };
            let field = |i, name: &str| {
                trace.borrow_mut().push(name.to_owned());
                if fail_at == i {
                    Err(error.clone())
                } else {
                    Ok(())
                }
            };
            let result = contracts_to_notional(
                contracts,
                mark,
                || {
                    field(0, "size")?;
                    Ok(size)
                },
                || {
                    field(1, "ccy")?;
                    Ok(ccy.clone())
                },
                || {
                    field(2, "inverse")?;
                    Ok(inverse)
                },
                || {
                    field(3, "quote")?;
                    Ok(vec![0xd800, 65])
                },
            );
            let regular = if inverse {
                vec!["size", "ccy", "inverse", "quote"]
            } else {
                vec!["size", "ccy", "inverse"]
            };
            let rejected = fail_at >= 0 && (fail_at as usize) < regular.len();
            let expected_trace = if rejected {
                &regular[..fail_at as usize + 1]
            } else {
                &regular[..]
            };
            assert_eq!(
                *trace.borrow(),
                expected_trace,
                "NOTIONAL native getter order"
            );
            let outcome = if rejected {
                let actual = result.unwrap_err();
                assert!(Rc::ptr_eq(&actual, &error), "native notional error pointer");
                assert_eq!(actual, error, "native complete notional error");
                json!({"error":{"name":actual.name,"message":actual.message,"same":true}})
            } else {
                let actual = result.unwrap();
                if expected.is_nan() {
                    assert!(actual.is_nan(), "NOTIONAL native NaN classification");
                } else {
                    assert_eq!(
                        actual.to_bits(),
                        expected.to_bits(),
                        "NOTIONAL native scalar value"
                    );
                }
                pack(actual)
            };
            notional.push(json!({"contracts":bits(contracts),"size":size.map(bits),"inverse":inverse,"ccy":ccy,"quote":[0xd800,97],"mark":bits(mark),"failAt":fail_at}));
            native_notional.push(json!({"trace":*trace.borrow(),"outcome":outcome}));
        }
    }
    assert_eq!(notional.len(), 60, "declared notional floor");
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/public-lifecycle-oracle.mjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            &serde_json::to_vec(
                &json!({"open":open,"subscriptions":subscriptions,"notional":notional}),
            )
            .unwrap(),
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source,
        json!({"open":native_open,"subscriptions":native_subscriptions,"notional":native_notional}),
        "live source after all native assertions"
    );
}
