//! Real canonical owner/facade operations against maintained Store witnesses.
use safe_cex_exchange::{
    js_runtime::{
        globals,
        value::{Js, Thrown, from_json},
    },
    native_store::DefaultStore,
};
use safe_cex_types::StoreReadPhase;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::{Rc, Weak},
};

fn source() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_store_live_view_source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn fixture() -> (Rc<DefaultStore>, Js) {
    let native = Rc::new(DefaultStore::new());
    let facade = safe_cex_exchange::js_runtime::store::facade(native.clone()).unwrap();
    facade.call_method("update", vec![from_json(&json!({
        "markets":[{"id":"m","symbol":"BTCUSD","base":"BTC","quote":"USD","active":true,
            "precision":{"amount":1,"price":0.5},"limits":{"amount":{"min":1,"max":1000},"leverage":{"min":1,"max":100}}}],
        "tickers":[{"id":"t","symbol":"BTCUSD","bid":1,"ask":2,"last":1,"mark":1,"index":1,
            "percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0}]
    }))]).unwrap();
    (native, facade)
}

#[test]
fn native_graph_retains_raw_rows_and_all_four_live_dictionary_aliases() {
    let expected = source();
    let (_, facade) = fixture();
    for case in expected["identity"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let market = name.starts_with("markets");
        let key = if name.ends_with("BySymbol") {
            "BTCUSD"
        } else if market {
            "m"
        } else {
            "t"
        };
        let table = facade.get(name).unwrap();
        let item = table.get(key).unwrap();
        let field = if market { "active" } else { "bid" };
        let value = if market {
            Js::Bool(false)
        } else {
            Js::Number(7.0)
        };
        facade
            .call_method(
                if market {
                    "updateMarket"
                } else {
                    "updateTicker"
                },
                vec![
                    Js::object(vec![("id", Js::str(if market { "m" } else { "t" }))]),
                    Js::object(vec![(field, value.clone())]),
                ],
            )
            .unwrap();
        assert!(
            item.get(field).unwrap().strict_equals(&value),
            "retained {name} row must see granular write"
        );
        assert!(table.same_reference(&facade.get(name).unwrap()));
        assert!(item.same_reference(&table.get(key).unwrap()));
        assert!(
            item.same_reference(
                &facade
                    .get(if market { "markets" } else { "tickers" })
                    .unwrap()
                    .get("0")
                    .unwrap()
            )
        );
    }
    let row = Js::object(vec![
        ("id", Js::str("raw")),
        ("symbol", Js::str("raw")),
        ("opaque", Js::utf16(&[0xd800])),
    ]);
    let rows = Js::array(vec![row.clone()]);
    facade
        .call_method("update", vec![Js::object(vec![("markets", rows.clone())])])
        .unwrap();
    assert!(facade.get("markets").unwrap().same_reference(&rows));
    assert!(
        facade
            .get("marketsById")
            .unwrap()
            .get("raw")
            .unwrap()
            .same_reference(&row)
    );
}

#[test]
fn captured_balance_is_lazy_retains_generation_and_does_not_retain_store_owner() {
    assert_eq!(source()["balances"].as_array().unwrap().len(), 2);
    let (native, facade) = fixture();
    let owner = Rc::downgrade(&native);
    let balance = facade.get("balance").unwrap();
    let handle = native.balance_handle().unwrap();
    let native_handle = handle.native_value().downcast::<Js>().unwrap();
    assert!(native_handle.same_reference(&balance));
    balance.set("total", Js::Number(-0.0)).unwrap();
    assert_eq!(handle.read().unwrap().total.to_bits(), (-0.0_f64).to_bits());
    facade
        .call_method(
            "update",
            vec![Js::object(vec![(
                "balance",
                Js::object(vec![
                    ("used", Js::Number(1.0)),
                    ("free", Js::Number(2.0)),
                    ("total", Js::Number(3.0)),
                    ("upnl", Js::Number(4.0)),
                ]),
            )])],
        )
        .unwrap();
    assert!(!facade.get("balance").unwrap().same_reference(&balance));
    let new_handle = native.balance_handle().unwrap();
    assert_eq!(new_handle.read().unwrap().total, 3.0);
    let cause = Js::object(vec![("code", Js::utf16(&[0xd800]))]);
    let thrown = cause.clone();
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    balance
        .define_accessor(
            "total",
            Some(Rc::new(move |_| {
                count.set(count.get() + 1);
                Err(Thrown::new(thrown.clone()))
            })),
            None,
        )
        .unwrap();
    assert_eq!(
        calls.get(),
        0,
        "capturing/downcasting must not project a balance"
    );
    facade.call_method("reset", vec![]).unwrap();
    drop(facade);
    drop(native);
    assert!(owner.upgrade().is_none());
    assert_eq!(new_handle.read().unwrap().total, 3.0);
    for expected_calls in 1..=2 {
        let error = handle.read().unwrap_err();
        assert_eq!(error.phase(), StoreReadPhase::Projection);
        assert!(
            error
                .native_cause()
                .downcast_ref::<Thrown>()
                .unwrap()
                .value
                .same_reference(&cause)
        );
        assert_eq!(calls.get(), expected_calls);
    }
}

#[test]
fn frozen_map_failure_retains_prethrow_row_write_and_skips_notifications() {
    let expected = source();
    let globals = Js::object(vec![]);
    globals::install_builtins::<()>(&globals, Weak::new());
    for case in expected["integrity"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["operation"] == "freeze")
    {
        let (_, facade) = fixture();
        let name = case["name"].as_str().unwrap();
        let market = name.starts_with("markets");
        let key = if name.ends_with("BySymbol") {
            "BTCUSD"
        } else if market {
            "m"
        } else {
            "t"
        };
        let map = facade.get(name).unwrap();
        let row = map.get(key).unwrap();
        let notices = Rc::new(Cell::new(0));
        let count = notices.clone();
        facade
            .call_method(
                "subscribe",
                vec![Js::function("notice", move |_, _| {
                    count.set(count.get() + 1);
                    Ok(Js::Undefined)
                })],
            )
            .unwrap();
        let target = if case["subject"] == "map" {
            map
        } else {
            row.clone()
        };
        globals
            .get("Object")
            .unwrap()
            .call_method("freeze", vec![target])
            .unwrap();
        let error = facade
            .call_method(
                if market {
                    "updateMarket"
                } else {
                    "updateTicker"
                },
                vec![
                    Js::object(vec![("id", Js::str(if market { "m" } else { "t" }))]),
                    Js::object(vec![(
                        if market { "active" } else { "bid" },
                        if market {
                            Js::Bool(false)
                        } else {
                            Js::Number(7.0)
                        },
                    )]),
                ],
            )
            .unwrap_err();
        assert_eq!(error.value.get("name").unwrap().to_display(), "TypeError");
        let actual = row.get(if market { "active" } else { "bid" }).unwrap();
        assert!(
            actual.strict_equals(&from_json(&case["rowValue"])),
            "{name} {:?}",
            case["subject"]
        );
        assert_eq!(notices.get(), 0);
    }
}

#[test]
fn every_source_integrity_operation_retains_its_mutation_and_generation_order() {
    let expected = source();
    let globals = Js::object(vec![]);
    globals::install_builtins::<()>(&globals, Weak::new());
    let object = globals.get("Object").unwrap();
    for case in expected["integrity"].as_array().unwrap() {
        let (_, facade) = fixture();
        let name = case["name"].as_str().unwrap();
        let market = name.starts_with("markets");
        let id = if market { "m" } else { "t" };
        let key = if name.ends_with("BySymbol") {
            "BTCUSD"
        } else {
            id
        };
        let field = if market { "active" } else { "bid" };
        let value = if market {
            Js::Bool(false)
        } else {
            Js::Number(7.0)
        };
        let map = facade.get(name).unwrap();
        let row = map.get(key).unwrap();
        let notices = Rc::new(Cell::new(0));
        let count = notices.clone();
        facade
            .call_method(
                "subscribe",
                vec![Js::function("notice", move |_, _| {
                    count.set(count.get() + 1);
                    Ok(Js::Undefined)
                })],
            )
            .unwrap();
        object
            .call_method(
                case["operation"].as_str().unwrap(),
                vec![if case["subject"] == "map" {
                    map.clone()
                } else {
                    row.clone()
                }],
            )
            .unwrap();
        let mutation = facade.call_method(
            if market {
                "updateMarket"
            } else {
                "updateTicker"
            },
            vec![
                Js::object(vec![("id", Js::str(id))]),
                Js::object(vec![(field, value)]),
            ],
        );
        let failure_name = |error: Option<Thrown>| {
            error.map_or(Value::Null, |error| {
                Value::String(error.value.get("name").unwrap().to_display())
            })
        };
        assert_eq!(
            failure_name(mutation.err()),
            case["mutationError"],
            "mutation {case}"
        );
        assert_eq!(notices.get(), case["mutationNotices"].as_u64().unwrap());
        assert!(
            row.get(field)
                .unwrap()
                .strict_equals(&from_json(&case["rowValue"])),
            "row write {case}"
        );
        assert_eq!(
            failure_name(map.set("extra", row.clone()).err()),
            case["insertionError"],
            "insertion {case}"
        );
        assert_eq!(
            failure_name(map.delete(&key.encode_utf16().collect::<Vec<_>>()).err()),
            case["deleteError"],
            "deletion {case}"
        );
        let (_, fresh) = fixture();
        let field_name = if market { "markets" } else { "tickers" };
        facade
            .call_method(
                "update",
                vec![Js::object(vec![(
                    field_name,
                    fresh.get(field_name).unwrap(),
                )])],
            )
            .unwrap();
        assert_eq!(notices.get(), case["mutationNotices"].as_u64().unwrap() + 1);
        assert!(!facade.get(name).unwrap().same_reference(&map));
        assert!(
            !facade
                .get(name)
                .unwrap()
                .get(key)
                .unwrap()
                .same_reference(&row)
        );
        assert!(
            row.get(field)
                .unwrap()
                .strict_equals(&from_json(&case["rowValue"])),
            "retained generation {case}"
        );
    }
}

#[test]
fn ticker_normalization_spreads_before_reading_and_mapping_the_rhs() {
    let expected = source();
    for throws in [false, true] {
        let (_, facade) = fixture();
        let events = Rc::new(RefCell::new(Vec::<String>::new()));
        let row = Js::object(vec![("id", Js::str("t")), ("symbol", Js::str("BTCUSD"))]);
        let log = events.clone();
        row.define_accessor(
            "info",
            Some(Rc::new(move |_| {
                log.borrow_mut().push("row.info".into());
                Ok(Js::Undefined)
            })),
            None,
        )
        .unwrap();
        let changes = Js::object(vec![]);
        let log = events.clone();
        changes
            .define_accessor(
                "tickers",
                Some(Rc::new(move |_| {
                    log.borrow_mut().push("changes.tickers".into());
                    Ok(Js::array(vec![row.clone()]))
                })),
                None,
            )
            .unwrap();
        let cause = Js::object(vec![("opaque", Js::utf16(&[0xd800]))]);
        let thrown = cause.clone();
        let log = events.clone();
        changes
            .define_accessor(
                "latency",
                Some(Rc::new(move |_| {
                    log.borrow_mut().push("changes.latency".into());
                    if throws {
                        Err(Thrown::new(thrown.clone()))
                    } else {
                        Ok(Js::Number(7.0))
                    }
                })),
                None,
            )
            .unwrap();
        let result = facade.call_method("update", vec![changes]);
        if throws {
            assert!(result.unwrap_err().value.same_reference(&cause));
        } else {
            result.unwrap();
        }
        let case = expected["ordering"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["case"] == "spread-before-strip" && case["throws"] == throws)
            .unwrap();
        assert_eq!(
            serde_json::to_value(&*events.borrow()).unwrap(),
            case["events"],
            "throwing latency must prevent row.info"
        );
    }
}

#[test]
fn removal_id_lookup_rechecks_selector_length_after_each_getter() {
    let expected = source();
    let (_, facade) = fixture();
    facade
        .call_method(
            "update",
            vec![Js::object(vec![(
                "orders",
                Js::array(vec![
                    Js::object(vec![("id", Js::str("a"))]),
                    Js::object(vec![("id", Js::str("b"))]),
                ]),
            )])],
        )
        .unwrap();
    let selector = Js::object(vec![]);
    let selectors = Js::array(vec![selector.clone()]);
    let captured = selectors.clone();
    let expanded = Cell::new(false);
    selector
        .define_accessor(
            "id",
            Some(Rc::new(move |_| {
                if !expanded.replace(true) {
                    captured.call_method("push", vec![Js::object(vec![("id", Js::str("b"))])])?;
                }
                Ok(Js::str("a"))
            })),
            None,
        )
        .unwrap();
    facade
        .call_method("removeOrders", vec![selectors.clone()])
        .unwrap();
    let case = expected["ordering"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["case"] == "dynamic-id-lookup-length")
        .unwrap();
    assert_eq!(
        facade
            .get("orders")
            .unwrap()
            .get("length")
            .unwrap()
            .as_f64(),
        Some(case["remaining"].as_array().unwrap().len() as f64)
    );
    assert_eq!(
        selectors.get("length").unwrap().as_f64(),
        case["selectorCount"].as_f64()
    );
}

#[test]
fn delta_loops_recheck_length_after_getters_append_a_second_delta() {
    let expected = source();
    for packed in [false, true] {
        let (_, facade) = fixture();
        let changes = Js::object(vec![]);
        let delta = if packed {
            Js::object(vec![("0", Js::Number(0.0)), ("1", Js::Number(1.0))])
        } else {
            Js::array(vec![Js::Number(0.0), changes.clone()])
        };
        let deltas = Js::array(vec![delta.clone()]);
        let captured = deltas.clone();
        (if packed { &delta } else { &changes })
            .define_accessor(
                if packed { "2" } else { "bid" },
                Some(Rc::new(move |_| {
                    if captured.get("length")?.as_f64() == Some(1.0) {
                        captured.call_method(
                            "push",
                            vec![if packed {
                                Js::array(vec![Js::Number(0.0), Js::Number(1.0), Js::Number(9.0)])
                            } else {
                                Js::array(vec![
                                    Js::Number(0.0),
                                    Js::object(vec![("bid", Js::Number(9.0))]),
                                ])
                            }],
                        )?;
                    }
                    Ok(Js::Number(7.0))
                })),
                None,
            )
            .unwrap();
        facade
            .call_method(
                if packed {
                    "applyPackedTickerDeltas"
                } else {
                    "applyTickerDeltas"
                },
                vec![deltas],
            )
            .unwrap();
        let name = if packed {
            "live-packed-length"
        } else {
            "live-legacy-length"
        };
        let case = expected["ordering"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["case"] == name)
            .unwrap();
        assert_eq!(
            facade
                .get("tickersById")
                .unwrap()
                .get("t")
                .unwrap()
                .get("bid")
                .unwrap()
                .as_f64(),
            case["bid"].as_f64()
        );
    }
}

#[test]
fn worker_patch_loop_rechecks_length_after_an_incoming_field_getter() {
    let expected = source();
    let (_, facade) = fixture();
    let a = Js::object(vec![
        ("id", Js::str("a")),
        ("symbol", Js::str("A")),
        ("active", Js::Bool(true)),
    ]);
    let b = Js::object(vec![
        ("id", Js::str("b")),
        ("symbol", Js::str("B")),
        ("active", Js::Bool(true)),
    ]);
    let current = Js::array(vec![a]);
    facade
        .call_method(
            "update",
            vec![Js::object(vec![("markets", current.clone())])],
        )
        .unwrap();
    let first = Js::object(vec![("id", Js::str("a")), ("symbol", Js::str("A"))]);
    let incoming = Js::array(vec![first.clone()]);
    let captured = incoming.clone();
    let second = b.clone();
    first
        .define_accessor(
            "active",
            Some(Rc::new(move |_| {
                if captured.get("length")?.as_f64() == Some(1.0) {
                    current.call_method("push", vec![second.clone()])?;
                    captured.call_method(
                        "push",
                        vec![Js::object(vec![
                            ("id", Js::str("b")),
                            ("symbol", Js::str("B")),
                            ("active", Js::Bool(false)),
                        ])],
                    )?;
                }
                Ok(Js::Bool(false))
            })),
            None,
        )
        .unwrap();
    facade
        .call_method(
            "update",
            vec![Js::object(vec![("markets", incoming)]), Js::Bool(true)],
        )
        .unwrap();
    let case = expected["ordering"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["case"] == "live-worker-patch-length")
        .unwrap();
    assert_eq!(
        b.get("active").unwrap().truthy(),
        case["active"].as_bool().unwrap()
    );
}

#[test]
fn position_id_index_rechecks_length_after_a_row_id_getter() {
    let expected = source();
    let (_, facade) = fixture();
    let first = Js::object(vec![("id", Js::str("a")), ("contracts", Js::Number(1.0))]);
    let positions = Js::array(vec![
        first.clone(),
        Js::object(vec![("id", Js::str("b")), ("contracts", Js::Number(1.0))]),
    ]);
    facade
        .call_method(
            "update",
            vec![Js::object(vec![("positions", positions.clone())])],
        )
        .unwrap();
    let captured = positions.clone();
    let appended = Cell::new(false);
    first
        .define_accessor(
            "id",
            Some(Rc::new(move |_| {
                if !appended.replace(true) {
                    captured.call_method(
                        "push",
                        vec![Js::object(vec![
                            ("id", Js::str("c")),
                            ("contracts", Js::Number(1.0)),
                        ])],
                    )?;
                }
                Ok(Js::str("a"))
            })),
            None,
        )
        .unwrap();
    facade
        .call_method(
            "updatePositions",
            vec![Js::array(vec![
                Js::array(vec![
                    Js::object(vec![("id", Js::str("c"))]),
                    Js::object(vec![("contracts", Js::Number(7.0))]),
                ]),
                Js::array(vec![
                    Js::object(vec![("id", Js::str("x"))]),
                    Js::object(vec![("contracts", Js::Number(8.0))]),
                ]),
            ])],
        )
        .unwrap();
    let case = expected["ordering"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["case"] == "live-id-index-length")
        .unwrap();
    assert_eq!(
        positions
            .get("2")
            .unwrap()
            .get("contracts")
            .unwrap()
            .as_f64(),
        case["contracts"].as_f64()
    );
}

#[test]
fn position_audit_reads_each_row_after_the_preceding_id_getter() {
    let expected = source();
    assert!(
        expected["ordering"]
            .as_array()
            .unwrap()
            .iter()
            .any(|case| case["case"] == "audit-reached-row-order")
    );
    let (_, facade) = fixture();
    let first = Js::object(vec![("contracts", Js::Number(1.0))]);
    let poison = Js::object(vec![("id", Js::str("b"))]);
    let cause = Js::object(vec![("opaque", Js::utf16(&[0xd800]))]);
    poison
        .define_accessor(
            "contracts",
            Some(Rc::new(move |_| Err(Thrown::new(cause.clone())))),
            None,
        )
        .unwrap();
    let replacement = Js::object(vec![("id", Js::str("c")), ("contracts", Js::Number(1.0))]);
    let positions = Js::array(vec![first.clone(), poison]);
    let captured = positions.clone();
    let next = replacement.clone();
    first
        .define_accessor(
            "id",
            Some(Rc::new(move |_| {
                captured.set("1", next.clone())?;
                Ok(Js::str("a"))
            })),
            None,
        )
        .unwrap();
    facade
        .call_method("update", vec![Js::object(vec![("positions", positions)])])
        .expect("audit must not visit the row replaced by the preceding getter");
    assert!(
        facade
            .get("positions")
            .unwrap()
            .get("1")
            .unwrap()
            .same_reference(&replacement)
    );
}
