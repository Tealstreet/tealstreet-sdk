use safe_cex_venue_okx::order_reads::{self as port, OrderReadOps};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Debug)]
enum Native {
    U,
    B(bool),
    N(f64),
    S(String),
    A(RefCell<Vec<V>>, Option<usize>),
    O(Vec<(&'static str, V)>),
    Call(String),
    Promise(Result<V, E>),
    Set(RefCell<Vec<V>>),
    Map(RefCell<Vec<(V, V)>>),
}
type V = Rc<Native>;
#[derive(Debug)]
struct Failure {
    name: &'static str,
    message: &'static str,
    code: &'static str,
    ids: [u8; 2],
}
type E = Rc<Failure>;
fn run<T>(f: impl Future<Output = T>) -> T {
    let w = Waker::noop().clone();
    let mut c = Context::from_waker(&w);
    let mut f = Box::pin(f);
    match f.as_mut().poll(&mut c) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture"),
    }
}
fn s(v: &str) -> V {
    Rc::new(Native::S(v.into()))
}
fn n(v: f64) -> V {
    Rc::new(Native::N(v))
}
fn u() -> V {
    Rc::new(Native::U)
}
fn b(v: bool) -> V {
    Rc::new(Native::B(v))
}
fn array(v: Vec<V>) -> V {
    Rc::new(Native::A(RefCell::new(v), None))
}
fn object(v: Vec<(&'static str, V)>) -> V {
    Rc::new(Native::O(v))
}
fn list(v: &V) -> Vec<V> {
    let Native::A(a, _) = v.as_ref() else {
        panic!("array")
    };
    a.borrow().clone()
}
fn get(v: &V, key: &str) -> V {
    match v.as_ref() {
        Native::O(o) => o
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(u),
        Native::A(a, logical) if key == "length" => n(logical.unwrap_or(a.borrow().len()) as f64),
        _ => u(),
    }
}
fn string(v: &V) -> String {
    match v.as_ref() {
        Native::S(s) => s.clone(),
        Native::N(n) => n.to_string(),
        Native::U => "undefined".into(),
        Native::B(b) => b.to_string(),
        _ => "[object Object]".into(),
    }
}
fn numeric(v: &V) -> f64 {
    match v.as_ref() {
        Native::N(n) => *n,
        Native::S(s) => s.parse().unwrap_or(f64::NAN),
        Native::U => f64::NAN,
        Native::B(b) => u8::from(*b) as f64,
        _ => f64::NAN,
    }
}
fn truthy(v: &V) -> bool {
    match v.as_ref() {
        Native::U => false,
        Native::B(b) => *b,
        Native::N(n) => *n != 0.0 && !n.is_nan(),
        Native::S(s) => !s.is_empty(),
        _ => true,
    }
}
fn equal(a: &V, c: &V) -> bool {
    match (a.as_ref(), c.as_ref()) {
        (Native::U, Native::U) => true,
        (Native::S(a), Native::S(c)) => a == c,
        (Native::N(a), Native::N(c)) => a == c,
        (Native::B(a), Native::B(c)) => a == c,
        _ => Rc::ptr_eq(a, c),
    }
}
fn pack(v: &V) -> Value {
    match v.as_ref() {
        Native::U => json!({"u":true}),
        Native::B(b) => json!(b),
        Native::N(n) => json!({"n":format!("{:016x}",n.to_bits())}),
        Native::S(s) => json!(s),
        Native::A(a, _) => json!(a.borrow().iter().map(pack).collect::<Vec<_>>()),
        Native::O(o) => json!({"o":o.iter().map(|(k,v)|json!([k,pack(v)])).collect::<Vec<_>>()}),
        _ => panic!("unpacked host value"),
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
        Rc::new(Native::A(RefCell::new(rows), Some(100)))
    } else {
        array(rows)
    }
}
fn invoke(t: &Rc<State>, callee: V, args: Vec<V>) -> Result<V, E> {
    let Native::Call(name) = callee.as_ref() else {
        panic!("callee")
    };
    event(
        t,
        json!([
            "invoke",
            name,
            args.iter()
                .map(|v| matches!(v.as_ref(), Native::A(_, _) | Native::O(_))
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
        let Native::O(fields) = options.as_ref() else {
            panic!("options")
        };
        assert_eq!(
            fields.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            ["params"],
            "ORDER native request option keys"
        );
        let params = get(options, "params");
        let Native::O(fields) = params.as_ref() else {
            panic!("params")
        };
        let keys = fields.iter().map(|(k, _)| *k).collect::<Vec<_>>();
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
            return Ok(Rc::new(Native::Promise(Err(t.err.clone()))));
        }
        let data = if t.scenario == "non-array" {
            object(vec![])
        } else {
            pages(t, &string(&get(&params, "instType")))
        };
        return Ok(Rc::new(Native::Promise(Ok(object(vec![(
            "data",
            object(vec![("data", data)]),
        )])))));
    }
    match name.as_str() {
        "this.fetchNormalOrders" => Ok(Rc::new(Native::Promise(Ok(array(vec![row("N", 1.0)]))))),
        "this.fetchAlgoOrders" => {
            assert_eq!(args.len(), 1);
            let kind = string(&args[0]);
            if t.scenario == "reject" && kind == "conditional" {
                return Ok(Rc::new(Native::Promise(Err(t.err.clone()))));
            }
            Ok(Rc::new(Native::Promise(Ok(array(vec![row(
                match kind.as_str() {
                    "oco" => "O",
                    "conditional" => "C",
                    "trigger" => "T",
                    "move_order_stop" => "M",
                    _ => panic!("kind"),
                },
                1.0,
            )])))))
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
            let Native::O(fields) = args[0].as_ref() else {
                panic!("omit")
            };
            Ok(object(
                fields
                    .iter()
                    .filter(|(_, v)| !matches!(v.as_ref(), Native::U))
                    .cloned()
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
            Ok(Rc::new(Native::Promise(Ok(u()))))
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
                Rc::ptr_eq(&args[1], &t.market),
                "ORDER native live market identity"
            );
            assert!(matches!(args[2].as_ref(), Native::Call(_)));
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
            let Native::O(fields) = args[0].as_ref() else {
                panic!("opts")
            };
            assert_eq!(
                fields.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
                ["from", "to"],
                "ORDER native historical-position options"
            );
            if t.scenario == "reject" {
                return Ok(Rc::new(Native::Promise(Err(t.err.clone()))));
            }
            let rows = array(vec![
                row("a", 1.0),
                row("inverse", 2.0),
                row("unknown", 3.0),
                row("b", 4.0),
            ]);
            t.raw.borrow_mut().extend(list(&rows));
            Ok(Rc::new(Native::Promise(Ok(rows))))
        }
        "this.fetchIncomeHistory" => {
            let Native::O(fields) = args[0].as_ref() else {
                panic!("range opts")
            };
            assert_eq!(
                fields.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
                ["startTime", "endTime", "incomeType"],
                "RANGE native ordered chunk fields"
            );
            assert!(equal(&get(&args[0], "incomeType"), &s("funding")));
            t.requests.borrow_mut().push(args[0].clone());
            if t.scenario == "reject" {
                return Ok(Rc::new(Native::Promise(Err(t.err.clone()))));
            }
            let index = t.requests.borrow().len();
            let record = object(vec![
                ("tranId", s("same")),
                ("timestamp", n(index as f64)),
                ("value", n(index as f64)),
            ]);
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
            Ok(Rc::new(Native::Promise(Ok(array(records)))))
        }
        _ => panic!("invoke {name}"),
    }
}
fn intrinsic(t: &Rc<State>, name: &str, args: Vec<V>) -> Result<V, E> {
    event(t, json!(["intrinsic", name]));
    match name {
        "array-spread" => Ok(array(args.iter().flat_map(list).collect())),
        "last-index" => Ok(list(&args[0]).last().unwrap().clone()),
        "response-data-data" => Ok(get(&get(&args[0], "data"), "data")),
        "Array.isArray-or-empty" => Ok(if matches!(args[0].as_ref(), Native::A(_, _)) {
            args[0].clone()
        } else {
            array(vec![])
        }),
        "opts-optional-symbol"
        | "opts-optional-from"
        | "opts-optional-to"
        | "opts-optional-limit" => Ok(get(&args[0], name.strip_prefix("opts-optional-").unwrap())),
        "market-by-opts-symbol" => Ok(if t.scenario == "unknown-symbol" {
            u()
        } else {
            t.market.clone()
        }),
        "optional-id" => Ok(get(&args[0], "id")),
        "market-by-row-instId" => Ok(t.market.clone()),
        "optional-market-by-row-instId" => Ok(match string(&get(&args[0], "instId")).as_str() {
            "INV" => object(vec![("isInverse", b(true))]),
            "UNKNOWN" => u(),
            _ => t.market.clone(),
        }),
        "new-Set" => Ok(Rc::new(Native::Set(RefCell::new(vec![])))),
        "Set.has-order-id" => {
            let Native::Set(set) = args[0].as_ref() else {
                panic!("set")
            };
            Ok(b(set
                .borrow()
                .iter()
                .any(|v| equal(v, &get(&args[1], "id")))))
        }
        "Set.add-order-id" => {
            let Native::Set(set) = args[0].as_ref() else {
                panic!("set")
            };
            set.borrow_mut().push(get(&args[1], "id"));
            Ok(u())
        }
        "array-push" => {
            let Native::A(a, _) = args[0].as_ref() else {
                panic!("push")
            };
            a.borrow_mut().push(args[1].clone());
            Ok(u())
        }
        "array-push-spread" => {
            let Native::A(a, _) = args[0].as_ref() else {
                panic!("push")
            };
            a.borrow_mut().extend(list(&args[1]));
            Ok(u())
        }
        "less-than" => Ok(b(numeric(&args[0]) < numeric(&args[1]))),
        "greater-than" => Ok(b(numeric(&args[0]) > numeric(&args[1]))),
        "String-current-to" => Ok(s(&string(&get(&args[0], "to")))),
        "reduce-oldest-positive-uTime" => {
            let oldest = list(&args[0])
                .iter()
                .map(|r| safe_cex_num::parse_int(&string(&get(r, "uTime")), Some(10)))
                .filter(|v| v.is_finite() && *v > 0.0)
                .fold(f64::INFINITY, f64::min);
            Ok(n(oldest))
        }
        "Number.isFinite" => Ok(b(numeric(&args[0]).is_finite())),
        "String-oldest-minus-one" => Ok(s(&(numeric(&args[0]) - 1.0).to_string())),
        "contractsToBase-bind" => Ok(Rc::new(Native::Call("bound-toBase".into()))),
        "sort-createdTime-desc" | "sort-closedTime-desc" | "sort-timestamp-desc" => {
            let field = name
                .strip_prefix("sort-")
                .unwrap()
                .strip_suffix("-desc")
                .unwrap();
            let Native::A(a, _) = args[0].as_ref() else {
                panic!("sort")
            };
            a.borrow_mut().sort_by(|a, b| {
                numeric(&get(b, field))
                    .partial_cmp(&numeric(&get(a, field)))
                    .unwrap()
            });
            Ok(args[0].clone())
        }
        "slice-zero-current-limit" => Ok(array(
            list(&args[0])
                .into_iter()
                .take(numeric(&get(&args[1], "limit")) as usize)
                .collect(),
        )),
        "new-Map" => Ok(Rc::new(Native::Map(RefCell::new(vec![])))),
        "Map.set-tranId-record" => {
            if t.scenario == "iterator-error" || t.scenario == "close-error" {
                return Err(t.err.clone());
            }
            let Native::Map(map) = args[0].as_ref() else {
                panic!("map")
            };
            let key = get(&args[1], "tranId");
            let mut map = map.borrow_mut();
            if let Some((_, v)) = map.iter_mut().find(|(k, _)| equal(k, &key)) {
                *v = args[1].clone();
            } else {
                map.push((key, args[1].clone()));
            }
            Ok(u())
        }
        "Map.values-spread" => {
            let Native::Map(map) = args[0].as_ref() else {
                panic!("map")
            };
            Ok(array(map.borrow().iter().map(|(_, v)| v.clone()).collect()))
        }
        "Math.min-current-plus-seven-days" => Ok(n(
            (numeric(&args[0]) + numeric(&args[2])).min(numeric(&args[1]))
        )),
        "add-one" => Ok(n(numeric(&args[0]) + 1.0)),
        _ => panic!("intrinsic {name}"),
    }
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
            Ok(get(v, k))
        }),
        capture: Rc::new(move |name| {
            event(&capture, json!(["capture", name]));
            Ok(match name {
                "this.isPublicDataOnly" => b(capture.scenario == "public"),
                "this.store.orders" => capture.stored.clone(),
                "LogSeverity.Warning" => s("warning"),
                _ => Rc::new(Native::Call(name.into())),
            })
        }),
        invoke: Rc::new(move |c, a| invoke(&call, c, a)),
        await_value: Rc::new(move |v| {
            let wait = wait.clone();
            Box::pin(async move {
                event(&wait, json!(["await"]));
                let Native::Promise(outcome) = v.as_ref() else {
                    panic!("promise")
                };
                outcome.clone()
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
        nullish: Rc::new(|v| matches!(v.as_ref(), Native::U)),
        equal: Rc::new(equal),
        literal: Rc::new(s),
        number: Rc::new(n),
        undefined: Rc::new(u),
        array: Rc::new(array),
        object: Rc::new(object),
        error_message: Rc::new(move |e| {
            assert!(Rc::ptr_eq(e, &error.err));
            assert_eq!(e.name, "TypeError");
            assert_eq!(e.message, "down");
            assert_eq!(e.code, "ORDERS");
            assert_eq!(e.ids, [8, 3]);
            event(&error, json!(["error-message"]));
            Ok(s("down"))
        }),
        parallel_map: Rc::new(move |callee, values, callback| {
            let mapping = mapping.clone();
            Box::pin(async move {
                let Native::Call(name) = callee.as_ref() else {
                    panic!("map callee")
                };
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
            let Native::Call(name) = mapper.as_ref() else {
                panic!("mapper")
            };
            assert_eq!(name, "mapOkxPositionHistory");
            assert!(
                Rc::ptr_eq(&market, &position.market),
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
    let state = Rc::new(State {
        method,
        scenario,
        events: RefCell::new(vec![]),
        requests: RefCell::new(vec![]),
        raw: RefCell::new(vec![]),
        returned: RefCell::new(vec![]),
        counter: RefCell::new(vec![]),
        err: Rc::new(Failure {
            name: "TypeError",
            message: "down",
            code: "ORDERS",
            ids: [8, 3],
        }),
        stored: array(vec![row("STORED", 1.0)]),
        market: object(vec![("isInverse", b(false))]),
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
        assert!(
            Rc::ptr_eq(e, &state.err),
            "ORDER native original rejection identity"
        );
        assert_eq!(e.name, "TypeError");
        assert_eq!(e.message, "down");
        assert_eq!(e.code, "ORDERS");
        assert_eq!(e.ids, [8, 3]);
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
                assert!(Rc::ptr_eq(out, &state.stored));
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
                Rc::ptr_eq(&rows[1], state.returned.borrow().last().unwrap()),
                "RANGE native original latest record identity"
            );
        }
        if matches!(
            method,
            "normal" | "algo" | "history" | "closed" | "raw" | "positions"
        ) && expected > 0
        {
            let raw_pointers = state
                .raw
                .borrow()
                .iter()
                .map(|r| Rc::as_ptr(r) as usize)
                .collect::<std::collections::HashSet<_>>();
            for row in &rows {
                assert_eq!(
                    string(&get(row, "posId")),
                    "reused",
                    "ORDER native repeated position id retained"
                );
                assert!(
                    raw_pointers.contains(&(Rc::as_ptr(row) as usize)),
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
                let Native::O(fields) = first.as_ref() else {
                    panic!("request object")
                };
                assert!(
                    !fields.iter().any(|(key, _)| *key == "after"),
                    "ORDER native omitted initial history cursor"
                );
                assert!(
                    matches!(after.as_ref(), Native::U),
                    "ORDER native undefined history cursor"
                );
            } else {
                assert!(
                    matches!(after.as_ref(), Native::S(value) if value == "20000"),
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
                matches!(get(&reqs[index], "after").as_ref(), Native::U),
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
            json!({"error":{"name":e.name,"message":e.message,"code":e.code,"ids":e.ids,"original":Rc::ptr_eq(&e,&state.err)}})
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
