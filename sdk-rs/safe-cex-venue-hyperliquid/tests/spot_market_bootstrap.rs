#[allow(dead_code)]
#[path = "support/spot_account_host.rs"]
mod account;
#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[allow(dead_code)]
#[path = "support/rest_host.rs"]
mod native;
#[path = "support/spot_market_bootstrap_host.rs"]
mod support;
use cache::{N, arr, eq, object, s};
use native::assert_value;
use safe_cex_venue_hyperliquid::{
    spot_json::JsString, spot_market_bootstrap::fetch_spot_markets_and_tickers,
    ws_ping_lifecycle::WsPingHost,
};
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
use support::{Host, State, get};
const SCENES: [&str; 24] = [
    "disabled",
    "ordinary",
    "no-alias",
    "missing-base",
    "missing-quote",
    "dead-mid",
    "zero-mark",
    "nonfinite-volume",
    "zero-precision",
    "source-info-overwrite",
    "proto-coin",
    "proto-token",
    "utf16-coin",
    "utf16-index",
    "post-error",
    "null-data",
    "malformed-assets",
    "null-response",
    "helper-error",
    "log-error",
    "drop-observer",
    "token-rhs-before-key",
    "asset-rhs-before-key",
    "late-helper-error",
];
fn key(scene: &str, coin: bool) -> N {
    match (scene, coin) {
        ("proto-coin", true) | ("proto-token", false) => s("__proto__"),
        ("utf16-coin", true) => N::S(JsString("coin".encode_utf16().chain([0xd800]).collect())),
        ("utf16-index", false) => N::S(JsString("idx".encode_utf16().chain([0xd800]).collect())),
        (_, true) => s("@a"),
        (_, false) => N::Num(0.),
    }
}
fn fixture(scene: &str) -> (N, N) {
    let index = key(scene, false);
    let coin = key(scene, true);
    let tokens = arr(vec![
        object(vec![
            ("index", index.clone()),
            ("name", s("UBTC")),
            ("fullName", s("Unit Bitcoin")),
            ("szDecimals", s("3")),
        ]),
        object(vec![
            ("index", N::Num(1.)),
            ("name", s("USDC")),
            ("szDecimals", s("6")),
        ]),
        object(vec![
            ("index", N::Num(2.)),
            ("name", s("USDH")),
            ("szDecimals", s("6")),
        ]),
    ]);
    let a = object(vec![
        ("name", coin.clone()),
        ("index", N::Num(7.)),
        (
            "tokens",
            arr(vec![
                if scene == "missing-base" {
                    N::Num(99.)
                } else {
                    index.clone()
                },
                N::Num(1.),
            ]),
        ),
    ]);
    if scene == "source-info-overwrite" {
        let mut h = cache::Host::default();
        for (k, v) in [
            ("hyperliquidId", s("overridden")),
            ("isSpot", N::B(false)),
            ("hlTokenName", s("source-override")),
        ] {
            h.put(&a, k, &v, true).unwrap()
        }
    }
    let universe = arr(vec![
        a,
        object(vec![
            ("name", s("@b")),
            ("index", N::Num(8.)),
            (
                "tokens",
                arr(vec![
                    index,
                    if scene == "missing-quote" {
                        N::Num(99.)
                    } else {
                        N::Num(2.)
                    },
                ]),
            ),
        ]),
    ]);
    let assets = arr(vec![
        object(vec![("coin", s("junk")), ("midPx", s("999"))]),
        object(vec![
            ("coin", s("@b")),
            ("midPx", s("20")),
            ("markPx", s("21")),
            ("prevDayPx", s("19")),
        ]),
        object(vec![
            ("coin", coin.clone()),
            ("midPx", s("9")),
            ("markPx", s("8")),
            ("prevDayPx", s("7")),
        ]),
        object(vec![
            ("coin", coin),
            (
                "midPx",
                if scene == "dead-mid" {
                    N::Null
                } else {
                    s("10")
                },
            ),
            ("markPx", s(if scene == "zero-mark" { "0" } else { "11" })),
            ("prevDayPx", s("9")),
        ]),
    ]);
    let data = if scene == "null-data" {
        N::Null
    } else {
        arr(vec![
            object(vec![("tokens", tokens), ("universe", universe)]),
            if scene == "malformed-assets" {
                object(vec![])
            } else {
                assets.clone()
            },
        ])
    };
    (data, assets)
}
fn host(scene: &str) -> Host {
    let (data, _) = fixture(scene);
    let base = account::Host::new(
        object(vec![]),
        object(vec![(
            "LogSeverity",
            object(vec![("Warning", s("warning"))]),
        )]),
        object(vec![]),
    );
    let mut h = Host {
        base,
        state: Rc::new(RefCell::new(State {
            scene: scene.into(),
            effects: vec![],
            data,
            marker: object(vec![
                ("name", s("SpotError")),
                (
                    "message",
                    N::S(JsString(
                        "spot-failure".encode_utf16().chain([0xd800]).collect(),
                    )),
                ),
                ("response", object(vec![("code", N::Num(55.))])),
            ]),
            key_object: object(vec![]),
            key_marker: object(vec![("message", s("key-failure"))]),
            rhs_marker: object(vec![(
                "message",
                N::S(JsString(
                    "rhs-failure".encode_utf16().chain([0xd800]).collect(),
                )),
            )]),
            token_reads: 0,
            precision_calls: 0,
            asset_reads: 0,
            settlements: vec![],
            proto: object(vec![]),
            dynamic: vec![],
            posts: 0,
            created_arrays: vec![],
        })),
    };
    if scene == "token-rhs-before-key" {
        let token = get(&get(&get(&h.state.borrow().data, "0"), "tokens"), "0");
        cache::Host::default()
            .put(&token, "index", &h.state.borrow().key_object, true)
            .unwrap();
    }
    if scene == "asset-rhs-before-key" {
        let asset = get(&get(&h.state.borrow().data, "1"), "0");
        cache::Host::default()
            .put(&asset, "coin", &h.state.borrow().key_object, true)
            .unwrap();
    }
    let e = h.e();
    let options = get(&e, "options");
    h.set(&options, "enableSpotMarkets", &N::B(scene != "disabled"))
        .unwrap();
    h.set(
        &e,
        "infoXhr",
        &object(vec![("post", N::Builtin("post".into()))]),
    )
    .unwrap();
    h.set(
        &e,
        "resolveSpotDisplayName",
        &N::Builtin("resolveSpotDisplayName".into()),
    )
    .unwrap();
    h
}
fn count(scene: &str) -> usize {
    if [
        "disabled",
        "log-error",
        "post-error",
        "null-data",
        "malformed-assets",
        "null-response",
        "helper-error",
        "late-helper-error",
        "token-rhs-before-key",
        "asset-rhs-before-key",
    ]
    .contains(&scene)
    {
        0
    } else if ["missing-base", "missing-quote", "dead-mid"].contains(&scene) {
        1
    } else {
        2
    }
}
#[test]
fn spot_bootstrap_native_coin_quote_alias_and_failure_floor24() {
    let mut rows = vec![];
    for scene in SCENES {
        let h = host(scene);
        let e = h.e();
        let op = fetch_spot_markets_and_tickers(h.clone(), e);
        let mut op = Some(op);
        if scene == "drop-observer" {
            drop(op.take())
        }
        h.base.drain();
        assert_eq!(
            h.state.borrow().settlements.len(),
            1,
            "native actual spot adopted settlement count"
        );
        let result = h.state.borrow().settlements[0].clone();
        if let Some(mut op) = op {
            let mut cx = Context::from_waker(Waker::noop());
            let Poll::Ready(observed) = Pin::new(&mut op).poll(&mut cx) else {
                panic!("native spot observer pending")
            };
            assert_eq!(observed.is_err(), result.is_err())
        }
        assert_eq!(
            result.is_err(),
            scene == "log-error",
            "native spot logger rejection scope {scene}"
        );
        assert_eq!(
            h.state.borrow().posts,
            usize::from(scene != "disabled"),
            "native spot opt-in post count {scene}"
        );
        if let Err(err) = &result {
            assert!(
                eq(err, &h.state.borrow().marker),
                "native spot original logger error identity"
            );
            assert!(
                eq(&get(&get(err, "response"), "code"), &N::Num(55.)),
                "native spot error response value"
            )
        }
        if let Ok(v) = &result {
            let markets = get(v, "markets");
            let tickers = get(v, "tickers");
            let N::A(m) = &markets else { panic!("markets") };
            let N::A(t) = &tickers else { panic!("tickers") };
            assert_eq!(
                m.borrow().len(),
                count(scene),
                "native independent spot market floor {scene}"
            );
            assert_eq!(
                t.borrow().len(),
                count(scene),
                "native independent spot ticker floor {scene}"
            );
            assert!(
                !eq(&markets, &tickers),
                "native fresh separate result arrays"
            );
            if [
                "post-error",
                "null-response",
                "helper-error",
                "late-helper-error",
            ]
            .contains(&scene)
            {
                let st = h.state.borrow();
                assert!(
                    !eq(&markets, &st.created_arrays[0]) && !eq(&tickers, &st.created_arrays[1]),
                    "native catch returns fresh arrays instead of partial data"
                )
            }
            let expected_rows = if count(scene) == 0 {
                vec![]
            } else if ["missing-base", "dead-mid"].contains(&scene) {
                vec![(s("@b"), true)]
            } else if scene == "missing-quote" {
                vec![(key(scene, true), false)]
            } else {
                vec![(key(scene, true), false), (s("@b"), true)]
            };
            for (row_index, (market, ticker)) in
                m.borrow().iter().zip(t.borrow().iter()).enumerate()
            {
                let (wanted_id, is_b) = &expected_rows[row_index];
                let is_b = *is_b;
                assert!(
                    eq(&get(market, "id"), wanted_id),
                    "native independent market ID sequence {scene}/{row_index}"
                );
                assert!(
                    eq(&get(ticker, "id"), wanted_id),
                    "native independent ticker ID sequence {scene}/{row_index}"
                );
                let quote = if is_b { "USDH" } else { "USDC" };
                let last = if is_b { 20. } else { 10. };
                assert!(
                    eq(&get(market, "quote"), &s(quote)),
                    "native real quote token {scene}"
                );
                assert!(
                    eq(&get(market, "symbol"), &s(&format!("UBTC/{quote}"))),
                    "native venue routing symbol {scene}"
                );
                let base = if scene == "no-alias" { "UBTC" } else { "BTC" };
                assert!(
                    eq(&get(market, "base"), &s(base)),
                    "native display base {scene}"
                );
                assert!(
                    eq(
                        &get(market, "prettySymbol"),
                        &if scene == "no-alias" {
                            N::U
                        } else {
                            s(&format!("BTC/{quote}"))
                        }
                    ),
                    "native conditional pretty symbol {scene}"
                );
                assert!(
                    eq(&get(ticker, "last"), &N::Num(last)),
                    "native coin-keyed last-context join {scene}"
                );
                let mark = if is_b {
                    21.
                } else if scene == "zero-mark" {
                    10.
                } else {
                    11.
                };
                assert!(
                    eq(&get(ticker, "mark"), &N::Num(mark)),
                    "native mark fallback {scene}"
                );
                for k in [
                    "bid",
                    "ask",
                    "index",
                    "fundingRate",
                    "fundingTime",
                    "openInterest",
                ] {
                    assert!(
                        matches!(get(ticker,k),N::Num(n)if n.to_bits()==0),
                        "native spot zero bits {k}"
                    )
                }
                assert!(
                    eq(&get(ticker, "symbol"), &s(&format!("UBTC/{quote}"))),
                    "native ticker routing symbol"
                );
                assert!(
                    eq(
                        &get(ticker, "percentage"),
                        &N::Num(if !is_b && scene == "zero-mark" {
                            1.
                        } else {
                            2.
                        })
                    ),
                    "native percentage helper input order"
                );
                let info = get(market, "info");
                if scene == "source-info-overwrite" && !is_b {
                    assert!(
                        eq(&get(&info, "hyperliquidId"), &s("overridden")),
                        "native source info spread takes precedence"
                    );
                    assert!(
                        eq(&get(&info, "isSpot"), &N::B(false)),
                        "native source info boolean overwrite"
                    )
                } else {
                    assert!(
                        eq(
                            &get(&info, "hyperliquidId"),
                            &N::Num(if is_b { 10008. } else { 10007. })
                        ),
                        "native correct spot index offset"
                    )
                }
                let native_assets = get(&h.state.borrow().data, "1");
                assert!(
                    eq(
                        &get(ticker, "info"),
                        &get(&native_assets, if is_b { "1" } else { "3" })
                    ),
                    "native original coin-joined asset object handle"
                );
                let precision = get(market, "precision");
                assert!(
                    eq(
                        &get(&precision, "amount"),
                        &N::Num(if scene == "zero-precision" { 0. } else { 3. })
                    ),
                    "native amount precision helper result"
                );
                assert!(
                    eq(
                        &get(&precision, "price"),
                        &N::Num(if scene == "zero-precision" { 0. } else { 0.01 })
                    ),
                    "native distinct price precision helper result"
                );
                if scene == "nonfinite-volume" {
                    assert!(
                        matches!(get(ticker,"volume"),N::Num(n)if n.is_nan()),
                        "native NaN volume remains native"
                    );
                    assert!(
                        matches!(get(ticker,"quoteVolume"),N::Num(n)if n.to_bits()==f64::INFINITY.to_bits()),
                        "native infinite quote volume remains native"
                    )
                }
            }
        }
        if ["token-rhs-before-key", "asset-rhs-before-key"].contains(&scene) {
            let last = h.state.borrow().effects.last().cloned().unwrap();
            let expected = N::S(JsString(
                "Failed to load Hyperliquid spot markets: rhs-failure"
                    .encode_utf16()
                    .chain([0xd800])
                    .collect(),
            ));
            assert!(
                eq(&get(&last, "1"), &expected),
                "native assignment RHS wins key coercion error {scene}"
            );
            assert!(
                eq(&get(&last, "2"), &s("warning")),
                "native assignment failure severity"
            );
        }
        if scene == "late-helper-error" {
            let st = h.state.borrow();
            for array in &st.created_arrays[..2] {
                let N::A(a) = array else {
                    panic!("partial array")
                };
                assert_eq!(
                    a.borrow().len(),
                    1,
                    "native one partial row before fresh catch reset"
                );
            }
            assert_eq!(
                st.precision_calls, 4,
                "native late failure stops at fourth precision call"
            );
        }
        let calculations = h
            .state
            .borrow()
            .effects
            .iter()
            .filter(|v| eq(&get(v, "0"), &s("calculatePricePrecision")))
            .cloned()
            .collect::<Vec<_>>();
        let expected_mids = if scene == "helper-error" {
            vec![10.]
        } else if scene == "late-helper-error" {
            vec![10., 20.]
        } else if count(scene) == 0 {
            vec![]
        } else if ["missing-base", "dead-mid"].contains(&scene) {
            vec![20.]
        } else if scene == "missing-quote" {
            vec![10.]
        } else {
            vec![10., 20.]
        };
        assert_eq!(
            calculations.len(),
            expected_mids.len(),
            "native independent precision calculation count {scene}"
        );
        for (call, mid) in calculations.iter().zip(expected_mids) {
            assert!(
                eq(&get(call, "length"), &N::Num(4.)),
                "native precision helper argument arity"
            );
            for (key, wanted) in [("1", mid), ("2", 3.), ("3", 8.)] {
                assert!(
                    eq(&get(call, key), &N::Num(wanted)),
                    "native precision helper argument {key}/{scene}"
                );
            }
        }
        rows.push((scene, h, result));
    }
    assert_eq!(rows.len(), 24, "native independent completed spot floor");
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/spot-market-bootstrap-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let oracle: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(oracle["completed"], 24);
    assert_eq!(oracle["last"], 23);
    for (i, (scene, h, result)) in rows.iter().enumerate() {
        let r = &oracle["cases"][i];
        assert_eq!(r["scene"], *scene);
        assert_eq!(result.is_err(), r["rejected"].as_bool().unwrap());
        assert_value(
            &arr(h.state.borrow().effects.clone()),
            &r["effects"],
            &format!("native spot effects {scene}"),
        );
        assert_value(
            result.as_ref().unwrap_or_else(|v| v),
            if result.is_err() {
                &r["error"]
            } else {
                &r["result"]
            },
            &format!("native spot result {scene}"),
        );
    }
}
