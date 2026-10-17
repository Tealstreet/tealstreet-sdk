use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::PositionMode;
use safe_cex_venue_okx::state_transitions::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn ready<T>(future: impl Future<Output = T>) -> T {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected pending native future"),
    }
}

#[test]
fn state_transitions_native_before_source() {
    let mut projections = vec![];
    let mut native = vec![];
    for kind in ["mode", "level"] {
        for mode in [PositionMode::OneWay, PositionMode::Hedged] {
            for level in [
                0.0_f64,
                -0.0,
                1.5,
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
            ] {
                for force in [None, Some(false), Some(true)] {
                    for fail in [false, true] {
                        let calls = RefCell::new(vec![]);
                        let release = Cell::new(false);
                        let mut cause = ExchangeError::new("state callback failure");
                        cause.name = "TypeError".into();
                        cause.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
                            "STATE"
                        )));
                        cause.store_subscriber_ids = vec![7, 3];
                        let error = Rc::new(cause);
                        let get_config = |value| {
                            calls.borrow_mut().push(value);
                            std::future::poll_fn(|_| {
                                if !release.get() {
                                    Poll::Pending
                                } else if fail {
                                    Poll::Ready(Err(error.clone()))
                                } else {
                                    Poll::Ready(Ok((mode, level)))
                                }
                            })
                        };
                        let mut pending = Box::pin(async {
                            if kind == "mode" {
                                get_or_fetch_position_mode(force, get_config)
                                    .await
                                    .map(|value| {
                                        assert_eq!(value, mode, "OKX-STATE native projected mode");
                                        json!({"mode":value})
                                    })
                            } else {
                                get_or_fetch_account_level(force, get_config)
                                    .await
                                    .map(|value| {
                                        assert_eq!(
                                            value.to_bits(),
                                            level.to_bits(),
                                            "OKX-STATE native level bits"
                                        );
                                        json!({"bits":format!("{:016x}",value.to_bits())})
                                    })
                            }
                        });
                        assert!(
                            pending
                                .as_mut()
                                .poll(&mut Context::from_waker(Waker::noop()))
                                .is_pending(),
                            "OKX-STATE native awaits dependency"
                        );
                        assert_eq!(
                            *calls.borrow(),
                            vec![force.unwrap_or(false)],
                            "OKX-STATE native force {kind}/{force:?}"
                        );
                        release.set(true);
                        let result = ready(pending);
                        let outcome = match result {
                            Ok(value) => {
                                assert!(!fail, "native successful projection");
                                value
                            }
                            Err(actual) => {
                                assert!(fail, "native rejected projection");
                                assert!(
                                    Rc::ptr_eq(&actual, &error),
                                    "OKX-STATE native error pointer"
                                );
                                assert_eq!(*actual, *error, "OKX-STATE native complete error");
                                json!({"error":{"name":actual.name,"message":actual.message,"same":true}})
                            }
                        };
                        let mut case = json!({"kind":kind,"mode":mode,"levelBits":format!("{:016x}",level.to_bits()),"fail":fail});
                        if let Some(force) = force {
                            case["force"] = json!(force);
                        }
                        projections.push(case);
                        native.push(json!({"calls":*calls.borrow(),"outcome":outcome}));
                    }
                }
            }
        }
    }
    let overlay = ready(fetch_account_market_overlay());
    assert_eq!(
        overlay,
        Value::Null,
        "OKX-STATE native overlay explicit null"
    );
    let mut warnings = vec![];
    for fail in [false, true] {
        let messages = RefCell::new(vec![]);
        let mut cause = ExchangeError::new("state callback failure");
        cause.name = "TypeError".into();
        cause.context = Some(json!({"retained":17}));
        let error = Rc::new(cause);
        let result = ready(change_margin_type(|text| {
            messages.borrow_mut().push(text.to_owned());
            if fail { Err(error.clone()) } else { Ok(()) }
        }));
        assert_eq!(
            *messages.borrow(),
            vec!["Only cross mode is supported"],
            "OKX-STATE native warning text"
        );
        if fail {
            let actual = result.as_ref().unwrap_err();
            assert!(
                Rc::ptr_eq(actual, &error),
                "OKX-STATE native warning error pointer"
            );
            assert_eq!(actual, &error, "native complete warning error");
        } else {
            result.as_ref().unwrap();
        }
        warnings.push(json!({"messages":*messages.borrow(),"error":result.err().map(|e|json!({"name":e.name,"message":e.message,"same":true}))}));
    }
    assert_eq!(projections.len(), 144, "declared projection profile floor");
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/state-transitions-oracle.mjs"
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
        .write_all(&serde_json::to_vec(&json!({"projections":projections})).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["projections"].as_array().unwrap().len(),
        144,
        "source projection floor"
    );
    assert_eq!(
        source["projections"],
        json!(native),
        "live projection after native values/pointers"
    );
    assert_eq!(source["overlay"], overlay, "live explicit null overlay");
    assert_eq!(
        source["warnings"],
        json!(warnings),
        "live warning after native identities"
    );
}
