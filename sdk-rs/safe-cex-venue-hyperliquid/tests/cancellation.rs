#[allow(dead_code)]
#[path = "support/spot_account_host.rs"]
mod account;
#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[allow(dead_code)]
#[path = "support/rest_host.rs"]
mod native;
#[path = "support/cancel_host.rs"]
mod support;
use account::get;
use cache::{N, arr, eq, num, object, s};
use safe_cex_venue_hyperliquid::cancellation::*;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    process::Command,
};
use support::{Host, hip};
const SCENES: [&str; 32] = [
    "plain",
    "empty",
    "hip3",
    "opposite-side",
    "missing-market",
    "missing-info",
    "null-asset",
    "invalid-id",
    "negative-zero-id",
    "infinity-id",
    "utf16-symbol",
    "vault",
    "testnet",
    "terminal-error",
    "terminal-string",
    "terminal-object",
    "post-denied",
    "object-denied",
    "post-false",
    "sign-error",
    "suppress-error",
    "clear-error",
    "next-suppress-error",
    "next-clear-error",
    "set-error",
    "remove-error",
    "wire-map-error",
    "remove-map-error",
    "fresh-store",
    "side-reread",
    "remove-callee",
    "drop-observer",
];
fn values(v: &N) -> Vec<N> {
    let N::A(a) = v else {
        panic!("CANCEL native array")
    };
    a.borrow().clone()
}
fn fields(v: &N) -> Vec<String> {
    let N::O(o) = v else {
        panic!("CANCEL native object")
    };
    o.borrow().fields.iter().map(|x| x.0.clone()).collect()
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
fn first_symbol(scene: &str) -> N {
    if scene == "utf16-symbol" {
        N::S(safe_cex_venue_hyperliquid::spot_json::JsString(vec![
            0xd800, 83,
        ]))
    } else {
        s("BTC/USDC")
    }
}
fn key(coin: N, side: &str) -> N {
    let N::S(t) = coin else { panic!("coin") };
    let mut t = t.0;
    t.push(124);
    t.extend(side.encode_utf16());
    N::S(safe_cex_venue_hyperliquid::spot_json::JsString(t))
}
fn case(scene: &'static str) -> N {
    let h = Host::new(scene);
    let e = h.base.exchange.clone();
    let rows = h.st.borrow().rows.clone();
    let observer = cancel_orders(&h, &e, &rows);
    let p = h.base.promise(observer);
    h.base.drain();
    let result = h.base.native_result(&p);
    let rejected = [
        "missing-info",
        "post-denied",
        "object-denied",
        "post-false",
        "sign-error",
        "suppress-error",
        "clear-error",
        "next-suppress-error",
        "next-clear-error",
        "set-error",
        "remove-error",
        "wire-map-error",
        "remove-map-error",
    ]
    .contains(&scene);
    assert_eq!(
        result.is_err(),
        rejected,
        "CANCEL native independent rejection {scene}"
    );
    if let Err(error) = &result {
        if scene == "missing-info" {
            assert!(
                eq(&get(error, "name"), &s("TypeError")),
                "CANCEL native missing info TypeError"
            );
            assert!(
                eq(
                    &get(error, "message"),
                    &s("Cannot read properties of undefined (reading 'hyperliquidId')")
                ),
                "CANCEL native missing-info optional-chain span"
            )
        } else {
            assert!(
                eq(error, &h.st.borrow().marker),
                "CANCEL native original rejection handle {scene}"
            )
        }
    }
    let wire_error = ["missing-info", "wire-map-error"].contains(&scene);
    let before_cleanup =
        wire_error || ["sign-error", "post-denied", "object-denied", "post-false"].contains(&scene);
    let early_loop = ["suppress-error", "next-suppress-error"].contains(&scene);
    let before_remove = before_cleanup
        || early_loop
        || ["clear-error", "next-clear-error", "set-error"].contains(&scene);
    let input_count = if scene == "empty" { 0 } else { 3 };
    if !wire_error {
        let st = h.st.borrow();
        assert_eq!(st.sign_args.len(), 5, "CANCEL native signing arity");
        assert!(
            eq(&st.sign_args[0], &st.wallet),
            "CANCEL native wallet alias"
        );
        let action = &st.sign_args[1];
        assert_eq!(
            fields(action),
            ["type", "cancels"],
            "CANCEL native action insertion order"
        );
        assert!(eq(&get(action, "type"), &s("cancel")));
        let cancels = get(action, "cancels");
        assert!(
            eq(&cancels, &st.mapped[0]),
            "CANCEL native original wire map result"
        );
        let cancels = values(&cancels);
        assert_eq!(
            cancels.len(),
            input_count,
            "CANCEL native independent cancel count"
        );
        for (i, c) in cancels.iter().enumerate() {
            assert_eq!(fields(c), ["a", "o"]);
            let a = num(&get(c, "a"));
            if scene == "missing-market" && i != 1 {
                assert!(a.is_nan(), "CANCEL native missing market asset NaN")
            } else {
                assert_eq!(
                    a,
                    if i == 1 {
                        91.
                    } else if scene == "null-asset" {
                        0.
                    } else {
                        73.
                    },
                    "CANCEL native asset value {scene}/{i}"
                )
            };
            let o = num(&get(c, "o"));
            if scene == "invalid-id" && i == 0 {
                assert!(o.is_nan(), "CANCEL native raw invalid ID NaN")
            } else {
                let expected = if i == 0 {
                    match scene {
                        "negative-zero-id" => -0.,
                        "infinity-id" => f64::INFINITY,
                        _ => 101.,
                    }
                } else if i == 1 {
                    202.
                } else {
                    303.
                };
                assert_eq!(
                    o.to_bits(),
                    expected.to_bits(),
                    "CANCEL native original ID bits {scene}/{i}"
                )
            }
        }
        assert!(eq(
            &st.sign_args[2],
            &if scene == "vault" {
                s("public-fixture-vault")
            } else {
                N::Null
            }
        ));
        assert_eq!(num(&st.sign_args[3]), 123456.);
        assert!(
            eq(&st.sign_args[4], &N::B(scene != "testnet")),
            "CANCEL native mainnet/testnet choice"
        );
        if scene != "sign-error" {
            assert_eq!(st.posted_args.len(), 3, "CANCEL native POST arity");
            assert!(eq(&st.posted_args[0], &s("/exchange")));
            let payload = &st.posted_args[1];
            assert!(
                eq(&get(payload, "action"), action),
                "CANCEL native payload action alias"
            );
            assert_eq!(num(&get(payload, "nonce")), 123456.);
            assert!(
                eq(&get(payload, "signature"), &st.signature),
                "CANCEL native original signature"
            );
            assert_eq!(
                fields(payload),
                if scene == "vault" {
                    vec!["action", "nonce", "signature", "vaultAddress"]
                } else {
                    vec!["action", "nonce", "signature"]
                }
            );
            if scene == "vault" {
                assert!(eq(
                    &get(payload, "vaultAddress"),
                    &s("public-fixture-vault")
                ))
            }
            let options = &st.posted_args[2];
            assert_eq!(fields(options), ["headers"]);
            assert!(
                eq(
                    &get(&get(options, "headers"), "Content-Type"),
                    &s("application/json")
                ),
                "CANCEL native POST content type"
            );
        }
    }
    let caught_post = [
        "terminal-error",
        "terminal-string",
        "terminal-object",
        "post-denied",
        "object-denied",
        "post-false",
    ]
    .contains(&scene);
    assert_eq!(
        count(&h, "instanceof"),
        usize::from(caught_post),
        "CANCEL native POST catch boundary"
    );
    assert_eq!(
        count(&h, "String"),
        usize::from(
            [
                "terminal-string",
                "terminal-object",
                "object-denied",
                "post-false"
            ]
            .contains(&scene)
        ),
        "CANCEL native Error.message vs String branch"
    );
    assert_eq!(
        count(&h, "isOnlyHyperliquidTerminalCancelError"),
        usize::from(caught_post),
        "CANCEL native terminal predicate count"
    );
    let suppressed = if before_cleanup {
        0
    } else if scene == "suppress-error" {
        2
    } else if scene == "next-suppress-error" {
        1
    } else {
        input_count
    };
    let suppression = effects(&h, "suppressCancelledOrder");
    assert_eq!(
        suppression.len(),
        suppressed,
        "CANCEL native suppression floor {scene}"
    );
    let original = values(&rows);
    for (i, call) in suppression.iter().enumerate() {
        assert!(
            eq(&get(call, "1"), &get(&original[i], "id")),
            "CANCEL native suppressed original raw ID {scene}/{i}"
        )
    }
    assert_eq!(
        h.st.borrow().set_calls,
        usize::from(!before_cleanup && !early_loop),
        "CANCEL native Set constructor phase"
    );
    let unique = if scene == "empty" || !hip(scene) {
        0
    } else if ["fresh-store", "clear-error", "next-clear-error"].contains(&scene) {
        1
    } else if scene == "opposite-side" {
        3
    } else {
        2
    };
    let clear_count = if before_cleanup || early_loop || scene == "set-error" {
        0
    } else {
        unique
    };
    let clears = effects(&h, "clearPrePlacement");
    assert_eq!(
        clears.len(),
        clear_count,
        "CANCEL native independent dedup clear floor {scene}"
    );
    for (i, call) in clears.iter().enumerate() {
        let coin = if i == 1 {
            s("ETH/USDC")
        } else {
            first_symbol(scene)
        };
        let side = if i == 1 || i == 2 || scene == "side-reread" && i == 0 {
            "sell"
        } else {
            "buy"
        };
        assert!(
            eq(&get(call, "1"), &coin),
            "CANCEL native clear original order symbol units {scene}/{i}"
        );
        assert!(
            eq(&get(call, "2"), &s(side)),
            "CANCEL native clear fresh side {scene}/{i}"
        )
    }
    if !before_cleanup && !early_loop && scene != "set-error" {
        let st = h.st.borrow();
        let N::Set(set) = st.cleared.as_ref().unwrap() else {
            panic!("CANCEL native Set")
        };
        let expected = if unique == 0 {
            vec![]
        } else {
            let mut keys = vec![key(first_symbol(scene), "buy")];
            if unique > 1 {
                keys.push(key(s("ETH/USDC"), "sell"))
            }
            if unique > 2 {
                keys.push(key(first_symbol(scene), "sell"))
            }
            keys
        };
        assert_eq!(
            set.borrow().len(),
            unique,
            "CANCEL native independent Set contents count"
        );
        for (i, k) in expected.iter().enumerate() {
            assert!(
                eq(&set.borrow()[i], k),
                "CANCEL native Set raw key/order {scene}/{i}"
            )
        }
    }
    assert_eq!(
        h.st.borrow().side_reads,
        if scene == "side-reread" { 2 } else { 0 },
        "CANCEL native repeated side getter"
    );
    let remove_attempt = !before_remove;
    let removed = remove_attempt && scene != "remove-map-error";
    assert_eq!(
        count(&h, "removeOrders"),
        usize::from(removed),
        "CANCEL native remove call floor"
    );
    assert_eq!(
        h.st.borrow().map_calls,
        if remove_attempt { 2 } else { 1 },
        "CANCEL native independent map phase"
    );
    if removed {
        let st = h.st.borrow();
        assert!(
            eq(
                st.remove_receiver.as_ref().unwrap(),
                &if scene == "fresh-store" {
                    st.fresh_store.clone()
                } else {
                    st.original_store.clone()
                }
            ),
            "CANCEL native removal callee captured before argument map"
        );
        let ids = st.removed.as_ref().unwrap();
        assert!(
            eq(ids, &st.mapped[1]),
            "CANCEL native original removal map output"
        );
        let ids = values(ids);
        assert_eq!(
            ids.len(),
            input_count,
            "CANCEL native independent removed ID count"
        );
        for (i, v) in ids.iter().enumerate() {
            assert_eq!(fields(v), ["id"]);
            assert!(
                eq(&get(v, "id"), &get(&original[i], "id")),
                "CANCEL native removal raw ID sequence"
            );
            assert!(!eq(v, &original[i]), "CANCEL native removal object fresh")
        }
    }
    let closed = effects(&h, "close");
    let close_count = usize::from(["suppress-error", "clear-error"].contains(&scene));
    assert_eq!(
        closed.len(),
        close_count,
        "CANCEL native body-only close count"
    );
    if let Some(c) = closed.first() {
        assert_eq!(
            num(&get(c, "1")),
            if scene == "suppress-error" { 0. } else { 1. },
            "CANCEL native correct iterator closed"
        )
    }
    if !rejected {
        assert!(
            matches!(result, Ok(N::U)),
            "CANCEL native source void discards opaque returns"
        )
    }
    let st = h.st.borrow();
    let keys = if let Some(N::Set(v)) = &st.cleared {
        v.borrow().clone()
    } else {
        vec![]
    };
    object(vec![
        ("rejected", N::B(rejected)),
        ("effects", arr(st.effects.clone())),
        ("removed", st.removed.clone().unwrap_or(N::U)),
        ("keys", arr(keys)),
        ("mapCalls", N::Num(st.map_calls as f64)),
        ("setCalls", N::Num(st.set_calls as f64)),
        ("sideReads", N::Num(st.side_reads as f64)),
    ])
}
#[test]
fn cancellation_native_before_source() {
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
        let rows = h.st.borrow().rows.clone();
        drop(cancel_orders(&h, &h.base.exchange, &rows));
        h.base.drain();
        let st = h.st.borrow();
        assert_eq!(
            st.settled.len(),
            1,
            "CANCEL native actual dropped settlement count"
        );
        assert!(
            matches!(st.settled[0], Ok(N::U)),
            "CANCEL native actual dropped void settlement"
        );
        assert!(
            st.removed.is_some(),
            "CANCEL native dropped body reaches removal"
        );
        assert_eq!(
            values(st.removed.as_ref().unwrap()).len(),
            3,
            "CANCEL native dropped independent removal floor"
        )
    }))
    .is_err()
    {
        failures.push("direct/drop-observer")
    }
    assert!(failures.is_empty(), "CANCEL native failures: {failures:?}");
    assert_eq!(completed, 32, "CANCEL independent completed native floor");
    assert_eq!(last, Some(31), "CANCEL native last index");
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/cancellation-oracle.cjs"
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
    assert_eq!(rows.len(), 32, "CANCEL independent source floor");
    assert_eq!(v["last_index"], 31);
    for ((scene, actual), row) in actual.iter().zip(rows) {
        assert_eq!(row["scene"], *scene);
        native::assert_value(actual, &row["state"], &format!("CANCEL source {scene}"))
    }
}
