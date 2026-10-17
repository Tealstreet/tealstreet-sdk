use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::Position;
use safe_cex_venue_binance::position_refresh::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Failure = Rc<ExchangeError>;
fn run<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    for _ in 0..3 {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
    panic!("fixture exceeded three polls");
}
fn error() -> Failure {
    let mut e = ExchangeError::new("position fixture");
    e.name = "RangeError".into();
    e.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "fixture-code"
    )));
    Rc::new(e)
}
fn original(result: &Result<(), Failure>, expected: bool, error: &Failure) {
    match result {
        Ok(()) => assert!(!expected, "native expected original failure"),
        Err(e) => {
            assert!(expected);
            assert!(Rc::ptr_eq(e, error), "native original failure identity");
            assert_eq!(e.name, "RangeError");
            assert_eq!(e.message, "position fixture");
            assert_eq!(observed_error_code(&e.code), Some(json!("fixture-code")));
        }
    }
}
fn number(actual: f64, expected: f64, label: &str) {
    if expected.is_nan() {
        assert!(actual.is_nan(), "{label} native NaN");
    } else {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "{label} native numeric bits ({actual:?} vs {expected:?})"
        );
    }
}
fn encode(n: f64) -> Value {
    if n.is_nan() {
        json!("nan")
    } else if n == 0.0 && n.is_sign_negative() {
        json!("-zero")
    } else {
        json!(n)
    }
}
fn state(throttle: f64) -> Rc<PositionFetchState> {
    Rc::new(PositionFetchState {
        is_disposed: Cell::new(false).into(),
        pending_events: Cell::new(true),
        scheduled: Cell::new(false),
        in_flight: Cell::new(false),
        last_fetch_time: Cell::new(0.0),
        throttle_ms: Cell::new(throttle),
    })
}
fn snapshot(s: &PositionFetchState) -> Value {
    json!({"disposed":s.is_disposed.get(),"pending":s.pending_events.get(),"scheduled":s.scheduled.get(),"inflight":s.in_flight.get(),"last":encode(s.last_fetch_time.get()),"throttle":encode(s.throttle_ms.get())})
}
fn source_value(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "{path} source decoded number ({:?} vs {:?})",
            a.as_f64(),
            b.as_f64()
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path} source array count");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                source_value(a, b, &format!("{path}/{i}"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "{path} source own keys"
            );
            for (key, value) in a {
                source_value(value, &b[key], &format!("{path}/{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path} source decoded value"),
    }
}

