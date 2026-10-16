#[allow(dead_code)]
#[path = "support/spot_account_host.rs"]
mod account;
#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[allow(dead_code)]
#[path = "support/rest_host.rs"]
mod native;
#[path = "support/placement_host.rs"]
mod support;
use account::get;
use cache::{N, arr, eq, num, object, s};
use safe_cex_venue_hyperliquid::placement_dispatch::*;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    process::Command,
};
use support::Host;
const SINGLE: [&str; 24] = [
    "market-buy",
    "market-sell",
    "market-zero",
    "market-empty",
    "market-vault",
    "market-spot",
    "limit",
    "tp",
    "sl",
    "tp-zero",
    "tp-full",
    "tp-invalid",
    "sl-nan",
    "tp-hip3",
    "tp-deferred",
    "tp-next-error",
    "tp-body-error",
    "sign-error",
    "post-error",
    "wrong-type",
    "trailing",
    "missing-market",
    "tp-utf16",
    "drop-observer",
];
const SERIAL: [&str; 9] = [
    "serial-empty",
    "serial-one",
    "serial-two",
    "serial-nested",
    "serial-guard-error",
    "serial-first-error",
    "serial-second-error",
    "serial-replaced",
    "serial-drop",
];
fn values(v: &N) -> Vec<N> {
    let N::A(a) = v else {
        panic!("DISPATCH native array")
    };
    a.borrow().clone()
}
fn fields(v: &N) -> Vec<String> {
    let N::O(o) = v else {
        panic!("DISPATCH native object")
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
fn case(scene: &'static str) -> N {
    let h = Host::new(scene);
    let e = h.base.exchange.clone();
    let serial = scene.starts_with("serial-");
    let opts = if serial {
        h.st.borrow().rows.clone()
    } else {
        h.st.borrow().opts.clone()
    };
    let observer = if serial {
        place_orders(&h, &e, &opts)
    } else {
        place_order(&h, &e, &opts)
    };
    let promise = h.base.promise(observer);
    h.base.drain();
    let result = h.base.native_result(&promise);
    let rejected = [
        "tp-invalid",
        "sl-nan",
        "tp-next-error",
        "tp-body-error",
        "sign-error",
        "post-error",
        "wrong-type",
        "trailing",
        "missing-market",
        "serial-guard-error",
        "serial-first-error",
        "serial-second-error",
    ]
    .contains(&scene);
    assert_eq!(
        result.is_err(),
        rejected,
        "DISPATCH native independent rejection {scene}"
    );
    if let Err(error) = &result {
        if [
            "tp-invalid",
            "sl-nan",
            "wrong-type",
            "trailing",
            "missing-market",
        ]
        .contains(&scene)
        {
            let message = match scene {
                "tp-invalid" => "Cannot place Hyperliquid tp for BTC/USDC: invalid trigger price",
                "sl-nan" => "Cannot place Hyperliquid sl for BTC/USDC: invalid trigger price",
                "wrong-type" => "Unknown order type unknown",
                "trailing" => "Not supported",
                _ => "Market BTC/USDC not found",
            };
            assert!(eq(&get(error, "name"), &s("Error")));
            assert!(
                eq(&get(error, "message"), &s(message)),
                "DISPATCH native exact guard message {scene}"
            )
        } else {
            assert!(
                eq(error, &h.st.borrow().marker),
                "DISPATCH native original error handle {scene}"
            )
        }
    }
    if serial {
        assert!(
            eq(
                &get(&h.st.borrow().effects[0], "0"),
                &s("assertCallerClientOrderIds")
            ),
            "DISPATCH native serial guard first effect"
        );
        assert_eq!(
            count(&h, "assertCallerClientOrderIds"),
            1,
            "DISPATCH native serial guard count"
        );
        assert!(
            eq(
                &get(&effects(&h, "assertCallerClientOrderIds")[0], "1"),
                &opts
            ),
            "DISPATCH native original serial input"
        );
        let calls = match scene {
            "serial-empty" | "serial-guard-error" => 0,
            "serial-one" | "serial-first-error" | "serial-replaced" => 1,
            _ => 2,
        };
        assert_eq!(
            count(&h, "placeOrder"),
            calls,
            "DISPATCH native serial callback count {scene}"
        );
        let rows = values(&opts);
        for (i, call) in effects(&h, "placeOrder").iter().enumerate() {
            assert!(
                eq(&get(call, "1"), &rows[i]),
                "DISPATCH native callback row alias {scene}/{i}"
            )
        }
        assert_eq!(
            count(&h, "resolveCallerClientOrderId"),
            calls,
            "DISPATCH native composed dispatcher count {scene}"
        );
        assert_eq!(
            count(&h, "replacement-place"),
            usize::from(scene == "serial-replaced"),
            "DISPATCH native fresh serial method lookup"
        );
        if scene == "serial-replaced" {
            let replacement = effects(&h, "replacement-place");
            assert!(
                eq(&get(&replacement[0], "1"), &rows[1]),
                "DISPATCH native replacement original second row"
            );
        }
        if !rejected {
            let actual = values(result.as_ref().unwrap());
            let expected = match scene {
                "serial-empty" => vec![],
                "serial-one" => vec![s("A1"), s("A2")],
                "serial-nested" => vec![s("A"), arr(vec![s("nested")])],
                "serial-replaced" => vec![s("A1"), s("A2"), s("replacement")],
                _ => vec![s("A1"), s("A2"), s("B1"), s("B2")],
            };
            assert_eq!(
                actual.len(),
                expected.len(),
                "DISPATCH native independent flattened floor {scene}"
            );
            for (i, (a, b)) in actual.iter().zip(expected.iter()).enumerate() {
                if matches!(b, N::A(_)) {
                    let av = values(a);
                    assert_eq!(av.len(), 1);
                    assert!(
                        eq(&av[0], &s("nested")),
                        "DISPATCH native one-level flat nested value"
                    );
                    assert!(
                        eq(a, &values(&get(&rows[0], "ids"))[1]),
                        "DISPATCH native nested alias"
                    )
                } else {
                    assert!(eq(a, b), "DISPATCH native serial ID order {scene}/{i}")
                }
            }
        }
        assert!(
            h.st.borrow().requests.is_empty(),
            "DISPATCH native serial limit-only composition has no wire helper"
        );
    } else {
        assert!(
            eq(
                &get(&h.st.borrow().effects[0], "0"),
                &s("resolveCallerClientOrderId")
            ),
            "DISPATCH native caller resolution first effect"
        );
        assert_eq!(
            count(&h, "resolveCallerClientOrderId"),
            1,
            "DISPATCH native caller ID resolution first"
        );
        assert!(
            eq(
                &get(&effects(&h, "resolveCallerClientOrderId")[0], "1"),
                &opts
            ),
            "DISPATCH native opts alias"
        );
        let guarded = ["wrong-type", "trailing", "missing-market"].contains(&scene);
        let invalid = ["tp-invalid", "sl-nan"].contains(&scene);
        let trigger =
            scene.starts_with("tp") || scene.starts_with("sl") || scene == "drop-observer";
        if scene == "limit" {
            assert_eq!(
                count(&h, "placeLimitOrder"),
                1,
                "DISPATCH native limit delegate count"
            );
            assert!(
                eq(result.as_ref().unwrap(), &h.st.borrow().ids),
                "DISPATCH native limit adopted original IDs"
            )
        }
        if !guarded && scene != "limit" {
            let st = h.st.borrow();
            assert_eq!(
                st.requests.len(),
                usize::from(!invalid),
                "DISPATCH native wire floor {scene}"
            );
            drop(st);
            assert_eq!(count(&h, "getOrderBasePrice"), 1);
            let base = &effects(&h, "getOrderBasePrice")[0];
            assert!(
                eq(&get(base, "1"), &h.st.borrow().market),
                "DISPATCH native base market alias"
            );
            assert!(
                eq(
                    &get(base, "2"),
                    &s(if trigger {
                        if scene.starts_with("sl") {
                            "sl order"
                        } else {
                            "tp order"
                        }
                    } else {
                        "market order"
                    })
                ),
                "DISPATCH native base context"
            );
            if !invalid {
                let st = h.st.borrow();
                let request = &st.requests[0];
                assert_eq!(
                    fields(request),
                    [
                        "coin",
                        "is_buy",
                        "sz",
                        "limit_px",
                        "order_type",
                        "reduce_only",
                        "cloid"
                    ],
                    "DISPATCH native request insertion order"
                );
                assert!(
                    eq(&get(request, "cloid"), &st.marker),
                    "DISPATCH native caller client order ID on the main wire"
                );
                assert!(eq(&get(request, "coin"), &s("coin")));
                assert!(
                    eq(&get(request, "is_buy"), &N::B(scene != "market-sell")),
                    "DISPATCH native direction"
                );
                assert_eq!(
                    num(&get(request, "sz")),
                    if ["tp-zero", "tp-full"].contains(&scene) {
                        0.
                    } else {
                        7.125
                    },
                    "DISPATCH native size vs full position"
                );
                assert_eq!(
                    num(&get(request, "limit_px")).to_bits(),
                    (if scene == "market-sell" {
                        190_f64
                    } else {
                        210_f64
                    })
                    .to_bits(),
                    "DISPATCH native marketable band bits"
                );
                assert!(
                    eq(&get(request, "reduce_only"), &N::B(scene == "tp-full")),
                    "DISPATCH native spot/perp reduce rule"
                );
                let kind = get(request, "order_type");
                if trigger {
                    let t = get(&kind, "trigger");
                    assert!(eq(&get(&t, "isMarket"), &N::B(true)));
                    assert!(
                        eq(
                            &get(&t, "tpsl"),
                            &s(if scene.starts_with("sl") { "sl" } else { "tp" })
                        ),
                        "DISPATCH native trigger kind"
                    );
                    assert_eq!(
                        num(&get(&t, "triggerPx")),
                        101.25,
                        "DISPATCH native original trigger price"
                    )
                } else {
                    assert!(
                        eq(&get(&get(&kind, "limit"), "tif"), &s("Gtc")),
                        "DISPATCH native market Gtc remainder"
                    )
                }
                assert_eq!(st.actions.len(), 1);
                let action = &st.actions[0];
                assert!(
                    eq(
                        &get(action, "kind"),
                        &s(if ["tp-zero", "tp-full"].contains(&scene) {
                            "orderWiresToPositionalTPSLAction"
                        } else {
                            "orderWiresToOrderAction"
                        })
                    ),
                    "DISPATCH native positional grouping"
                );
                assert!(eq(&get(action, "builder"), &st.builder));
                let orders = values(&get(action, "orders"));
                assert_eq!(orders.len(), 1, "DISPATCH native action wire floor");
                assert!(eq(&orders[0], &st.wires[0]));
                assert!(
                    eq(&get(&st.wires[0], "request"), request),
                    "DISPATCH native original request"
                );
                assert_eq!(num(&get(&st.wires[0], "assetIndex")), 73.);
                assert_eq!(st.sign_args.len(), 5);
                assert!(eq(&st.sign_args[0], &st.wallet));
                assert!(eq(&st.sign_args[1], action));
                assert!(eq(
                    &st.sign_args[2],
                    &if scene == "market-vault" {
                        s("public-fixture-vault")
                    } else {
                        N::Null
                    }
                ));
                assert_eq!(num(&st.sign_args[3]), 123456.);
                assert!(
                    eq(&st.sign_args[4], &N::B(scene != "market-zero")),
                    "DISPATCH native mainnet vs testnet"
                );
                if scene != "sign-error" {
                    assert_eq!(st.payloads.len(), 1);
                    let p = &st.payloads[0];
                    assert!(eq(&get(p, "action"), action));
                    assert!(eq(&get(p, "signature"), &st.signature));
                    assert_eq!(num(&get(p, "nonce")), 123456.);
                    assert_eq!(
                        fields(p),
                        if scene == "market-vault" {
                            vec!["action", "nonce", "signature", "vaultAddress"]
                        } else {
                            vec!["action", "nonce", "signature"]
                        }
                    );
                    if scene == "market-vault" {
                        assert!(eq(&get(p, "vaultAddress"), &s("public-fixture-vault")))
                    }
                }
                drop(st);
                let adjust = effects(&h, "adjust");
                assert_eq!(
                    adjust.len(),
                    usize::from(scene != "tp-zero"),
                    "DISPATCH native zero-size bypass adjust"
                );
                if let Some(a) = adjust.first() {
                    assert_eq!(num(&get(a, "1")), 7.5);
                    assert_eq!(num(&get(a, "2")), 0.01)
                }
                let symbol_calls = effects(&h, "symbolToCoin");
                assert!(
                    eq(
                        &get(&symbol_calls[0], "1"),
                        &get(&h.st.borrow().market, "symbol")
                    ),
                    "DISPATCH native helper symbol UTF16"
                );
            }
        }
        if !rejected && scene != "limit" {
            assert!(
                eq(result.as_ref().unwrap(), &h.st.borrow().ids),
                "DISPATCH native returned original IDs"
            );
            let market = !trigger;
            let empty = ["market-empty", "tp-deferred"].contains(&scene);
            assert_eq!(
                count(&h, "rememberLimitOrderTimeInForce"),
                usize::from(market && !empty),
                "DISPATCH native market-only TIF"
            );
            if market && !empty {
                let call = &effects(&h, "rememberLimitOrderTimeInForce")[0];
                assert!(eq(&get(call, "1"), &s("id-A")));
                assert!(eq(&get(call, "2"), &s("gtc")))
            }
            assert_eq!(
                count(&h, "scheduleOrderRefresh"),
                usize::from(scene == "tp-deferred"),
                "DISPATCH native deferred-only refresh"
            );
            assert_eq!(
                count(&h, "responseHasDeferredOrderStatus"),
                usize::from(scene == "tp-deferred"),
                "DISPATCH native deferred short circuit"
            );
            let hip = ["tp-hip3", "tp-deferred", "drop-observer"].contains(&scene);
            // HIP-3 stops, and core-perp stop ENTRIES (not reduce-only), are pre-cached.
            let reduce_only = scene == "tp-full";
            let trigger = scene.starts_with("tp") || scene.starts_with("sl") || scene == "drop-observer";
            assert_eq!(
                count(&h, "addToPrePlacementCache"),
                usize::from(trigger && (hip || !reduce_only)),
                "DISPATCH native precache count"
            );
            // Market and TP/SL wires are recorded once, after signing and before the POST.
            let signed = !["limit", "sign-error", "tp-invalid", "sl-nan", "wrong-type", "trailing", "missing-market"]
                .contains(&scene);
            assert_eq!(
                count(&h, "recordPlacement"),
                usize::from(signed),
                "DISPATCH native one placement record per signed wire"
            );
            assert_eq!(
                count(&h, "refreshHip3OrdersWithRetry"),
                usize::from(hip),
                "DISPATCH native retry count"
            );
            let st = h.st.borrow();
            let expected = if hip && !empty { 2 } else { 0 };
            assert_eq!(
                st.orders.len(),
                expected,
                "DISPATCH native independent optimistic floor"
            );
            for (i, o) in st.orders.iter().enumerate() {
                assert_eq!(
                    fields(o),
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
                    ]
                );
                for (k, v) in [
                    ("id", if i == 0 { s("id-A") } else { s("id-B") }),
                    ("status", s("creating")),
                    ("symbol", s("BTC/USDC")),
                    ("type", s("tp")),
                    ("side", s("buy")),
                    ("price", N::Num(101.25)),
                    ("amount", N::Num(7.5)),
                    ("filled", N::Num(0.)),
                    ("remaining", N::Num(7.5)),
                    ("reduceOnly", N::B(false)),
                    ("lastUpdate", N::Num(987654.)),
                ] {
                    assert!(
                        eq(&get(o, k), &v),
                        "DISPATCH native optimistic field {scene}/{i}/{k}"
                    )
                }
            }
        }
        if [
            "tp-hip3",
            "tp-deferred",
            "tp-next-error",
            "tp-body-error",
            "drop-observer",
        ]
        .contains(&scene)
        {
            assert_eq!(
                count(&h, "addToPrePlacementCache"),
                1,
                "DISPATCH native independent precache prefix"
            );
            let effects = h.st.borrow().effects.clone();
            let before = effects
                .iter()
                .position(|v| eq(&get(v, "0"), &s("addToPrePlacementCache")))
                .expect("DISPATCH native precache exists");
            let after = effects
                .iter()
                .position(|v| eq(&get(v, "0"), &s("postOrderAction")))
                .expect("DISPATCH native post exists");
            assert!(before < after, "DISPATCH native precache before REST");
        }
        assert_eq!(
            count(&h, "close"),
            usize::from(scene == "tp-body-error"),
            "DISPATCH native close only body throw"
        );
    }
    let st = h.st.borrow();
    object(vec![
        ("rejected", N::B(rejected)),
        ("result", result.unwrap_or(N::U)),
        ("requests", arr(st.requests.clone())),
        ("actions", arr(st.actions.clone())),
        ("payloads", arr(st.payloads.clone())),
        ("orders", arr(st.orders.clone())),
        ("effects", arr(st.effects.clone())),
    ])
}
#[test]
fn dispatch_native_before_source() {
    let mut actual = vec![];
    let mut failures = vec![];
    let mut completed = 0;
    let mut last = None;
    for scene in SINGLE.iter().chain(SERIAL.iter()) {
        match catch_unwind(AssertUnwindSafe(|| case(scene))) {
            Ok(v) => actual.push((*scene, v)),
            Err(_) => failures.push(*scene),
        }
        last = Some(completed);
        completed += 1
    }
    for scene in ["drop-observer", "serial-drop"] {
        if catch_unwind(AssertUnwindSafe(|| {
            let h = Host::new(scene);
            let e = h.base.exchange.clone();
            let serial = scene == "serial-drop";
            let opts = if serial {
                h.st.borrow().rows.clone()
            } else {
                h.st.borrow().opts.clone()
            };
            if serial {
                drop(place_orders(&h, &e, &opts))
            } else {
                drop(place_order(&h, &e, &opts))
            }
            h.base.drain();
            let st = h.st.borrow();
            assert_eq!(
                st.settled.len(),
                // placeOrder settles its own promise and #placeOrderWithCloid's.
                if serial { 5 } else { 2 },
                "DISPATCH native actual dropped observer settlements {scene}"
            );
            let r = st.settled.last().unwrap().as_ref().unwrap();
            if serial {
                let rows = values(r);
                assert_eq!(
                    rows.len(),
                    4,
                    "DISPATCH native dropped serial independent flattened floor"
                );
                for (i, want) in ["A1", "A2", "B1", "B2"].iter().enumerate() {
                    assert!(
                        eq(&rows[i], &s(want)),
                        "DISPATCH native dropped serial ID order"
                    )
                }
            } else {
                assert!(
                    eq(r, &st.ids),
                    "DISPATCH native actual dropped original IDs"
                );
                assert_eq!(st.orders.len(), 2, "DISPATCH native dropped body effects")
            }
        }))
        .is_err()
        {
            failures.push(scene)
        }
    }
    assert!(
        failures.is_empty(),
        "DISPATCH native failures: {failures:?}"
    );
    assert_eq!(completed, 33, "DISPATCH independent completed native floor");
    assert_eq!(last, Some(32), "DISPATCH native last index");
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/placement-dispatch-oracle.cjs"
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
    assert_eq!(rows.len(), 33, "DISPATCH independent source floor");
    assert_eq!(v["last_index"], 32);
    for ((scene, actual), row) in actual.iter().zip(rows) {
        assert_eq!(row["scene"], *scene);
        native::assert_value(actual, &row["state"], &format!("DISPATCH source {scene}"))
    }
}
