#[allow(dead_code)]
#[path = "support/spot_account_host.rs"]
mod account;
#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[allow(dead_code)]
#[path = "support/rest_host.rs"]
mod native;
#[allow(dead_code)]
#[path = "support/order_retry_host.rs"]
mod support;
use account::get;
use cache::{N, arr, eq, object, s};
use safe_cex_venue_hyperliquid::{
    order_retry::*, spot_account_reads::SpotAccountHost, spot_json::JsString,
    ws_ping_lifecycle::WsPingHost,
};
use std::{
    cell::RefCell,
    collections::HashMap,
    future::Future,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
use support::{Host, State};
const REFRESH: [&str; 10] = [
    "inactive",
    "active",
    "empty",
    "disposed",
    "duplicates",
    "falsy",
    "add-error",
    "iterator-error",
    "start-error",
    "full-chain",
];
const REGISTER: [&str; 11] = [
    "one",
    "two",
    "three",
    "four",
    "five",
    "zero",
    "fraction",
    "nan",
    "string",
    "stateful",
    "set-error",
];
const TIMER: [&str; 25] = [
    "missing",
    "found-all",
    "partial",
    "duplicate-found",
    "missing-id",
    "disposed",
    "last-missing",
    "last-found",
    "reject",
    "last-reject",
    "sync-error",
    "fetch-null",
    "delete-error",
    "next-error",
    "step-once-error",
    "step-error",
    "end-error",
    "disposed-end-error",
    "log-error",
    "string",
    "stateful",
    "stateful-reject",
    "drop-observer",
    "reset-error",
    "disposed-get-error",
];
fn set(values: Vec<N>) -> N {
    N::Set(Rc::new(RefCell::new(values)))
}
fn members(v: &N) -> Vec<N> {
    let N::Set(v) = v else {
        panic!("native Set discriminant")
    };
    v.borrow().clone()
}
fn values(v: &N) -> Vec<N> {
    let N::A(v) = v else {
        panic!("native array discriminant")
    };
    v.borrow().clone()
}
fn same_values(actual: &[N], expected: &[N], label: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{label}: independent member floor"
    );
    for (a, b) in actual.iter().zip(expected) {
        assert!(eq(a, b), "{label}: exact ordered member {a:?} vs {b:?}")
    }
}
fn rows(ids: Vec<N>) -> N {
    arr(ids
        .into_iter()
        .map(|id| {
            if matches!(id, N::U) {
                object(vec![])
            } else {
                object(vec![("id", id)])
            }
        })
        .collect())
}
fn attempt(scene: &str) -> N {
    match scene {
        "two" => N::Num(2.),
        "three" => N::Num(3.),
        "four" => N::Num(4.),
        "five" | "last-missing" | "last-found" | "last-reject" | "end-error" | "log-error" => {
            N::Num(5.)
        }
        "zero" => N::Num(0.),
        "fraction" => N::Num(2.5),
        "nan" => N::Num(f64::NAN),
        "string" => s("2"),
        "stateful" | "stateful-reject" => object(vec![]),
        _ => N::Num(1.),
    }
}
fn delay(scene: &str) -> f64 {
    match scene {
        "two" | "string" | "stateful" | "stateful-reject" => 700.,
        "three" => 1500.,
        "four" => 3000.,
        "five" | "zero" | "fraction" | "nan" | "last-missing" | "last-found" | "last-reject"
        | "end-error" | "log-error" => 5000.,
        _ => 300.,
    }
}
fn marker(name: &str, code: f64, last: u16) -> N {
    let value = object(vec![
        ("name", s(name)),
        (
            "message",
            N::S(JsString("oops".encode_utf16().chain([last]).collect())),
        ),
        ("response", object(vec![("code", N::Num(code))])),
    ]);
    let N::O(o) = &value else { unreachable!() };
    for (k, _, e) in &mut o.borrow_mut().fields {
        if k == "name" || k == "message" {
            *e = false
        }
    }
    value
}
fn fixture(family: &str, scene: &str) -> Host {
    let base = account::Host::new(
        object(vec![]),
        object(vec![(
            "LogSeverity",
            object(vec![("Warning", s("Warning"))]),
        )]),
        object(vec![]),
    );
    let rows = match scene {
        "found-all" | "last-found" | "delete-error" | "next-error" => rows(vec![s("a"), s("b")]),
        "partial" => rows(vec![s("a")]),
        "duplicate-found" => rows(vec![s("a"), s("a"), s("z")]),
        "missing-id" => rows(vec![N::U, s("a")]),
        "fetch-null" => N::Null,
        _ => arr(vec![]),
    };
    let st = Rc::new(RefCell::new(State {
        scene: scene.into(),
        effects: vec![],
        callbacks: HashMap::new(),
        promises: vec![],
        marker: marker("Error", 55., 0xd800),
        secondary: marker("LaterError", 77., 0xdfff),
        timers: vec![],
        rows,
        attempt: attempt(scene),
        coercions: 0,
        found: N::U,
        found_iterator: N::U,
        fetch_calls: 0,
        step_calls: 0,
        end_calls: 0,
        log_calls: 0,
        settlements: vec![],
    }));
    let mut h = Host { base, st };
    let pending = if family == "refresh" {
        if scene == "active" {
            vec![s("z")]
        } else {
            vec![]
        }
    } else {
        vec![s("a"), s("b")]
    };
    for (k, v) in [
        ("_pendingRetryOrderIds", set(pending)),
        (
            "_retryChainActive",
            N::B(family != "refresh" || scene == "active"),
        ),
        ("_retryChainTimer", N::Num(7.)),
        (
            "isDisposed",
            N::B(matches!(scene, "disposed" | "disposed-end-error")),
        ),
        ("runRetryChainStep", N::Builtin("runRetryChainStep".into())),
        ("endRetryChain", N::Builtin("endRetryChain".into())),
        ("fetchOrders", N::Builtin("fetchOrders".into())),
    ] {
        h.put(k, &v)
    }
    h
}
fn drive<F: Future<Output = Result<N, N>>>(h: &Host, f: F) -> Result<N, N> {
    let mut f = Box::pin(f);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..100 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        h.base.drain()
    }
    panic!("native finite retry driver")
}
fn error_text(prefix: &str, later: bool) -> N {
    let mut text = prefix.encode_utf16().collect::<Vec<_>>();
    text.extend(
        if later {
            "LaterError: oops"
        } else {
            "Error: oops"
        }
        .encode_utf16(),
    );
    text.push(if later { 0xdfff } else { 0xd800 });
    N::S(JsString(text))
}
fn expected_logs(scene: &str) -> Vec<N> {
    let exhausted = s("HIP-3 order refresh: 2 expected orders not found after 5 attempts");
    let prefix = if scene == "stateful-reject" {
        "HIP-3 order refresh failed (attempt label): "
    } else if matches!(scene, "last-reject" | "end-error" | "log-error") {
        "HIP-3 order refresh failed (attempt 5): "
    } else {
        "HIP-3 order refresh failed (attempt 1): "
    };
    match scene {
        "last-missing" | "full-chain" => vec![exhausted],
        "end-error" | "log-error" => vec![exhausted, error_text(prefix, false)],
        "reject" | "last-reject" | "sync-error" | "delete-error" | "next-error"
        | "step-once-error" | "step-error" | "stateful-reject" => vec![error_text(prefix, false)],
        "fetch-null" => vec![s(
            "HIP-3 order refresh failed (attempt 1): TypeError: Cannot read properties of null (reading 'map')",
        )],
        _ => vec![],
    }
}
fn assert_error(h: &Host, result: &Result<N, N>, later: bool, label: &str) {
    let expected = if later {
        h.st.borrow().secondary.clone()
    } else {
        h.st.borrow().marker.clone()
    };
    assert!(result.is_err(), "native original rejection branch {label}");
    let actual = result.as_ref().unwrap_err();
    assert!(
        eq(actual, &expected),
        "native original rejection handle {label}"
    );
    assert!(
        eq(
            &get(&get(actual, "response"), "code"),
            &N::Num(if later { 77. } else { 55. })
        ),
        "native complete error response {label}"
    );
    let N::S(message) = get(actual, "message") else {
        panic!("native error UTF16")
    };
    assert_eq!(
        message.0,
        "oops"
            .encode_utf16()
            .chain([if later { 0xdfff } else { 0xd800 }])
            .collect::<Vec<_>>(),
        "native original error UTF16 {label}"
    )
}
fn snapshot(h: &Host, result: &Result<N, N>) -> N {
    let effects = h.st.borrow().effects.clone();
    object(vec![
        ("result", result.as_ref().cloned().unwrap_or(N::U)),
        ("rejected", N::B(result.is_err())),
        ("effects", arr(effects)),
        (
            "pending",
            arr(members(&get(&h.e(), "_pendingRetryOrderIds"))),
        ),
        ("active", get(&h.e(), "_retryChainActive")),
        ("timer", get(&h.e(), "_retryChainTimer")),
        ("fetchCalls", N::Num(h.st.borrow().fetch_calls as f64)),
        ("stepCalls", N::Num(h.st.borrow().step_calls as f64)),
        ("endCalls", N::Num(h.st.borrow().end_calls as f64)),
        ("coercions", N::Num(h.st.borrow().coercions as f64)),
    ])
}
#[test]
fn native_order_retry_before_source() {
    let mut native_rows = vec![];
    let mut failures = vec![];
    for (family, scenes) in [
        ("refresh", REFRESH.as_slice()),
        ("register", REGISTER.as_slice()),
        ("timer", TIMER.as_slice()),
    ] {
        for scene in scenes {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut h = fixture(family, scene);
                let e = h.e();
                let attempt = h.st.borrow().attempt.clone();
                let result = if family == "refresh" {
                    let ids = arr(match *scene {
                        "empty" => vec![],
                        "duplicates" => vec![s("a"), s("a"), s("b")],
                        "falsy" => vec![s(""), N::Num(0.), N::Null, s("a")],
                        _ => vec![s("a"), s("b")],
                    });
                    let result = refresh_hip3_orders_with_retry(&mut h, &e, &ids);
                    if *scene == "full-chain" {
                        for index in 0..5 {
                            let callback =
                                h.st.borrow()
                                    .timers
                                    .get(index)
                                    .expect("native independent full-chain timer floor")
                                    .0
                                    .clone();
                            let promise = h
                                .call(&callback, &object(vec![("isDisposed", N::B(true))]), &[])
                                .unwrap();
                            let f = h.await_value(promise);
                            assert!(
                                matches!(drive(&h, f), Ok(N::U)),
                                "native full-chain callback void {index}"
                            );
                        }
                        assert_eq!(
                            h.st.borrow().timers.len(),
                            5,
                            "native independent full-chain timer ceiling"
                        );
                    }
                    result
                } else {
                    let registered = run_retry_chain_step(&mut h, &e, &attempt);
                    if family == "register" {
                        registered
                    } else {
                        assert!(
                            matches!(registered, Ok(N::U)),
                            "native timer registration void {scene}"
                        );
                        let callback = h.st.borrow().timers[0].0.clone();
                        if *scene == "drop-observer" {
                            let N::Builtin(id) = &callback else {
                                panic!("native timer callable")
                            };
                            let capture = h.st.borrow().callbacks.get(id).cloned().unwrap();
                            let observer = start_order_retry_callback(&h, capture);
                            assert_eq!(
                                h.st.borrow().fetch_calls,
                                1,
                                "native eager dropped callback prefix"
                            );
                            assert!(
                                matches!(get(&e, "_retryChainTimer"), N::Null),
                                "native eager timer reset before observer drop"
                            );
                            drop(observer);
                            h.base.drain();
                            assert_eq!(
                                h.st.borrow().settlements.len(),
                                1,
                                "native independent dropped timer settlement floor"
                            );
                            h.st.borrow().settlements[0].clone()
                        } else {
                            let promise = h
                                .call(&callback, &object(vec![("isDisposed", N::B(true))]), &[])
                                .unwrap();
                            let f = h.await_value(promise);
                            drive(&h, f)
                        }
                    }
                };
                if family == "refresh" {
                    let failed = matches!(*scene, "add-error" | "iterator-error" | "start-error");
                    if failed {
                        assert_error(&h, &result, false, scene)
                    } else {
                        assert!(
                            matches!(result, Ok(N::U)),
                            "native refresh root implicit void {scene}"
                        )
                    }
                    let expected = match *scene {
                        "active" => vec![s("z"), s("a"), s("b")],
                        "empty" | "iterator-error" | "full-chain" => vec![],
                        "falsy" => vec![s(""), N::Num(0.), N::Null, s("a")],
                        "add-error" => vec![s("a")],
                        _ => vec![s("a"), s("b")],
                    };
                    same_values(
                        &members(&get(&e, "_pendingRetryOrderIds")),
                        &expected,
                        &format!("native merged pending IDs {scene}"),
                    );
                    assert!(
                        eq(
                            &get(&e, "_retryChainActive"),
                            &N::B(!matches!(
                                *scene,
                                "add-error" | "iterator-error" | "full-chain"
                            ))
                        ),
                        "native active set after merge {scene}"
                    );
                    assert_eq!(
                        h.st.borrow().step_calls,
                        if matches!(*scene, "active" | "add-error" | "iterator-error") {
                            0
                        } else if *scene == "full-chain" {
                            5
                        } else {
                            1
                        },
                        "native independent merged start count {scene}"
                    );
                    let steps: Vec<N> =
                        h.st.borrow()
                            .effects
                            .iter()
                            .filter(|v| eq(&get(v, "0"), &s("next")))
                            .map(|v| get(v, "1"))
                            .collect();
                    same_values(
                        &steps,
                        &if *scene == "full-chain" {
                            (1..=5).map(|i| N::Num(i as f64)).collect::<Vec<_>>()
                        } else if matches!(*scene, "active" | "add-error" | "iterator-error") {
                            vec![]
                        } else {
                            vec![N::Num(1.)]
                        },
                        &format!("native initial attempt sequence {scene}"),
                    );
                    if *scene == "full-chain" {
                        let delays: Vec<_> =
                            h.st.borrow().timers.iter().map(|p| p.1.clone()).collect();
                        same_values(
                            &delays,
                            &[
                                N::Num(300.),
                                N::Num(700.),
                                N::Num(1500.),
                                N::Num(3000.),
                                N::Num(5000.),
                            ],
                            "native five progressive delays",
                        );
                        assert_eq!(
                            h.st.borrow().fetch_calls,
                            5,
                            "native full-chain fetch floor"
                        );
                        assert_eq!(
                            h.st.borrow().end_calls,
                            1,
                            "native full-chain exact end floor"
                        );
                        assert!(
                            matches!(get(&e, "_retryChainTimer"), N::Null),
                            "native full-chain timer null"
                        )
                    } else {
                        assert!(
                            eq(&get(&e, "_retryChainTimer"), &N::Num(7.)),
                            "native pure refresh virtual step preserves timer {scene}"
                        )
                    }
                } else if family == "register" {
                    if *scene == "set-error" {
                        assert_error(&h, &result, false, scene);
                        assert!(
                            eq(&get(&e, "_retryChainTimer"), &N::Num(7.)),
                            "native registration throw preserves old timer"
                        );
                        assert!(
                            h.st.borrow().timers.is_empty(),
                            "native failed registration no stored callback"
                        )
                    } else {
                        assert!(
                            matches!(result, Ok(N::U)),
                            "native registration implicit void {scene}"
                        );
                        assert_eq!(
                            h.st.borrow().timers.len(),
                            1,
                            "native exact timer registration floor"
                        );
                        assert!(
                            eq(&h.st.borrow().timers[0].1, &N::Num(delay(scene))),
                            "native independently expected delay {scene}"
                        );
                        assert!(
                            eq(&get(&e, "_retryChainTimer"), &N::Num(11.)),
                            "native actual registered timer handle {scene}"
                        )
                    }
                } else {
                    let early = matches!(
                        *scene,
                        "disposed" | "disposed-end-error" | "reset-error" | "disposed-get-error"
                    );
                    assert_eq!(
                        h.st.borrow().fetch_calls,
                        usize::from(!early),
                        "native independent timer fetch floor {scene}"
                    );
                    let later = matches!(*scene, "step-error" | "end-error" | "log-error");
                    if later
                        || matches!(
                            *scene,
                            "disposed-end-error" | "reset-error" | "disposed-get-error"
                        )
                    {
                        assert_error(&h, &result, later, scene)
                    } else {
                        assert!(
                            matches!(result, Ok(N::U)),
                            "native actual timer callback void {scene}"
                        )
                    }
                    let ends = match *scene {
                        "disposed" | "found-all" | "last-found" | "last-missing"
                        | "last-reject" | "disposed-end-error" => 1,
                        "end-error" => 2,
                        _ => 0,
                    };
                    assert_eq!(
                        h.st.borrow().end_calls,
                        ends,
                        "native independent end-call floor {scene}"
                    );
                    let ended = matches!(
                        *scene,
                        "disposed" | "found-all" | "last-found" | "last-missing" | "last-reject"
                    );
                    assert!(
                        eq(&get(&e, "_retryChainActive"), &N::B(!ended)),
                        "native actual end state {scene}"
                    );
                    let expected = if ended {
                        vec![]
                    } else if matches!(
                        *scene,
                        "partial"
                            | "duplicate-found"
                            | "missing-id"
                            | "delete-error"
                            | "next-error"
                    ) {
                        vec![s("b")]
                    } else {
                        vec![s("a"), s("b")]
                    };
                    same_values(
                        &members(&get(&e, "_pendingRetryOrderIds")),
                        &expected,
                        &format!("native found-ID removal {scene}"),
                    );
                    let nexts = if matches!(*scene, "step-once-error" | "step-error") {
                        2
                    } else if early || ended || matches!(*scene, "end-error" | "log-error") {
                        0
                    } else {
                        1
                    };
                    assert_eq!(
                        h.st.borrow().step_calls,
                        nexts,
                        "native exact continuation count {scene}"
                    );
                    for effect in &h.st.borrow().effects {
                        if eq(&get(effect, "0"), &s("next")) {
                            assert!(
                                eq(
                                    &get(effect, "1"),
                                    &if matches!(*scene, "string" | "stateful" | "stateful-reject")
                                    {
                                        s("21")
                                    } else {
                                        N::Num(2.)
                                    }
                                ),
                                "native captured attempt plus-one semantics {scene}"
                            )
                        }
                    }
                    assert!(
                        eq(
                            &get(&e, "_retryChainTimer"),
                            &if *scene == "reset-error" {
                                N::Num(11.)
                            } else {
                                N::Null
                            }
                        ),
                        "native timer reset exact state {scene}"
                    );
                }
                let logs: Vec<_> =
                    h.st.borrow()
                        .effects
                        .iter()
                        .filter(|v| eq(&get(v, "0"), &s("log")))
                        .cloned()
                        .collect();
                let expected = if family == "register" {
                    vec![]
                } else {
                    expected_logs(scene)
                };
                assert_eq!(
                    logs.len(),
                    expected.len(),
                    "native independent warning floor {family}/{scene}"
                );
                for (log, message) in logs.iter().zip(expected) {
                    assert!(
                        eq(&get(log, "1"), &message),
                        "native full warning UTF16 {family}/{scene}"
                    );
                    assert!(
                        eq(&get(log, "2"), &s("Warning")),
                        "native warning severity {scene}"
                    );
                    assert_eq!(values(log).len(), 3, "native warning arity {scene}")
                }
                let deleted: Vec<N> =
                    h.st.borrow()
                        .effects
                        .iter()
                        .filter(|v| eq(&get(v, "0"), &s("delete")))
                        .map(|v| get(v, "1"))
                        .collect();
                let expected_deleted = if family != "timer" {
                    vec![]
                } else {
                    match *scene {
                        "found-all" | "last-found" | "delete-error" => vec![s("a"), s("b")],
                        "partial" | "next-error" => vec![s("a")],
                        "duplicate-found" => vec![s("a"), s("z")],
                        "missing-id" => vec![N::U, s("a")],
                        _ => vec![],
                    }
                };
                same_values(
                    &deleted,
                    &expected_deleted,
                    &format!("native independent found-ID enumeration {family}/{scene}"),
                );
                let closes =
                    h.st.borrow()
                        .effects
                        .iter()
                        .filter(|v| eq(&get(v, "0"), &s("close")))
                        .count();
                assert_eq!(
                    closes,
                    usize::from(matches!(*scene, "add-error" | "delete-error")),
                    "native iterator close only on body error {scene}"
                );
                let expected_hints = if *scene == "stateful" {
                    if family == "register" {
                        vec![s("number")]
                    } else {
                        vec![s("number"), s("number"), s("default")]
                    }
                } else if *scene == "stateful-reject" {
                    vec![s("number"), s("string"), s("number"), s("default")]
                } else {
                    vec![]
                };
                let hints: Vec<N> =
                    h.st.borrow()
                        .effects
                        .iter()
                        .filter(|v| eq(&get(v, "0"), &s("coerce")))
                        .map(|v| get(v, "1"))
                        .collect();
                same_values(
                    &hints,
                    &expected_hints,
                    &format!("native original attempt coercion sequence {family}/{scene}"),
                );
                (family, *scene, snapshot(&h, &result))
            }));
            match outcome {
                Ok(row) => native_rows.push(row),
                Err(error) => {
                    let text = error
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| error.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_default();
                    failures.push(format!("{family}/{scene}: {text}"))
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "native scene failures before source {failures:?}"
    );
    assert_eq!(native_rows.len(), 46, "native completed retry floor");
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/order-retry-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source retry oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 46, "independent source retry floor");
    assert_eq!(source["last_index"], 45);
    for ((family, scene, value), row) in native_rows.iter().zip(cases) {
        assert_eq!(row["family"], *family);
        assert_eq!(row["scene"], *scene);
        native::assert_value(
            value,
            &row["state"],
            &format!("supplemental source {family}/{scene}"),
        )
    }
}
