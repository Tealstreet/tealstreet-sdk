#[path = "support/lifecycle.rs"]
mod fixture;
use fixture::*;
use safe_cex_venue_woo::{legacy_lifecycle::*, public_canonical::CanonicalOps};
use serde_json::json;
use std::{cell::RefCell, rc::Rc};
#[test]
fn startup_parallel_continuations_and_captured_policy_native() {
    let h = Rc::new(Runtime::default());
    let ops = h.operations();
    let r = receiver(&h);
    let markets = pending();
    let raw = pending();
    let balance = pending();
    let positions = pending();
    let orders = pending();
    for (k, v) in [
        ("initExternalPublicData", b(false)),
        ("fetchMarkets", markets.clone()),
        ("fetchTickersRaw", raw.clone()),
        ("fetchBalance", balance.clone()),
        ("fetchPositions", positions.clone()),
        ("fetchOrders", orders.clone()),
    ] {
        set(&r, k, h.bind(k, v));
    }
    let c = h.clone();
    set(
        &r,
        "parseTickers",
        function(move |_, a| {
            c.trace.borrow_mut().push("parse".into());
            Ok(a[0].clone())
        }),
    );
    let p = start(ops, r.clone());
    assert_trace(&h, &["initExternalPublicData"]);
    h.pump();
    assert_trace(
        &h,
        &["initExternalPublicData", "fetchMarkets", "fetchTickersRaw"],
    );
    let market_rows = decode(&json!([{"id":"market-11"}]));
    resolve(&markets, Ok(market_rows.clone()));
    h.pump();
    assert_trace(
        &h,
        &[
            "initExternalPublicData",
            "fetchMarkets",
            "fetchTickersRaw",
            "update:markets,loaded",
            "public",
        ],
    );
    set(&r, "isPublicDataOnly", b(true));
    let ticker_rows = decode(&json!([{"id":"ticker-13"}]));
    resolve(&raw, Ok(ticker_rows.clone()));
    h.pump();
    assert_trace(
        &h,
        &[
            "initExternalPublicData",
            "fetchMarkets",
            "fetchTickersRaw",
            "update:markets,loaded",
            "public",
            "parse",
            "update:tickers,loaded",
            "funding",
            "private",
            "fetchBalance",
            "fetchPositions",
            "fetchOrders",
        ],
    );
    let order_rows = decode(&json!([{"id":"order-17"}]));
    let balance_value = decode(&json!({"free":19}));
    let position_rows = decode(&json!([{"id":"position-23"}]));
    resolve(&orders, Ok(order_rows.clone()));
    h.pump();
    resolve(&balance, Ok(balance_value.clone()));
    h.pump();
    resolve(&positions, Ok(position_rows.clone()));
    assert!(matches!(outcome(&h, &p).unwrap().as_ref(), Js::U));
    let expected = [
        "initExternalPublicData",
        "fetchMarkets",
        "fetchTickersRaw",
        "update:markets,loaded",
        "public",
        "parse",
        "update:tickers,loaded",
        "funding",
        "private",
        "fetchBalance",
        "fetchPositions",
        "fetchOrders",
        "update:orders,loaded",
        "update:balance,loaded",
        "positions:update",
        "update:loaded",
        "tick",
        "log",
    ];
    assert_trace(&h, &expected);
    let store = get(&r, "store").unwrap();
    for (k, v) in [
        ("markets", market_rows),
        ("tickers", ticker_rows),
        ("orders", order_rows),
        ("balance", balance_value),
    ] {
        assert!(
            Rc::ptr_eq(&get(&store, k).unwrap(), &v),
            "LIFECYCLE-NATIVE startup alias {k}"
        );
    }
    let positions_call = h
        .args
        .borrow()
        .iter()
        .find(|(k, _)| k == "positions:update")
        .unwrap()
        .1
        .clone();
    assert!(
        Rc::ptr_eq(&positions_call[0], &position_rows),
        "LIFECYCLE-NATIVE positions reference"
    );
    assert_eq!(
        literal(&get(&store, "loaded").unwrap()),
        json!({"old":true,"markets":true,"tickers":true,"orders":true,"balance":true,"positions":true})
    );
    assert_eq!(
        literal(&h.args.borrow().iter().find(|(k, _)| k == "log").unwrap().1[0]),
        json!("Ready to trade on WOO X")
    );
    let source = source_cases(&[json!({"kind":"start","case":"parallel"})]);
    assert_eq!(source[0]["trace"], json!(expected));
}
#[test]
fn startup_non_cancellation_and_public_only_native() {
    let mut results = vec![];
    for case in ["rejected", "external-public-only", "disposed"] {
        let h = Rc::new(Runtime::default());
        let ops = h.operations();
        let r = receiver(&h);
        let markets = pending();
        let raw = pending();
        set(
            &r,
            "initExternalPublicData",
            h.bind("init", b(case == "external-public-only")),
        );
        set(&r, "fetchMarkets", h.bind("markets", markets.clone()));
        set(&r, "fetchTickersRaw", h.bind("raw", raw.clone()));
        set(&r, "isPublicDataOnly", b(true));
        let p = start(ops, r.clone());
        assert_trace(&h, &["init"]);
        h.pump();
        if case == "rejected" {
            resolve(&raw, Err(Rc::new("raw rejected".into())));
            assert_eq!(&*outcome(&h, &p).err().unwrap(), "raw rejected");
            resolve(&markets, Ok(arr(vec![s("late")])));
            h.pump();
            assert_trace(
                &h,
                &["init", "markets", "raw", "update:markets,loaded", "public"],
            );
        } else if case == "disposed" {
            set(&r, "isDisposed", b(true));
            resolve(&markets, Ok(arr(vec![])));
            resolve(&raw, Ok(arr(vec![])));
            assert!(outcome(&h, &p).is_ok());
            assert_trace(&h, &["init", "markets", "raw"]);
        } else {
            assert!(outcome(&h, &p).is_ok());
            assert_trace(&h, &["init", "update:loaded"]);
            let store = get(&r, "store").unwrap();
            assert_eq!(
                literal(&get(&store, "loaded").unwrap()),
                json!({"old":true,"orders":true,"positions":true,"balance":true})
            );
        }
        results.push(json!({"kind":"start","case":case,"trace":h.trace.borrow().clone()}));
    }
    let cases = results
        .iter()
        .map(|r| json!({"kind":r["kind"],"case":r["case"]}))
        .collect::<Vec<_>>();
    let source = source_cases(&cases);
    for (i, r) in results.iter().enumerate() {
        assert_eq!(source[i]["trace"], r["trace"]);
    }
}
#[test]
fn polling_modulo_catch_disposal_and_scheduling_native() {
    let mut observed = vec![];
    for case in [
        "zero",
        "one",
        "five",
        "string-five",
        "disposed-after-await",
        "fetch-reject",
        "loop-throw",
        "emit-throw",
    ] {
        let h = Rc::new(Runtime::default());
        let ops = h.operations();
        let r = receiver(&h);
        let positions = decode(&json!([{"id":"position-29"}]));
        let balance = decode(&json!({"free":31}));
        let result =
            if case == "disposed-after-await" || case == "fetch-reject" || case == "emit-throw" {
                pending()
            } else {
                obj(vec![
                    ("positions", positions.clone()),
                    ("balance", balance.clone()),
                ])
            };
        set(
            &r,
            "fetchPositionsAndBalance",
            h.bind("full", result.clone()),
        );
        set(&r, "fetchPositions", h.bind("positions", positions.clone()));
        let order_rows = decode(&json!([{"id":"order-37"}]));
        set(&r, "fetchOrders", h.bind("orders", order_rows.clone()));
        let c = h.clone();
        let label = case.to_owned();
        h.imports.borrow_mut().insert(
            "loop".into(),
            function(move |_, a| {
                c.trace.borrow_mut().push("loop".into());
                c.args.borrow_mut().push(("loop".into(), a));
                if label == "loop-throw" {
                    Err(Rc::new("loop failed".into()))
                } else {
                    Ok(u())
                }
            }),
        );
        if case == "emit-throw" {
            let c = h.clone();
            set(
                &r,
                "emitAndLogError",
                function(move |_, a| {
                    c.args.borrow_mut().push(("emit".into(), a));
                    c.trace.borrow_mut().push("emit".into());
                    Err(Rc::new("emit failed".into()))
                }),
            );
        }
        let count = match case {
            "one" => Some(n(1.)),
            "five" => Some(n(5.)),
            "string-five" => Some(s("5")),
            _ => None,
        };
        let p = tick(ops, r.clone(), count);
        assert_trace(&h, &[if case == "one" { "positions" } else { "full" }]);
        if case == "disposed-after-await" {
            set(&r, "isDisposed", b(true));
            resolve(
                &result,
                Ok(obj(vec![
                    ("positions", positions.clone()),
                    ("balance", balance.clone()),
                ])),
            );
        } else if case == "fetch-reject" || case == "emit-throw" {
            resolve(&result, Err(Rc::new("fetch failed".into())));
        }
        let value = outcome(&h, &p);
        let expected: &[&str] = match case {
            "zero" | "string-five" | "loop-throw" => &[
                "full",
                "positions:update",
                "update:balance,loaded",
                "interval",
                "loop",
            ],
            "one" => &["positions", "positions:update", "interval", "loop"],
            "five" => &[
                "full",
                "positions:update",
                "update:balance,loaded",
                "orders",
                "update:orders,loaded",
                "interval",
                "loop",
            ],
            "disposed-after-await" => &["full"],
            "fetch-reject" => &["full", "emit", "interval", "loop"],
            "emit-throw" => &["full", "emit"],
            _ => unreachable!(),
        };
        assert_trace(&h, expected);
        if case == "loop-throw" || case == "emit-throw" {
            assert_eq!(
                &*value.err().unwrap(),
                if case == "loop-throw" {
                    "loop failed"
                } else {
                    "emit failed"
                },
                "LIFECYCLE-NATIVE error outside catch"
            );
        } else {
            assert!(value.is_ok());
        }
        if !matches!(case, "disposed-after-await" | "fetch-reject" | "emit-throw") {
            let args = h.args.borrow();
            let arg = &args
                .iter()
                .find(|(k, _)| k == "positions:update")
                .unwrap()
                .1[0];
            assert!(
                Rc::ptr_eq(arg, &positions),
                "LIFECYCLE-NATIVE polling positions alias"
            );
        }
        let store = get(&r, "store").unwrap();
        let loaded = get(&store, "loaded").unwrap();
        let expected_loaded = match case {
            "zero" | "string-five" | "loop-throw" => {
                json!({"old":true,"balance":true,"positions":true})
            }
            "five" => json!({"old":true,"balance":true,"positions":true,"orders":true}),
            _ => json!({"old":true}),
        };
        assert_eq!(
            literal(&loaded),
            expected_loaded,
            "LIFECYCLE-NATIVE polling loaded values"
        );
        if matches!(case, "zero" | "five" | "string-five" | "loop-throw") {
            assert!(
                Rc::ptr_eq(&get(&store, "balance").unwrap(), &balance),
                "LIFECYCLE-NATIVE polling balance alias"
            );
        }
        if case == "five" {
            assert!(
                Rc::ptr_eq(&get(&store, "orders").unwrap(), &order_rows),
                "LIFECYCLE-NATIVE polling orders alias"
            );
        }
        if matches!(case, "fetch-reject" | "emit-throw") {
            let args = h.args.borrow();
            let emitted = &args.iter().find(|(k, _)| k == "emit").unwrap().1;
            assert_eq!(emitted.len(), 1);
            assert_eq!(
                text(&emitted[0]),
                "fetch failed",
                "LIFECYCLE-NATIVE emitted catch value"
            );
        }
        let scheduled = h.args.borrow().iter().find(|(k, _)| k == "loop").cloned();
        if let Some((_, a)) = scheduled {
            assert_eq!(number(&a[1]), 137.);
            let _ = call(a[0].clone(), u(), vec![]).unwrap();
            let expected = match case {
                "one" => json!(2.0),
                "five" => json!(6.0),
                "string-five" => json!("51"),
                _ => json!(1.0),
            };
            let args = h.args.borrow();
            assert_eq!(
                literal(&args.iter().find(|(k, _)| k == "tick").unwrap().1[0]),
                expected,
                "LIFECYCLE-NATIVE scheduled source addition"
            );
        }
        let mut trace = h.trace.borrow().clone();
        if trace.last().map(|s| s.as_str()) == Some("tick") {
            trace.pop();
        }
        observed.push(json!({"kind":"tick","case":case,"trace":trace}));
    }
    let cases = observed
        .iter()
        .map(|v| json!({"kind":"tick","case":v["case"]}))
        .collect::<Vec<_>>();
    let source = source_cases(&cases);
    for (i, v) in observed.iter().enumerate() {
        assert_eq!(source[i]["trace"], v["trace"]);
    }
}
#[test]
fn historical_fold_defaults_units_and_live_settle_native() {
    let mut outputs = vec![];
    for case in ["explicit", "default", "reject"] {
        let h = Rc::new(Runtime::default());
        let ops = h.operations();
        let r = receiver(&h);
        let store = get(&r, "store").unwrap();
        let market = obj(vec![("quote", s("USDC"))]);
        set(
            &store,
            "marketsBySymbol",
            obj(vec![("BTC", market.clone())]),
        );
        let orders = arr(vec![s("order-axis")]);
        let pending_orders = pending();
        set(
            &r,
            "fetchHistoricalOrders",
            h.bind("orders", pending_orders.clone()),
        );
        let c = h.clone();
        let expected = orders.clone();
        h.imports.borrow_mut().insert(
            "wooOrdersToExecutions".into(),
            function(move |_, a| {
                assert!(
                    Rc::ptr_eq(&a[0], &expected),
                    "LIFECYCLE-NATIVE journal order alias"
                );
                c.trace.borrow_mut().push("executions".into());
                Ok(a[0].clone())
            }),
        );
        let threshold = if case == "default" { 408_000_000. } else { 11. };
        let intervals = arr(vec![
            obj(vec![("id", s("open"))]),
            obj(vec![("id", s("old")), ("closedAt", n(threshold - 1.))]),
            obj(vec![("id", s("keep")), ("closedAt", n(threshold))]),
            obj(vec![("id", s("empty")), ("closedAt", n(threshold + 1.))]),
            obj(vec![("id", s("next")), ("closedAt", n(threshold + 2.))]),
        ]);
        let c = h.clone();
        h.imports.borrow_mut().insert(
            "reconstructPositionIntervals".into(),
            function(move |_, _| {
                c.trace.borrow_mut().push("intervals".into());
                Ok(intervals.clone())
            }),
        );
        let c = h.clone();
        h.imports.borrow_mut().insert(
            "mapFoldedInterval".into(),
            function(move |_, a| {
                let id = text(&get(&a[0], "id")?);
                c.trace.borrow_mut().push(format!("fold:{id}"));
                assert_eq!(text(&get(&a[1], "account")?), "account-41");
                let resolver = get(&a[1], "resolveSettleCurrency")?;
                let quote = call(
                    resolver,
                    u(),
                    vec![s(if id == "next" { "UNKNOWN" } else { "BTC" })],
                )?;
                assert_eq!(
                    text(&quote),
                    if id == "next" { "USDT" } else { "USDC" },
                    "LIFECYCLE-NATIVE settle callback"
                );
                if id == "empty" {
                    Ok(Rc::new(Js::Null))
                } else {
                    Ok(obj(vec![("id", s(&id)), ("settle", quote)]))
                }
            }),
        );
        let opts = if case == "default" {
            obj(vec![
                ("account", s("account-41")),
                ("to", Rc::new(Js::Null)),
            ])
        } else {
            obj(vec![
                ("account", s("account-41")),
                ("from", n(11.)),
                ("to", n(19.)),
            ])
        };
        let p = fetch_historical_positions(ops, r, opts);
        assert_trace(
            &h,
            if case == "default" {
                &["now", "orders"]
            } else {
                &["orders"]
            },
        );
        let request = h
            .args
            .borrow()
            .iter()
            .find(|(k, _)| k == "orders")
            .unwrap()
            .1[0]
            .clone();
        assert_eq!(
            number(&get(&request, "from").unwrap()),
            if case == "default" { 408_000_000. } else { 11. },
            "LIFECYCLE-NATIVE history default units"
        );
        assert_eq!(
            number(&get(&request, "to").unwrap()),
            if case == "default" {
                3_000_000_000.
            } else {
                19.
            }
        );
        let Js::O(fields) = request.as_ref() else {
            panic!("request object")
        };
        assert_eq!(
            fields
                .borrow()
                .iter()
                .map(|(k, _)| k.clone())
                .collect::<Vec<_>>(),
            ["from", "to"]
        );
        resolve(
            &pending_orders,
            if case == "reject" {
                Err(Rc::new("history denied".into()))
            } else {
                Ok(orders)
            },
        );
        let value = outcome(&h, &p);
        if case == "reject" {
            assert_eq!(&*value.err().unwrap(), "history denied");
            assert_trace(&h, &["orders"]);
        } else {
            let result = value.unwrap();
            assert_eq!(
                literal(&result),
                json!([{"id":"keep","settle":"USDC"},{"id":"next","settle":"USDT"}]),
                "LIFECYCLE-NATIVE folded records"
            );
            let expected = if case == "default" {
                vec![
                    "now",
                    "orders",
                    "executions",
                    "intervals",
                    "fold:keep",
                    "fold:empty",
                    "fold:next",
                ]
            } else {
                vec![
                    "orders",
                    "executions",
                    "intervals",
                    "fold:keep",
                    "fold:empty",
                    "fold:next",
                ]
            };
            assert_trace(&h, &expected);
        }
        outputs.push(json!({"kind":"history","case":case,"trace":h.trace.borrow().clone()}));
    }
    let cases = outputs
        .iter()
        .map(|v| json!({"kind":"history","case":v["case"]}))
        .collect::<Vec<_>>();
    let source = source_cases(&cases);
    for (i, v) in outputs.iter().enumerate() {
        assert_eq!(source[i]["trace"], v["trace"]);
    }
}
#[test]
fn orderbook_native_shared_updates_live_lengths_and_second_row_read() {
    use safe_cex_types::{OrderBook, OrderBookOrders};
    let h = Rc::new(Runtime::default());
    let mut base = h.operations();
    let ops = Rc::get_mut(&mut base).unwrap();
    let book = Rc::new(RefCell::new(OrderBook {
        bids: vec![OrderBookOrders {
            price: 19.,
            amount: 23.,
            total: 29.,
        }],
        asks: vec![],
    }));
    let handle = obj(vec![("book", s("retained"))]);
    let bids = arr(vec![arr(vec![n(19.), n(31.)])]);
    let asks = arr(vec![arr(vec![n(41.), n(43.)])]);
    let update = obj(vec![(
        "data",
        obj(vec![("bids", bids.clone()), ("asks", asks.clone())]),
    )]);
    let reads = Rc::new(RefCell::new(0));
    let counts = reads.clone();
    let bid_alias = bids.clone();
    let initial = ops.value.clone();
    let mut copied = CanonicalOps {
        undefined: initial.undefined.clone(),
        object: initial.object.clone(),
        array: initial.array.clone(),
        get: initial.get.clone(),
        global: initial.global.clone(),
        invoke: initial.invoke.clone(),
        function: initial.function.clone(),
        spread: initial.spread.clone(),
        truthy: initial.truthy.clone(),
        strict_equal: initial.strict_equal.clone(),
        string: initial.string.clone(),
        number: initial.number.clone(),
        subtract: initial.subtract.clone(),
        get_key: initial.get_key.clone(),
        error: initial.error.clone(),
    };
    copied.get_key = Rc::new(move |v, k| {
        if Rc::ptr_eq(v, &bid_alias) && number(k) == 0. {
            let mut reads = counts.borrow_mut();
            *reads += 1;
            if *reads == 2 {
                return Ok(arr(vec![n(997.), n(37.)]));
            }
        }
        get(v, &text(k))
    });
    ops.value = Rc::new(copied);
    let actual_book = book.clone();
    let expected_handle = handle.clone();
    let c = h.clone();
    let rows = bids.clone();
    h.imports.borrow_mut().insert(
        "updateOrderBookByPrice".into(),
        function(move |_, a| {
            assert!(
                Rc::ptr_eq(&a[0], &expected_handle),
                "LIFECYCLE-NATIVE book reference"
            );
            let side = text(&a[1]);
            let price = number(&a[2]);
            let amount = number(&a[3]);
            c.trace
                .borrow_mut()
                .push(format!("{side}:{price}:{amount}"));
            safe_cex_store::update_order_book::update_order_book_by_price(
                &mut actual_book.borrow_mut(),
                side == "bids",
                price,
                amount,
                true,
            );
            if side == "bids" && price == 19. {
                let Js::A(rows) = rows.as_ref() else {
                    panic!("rows")
                };
                rows.borrow_mut().push(arr(vec![n(17.), n(47.)]));
            }
            Ok(n(0.))
        }),
    );
    let result = process_order_book_update(&base, handle, update).unwrap();
    assert!(matches!(result.as_ref(), Js::U));
    assert_trace(&h, &["bids:19:37", "bids:17:47", "asks:41:43"]);
    assert_eq!(
        *reads.borrow(),
        2,
        "LIFECYCLE-NATIVE independent row reread"
    );
    let b = book.borrow();
    assert_eq!(b.bids.len(), 2);
    assert_eq!(
        [b.bids[0].price, b.bids[0].amount, b.bids[0].total],
        [19., 37., 29.]
    );
    assert_eq!(
        [b.bids[1].price, b.bids[1].amount, b.bids[1].total],
        [17., 47., 0.]
    );
    assert_eq!(b.asks.len(), 1);
    assert_eq!(
        [b.asks[0].price, b.asks[0].amount, b.asks[0].total],
        [41., 43., 0.]
    );
    drop(b);
    let source = source_cases(&[json!({"kind":"book","case":"live"})]);
    assert_eq!(
        source[0]["trace"],
        json!(["bids:19:37", "bids:17:47", "asks:41:43"])
    );
}
