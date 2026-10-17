use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_bybit::private_ws_lifecycle::*;
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[derive(Default)]
struct State {
    disposed: Cell<bool>,
    enabled: Cell<bool>,
    ping_at: Cell<f64>,
    reads: Cell<usize>,
    clock_calls: Cell<usize>,
    setters: Cell<usize>,
    auth_calls: Cell<usize>,
    disregards: Cell<usize>,
    reconnects: Cell<usize>,
    snapshots: Cell<usize>,
    trace: RefCell<Vec<&'static str>>,
    sleeps: RefCell<Vec<u64>>,
    frames: RefCell<Vec<String>>,
    logs: RefCell<Vec<Rc<ExchangeError>>>,
}
impl State {
    fn disposed(&self) -> bool {
        self.reads.set(self.reads.get() + 1);
        self.trace.borrow_mut().push("disposed");
        self.disposed.get()
    }
    fn send(&self, frame: &str) {
        self.trace.borrow_mut().push("send");
        self.frames.borrow_mut().push(frame.into());
    }
}
fn errors() -> BTreeMap<&'static str, Rc<ExchangeError>> {
    [
        "start",
        "clock",
        "setter",
        "send",
        "sync",
        "sleep",
        "auth",
        "log",
        "disregard",
        "reconnect",
        "subscribe",
        "ping",
        "snapshot",
    ]
    .into_iter()
    .map(|phase| {
        let mut error = ExchangeError::new(format!("{phase} failed"));
        error.name = format!("{phase}Error");
        error.code = Some(safe_cex_exchange::js_runtime::value::from_json(
            &serde_json::json!(17),
        ));
        (phase, Rc::new(error))
    })
    .collect()
}
fn fail(
    phase: &str,
    current: &str,
    errors: &BTreeMap<&str, Rc<ExchangeError>>,
) -> Result<(), Rc<ExchangeError>> {
    if phase == current {
        Err(Rc::clone(&errors[current]))
    } else {
        Ok(())
    }
}
fn outcome(
    actual: &Result<(), Rc<ExchangeError>>,
    expected: &Value,
    errors: &BTreeMap<&str, Rc<ExchangeError>>,
) {
    assert_eq!(
        actual.is_ok(),
        expected["ok"] == true,
        "LIFECYCLE native rejection"
    );
    if let Err(error) = actual {
        assert_eq!(error.name, expected["error"], "LIFECYCLE native error name");
        assert_eq!(
            error.message, expected["message"],
            "LIFECYCLE native error message"
        );
        assert_eq!(
            observed_error_code(&error.code).as_ref(),
            expected.get("code"),
            "LIFECYCLE native error code"
        );
        assert!(
            Rc::ptr_eq(error, &errors[expected["originalPhase"].as_str().unwrap()]),
            "LIFECYCLE native original error identity"
        );
    }
}
fn count(actual: usize, expected: &Value, label: &str) {
    assert_eq!(
        actual,
        expected.as_u64().unwrap() as usize,
        "LIFECYCLE native {label}"
    );
}
fn number(actual: f64, expected: &Value, label: &str) {
    let bits = if actual.is_nan() {
        "nan".into()
    } else {
        format!("{:016x}", actual.to_bits())
    };
    assert_eq!(bits, expected.as_str().unwrap(), "LIFECYCLE native {label}");
}
fn trace(actual: &[&str], expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(
        actual.len(),
        expected.len(),
        "LIFECYCLE native callback count"
    );
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(
            *actual,
            expected.as_str().unwrap(),
            "LIFECYCLE native callback order"
        );
    }
}
fn frames(actual: &[String], expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(actual.len(), expected.len(), "LIFECYCLE native frame count");
    // Every decoded native field assertion precedes every raw wire comparison.
    for (actual, expected) in actual.iter().zip(expected) {
        let actual: Value = serde_json::from_str(actual).unwrap();
        let expected: Value = serde_json::from_str(expected.as_str().unwrap()).unwrap();
        assert_eq!(
            actual["op"].as_str(),
            expected["op"].as_str(),
            "LIFECYCLE native operation"
        );
        let field_count = if expected["op"] == "ping" { 1 } else { 2 };
        assert_eq!(
            actual.as_object().unwrap().len(),
            field_count,
            "LIFECYCLE native payload field count"
        );
        if expected["op"] == "subscribe" {
            let actual = actual["args"].as_array().unwrap();
            let expected = expected["args"].as_array().unwrap();
            assert_eq!(actual.len(), expected.len(), "LIFECYCLE native topic count");
            for (actual, expected) in actual.iter().zip(expected) {
                assert_eq!(
                    actual.as_str(),
                    expected.as_str(),
                    "LIFECYCLE native subscription topic"
                );
            }
        }
    }
}
async fn pending_once() {
    let mut pending = true;
    std::future::poll_fn(|cx| {
        if pending {
            pending = false;
            cx.waker().wake_by_ref();
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
        value => value.parse().unwrap(),
    }
}

#[test]
#[ignore = "requires installed live TypeScript"]
fn private_ws_lifecycle_native_source_parity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-ws-lifecycle-fixtures.cjs"
        ))
        .output()
        .expect("source oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixtures: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (group, expected) in [
        ("opens", 4),
        ("pings", 80),
        ("subscriptions", 4),
        ("reconnects", 52),
    ] {
        assert_eq!(
            fixtures[group].as_array().unwrap().len(),
            expected,
            "LIFECYCLE independent {group} floor"
        );
    }
    let mut wire_comparisons = Vec::new();
    for row in fixtures["opens"].as_array().unwrap() {
        let state = State::default();
        let started = Cell::new(0);
        let errors = errors();
        let actual = on_open(|| {
            started.set(started.get() + 1);
            state.trace.borrow_mut().push("start");
            if row["reject"] == true {
                Err(Rc::clone(&errors["start"]))
            } else {
                Ok(())
            }
        });
        count(started.get(), &row["started"], "open start count");
        outcome(&actual, &row["result"], &errors);
        if actual.is_ok() {
            assert_eq!(
                row["result"]["undefined"], true,
                "LIFECYCLE source open does not await pending promise"
            );
        }
        trace(&state.trace.borrow(), &row["trace"]);
    }
    for row in fixtures["pings"].as_array().unwrap() {
        let state = State::default();
        state.disposed.set(row["disposed"] == true);
        state.ping_at.set(3.);
        let errors = errors();
        let phase = row["phase"].as_str().unwrap();
        let actual = ping(
            || state.disposed(),
            || {
                state.clock_calls.set(state.clock_calls.get() + 1);
                state.trace.borrow_mut().push("clock");
                fail(phase, "clock", &errors)?;
                Ok(token(&row["token"]))
            },
            |value| {
                state.setters.set(state.setters.get() + 1);
                state.trace.borrow_mut().push("setter");
                fail(phase, "setter", &errors)?;
                state.ping_at.set(value);
                Ok(())
            },
            |frame| {
                state.send(frame);
                fail(phase, "send", &errors)
            },
        );
        outcome(&actual, &row["result"], &errors);
        number(state.ping_at.get(), &row["pingAt"], "ping timestamp");
        count(state.reads.get(), &row["reads"], "disposed read count");
        count(
            state.clock_calls.get(),
            &row["clockCalls"],
            "clock call count",
        );
        count(state.setters.get(), &row["setterCalls"], "setter count");
        trace(&state.trace.borrow(), &row["trace"]);
        frames(&state.frames.borrow(), &row["sends"]);
        wire_comparisons.push((state.frames.borrow().clone(), row["sends"].clone()));
    }
    for row in fixtures["subscriptions"].as_array().unwrap() {
        let state = State::default();
        let errors = errors();
        let send = |frame: &str| {
            state.send(frame);
            if row["reject"] == true {
                Err(Rc::clone(&errors["send"]))
            } else {
                Ok(())
            }
        };
        let actual = if row["family"] == "inverse" {
            inverse_subscribe(send)
        } else {
            spot_subscribe(send)
        };
        outcome(&actual, &row["result"], &errors);
        trace(&state.trace.borrow(), &row["trace"]);
        frames(&state.frames.borrow(), &row["sends"]);
        wire_comparisons.push((state.frames.borrow().clone(), row["sends"].clone()));
    }
    for row in fixtures["reconnects"].as_array().unwrap() {
        let input = &row["input"];
        let state = State::default();
        state.disposed.set(input["disposed"] == true);
        state.enabled.set(input["enabled"] == true);
        state.ping_at.set(7.);
        let errors = errors();
        let phase = input["phase"].as_str().unwrap_or("");
        let dispose_at = input["disposeAt"].as_str().unwrap_or("");
        let future = send_reconnect_sequence(
            || state.disposed(),
            || {
                state.trace.borrow_mut().push("sync");
                async {
                    pending_once().await;
                    if dispose_at == "sync" {
                        state.disposed.set(true);
                    }
                    fail(phase, "sync", &errors)
                }
            },
            |delay| {
                state.trace.borrow_mut().push("sleep");
                state.sleeps.borrow_mut().push(delay);
                async {
                    pending_once().await;
                    if dispose_at == "sleep" {
                        state.disposed.set(true);
                    }
                    fail(phase, "sleep", &errors)
                }
            },
            || {
                state.trace.borrow_mut().push("auth");
                state.auth_calls.set(state.auth_calls.get() + 1);
                async {
                    pending_once().await;
                    if dispose_at == "auth" {
                        state.disposed.set(true);
                    }
                    if input["changeFlag"] == true {
                        state.enabled.set(!state.enabled.get());
                    }
                    if input["auth"] == "throw" {
                        Err(Rc::clone(&errors["auth"]))
                    } else {
                        Ok(input["auth"] == "yes")
                    }
                }
            },
            |error| {
                state.trace.borrow_mut().push("log");
                assert!(
                    Rc::ptr_eq(&error, &errors["auth"]),
                    "LIFECYCLE native logged auth identity"
                );
                state.logs.borrow_mut().push(error);
                fail(phase, "log", &errors)
            },
            || {
                state.trace.borrow_mut().push("disregard");
                state.disregards.set(state.disregards.get() + 1);
                fail(phase, "disregard", &errors)
            },
            || {
                state.trace.borrow_mut().push("reconnect");
                state.reconnects.set(state.reconnects.get() + 1);
                fail(phase, "reconnect", &errors)
            },
            || {
                state.trace.borrow_mut().push("subscribe");
                fail(phase, "subscribe", &errors)?;
                let send = |frame: &str| {
                    state.send(frame);
                    Ok(())
                };
                if row["family"] == "inverse" {
                    inverse_subscribe(send)
                } else {
                    spot_subscribe(send)
                }
            },
            || {
                state.trace.borrow_mut().push("ping");
                fail(phase, "ping", &errors)?;
                ping(
                    || state.disposed(),
                    || Ok(33.),
                    |value| {
                        state.ping_at.set(value);
                        Ok(())
                    },
                    |frame| {
                        state.send(frame);
                        Ok(())
                    },
                )
            },
            || state.enabled.get(),
            || {
                state.trace.borrow_mut().push("snapshot");
                state.snapshots.set(state.snapshots.get() + 1);
                fail(phase, "snapshot", &errors)
            },
            input["syncTime"] == true,
        );
        let mut future = Box::pin(future);
        let mut cx = Context::from_waker(Waker::noop());
        let first = future.as_mut().poll(&mut cx);
        trace(&state.trace.borrow(), &row["prefix"]);
        let mut actual = match first {
            Poll::Ready(value) => Some(value),
            Poll::Pending => None,
        };
        for _ in 0..4 {
            if actual.is_some() {
                break;
            }
            if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
                actual = Some(value);
            }
        }
        let actual = actual.expect("LIFECYCLE native controlled future did not settle by poll 5");
        outcome(&actual, &row["result"], &errors);
        assert_eq!(
            state.disposed.get(),
            row["disposed"] == true,
            "LIFECYCLE native final disposal"
        );
        number(
            state.ping_at.get(),
            &row["pingAt"],
            "reconnect ping timestamp",
        );
        for (value, key) in [
            (state.reads.get(), "reads"),
            (state.auth_calls.get(), "authCalls"),
            (state.disregards.get(), "disregards"),
            (state.reconnects.get(), "reconnectStarts"),
            (state.snapshots.get(), "snapshotStarts"),
        ] {
            count(value, &row[key], key);
        }
        let expected = row["sleeps"].as_array().unwrap();
        assert_eq!(
            state.sleeps.borrow().len(),
            expected.len(),
            "LIFECYCLE native sleep count"
        );
        for (actual, expected) in state.sleeps.borrow().iter().zip(expected) {
            assert_eq!(
                *actual,
                expected.as_u64().unwrap(),
                "LIFECYCLE native sleep delay"
            );
        }
        let logs = state.logs.borrow();
        let expected = row["logs"].as_array().unwrap();
        assert_eq!(logs.len(), expected.len(), "LIFECYCLE native log count");
        for (actual, expected) in logs.iter().zip(expected) {
            assert_eq!(
                actual.name, expected["name"],
                "LIFECYCLE native logged name"
            );
            assert_eq!(
                actual.message, expected["message"],
                "LIFECYCLE native logged message"
            );
            assert_eq!(
                observed_error_code(&actual.code).as_ref(),
                expected.get("code"),
                "LIFECYCLE native logged code"
            );
            assert_eq!(
                Rc::ptr_eq(actual, &errors["auth"]),
                expected["sameAuth"] == true,
                "LIFECYCLE native logged identity"
            );
        }
        trace(&state.trace.borrow(), &row["trace"]);
        frames(&state.frames.borrow(), &row["sends"]);
        wire_comparisons.push((state.frames.borrow().clone(), row["sends"].clone()));
    }
    // Across the entire test, every native assertion runs before any wire assertion.
    for (actual, expected) in wire_comparisons {
        for (actual, expected) in actual.iter().zip(expected.as_array().unwrap()) {
            assert_eq!(actual, expected.as_str().unwrap(), "LIFECYCLE wire frame");
        }
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
