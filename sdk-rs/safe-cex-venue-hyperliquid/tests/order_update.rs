#[allow(dead_code)]
#[path = "support/spot_account_host.rs"]
mod account;
#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[allow(dead_code)]
#[path = "support/rest_host.rs"]
mod native;
#[path = "support/update_host.rs"]
mod support;
use account::get;
use cache::{N, arr, eq, num, object, s};
use safe_cex_venue_hyperliquid::order_update::*;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    process::Command,
};
use support::{Host, classification, hip, initial_type, raw_price, side};
const SCENES: [&str; 41] = [
    "limit",
    "hip-limit",
    "tp-sell-cross",
    "tp-buy-cross",
    "tp-sell-keep",
    "tp-buy-keep",
    "sl-sell-projected-tp",
    "sl-buy-projected-tp",
    "sl-keep",
    "equal-sell",
    "equal-buy",
    "divergence-quarter",
    "divergence-over",
    "wrong-denominator",
    "last-fallback",
    "index-only",
    "no-reference",
    "missing-ticker",
    "nan-trigger",
    "zero-trigger",
    "infinite-trigger",
    "price-undefined",
    "amount-zero",
    "inherited-trigger-fields",
    "inherited-update",
    "raw-reduce",
    "post-only",
    "cancel-error",
    "place-error",
    "dead-error",
    "log-error",
    "store-error",
    "missing-market-id",
    "order-null",
    "update-null",
    "opts-null",
    "opts-undefined",
    "fresh-id-after-cancel",
    "drop-observer",
    "missing-market",
    "utf16-symbol",
];
fn values(v: &N) -> Vec<N> {
    let N::A(a) = v else {
        panic!("UPDATE native array")
    };
    a.borrow().clone()
}
fn fields(v: &N) -> Vec<String> {
    let N::O(o) = v else {
        panic!("UPDATE native object")
    };
    o.borrow().fields.iter().map(|p| p.0.clone()).collect()
}
fn effects(h: &Host, k: &str) -> Vec<N> {
    h.st.borrow()
        .effects
        .iter()
        .filter(|v| eq(&get(v, "0"), &s(k)))
        .cloned()
        .collect()
}
fn count(h: &Host, k: &str) -> usize {
    effects(h, k).len()
}
fn switched(scene: &str) -> bool {
    [
        "tp-sell-cross",
        "tp-buy-cross",
        "equal-sell",
        "divergence-quarter",
        "last-fallback",
        "index-only",
        "log-error",
    ]
    .contains(&scene)
}
fn new_kind(scene: &str) -> &'static str {
    if switched(scene) {
        "sl"
    } else {
        initial_type(scene)
    }
}
fn scalar(actual: &N, want: &N, label: &str) {
    if matches!(want,N::Num(n)if n.is_nan()) {
        assert!(matches!(actual,N::Num(n)if n.is_nan()), "{label}")
    } else if let (N::Num(a), N::Num(b)) = (actual, want) {
        assert_eq!(a.to_bits(), b.to_bits(), "{label}")
    } else {
        assert!(eq(actual, want), "{label}")
    }
}
fn expected_log(scene: &str) -> Option<N> {
    let (message, severity) = if switched(scene) {
        (
            format!(
                "Order type changed from tp to sl based on new price {} vs market 100",
                if ["tp-buy-cross"].contains(&scene) {
                    120
                } else if scene == "equal-sell" {
                    100
                } else {
                    90
                }
            ),
            "Info",
        )
    } else if ["divergence-over", "wrong-denominator"].contains(&scene) {
        (
            format!(
                "Skipping TP/SL type re-evaluation for BTC/USDC: inconsistent reference prices (mark=100, last={}, index=100)",
                if scene == "wrong-denominator" {
                    "130"
                } else {
                    "125.01"
                }
            ),
            "Warning",
        )
    } else if ["sl-sell-projected-tp", "sl-buy-projected-tp"].contains(&scene) {
        (
            format!(
                "Skipping SL->TP auto-switch for BTC/USDC at trigger {}; keeping stop-loss type",
                if scene == "sl-buy-projected-tp" {
                    90
                } else {
                    120
                }
            ),
            "Warning",
        )
    } else if scene == "utf16-symbol" {
        let mut text = "Skipping SL->TP auto-switch for "
            .encode_utf16()
            .collect::<Vec<_>>();
        text.extend([0xd800, 83]);
        text.extend(" at trigger 120; keeping stop-loss type".encode_utf16());
        return Some(arr(vec![
            N::S(safe_cex_venue_hyperliquid::spot_json::JsString(text)),
            s("Warning"),
        ]));
    } else {
        return None;
    };
    Some(arr(vec![s(&message), s(severity)]))
}
fn case(scene: &'static str) -> N {
    let h = Host::new(scene);
    let e = h.base.exchange.clone();
    let opts = h.st.borrow().opts.clone();
    let observer = update_order(&h, &e, &opts);
    let promise = h.base.promise(observer);
    h.base.drain();
    let result = h.base.native_result(&promise);
    let syntax = [
        "missing-market-id",
        "order-null",
        "update-null",
        "opts-null",
        "opts-undefined",
    ]
    .contains(&scene);
    let rejected = syntax
        || [
            "cancel-error",
            "place-error",
            "dead-error",
            "log-error",
            "store-error",
        ]
        .contains(&scene);
    assert_eq!(
        result.is_err(),
        rejected,
        "UPDATE native independent rejection {scene}"
    );
    if let Err(error) = &result {
        if syntax {
            assert!(
                eq(&get(error, "name"), &s("TypeError")),
                "UPDATE native TypeError class"
            );
            let text = match scene {
                "opts-null" => {
                    "Cannot destructure property 'order' of 'object null' as it is null."
                }
                "opts-undefined" => {
                    "Cannot destructure property 'order' of 'undefined' as it is undefined."
                }
                "order-null" => "Cannot read properties of null (reading 'symbol')",
                "update-null" => "Cannot use 'in' operator to search for 'price' in null",
                _ => "Cannot read properties of undefined (reading 'includes')",
            };
            assert!(
                eq(&get(error, "message"), &s(text)),
                "UPDATE native exact binding/property diagnostics {scene}"
            )
        } else {
            assert!(
                eq(error, &h.st.borrow().marker),
                "UPDATE native original error handle {scene}"
            )
        }
    }
    let early = syntax || scene == "log-error";
    let cancel = !(early || scene == "store-error");
    let dead = cancel && scene != "cancel-error";
    let place = dead && scene != "dead-error";
    assert_eq!(
        count(&h, "cancelOrders"),
        usize::from(cancel),
        "UPDATE native cancel count {scene}"
    );
    assert_eq!(
        count(&h, "dead-add"),
        usize::from(dead),
        "UPDATE native dead-add count {scene}"
    );
    assert_eq!(
        count(&h, "placeOrder"),
        usize::from(place),
        "UPDATE native place count {scene}"
    );
    if cancel {
        assert!(
            eq(h.st.borrow().cancel_seen.as_ref().unwrap(), &s("old-id")),
            "UPDATE native cancellation sees original ID before await"
        );
        assert!(
            h.st.borrow().cancel_completed || scene == "cancel-error",
            "UPDATE native cancellation fulfillment recorded"
        );
    }
    let dead_keys = {
        let dead = h.st.borrow().dead.clone();
        let N::Set(v) = dead else { unreachable!() };
        v.borrow().clone()
    };
    assert_eq!(
        dead_keys.len(),
        usize::from(place),
        "UPDATE native independent dead Set floor"
    );
    if place {
        assert!(
            eq(
                &dead_keys[0],
                &s(if scene == "fresh-id-after-cancel" {
                    "later-id"
                } else {
                    "old-id"
                })
            ),
            "UPDATE native fresh original ID after cancel await"
        )
    }
    let logs = effects(&h, "log");
    let expected = if syntax { None } else { expected_log(scene) };
    assert_eq!(
        logs.len(),
        usize::from(expected.is_some()),
        "UPDATE native independent log count {scene}"
    );
    if let Some(expected) = expected {
        let parts = values(&expected);
        assert!(
            eq(&get(&logs[0], "1"), &parts[0]),
            "UPDATE native exact log UTF16/text {scene}"
        );
        assert!(
            eq(&get(&logs[0], "2"), &parts[1]),
            "UPDATE native exact log severity {scene}"
        );
    }
    if classification(scene) && !syntax {
        assert_eq!(
            count(&h, "Number"),
            4,
            "UPDATE native Number conversion count"
        );
        let candidate_count = match scene {
            "no-reference" | "missing-ticker" => 0,
            "index-only" => 1,
            "last-fallback" => 2,
            _ => 3,
        };
        let st = h.st.borrow();
        let candidates = values(st.candidates.as_ref().unwrap());
        assert_eq!(
            candidates.len(),
            candidate_count,
            "UPDATE native independent filtered candidates {scene}"
        );
        if candidate_count > 0 {
            assert_eq!(
                num(&candidates[0]),
                100.,
                "UPDATE native first valid reference"
            )
        }
        if let Some(reference) = &st.reference {
            assert_eq!(
                num(reference),
                100.,
                "UPDATE native reference binding value"
            );
            let max = num(st.max.as_ref().unwrap());
            let expected = match scene {
                "divergence-quarter" => 0.25,
                "divergence-over" => (125.01_f64 - 100.) / 100.,
                "wrong-denominator" => 0.3,
                "last-fallback" => 0.1,
                _ => 0.,
            };
            assert_eq!(
                max.to_bits(),
                expected.to_bits(),
                "UPDATE native original-reference divergence denominator {scene}"
            )
        }
    } else {
        assert_eq!(
            count(&h, "Number"),
            0,
            "UPDATE native non-trigger avoids classification conversions"
        )
    }
    if place {
        let st = h.st.borrow();
        let new = st.placed.as_ref().unwrap();
        let expected_fields = if scene == "inherited-trigger-fields" {
            vec![
                "symbol",
                "type",
                "side",
                "price",
                "amount",
                "reduceOnly",
                "timeInForce",
                "tpTriggerType",
                "slTriggerType",
            ]
        } else {
            vec![
                "symbol",
                "type",
                "side",
                "price",
                "amount",
                "reduceOnly",
                "timeInForce",
            ]
        };
        assert_eq!(
            fields(new),
            expected_fields,
            "UPDATE native fresh replacement field order {scene}"
        );
        assert!(
            !eq(new, &st.order),
            "UPDATE native replacement object fresh"
        );
        assert!(
            eq(&get(new, "symbol"), &st.symbol),
            "UPDATE native original symbol units"
        );
        assert!(
            eq(&get(new, "type"), &s(new_kind(scene))),
            "UPDATE native safety direction and chosen replacement type {scene}"
        );
        assert!(eq(&get(new, "side"), &s(side(scene))));
        scalar(
            &get(new, "price"),
            &raw_price(scene),
            &format!("UPDATE native exact placed raw price {scene}"),
        );
        assert_eq!(
            num(&get(new, "amount")),
            match scene {
                "amount-zero" => 0.,
                "inherited-update" => 9.,
                _ => 7.5,
            }
        );
        assert!(
            eq(
                &get(new, "reduceOnly"),
                &if scene == "raw-reduce" {
                    st.reduce.clone()
                } else {
                    N::B(false)
                }
            ),
            "UPDATE native raw truthy reduce identity {scene}"
        );
        assert!(
            eq(
                &get(new, "timeInForce"),
                &s(if scene == "post-only" { "post" } else { "gtc" })
            ),
            "UPDATE native TIF retained"
        );
        if scene == "inherited-trigger-fields" {
            assert!(eq(&get(new, "tpTriggerType"), &s("last")));
            assert!(
                matches!(get(new, "slTriggerType"), N::U),
                "UPDATE native present undefined inherited trigger copied"
            )
        }
    }
    let expected_placeholder = hip(scene) && !early;
    assert_eq!(
        h.st.borrow().placeholders.len(),
        usize::from(expected_placeholder),
        "UPDATE native independent placeholder floor {scene}"
    );
    if expected_placeholder {
        let st = h.st.borrow();
        let p = &st.placeholders[0];
        assert_eq!(
            fields(p),
            [
                "id",
                "status",
                "symbol",
                "type",
                "side",
                "price",
                "amount",
                "filled",
                "remaining",
                "reduceOnly",
                "lastUpdate"
            ],
            "UPDATE native placeholder insertion order"
        );
        for (k, v) in [
            ("id", s("old-id")),
            ("status", s("updating")),
            ("symbol", st.symbol.clone()),
            ("type", s(new_kind(scene))),
            ("side", s(side(scene))),
            (
                "price",
                if ["zero-trigger", "price-undefined"].contains(&scene) {
                    N::Num(110.)
                } else {
                    raw_price(scene)
                },
            ),
            (
                "amount",
                N::Num(if scene == "inherited-update" { 9. } else { 7.5 }),
            ),
            ("filled", N::Num(0.)),
            (
                "remaining",
                N::Num(if scene == "inherited-update" { 9. } else { 7.5 }),
            ),
            ("reduceOnly", N::B(scene == "raw-reduce")),
            ("lastUpdate", N::Num(987654.)),
        ] {
            scalar(
                &get(p, k),
                &v,
                &format!("UPDATE native exact placeholder {scene}/{k}"),
            )
        }
    }
    if !rejected {
        let st = h.st.borrow();
        let returned = result.as_ref().unwrap();
        assert!(
            eq(returned, &st.result),
            "UPDATE native original adopted placement result"
        );
        let ids = values(returned);
        assert_eq!(ids.len(), 2, "UPDATE native independent returned ID floor");
        assert!(eq(&ids[0], &s("new-A")));
        assert!(eq(&ids[1], &s("new-B")))
    }
    let st = h.st.borrow();
    object(vec![
        ("rejected", N::B(rejected)),
        ("effects", arr(st.effects.clone())),
        ("placeholders", arr(st.placeholders.clone())),
        ("placed", st.placed.clone().unwrap_or(N::U)),
        ("cancelSeen", st.cancel_seen.clone().unwrap_or(N::U)),
        ("dead", arr(dead_keys)),
        ("result", result.unwrap_or(N::U)),
    ])
}
#[test]
fn update_native_before_source() {
    let mut actual = vec![];
    let mut failures = vec![];
    let mut completed = 0;
    let mut last = None;
    for scene in SCENES {
        match catch_unwind(AssertUnwindSafe(|| case(scene))) {
            Ok(v) => actual.push((scene, v)),
            Err(_) => failures.push(scene),
        }
        last = Some(completed);
        completed += 1
    }
    if catch_unwind(AssertUnwindSafe(|| {
        let h = Host::new("drop-observer");
        let opts = h.st.borrow().opts.clone();
        drop(update_order(&h, &h.base.exchange, &opts));
        h.base.drain();
        let st = h.st.borrow();
        assert_eq!(
            st.settled.len(),
            1,
            "UPDATE native actual dropped settlement count"
        );
        assert!(
            eq(st.settled[0].as_ref().unwrap(), &st.result),
            "UPDATE native actual dropped original result"
        );
        assert!(
            st.placed.is_some(),
            "UPDATE native dropped body reaches placement"
        )
    }))
    .is_err()
    {
        failures.push("direct/drop-observer")
    }
    assert!(failures.is_empty(), "UPDATE native failures: {failures:?}");
    assert_eq!(completed, 41, "UPDATE independent completed native floor");
    assert_eq!(last, Some(40), "UPDATE native last index");
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order-update-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = v["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 41, "UPDATE independent source floor");
    assert_eq!(v["last_index"], 40);
    for ((scene, actual), row) in actual.iter().zip(rows) {
        assert_eq!(row["scene"], *scene);
        native::assert_value(actual, &row["state"], &format!("UPDATE source {scene}"))
    }
}
