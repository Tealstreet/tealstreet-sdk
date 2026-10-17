use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_kucoin::private_socket::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};

fn failure() -> ExchangeError {
    let mut error = ExchangeError::new("private operation failure");
    error.name = "TypeError".into();
    error.code = Some(safe_cex_exchange::js_runtime::value::Js::str(
        "PRIVATE_SEND",
    ));
    error.context = Some(json!({"retained":17}));
    error.store_subscriber_ids = vec![7, 3];
    error
}

fn source(cases: &[Value], count: usize) -> Value {
    assert_eq!(
        cases.len(),
        count,
        "independent private lifecycle profile floor"
    );
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-lifecycle.cjs"
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
        .write_all(&serde_json::to_vec(cases).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source.as_array().unwrap().len(),
        count,
        "source private lifecycle floor"
    );
    source
}

#[test]
fn private_subscriptions_native_before_source() {
    let mut cases = vec![];
    let mut native = vec![];
    let mut total = 0;
    for venue in ["kucoin", "kucoinspot"] {
        let topics: &[&str] = if venue == "kucoin" {
            &[
                "/contractMarket/tradeOrders",
                "/contractMarket/advancedOrders",
                "/contract/positionAll",
                "/contractAccount/wallet",
                "/contract/marginMode",
                "/contract/crossLeverage",
            ]
        } else {
            &["/spotMarket/tradeOrdersV2", "/account/balance"]
        };
        for initial in [false, true] {
            for fail_at in [-1_i32, 0, 1, 5] {
                let subscribed = Cell::new(initial);
                let events = RefCell::new(vec![]);
                let frames = RefCell::new(vec![]);
                let dates = Cell::new(0);
                let now = || {
                    events.borrow_mut().push("date");
                    let index = dates.get();
                    dates.set(index + 1);
                    123.0 + index as f64
                };
                let error = Rc::new(failure());
                let send = |frame: &str| {
                    if venue == "kucoin" {
                        assert!(
                            subscribed.get(),
                            "KUCOIN-PRIVATE native subscribed before send"
                        );
                    }
                    events.borrow_mut().push("send");
                    let index = frames.borrow().len();
                    frames.borrow_mut().push(frame.to_owned());
                    if index as i32 == fail_at {
                        Err(error.clone())
                    } else {
                        Ok(())
                    }
                };
                let result = if venue == "kucoin" {
                    subscribe_futures_private(&subscribed, now, send)
                } else {
                    subscribe_spot_private(now, send)
                };
                let active = venue != "kucoin" || !initial;
                let expected_count = if !active {
                    0
                } else if fail_at >= 0 && (fail_at as usize) < topics.len() {
                    fail_at as usize + 1
                } else {
                    topics.len()
                };
                let expected_error =
                    (active && fail_at >= 0 && (fail_at as usize) < topics.len()).then_some(&error);
                assert_eq!(
                    result.as_ref().err(),
                    expected_error,
                    "KUCOIN-PRIVATE native complete subscription error"
                );
                assert_eq!(
                    subscribed.get(),
                    if venue == "kucoin" { true } else { initial },
                    "KUCOIN-PRIVATE native retained subscribed"
                );
                assert_eq!(
                    frames.borrow().len(),
                    expected_count,
                    "KUCOIN-PRIVATE native frame count"
                );
                assert_eq!(
                    dates.get(),
                    expected_count,
                    "KUCOIN-PRIVATE native date count"
                );
                assert_eq!(
                    *events.borrow(),
                    ["date", "send"].repeat(expected_count),
                    "KUCOIN-PRIVATE native ordered effects"
                );
                for (index, frame) in frames.borrow().iter().enumerate() {
                    let decoded: Value = serde_json::from_str(frame).unwrap();
                    assert_eq!(
                        decoded.as_object().unwrap().len(),
                        5,
                        "KUCOIN-PRIVATE native field count"
                    );
                    assert_eq!(
                        decoded["id"].as_f64(),
                        Some(123.0 + index as f64),
                        "KUCOIN-PRIVATE native timestamp"
                    );
                    assert_eq!(
                        decoded["type"], "subscribe",
                        "KUCOIN-PRIVATE native operation"
                    );
                    assert_eq!(
                        decoded["topic"], topics[index],
                        "KUCOIN-PRIVATE native topic {venue}/{index}"
                    );
                    assert_eq!(
                        decoded["privateChannel"].as_bool(),
                        Some(true),
                        "KUCOIN-PRIVATE native private flag"
                    );
                    assert_eq!(
                        decoded["response"].as_bool(),
                        Some(true),
                        "KUCOIN-PRIVATE native response flag"
                    );
                }
                let same = result
                    .as_ref()
                    .err()
                    .is_some_and(|actual| Rc::ptr_eq(actual, &error));
                assert_eq!(
                    same,
                    expected_error.is_some(),
                    "KUCOIN-PRIVATE native error pointer identity"
                );
                total += expected_count;
                cases.push(
                    json!({"kind":"subscribe","venue":venue,"subscribed":initial,"failAt":fail_at}),
                );
                native.push(json!({"events":*events.borrow(),"frames":*frames.borrow(),"subscribed":subscribed.get(),
                    "error":result.err().map(|e|json!({"name":e.name,"message":e.message,"same":same}))}));
            }
        }
    }
    assert_eq!(total, 29, "fifteen futures plus twice seven spot frames");
    let subscribed = Cell::new(false);
    let sends = Cell::new(0);
    subscribe_futures_private(
        &subscribed,
        || 123.0,
        |_: &str| {
            sends.set(sends.get() + 1);
            subscribe_futures_private::<ExchangeError>(
                &subscribed,
                || panic!("reentry must not read clock"),
                |_| panic!("reentry must not send"),
            )
        },
    )
    .unwrap();
    assert_eq!(
        sends.get(),
        6,
        "KUCOIN-PRIVATE native guarded reentry six sends"
    );
    assert_eq!(
        json!(native),
        source(&cases, 16),
        "live subscriptions after all native identities"
    );
}

