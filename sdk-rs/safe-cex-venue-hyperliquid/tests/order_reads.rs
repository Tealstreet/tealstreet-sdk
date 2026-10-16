#[allow(dead_code)]
#[path = "support/spot_account_host.rs"]
mod account;
#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[allow(dead_code)]
#[path = "support/rest_host.rs"]
mod native;
#[allow(dead_code)]
#[path = "support/order_read_host.rs"]
mod support;
use account::get;
use cache::{N, arr, eq, object, s};
use safe_cex_venue_hyperliquid::{
    order_reads::*, spot_account_reads::SpotAccountHost, spot_json::JsString,
    ws_ping_lifecycle::WsPingHost,
};
use std::{
    cell::RefCell,
    collections::HashMap,
    future::Future,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
use support::{Host, State};
const INTERNAL: [&str; 24] = [
    "ordinary",
    "empty-core",
    "core-nonarray",
    "core-error",
    "core-parse-error",
    "dex-error",
    "dex-nonarray",
    "dex-empty",
    "dex-parse-error",
    "store-swap",
    "suppression-recent",
    "suppression-boundary",
    "suppression-undefined",
    "raw-observed",
    "changed-status",
    "changed-price",
    "changed-amount",
    "changed-filled",
    "changed-type",
    "changed-tif",
    "warning-error",
    "logger-error",
    "cancel-binding-error",
    "cancel-next-error",
];
const FETCH: [&str; 10] = [
    "new",
    "concurrent",
    "existing",
    "changed-inflight",
    "replacement",
    "reject",
    "sync-error",
    "finally-get-error",
    "finally-set-error",
    "drop-observer",
];
const REFRESH: [&str; 4] = ["fulfilled", "reject", "sync-error", "drop-observer"];
fn array(v: &N) -> Vec<N> {
    let N::A(a) = v else {
        panic!("native array discriminant {v:?}")
    };
    a.borrow().clone()
}
fn map(v: &N) -> Vec<(N, N)> {
    let N::Map(a) = v else {
        panic!("native map discriminant")
    };
    a.borrow().clone()
}
fn idlist(v: &N) -> Vec<String> {
    array(v)
        .iter()
        .map(|v| {
            let N::S(s) = get(v, "id") else {
                panic!("native ID discriminant")
            };
            String::from_utf16_lossy(&s.0)
        })
        .collect()
}
fn row(id: &str, status: &str) -> N {
    object(vec![
        ("id", s(id)),
        (
            "symbol",
            s(if id == "hip-a" {
                "HIP-A"
            } else if id == "hip-b" {
                "HIP-B"
            } else {
                "CORE"
            }),
        ),
        ("status", s(status)),
        ("price", N::Num(33.)),
        ("amount", N::Num(5.)),
        ("filled", N::Num(1.)),
        ("type", s("Limit")),
        ("timeInForce", s("Gtc")),
    ])
}
fn raw(id: &str, row: N) -> N {
    object(vec![("oid", s(id)), ("row", row)])
}
fn fixture(family: &str, scene: &str) -> Host {
    let enums = object(vec![
        (
            "LogSeverity",
            object(vec![
                ("Debug", s("Debug")),
                ("Info", s("Info")),
                ("Warning", s("Warning")),
                ("Error", s("Error")),
            ]),
        ),
        (
            "OrderStatus",
            object(vec![
                ("Creating", s("Creating")),
                ("Updating", s("Updating")),
            ]),
        ),
    ]);
    let base = account::Host::new(object(vec![]), enums, object(vec![("INFO", s("info"))]));
    let a = row("a", "Open");
    let b = row("b", "Open");
    let c = row("c", "Open");
    let d = row("d", "Open");
    let duplicate = row("c", "DuplicateDex");
    let initial = arr(vec![
        row("a", "Open"),
        row("b", "Open"),
        row("gone", "Open"),
        row("creating", "Creating"),
        row("updating", "Updating"),
        row("hip-a", "Open"),
        row("hip-b", "Open"),
    ]);
    let swap = array(&initial);
    let mut swap = swap;
    swap.push(row("ws", "Open"));
    let swapped = arr(swap);
    let core = if matches!(scene, "empty-core") {
        arr(vec![])
    } else if scene == "core-nonarray" {
        N::Null
    } else if scene == "raw-observed" {
        arr(vec![
            object(vec![("oid", N::Num(0.))]),
            object(vec![("oid", s(""))]),
            object(vec![("oid", N::Null)]),
            object(vec![]),
            N::Null,
            N::U,
            N::Num(0.),
            s("primitive"),
            N::Builtin("raw-function".into()),
        ])
    } else {
        arr(vec![raw("a", a), raw("b", b.clone()), raw("c", c)])
    };
    let dex = if scene == "dex-nonarray" {
        N::B(false)
    } else if scene == "dex-empty" {
        arr(vec![])
    } else {
        arr(vec![raw("d", d), raw("c", duplicate)])
    };
    let marker = object(vec![
        ("name", s("Error")),
        ("message", N::S(JsString(vec![111, 111, 112, 115, 0xd800]))),
        ("response", object(vec![("code", N::Num(55.))])),
    ]);
    let final_error = object(vec![
        ("name", s("FinalError")),
        (
            "message",
            N::S(JsString(vec![102, 105, 110, 97, 108, 0xdfff])),
        ),
        ("response", object(vec![("code", N::Num(77.))])),
    ]);
    for value in [&marker, &final_error] {
        let N::O(o) = value else { unreachable!() };
        for (k, _, e) in &mut o.borrow_mut().fields {
            if k == "name" || k == "message" {
                *e = false
            }
        }
    }
    let fetch_result = arr(vec![row("result", "Open")]);
    let st = Rc::new(RefCell::new(State {
        scene: scene.into(),
        family: family.into(),
        effects: vec![],
        callbacks: HashMap::new(),
        promises: vec![],
        posted: vec![],
        marker,
        final_error,
        core,
        dex,
        initial: initial.clone(),
        swapped,
        fetch_result,
        flight: N::U,
        replacement: N::U,
        fetch_calls: 0,
        closes: 0,
        sets: 0,
        settlements: vec![],
        flight_reads: 0,
        cancel_iterator: N::U,
        collected: N::U,
    }));
    let mut h = Host { base, st };
    let e = h.e();
    let fields = [
        ("filled", N::Num(2.)),
        ("status", s("Closed")),
        ("price", N::Num(34.)),
        ("amount", N::Num(6.)),
        ("type", s("StopLoss")),
        ("timeInForce", s("Ioc")),
    ];
    let field = match scene {
        "changed-status" => "status",
        "changed-price" => "price",
        "changed-amount" => "amount",
        "changed-type" => "type",
        "changed-tif" => "timeInForce",
        _ => "filled",
    };
    let value = fields.iter().find(|(k, _)| *k == field).unwrap().1.clone();
    h.base.set(&b, field, &value).unwrap();
    let store = get(&e, "store");
    for (k, v) in [
        ("orders", initial),
        (
            "marketsBySymbol",
            object(vec![
                (
                    "HIP-A",
                    object(vec![("info", object(vec![("hip3Dex", s("dex-a"))]))]),
                ),
                (
                    "HIP-B",
                    object(vec![("info", object(vec![("hip3Dex", s("DEX-B"))]))]),
                ),
            ]),
        ),
        ("addOrder", N::Builtin("addOrder".into())),
        ("updateOrder", N::Builtin("updateOrder".into())),
        ("removeOrder", N::Builtin("removeOrder".into())),
    ] {
        h.base.set(&store, k, &v).unwrap()
    }
    let cancel = if scene == "raw-observed" {
        vec![
            (s("0"), N::Num(1.)),
            (s(""), N::Num(1.)),
            (s("gone"), N::Num(1.)),
        ]
    } else if scene.starts_with("suppression-") {
        vec![
            (
                s("c"),
                if scene == "suppression-undefined" {
                    N::U
                } else {
                    N::Num(if scene == "suppression-recent" {
                        99999.
                    } else {
                        70000.
                    })
                },
            ),
            (s("gone"), N::Num(1.)),
        ]
    } else {
        vec![(s("gone"), N::Num(1.))]
    };
    for (k, v) in [
        (
            "hip3DexNames",
            if scene == "raw-observed" {
                arr(vec![])
            } else {
                arr(vec![s("dex-a"), s("DEX-B")])
            },
        ),
        ("_cancelledOrders", N::Map(Rc::new(RefCell::new(cancel)))),
        ("CANCEL_SUPPRESSION_TRUST_WINDOW_MS", N::Num(30000.)),
        ("parseOpenOrders", N::Builtin("parseOpenOrders".into())),
        (
            "unsuppressCancelledOrder",
            N::Builtin("unsuppressCancelledOrder".into()),
        ),
        (
            "fetchOrdersInternal",
            N::Builtin("fetchOrdersInternal".into()),
        ),
        ("fetchOrders", N::Builtin("fetchOrders".into())),
        ("_fetchOrdersInFlight", N::Null),
    ] {
        h.put(k, &v)
    }
    let replacement = h.ready(Ok(arr(vec![row("replacement", "Open")])));
    h.st.borrow_mut().replacement = replacement;
    if scene == "existing" || scene == "changed-inflight" {
        let result = h.st.borrow().fetch_result.clone();
        let flight = h.ready(Ok(result));
        h.st.borrow_mut().flight = flight.clone();
        h.put("_fetchOrdersInFlight", &flight)
    }
    h
}
fn drive<F: Future<Output = Result<N, N>>>(h: &Host, f: F) -> Result<N, N> {
    let mut f = Box::pin(f);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..100 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        h.base.drain()
    }
    panic!("native finite order-read driver")
}
fn texts(parts: &[&str]) -> N {
    let mut units = parts.concat().encode_utf16().collect::<Vec<_>>();
    units.push(0xd800);
    N::S(JsString(units))
}
fn expected_logs(scene: &str) -> Vec<(N, N)> {
    let mut out: Vec<(N, N)> = vec![];
    let plain = |text: &str, severity: &str| (s(text), s(severity));
    let error = |prefix: &str, severity: &str| (texts(&[prefix, "Error: oops"]), s(severity));
    if scene == "raw-observed" {
        return out;
    }
    if scene == "core-error" || scene == "core-parse-error" {
        return vec![error("Failed to fetch orders: ", "Error")];
    }
    if scene == "logger-error" {
        return vec![
            plain("Fetching orders from HIP-3 dex: dex-a", "Debug"),
            error("Failed to fetch orders from HIP-3 dex dex-a: ", "Warning"),
            error("Failed to fetch orders: ", "Error"),
        ];
    }
    out.push(plain("Fetching orders from HIP-3 dex: dex-a", "Debug"));
    out.push(plain(
        if scene == "dex-nonarray" || scene == "dex-empty" {
            "HIP-3 dex dex-a returned 0 orders"
        } else {
            "HIP-3 dex dex-a returned 2 orders"
        },
        "Debug",
    ));
    if scene == "dex-parse-error" {
        out.push(error(
            "Failed to fetch orders from HIP-3 dex dex-a: ",
            "Warning",
        ))
    } else if scene != "dex-nonarray" && scene != "dex-empty" {
        out.push(plain(
            "Successfully parsed 2 orders from HIP-3 dex: dex-a",
            "Info",
        ))
    }
    out.push(plain("Fetching orders from HIP-3 dex: DEX-B", "Debug"));
    if scene == "dex-error" || scene == "warning-error" {
        out.push(error(
            "Failed to fetch orders from HIP-3 dex DEX-B: ",
            "Warning",
        ))
    } else {
        out.push(plain("HIP-3 dex DEX-B returned 0 orders", "Debug"))
    }
    if scene == "suppression-boundary" {
        out.push(plain(
            "Cancel suppression expired for order c — REST still shows it as open, un-suppressing",
            "Warning",
        ))
    }
    if matches!(
        scene,
        "warning-error" | "cancel-binding-error" | "cancel-next-error"
    ) {
        out.push(error("Failed to fetch orders: ", "Error"))
    }
    out
}
fn assert_internal(h: &Host, scene: &str, result: &Result<N, N>) {
    if scene == "logger-error" {
        assert!(
            eq(result.as_ref().unwrap_err(), &h.st.borrow().marker),
            "native original outer logger rejection"
        )
    } else {
        assert!(result.is_ok(), "native internal handled error {scene}")
    }
    let empty = matches!(
        scene,
        "core-error"
            | "core-parse-error"
            | "warning-error"
            | "cancel-binding-error"
            | "cancel-next-error"
            | "raw-observed"
    );
    if let Ok(result) = result {
        let ids = if empty {
            vec![]
        } else if scene == "empty-core" || scene == "core-nonarray" {
            vec!["d", "c"]
        } else if scene == "dex-nonarray" || scene == "dex-empty" || scene == "dex-parse-error" {
            vec!["a", "b", "c"]
        } else {
            vec!["a", "b", "c", "d"]
        };
        assert_eq!(
            idlist(result),
            ids,
            "native independent returned ID floor {scene}"
        );
        if !empty && !matches!(scene, "empty-core" | "core-nonarray") {
            let core = h.st.borrow().core.clone();
            let rows = array(result);
            for (i, row) in rows.iter().take(3).enumerate() {
                assert!(
                    eq(row, &get(&array(&core)[i], "row")),
                    "native original first-wins core row {scene}/{i}"
                )
            }
        }
        if empty && scene != "raw-observed" {
            assert!(
                !eq(result, &h.st.borrow().collected),
                "native fresh catch array {scene}"
            )
        }
    }
    let effects = h.st.borrow().effects.clone();
    let logs: Vec<_> = effects
        .iter()
        .filter(|v| eq(&get(v, "0"), &s("log")))
        .collect();
    let expected = expected_logs(scene);
    assert_eq!(
        logs.len(),
        expected.len(),
        "native independent log floor {scene}"
    );
    for (log, (message, severity)) in logs.iter().zip(expected) {
        assert!(
            eq(&get(log, "1"), &message),
            "native full UTF16 log message {scene}"
        );
        assert!(eq(&get(log, "2"), &severity), "native log severity {scene}");
        assert_eq!(array(log).len(), 3, "native exact log arity {scene}")
    }
    let action_ids = |kind: &str| -> Vec<String> {
        effects
            .iter()
            .filter(|v| eq(&get(v, "0"), &s(kind)))
            .map(|v| {
                let N::S(t) = get(&get(v, "1"), "id") else {
                    panic!("native action ID")
                };
                String::from_utf16_lossy(&t.0)
            })
            .collect()
    };
    let no_actions = matches!(
        scene,
        "core-error" | "core-parse-error" | "warning-error" | "logger-error"
    );
    let added = if no_actions || scene == "raw-observed" {
        vec![]
    } else if scene == "suppression-recent" {
        vec!["d"]
    } else if scene == "dex-nonarray" || scene == "dex-empty" || scene == "dex-parse-error" {
        vec!["c"]
    } else if scene == "empty-core" || scene == "core-nonarray" {
        vec!["d", "c"]
    } else {
        vec!["c", "d"]
    };
    assert_eq!(
        action_ids("add"),
        added,
        "native exact added order IDs {scene}"
    );
    let updated = if no_actions
        || scene == "raw-observed"
        || scene == "empty-core"
        || scene == "core-nonarray"
    {
        vec![]
    } else {
        vec!["b"]
    };
    assert_eq!(
        action_ids("update"),
        updated,
        "native exact updated order IDs {scene}"
    );
    let removed = if no_actions {
        vec![]
    } else if scene == "raw-observed" {
        vec!["a", "b", "gone"]
    } else if scene == "empty-core" || scene == "core-nonarray" {
        vec!["a", "b", "gone", "hip-a", "hip-b"]
    } else if scene == "dex-error" {
        vec!["gone", "hip-a"]
    } else if scene == "store-swap" {
        vec!["gone", "hip-a", "hip-b", "ws"]
    } else {
        vec!["gone", "hip-a", "hip-b"]
    };
    assert_eq!(
        action_ids("remove"),
        removed,
        "native exact removed IDs and failed-DEX preservation {scene}"
    );
    if !no_actions && !matches!(scene, "raw-observed" | "empty-core" | "core-nonarray") {
        let current = get(&get(&h.e(), "store"), "orders");
        assert!(
            eq(&array(&current)[0], &array(&h.st.borrow().initial)[0]),
            "native unchanged store order alias {scene}"
        )
    }
    let cancelled = map(&get(&h.e(), "_cancelledOrders"));
    let expected_cancel =
        if no_actions || scene == "cancel-binding-error" || scene == "cancel-next-error" {
            vec!["gone"]
        } else if scene == "raw-observed" {
            vec!["0", ""]
        } else if scene == "suppression-recent" || scene == "suppression-undefined" {
            vec!["c"]
        } else {
            vec![]
        };
    let keys: Vec<String> = cancelled
        .iter()
        .map(|(k, _)| {
            let N::S(t) = k else {
                panic!("native cancel key")
            };
            String::from_utf16_lossy(&t.0)
        })
        .collect();
    assert_eq!(
        keys, expected_cancel,
        "native raw observed cancellation keys {scene}"
    );
    assert_eq!(
        h.st.borrow().closes,
        usize::from(matches!(
            scene,
            "cancel-binding-error" | "warning-error" | "logger-error"
        )),
        "native outer iterator close boundaries {scene}"
    );
    let expected_dexes = if matches!(
        scene,
        "core-error" | "core-parse-error" | "logger-error" | "raw-observed"
    ) {
        vec![N::U]
    } else {
        vec![N::U, s("dex-a"), s("DEX-B")]
    };
    assert_eq!(
        h.st.borrow().posted.len(),
        expected_dexes.len(),
        "native independent request floor {scene}"
    );
    for (request, expected_dex) in h.st.borrow().posted.iter().zip(expected_dexes) {
        assert!(
            eq(&get(&request[1], "dex"), &expected_dex),
            "native exact request DEX order {scene}"
        );
    }
    for effect in &effects {
        if eq(&get(effect, "0"), &s("update")) {
            let core = h.st.borrow().core.clone();
            assert!(
                eq(&get(effect, "2"), &get(&array(&core)[1], "row")),
                "native original update payload handle {scene}"
            );
            let selector = get(effect, "1");
            let N::O(o) = selector else {
                panic!("native selector object")
            };
            assert_eq!(
                o.borrow().fields.len(),
                1,
                "native selector own field floor"
            );
        }
    }
    for request in &h.st.borrow().posted {
        assert_eq!(request.len(), 3, "native post arity");
        assert!(eq(&request[0], &s("info")), "native endpoint");
        assert!(
            eq(&get(&request[1], "type"), &s("frontendOpenOrders")),
            "native request type"
        );
        assert!(
            eq(&get(&request[1], "user"), &s("vault")),
            "native original vault user"
        );
        assert!(
            eq(
                &get(&get(&request[2], "headers"), "Content-Type"),
                &s("application/json")
            ),
            "native request headers"
        )
    }
}
fn state(h: &Host, result: &Result<N, N>, dropped: bool) -> N {
    let failed = result.is_err();
    let result = if dropped {
        N::U
    } else {
        result.as_ref().cloned().unwrap_or(N::U)
    };
    let effects = h.st.borrow().effects.clone();
    object(vec![
        ("result", result),
        ("rejected", N::B(!dropped && failed)),
        ("effects", arr(effects)),
        ("orders", get(&get(&h.e(), "store"), "orders")),
        (
            "cancelled",
            arr(map(&get(&h.e(), "_cancelledOrders"))
                .into_iter()
                .map(|(k, v)| arr(vec![k, v]))
                .collect()),
        ),
        (
            "flightNull",
            N::B(matches!(get(&h.e(), "_fetchOrdersInFlight"), N::Null)),
        ),
        ("fetchCalls", N::Num(h.st.borrow().fetch_calls as f64)),
    ])
}
#[test]
fn native_order_reads_before_source() {
    let mut rows = vec![];
    let mut failures = vec![];
    for (family, scenes) in [
        ("internal", INTERNAL.as_slice()),
        ("fetch", FETCH.as_slice()),
        ("refresh", REFRESH.as_slice()),
    ] {
        for scene in scenes {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut h = fixture(family, scene);
                let e = h.e();
                let dropped = *scene == "drop-observer";
                let result = if family == "internal" {
                    let f = fetch_orders_internal(&h, &e);
                    drive(&h, f)
                } else if family == "fetch" {
                    let f = fetch_orders(&h, &e, &N::U);
                    if *scene == "concurrent" {
                        let second = fetch_orders(&h, &e, &arr(vec![s("ignored")]));
                        let a = drive(&h, f).unwrap();
                        let b = drive(&h, second).unwrap();
                        assert!(eq(&a, &b), "native concurrent original response handle");
                        Ok(a)
                    } else if dropped {
                        drop(f);
                        h.base.drain();
                        assert_eq!(
                            h.st.borrow().settlements.len(),
                            1,
                            "native independent dropped fetch settlement floor"
                        );
                        h.st.borrow().settlements.last().cloned().unwrap()
                    } else {
                        drive(&h, f)
                    }
                } else {
                    match refresh_orders(&mut h, &e) {
                        Ok(v) => {
                            if dropped {
                                h.base.drain();
                                h.base.native_result(&v)
                            } else {
                                let f = h.await_value(v);
                                drive(&h, f)
                            }
                        }
                        Err(e) => Err(e),
                    }
                };
                if family == "internal" {
                    assert_internal(&h, scene, &result)
                } else {
                    let error = matches!(
                        *scene,
                        "reject" | "sync-error" | "finally-get-error" | "finally-set-error"
                    );
                    assert_eq!(
                        result.is_err(),
                        error,
                        "native wrapper rejection branch {family}/{scene}"
                    );
                    if error {
                        let expected = if scene.starts_with("finally-") {
                            h.st.borrow().final_error.clone()
                        } else {
                            h.st.borrow().marker.clone()
                        };
                        assert!(
                            eq(result.as_ref().unwrap_err(), &expected),
                            "native original/finally rejection identity {family}/{scene}"
                        );
                        let expected_code = if scene.starts_with("finally-") {
                            77.
                        } else {
                            55.
                        };
                        assert!(
                            eq(
                                &get(&get(&expected, "response"), "code"),
                                &N::Num(expected_code)
                            ),
                            "native rejection full response field"
                        )
                    } else if family == "refresh" {
                        assert!(
                            matches!(result, Ok(N::U)),
                            "native refresh callback discards opaque resolved rows"
                        )
                    } else {
                        let expected = if *scene == "changed-inflight" {
                            vec!["replacement"]
                        } else {
                            vec!["result"]
                        };
                        assert_eq!(
                            idlist(result.as_ref().unwrap()),
                            expected,
                            "native independent wrapper result ID floor {scene}"
                        );
                        if *scene == "changed-inflight" {
                            let expected =
                                h.base.native_result(&h.st.borrow().replacement).unwrap();
                            assert!(
                                eq(result.as_ref().unwrap(), &expected),
                                "native second getter original replacement response handle"
                            );
                        }
                        if *scene != "changed-inflight" {
                            assert!(
                                eq(result.as_ref().unwrap(), &h.st.borrow().fetch_result),
                                "native wrapper original response alias {scene}"
                            )
                        }
                    }
                    assert_eq!(
                        h.st.borrow().fetch_calls,
                        if family == "fetch" && matches!(*scene, "existing" | "changed-inflight") {
                            0
                        } else {
                            1
                        },
                        "native independent request coalescing count {family}/{scene}"
                    );
                    if family == "fetch" {
                        let flight = get(&e, "_fetchOrdersInFlight");
                        let cleared = matches!(
                            *scene,
                            "new" | "concurrent" | "reject" | "sync-error" | "drop-observer"
                        );
                        assert_eq!(
                            matches!(flight, N::Null),
                            cleared,
                            "native exact flight cleanup guard {scene}"
                        );
                        if *scene == "replacement" {
                            assert!(
                                eq(&flight, &h.st.borrow().replacement),
                                "native replacement flight preserved"
                            )
                        }
                    }
                }
                (family, *scene, state(&h, &result, dropped))
            }));
            match outcome {
                Ok(row) => rows.push(row),
                Err(error) => {
                    let text = error
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| error.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_default();
                    failures.push(format!("{family}/{scene}: {text}"));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "native scene failures before source: {failures:?}"
    );
    assert_eq!(rows.len(), 38, "native completed order-read floor");
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/order-reads-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source oracle failed {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let source = value["cases"].as_array().unwrap();
    assert_eq!(source.len(), 38, "independent source order-read floor");
    assert_eq!(value["last_index"], 37);
    for ((family, scene, native), source) in rows.iter().zip(source) {
        assert_eq!(source["family"], *family);
        assert_eq!(source["scene"], *scene);
        native::assert_value(
            native,
            &source["state"],
            &format!("supplemental source {family}/{scene}"),
        )
    }
}
