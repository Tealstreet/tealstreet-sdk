#[allow(dead_code)]
#[path = "support/spot_account_host.rs"]
mod account;
#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[allow(dead_code)]
#[path = "support/rest_host.rs"]
mod native;
#[path = "support/market_bootstrap_host.rs"]
mod support;
use cache::{N, arr, eq, object, s};
use native::assert_value;
use safe_cex_venue_hyperliquid::{
    market_bootstrap::{self, MarketBootstrapCallback, MarketBootstrapHost},
    spot_json::JsString,
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
use support::{Host, State, get};
const SCENES: [&str; 24] = [
    "core-only",
    "spot-only",
    "all-success",
    "first-null",
    "second-null",
    "all-null",
    "timeout-first",
    "timeout-second",
    "deadline-empty",
    "deadline-after-first",
    "fees-error",
    "core-error",
    "fetch-sync-error",
    "timeout-error",
    "core-logger",
    "unicode-name",
    "zero-spot",
    "capped-budget",
    "minimum-budget",
    "drop-observer",
    "close-error",
    "next-error",
    "failed-then-timeout",
    "body-and-close-error",
];
fn expected(scene: &str) -> Option<Vec<&'static str>> {
    if [
        "fees-error",
        "core-error",
        "fetch-sync-error",
        "timeout-error",
        "close-error",
        "next-error",
        "body-and-close-error",
    ]
    .contains(&scene)
    {
        return None;
    }
    let mut values = vec!["core"];
    values.extend(
        if [
            "core-only",
            "spot-only",
            "all-null",
            "timeout-first",
            "deadline-empty",
            "failed-then-timeout",
        ]
        .contains(&scene)
        {
            vec![]
        } else if scene == "first-null" {
            vec!["b"]
        } else if ["second-null", "timeout-second", "deadline-after-first"].contains(&scene) {
            vec!["a"]
        } else {
            vec!["a", "b"]
        },
    );
    if !["core-only", "zero-spot"].contains(&scene) {
        values.push("spot")
    }
    Some(values)
}
fn host(scene: &str) -> Host {
    let enums = object(vec![(
        "LogSeverity",
        object(vec![("Info", s("info")), ("Warning", s("warning"))]),
    )]);
    let base = account::Host::new(object(vec![]), enums, object(vec![]));
    let name = if scene == "unicode-name" {
        N::S(JsString(vec![97, 0xd800]))
    } else {
        s("a")
    };
    let dexes = arr(if ["core-only", "spot-only"].contains(&scene) {
        vec![]
    } else {
        vec![
            object(vec![("name", name), ("dexPosition", N::Num(0.))]),
            object(vec![("name", s("b")), ("dexPosition", N::Num(1.))]),
        ]
    });
    let mut h = Host {
        base,
        state: Rc::new(RefCell::new(State {
            effects: vec![],
            callbacks: HashMap::new(),
            scene: scene.into(),
            clock: 0,
            marker: object(vec![
                ("name", s("BootstrapError")),
                (
                    "message",
                    N::S(JsString(
                        "bootstrap-failure".encode_utf16().chain([0xd800]).collect(),
                    )),
                ),
                ("response", object(vec![("code", N::Num(77.))])),
            ]),
            close_marker: object(vec![
                ("name", s("CloseError")),
                ("message", s("different return")),
            ]),
            dexes,
            fees: object(vec![("maker", N::Num(0.1)), ("taker", N::Num(0.2))]),
            meta: object(vec![("marker", s("meta"))]),
            assets: arr(vec![object(vec![("marker", s("asset"))])]),
            markets: arr(vec![s("core-m")]),
            tickers: arr(vec![s("core-t")]),
            settlements: vec![],
            iterators: vec![],
        })),
    };
    let e = h.e();
    h.set(&e, "infoXhr", &object(vec![("marker", s("info"))]))
        .unwrap();
    h.set(&e, "hip3DexNames", &arr(vec![s("sentinel")]))
        .unwrap();
    for k in [
        "fetchFee",
        "fetchHip3Dexs",
        "fetchSpotMarketsAndTickers",
        "withStartupTimeout",
        "fetchMarketsHelper",
        "fetchHip3Markets",
    ] {
        h.set(&e, k, &N::Builtin(k.into())).unwrap()
    }
    h
}
fn labels(v: &[N]) -> Vec<String> {
    v.iter()
        .map(|v| {
            let N::S(s) = get(v, "0") else {
                panic!("effect label")
            };
            String::from_utf16(&s.0).unwrap()
        })
        .collect()
}
#[test]
fn market_bootstrap_retains_native_partial_results_before_source_floor24() {
    let mut results = vec![];
    for scene in SCENES {
        let h = host(scene);
        let op = market_bootstrap::fetch_markets_and_tickers(h.clone(), h.e());
        let prefix = arr(h.state.borrow().effects.clone());
        assert_eq!(
            &labels(&h.state.borrow().effects)[..2],
            ["all-get", "core"],
            "native Promise callee before eager core"
        );
        let mut observer = Some(op);
        if scene == "drop-observer" {
            drop(observer.take());
        }
        h.base.drain();
        assert_eq!(
            h.state.borrow().settlements.len(),
            1,
            "native actual bootstrap settlement count"
        );
        let result = h.state.borrow().settlements[0].clone();
        if let Some(mut observer) = observer {
            let mut cx = Context::from_waker(Waker::noop());
            let Poll::Ready(observed) = Pin::new(&mut observer).poll(&mut cx) else {
                panic!("native observer unresolved")
            };
            assert_eq!(
                observed.is_err(),
                result.is_err(),
                "native observer rejection matches adopted settlement"
            )
        }
        let wanted = expected(scene);
        assert_eq!(
            result.is_err(),
            wanted.is_none(),
            "native bootstrap rejection {scene}"
        );
        if let Err(err) = &result {
            assert!(
                eq(err, &h.state.borrow().marker),
                "native bootstrap original error identity {scene}"
            )
        }
        if let Err(error) = &result {
            assert!(
                eq(&get(error, "name"), &s("BootstrapError")),
                "native original error name"
            );
            assert!(
                eq(
                    &get(error, "message"),
                    &N::S(JsString(
                        "bootstrap-failure".encode_utf16().chain([0xd800]).collect()
                    ))
                ),
                "native original error UTF16 message"
            );
            assert!(
                eq(&get(&get(error, "response"), "code"), &N::Num(77.)),
                "native original error response code"
            );
            assert!(
                !eq(error, &h.state.borrow().close_marker),
                "native body throw wins distinct iterator return throw"
            );
        }
        if let Some(roots) = wanted {
            let v = result.as_ref().unwrap();
            let markets = get(v, "0");
            let tickers = get(v, "1");
            let st = h.state.borrow();
            assert!(
                eq(&markets, &st.markets),
                "native original market array handle"
            );
            assert!(
                eq(&tickers, &st.tickers),
                "native original ticker array handle"
            );
            let N::A(m) = markets else { panic!("markets") };
            let N::A(t) = tickers else { panic!("tickers") };
            assert_eq!(
                m.borrow().len(),
                roots.len(),
                "native independent retained market count {scene}"
            );
            assert_eq!(
                t.borrow().len(),
                roots.len(),
                "native independent retained ticker count {scene}"
            );
            for (i, root) in roots.iter().enumerate() {
                assert!(
                    eq(&m.borrow()[i], &s(&format!("{root}-m"))),
                    "native retained market sequence {scene}/{i}"
                );
                assert!(
                    eq(&t.borrow()[i], &s(&format!("{root}-t"))),
                    "native distinct ticker sequence {scene}/{i}"
                )
            }
        }
        let names = get(&h.e(), "hip3DexNames");
        let N::A(names) = names else {
            panic!("native loaded names array")
        };
        let expected_names = if result.is_err() || ["core-only", "spot-only"].contains(&scene) {
            vec![s("sentinel")]
        } else {
            expected(scene)
                .unwrap()
                .into_iter()
                .filter(|name| !["core", "spot"].contains(name))
                .map(|name| {
                    if name == "a" && scene == "unicode-name" {
                        N::S(JsString(vec![97, 0xd800]))
                    } else {
                        s(name)
                    }
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names.borrow().len(),
            expected_names.len(),
            "native independent loaded dex count {scene}"
        );
        for (actual, wanted) in names.borrow().iter().zip(&expected_names) {
            assert!(
                eq(actual, wanted),
                "native independent loaded dex name {scene}"
            );
        }
        if ["all-success", "capped-budget", "minimum-budget"].contains(&scene) {
            let timeout = h
                .state
                .borrow()
                .effects
                .iter()
                .filter(|v| labels(std::slice::from_ref(v))[0] == "timeout")
                .nth(2)
                .cloned()
                .unwrap();
            let budget = if scene == "capped-budget" {
                100.
            } else if scene == "minimum-budget" {
                40.
            } else {
                60.
            };
            assert!(
                eq(&get(&timeout, "1"), &N::Num(budget)),
                "native independent bounded per-dex budget {scene}"
            );
        }
        let events = labels(&h.state.borrow().effects);
        let closes = usize::from(
            [
                "timeout-first",
                "timeout-second",
                "deadline-empty",
                "deadline-after-first",
                "fetch-sync-error",
                "timeout-error",
                "close-error",
                "failed-then-timeout",
                "body-and-close-error",
            ]
            .contains(&scene),
        );
        assert_eq!(
            events.iter().filter(|s| s.as_str() == "dex-close").count(),
            closes,
            "native independent iterator close count {scene}"
        );
        if scene == "core-logger" {
            let log = h
                .state
                .borrow()
                .effects
                .iter()
                .find(|v| labels(std::slice::from_ref(v))[0] == "log")
                .cloned()
                .unwrap();
            assert!(
                eq(&get(&log, "1"), &s("core diagnostic")),
                "native core logger original message"
            );
            assert!(
                eq(&get(&log, "2"), &s("info")),
                "native core logger default severity"
            )
        }
        results.push((scene, h, result, prefix));
    }
    // Direct owned callback values are asserted before loading any source artifact.
    let mut h = host("callbacks");
    let dex = get(&h.state.borrow().dexes, "0");
    let cb = MarketBootstrapCallback::DexName;
    let name =
        market_bootstrap::run_market_bootstrap_callback(&mut h, &cb, std::slice::from_ref(&dex))
            .unwrap();
    assert!(
        eq(&name, &get(&dex, "name")),
        "native bootstrap callback original dex name"
    );
    let loaded = arr(vec![s("a")]);
    let failed = arr(vec![s("b")]);
    for (name, wanted) in [("a", false), ("b", false), ("c", true)] {
        let cb = MarketBootstrapCallback::Unreached {
            loaded: loaded.clone(),
            failed: failed.clone(),
        };
        let result =
            market_bootstrap::run_market_bootstrap_callback(&mut h, &cb, &[s(name)]).unwrap();
        assert!(
            eq(&result, &N::B(wanted)),
            "native unreached independently excludes loaded and failed"
        )
    }
    let logger = h
        .bootstrap_callback(MarketBootstrapCallback::CoreLog { exchange: h.e() })
        .unwrap();
    h.call(&logger, &N::U, &[s("message"), N::Null]).unwrap();
    let last = h.state.borrow().effects.last().cloned().unwrap();
    assert!(
        matches!(get(&last, "2"), N::Null),
        "native core logger null does not default"
    );
    assert_eq!(
        results.len(),
        24,
        "native independent completed bootstrap floor"
    );
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/market-bootstrap-oracle.cjs"
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
    for (i, (scene, h, result, prefix)) in results.iter().enumerate() {
        let row = &oracle["cases"][i];
        assert_eq!(row["scene"], *scene);
        assert_value(
            prefix,
            &row["prefix"],
            &format!("native bootstrap eager prefix {scene}"),
        );
        assert_value(
            &arr(h.state.borrow().effects.clone()),
            &row["effects"],
            &format!("native bootstrap effects {scene}"),
        );
        assert_eq!(result.is_err(), row["rejected"].as_bool().unwrap());
        assert_value(
            result.as_ref().unwrap_or_else(|v| v),
            if result.is_err() {
                &row["error"]
            } else {
                &row["result"]
            },
            &format!("native bootstrap result {scene}"),
        );
        assert_value(
            &get(&h.e(), "hip3DexNames"),
            &row["names"],
            &format!("native bootstrap loaded dex names {scene}"),
        );
    }
}
