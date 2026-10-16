#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[path = "support/core_book_host.rs"]
mod support;
use cache::{N, s};
use safe_cex_venue_hyperliquid::{
    core_book_listener::*, spot_json::JsString, ws_ping_lifecycle::WsPingHost,
};
use serde_json::{Value, json};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    process::Command,
};
use support::{Host, input, own, same};
fn check(actual: &N, expected: &N, label: &str) {
    match (actual, expected) {
        (N::Num(a), N::Num(b)) => {
            if b.is_nan() {
                assert!(a.is_nan(), "{label}")
            } else {
                assert_eq!(a.to_bits(), b.to_bits(), "{label}")
            }
        }
        _ => assert!(
            same(actual, expected),
            "{label}: {actual:?} != {expected:?}"
        ),
    }
}
fn array(v: &N) -> Vec<N> {
    let N::A(v) = v else { panic!("BOOK array") };
    v.borrow().clone()
}
fn keys(v: &N) -> Vec<String> {
    let N::O(v) = v else { panic!("BOOK object") };
    v.borrow().fields.iter().map(|p| p.0.clone()).collect()
}
fn undef(v: Result<N, N>) {
    assert!(
        matches!(v, Ok(N::U)),
        "BOOK discarded callback return {v:?}"
    )
}
fn run(row: &Value) -> N {
    let mut h = Host::new(row);
    let socket = h.socket.clone();
    let result = core_listen_order_book(&mut h, socket, s("BTC/USDC"), N::Builtin("output".into()));
    let outer_error = matches!(
        row["expectedError"].as_str(),
        Some("send" | "missing" | "precision")
    );
    let unsubscribe = if outer_error {
        let error = result.expect_err("BOOK expected outer rejection");
        if row["expectedError"] == "send" {
            assert!(same(&error, &h.error), "BOOK original send error identity")
        } else {
            let msg = if row["expectedError"] == "missing" {
                "Market with symbol BTC/USDC not found"
            } else {
                "Market with symbol BTC/USDC has no price precision"
            };
            check(
                &own(&error, "message"),
                &s(msg),
                "BOOK constructed error message",
            );
        }
        None
    } else {
        Some(result.unwrap())
    };
    let name = row["name"].as_str().unwrap();
    if !matches!(name, "missing-market" | "zero-precision") {
        let inner = h.slot(&h.inner, &JsString("inner".encode_utf16().collect()));
        assert_eq!(
            keys(&inner),
            ["updates", "isSnapshotLoaded"],
            "BOOK independent initial inner keys"
        );
        assert_eq!(
            array(&own(&inner, "updates")).len(),
            0,
            "BOOK independent initial update count"
        );
        check(
            &own(&inner, "isSnapshotLoaded"),
            &N::B(false),
            "BOOK independent initial snapshot false",
        );
    }
    if matches!(name, "unsubscribe-pending" | "queued-poll") {
        undef(h.invoke(unsubscribe.as_ref().unwrap(), &[]))
    }
    if matches!(
        name,
        "disconnected-poll" | "queued-poll" | "zero-handle" | "timer-reentry"
    ) {
        h.connected(true);
        let timer = h.timers[0].1.clone();
        undef(h.invoke(&timer, &[]))
    }
    let mut callback_error = false;
    for action in row["actions"].as_array().unwrap() {
        if action["kind"] == "mutate-bbo" {
            let best = own(&h.last_bbo, "bbo");
            let best = array(&best)[0].clone();
            h.set(&best, "px", &input(&action["px"])).unwrap();
            continue;
        }
        let event = h.frame(action);
        let parser = h.parser(action["kind"] == "bbo");
        let registered = h.slot(
            &h.handlers,
            &h.key(if action["kind"] == "bbo" {
                "bbo."
            } else {
                "l2Book."
            }),
        );
        assert!(
            array(&registered).iter().any(|f| same(f, &parser)),
            "BOOK actual installed parser identity"
        );
        let result = h.invoke(&parser, &[event]);
        if row["expectedError"] == "callback" {
            let error = result.expect_err("BOOK callback throws");
            assert!(
                same(&error, &h.error),
                "BOOK original callback error identity"
            );
            callback_error = true
        } else {
            undef(result)
        }
    }
    assert_eq!(
        callback_error,
        row["expectedError"] == "callback",
        "BOOK completed callback error path"
    );
    if let Some(f) = &unsubscribe {
        if !matches!(name, "unsubscribe-pending" | "queued-poll") {
            undef(h.invoke(f, &[]));
            if name == "double-unsubscribe" {
                undef(h.invoke(f, &[]))
            }
        }
    }
    // Every decoded native value assertion precedes source encoding/comparison.
    let books = row["expectedBooks"].as_array().unwrap();
    assert_eq!(
        h.books.len(),
        books.len(),
        "BOOK independent callback count {name}"
    );
    assert_eq!(
        h.pre_total.len(),
        books.len(),
        "BOOK total invocation count {name}"
    );
    assert_eq!(
        h.total_inputs.len(),
        books.len(),
        "BOOK independent original total input count"
    );
    for (i, (book, expected)) in h.books.iter().zip(books).enumerate() {
        assert!(
            same(book, &h.total_inputs[i]),
            "BOOK callback original total object identity"
        );
        assert_eq!(keys(book), ["bids", "asks"], "BOOK exact output keys");
        for old in &h.books[..i] {
            assert!(!same(old, book), "BOOK fresh output identity")
        }
        for (j, side) in ["bids", "asks"].into_iter().enumerate() {
            let actual = array(&own(book, side));
            let before = array(&own(&h.pre_total[i], side));
            let expected = expected[j].as_array().unwrap();
            assert_eq!(
                actual.len(),
                expected.len(),
                "BOOK independent side length {name}/{i}/{side}"
            );
            assert_eq!(
                before.len(),
                expected.len(),
                "BOOK original pre-total side floor"
            );
            for (k, (level, expected)) in actual.iter().zip(expected).enumerate() {
                assert_eq!(
                    keys(level),
                    ["price", "amount", "total"],
                    "BOOK exact level keys"
                );
                for (field, col) in [("price", 0), ("amount", 1), ("total", 2)] {
                    check(
                        &own(level, field),
                        &input(&expected[col]),
                        &format!("BOOK decoded native {name}/{i}/{side}/{k}/{field}"),
                    )
                }
                check(
                    &own(&before[k], "total"),
                    &N::Num(0.),
                    "BOOK total reset before imported accumulator",
                );
            }
        }
    }
    assert_eq!(
        h.sends.iter().filter(|p| !p.0).count(),
        row["expectedSubscriptions"].as_u64().unwrap() as usize,
        "BOOK independent subscription attempts"
    );
    assert_eq!(
        h.sends.iter().filter(|p| p.0).count(),
        row["expectedUnsubscriptions"].as_u64().unwrap() as usize,
        "BOOK independent unsubscription attempts"
    );
    for (i, (_, payload)) in h.sends.iter().enumerate() {
        check(
            &own(payload, "type"),
            &s(if i % 2 == 0 { "l2Book" } else { "bbo" }),
            "BOOK source send kind order",
        )
    }
    let count = h.slot(&h.topics, &h.key("l2Book."));
    check(
        &count,
        &input(&row["expectedCount"]),
        "BOOK independent topic count",
    );
    for prefix in ["l2Book.", "bbo."] {
        let entry = h.slot(&h.handlers, &h.key(prefix));
        if matches!(entry, N::U) {
            assert!(matches!(name, "missing-market" | "zero-precision"))
        } else {
            let a = array(&entry);
            assert_eq!(
                a.len(),
                row["expectedHandlers"].as_u64().unwrap() as usize,
                "BOOK independent retained handler count"
            );
            if name == "foreign-handlers" {
                assert!(
                    same(&a[0], &N::Builtin("foreign".into())),
                    "BOOK retained original foreign handler"
                )
            }
        }
    }
    let expected_filters = if outer_error {
        0
    } else if name == "double-unsubscribe" {
        4
    } else {
        2
    };
    assert_eq!(
        h.filter_results.len(),
        expected_filters,
        "BOOK independent cleanup filter invocation floor"
    );
    if h.filter_results.len() >= 2 {
        for (prefix, result) in [
            ("l2Book.", h.filter_results.len() - 2),
            ("bbo.", h.filter_results.len() - 1),
        ] {
            let final_array = h.slot(&h.handlers, &h.key(prefix));
            assert!(
                !same(&final_array, &h.filter_results[result]),
                "BOOK independent filter spread identity"
            );
        }
    }
    for (table, expected) in [
        (
            &h.handlers,
            if matches!(name, "missing-market" | "zero-precision") {
                vec![]
            } else {
                vec![h.key("l2Book."), h.key("bbo.")]
            },
        ),
        (
            &h.topics,
            if matches!(name, "missing-market" | "zero-precision") {
                vec![]
            } else {
                vec![h.key("l2Book.")]
            },
        ),
    ] {
        let actual = &h
            .tables
            .iter()
            .find(|p| same(&p.value, table))
            .unwrap()
            .entries;
        assert_eq!(
            actual.iter().map(|p| p.0.clone()).collect::<Vec<_>>(),
            expected,
            "BOOK exact native UTF16 dictionary keys"
        )
    }
    assert_eq!(
        h.tables
            .iter()
            .find(|p| same(&p.value, &h.inner))
            .unwrap()
            .entries
            .len(),
        row["expectedInner"].as_u64().unwrap() as usize,
        "BOOK independent inner-state floor"
    );
    assert_eq!(
        h.timers.len(),
        row["expectedTimers"].as_u64().unwrap() as usize,
        "BOOK independent timer floor"
    );
    assert_eq!(
        h.clear.len(),
        row["expectedClear"].as_array().unwrap().len(),
        "BOOK independent clear floor"
    );
    for (a, b) in h.clear.iter().zip(row["expectedClear"].as_array().unwrap()) {
        check(a, &input(b), "BOOK shared timeout cell overwrite")
    }
    assert_eq!(
        h.logs.len(),
        row["expectedLogs"].as_array().unwrap().len(),
        "BOOK independent log floor"
    );
    for (a, b) in h.logs.iter().zip(row["expectedLogs"].as_array().unwrap()) {
        check(a, &input(b), "BOOK native log text")
    }
    h.snapshot()
}
fn encode(v: &N) -> Value {
    match v {
        N::U => json!(["u"]),
        N::Null => json!(["null"]),
        N::B(v) => json!(["bool", v]),
        N::Num(v) => json!([
            "number",
            if v.is_nan() {
                "nan".into()
            } else {
                format!("{:016x}", v.to_bits())
            }
        ]),
        N::S(JsString(v)) => json!(["string", v]),
        N::Builtin(_) => json!(["function"]),
        N::A(v) => json!(["array", v.borrow().iter().map(encode).collect::<Vec<_>>()]),
        N::O(v) => json!([
            "object",
            v.borrow()
                .fields
                .iter()
                .map(|(k, v, _)| json!([k, encode(v)]))
                .collect::<Vec<_>>()
        ]),
        _ => panic!("candle wire fixture"),
    }
}
#[test]
fn core_order_book_native_before_source() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/core-book-listener-inputs.json")).unwrap();
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 34, "BOOK independent source input floor");
    let mut completed = vec![];
    let mut failures = vec![];
    for row in rows {
        match catch_unwind(AssertUnwindSafe(|| run(row))) {
            Ok(v) => completed.push(v),
            Err(_) => failures.push(row["name"].clone()),
        }
    }
    assert!(failures.is_empty(), "BOOK native failures {failures:?}");
    assert_eq!(
        completed.len(),
        34,
        "BOOK independently completed native floor"
    );
    assert_eq!(rows[33]["name"], "utf16-keys", "BOOK native stop33");
    let wire = completed.iter().map(encode).collect::<Vec<_>>();
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/core-book-listener-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "BOOK source {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["last_index"], 33, "BOOK source stop33");
    assert_eq!(
        source["cases"],
        json!(wire),
        "BOOK wire after every native assertion"
    );
}