fn compare_source(variant: &str, method: &str, native: Vec<Value>) {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/position-refresh.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 94, "fixed source floor 2*(22+25)");
    let rows: Vec<_> = rows
        .into_iter()
        .filter(|r| r["variant"] == variant && r["method"] == method)
        .collect();
    assert_eq!(rows.len(), if method == "throttle" { 22 } else { 25 });
    for (index, (actual, expected)) in rows.into_iter().zip(native).enumerate() {
        source_value(
            &actual,
            &expected,
            &format!("POSITION {variant} {method} source case {index}"),
        );
    }
}
fn throttle(variant: &str) {
    let names = [
        "queued",
        "boundary",
        "overdue",
        "disposed",
        "paused",
        "scheduled",
        "no-pending",
        "negative-elapsed",
        "nan-now",
        "nan-throttle",
        "negative-zero-throttle",
        "clock-fail",
        "pause-fail",
        "set-fail",
        "immediate-fail",
        "callback-disposed",
        "callback-no-pending",
        "callback-fail",
        "late-method",
        "repeat-callback",
        "clock-mutates",
        "pause-mutates",
    ];
    let futures = variant == "binance";
    let standard = if futures { 10_000.0 } else { 500.0 };
    let mut native = Vec::new();
    for (index, name) in names.into_iter().enumerate() {
        let state = state(standard);
        state.is_disposed.set(name == "disposed");
        state.pending_events.set(name != "no-pending");
        state.scheduled.set(name == "scheduled");
        let now_value = match name {
            "boundary" | "immediate-fail" => standard,
            "overdue" => standard + 1.0,
            "nan-now" => f64::NAN,
            "negative-zero-throttle" => 0.0,
            _ => 100.0,
        };
        if name == "negative-elapsed" {
            state.last_fetch_time.set(200.0);
        }
        if name == "nan-throttle" {
            state.throttle_ms.set(f64::NAN);
        }
        if name == "negative-zero-throttle" {
            state.throttle_ms.set(-0.0);
        }
        let error = error();
        let trace = Rc::new(RefCell::new(Vec::<Value>::new()));
        let observed = Rc::new(RefCell::new(Vec::<(bool, bool, bool)>::new()));
        let calls = Rc::new(RefCell::new(Vec::<u64>::new()));
        let current = Rc::new(Cell::new(1));
        let delays = RefCell::new(Vec::<f64>::new());
        let registered = RefCell::new(None::<PositionTimerCallback<Failure>>);
        let starter: PositionFetchStarter<Failure> = {
            let state = state.clone();
            let trace = trace.clone();
            let observed = observed.clone();
            let calls = calls.clone();
            let current = current.clone();
            let error = error.clone();
            Rc::new(move || {
                let id = current.get();
                trace.borrow_mut().push(json!(["execute", id]));
                observed.borrow_mut().push((
                    state.scheduled.get(),
                    state.pending_events.get(),
                    state.is_disposed.get(),
                ));
                calls.borrow_mut().push(id);
                if name == "immediate-fail" || name == "callback-fail" {
                    Err(error.clone())
                } else {
                    Ok(())
                }
            })
        };
        let now = || {
            trace.borrow_mut().push(json!(["now"]));
            if name == "clock-fail" {
                return Err(error.clone());
            }
            if name == "clock-mutates" {
                state.is_disposed.set(true);
            }
            Ok(now_value)
        };
        let paused = || {
            trace.borrow_mut().push(json!(["paused"]));
            if name == "pause-fail" {
                return Err(error.clone());
            }
            if name == "pause-mutates" {
                state.is_disposed.set(true);
                state.last_fetch_time.set(50.0);
                state.throttle_ms.set(200.0);
            }
            Ok(name == "paused")
        };
        let set_timer = |callback, delay| {
            trace.borrow_mut().push(json!(["set", encode(delay)]));
            delays.borrow_mut().push(delay);
            *registered.borrow_mut() = Some(callback);
            if name == "set-fail" {
                Err(error.clone())
            } else {
                Ok(42_u64)
            }
        };
        let result = if futures {
            throttle_binance_positions(&state, now, paused, set_timer, starter.clone())
        } else {
            throttle_binance_pm_positions(&state, now, set_timer, starter.clone())
        };
        let early = ["disposed", "scheduled", "clock-fail", "clock-mutates"].contains(&name)
            || (futures && ["paused", "pause-fail"].contains(&name));
        let immediate = [
            "boundary",
            "overdue",
            "negative-zero-throttle",
            "immediate-fail",
        ]
        .contains(&name);
        let expected_delay = if early || immediate {
            None
        } else {
            Some(match name {
                "nan-now" | "nan-throttle" => f64::NAN,
                "negative-elapsed" => standard + 100.0,
                "pause-mutates" if futures => 150.0,
                _ => standard - 100.0,
            })
        };
        assert_eq!(
            delays.borrow().len(),
            usize::from(expected_delay.is_some()),
            "native independent timer count"
        );
        if let Some(expected) = expected_delay {
            number(
                delays.borrow()[0],
                expected,
                &format!("POSITION {variant} throttle native delay case {index}"),
            );
        }
        let failed = ["clock-fail", "set-fail", "immediate-fail"].contains(&name)
            || (futures && name == "pause-fail");
        original(&result, failed, &error);
        let mut callback_is_execute = Value::Null;
        let mut callback_void = Value::Null;
        let mut callback_failed = false;
        if let Some(callback) = registered.borrow().as_ref() {
            assert!(
                !Rc::ptr_eq(callback, &starter),
                "native fresh timer wrapper"
            );
            callback_is_execute = json!(false);
            if !failed {
                if name == "late-method" {
                    current.set(2);
                }
                if name == "callback-disposed" {
                    state.is_disposed.set(true);
                }
                if name == "callback-no-pending" {
                    state.pending_events.set(false);
                }
                let first = callback();
                callback_failed = name == "callback-fail";
                original(&first, callback_failed, &error);
                if first.is_ok() {
                    callback_void = json!(true);
                    if name == "repeat-callback" {
                        original(&callback(), false, &error);
                    }
                }
            }
        }
        let no_execute = early
            || [
                "no-pending",
                "set-fail",
                "callback-disposed",
                "callback-no-pending",
            ]
            .contains(&name)
            || (futures && name == "pause-mutates");
        let expected_calls = if no_execute {
            vec![]
        } else if name == "repeat-callback" {
            vec![1, 1]
        } else {
            vec![if name == "late-method" { 2 } else { 1 }]
        };
        assert_eq!(
            *calls.borrow(),
            expected_calls,
            "POSITION {variant} throttle native execute calls case {index}"
        );
        assert_eq!(
            *observed.borrow(),
            vec![(false, true, false); expected_calls.len()],
            "native execute sees cleared scheduled flag and live pending/disposal"
        );
        let disposed = ["disposed", "clock-mutates", "callback-disposed"].contains(&name)
            || (futures && name == "pause-mutates");
        let pending = name != "no-pending"
            && name != "callback-no-pending"
            && !(futures && ["disposed", "clock-mutates", "paused"].contains(&name));
        assert_eq!(
            state.is_disposed.get(),
            disposed,
            "native throttle disposal"
        );
        assert_eq!(
            state.pending_events.get(),
            pending,
            "native throttle pending"
        );
        assert_eq!(
            state.scheduled.get(),
            name == "scheduled" || name == "set-fail",
            "native throttle scheduling flag"
        );
        assert!(
            !state.in_flight.get(),
            "native throttle leaves in-flight untouched"
        );
        number(
            state.last_fetch_time.get(),
            if futures && name == "pause-mutates" {
                50.0
            } else if name == "negative-elapsed" {
                200.0
            } else {
                0.0
            },
            "native throttle last time",
        );
        number(
            state.throttle_ms.get(),
            match name {
                "nan-throttle" => f64::NAN,
                "negative-zero-throttle" => -0.0,
                "pause-mutates" if futures => 200.0,
                _ => standard,
            },
            "native throttle current interval",
        );
        let paused_count =
            usize::from(futures && !(["disposed", "clock-fail", "clock-mutates"].contains(&name)));
        assert_eq!(
            trace.borrow().len(),
            1 + paused_count + usize::from(expected_delay.is_some()) + expected_calls.len(),
            "native independent throttle effect count"
        );
        let mut row = snapshot(&state);
        let object = row.as_object_mut().unwrap();
        for (key, value) in [
            ("variant", json!(variant)),
            ("method", json!("throttle")),
            ("name", json!(name)),
            ("trace", json!(*trace.borrow())),
            ("observed", json!(*observed.borrow())),
            ("calls", json!(*calls.borrow())),
            (
                "delay",
                expected_delay
                    .map(|_| encode(delays.borrow()[0]))
                    .unwrap_or(Value::Null),
            ),
            ("callbackIsExecute", callback_is_execute),
            ("callbackVoid", callback_void),
            (
                "failure",
                if failed {
                    json!("original")
                } else {
                    Value::Null
                },
            ),
            (
                "callbackFailure",
                if callback_failed {
                    json!("original")
                } else {
                    Value::Null
                },
            ),
        ] {
            object.insert(key.into(), value);
        }
        native.push(row);
    }
    assert_eq!(
        native.len(),
        22,
        "authored throttle floor stopped pause-mutates"
    );
    compare_source(variant, "throttle", native);
}
#[derive(Debug, PartialEq)]
struct Observation {
    label: &'static str,
    inflight: bool,
    pending: bool,
    disposed: bool,
    last: f64,
}
fn observe(label: &'static str, state: &PositionFetchState) -> Observation {
    Observation {
        label,
        inflight: state.in_flight.get(),
        pending: state.pending_events.get(),
        disposed: state.is_disposed.get(),
        last: state.last_fetch_time.get(),
    }
}
fn observation_wire(values: &[Observation]) -> Value {
    json!(
        values
            .iter()
            .map(|o| json!([o.label, o.inflight, o.pending, o.disposed, encode(o.last)]))
            .collect::<Vec<_>>()
    )
}
fn positions() -> Rc<Vec<Position>> {
    Rc::new([("first",3.6,12.3),("second",2.1,14.2)].into_iter().map(|(id,contracts,entry)|serde_json::from_value(json!({"id":id,"symbol":"BTCUSDT","side":"long","entryPrice":entry,"notional":44.28,"leverage":2,"unrealizedPnl":1.9,"contracts":contracts,"liquidationPrice":7,"lastUpdate":100})).unwrap()).collect())
}
fn execute(variant: &str) {
    let names = [
        "normal",
        "inflight",
        "disposed",
        "paused",
        "pause-fail",
        "version-start-fail",
        "clock-fail",
        "fetch-sync-fail",
        "fetch-reject",
        "disposed-during-fetch",
        "version-changed",
        "pending-during-fetch",
        "version-nan",
        "version-zero-sign",
        "version-end-fail",
        "apply-fail",
        "apply-sets-pending",
        "apply-disposes",
        "retry-fail",
        "retry-sets-inflight",
        "paused-mutates-disposal",
        "clock-disposes",
        "clock-mutates-pending",
        "fetch-marks-version-and-pending",
        "pending-after-reject",
    ];
    let futures = variant == "binance";
    let standard = if futures { 10_000.0 } else { 500.0 };
    let mut native = Vec::new();
    for (index, name) in names.into_iter().enumerate() {
        let state = state(standard);
        state.is_disposed.set(name == "disposed");
        state.in_flight.set(name == "inflight");
        let error = error();
        let trace = RefCell::new(Vec::<Value>::new());
        let observed = RefCell::new(Vec::<Observation>::new());
        let version = Cell::new(if name == "version-nan" {
            f64::NAN
        } else if name == "version-zero-sign" {
            -0.0
        } else {
            1.0
        });
        let versions = RefCell::new(Vec::<f64>::new());
        let polls = Cell::new(0);
        let fetch_count = Cell::new(0);
        let apply_count = Cell::new(0);
        let retries = Cell::new(0);
        let positions = positions();
        let captured = RefCell::new(None::<Rc<Vec<Position>>>);
        let paused = || {
            trace.borrow_mut().push(json!(["paused"]));
            if name == "pause-fail" {
                return Err(error.clone());
            }
            if name == "paused-mutates-disposal" {
                state.is_disposed.set(true);
            }
            Ok(name == "paused")
        };
        let get_version = || {
            let v = version.get();
            versions.borrow_mut().push(v);
            trace.borrow_mut().push(json!(["version", encode(v)]));
            if (name == "version-start-fail" && versions.borrow().len() == 1)
                || (name == "version-end-fail" && versions.borrow().len() == 2)
            {
                Err(error.clone())
            } else {
                Ok(v)
            }
        };
        let now = || {
            trace.borrow_mut().push(json!(["now"]));
            observed.borrow_mut().push(observe("now", &state));
            if name == "clock-fail" {
                return Err(error.clone());
            }
            if name == "clock-disposes" {
                state.is_disposed.set(true);
            }
            if name == "clock-mutates-pending" {
                state.pending_events.set(true);
            }
            Ok(1000.0)
        };
        let fetch_positions = || {
            fetch_count.set(fetch_count.get() + 1);
            trace.borrow_mut().push(json!(["fetch"]));
            observed.borrow_mut().push(observe("fetch", &state));
            if name == "fetch-sync-fail" {
                return Err(error.clone());
            }
            Ok(std::future::poll_fn(|_| {
                polls.set(polls.get() + 1);
                if polls.get() == 1 {
                    return Poll::Pending;
                }
                if name == "disposed-during-fetch" {
                    state.is_disposed.set(true);
                }
                if name == "version-changed" || name == "fetch-marks-version-and-pending" {
                    version.set(2.0);
                }
                if name == "version-zero-sign" {
                    version.set(0.0);
                }
                if [
                    "pending-during-fetch",
                    "retry-fail",
                    "retry-sets-inflight",
                    "fetch-marks-version-and-pending",
                    "pending-after-reject",
                ]
                .contains(&name)
                {
                    state.pending_events.set(true);
                }
                Poll::Ready(
                    if name == "fetch-reject" || name == "pending-after-reject" {
                        Err(error.clone())
                    } else {
                        Ok(positions.clone())
                    },
                )
            }))
        };
        let apply = |p| {
            trace.borrow_mut().push(json!(["apply"]));
            observed.borrow_mut().push(observe("apply", &state));
            apply_count.set(apply_count.get() + 1);
            *captured.borrow_mut() = Some(p);
            if name == "apply-sets-pending" {
                state.pending_events.set(true);
            }
            if name == "apply-disposes" {
                state.is_disposed.set(true);
            }
            if name == "apply-fail" {
                Err(error.clone())
            } else {
                Ok(())
            }
        };
        let retry = || {
            trace.borrow_mut().push(json!(["retry"]));
            observed.borrow_mut().push(observe("retry", &state));
            retries.set(retries.get() + 1);
            if name == "retry-sets-inflight" {
                state.in_flight.set(true);
            }
            if name == "retry-fail" {
                Err(error.clone())
            } else {
                Ok(())
            }
        };
        let result = if futures {
            run(execute_binance_position_fetch(
                &state,
                paused,
                get_version,
                now,
                fetch_positions,
                apply,
                retry,
            ))
        } else {
            run(execute_binance_pm_position_fetch(
                &state,
                now,
                fetch_positions,
                apply,
            ));
            Ok(())
        };
        let skipped = futures
            && [
                "inflight",
                "disposed",
                "paused",
                "pause-fail",
                "version-start-fail",
            ]
            .contains(&name);
        let clock_reached = !skipped;
        let fetch_reached = clock_reached && name != "clock-fail";
        let fulfilled = fetch_reached
            && !["fetch-sync-fail", "fetch-reject", "pending-after-reject"].contains(&name);
        let disposed_after_fetch = name == "disposed"
            || name == "disposed-during-fetch"
            || name == "clock-disposes"
            || (futures && name == "paused-mutates-disposal");
        let should_apply = fulfilled
            && !disposed_after_fetch
            && !(futures
                && [
                    "version-changed",
                    "version-nan",
                    "version-end-fail",
                    "fetch-marks-version-and-pending",
                ]
                .contains(&name));
        let pending_during = [
            "pending-during-fetch",
            "retry-fail",
            "retry-sets-inflight",
            "fetch-marks-version-and-pending",
            "pending-after-reject",
        ]
        .contains(&name);
        let should_retry =
            futures && fetch_reached && (pending_during || name == "apply-sets-pending");
        assert_eq!(
            fetch_count.get(),
            u64::from(fetch_reached),
            "POSITION {variant} execute native fetch count case {index}"
        );
        assert_eq!(
            apply_count.get(),
            u64::from(should_apply),
            "POSITION {variant} execute native apply count case {index}"
        );
        assert_eq!(
            retries.get(),
            u64::from(should_retry),
            "native independent retry count"
        );
        number(
            state.last_fetch_time.get(),
            if fetch_reached { 1000.0 } else { 0.0 },
            &format!("POSITION {variant} execute native last time case {index}"),
        );
        number(
            state.throttle_ms.get(),
            standard,
            "native untouched execute throttle interval",
        );
        assert!(
            !state.scheduled.get(),
            "native execute does not change scheduling flag"
        );
        assert_eq!(
            polls.get(),
            if fetch_reached && name != "fetch-sync-fail" {
                2
            } else {
                0
            },
            "native actual await suspension count"
        );
        let pending = if fetch_reached {
            pending_during || (should_apply && name == "apply-sets-pending")
        } else {
            true
        };
        assert_eq!(
            state.pending_events.get(),
            pending,
            "native final pending flag"
        );
        let disposed = name == "disposed"
            || name == "clock-disposes"
            || (futures && name == "paused-mutates-disposal")
            || (fetch_reached && name == "disposed-during-fetch")
            || (should_apply && name == "apply-disposes");
        assert_eq!(state.is_disposed.get(), disposed, "native final disposal");
        let in_flight = if futures {
            name == "inflight"
                || name == "clock-fail"
                || (should_retry && name == "retry-sets-inflight")
        } else {
            name == "inflight"
        };
        assert_eq!(
            state.in_flight.get(),
            in_flight,
            "native final in-flight guard"
        );
        let failed = futures
            && [
                "pause-fail",
                "version-start-fail",
                "clock-fail",
                "retry-fail",
            ]
            .contains(&name);
        original(&result, failed, &error);
        let end_read = futures && fulfilled && !disposed_after_fetch;
        let start_read =
            futures && !["inflight", "disposed", "paused", "pause-fail"].contains(&name);
        assert_eq!(
            versions.borrow().len(),
            usize::from(start_read) + usize::from(end_read),
            "native independent version read count"
        );
        if start_read {
            number(
                versions.borrow()[0],
                match name {
                    "version-nan" => f64::NAN,
                    "version-zero-sign" => -0.0,
                    _ => 1.0,
                },
                "native start version",
            );
        }
        if end_read {
            number(
                versions.borrow()[1],
                match name {
                    "version-nan" => f64::NAN,
                    "version-zero-sign" => 0.0,
                    "version-changed" | "fetch-marks-version-and-pending" => 2.0,
                    _ => 1.0,
                },
                "native resume version",
            );
        }
        let mut expected_observed = Vec::new();
        let observed_flight = if futures { true } else { name == "inflight" };
        if clock_reached {
            expected_observed.push(Observation {
                label: "now",
                inflight: observed_flight,
                pending: true,
                disposed: name == "disposed" || (futures && name == "paused-mutates-disposal"),
                last: 0.0,
            });
        }
        if fetch_reached {
            expected_observed.push(Observation {
                label: "fetch",
                inflight: observed_flight,
                pending: false,
                disposed: name == "disposed"
                    || name == "clock-disposes"
                    || (futures && name == "paused-mutates-disposal"),
                last: 1000.0,
            });
        }
        if should_apply {
            expected_observed.push(Observation {
                label: "apply",
                inflight: observed_flight,
                pending: pending_during,
                disposed: false,
                last: 1000.0,
            });
        }
        if should_retry {
            expected_observed.push(Observation {
                label: "retry",
                inflight: false,
                pending: true,
                disposed: false,
                last: 1000.0,
            });
        }
        assert_eq!(
            *observed.borrow(),
            expected_observed,
            "native state before callbacks/finally retry"
        );
        let mut wire_positions = Value::Null;
        if let Some(p) = captured.borrow().as_ref() {
            assert!(
                Rc::ptr_eq(p, &positions),
                "native original position array identity"
            );
            assert_eq!(p.len(), 2, "native independent decoded position count");
            for (row, (id, contracts, entry)) in
                [("first", 3.6_f64, 12.3_f64), ("second", 2.1_f64, 14.2_f64)]
                    .into_iter()
                    .enumerate()
            {
                assert_eq!(p[row].id, id, "native position id");
                number(p[row].contracts, contracts, "native decoded contracts");
                number(p[row].entry_price, entry, "native decoded entry price");
            }
            wire_positions = json!(
                p.iter()
                    .map(|p| json!({"id":p.id,"contracts":p.contracts,"entryPrice":p.entry_price}))
                    .collect::<Vec<_>>()
            );
        }
        let paused_count = usize::from(futures && !(["inflight", "disposed"].contains(&name)));
        assert_eq!(
            trace.borrow().len(),
            paused_count
                + versions.borrow().len()
                + usize::from(clock_reached)
                + usize::from(fetch_reached)
                + usize::from(should_apply)
                + usize::from(should_retry),
            "native independent execute effect count"
        );
        let mut row = snapshot(&state);
        let object = row.as_object_mut().unwrap();
        for (key, value) in [
            ("variant", json!(variant)),
            ("method", json!("execute")),
            ("name", json!(name)),
            ("trace", json!(*trace.borrow())),
            ("observed", observation_wire(&observed.borrow())),
            ("applyCount", json!(apply_count.get())),
            ("retryCount", json!(retries.get())),
            (
                "failure",
                if failed {
                    json!("original")
                } else {
                    Value::Null
                },
            ),
            ("versionReads", json!(versions.borrow().len())),
            ("positions", wire_positions),
            (
                "identity",
                if should_apply {
                    json!(true)
                } else {
                    Value::Null
                },
            ),
        ] {
            object.insert(key.into(), value);
        }
        native.push(row);
    }
    assert_eq!(
        native.len(),
        25,
        "authored execute floor stopped pending-after-reject"
    );
    compare_source(variant, "execute", native);
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn futures_throttle() {
    throttle("binance");
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn pm_throttle() {
    throttle("binancepm");
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn futures_execute() {
    execute("binance");
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn pm_execute() {
    execute("binancepm");
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