#[test]
fn private_open_native_before_source() {
    let mut cases = vec![];
    let mut native = vec![];
    for venue in ["kucoin", "kucoinspot"] {
        for disposed in [false, true] {
            for initial in [false, true] {
                for reconnecting in [false, true] {
                    for fail in ["none", "subscribe", "ping", "snapshot"] {
                        let subscribed = Cell::new(initial);
                        let events = RefCell::new(vec![]);
                        let error = Rc::new(failure());
                        let subscribe = || {
                            events.borrow_mut().push("subscribe".to_owned());
                            if venue == "kucoin" {
                                assert!(
                                    !subscribed.get(),
                                    "KUCOIN-OPEN native reset before subscribe"
                                );
                                subscribed.set(true);
                            }
                            if fail == "subscribe" {
                                Err(error.clone())
                            } else {
                                Ok(())
                            }
                        };
                        let ping = || {
                            events.borrow_mut().push("ping".into());
                            if fail == "ping" {
                                Err(error.clone())
                            } else {
                                Ok(())
                            }
                        };
                        let snapshot = |batch: Option<u32>| {
                            events
                                .borrow_mut()
                                .push(batch.map_or("snapshot".into(), |n| format!("snapshot:{n}")));
                            if fail == "snapshot" {
                                Err(error.clone())
                            } else {
                                Ok(())
                            }
                        };
                        let result = if venue == "kucoin" {
                            on_open_futures_private(
                                disposed,
                                &subscribed,
                                subscribe,
                                ping,
                                reconnecting,
                                || snapshot(None),
                            )
                        } else {
                            on_open_spot_private(disposed, subscribe, ping, reconnecting, |n| {
                                snapshot(Some(n))
                            })
                        };
                        let mut expected = vec![];
                        if !disposed {
                            expected.push("subscribe".to_owned());
                            if fail != "subscribe" {
                                expected.push("ping".into());
                                if fail != "ping" && reconnecting {
                                    expected.push(if venue == "kucoin" {
                                        "snapshot".into()
                                    } else {
                                        "snapshot:10".into()
                                    });
                                }
                            }
                        }
                        let rejected = !disposed
                            && (fail == "subscribe"
                                || fail == "ping"
                                || (fail == "snapshot" && reconnecting));
                        assert_eq!(
                            *events.borrow(),
                            expected,
                            "KUCOIN-OPEN native call order {venue}/{disposed}/{initial}/{reconnecting}/{fail}"
                        );
                        assert_eq!(
                            result.as_ref().err(),
                            rejected.then_some(&error),
                            "KUCOIN-OPEN native complete synchronous error"
                        );
                        assert_eq!(
                            subscribed.get(),
                            if venue == "kucoin" && !disposed {
                                true
                            } else {
                                initial
                            },
                            "KUCOIN-OPEN native subscribed state"
                        );
                        let same = result
                            .as_ref()
                            .err()
                            .is_some_and(|actual| Rc::ptr_eq(actual, &error));
                        assert_eq!(same, rejected, "KUCOIN-OPEN native error pointer identity");
                        cases.push(json!({"kind":"open","venue":venue,"disposed":disposed,"subscribed":initial,"reconnecting":reconnecting,"failure":fail}));
                        native.push(json!({"events":*events.borrow(),"frames":[],"subscribed":subscribed.get(),
                            "error":result.err().map(|e|json!({"name":e.name,"message":e.message,"same":same}))}));
                    }
                }
            }
        }
    }
    assert_eq!(
        json!(native),
        source(&cases, 64),
        "live onOpen after native state/error observations"
    );
}

#[test]
fn private_subscription_open_core_composition() {
    for venue in ["kucoin", "kucoinspot"] {
        let fixture = super::setup(123.0, 1.0);
        let subscribed = Cell::new(false);
        let result = if venue == "kucoin" {
            subscribe_futures_private(
                &subscribed,
                || 123.0,
                |frame| fixture.socket.send_with_log(frame),
            )
        } else {
            subscribe_spot_private(|| 123.0, |frame| fixture.socket.send_with_log(frame))
        };
        result.unwrap();
        assert_eq!(
            fixture.runtime.io.frames.borrow().len(),
            if venue == "kucoin" { 6 } else { 2 },
            "KUCOIN-PRIVATE native OPEN core frames"
        );
        for frame in fixture.runtime.io.frames.borrow().iter() {
            let decoded: Value = serde_json::from_str(frame).unwrap();
            assert_eq!(
                decoded["privateChannel"], true,
                "KUCOIN-PRIVATE native OPEN core private flag"
            );
        }
        fixture.runtime.jobs.borrow_mut().clear();
    }
}
