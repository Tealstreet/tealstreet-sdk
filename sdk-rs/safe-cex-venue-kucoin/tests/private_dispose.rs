use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_kucoin::private_dispose::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};

#[derive(Clone)]
enum Handle {
    Number(i32),
    Object(Rc<u8>),
}
fn truthy(handle: &Handle) -> bool {
    match handle {
        Handle::Number(value) => *value != 0,
        Handle::Object(_) => true,
    }
}
fn tag(handle: Option<&Handle>) -> &'static str {
    match handle {
        None => "none",
        Some(Handle::Number(0)) => "zero",
        Some(Handle::Number(_)) => "number",
        Some(Handle::Object(_)) => "object",
    }
}
fn handle(kind: &str, object: &Rc<u8>) -> Option<Handle> {
    match kind {
        "none" => None,
        "zero" => Some(Handle::Number(0)),
        "number" => Some(Handle::Number(7)),
        _ => Some(Handle::Object(object.clone())),
    }
}

#[test]
fn private_dispose_native_slots_before_source() {
    let mut cases = vec![];
    let mut native = vec![];
    for venue in ["kucoin", "kucoinspot"] {
        for orders_kind in ["none", "zero", "number", "object"] {
            let balances: &[&str] = if venue == "kucoin" {
                &["none"]
            } else {
                &["none", "zero", "number", "object"]
            };
            for balance_kind in balances {
                let failures: &[(i32, bool)] = if venue == "kucoin" {
                    &[(-1, false), (0, false), (-1, true)]
                } else {
                    &[(-1, false), (0, false), (1, false), (-1, true)]
                };
                for &(fail_at, fail_super) in failures {
                    let object = Rc::new(9);
                    let orders = RefCell::new(handle(orders_kind, &object));
                    let balance = RefCell::new(handle(balance_kind, &object));
                    let events = RefCell::new(vec![]);
                    let clears = Cell::new(0);
                    let mut cause = ExchangeError::new("dispose callback failure");
                    cause.name = "TypeError".into();
                    cause.code = Some(safe_cex_exchange::js_runtime::value::Js::str("CLEAR"));
                    cause.store_subscriber_ids = vec![7, 3];
                    let error = Rc::new(cause);
                    let clear = |value: Handle| {
                        let slot = if clears.get() == 0
                            && orders_kind != "none"
                            && orders_kind != "zero"
                        {
                            &orders
                        } else {
                            &balance
                        };
                        assert_ne!(
                            tag(slot.borrow().as_ref()),
                            "none",
                            "KUCOIN-DISPOSE native populated slot during clear"
                        );
                        if let Handle::Object(pointer) = &value {
                            assert!(
                                Rc::ptr_eq(pointer, &object),
                                "KUCOIN-DISPOSE native handle pointer"
                            );
                        }
                        events
                            .borrow_mut()
                            .push(format!("clear:{}", tag(Some(&value))));
                        let index = clears.get();
                        clears.set(index + 1);
                        if index == fail_at {
                            Err(error.clone())
                        } else {
                            Ok(())
                        }
                    };
                    let super_dispose = || {
                        events.borrow_mut().push("super".into());
                        if fail_super {
                            Err(error.clone())
                        } else {
                            Ok(())
                        }
                    };
                    let result = if venue == "kucoin" {
                        dispose_futures_private(&orders, truthy, clear, super_dispose)
                    } else {
                        dispose_spot_private(&orders, &balance, truthy, clear, super_dispose)
                    };
                    let mut expected_events = vec![];
                    let mut expected_orders = orders_kind;
                    let mut expected_balance = *balance_kind;
                    let mut rejected = false;
                    let mut index = 0;
                    for (role, kind) in [("orders", orders_kind), ("balance", *balance_kind)] {
                        if role == "balance" && venue == "kucoin" {
                            break;
                        }
                        if kind == "none" || kind == "zero" {
                            continue;
                        }
                        expected_events.push(format!("clear:{kind}"));
                        if index == fail_at {
                            rejected = true;
                            break;
                        }
                        if role == "orders" {
                            expected_orders = "none";
                        } else {
                            expected_balance = "none";
                        }
                        index += 1;
                    }
                    if !rejected {
                        expected_events.push("super".into());
                        rejected = fail_super;
                    }
                    assert_eq!(
                        *events.borrow(),
                        expected_events,
                        "KUCOIN-DISPOSE native call order {venue}/{orders_kind}/{balance_kind}/{fail_at}/{fail_super}"
                    );
                    assert_eq!(
                        tag(orders.borrow().as_ref()),
                        expected_orders,
                        "KUCOIN-DISPOSE native orders slot"
                    );
                    assert_eq!(
                        tag(balance.borrow().as_ref()),
                        expected_balance,
                        "KUCOIN-DISPOSE native balance slot"
                    );
                    assert_eq!(result.is_err(), rejected, "native disposal rejection");
                    if let Err(actual) = &result {
                        assert!(
                            Rc::ptr_eq(actual, &error),
                            "KUCOIN-DISPOSE native error pointer"
                        );
                        assert_eq!(actual, &error, "native complete disposal error");
                    }
                    cases.push(json!({"venue":venue,"orders":orders_kind,"balance":balance_kind,"failAt":fail_at,"failSuper":fail_super}));
                    native.push(json!({"events":*events.borrow(),"orders":tag(orders.borrow().as_ref()),"balance":tag(balance.borrow().as_ref()),
                        "error":result.err().map(|e|json!({"name":e.name,"message":e.message,"same":true}))}));
                }
            }
        }
    }
    assert_eq!(cases.len(), 76, "declared disposal profile floor");
    let timer = RefCell::new(Some(7));
    dispose_futures_private(
        &timer,
        |_| true,
        |_| {
            *timer.borrow_mut() = Some(9);
            Ok::<_, ()>(())
        },
        || Ok(()),
    )
    .unwrap();
    assert_eq!(
        *timer.borrow(),
        None,
        "KUCOIN-DISPOSE native successful clear erases replacement"
    );
    let orders = RefCell::new(Some(7));
    let balance = RefCell::new(Some(9));
    let events = RefCell::new(vec![]);
    dispose_spot_private(
        &orders,
        &balance,
        |_| true,
        |value| {
            events.borrow_mut().push(value);
            if value == 7 {
                *balance.borrow_mut() = Some(11);
            }
            Ok::<_, ()>(())
        },
        || Ok(()),
    )
    .unwrap();
    assert_eq!(
        *events.borrow(),
        vec![7, 11],
        "KUCOIN-DISPOSE native reentrant second-slot read"
    );
    for spot in [false, true] {
        let orders = RefCell::new(Some(7));
        let balance = RefCell::new(Some(9));
        let clear = |_| {
            *orders.borrow_mut() = Some(19);
            Err("replacement failure")
        };
        let super_dispose = || panic!("super after cancellation failure");
        let result = if spot {
            dispose_spot_private(&orders, &balance, |_| true, clear, super_dispose)
        } else {
            dispose_futures_private(&orders, |_| true, clear, super_dispose)
        };
        assert_eq!(
            result,
            Err("replacement failure"),
            "native replacement error"
        );
        assert_eq!(
            *orders.borrow(),
            Some(19),
            "native replacement retained on throw"
        );
        assert_eq!(
            *balance.borrow(),
            Some(9),
            "native later slot retained on throw"
        );
    }
    let timer = RefCell::new(Some(f64::NAN));
    let supers = Cell::new(0);
    dispose_futures_private(
        &timer,
        |v| *v != 0.0 && !v.is_nan(),
        |_| panic!("falsey NaN cancellation"),
        || {
            supers.set(supers.get() + 1);
            Ok::<_, ()>(())
        },
    )
    .unwrap();
    assert!(
        timer.borrow().unwrap().is_nan(),
        "native falsey NaN retained"
    );
    assert_eq!(supers.get(), 1, "native falsey NaN calls super once");
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-dispose.cjs"
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
        .write_all(&serde_json::to_vec(&cases).unwrap())
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
        76,
        "source disposal profile floor"
    );
    assert_eq!(
        source,
        json!(native),
        "live disposal after native slots/errors"
    );
}
