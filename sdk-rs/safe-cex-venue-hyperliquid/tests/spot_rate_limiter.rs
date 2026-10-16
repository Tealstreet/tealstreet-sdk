#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[path = "support/spot_limiter_host.rs"]
mod support;
use cache::{N, arr, eq, object, s};
use safe_cex_venue_hyperliquid::spot_rate_limiter::*;
use serde_json::{Value, json};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    process::Command,
};
use support::{Host, input, own};
fn same(actual: &N, expected: &Value, label: &str) {
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
            eq(actual, &expected),
            "{label}: {actual:?} expected {expected:?}"
        ),
    }
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
        N::S(v) => json!(["string", v.0]),
        N::A(v) => json!(["array", v.borrow().iter().map(encode).collect::<Vec<_>>()]),
        N::O(v) => json!([
            "object",
            v.borrow()
                .fields
                .iter()
                .map(|(k, v, _)| json!([k, encode(v)]))
                .collect::<Vec<_>>()
        ]),
        N::Builtin(_) => json!(["function"]),
        _ => panic!("limiter wire projection"),
    }
}
fn run(row: &Value) -> N {
    let mut h = Host::new(row);
    let limiter = h.limiter.clone();
    let mut observed = vec![];
    match row["family"].as_str().unwrap() {
        "constructor" => spot_limiter_constructor(&mut h, &limiter).unwrap(),
        "refill" => spot_refill_tokens(&mut h, &limiter).unwrap(),
        "wait" => {
            let task = spot_wait_for_token(h.clone(), limiter.clone(), h.weight.clone());
            if row["drop"] == true {
                drop(task)
            } else {
                observed.push(h.promise(Box::pin(task)))
            }
            if row["expectedTimers"].as_array().unwrap().is_empty() && row["error"] != true {
                same(
                    &own(&limiter, "tokens"),
                    &row["expectedTokens"],
                    &format!(
                        "LIMITER native {} eager prefix tokens",
                        row["name"].as_str().unwrap()
                    ),
                );
            }
            if row["concurrent"] == true {
                let task = spot_wait_for_token(h.clone(), limiter.clone(), h.weight.clone());
                observed.push(h.promise(Box::pin(task)))
            }
            if row["replace"] == true {
                h.replace_recursive()
            }
            if let Some(fire) = row["fire"].as_array() {
                for index in fire {
                    h.fire(index.as_u64().unwrap() as usize)
                }
            }
            h.pump();
        }
        _ => panic!("limiter family"),
    }
    let label = format!("LIMITER native {}", row["name"].as_str().unwrap());
    let N::O(fields) = &limiter else {
        unreachable!()
    };
    assert_eq!(
        fields
            .borrow()
            .fields
            .iter()
            .take(3)
            .map(|p| p.0.clone())
            .collect::<Vec<_>>(),
        ["tokens", "lastRefill", "capacity"],
        "{label} own declaration order"
    );
    if row["family"] == "constructor" {
        assert_eq!(
            fields
                .borrow()
                .fields
                .iter()
                .map(|p| p.0.clone())
                .collect::<Vec<_>>(),
            ["tokens", "lastRefill", "capacity"],
            "LIMITER native constructor exact own field floor"
        );
    }
    same(
        &own(&limiter, "tokens"),
        &row["expectedTokens"],
        &format!("{label} independent tokens"),
    );
    same(
        &own(&limiter, "lastRefill"),
        &row["expectedLast"],
        &format!("{label} captured lastRefill"),
    );
    let expected_capacity = if row["family"] == "constructor" {
        json!(1200)
    } else {
        row["capacity"].clone()
    };
    same(
        &own(&limiter, "capacity"),
        &expected_capacity,
        &format!("{label} capacity"),
    );
    let timers = h.timers();
    let expected = row["expectedTimers"].as_array().unwrap();
    assert_eq!(
        timers.len(),
        expected.len(),
        "{label} independent timer floor"
    );
    for ((_, delay), expected) in timers.iter().zip(expected) {
        same(delay, expected, &format!("{label} exact delay arithmetic"));
    }
    assert!(h.alias(), "{label} original recursive weight identity");
    let mut status = vec![];
    if row["family"] == "wait" && row["drop"] != true {
        assert_eq!(
            observed.len(),
            if row["concurrent"] == true { 2 } else { 1 },
            "{label} independent observer floor"
        );
        for p in observed {
            match h.result(&p) {
                None => {
                    assert_eq!(row["pending"], true, "{label} unexpected pending");
                    status.push(arr(vec![s("pending")]))
                }
                Some(Err(e)) => {
                    assert_eq!(row["error"], true, "{label} unexpected rejection");
                    assert!(
                        eq(&e, &h.original_error),
                        "{label} original rejection identity"
                    );
                    status.push(arr(vec![s("rejected")]))
                }
                Some(Ok(v)) => {
                    assert_ne!(row["pending"], true, "{label} premature settlement");
                    assert_ne!(row["error"], true, "{label} swallowed rejection");
                    same(
                        &v,
                        &row["expectedResult"],
                        &format!("{label} adopted result"),
                    );
                    status.push(arr(vec![s("fulfilled"), v]))
                }
            }
        }
    } else if row["drop"] == true {
        assert_eq!(
            h.pending_tasks(),
            0,
            "LIMITER native dropped outer task completed after adoption"
        );
        status.push(s("dropped"))
    }
    object(vec![
        ("name", s(row["name"].as_str().unwrap())),
        (
            "state",
            arr(vec![
                own(&limiter, "tokens"),
                own(&limiter, "lastRefill"),
                own(&limiter, "capacity"),
            ]),
        ),
        ("timers", arr(timers.into_iter().map(|p| p.1).collect())),
        ("trace", h.trace()),
        ("observed", arr(status)),
        ("recursiveWeightAlias", N::B(h.alias())),
    ])
}
#[test]
fn spot_limiter_native_before_source() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/spot-limiter-inputs.json")).unwrap();
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 34, "LIMITER independent input floor");
    let mut native = vec![];
    let mut failures = vec![];
    for row in rows {
        match catch_unwind(AssertUnwindSafe(|| run(row))) {
            Ok(v) => native.push(v),
            Err(_) => failures.push(row["name"].clone()),
        }
    }
    assert!(failures.is_empty(), "LIMITER native failures {failures:?}");
    assert_eq!(
        native.len(),
        34,
        "LIMITER independently completed native floor"
    );
    assert!(
        eq(
            &own(native.last().unwrap(), "name"),
            &s("fractional-second-rounding")
        ),
        "LIMITER native final iteration33"
    );
    // Every direct native assertion above completes before any wire projection.
    let expected = native.iter().map(encode).collect::<Vec<_>>();
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/spot-limiter-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "LIMITER source {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["last_index"], 33, "LIMITER source stop iteration");
    assert_eq!(
        value["cases"],
        json!(expected),
        "LIMITER source wire after every native assertion"
    );
}
