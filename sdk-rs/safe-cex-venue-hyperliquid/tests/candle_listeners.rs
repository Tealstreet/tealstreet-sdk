#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[path = "support/candle_listener_host.rs"]
mod support;
use cache::{N, arr, object, s};
use safe_cex_venue_hyperliquid::{candle_listeners::*, spot_json::JsString};
use serde_json::{Value, json};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    process::Command,
};
use support::{Host, input, own, same};
fn check(actual: &N, expected: &Value, label: &str) {
    let expected = input(expected);
    match (actual, &expected) {
        (N::Num(a), N::Num(b)) => {
            if b.is_nan() {
                assert!(a.is_nan(), "{label}")
            } else {
                assert_eq!(a.to_bits(), b.to_bits(), "{label}")
            }
        }
        _ => assert!(
            same(actual, &expected),
            "{label}: actual{actual:?}, expected{expected:?}"
        ),
    }
}
fn undefined(r: Result<N, N>, label: &str) {
    assert!(matches!(r, Ok(N::U)), "{label}: {r:?}")
}
fn snapshot_dict(h: &Host, table: &N) -> N {
    let d = h.dicts.iter().find(|d| same(&d.value, table)).unwrap();
    arr(d
        .entries
        .iter()
        .map(|(k, v)| arr(vec![N::S(k.clone()), v.clone()]))
        .collect())
}
fn run(row: &Value) -> N {
    let mut h = Host::new(row);
    let socket = h.socket.clone();
    let opts = h.opts.clone();
    let label = format!(
        "CANDLE native {}/{}",
        row["venue"].as_str().unwrap(),
        row["name"].as_str().unwrap()
    );
    let old_handlers = h.slot(&h.handlers, &h.expected_key(true));
    let result = if row["venue"] == "core" {
        core_listen_ohlcv(&mut h, socket, opts, N::Builtin("output".into()))
    } else {
        spot_listen_ohlcv(&mut h, socket, opts, N::Builtin("output".into()))
    };
    let unsubscribe = if row["outerError"] == true {
        let Err(e) = result else {
            panic!("{label} missing outer error")
        };
        assert!(same(&e, &h.error), "{label} original outer error");
        None
    } else {
        Some(result.unwrap())
    };
    if matches!(
        row["name"].as_str().unwrap(),
        "unsubscribe-pending" | "zero-handle" | "queued-poll"
    ) {
        undefined(
            h.invoke(unsubscribe.as_ref().unwrap(), &[]),
            &format!("{label} early unsubscribe return"),
        );
    }
    if matches!(
        row["name"].as_str().unwrap(),
        "disconnected-poll"
            | "zero-handle"
            | "queued-poll"
            | "timer-reentry"
            | "clear-getter-reentry"
            | "clear-call-reentry"
    ) {
        h.set_connected(true);
        undefined(h.fire(0), &format!("{label} poll return"));
    }
    if row["expectedStats"].as_u64().unwrap() > 0 {
        let parser = h.parser().expect("allocated native parser");
        let handlers = h.slot(&h.handlers, &h.expected_key(true));
        let N::A(a) = &handlers else {
            panic!("{label} registered native parser array")
        };
        assert!(
            a.borrow().iter().any(|p| same(p, &parser)),
            "{label} registered original parser identity"
        );
        if matches!(old_handlers, N::A(_)) {
            assert!(
                !same(&handlers, &old_handlers),
                "{label} registration spread fresh array"
            )
        }
        let frame = h.frames.clone();
        let parsed = h.invoke(&parser, &[frame]);
        if row["parserError"] == true {
            let Err(e) = parsed else {
                panic!("{label} missing parser error")
            };
            assert!(same(&e, &h.error), "{label} original callback error")
        } else {
            undefined(parsed, &format!("{label} parser discards callback return"));
        }
    }
    if !matches!(
        row["name"].as_str().unwrap(),
        "unsubscribe-pending" | "zero-handle" | "queued-poll"
    ) {
        if let Some(unsubscribe) = &unsubscribe {
            undefined(
                h.invoke(unsubscribe, &[]),
                &format!("{label} unsubscribe discards cleanup return"),
            );
            if matches!(
                row["name"].as_str().unwrap(),
                "double-unsubscribe" | "clear-getter-reentry" | "clear-call-reentry"
            ) {
                undefined(
                    h.invoke(unsubscribe, &[]),
                    &format!("{label} repeated unsubscribe return"),
                );
            }
        }
    }
    let expected = row["expectedCallbacks"].as_array().unwrap();
    assert_eq!(
        h.candles.len(),
        expected.len(),
        "{label} independent callback floor"
    );
    for (i, (candle, expected)) in h.candles.iter().zip(expected).enumerate() {
        let N::O(fields) = candle else {
            panic!("{label} native candle object")
        };
        assert_eq!(
            fields
                .borrow()
                .fields
                .iter()
                .map(|p| p.0.clone())
                .collect::<Vec<_>>(),
            ["timestamp", "open", "high", "low", "close", "volume"],
            "{label} exact candle keys"
        );
        assert_eq!(
            expected.as_array().unwrap().len(),
            6,
            "CANDLE independent six native field expectations"
        );
        for (k, e) in ["timestamp", "open", "high", "low", "close", "volume"]
            .into_iter()
            .zip(expected.as_array().unwrap())
        {
            check(&own(candle, k), e, &format!("{label} decoded {k}"));
        }
        for old in &h.candles[..i] {
            assert!(
                !same(candle, old),
                "{label} distinct native candle allocation"
            );
        }
    }
    assert_eq!(
        h.send_calls,
        row["expectedSends"].as_u64().unwrap() as usize,
        "{label} independent actual send attempts"
    );
    assert_eq!(
        h.sent.len(),
        row["expectedSends"].as_u64().unwrap() as usize,
        "{label} independent send floor"
    );
    assert_eq!(
        h.stat_keys.len(),
        row["expectedStats"].as_u64().unwrap() as usize,
        "{label} independent stats floor"
    );
    for key in &h.stat_keys {
        assert_eq!(*key, h.expected_key(false), "{label} raw UTF16 topic stats");
    }
    assert_eq!(
        h.cleanup_keys.len(),
        row["expectedCleanup"].as_u64().unwrap() as usize,
        "{label} independent cleanup floor"
    );
    for key in &h.cleanup_keys {
        assert_eq!(
            *key,
            h.expected_key(true),
            "{label} raw UTF16 handler cleanup"
        );
    }
    assert_eq!(
        h.timers.len(),
        row["expectedTimers"].as_u64().unwrap() as usize,
        "{label} independent timer floor"
    );
    assert_eq!(
        h.timers.iter().map(|t| t.id).collect::<Vec<_>>(),
        (0..row["expectedTimers"].as_u64().unwrap())
            .map(|i| if row["name"] == "zero-handle" {
                0
            } else {
                42 * (i as i32 + 1)
            })
            .collect::<Vec<_>>(),
        "{label} native retained timer handles"
    );
    assert_eq!(
        h.cleared,
        row["expectedClear"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap() as i32)
            .collect::<Vec<_>>(),
        "{label} shared timeout cell and overwrite order"
    );
    assert_eq!(
        h.interval_reads,
        row["expectedIntervalReads"].as_u64().unwrap() as usize,
        "{label} independent live interval read floor"
    );
    assert_eq!(
        h.bad_interval_reads, 0,
        "{label} mismatching symbol skips interval getter"
    );
    let missing = row["venue"] == "core" && row["name"] == "missing-market";
    for (table, key, has_key, name) in [
        (&h.handlers, h.expected_key(true), !missing, "handlers"),
        (&h.topics, h.expected_key(false), !missing, "topics"),
        (
            &h.other_handlers,
            h.expected_key(true),
            row["name"] == "table-swap",
            "other handlers",
        ),
        (
            &h.other_topics,
            h.expected_key(false),
            row["name"] == "table-swap",
            "other topics",
        ),
    ] {
        let actual = h
            .dicts
            .iter()
            .find(|d| same(&d.value, table))
            .unwrap()
            .entries
            .iter()
            .map(|p| p.0.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            if has_key { vec![key] } else { vec![] },
            "{label} exact native {name} keys"
        );
    }
    let handlers = h.slot(&h.handlers, &h.expected_key(true));
    let expected_handlers = row["expectedHandlers"].as_u64().unwrap() as usize;
    if row["venue"] == "core" && row["name"] == "missing-market" {
        assert!(matches!(handlers, N::U));
        assert_eq!(h.logs.len(), 1);
        assert_eq!(h.logs[0].0,"[Hyperliquid] listenOHLCV: Market not found for symbol \"BTC/USDC\", skipping subscription".encode_utf16().collect::<Vec<_>>(),"{label} full missing market log");
    } else {
        assert!(h.logs.is_empty());
        let N::A(a) = &handlers else {
            panic!("{label} native handler array")
        };
        assert_eq!(
            a.borrow().len(),
            expected_handlers,
            "{label} independent retained handler floor"
        );
        if row["name"] == "retain-foreign" || row["name"] == "table-swap" {
            assert!(
                same(&a.borrow()[0], &h.foreign),
                "{label} exact foreign function retained"
            )
        }
    }
    check(
        &h.slot(&h.topics, &h.expected_key(false)),
        &row["expectedCount"],
        &format!("{label} native topic count"),
    );
    if let Some(filtered) = h.filter_results.last() {
        assert!(
            !same(&handlers, filtered),
            "{label} filter result separately spread"
        );
    }
    if row["name"] == "table-swap" {
        let original = h.slot(&h.other_handlers, &h.expected_key(true));
        let N::A(a) = original else {
            panic!("other handlers")
        };
        assert_eq!(a.borrow().len(), 1);
        assert!(same(&a.borrow()[0], &h.foreign));
        check(
            &h.slot(&h.other_topics, &h.expected_key(false)),
            &json!("2"),
            &format!("{label} RHS dictionary unchanged"),
        );
    }
    object(vec![
        ("venue", s(row["venue"].as_str().unwrap())),
        ("name", s(row["name"].as_str().unwrap())),
        ("effects", arr(h.effects.clone())),
        ("handlers", snapshot_dict(&h, &h.handlers)),
        ("topics", snapshot_dict(&h, &h.topics)),
        ("otherHandlers", snapshot_dict(&h, &h.other_handlers)),
        ("otherTopics", snapshot_dict(&h, &h.other_topics)),
    ])
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
fn candle_listeners_native_before_source() {
    let inputs: Value =
        serde_json::from_str(include_str!("fixtures/candle-listener-inputs.json")).unwrap();
    let inputs = inputs.as_array().unwrap();
    assert_eq!(inputs.len(), 58, "CANDLE independent input floor");
    let mut native = vec![];
    let mut failures = vec![];
    for row in inputs {
        match catch_unwind(AssertUnwindSafe(|| run(row))) {
            Ok(v) => native.push(v),
            Err(_) => failures.push(format!("{}/{}", row["venue"], row["name"])),
        }
    }
    assert!(failures.is_empty(), "CANDLE native failures {failures:?}");
    assert_eq!(
        native.len(),
        58,
        "CANDLE independently completed native floor"
    );
    assert!(same(
        &own(native.last().unwrap(), "name"),
        &s("parse-callee-capture")
    ));
    assert!(
        same(&own(native.last().unwrap(), "venue"), &s("spot")),
        "CANDLE native stop57"
    );
    // Native proof completes for both bodies before the first wire assertion.
    let wire = native.iter().map(encode).collect::<Vec<_>>();
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/candle-listener-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "CANDLE source {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(source["last_index"], 57, "CANDLE source stop");
    assert_eq!(
        source["cases"],
        json!(wire),
        "CANDLE wire comparison after every native assertion"
    );
}
