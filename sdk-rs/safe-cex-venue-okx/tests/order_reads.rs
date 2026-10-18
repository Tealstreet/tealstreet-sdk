use safe_cex_exchange::js_runtime::value::Js;
use safe_cex_venue_okx::order_reads::{self as port, OrderReadOps};
#[path = "support/production_intrinsics.rs"]
#[allow(dead_code)]
mod production;
use production::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn run<T>(f: impl Future<Output = T>) -> T {
    let w = Waker::noop().clone();
    let mut c = Context::from_waker(&w);
    let mut f = Box::pin(f);
    match f.as_mut().poll(&mut c) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture"),
    }
}
fn row(id: &str, time: f64) -> V {
    object(vec![
        ("id", s(id)),
        ("ordId", s(id)),
        ("algoId", s(&format!("a{id}"))),
        (
            "instId",
            s(match id {
                "inverse" => "INV",
                "unknown" => "UNKNOWN",
                _ => "M",
            }),
        ),
        ("uTime", s(&time.to_string())),
        ("createdTime", n(time)),
        ("closedTime", n(30000.0 - time)),
        ("timestamp", n(time)),
        ("posId", s("reused")),
    ])
}
struct State {
    method: &'static str,
    scenario: &'static str,
    events: RefCell<Vec<Value>>,
    requests: RefCell<Vec<V>>,
    raw: RefCell<Vec<V>>,
    returned: RefCell<Vec<V>>,
    counter: RefCell<Vec<(String, usize)>>,
    err: E,
    stored: V,
    market: V,
    close_count: Cell<usize>,
    receiver: V,
    logical_lengths: RefCell<Vec<V>>,
}
fn event(t: &State, v: Value) {
    t.events.borrow_mut().push(v);
}
fn pages(t: &State, instrument: &str) -> V {
    let mut counts = t.counter.borrow_mut();
    let index = if let Some((_, count)) = counts.iter_mut().find(|(key, _)| key == instrument) {
        let n = *count;
        *count += 1;
        n
    } else {
        counts.push((instrument.into(), 1));
        0
    };
    drop(counts);
    let count = if t.scenario == "cap" {
        1
    } else if t.scenario == "empty" {
        0
    } else if matches!(t.scenario, "full" | "repeat" | "cap" | "invalid-time") {
        if t.scenario == "full" && index > 0 {
            1
        } else {
            100
        }
    } else {
        2
    };
    let start = if t.scenario == "repeat" {
        10000.0
    } else {
        10000.0 - index as f64 * 100.0
    };
    let rows = (0..count)
        .map(|i| {
            row(
                &format!("{}", start as i64 - i as i64),
                if t.scenario == "invalid-time" {
                    0.0
                } else {
                    start - i as f64
                },
            )
        })
        .collect::<Vec<_>>();
    t.raw.borrow_mut().extend(rows.clone());
    if t.scenario == "cap" {
        {
            let value = array(rows);
            t.logical_lengths.borrow_mut().push(value.clone());
            value
        }
    } else {
        array(rows)
    }
}
fn invoke(t: &Rc<State>, callee: V, args: Vec<V>) -> Result<V, E> {
    let name = production::name(&callee);
    event(
        t,
        json!([
            "invoke",
            name,
            args.iter()
                .map(|v| matches!(v, Js::Array(_) | Js::Object(_))
                    .then_some("value")
                    .unwrap_or("scalar"))
                .collect::<Vec<_>>()
        ]),
    );
    if name.starts_with("xhr.get:") {
        assert_eq!(
            name,
            match t.method {
                "normal" => "xhr.get:UNFILLED_ORDERS",
                "algo" => "xhr.get:UNFILLED_ALGO_ORDERS",
                "history" => "xhr.get:ORDERS_HISTORY",
                _ => "xhr.get:POSITIONS_HISTORY",
            },
            "ORDER native endpoint"
        );
        let options = &args[0];
        let fields = production::fields(&options);
        assert_eq!(
            fields.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
            ["params"],
            "ORDER native request option keys"
        );
        let params = get(options, "params");
        let fields = production::fields(&params);
        let keys = fields.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>();
        let expected = match t.method {
            "normal" => vec!["instType", "after"],
            "algo" => vec!["instType", "ordType", "after"],
            "history" => vec!["instType", "begin", "end", "limit"],
            _ => vec!["instType", "before", "after", "limit"],
        };
        let filtered = if t.method == "history" {
            keys.iter()
                .copied()
                .filter(|k| !matches!(*k, "after"))
                .collect::<Vec<_>>()
        } else {
            keys.clone()
        };
        assert_eq!(filtered, expected, "ORDER native params key order");
        let instrument = string(&get(&params, "instType"));
        if matches!(t.method, "normal" | "algo") {
            assert!(
                matches!(instrument.as_str(), "SWAP" | "FUTURES"),
                "ORDER native instrument membership"
            );
        } else {
            assert_eq!(instrument, "SWAP", "ORDER native fixed instrument");
        }
        if t.method == "algo" {
            assert_eq!(
                string(&get(&params, "ordType")),
                "oco",
                "ORDER native original algo type"
            );
        }
        if matches!(t.method, "history" | "closed" | "raw") {
            assert_eq!(
                numeric(&get(&params, "limit")),
                100.0,
                "ORDER native page size"
            );
        }
        if t.method == "history" {
            assert_eq!(numeric(&get(&params, "begin")), 3.0);
            assert_eq!(numeric(&get(&params, "end")), 20000.0);
        } else if matches!(t.method, "closed" | "raw") {
            assert_eq!(numeric(&get(&params, "before")), 3.0);
        }
        t.requests.borrow_mut().push(params.clone());
        if t.scenario == "reject" {
            return Ok(promise(Err(t.err.clone())));
        }
        let data = if t.scenario == "non-array" {
            object(vec![])
        } else {
            pages(t, &string(&get(&params, "instType")))
        };
        return Ok(promise(Ok(object(vec![(
            "data",
            object(vec![("data", data)]),
        )]))));
    }
    match name.as_str() {
        "this.fetchNormalOrders" => Ok(promise(Ok(array(vec![row("N", 1.0)])))),
        "this.fetchAlgoOrders" => {
            assert_eq!(args.len(), 1);
            let kind = string(&args[0]);
            if t.scenario == "reject" && kind == "conditional" {
                return Ok(promise(Err(t.err.clone())));
            }
            Ok(promise(Ok(array(vec![row(
                match kind.as_str() {
                    "oco" => "O",
                    "conditional" => "C",
                    "trigger" => "T",
                    "move_order_stop" => "M",
                    _ => panic!("kind"),
                },
                1.0,
            )]))))
        }
        "this.getInstrumentTypes" => Ok(array(if t.scenario == "empty" {
            vec![]
        } else {
            vec![s("SWAP"), s("FUTURES")]
        })),
        "this.mapOrders" => {
            t.returned.borrow_mut().extend(list(&args[0]));
            Ok(args[0].clone())
        }
        "lodash.flatten" => Ok(array(list(&args[0]).iter().flat_map(list).collect())),
        "omitUndefined" => {
            let fields = production::fields(&args[0]);
            Ok(object(
                fields
                    .iter()
                    .filter(|(_, v)| !v.is_undefined())
                    .map(|(k, v)| (k.as_str(), v.clone()))
                    .collect(),
            ))
        }
        "oldestCursorId" => {
            assert_eq!(
                string(&args[1]),
                "ordId",
                "ORDER native ordId cursor selector"
            );
            Ok(list(&args[0])
                .iter()
                .map(|r| get(r, "ordId"))
                .min_by_key(|v| string(v).parse::<i64>().unwrap())
                .unwrap_or_else(u))
        }
        "sleep" => {
            assert_eq!(numeric(&args[0]), 100.0, "ORDER native sleep delay");
            Ok(promise(Ok(u())))
        }
        "this.log" => {
            let text = string(&args[0]);
            assert_eq!(
                text,
                if t.method == "history" {
                    "fetchHistoricalOrders stopped at the 50-page cap; older orders were not fetched"
                } else {
                    "fetchClosedPnl stopped at the 100-page cap; older positions were not fetched"
                },
                "ORDER native cap warning"
            );
            assert_eq!(string(&args[1]), "warning");
            Ok(u())
        }
        "this.emitAndLogError" => {
            assert_eq!(string(&args[0]), "down", "ORDER native emitted error");
            if t.scenario == "emit-error" {
                Err(t.err.clone())
            } else {
                Ok(u())
            }
        }
        "mapOkxHistoricalOrder" | "mapOkxClosedPosition" => {
            assert_eq!(
                name,
                if t.method == "history" {
                    "mapOkxHistoricalOrder"
                } else {
                    "mapOkxClosedPosition"
                },
                "ORDER native mapper selection"
            );
            assert!(
                args[1].same_reference(&t.market),
                "ORDER native live market identity"
            );
            assert!(args[2].is_callable());
            if name == "mapOkxClosedPosition" {
                assert_eq!(string(&args[3]), "ACCOUNT", "ORDER native accountName");
            }
            if t.scenario == "iterator-error" || t.scenario == "close-error" {
                return Err(t.err.clone());
            }
            t.returned.borrow_mut().push(args[0].clone());
            Ok(args[0].clone())
        }
        "this.fetchClosedPositionRows" => {
            let fields = production::fields(&args[0]);
            assert_eq!(
                fields.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
                ["from", "to"],
                "ORDER native historical-position options"
            );
            if t.scenario == "reject" {
                return Ok(promise(Err(t.err.clone())));
            }
            let rows = array(vec![
                row("a", 1.0),
                row("inverse", 2.0),
                row("unknown", 3.0),
                row("b", 4.0),
            ]);
            t.raw.borrow_mut().extend(list(&rows));
            Ok(promise(Ok(rows)))
        }
        "this.fetchIncomeHistory" => {
            let fields = production::fields(&args[0]);
            assert_eq!(
                fields.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
                ["startTime", "endTime", "incomeType"],
                "RANGE native ordered chunk fields"
            );
            assert!(equal(&get(&args[0], "incomeType"), &s("funding")));
            t.requests.borrow_mut().push(args[0].clone());
            if t.scenario == "reject" {
                return Ok(promise(Err(t.err.clone())));
            }
            let index = t.requests.borrow().len();
            let record = object(vec![
                ("tranId", s("same")),
                ("timestamp", n(index as f64)),
                ("value", n(index as f64)),
            ]);
            if matches!(t.scenario, "iterator-error" | "close-error") {
                let error = t.err.clone();
                record
                    .define_accessor("tranId", Some(Rc::new(move |_| Err(error.clone()))), None)
                    .expect("fresh test accessor target");
            }
            t.returned.borrow_mut().push(record.clone());
            let mut records = vec![record];
            if index == 1 {
                let other = object(vec![
                    ("tranId", s("other")),
                    ("timestamp", n(3.0)),
                    ("value", n(10.0)),
                ]);
                t.returned.borrow_mut().push(other.clone());
                records.push(other);
            }
            Ok(promise(Ok(array(records))))
        }
        _ => panic!("invoke {name}"),
    }
}
fn intrinsic(t: &Rc<State>, name: &str, args: Vec<V>) -> Result<V, E> {
    event(t, json!(["intrinsic", name]));
    production::intrinsic(t.receiver.clone(), name, args)
}
fn ops(t: Rc<State>) -> Rc<OrderReadOps<'static, V, E>> {
    let read = t.clone();
    let capture = t.clone();
    let call = t.clone();
    let wait = t.clone();
    let operation = t.clone();
    let iterator = t.clone();
    let error = t.clone();
    let mapping = t.clone();
    let position = t.clone();
    Rc::new(OrderReadOps {
        get: Rc::new(move |v, k| {
            event(&read, json!(["get", k]));
            Ok(
                if k == "length"
                    && read
                        .logical_lengths
                        .borrow()
                        .iter()
                        .any(|a| a.same_reference(v))
                {
                    n(100.)
                } else {
                    get(v, k)
                },
            )
        }),
        capture: Rc::new(move |name| {
            event(&capture, json!(["capture", name]));
            Ok(match name {
                "this.isPublicDataOnly" => b(capture.scenario == "public"),
                "this.store.orders" => capture.stored.clone(),
                "LogSeverity.Warning" => s("warning"),
                _ => named(name),
            })
        }),
        invoke: Rc::new(move |c, a| invoke(&call, c, a)),
        await_value: Rc::new(move |v| {
            let wait = wait.clone();
            Box::pin(async move {
                event(&wait, json!(["await"]));
                outcome(&v)
            })
        }),
        intrinsic: Rc::new(move |name, args| intrinsic(&operation, name, args)),
        for_of: Rc::new(move |rows, body| {
            event(&iterator, json!(["iterator"]));
            for row in list(&rows) {
                if let Err(e) = body(row) {
                    iterator.close_count.set(iterator.close_count.get() + 1);
                    event(
                        &iterator,
                        json!(["iterator-close", iterator.scenario == "close-error"]),
                    );
                    return Err(e);
                }
            }
            Ok(())
        }),
        truthy: Rc::new(truthy),
        nullish: Rc::new(Js::is_nullish),
        equal: Rc::new(equal),
        literal: Rc::new(s),
        number: Rc::new(n),
        undefined: Rc::new(u),
        array: Rc::new(array),
        object: Rc::new(object),
        error_message: Rc::new(move |e| {
            assert_failure(e, &error.err);
            event(&error, json!(["error-message"]));
            Ok(s("down"))
        }),
        parallel_map: Rc::new(move |callee, values, callback| {
            let mapping = mapping.clone();
            Box::pin(async move {
                let name = production::name(&callee);
                assert_eq!(name, "p-iteration.map", "ORDER native captured map callee");
                event(&mapping, json!(["parallel-map"]));
                let mut result = vec![];
                for value in list(&values) {
                    result.push(callback(value).await?);
                }
                Ok(array(result))
            })
        }),
        map_position: Rc::new(move |mapper, row, market, account| {
            let name = production::name(&mapper);
            assert_eq!(name, "mapOkxPositionHistory");
            assert!(
                market.same_reference(&position.market),
                "POSITION native captured row-market identity"
            );
            assert_eq!(
                string(&account),
                "ACCOUNT",
                "POSITION native original account"
            );
            event(&position, json!(["map-position", string(&get(&row, "id"))]));
            if matches!(position.scenario, "iterator-error" | "close-error") {
                return Err(position.err.clone());
            }
            position.returned.borrow_mut().push(row.clone());
            Ok(row)
        }),
    })
}
fn fixture(method: &'static str, scenario: &'static str) -> Value {
    let opts = object(vec![
        (
            "symbol",
            if scenario == "unknown-symbol" {
                s("BAD")
            } else {
                u()
            },
        ),
        ("from", n(3.0)),
        ("to", n(20000.0)),
        ("limit", if scenario == "limit" { n(1.0) } else { u() }),
        ("account", s("ACCOUNT")),
        ("accountName", s("ACCOUNT")),
    ]);
    let market = object(vec![("isInverse", b(false))]);
    let receiver = object(vec![
        (
            "store",
            object(vec![
                ("marketsBySymbol", object(vec![])),
                (
                    "marketsById",
                    object(vec![
                        ("M", market.clone()),
                        ("INV", object(vec![("isInverse", b(true))])),
                    ]),
                ),
            ]),
        ),
        ("contractsToBase", named("contractsToBase")),
    ]);
    let state = Rc::new(State {
        method,
        scenario,
        events: RefCell::new(vec![]),
        requests: RefCell::new(vec![]),
        raw: RefCell::new(vec![]),
        returned: RefCell::new(vec![]),
        counter: RefCell::new(vec![]),
        err: failure(),
        stored: array(vec![row("STORED", 1.0)]),
        market: market.clone(),
        receiver,
        logical_lengths: RefCell::new(vec![]),
        close_count: Cell::new(0),
    });
    let h = ops(state.clone());
    let output = match method {
        "aggregate" => run(port::fetch_orders(h)),
        "normal" => run(port::fetch_normal_orders(h)),
        "algo" => run(port::fetch_algo_orders(h, s("oco"))),
        "history" => run(port::fetch_historical_orders(h, opts)),
        "raw" => run(port::fetch_closed_position_rows(h, opts)),
        "positions" => run(port::fetch_historical_positions(h, opts)),
        "closed" => run(port::fetch_closed_pnl(h, opts)),
        "range" => run(port::fetch_income_history_range(
            h,
            n(if scenario == "empty" { 1.0 } else { 0.0 }),
            n(if scenario == "empty" {
                0.0
            } else {
                604800003.0
            }),
            s("funding"),
        )),
        _ => panic!("method"),
    };
    let caught = matches!(method, "aggregate" | "history" | "closed");
    let throws =
        (scenario == "reject" || matches!(scenario, "iterator-error" | "close-error")) && !caught;
    assert_eq!(output.is_err(), throws, "ORDER native failure scope");
    if let Err(e) = &output {
        assert_failure(e, &state.err);
    } else {
        let out = output.as_ref().unwrap();
        let rows = list(out);
        let expected = if scenario == "public"
            || scenario == "unknown-symbol"
            || scenario == "empty"
            || scenario == "non-array"
            || matches!(scenario, "iterator-error" | "close-error")
        {
            0
        } else if scenario == "reject" {
            if method == "aggregate" { 1 } else { 0 }
        } else {
            match method {
                "aggregate" => 5,
                "normal" | "algo" => {
                    if scenario == "full" {
                        202
                    } else {
                        4
                    }
                }
                "positions" => 2,
                "range" => 2,
                "history" => {
                    if scenario == "cap" {
                        50
                    } else if scenario == "limit" {
                        1
                    } else if scenario == "full" {
                        101
                    } else if matches!(scenario, "repeat" | "invalid-time") {
                        100
                    } else {
                        2
                    }
                }
                "raw" | "closed" => {
                    if scenario == "cap" {
                        100
                    } else if scenario == "full" {
                        101
                    } else if scenario == "repeat" {
                        200
                    } else if scenario == "invalid-time" {
                        100
                    } else {
                        2
                    }
                }
                _ => panic!("expect"),
            }
        };
        assert_eq!(
            rows.len(),
            expected,
            "ORDER native independent result count"
        );
        if method == "aggregate" {
            assert_eq!(
                rows.iter()
                    .map(|r| string(&get(r, "id")))
                    .collect::<Vec<_>>(),
                if scenario == "reject" {
                    vec!["STORED"]
                } else {
                    vec!["N", "O", "M", "C", "T"]
                },
                "ORDER native aggregation order"
            );
            if scenario == "reject" {
                assert!(out.same_reference(&state.stored));
            }
        }
        if method == "positions" && expected > 0 {
            assert_eq!(
                rows.iter()
                    .map(|r| string(&get(r, "id")))
                    .collect::<Vec<_>>(),
                ["a", "b"],
                "POSITION native skip inverse and unknown"
            );
        }
        if method == "range" && expected > 0 {
            assert_eq!(
                rows.iter()
                    .map(|r| string(&get(r, "tranId")))
                    .collect::<Vec<_>>(),
                ["other", "same"],
                "RANGE native timestamp ordering"
            );
            assert_eq!(
                numeric(&get(&rows[1], "timestamp")),
                2.0,
                "RANGE native latest duplicate timestamp"
            );
            assert_eq!(
                numeric(&get(&rows[0], "value")),
                10.0,
                "RANGE native original other record"
            );
            assert_eq!(
                numeric(&get(&rows[1], "value")),
                2.0,
                "RANGE native last duplicate wins"
            );
            assert!(
                rows[1].same_reference(state.returned.borrow().last().unwrap()),
                "RANGE native original latest record identity"
            );
        }
        if matches!(
            method,
            "normal" | "algo" | "history" | "closed" | "raw" | "positions"
        ) && expected > 0
        {
            let raw_values = state.raw.borrow();
            for row in &rows {
                assert_eq!(
                    string(&get(row, "posId")),
                    "reused",
                    "ORDER native repeated position id retained"
                );
                assert!(
                    raw_values.iter().any(|raw| raw.same_reference(row)),
                    "ORDER native original row/mapper return identity"
                );
            }
            if matches!(method, "history" | "closed" | "raw") {
                assert_eq!(
                    string(&get(&rows[0], "id")),
                    if method == "closed" {
                        match scenario {
                            "normal" => "9999",
                            "full" => "9900",
                            "repeat" => "9901",
                            "cap" => "100",
                            _ => "10000",
                        }
                    } else {
                        "10000"
                    },
                    "ORDER native newest first"
                );
            }
        }
    }
    if matches!(method, "history" | "raw" | "closed") {
        let count = if matches!(scenario, "public" | "unknown-symbol") {
            0
        } else if scenario == "cap" {
            if method == "history" { 50 } else { 100 }
        } else if matches!(scenario, "full" | "repeat") {
            2
        } else {
            1
        };
        assert_eq!(
            state.requests.borrow().len(),
            count,
            "ORDER native independent request count"
        );
        if count > 1 {
            let after = get(&state.requests.borrow()[1], "after");
            assert_eq!(
                string(&after),
                if scenario == "cap" {
                    if method == "history" { "10000" } else { "9999" }
                } else if method == "history" {
                    "9901"
                } else {
                    "9900"
                },
                "ORDER native id versus time cursor"
            );
        }
        let warnings = state
            .events
            .borrow()
            .iter()
            .filter(|event| event == &&json!(["capture", "this.log"]))
            .count();
        assert_eq!(
            warnings,
            usize::from(scenario == "cap" && method != "raw"),
            "ORDER native cap warning count"
        );
    }
    if matches!(method, "history" | "raw" | "closed") {
        let reqs = state.requests.borrow();
        if let Some(first) = reqs.first() {
            let after = get(first, "after");
            if method == "history" {
                let fields = production::fields(first);
                assert!(
                    !fields.iter().any(|(key, _)| key == "after"),
                    "ORDER native omitted initial history cursor"
                );
                assert!(
                    after.is_undefined(),
                    "ORDER native undefined history cursor"
                );
            } else {
                assert!(
                    matches!(&after, Js::String(value) if value.as_ref()==[50,48,48,48,48]),
                    "ORDER native initial timestamp cursor"
                );
            }
        }
    }
    if matches!(method, "normal" | "algo") {
        assert_eq!(
            &state.events.borrow()[..3],
            &[
                json!(["capture", "lodash.flatten"]),
                json!(["capture", "p-iteration.map"]),
                json!(["capture", "this.getInstrumentTypes"])
            ],
            "ORDER native pre-await callee capture order"
        );
        let reqs = state.requests.borrow();
        for index in if scenario == "full" {
            vec![0, 2]
        } else if scenario == "empty" {
            vec![]
        } else if scenario == "reject" {
            vec![0]
        } else {
            vec![0, 1]
        } {
            assert!(
                get(&reqs[index], "after").is_undefined(),
                "ORDER native initial unfilled cursor"
            );
        }
        assert_eq!(
            reqs.len(),
            if scenario == "empty" {
                0
            } else if scenario == "reject" {
                1
            } else if scenario == "full" {
                4
            } else {
                2
            },
            "ORDER native per instrument request floor"
        );
        let types = reqs
            .iter()
            .map(|p| string(&get(p, "instType")))
            .collect::<Vec<_>>();
        let expected_types = if scenario == "empty" {
            vec![]
        } else if scenario == "reject" {
            vec!["SWAP"]
        } else if scenario == "full" {
            vec!["SWAP", "SWAP", "FUTURES", "FUTURES"]
        } else {
            vec!["SWAP", "FUTURES"]
        };
        assert_eq!(
            types, expected_types,
            "ORDER native independent instrument sequence"
        );
        if scenario == "full" {
            assert_eq!(
                string(&get(&reqs[1], "after")),
                if method == "algo" { "a9901" } else { "9901" },
                "ORDER native last accumulated id cursor"
            );
        }
    }
    if method == "range"
        && !matches!(
            scenario,
            "empty" | "reject" | "iterator-error" | "close-error"
        )
    {
        let reqs = state.requests.borrow();
        assert_eq!(reqs.len(), 2, "RANGE native independent window count");
        assert_eq!(numeric(&get(&reqs[0], "startTime")), 0.0);
        assert_eq!(
            numeric(&get(&reqs[0], "endTime")),
            604800000.0,
            "RANGE native seven days"
        );
        assert_eq!(
            numeric(&get(&reqs[1], "startTime")),
            604800001.0,
            "RANGE native exclusive next start"
        );
        assert_eq!(numeric(&get(&reqs[1], "endTime")), 604800003.0);
    }
    assert_eq!(
        state.close_count.get(),
        usize::from(matches!(scenario, "iterator-error" | "close-error")),
        "ORDER native IteratorClose floor"
    );
    let emitted = state
        .events
        .borrow()
        .iter()
        .filter(|event| event == &&json!(["capture", "this.emitAndLogError"]))
        .count();
    assert_eq!(
        emitted,
        usize::from(caught && matches!(scenario, "reject" | "iterator-error" | "close-error")),
        "ORDER native emission floor"
    );
    let result = match output {
        Ok(v) => json!({"value":pack(&v)}),
        Err(e) => {
            json!({"error":error_pack(&e,&state.err)})
        }
    };
    json!({"method":method,"scenario":scenario,"result":result,"requests":state.requests.borrow().iter().map(pack).collect::<Vec<_>>(),"events":state.events.borrow().iter().filter(|v|matches!(v[0].as_str(),Some("invoke"|"parallel-map"|"map-position"|"iterator-close"))).cloned().collect::<Vec<_>>()})
}
#[test]
fn order_reads_native_before_source() {
    let mut cases = vec![];
    for scenario in ["normal", "reject"] {
        cases.push(fixture("aggregate", scenario));
    }
    for method in ["normal", "algo"] {
        for scenario in ["normal", "full", "empty", "reject"] {
            if !(method == "history" && scenario == "invalid-time") {
                cases.push(fixture(method, scenario));
            }
        }
    }
    for method in ["history", "raw", "closed"] {
        for scenario in [
            "normal",
            "full",
            "empty",
            "public",
            "unknown-symbol",
            "non-array",
            "reject",
            "repeat",
            "invalid-time",
            "cap",
        ] {
            if !(method == "history" && scenario == "invalid-time") {
                cases.push(fixture(method, scenario));
            }
        }
        if method == "history" {
            cases.push(fixture(method, "limit"));
        }
        if method != "raw" {
            for scenario in ["iterator-error", "close-error"] {
                if !(method == "history" && scenario == "invalid-time") {
                    cases.push(fixture(method, scenario));
                }
            }
        }
    }
    for method in ["positions", "range"] {
        for scenario in ["normal", "reject", "iterator-error", "close-error"] {
            if !(method == "history" && scenario == "invalid-time") {
                cases.push(fixture(method, scenario));
            }
        }
        if method == "range" {
            cases.push(fixture(method, "empty"));
        }
    }
    assert_eq!(cases.len(), 53, "independent method fixture floor");
    let input = json!({"cases":cases});
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/order-reads-oracle.mjs");
    let mut child = Command::new("node")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["cases"].as_array().unwrap().len(),
        53,
        "independent source fixture floor"
    );
    for (index, (native, source)) in input["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(source["cases"].as_array().unwrap())
        .enumerate()
    {
        for key in ["method", "scenario", "result", "requests", "events"] {
            assert!(
                native[key] == source[key],
                "maintained source mismatch case {index} {}/{} field {key}",
                native["method"],
                native["scenario"]
            );
        }
    }
}
