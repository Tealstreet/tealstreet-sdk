#[allow(dead_code)]
#[path = "support/spot_account_host.rs"]
mod account;
#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[allow(dead_code)]
#[path = "support/rest_host.rs"]
mod native;
#[path = "support/startup_lifecycle_host.rs"]
mod support;
use cache::{N, arr, eq, object, s};
use native::assert_value;
use safe_cex_venue_hyperliquid::{
    startup_lifecycle::{StartupCallback, run_startup_callback, start, tick},
    ws_ping_lifecycle::WsPingHost,
};
use std::{
    cell::RefCell,
    collections::HashMap,
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
use support::{Host, State};
const STARTS: [&str; 17] = [
    "private",
    "external",
    "public",
    "public-external",
    "no-key",
    "resolve-success",
    "resolve-disposed",
    "mode-disposed",
    "markets-disposed",
    "init-error",
    "markets-error",
    "reject-all",
    "core-null",
    "no-track",
    "truthy-public-option",
    "log-error",
    "drop-observer",
];
const TICKS: [&str; 12] = [
    "both",
    "neither",
    "equal-order",
    "equal-position",
    "no-dex",
    "not-needed",
    "disposed",
    "public",
    "reject-all",
    "sync-orders-error",
    "dispose-on-orders",
    "fire-replacement",
];
fn get(v: &N, k: &str) -> N {
    account::get(v, k)
}
fn host(scene: &str, enums: N) -> Host {
    let base = account::Host::new(object(vec![]), enums, object(vec![]));
    let mut h = Host {
        base,
        state: Rc::new(RefCell::new(State {
            effects: vec![],
            callbacks: HashMap::new(),
            promises: vec![],
            scene: scene.into(),
            core: object(vec![("marker", s("core-state"))]),
            positions: arr(vec![object(vec![("symbol", s("native-position"))])]),
            marker: N::S(safe_cex_venue_hyperliquid::spot_json::JsString(
                "failure".encode_utf16().chain([0xd800]).collect(),
            )),
            drop_mode: scene == "drop-observer",
            release_mode: false,
            mode_reached: false,
            settlements: vec![],
        })),
    };
    h.base.markets = arr(vec![object(vec![("symbol", s("BTC/USDC"))])]);
    let store = get(&h.e(), "store");
    h.set(&store, "markets", &h.base.markets.clone()).unwrap();
    let options = get(&h.e(), "options");
    h.set(
        &options,
        "key",
        &s(if scene == "no-key" { "" } else { "key" }),
    )
    .unwrap();
    h.set(
        &options,
        "publicDataOnly",
        &if scene == "truthy-public-option" {
            s("yes")
        } else {
            N::B(scene.starts_with("public"))
        },
    )
    .unwrap();
    for (k, v) in [
        (
            "hasPrivateSessionIdentity",
            N::B(!["no-key", "resolve-success", "resolve-disposed"].contains(&scene)),
        ),
        ("isPublicDataOnly", N::B(scene.starts_with("public"))),
        ("isDisposed", N::B(scene == "disposed")),
        (
            "hip3DexNames",
            arr(if scene == "no-dex" {
                vec![]
            } else {
                vec![s("dex")]
            }),
        ),
        (
            "_hip3OrdersNeedRefresh",
            N::B(!["not-needed", "neither"].contains(&scene)),
        ),
        (
            "_lastOrderRefreshTime",
            N::Num(if scene == "equal-order" {
                190000.
            } else if scene == "neither" {
                199999.
            } else {
                0.
            }),
        ),
        (
            "_lastPositionRefreshTime",
            N::Num(if scene == "equal-position" {
                80000.
            } else if scene == "neither" {
                199999.
            } else {
                0.
            }),
        ),
        ("HIP3_ORDER_REFRESH_INTERVAL_MS", N::Num(10000.)),
        ("HIP3_POSITION_REFRESH_INTERVAL_MS", N::Num(120000.)),
    ] {
        h.put(k, v)
    }
    for k in [
        "resolveApplicationIdFromKey",
        "warnOnMismatchedApiWallet",
        "resolveUserAbstractionMode",
        "fetchCorePerpsState",
        "fetchBuilderFeeApproval",
        "refreshOrders",
        "fetchAllHip3Positions",
        "buildMarketsLowerCaseMap",
        "shouldTrackSpotHoldings",
        "requestUnifiedSpotCollateralRefresh",
    ] {
        h.put(k, N::Builtin(k.into()))
    }
    let socket = get(&h.e(), "privateWebsocket");
    h.set(
        &socket,
        "applyCoreClearinghouseState",
        &N::Builtin("applyCoreClearinghouseState".into()),
    )
    .unwrap();
    h
}
fn label(v: &N) -> String {
    let N::S(t) = get(v, "0") else {
        panic!("effectlabel")
    };
    String::from_utf16(&t.0).unwrap()
}
#[test]
fn primary_start_and_tick_native_then_source_floor29() {
    // Independent native expectations run before loading any source oracle data.
    let enums = object(vec![(
        "LogSeverity",
        object(vec![("Warning", s("warning")), ("Debug", s("debug"))]),
    )]);
    // Callback returns are native values, not inferred from eventual wire output.
    let mut callback_host = host("callback", enums.clone());
    let owner = callback_host.e();
    let marker = callback_host.state.borrow().marker.clone();
    for returns_null in [false, true] {
        let value = run_startup_callback(
            &mut callback_host,
            &StartupCallback::Warning {
                exchange: owner.clone(),
                prefix: "native-prefix: ",
                returns_null,
            },
            std::slice::from_ref(&marker),
        )
        .unwrap();
        assert!(
            matches!((&value, returns_null), (N::Null, true) | (N::U, false)),
            "native warning callback distinct null return"
        );
        let effect = callback_host
            .state
            .borrow()
            .effects
            .last()
            .cloned()
            .unwrap();
        let mut expected = "native-prefix: failure".encode_utf16().collect::<Vec<_>>();
        expected.push(0xd800);
        assert!(
            eq(
                &get(&effect, "1"),
                &N::S(safe_cex_venue_hyperliquid::spot_json::JsString(expected))
            ),
            "native warning callback original UTF16 rejection substitution"
        );
        assert!(
            eq(&get(&effect, "2"), &s("warning")),
            "native warning callback severity argument"
        );
    }
    let cb = StartupCallback::CoreState {
        exchange: owner.clone(),
    };
    let core = callback_host.state.borrow().core.clone();
    for disposed in [true, false] {
        callback_host.put("isDisposed", N::B(disposed));
        for state in [N::Null, core.clone()] {
            let before = callback_host.state.borrow().effects.len();
            let returned =
                run_startup_callback(&mut callback_host, &cb, std::slice::from_ref(&state))
                    .unwrap();
            assert!(
                matches!(returned, N::U),
                "native core callback undefined return"
            );
            assert_eq!(
                callback_host.state.borrow().effects.len() - before,
                usize::from(!disposed && !matches!(state, N::Null)),
                "native core callback disposal and null guard"
            );
        }
    }
    callback_host.state.borrow_mut().scene = "log-error".into();
    let error = run_startup_callback(
        &mut callback_host,
        &StartupCallback::Warning {
            exchange: owner,
            prefix: "prefix: ",
            returns_null: true,
        },
        std::slice::from_ref(&marker),
    )
    .unwrap_err();
    assert!(
        eq(&error, &marker),
        "native warning logger original throw identity"
    );
    let mut results = vec![];
    for (family, scenes) in [("start", STARTS.as_slice()), ("tick", TICKS.as_slice())] {
        for scene in scenes {
            let mut h = host(scene, enums.clone());
            let e = h.e();
            let observer = if family == "start" {
                start(h.clone(), e.clone())
            } else {
                tick(h.clone(), e.clone())
            };
            if *scene == "drop-observer" {
                drop(observer);
                assert!(
                    !h.state.borrow().mode_reached,
                    "native observer dropped before initializer await resumes"
                );
                h.state.borrow_mut().release_mode = true;
                h.base.drain();
                assert!(
                    h.state.borrow().mode_reached,
                    "native dropped observer reaches mode"
                );
            } else {
                h.base.drain();
                let mut observer = observer;
                let mut cx = Context::from_waker(Waker::noop());
                let Poll::Ready(result) = Pin::new(&mut observer).poll(&mut cx) else {
                    panic!("native lifecycle observer unresolved {family}/{scene}")
                };
                let rejected = [
                    "init-error",
                    "markets-error",
                    "log-error",
                    "sync-orders-error",
                ]
                .contains(scene);
                assert_eq!(
                    result.is_err(),
                    rejected,
                    "native lifecycle rejection {family}/{scene}"
                );
                if let Err(err) = &result {
                    assert!(
                        eq(err, &h.state.borrow().marker),
                        "native lifecycle original rejection {scene}"
                    );
                }
                results.push((family.to_owned(), scene.to_string(), h.clone(), result));
            }
            if *scene == "drop-observer" {
                assert_eq!(
                    h.state.borrow().settlements.len(),
                    1,
                    "native dropped observer actual settlement count"
                );
                let actual = h.state.borrow().settlements[0].clone();
                assert!(
                    matches!(actual, Ok(N::U)),
                    "native dropped observer actual undefined settlement"
                );
                results.push((family.into(), scene.to_string(), h.clone(), actual));
            }
            let labels = h
                .state
                .borrow()
                .effects
                .iter()
                .map(label)
                .collect::<Vec<_>>();
            if family == "start" {
                let core_expected = ![
                    "public",
                    "public-external",
                    "no-key",
                    "resolve-disposed",
                    "mode-disposed",
                    "markets-disposed",
                    "init-error",
                    "markets-error",
                    "reject-all",
                    "core-null",
                ]
                .contains(scene);
                assert_eq!(
                    labels
                        .iter()
                        .filter(|v| v.as_str() == "applyCoreClearinghouseState")
                        .count(),
                    usize::from(core_expected),
                    "native independent core snapshot count {scene}"
                );
            }
            if *scene == "reject-all" {
                let prefixes = if family == "start" {
                    vec![
                        "userAbstraction resolution failed: ",
                        "Initial core-perp account-state fetch failed: ",
                        "Builder fee approval check failed: ",
                        "Failed to fetch initial orders: ",
                        "Failed to fetch initial HIP-3 positions: ",
                    ]
                } else {
                    vec![
                        "HIP-3 order refresh failed: ",
                        "Periodic HIP-3 position refresh failed: ",
                    ]
                };
                for prefix in prefixes {
                    let mut text = prefix
                        .encode_utf16()
                        .chain("failure".encode_utf16())
                        .collect::<Vec<_>>();
                    text.push(0xd800);
                    let expected = N::S(safe_cex_venue_hyperliquid::spot_json::JsString(text));
                    let st = h.state.borrow();
                    let matches = st
                        .effects
                        .iter()
                        .filter(|v| label(v) == "log" && eq(&get(v, "1"), &expected))
                        .collect::<Vec<_>>();
                    assert_eq!(
                        matches.len(),
                        1,
                        "native warning exact prefix and original UTF16 value {prefix}"
                    );
                    assert!(
                        eq(&get(matches[0], "2"), &s("warning")),
                        "native warning severity {prefix}"
                    );
                }
            }
            if family == "tick" {
                let order = ![
                    "neither",
                    "equal-order",
                    "no-dex",
                    "not-needed",
                    "disposed",
                    "public",
                ]
                .contains(scene);
                let position = ![
                    "neither",
                    "equal-position",
                    "no-dex",
                    "disposed",
                    "public",
                    "sync-orders-error",
                ]
                .contains(scene);
                assert_eq!(
                    labels.iter().filter(|v| *v == "refreshOrders").count(),
                    usize::from(order),
                    "native tick strict order guard {scene}"
                );
                assert_eq!(
                    labels
                        .iter()
                        .filter(|v| *v == "fetchAllHip3Positions")
                        .count(),
                    usize::from(position),
                    "native tick strict position guard {scene}"
                );
                assert_eq!(
                    labels.iter().filter(|v| *v == "loop").count(),
                    usize::from(
                        ![
                            "disposed",
                            "public",
                            "sync-orders-error",
                            "dispose-on-orders"
                        ]
                        .contains(scene)
                    ),
                    "native tick final disposal guard {scene}"
                );
                if order {
                    assert!(
                        eq(&get(&e, "_hip3OrdersNeedRefresh"), &N::B(false)),
                        "native tick clears pending before refresh"
                    );
                    assert!(
                        eq(&get(&e, "_lastOrderRefreshTime"), &N::Num(200000.)),
                        "native tick order timestamp"
                    );
                }
                if *scene == "fire-replacement" {
                    h.base.fire_loop();
                    assert!(
                        eq(
                            &get(&h.base.aliases("tick", &N::U), "loopResultIdentity"),
                            &N::B(true)
                        ),
                        "native timer lexical owner and result identity"
                    );
                    h.record("loop-fired", &[N::B(true)]);
                }
            } else if ![
                "resolve-disposed",
                "mode-disposed",
                "init-error",
                "markets-error",
                "markets-disposed",
                "log-error",
            ]
            .contains(scene)
            {
                let store = get(&e, "store");
                assert!(
                    eq(&get(&store, "positions"), &h.state.borrow().positions),
                    "native startup positions retained original handle {scene}"
                );
                let loaded = get(&store, "loaded");
                let expected = *scene != "no-key";
                for k in ["orders", "balance", "positions"] {
                    assert!(
                        eq(&get(&loaded, k), &N::B(expected)),
                        "native startup private loaded {k}/{scene}"
                    );
                }
                assert!(
                    eq(&get(&loaded, "custom"), &N::B(true)),
                    "native startup loaded spread retained field"
                );
                for k in ["used", "free", "total", "upnl"] {
                    assert!(
                        matches!(get(&get(&store,"balance"),k),N::Num(v)if v.to_bits()==0),
                        "native startup zero balance bits {k}"
                    );
                }
                let updated = labels.iter().position(|v| v == "update").unwrap();
                if let Some(connected) = labels.iter().position(|v| v == "private-connect") {
                    assert!(
                        updated < connected,
                        "native startup store before private connection"
                    );
                }
            }
        }
    }
    assert_eq!(
        results.len(),
        29,
        "native independent lifecycle completed floor"
    );
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/startup-lifecycle-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(oracle["completed"], 29);
    assert_eq!(oracle["last"], 28);
    for (i, (family, scene, h, result)) in results.iter().enumerate() {
        let row = &oracle["cases"][i];
        assert_eq!(row["family"], *family);
        assert_eq!(row["scene"], *scene);
        assert_value(
            &arr(h.state.borrow().effects.clone()),
            &row["effects"],
            &format!("native source lifecycle effects {family}/{scene}"),
        );
        assert_eq!(result.is_err(), row["rejected"].as_bool().unwrap());
        assert_value(
            result.as_ref().unwrap_or_else(|e| e),
            &if result.is_err() {
                row["error"].clone()
            } else {
                row["result"].clone()
            },
            "native original return or rejection",
        );
        let e = h.e();
        let store = get(&e, "store");
        let state = object(vec![
            ("order", get(&e, "_lastOrderRefreshTime")),
            ("position", get(&e, "_lastPositionRefreshTime")),
            ("needed", get(&e, "_hip3OrdersNeedRefresh")),
            ("disposed", get(&e, "isDisposed")),
            ("loaded", get(&store, "loaded")),
        ]);
        assert_value(&state, &row["state"], "native lifecycle final fields");
    }
}
