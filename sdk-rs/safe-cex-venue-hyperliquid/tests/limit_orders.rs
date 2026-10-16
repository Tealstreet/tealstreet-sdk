#[allow(dead_code)]
#[path = "support/spot_account_host.rs"]
mod account;
#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[allow(dead_code)]
#[path = "support/rest_host.rs"]
mod native;
#[path = "support/limit_order_host.rs"]
mod support;
use account::get;
use cache::{N, arr, eq, num, object, s};
use safe_cex_venue_hyperliquid::limit_orders::*;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    process::Command,
};
use support::Host;
const WIRES: [&str; 16] = [
    "plain",
    "sell",
    "post-only",
    "both",
    "sl",
    "tp",
    "spot-both",
    "reduce",
    "spot-reduce",
    "zero",
    "negative-zero",
    "nan",
    "infinity",
    "utf16",
    "missing-market",
    "wrong-type",
];
const PLACE: [&str; 19] = [
    "plain",
    "hip3",
    "post-only",
    "both",
    "vault",
    "testnet",
    "empty",
    "falsy-first",
    "builder-error",
    "sign-error",
    "post-error",
    "validate-error",
    "binding-error",
    "pending-error",
    "next-error",
    "drop-observer",
    "fresh-after-builder",
    "fresh-market",
    "wrong-type",
];
fn values(v: &N) -> Vec<N> {
    let N::A(a) = v else {
        panic!("LIMIT native array")
    };
    a.borrow().clone()
}
fn fields(v: &N) -> Vec<String> {
    let N::O(o) = v else {
        panic!("LIMIT native object")
    };
    o.borrow().fields.iter().map(|x| x.0.clone()).collect()
}
fn names(h: &Host) -> Vec<String> {
    h.st.borrow()
        .effects
        .iter()
        .map(|v| {
            let N::S(t) = get(v, "0") else {
                panic!("effect")
            };
            String::from_utf16_lossy(&t.0)
        })
        .collect()
}
fn count(h: &Host, k: &str) -> usize {
    names(h).iter().filter(|x| x.as_str() == k).count()
}
fn check_requests(h: &Host, scene: &str) {
    let st = h.st.borrow();
    let adjust = st
        .effects
        .iter()
        .find(|v| eq(&get(v, "0"), &s("adjust")))
        .unwrap();
    assert_eq!(values(adjust).len(), 3, "LIMIT native adjust arity");
    assert_eq!(
        num(&get(adjust, "1")),
        7.5,
        "LIMIT native original amount input"
    );
    assert_eq!(
        num(&get(adjust, "2")),
        0.01,
        "LIMIT native original precision input"
    );
    let coin_call = st
        .effects
        .iter()
        .find(|v| eq(&get(v, "0"), &s("symbolToCoin")))
        .unwrap();
    assert!(
        eq(&get(coin_call, "1"), &get(&st.market, "symbol")),
        "LIMIT native original symbol units"
    );
    let expected = if ["both", "spot-both"].contains(&scene) {
        3
    } else if ["sl", "tp"].contains(&scene) {
        2
    } else {
        1
    };
    assert_eq!(
        st.requests.len(),
        expected,
        "LIMIT native independent request floor {scene}"
    );
    assert_eq!(
        st.wires.len(),
        expected,
        "LIMIT native independent wire floor {scene}"
    );
    for (i, r) in st.requests.iter().enumerate() {
        assert_eq!(
            fields(r),
            [
                "coin",
                "is_buy",
                "sz",
                "limit_px",
                "order_type",
                "reduce_only",
                "cloid"
            ],
            "LIMIT native request field order {scene}/{i}"
        );
        // The entry wire carries the default-minted placement cloid; each leg mints after it.
        assert!(
            eq(&get(r, "cloid"), &s(&format!("0xmint{}", i + 1))),
            "LIMIT native cloid {scene}/{i}"
        );
        assert!(
            eq(&get(&st.wires[i], "request"), r),
            "LIMIT native original request handle"
        );
        assert_eq!(
            num(&get(&st.wires[i], "assetIndex")),
            73.,
            "LIMIT native asset"
        );
        let coin = if scene == "utf16" {
            N::S(safe_cex_venue_hyperliquid::spot_json::JsString(vec![
                0xdfff, 67,
            ]))
        } else {
            s("coin")
        };
        assert!(eq(&get(r, "coin"), &coin), "LIMIT native raw coin units");
        assert_eq!(num(&get(r, "sz")), 7.125, "LIMIT native adjusted size");
        assert!(
            eq(
                &get(r, "is_buy"),
                &N::B(if i == 0 {
                    scene != "sell"
                } else {
                    scene == "sell"
                })
            ),
            "LIMIT native main/opposite trigger side"
        );
        let price = match scene {
            "zero" | "negative-zero" | "nan" => 0.,
            "infinity" => f64::INFINITY,
            _ => 101.25_f64,
        };
        assert_eq!(
            num(&get(r, "limit_px")).to_bits(),
            price.to_bits(),
            "LIMIT native price bits {scene}/{i}"
        );
        let reduce = !scene.starts_with("spot") && (i > 0 || scene == "reduce");
        assert!(
            eq(&get(r, "reduce_only"), &N::B(reduce)),
            "LIMIT native reduce rule {scene}/{i}"
        );
        let kind = get(r, "order_type");
        if i == 0 {
            assert_eq!(fields(&kind), ["limit"]);
            assert!(
                eq(
                    &get(&get(&kind, "limit"), "tif"),
                    &s(if scene == "post-only" { "Alo" } else { "Gtc" })
                ),
                "LIMIT native wire TIF"
            )
        } else {
            assert_eq!(fields(&kind), ["trigger"]);
            let t = get(&kind, "trigger");
            let sl = scene != "tp" && i == 1;
            assert!(
                eq(&get(&t, "tpsl"), &s(if sl { "sl" } else { "tp" })),
                "LIMIT native trigger order"
            );
            assert_eq!(
                num(&get(&t, "triggerPx")),
                if sl { 90.5 } else { 120.75 },
                "LIMIT native trigger value"
            );
            assert!(eq(&get(&t, "isMarket"), &N::B(true)));
        }
    }
}
fn case(family: &str, scene: &'static str) -> N {
    let mut h = Host::new(scene);
    let e = h.base.exchange.clone();
    let opts = h.st.borrow().opts.clone();
    let result = if family == "wires" {
        create_limit_order_wires(&mut h, &e, &opts, &N::U)
    } else {
        let observer = place_limit_order(&h, &e, &opts, &N::U);
        let promise = h.base.promise(observer);
        h.base.drain();
        h.base.native_result(&promise)
    };
    let errors = [
        "missing-market",
        "wrong-type",
        "builder-error",
        "sign-error",
        "post-error",
        "validate-error",
        "binding-error",
        "pending-error",
        "next-error",
    ];
    let rejected = errors.contains(&scene);
    assert_eq!(
        result.is_err(),
        rejected,
        "LIMIT native rejection {family}/{scene}"
    );
    if let Err(error) = &result {
        if scene == "wrong-type" || scene == "missing-market" {
            assert!(
                eq(&get(error, "name"), &s("Error")),
                "LIMIT native error name"
            );
            assert!(
                eq(
                    &get(error, "message"),
                    &s(if scene == "wrong-type" {
                        "Not supported"
                    } else {
                        "Market BTC/USDC not found"
                    })
                ),
                "LIMIT native exact guard message"
            )
        } else {
            assert!(
                eq(error, &h.st.borrow().marker),
                "LIMIT native original error identity {scene}"
            )
        }
    }
    if family == "wires" && !rejected {
        check_requests(&h, scene);
        let r = result.as_ref().unwrap();
        let values = values(r);
        let expected = if ["both", "spot-both"].contains(&scene) {
            3
        } else if ["sl", "tp"].contains(&scene) {
            2
        } else {
            1
        };
        assert_eq!(
            values.len(),
            expected,
            "LIMIT native returned wire count {scene}"
        );
        for (i, v) in values.iter().enumerate() {
            assert!(
                eq(v, &h.st.borrow().wires[i]),
                "LIMIT native original wire returned"
            )
        }
    }
    if family == "place" && scene != "wrong-type" {
        // Placement composes the real wire body; request checks precede every source comparison.
        let request_scene = if scene == "both" {
            "both"
        } else if scene == "post-only" {
            "post-only"
        } else {
            "plain"
        };
        check_requests(&h, request_scene);
        assert_eq!(count(&h, "getBuilderInfo"), 1, "LIMIT native builder count");
        if scene != "builder-error" {
            let secret_call =
                h.st.borrow()
                    .effects
                    .iter()
                    .find(|v| eq(&get(v, "0"), &s("createWalletFromPrivateKey")))
                    .unwrap()
                    .clone();
            assert!(
                eq(&get(&secret_call, "1"), &s("public-fixture-key")),
                "LIMIT native wallet secret input"
            );
            let st = h.st.borrow();
            assert_eq!(st.actions.len(), 1, "LIMIT native action count");
            let action = &st.actions[0];
            assert!(
                eq(&get(action, "builder"), &st.builder),
                "LIMIT native builder alias"
            );
            let orders = values(&get(action, "orders"));
            assert_eq!(
                orders.len(),
                if scene == "both" { 3 } else { 1 },
                "LIMIT native action input floor"
            );
            for (i, w) in orders.iter().enumerate() {
                assert!(eq(w, &st.wires[i]))
            }
            assert!(
                eq(
                    &get(action, "kind"),
                    &s(if ["both", "fresh-after-builder"].contains(&scene) {
                        "orderWiresToNormalTPSLAction"
                    } else {
                        "orderWiresToOrderAction"
                    })
                ),
                "LIMIT native grouping after builder await"
            );
            assert_eq!(st.sign_args.len(), 5, "LIMIT native sign arity");
            assert!(eq(&st.sign_args[0], &st.wallet));
            assert!(eq(&st.sign_args[1], action));
            assert!(
                eq(
                    &st.sign_args[2],
                    &if scene == "vault" {
                        s("public-fixture-vault")
                    } else {
                        N::Null
                    }
                ),
                "LIMIT native sign vault"
            );
            assert_eq!(num(&st.sign_args[3]), 123456.);
            assert!(
                eq(&st.sign_args[4], &N::B(scene != "testnet")),
                "LIMIT native mainnet flag"
            );
        }
        if !["builder-error", "sign-error"].contains(&scene) {
            let st = h.st.borrow();
            assert_eq!(st.payloads.len(), 1, "LIMIT native payload count");
            let payload = &st.payloads[0];
            if scene == "vault" {
                assert!(
                    eq(&get(payload, "vaultAddress"), &s("public-fixture-vault")),
                    "LIMIT native payload vault value"
                )
            }
            assert_eq!(
                fields(payload),
                if scene == "vault" {
                    vec!["action", "nonce", "signature", "vaultAddress"]
                } else {
                    vec!["action", "nonce", "signature"]
                }
            );
            assert!(
                eq(&get(payload, "action"), &st.actions[0]),
                "LIMIT native payload action alias"
            );
            assert!(
                eq(&get(payload, "signature"), &st.signature),
                "LIMIT native signature alias"
            );
            assert!(
                eq(&get(&st.signature, "r"), &s("sig-r")),
                "LIMIT native signature r"
            );
            assert!(
                eq(&get(&st.signature, "s"), &s("sig-s")),
                "LIMIT native signature s"
            );
            assert_eq!(
                num(&get(&st.signature, "v")),
                28.,
                "LIMIT native signature v"
            );
            assert_eq!(num(&get(payload, "nonce")), 123456.);
        }
        let hip = [
            "hip3",
            "post-only",
            "both",
            "empty",
            "falsy-first",
            "binding-error",
            "pending-error",
            "next-error",
            "drop-observer",
            "fresh-market",
        ]
        .contains(&scene);
        if !rejected {
            let st = h.st.borrow();
            assert!(
                eq(result.as_ref().unwrap(), &st.ids),
                "LIMIT native original IDs return"
            );
            let expected = if scene == "empty" { 0 } else { 2 };
            assert_eq!(
                st.orders.len(),
                if hip { expected } else { 0 },
                "LIMIT native optimistic count {scene}"
            );
            for (i, o) in st.orders.iter().enumerate() {
                let original_ids = values(&st.ids);
                assert!(
                    eq(&get(o, "id"), &original_ids[i]),
                    "LIMIT native optimistic id sequence"
                );
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
                        "timeInForce",
                        "lastUpdate"
                    ]
                );
                for (k, v) in [
                    ("status", s("creating")),
                    (
                        "symbol",
                        s(if scene == "fresh-market" {
                            "later-symbol"
                        } else {
                            "BTC/USDC"
                        }),
                    ),
                    ("type", s("limit")),
                    ("side", s("buy")),
                    ("price", N::Num(101.25)),
                    ("amount", N::Num(7.5)),
                    ("filled", N::Num(0.)),
                    ("remaining", N::Num(7.5)),
                    ("reduceOnly", N::B(false)),
                    (
                        "timeInForce",
                        s(if scene == "post-only" { "post" } else { "gtc" }),
                    ),
                    ("lastUpdate", N::Num(987654.)),
                ] {
                    assert!(
                        eq(&get(o, k), &v),
                        "LIMIT native optimistic field {scene}/{i}/{k}"
                    )
                }
            }
            drop(st);
            assert_eq!(
                count(&h, "rememberLimitOrderTimeInForce"),
                usize::from(!["empty", "falsy-first"].contains(&scene)),
                "LIMIT native first binding TIF"
            );
            assert_eq!(
                count(&h, "refreshHip3OrdersWithRetry"),
                usize::from(hip),
                "LIMIT native retry count"
            );
            if hip {
                let ns = names(&h);
                assert_eq!(
                    count(&h, "addToPrePlacementCache"),
                    1,
                    "LIMIT native independent preplacement count"
                );
                assert!(
                    ns.iter()
                        .position(|x| x == "addToPrePlacementCache")
                        .expect("LIMIT native precache exists")
                        < ns.iter()
                            .position(|x| x == "postOrderAction")
                            .expect("LIMIT native REST exists"),
                    "LIMIT native preplacement before REST"
                );
                let pre =
                    h.st.borrow()
                        .effects
                        .iter()
                        .find(|v| eq(&get(v, "0"), &s("addToPrePlacementCache")))
                        .unwrap()
                        .clone();
                let pre_args = values(&pre);
                assert_eq!(pre_args.len(), 7, "LIMIT native precache arity");
                for (i, v) in [
                    s("coin"),
                    s("buy"),
                    s("limit"),
                    N::Num(101.25),
                    N::Num(7.5),
                    s("0xmint1"),
                ]
                    .iter()
                    .enumerate()
                {
                    assert!(
                        eq(&pre_args[i + 1], v),
                        "LIMIT native precache argument {i}"
                    )
                }
                assert_eq!(
                    count(&h, "addPendingHip3Order"),
                    expected,
                    "LIMIT native pending count"
                );
                let retry =
                    h.st.borrow()
                        .effects
                        .iter()
                        .find(|v| eq(&get(v, "0"), &s("refreshHip3OrdersWithRetry")))
                        .unwrap()
                        .clone();
                assert!(
                    eq(&get(&retry, "1"), &h.st.borrow().ids),
                    "LIMIT native retry original IDs"
                )
            }
        }
        assert_eq!(
            count(&h, "close"),
            usize::from(scene == "pending-error"),
            "LIMIT native body-only iterator close"
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
fn limit_bodies_native_before_source() {
    let mut actual = vec![];
    let mut failures = vec![];
    let mut completed = 0;
    let mut last = None;
    for (family, scenes) in [("wires", WIRES.as_slice()), ("place", PLACE.as_slice())] {
        for scene in scenes {
            match catch_unwind(AssertUnwindSafe(|| case(family, scene))) {
                Ok(v) => actual.push((family, *scene, v)),
                Err(_) => failures.push(format!("{family}/{scene}")),
            }
            last = Some(completed);
            completed += 1
        }
    }

    assert_eq!(completed, 35, "LIMIT independent completed native floor");
    assert_eq!(last, Some(34), "LIMIT native last index");
    // Dropping the Rust observer cannot cancel the eagerly retained placement body.
    if catch_unwind(AssertUnwindSafe(|| {
        let h = Host::new("drop-observer");
        let opts = h.st.borrow().opts.clone();
        drop(place_limit_order(&h, &h.base.exchange, &opts, &N::U));
        h.base.drain();
        let st = h.st.borrow();
        assert_eq!(
            st.settled.len(),
            1,
            "LIMIT native dropped observer actual settlement count"
        );
        assert!(
            eq(st.settled[0].as_ref().unwrap(), &st.ids),
            "LIMIT native dropped observer original IDs"
        );
        assert_eq!(
            st.orders.len(),
            2,
            "LIMIT native dropped observer body completed"
        );
        drop(st);
    }))
    .is_err()
    {
        failures.push("direct/drop-observer".into());
    }
    assert!(
        failures.is_empty(),
        "LIMIT native assertions failed: {failures:?}"
    );
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/limit-orders-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = oracle["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 35, "LIMIT independent source floor");
    assert_eq!(oracle["last_index"], 34);
    for ((family, scene, v), row) in actual.iter().zip(rows) {
        assert_eq!(row["family"], *family);
        assert_eq!(row["scene"], *scene);
        native::assert_value(v, &row["state"], &format!("LIMIT source {family}/{scene}"))
    }
}
