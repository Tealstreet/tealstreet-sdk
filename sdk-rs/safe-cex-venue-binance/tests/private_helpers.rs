use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_binance::private_helpers::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    rc::Rc,
};
type Failure = Rc<ExchangeError>;
fn oracle() -> Vec<Value> {
    let r = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-helpers.mjs"
        ))
        .output()
        .unwrap();
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let rows: Vec<Value> = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(rows.len(), 72, "fixed authored oracle floor 2*(7+13+16)");
    rows
}
fn encoded(value: Option<u64>) -> Value {
    value.map(|n| json!(n)).unwrap_or(json!("unset"))
}
fn error() -> Failure {
    let mut e = ExchangeError::new("original helper failure");
    e.name = "RangeError".into();
    e.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "fixture-code"
    )));
    Rc::new(e)
}
fn original(result: &Result<(), Failure>, failure: bool, e: &Failure) {
    match result {
        Ok(()) => assert!(!failure, "expected original failure"),
        Err(r) => {
            assert!(failure);
            assert!(Rc::ptr_eq(r, e), "native original error identity");
        }
    }
}
fn disposal(variant: &str) {
    let mut completed = 0;
    for case in oracle()
        .into_iter()
        .filter(|c| c["variant"] == variant && c["method"] == "dispose")
    {
        let name = case["name"].as_str().unwrap();
        let unset = name == "unset";
        let same = name == "same";
        let state = PrivateTimerState {
            retry_timer: Cell::new(if unset { None } else { Some(11) }),
            refresh_timer: Cell::new(if unset {
                None
            } else if same {
                Some(11)
            } else {
                Some(22)
            }),
        };
        let active = RefCell::new(if unset {
            BTreeSet::new()
        } else if same {
            BTreeSet::from([11])
        } else {
            BTreeSet::from([11, 22])
        });
        let clears = RefCell::new(vec![]);
        let disposed = Cell::new(false);
        let base_calls = Cell::new(0);
        let e = error();
        let clear = |handle| {
            clears.borrow_mut().push(handle);
            let ordinal = clears.borrow().len();
            if name == format!("clear{ordinal}-fail") {
                return Err(e.clone());
            }
            if let Some(id) = handle {
                active.borrow_mut().remove(&id);
            }
            if name == "late-refresh" && ordinal == 1 {
                state.refresh_timer.set(Some(33));
                active.borrow_mut().insert(33);
            }
            Ok(())
        };
        let base = || {
            base_calls.set(base_calls.get() + 1);
            if name == "base-fail" {
                return Err(e.clone());
            }
            disposed.set(true);
            Ok(())
        };
        let result = if variant == "binance" {
            dispose_binance_private(&state, clear, base)
        } else {
            dispose_binance_pm_private(&state, clear, base)
        };
        let expected_clears = match name {
            "unset" => vec![None, None],
            "same" => vec![Some(11), Some(11)],
            "clear1-fail" => vec![Some(11)],
            "late-refresh" => vec![Some(11), Some(33)],
            _ => vec![Some(11), Some(22)],
        };
        assert_eq!(
            *clears.borrow(),
            expected_clears,
            "PRIVATE {variant} dispose native cancellation IDs"
        );
        let expected_active: Vec<u64> = match name {
            "clear1-fail" => vec![11, 22],
            "clear2-fail" => vec![22],
            "late-refresh" => vec![22],
            _ => vec![],
        };
        assert_eq!(
            active.borrow().iter().copied().collect::<Vec<_>>(),
            expected_active,
            "native independent remaining timers"
        );
        assert_eq!(
            base_calls.get(),
            usize::from(!name.starts_with("clear")),
            "native base call count"
        );
        assert_eq!(
            disposed.get(),
            !name.ends_with("fail"),
            "native disposal flag"
        );
        assert_eq!(
            state.retry_timer.get(),
            if unset { None } else { Some(11) },
            "native retry field retained"
        );
        assert_eq!(
            state.refresh_timer.get(),
            if unset {
                None
            } else if same {
                Some(11)
            } else if name == "late-refresh" {
                Some(33)
            } else {
                Some(22)
            },
            "native live refresh field retained"
        );
        original(&result, name.ends_with("fail"), &e);
        // Every native assertion above; decoded source effects below.
        let source_trace = case["trace"].as_array().unwrap();
        let expected_base = usize::from(!name.starts_with("clear"));
        assert_eq!(source_trace.len(), expected_clears.len() + expected_base);
        for (i, id) in expected_clears.iter().enumerate() {
            assert_eq!(source_trace[i][0], "clear");
            assert_eq!(source_trace[i][1], encoded(*id));
        }
        if expected_base == 1 {
            assert_eq!(
                source_trace.last().unwrap().as_array().unwrap(),
                &vec![json!("base")]
            );
        }
        assert_eq!(
            case["cleared"].as_array().unwrap().len(),
            expected_clears.len()
        );
        for (source, id) in case["cleared"]
            .as_array()
            .unwrap()
            .iter()
            .zip(&expected_clears)
        {
            assert_eq!(*source, encoded(*id));
        }
        assert_eq!(case["remaining"], json!(expected_active));
        assert_eq!(case["disposed"], disposed.get());
        assert_eq!(case["retry"], encoded(state.retry_timer.get()));
        assert_eq!(case["refresh"], encoded(state.refresh_timer.get()));
        assert_eq!(
            case["failure"].as_str(),
            if name.ends_with("fail") {
                Some("original")
            } else {
                None
            }
        );
        completed += 1;
    }
    assert_eq!(completed, 7, "fixed disposal cases per family");
}
fn delay(name: &str) -> f64 {
    match name {
        "zero" => 0.0,
        "negative" => -1.0,
        "fraction" => 1.5,
        "nan" => f64::NAN,
        "infinity" => f64::INFINITY,
        "negative-infinity" => f64::NEG_INFINITY,
        "negative-zero" => -0.0,
        _ => 1800000.0,
    }
}
fn encoded_delay(value: f64) -> Value {
    if value.is_nan() {
        json!("nan")
    } else if value == f64::INFINITY {
        json!("infinity")
    } else if value == f64::NEG_INFINITY {
        json!("-infinity")
    } else if value == 0.0 && value.is_sign_negative() {
        json!("-zero")
    } else {
        json!(value)
    }
}
fn compare_source_delay(source: &Value, value: f64) {
    if source.is_number() {
        assert_eq!(source.as_f64().unwrap().to_bits(), value.to_bits());
    } else {
        assert_eq!(*source, encoded_delay(value));
    }
}
fn scheduling(variant: &str) {
    let mut completed = 0;
    for case in oracle()
        .into_iter()
        .filter(|c| c["variant"] == variant && c["method"] == "scheduleListenKeyRefresh")
    {
        let name = case["name"].as_str().unwrap();
        let unset = name == "unset-field";
        let state = PrivateTimerState {
            retry_timer: Cell::new(Some(11)),
            refresh_timer: Cell::new(if unset { None } else { Some(22) }),
        };
        let e = error();
        let clears = RefCell::new(vec![]);
        let active = RefCell::new(if unset {
            BTreeSet::new()
        } else {
            BTreeSet::from([22])
        });
        let operations = RefCell::new(vec![]);
        let seen_delay = Cell::new(None);
        let seen_timer = Cell::new(None);
        let stored = RefCell::new(None::<RefreshCallback>);
        let calls = Rc::new(RefCell::new(vec![]));
        let handler = Rc::new(Cell::new(1));
        let h = handler.clone();
        let c = calls.clone();
        let update: RefreshCallback = Rc::new(move || c.borrow_mut().push(h.get()));
        let clear = |id| {
            operations.borrow_mut().push("clear");
            clears.borrow_mut().push(id);
            if name == "clear-fail" {
                return Err(e.clone());
            }
            if let Some(id) = id {
                active.borrow_mut().remove(&id);
            }
            if name == "late-field-set-fail" {
                state.refresh_timer.set(Some(33));
            }
            Ok(())
        };
        let set = |d, callback| {
            operations.borrow_mut().push("set");
            seen_delay.set(Some(d));
            seen_timer.set(Some(state.refresh_timer.get()));
            assert!(
                calls.borrow().is_empty(),
                "native no eager update before timer creation"
            );
            if name == "set-fail" || name == "late-field-set-fail" {
                return Err(e.clone());
            }
            *stored.borrow_mut() = Some(callback);
            active.borrow_mut().insert(42);
            Ok(42)
        };
        let result = if variant == "binance" {
            schedule_binance_listen_key_refresh(&state, delay(name), clear, set, update.clone())
        } else {
            schedule_binance_pm_listen_key_refresh(&state, delay(name), clear, set, update.clone())
        };
        assert_eq!(
            *clears.borrow(),
            vec![if unset { None } else { Some(22) }],
            "native one refresh cancellation"
        );
        let failure = name.ends_with("fail");
        original(&result, failure, &e);
        assert!(
            calls.borrow().is_empty(),
            "native update deferred until timer fire"
        );
        assert_eq!(
            state.retry_timer.get(),
            Some(11),
            "native retry timer untouched"
        );
        assert_eq!(
            state.refresh_timer.get(),
            if name == "clear-fail" || name == "set-fail" {
                Some(22)
            } else if name == "late-field-set-fail" {
                Some(33)
            } else {
                Some(42)
            },
            "native new timer assigned only after success"
        );
        assert_eq!(
            active.borrow().iter().copied().collect::<Vec<_>>(),
            if name == "clear-fail" {
                vec![22]
            } else if failure {
                vec![]
            } else {
                vec![42]
            },
            "native independent active refresh timers"
        );
        assert_eq!(
            seen_delay.get().is_some(),
            name != "clear-fail",
            "native timer creation count"
        );
        if let Some(actual) = seen_delay.get() {
            let expected = delay(name);
            if expected.is_finite() && expected != 0.0 {
                assert_eq!(actual, expected, "PRIVATE {variant} schedule native delay");
            }
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "native raw f64 delay bits"
            );
            assert_eq!(
                seen_timer.get(),
                Some(if unset {
                    None
                } else if name == "late-field-set-fail" {
                    Some(33)
                } else {
                    Some(22)
                }),
                "native owner timer field during creation"
            );
        }
        if let Some(callback) = stored.borrow().as_ref() {
            assert!(!Rc::ptr_eq(callback, &update), "native fresh void callback");
            if name != "repeat-callback" {
                handler.set(2);
            }
            callback();
            if name == "repeat-callback" {
                callback();
            }
        }
        let expected_calls = if failure {
            vec![]
        } else if name == "repeat-callback" {
            vec![1, 1]
        } else {
            vec![2]
        };
        assert_eq!(
            *calls.borrow(),
            expected_calls,
            "native live update method and callback invocation count"
        );
        let expected_operations = if name == "clear-fail" {
            vec!["clear"]
        } else {
            vec!["clear", "set"]
        };
        assert_eq!(
            *operations.borrow(),
            expected_operations,
            "native cancellation before timer creation"
        );
        // Native state/arguments/errors were asserted before source effects.
        let source_trace = case["trace"].as_array().unwrap();
        assert_eq!(source_trace.len(), expected_operations.len());
        assert_eq!(source_trace[0][0], "clear");
        assert_eq!(
            source_trace[0][1],
            if unset { json!("unset") } else { json!(22) }
        );
        if name != "clear-fail" {
            assert_eq!(source_trace[1][0], "set");
            compare_source_delay(&source_trace[1][1], delay(name));
        }
        compare_source_delay(&case["delay"], delay(name));
        assert_eq!(case["refresh"], encoded(state.refresh_timer.get()));
        assert_eq!(case["retry"], 11);
        if let Some(actual) = seen_delay.get() {
            compare_source_delay(&case["seenDelay"], actual);
        } else {
            assert_eq!(case["seenDelay"], "unset");
        }
        assert_eq!(
            case["seenTimer"],
            seen_timer.get().map(encoded).unwrap_or(Value::Null)
        );
        assert_eq!(case["callbackIsUpdate"], false);
        assert_eq!(
            case["returnUndefined"],
            if failure { Value::Null } else { json!(true) }
        );
        assert_eq!(case["calls"], json!(expected_calls));
        assert_eq!(
            case["remaining"],
            json!(active.borrow().iter().copied().collect::<Vec<_>>())
        );
        assert_eq!(
            case["failure"].as_str(),
            if failure { Some("original") } else { None }
        );
        completed += 1;
    }
    assert_eq!(completed, 13, "fixed scheduling cases per family");
}
#[derive(Default)]
struct Socket {
    closed: Cell<bool>,
    ready: Cell<f64>,
}
fn detachment(variant: &str) {
    let mut completed = 0;
    for case in oracle()
        .into_iter()
        .filter(|c| c["variant"] == variant && c["method"] == "detachSocket")
    {
        let name = case["name"].as_str().unwrap();
        let initial = match name {
            "open" | "listeners-absent" | "swapped-owner" | "late-state-change" => 1.0,
            "closing" | "changed-ready" => 2.0,
            "closed" => 3.0,
            _ => 0.0,
        };
        let socket = Rc::new(Socket {
            ready: Cell::new(initial),
            ..Default::default()
        });
        let e = error();
        let owner = Cell::new(if name == "missing" { None } else { Some(100) });
        let gets = Cell::new(0);
        let removed = RefCell::new(vec![]);
        let remaining = RefCell::new(BTreeSet::from(["message", "close", "open"]));
        let reads = Cell::new(0);
        let closes = Cell::new(0);
        let close_handler = Cell::new(202);
        let get = || {
            gets.set(gets.get() + 1);
            Ok(if name == "missing" {
                None
            } else {
                Some(socket.clone())
            })
        };
        let remove = |arg: &Rc<Socket>, event| {
            assert!(
                Rc::ptr_eq(arg, &socket),
                "native captured socket for removal"
            );
            if name == "listeners-absent" {
                return Ok(());
            }
            let id = match event {
                "message" => 101,
                "close" => close_handler.get(),
                _ => 303,
            };
            removed.borrow_mut().push((event, id));
            if name == format!("remove{}-fail", removed.borrow().len()) {
                return Err(e.clone());
            }
            remaining.borrow_mut().remove(event);
            if removed.borrow().len() == 1 {
                if name == "swapped-owner" {
                    owner.set(Some(404));
                }
                if name == "handler-change" {
                    close_handler.set(999);
                }
                if name == "late-state-change" {
                    arg.ready.set(3.0);
                }
            }
            Ok(())
        };
        let ready = |arg: &Rc<Socket>| {
            assert!(
                Rc::ptr_eq(arg, &socket),
                "native captured socket for ready state"
            );
            reads.set(reads.get() + 1);
            if name == format!("read{}-fail", reads.get()) {
                return Err(e.clone());
            }
            Ok(if name == "changed-ready" {
                if reads.get() == 1 { 2.0 } else { 0.0 }
            } else {
                arg.ready.get()
            })
        };
        let close = |arg: &Rc<Socket>| {
            assert!(Rc::ptr_eq(arg, &socket), "native captured socket for close");
            closes.set(closes.get() + 1);
            if name == "close-fail" {
                return Err(e.clone());
            }
            arg.closed.set(true);
            Ok(())
        };
        let result = if variant == "binance" {
            detach_binance_private_socket(get, remove, ready, close)
        } else {
            detach_binance_pm_private_socket(get, remove, ready, close)
        };
        assert_eq!(gets.get(), 1, "native owner socket captured once");
        let removal_count = match name {
            "missing" | "listeners-absent" => 0,
            "remove1-fail" => 1,
            "remove2-fail" => 2,
            _ => 3,
        };
        let expected_removed: Vec<_> = [
            ("message", 101),
            ("close", if name == "handler-change" { 999 } else { 202 }),
            ("open", 303),
        ]
        .into_iter()
        .take(removal_count)
        .collect();
        assert_eq!(
            *removed.borrow(),
            expected_removed,
            "native ordered listener identities"
        );
        let mut expected_remaining = vec!["close", "message", "open"];
        let committed = if name == "missing" || name == "listeners-absent" {
            0
        } else if name.starts_with("remove") {
            removal_count - 1
        } else {
            3
        };
        for (event, _) in expected_removed.iter().take(committed) {
            expected_remaining.retain(|e| e != event);
        }
        assert_eq!(
            remaining.borrow().iter().copied().collect::<Vec<_>>(),
            expected_remaining,
            "native independent live listeners"
        );
        let expected_reads = if name == "missing" || name.starts_with("remove") {
            0
        } else if name == "read1-fail"
            || matches!(name, "open" | "listeners-absent" | "swapped-owner")
        {
            1
        } else {
            2
        };
        assert_eq!(
            reads.get(),
            expected_reads,
            "native short-circuit ready-state reads"
        );
        let should_close = !matches!(name, "missing" | "closing" | "closed" | "late-state-change")
            && !name.starts_with("remove")
            && !name.starts_with("read");
        assert_eq!(
            closes.get(),
            usize::from(should_close),
            "PRIVATE {variant} detach native close count"
        );
        assert_eq!(
            socket.closed.get(),
            should_close && name != "close-fail",
            "native captured socket closed flag"
        );
        assert_eq!(
            owner.get(),
            if name == "missing" {
                None
            } else if name == "swapped-owner" {
                Some(404)
            } else {
                Some(100)
            },
            "native current owner socket retained"
        );
        original(&result, name.ends_with("fail"), &e);
        // All decoded native values/counts above, source effects below.
        let source_trace = case["trace"].as_array().unwrap();
        assert_eq!(
            source_trace.len(),
            removal_count + expected_reads + usize::from(should_close)
        );
        for (i, (event, id)) in expected_removed.iter().enumerate() {
            assert_eq!(
                source_trace[i].as_array().unwrap(),
                &vec![json!("remove"), json!(100), json!(event), json!(id)]
            );
        }
        for i in 0..expected_reads {
            assert_eq!(
                source_trace[removal_count + i].as_array().unwrap(),
                &vec![json!("read"), json!(100)]
            );
        }
        if should_close {
            assert_eq!(
                source_trace.last().unwrap().as_array().unwrap(),
                &vec![json!("close"), json!(100)]
            );
        }
        assert_eq!(case["removed"].as_array().unwrap().len(), removal_count);
        for (source, (event, id)) in case["removed"]
            .as_array()
            .unwrap()
            .iter()
            .zip(&expected_removed)
        {
            assert_eq!(source[0], *event);
            assert_eq!(source[1], *id);
        }
        assert_eq!(case["remaining"], json!(expected_remaining));
        assert_eq!(case["reads"], expected_reads);
        assert_eq!(case["closeCalls"], usize::from(should_close));
        assert_eq!(case["closed"], socket.closed.get());
        assert_eq!(
            case["owner"],
            owner.get().map(|id| json!(id)).unwrap_or(Value::Null)
        );
        assert_eq!(
            case["failure"].as_str(),
            if name.ends_with("fail") {
                Some("original")
            } else {
                None
            }
        );
        completed += 1;
    }
    assert_eq!(completed, 16, "fixed detachment cases per family");
}
#[test]
#[ignore = "requires source oracle"]
fn futures_dispose() {
    disposal("binance");
}
#[test]
#[ignore = "requires source oracle"]
fn pm_dispose() {
    disposal("binancepm");
}
#[test]
#[ignore = "requires source oracle"]
fn futures_schedule() {
    scheduling("binance");
}
#[test]
#[ignore = "requires source oracle"]
fn pm_schedule() {
    scheduling("binancepm");
}
#[test]
#[ignore = "requires source oracle"]
fn futures_detach() {
    detachment("binance");
}
#[test]
#[ignore = "requires source oracle"]
fn pm_detach() {
    detachment("binancepm");
}
