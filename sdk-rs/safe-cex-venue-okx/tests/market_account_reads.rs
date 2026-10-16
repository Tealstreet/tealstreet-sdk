use safe_cex_venue_okx::market_account_reads::{self as port, MarketAccountOps, ReadAwait};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
type V = Rc<Native>;
type E = Rc<Error>;
struct Error {
    name: &'static str,
    message: &'static str,
    fresh: bool,
    code: Option<&'static str>,
    ids: Option<[u8; 2]>,
}
enum Native {
    U,
    Null,
    B(bool),
    N(f64),
    S(String),
    O(RefCell<Vec<(String, V)>>),
    A(RefCell<Vec<V>>),
    Call(String),
    P(Result<V, E>),
    Member(V, String),
    Big(f64),
    Iter(Vec<V>),
}
fn u() -> V {
    Rc::new(Native::U)
}
fn s(x: &str) -> V {
    Rc::new(Native::S(x.into()))
}
fn n(x: f64) -> V {
    Rc::new(Native::N(x))
}
fn b(x: bool) -> V {
    Rc::new(Native::B(x))
}
fn array(x: Vec<V>) -> V {
    Rc::new(Native::A(RefCell::new(x)))
}
fn object(x: Vec<(&str, V)>) -> V {
    Rc::new(Native::O(RefCell::new(
        x.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    )))
}
fn get(v: &V, k: &str) -> V {
    match v.as_ref() {
        Native::O(x) => x
            .borrow()
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(u),
        Native::A(x) => {
            if k == "length" {
                n(x.borrow().len() as f64)
            } else {
                k.parse::<usize>()
                    .ok()
                    .and_then(|i| x.borrow().get(i).cloned())
                    .unwrap_or_else(u)
            }
        }
        Native::Iter(_) => s("999"),
        _ => u(),
    }
}
fn put(v: &V, k: &str, value: V) {
    let Native::O(x) = v.as_ref() else {
        panic!("object set")
    };
    let mut x = x.borrow_mut();
    if let Some((_, v)) = x.iter_mut().find(|(key, _)| key == k) {
        *v = value
    } else {
        x.push((k.into(), value))
    }
}
fn items(v: &V) -> Vec<V> {
    let Native::A(x) = v.as_ref() else {
        panic!("array")
    };
    x.borrow().clone()
}
fn text(v: &V) -> String {
    match v.as_ref() {
        Native::S(s) => s.clone(),
        Native::N(x) => safe_cex_num::js_number_string(*x),
        Native::U => "undefined".into(),
        Native::Null => "null".into(),
        Native::B(x) => x.to_string(),
        _ => panic!("primitive"),
    }
}
fn num(v: &V) -> f64 {
    match v.as_ref() {
        Native::N(x) => *x,
        Native::S(x) => safe_cex_num::number_from_string(x),
        Native::Null => 0.,
        Native::U => f64::NAN,
        _ => panic!("number"),
    }
}
fn truth(v: &V) -> bool {
    match v.as_ref() {
        Native::U | Native::Null => false,
        Native::B(x) => *x,
        Native::N(x) => *x != 0. && !x.is_nan(),
        Native::S(x) => !x.is_empty(),
        _ => true,
    }
}
fn null(v: &V) -> bool {
    matches!(v.as_ref(), Native::U | Native::Null)
}
fn equal(a: &V, b: &V) -> bool {
    match (a.as_ref(), b.as_ref()) {
        (Native::U, Native::U) | (Native::Null, Native::Null) => true,
        (Native::S(a), Native::S(b)) => a == b,
        (Native::N(a), Native::N(b)) => a == b,
        (Native::B(a), Native::B(b)) => a == b,
        _ => Rc::ptr_eq(a, b),
    }
}
fn pack(v: &V) -> Value {
    match v.as_ref() {
        Native::U => json!({"u":true}),
        Native::Null => Value::Null,
        Native::B(x) => json!(x),
        Native::S(x) => json!(x),
        Native::N(x) => {
            json!({"n":if x.is_nan(){"nan".into()}else{format!("{:016x}",x.to_bits())}})
        }
        Native::O(x) => {
            json!({"o":x.borrow().iter().map(|(k,v)|json!([k,pack(v)])).collect::<Vec<_>>()})
        }
        Native::A(x) => json!(x.borrow().iter().map(pack).collect::<Vec<_>>()),
        Native::Call(x) => json!({"call":x}),
        _ => panic!("pack"),
    }
}
struct State {
    method: &'static str,
    case: &'static str,
    err: E,
    markets: V,
    tickers: V,
    balance: V,
    positions: V,
    raw_bal: V,
    by_id: V,
    by_symbol: V,
    orders: V,
    events: RefCell<Vec<Value>>,
    captures: RefCell<Vec<String>>,
    emissions: Cell<usize>,
    calls: RefCell<Vec<String>>,
    raw_rows: RefCell<Vec<V>>,
    requests: RefCell<Vec<(String, V)>>,
    sleeps: RefCell<Vec<f64>>,
    changed_min: Cell<bool>,
    binding_reads: Cell<usize>,
    binding_closes: Cell<usize>,
    metadata_args: RefCell<Vec<V>>,
}
fn emit(t: &State, v: Value) {
    t.events.borrow_mut().push(v)
}
fn ready(f: ReadAwait<V, E>) -> Result<V, E> {
    let mut f = f;
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(x) => x,
        Poll::Pending => panic!("profile future unexpectedly pending"),
    }
}
fn promise(result: Result<V, E>) -> V {
    Rc::new(Native::P(result))
}
fn raw_market(id: &str, inverse: bool) -> V {
    object(vec![
        ("instId", s(id)),
        ("ctType", s(if inverse { "inverse" } else { "linear" })),
        ("uly", s(if inverse { "btc-usd" } else { "btc-usdt" })),
        ("settleCcy", s(if inverse { "btc" } else { "usdt" })),
        ("ctValCcy", s(if inverse { "usd" } else { "btc" })),
        ("maxIcebergSz", s("101")),
        ("maxLmtSz", s("102")),
        ("maxMktSz", s("103")),
        ("maxStopSz", s("104")),
        ("maxTriggerSz", s("105")),
        ("maxTwapSz", s("106")),
        ("state", s("live")),
        ("lotSz", s("3")),
        ("tickSz", s("0.25")),
        ("minSz", s("2")),
        ("ctVal", s("0.5")),
        ("lever", s("9")),
    ])
}
fn raw_ticker(id: &str) -> V {
    object(vec![
        ("instId", s(id)),
        ("open24h", s("10")),
        ("last", s("12")),
        ("bidPx", s("11")),
        ("askPx", s("13")),
        ("volCcy24h", s("14")),
        ("vol24h", s("15")),
    ])
}
fn market_info(id: &str, symbol: &str) -> V {
    object(vec![("id", s(id)), ("symbol", s(symbol))])
}
fn metadata(symbol: V, side: &str, mode: &str) -> V {
    object(vec![
        ("symbol", symbol),
        ("side", s(side)),
        ("leverage", n(9.)),
        ("positionMode", s(mode)),
        ("marginType", s("cross")),
        ("id", s("OVERRIDE")),
        ("entryPrice", n(999.)),
    ])
}
fn invoke(t: &State, callee: V, args: Vec<V>) -> Result<V, E> {
    if let Native::Member(v, name) = callee.as_ref() {
        return Ok(match name.as_str() {
            "split" => array(text(v).split(&text(&args[0])).map(s).collect()),
            "toUpperCase" => s(&text(v).to_uppercase()),
            "includes" => b(items(v).iter().any(|x| equal(x, &args[0]))),
            "multipliedBy" => Rc::new(Native::Big(match v.as_ref() {
                Native::Big(x) => *x * num(&args[0]),
                _ => panic!("BigNumber receiver"),
            })),
            "toNumber" => n(match v.as_ref() {
                Native::Big(x) => *x,
                _ => panic!("BigNumber receiver"),
            }),
            _ => panic!("member {name}"),
        });
    }
    let Native::Call(name) = callee.as_ref() else {
        panic!("callee")
    };
    t.calls.borrow_mut().push(name.clone());
    if name.starts_with("this.xhr.get") {
        let route = if name == "this.xhr.get" {
            assert_eq!(
                text(&args[0]),
                "INSTRUMENTS",
                "REQUEST native market endpoint"
            );
            "INSTRUMENTS"
        } else {
            name.split(':')
                .nth(1)
                .unwrap()
                .strip_prefix("ENDPOINTS.")
                .unwrap()
        };
        let opts = if name == "this.xhr.get" {
            args[1].clone()
        } else {
            args[0].clone()
        };
        let params = get(&opts, "params");
        let Native::O(fields) = params.as_ref() else {
            panic!("params")
        };
        let expected = if route == "LEVERAGE" {
            vec!["instId", "mgnMode"]
        } else {
            vec!["instType"]
        };
        assert_eq!(
            fields
                .borrow()
                .iter()
                .map(|(k, _)| k.as_str())
                .collect::<Vec<_>>(),
            expected,
            "REQUEST native params keys"
        );
        if route == "LEVERAGE" {
            assert_eq!(
                text(&get(&params, "mgnMode")),
                "cross",
                "LEVERAGE native margin mode"
            )
        } else {
            if route == "BALANCE" && t.case == "empty" {
                assert!(
                    matches!(get(&params, "instType").as_ref(), Native::U),
                    "REQUEST native empty balance instrument"
                )
            } else {
                assert!(
                    matches!(get(&params,"instType").as_ref(),Native::S(x)if x=="SWAP"||x=="FUTURES"),
                    "REQUEST native instrument type"
                )
            }
        };
        emit(t, json!(["xhr", route, pack(&opts)]));
        t.requests.borrow_mut().push((route.into(), params.clone()));
        if t.case == "reject" {
            return Ok(promise(Err(t.err.clone())));
        }
        let rows = match route {
            "INSTRUMENTS" => {
                let m = raw_market(
                    if text(&get(&params, "instType")) == "SWAP" {
                        "M0"
                    } else {
                        "M1"
                    },
                    text(&get(&params, "instType")) == "FUTURES",
                );
                if t.case == "max-nan" {
                    put(&m, "maxMktSz", s("bad"))
                }
                t.raw_rows.borrow_mut().push(m.clone());
                array(vec![m, raw_market("SKIP", false)])
            }
            "TICKERS" => {
                let row = raw_ticker(if text(&get(&params, "instType")) == "SWAP" {
                    "M0"
                } else {
                    "M1"
                });
                t.raw_rows.borrow_mut().push(row.clone());
                if t.case == "non-array" {
                    row
                } else {
                    array(vec![row])
                }
            }
            "BALANCE" => {
                let first = object(vec![("balData", t.raw_bal.clone())]);
                if matches!(t.case, "binding-close" | "binding-error-close") {
                    put(&first, "marker", s(t.case));
                    Rc::new(Native::Iter(vec![first]))
                } else {
                    array(vec![first])
                }
            }
            "POSITIONS" => array(vec![object(vec![(
                "id",
                s(if text(&get(&params, "instType")) == "SWAP" {
                    "OPEN0"
                } else {
                    "OPEN1"
                }),
            )])]),
            "LEVERAGE" => array(
                text(&get(&params, "instId"))
                    .split(',')
                    .map(|id| object(vec![("instId", s(id)), ("lever", s("7"))]))
                    .collect(),
            ),
            _ => panic!("route"),
        };
        return Ok(promise(Ok(object(vec![(
            "data",
            object(vec![("data", rows)]),
        )]))));
    }
    Ok(match name.as_str() {
        "parseFloat" => n(safe_cex_num::coercion::parse_float(&text(&args[0]))),
        "String" => s(&text(&args[0])),
        "BigNumber" => Rc::new(Native::Big(num(&args[0]))),
        "Math.min:old" | "Math.min:new" => {
            if t.case == "callee-reenter" {
                assert_eq!(
                    name, "Math.min:old",
                    "MARKET native captured minimum callee"
                )
            };
            let x = if args.iter().any(|v| num(v).is_nan()) {
                f64::NAN
            } else {
                args.iter().map(num).fold(f64::INFINITY, f64::min)
            };
            n(if name.ends_with("new") { x + 1000. } else { x })
        }
        "Math.max" => n(args.iter().map(num).fold(f64::NEG_INFINITY, f64::max)),
        "roundUSD" => n(safe_cex_num::round_usd(num(&args[0]))),
        "add" => n(safe_cex_num::add(num(&args[0]), num(&args[1]))),
        "subtract" => n(safe_cex_num::subtract(num(&args[0]), num(&args[1]))),
        "this.toFiniteNumber" => {
            let x = num(&args[0]);
            n(if x.is_finite() { x } else { 0. })
        }
        "this.getInstrumentTypes" => {
            emit(t, json!(["types"]));
            array(if t.case == "empty" {
                vec![]
            } else if t.case == "callee-reenter" {
                vec![s("SWAP")]
            } else if t.case == "inverse" {
                vec![s("FUTURES")]
            } else {
                vec![s("SWAP"), s("FUTURES")]
            })
        }
        "this.getMarketInstrumentsEndpoint" => {
            emit(t, json!(["endpoint"]));
            s("INSTRUMENTS")
        }
        "this.isSupportedMarketInstrument" => {
            emit(t, json!(["supported", pack(&get(&args[0], "instId"))]));
            b(text(&get(&args[0], "instId")) != "SKIP")
        }
        "this.formatMarketPrettySymbol" => {
            emit(
                t,
                json!([
                    "pretty",
                    pack(&get(&args[0], "instId")),
                    pack(&args[1]),
                    pack(&args[2])
                ]),
            );
            if t.case == "no-pretty" {
                u()
            } else {
                s(&format!("{}/{}", text(&args[1]), text(&args[2])))
            }
        }
        "this.formatMarketSymbol" => {
            emit(t, json!(["symbol", pack(&get(&args[0], "instId"))]));
            s(if text(&get(&args[0], "instId")) == "M1" {
                "BTCSYM"
            } else {
                "SYM"
            })
        }
        "Promise.all" => {
            emit(t, json!(["all"]));
            let mut values = vec![];
            for p in items(&args[0]) {
                let Native::P(result) = p.as_ref() else {
                    panic!("all promise")
                };
                match result {
                    Ok(v) => values.push(v.clone()),
                    Err(e) => return Ok(promise(Err(e.clone()))),
                }
            }
            promise(Ok(array(values)))
        }
        "this.getOrFetchPositionMode" => {
            if t.method == "combined" {
                assert_eq!(args.len(), 1, "POSITION native force argument count");
                assert!(
                    matches!(args[0].as_ref(), Native::B(true)),
                    "POSITION native forced mode refresh"
                )
            } else {
                assert_eq!(args.len(), 0, "METADATA native mode argument count")
            }

            emit(
                t,
                json!(["mode", args.iter().map(pack).collect::<Vec<_>>()]),
            );
            promise(if t.case == "reject" {
                Err(t.err.clone())
            } else {
                Ok(s(if t.case == "hedged" {
                    "hedged"
                } else {
                    "oneway"
                }))
            })
        }
        "this.fetchLeverages" => {
            emit(t, json!(["leverages", pack(&args[0])]));
            promise(Ok(object(vec![("SYM", n(9.)), ("BTCSYM", n(3.))])))
        }
        "this.fetchBalanceAndOpenPositions" => {
            emit(t, json!(["open"]));
            promise(Ok(object(vec![
                ("balance", t.balance.clone()),
                ("positions", t.positions.clone()),
            ])))
        }
        "this.fetchPositionMetadata" => {
            let symbols = items(&args[0]);
            t.metadata_args.borrow_mut().push(args[0].clone());
            emit(t, json!(["metadata", pack(&args[0])]));
            promise(if t.case == "reject" {
                Err(t.err.clone())
            } else {
                Ok(array(
                    symbols
                        .into_iter()
                        .flat_map(|v| {
                            vec![
                                metadata(v.clone(), "long", "oneway"),
                                metadata(v, "short", "oneway"),
                            ]
                        })
                        .collect(),
                ))
            })
        }
        "createPositionId" => {
            emit(
                t,
                json!(["position-id", args.iter().map(pack).collect::<Vec<_>>()]),
            );
            s("CREATED")
        }
        "this.mapPositions" => {
            assert_eq!(
                items(&args[0]).len(),
                if t.case == "empty" { 0 } else { 2 },
                "OPEN native raw mapped position floor"
            );
            emit(t, json!(["map-positions", pack(&args[0])]));
            t.positions.clone()
        }
        "this.formatBalanceUsedAvail" => {
            assert!(
                Rc::ptr_eq(&args[0], &t.raw_bal),
                "OPEN native balData before iterator close identity"
            );
            assert!(
                Rc::ptr_eq(&args[1], &t.positions),
                "OPEN native mapped positions identity"
            );
            assert!(
                Rc::ptr_eq(&args[2], &t.orders),
                "OPEN native orders identity"
            );
            emit(t, json!(["format-balance"]));
            promise(Ok(t.balance.clone()))
        }
        "this.getOrFetchAccountLevel" => {
            emit(t, json!(["level"]));
            promise(if t.case == "reject" {
                Err(t.err.clone())
            } else {
                Ok(if t.case == "unified" {
                    n(3.)
                } else if t.case == "level-string" {
                    s("2")
                } else if t.case == "level-zero" {
                    n(0.)
                } else {
                    n(2.)
                })
            })
        }
        "findTickerMarkForCurrency" => {
            assert!(
                Rc::ptr_eq(&args[0], &t.tickers),
                "BALANCE native original ticker collection identity"
            );
            emit(t, json!(["mark", pack(&args[1])]));
            n(if text(&args[1]) == "BTC" { 10. } else { 1. })
        }
        "this.convertMarginValueToUsd" => {
            let key = if text(&args[1]) == "BTC" { "M1" } else { "M0" };
            assert!(
                Rc::ptr_eq(&args[2], &get(&t.by_id, key)),
                "BALANCE native original market identity"
            );
            assert_eq!(num(&args[3]), 10., "BALANCE native mark argument");
            emit(
                t,
                json!(["usd", args[..2].iter().map(pack).collect::<Vec<_>>()]),
            );
            n(num(&args[0]) * if text(&args[1]) == "BTC" { 10. } else { 1. })
        }
        "this.convertUsdValueToMarginCurrency" => {
            assert_eq!(
                text(&get(&args[2], "id")),
                "M0",
                "BALANCE native order market"
            );
            assert_eq!(num(&args[3]), 20., "BALANCE native order price argument");
            emit(
                t,
                json!(["margin", args[..2].iter().map(pack).collect::<Vec<_>>()]),
            );
            n(num(&args[0]) / if text(&args[1]) == "BTC" { 10. } else { 1. })
        }
        "this.emitAndLogError" => {
            assert!(
                matches!(args[0].as_ref(),Native::S(x)if x=="down"),
                "ERROR native emitted value"
            );
            t.emissions.set(t.emissions.get() + 1);
            emit(t, json!(["error", pack(&args[0])]));
            u()
        }
        "chunk" => {
            assert_eq!(num(&args[1]), 20., "LEVERAGE native chunk width");
            array(
                items(&args[0])
                    .chunks(20)
                    .map(|x| array(x.to_vec()))
                    .collect(),
            )
        }
        "flatten" => {
            let rows = items(&args[0])
                .into_iter()
                .flat_map(|v| items(&v))
                .collect();
            array(rows)
        }
        "sleep" => {
            let delay = num(&args[0]);
            t.sleeps.borrow_mut().push(delay);
            emit(t, json!(["sleep", pack(&args[0])]));
            promise(Ok(u()))
        }
        _ => panic!("invoke {name}"),
    })
}
fn intr(t: &State, name: &str, args: Vec<V>) -> Result<V, E> {
    Ok(match name {
        "get-index" => get(&args[0], &text(&args[1])),
        "set-index" => {
            put(&args[0], &text(&args[1]), args[2].clone());
            u()
        }
        "to-property-key" => s(&text(&args[0])),
        "toUpperCase-expression" => s(&text(&args[0]).to_uppercase()),
        "destructure-two" => {
            let x = items(&args[0]);
            array(vec![
                x.first().cloned().unwrap_or_else(u),
                x.get(1).cloned().unwrap_or_else(u),
            ])
        }
        "ticker-percentage" => n(((num(&args[0]) - num(&args[1])) / num(&args[1])) * 100.),
        "multiply" => n(num(&args[0]) * num(&args[1])),
        "order-notional-over-leverage" => n((num(&args[0]) * num(&args[1])) / num(&args[2])),
        "spread-append" => {
            let mut rows = items(&args[0]);
            rows.push(args[1].clone());
            array(rows)
        }
        "array-spread-two" => {
            let mut rows = items(&args[0]);
            rows.extend(items(&args[1]));
            array(rows)
        }
        "object-spread-after-id" => {
            let out = object(vec![("id", args[0].clone())]);
            let Native::O(fields) = args[1].as_ref() else {
                panic!("metadata")
            };
            for (k, v) in fields.borrow().iter() {
                put(&out, k, v.clone())
            }
            out
        }
        "Object.values-expression" => {
            let Native::O(fields) = args[0].as_ref() else {
                panic!("values")
            };
            array(fields.borrow().iter().map(|(_, v)| v.clone()).collect())
        }
        "join-comma" => s(&items(&args[0])
            .iter()
            .map(text)
            .collect::<Vec<_>>()
            .join(",")),
        _ => panic!("intrinsic {name} {}", t.case),
    })
}
fn ops(t: Rc<State>) -> Rc<MarketAccountOps<V, E>> {
    let read = t.clone();
    let cap = t.clone();
    let call = t.clone();
    let op = t.clone();
    let binding = t.clone();
    let err = t.clone();
    Rc::new(MarketAccountOps {
        get: Rc::new(move |v, k| {
            if k == "maxIcebergSz" && read.case == "callee-reenter" {
                read.changed_min.set(true)
            }
            if k == "last" && read.case == "get-error" {
                return Err(read.err.clone());
            }
            if k == "balData" {
                read.binding_reads.set(read.binding_reads.get() + 1)
            }
            if k == "balData"
                && matches!(get(v,"marker").as_ref(),Native::S(x)if x=="binding-error-close")
            {
                binding_read_error(&read)?;
            }
            Ok(get(v, k))
        }),
        set: Rc::new(|v, k, x| {
            put(v, k, x);
            Ok(())
        }),
        capture: Rc::new(move |name| {
            cap.captures.borrow_mut().push(name.into());
            Ok(match name {
                "this.store.markets" => cap.markets.clone(),
                "this.store.tickers" => cap.tickers.clone(),
                "this.store.balance" => cap.balance.clone(),
                "this.store.positions" => cap.positions.clone(),
                "this.store.orders" => cap.orders.clone(),
                "this._accountPositionMode" => {
                    if cap.case == "mode-missing" {
                        u()
                    } else {
                        s("oneway")
                    }
                }
                "this.isDisposed" => b(cap.case == "disposed"),
                "this.store.marketsById" => cap.by_id.clone(),
                "this.store.marketsBySymbol" => cap.by_symbol.clone(),
                "PositionMode.OneWay" => s("oneway"),
                "PositionSide.Long" => s("long"),
                "PositionSide.Short" => s("short"),
                "MarginType.Cross" => s("cross"),
                "OrderType.Limit" => s("limit"),
                "OrderStatus.Open" => s("open"),
                "Math.min" => Rc::new(Native::Call(format!(
                    "Math.min:{}",
                    if cap.changed_min.get() { "new" } else { "old" }
                ))),
                _ => Rc::new(Native::Call(name.into())),
            })
        }),
        member: Rc::new(|v, name| Ok(Rc::new(Native::Member(v, name.into())))),
        invoke: Rc::new(move |v, a| invoke(&call, v, a)),
        intrinsic: Rc::new(move |n, a| intr(&op, n, a)),
        truthy: Rc::new(truth),
        nullish: Rc::new(null),
        equal: Rc::new(equal),
        literal: Rc::new(s),
        number: Rc::new(n),
        boolean: Rc::new(b),
        undefined: Rc::new(u),
        array: Rc::new(array),
        object: Rc::new(object),
        map: Rc::new(|rows, f| {
            let mut out = vec![];
            for row in items(&rows) {
                out.push(f(row)?)
            }
            Ok(array(out))
        }),
        flat_map: Rc::new(|rows, f| {
            let mut out = vec![];
            for row in items(&rows) {
                let next = f(row)?;
                if matches!(next.as_ref(), Native::A(_)) {
                    out.extend(items(&next))
                } else {
                    out.push(next)
                }
            }
            Ok(array(out))
        }),
        filter: Rc::new(|rows, f| {
            let mut out = vec![];
            for row in items(&rows) {
                if f(row.clone())? {
                    out.push(row)
                }
            }
            Ok(array(out))
        }),
        reduce: Rc::new(|rows, mut acc, f| {
            for row in items(&rows) {
                acc = f(acc, row)?
            }
            Ok(acc)
        }),
        for_of: Rc::new(|rows, f| {
            for row in items(&rows) {
                f(row)?
            }
            Ok(())
        }),
        some: Rc::new(|rows, f| {
            for row in items(&rows) {
                if f(row)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }),
        bind_first: Rc::new(move |values, f| {
            let Native::Iter(rows) = values.as_ref() else {
                return f(items(&values).first().cloned().unwrap_or_else(u));
            };
            let row = rows[0].clone();
            let result = f(row.clone());
            binding.binding_closes.set(binding.binding_closes.get() + 1);
            emit(&binding, json!(["binding-close"]));
            if binding.case == "binding-close" {
                put(&row, "balData", array(vec![s("REPLACED")]))
            }
            let close = if binding.case == "binding-error-close" {
                Err(Rc::new(Error {
                    name: "TypeError",
                    message: "cleanup",
                    fresh: false,
                    code: Some("CLOSE"),
                    ids: Some([1, 2]),
                }))
            } else {
                Ok(())
            };
            match result {
                Err(error) => Err(error),
                Ok(value) => {
                    close?;
                    Ok(value)
                }
            }
        }),
        sum_by: Rc::new(|callee, rows, f| {
            assert!(
                matches!(callee.as_ref(),Native::Call(x)if x=="sumBy"),
                "BALANCE native captured sum callee"
            );
            let mut sum = 0.;
            for row in items(&rows) {
                sum += num(&if let Some(f) = &f { f(row)? } else { row })
            }
            Ok(n(sum))
        }),
        parallel_map: Rc::new(|callee, batches, f| {
            assert!(
                matches!(callee.as_ref(),Native::Call(x)if x=="p-iteration.map"),
                "LEVERAGE native captured map callee"
            );
            Box::pin(async move {
                let mut values = vec![];
                for batch in items(&batches) {
                    values.push(f(batch).await?)
                }
                Ok(array(values))
            })
        }),
        await_value: Rc::new(|value| {
            let Native::P(result) = value.as_ref() else {
                panic!("promise")
            };
            let result = result.clone();
            Box::pin(async move { result })
        }),
        error_value: Rc::new(move |e| {
            assert!(Rc::ptr_eq(e, &err.err), "ERROR native original identity");
            assert_eq!(
                (e.name, e.message, e.fresh, e.code, e.ids),
                ("TypeError", "down", false, Some("READ"), Some([9, 4])),
                "ERROR native full error value"
            );
            object(vec![
                ("message", s(e.message)),
                ("code", s("READ")),
                ("ids", array(vec![n(9.), n(4.)])),
            ])
        }),
        new_error: Rc::new(|callee, message| {
            assert!(matches!(callee.as_ref(),Native::Call(x)if x=="Error"));
            assert!(matches!(message.as_ref(),Native::S(x)if x=="Position mode not loaded"));
            Ok(Rc::new(Error {
                name: "Error",
                message: "Position mode not loaded",
                fresh: true,
                code: None,
                ids: None,
            }))
        }),
    })
}
fn binding_read_error(t: &State) -> Result<(), E> {
    Err(t.err.clone())
}
fn assert_keys(value: &V, expected: &[&str], name: &str) {
    let Native::O(fields) = value.as_ref() else {
        panic!("object keys")
    };
    assert_eq!(
        fields
            .borrow()
            .iter()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>(),
        expected,
        "{name}"
    )
}
fn assert_number(value: &V, expected: f64, name: &str) {
    assert!(matches!(value.as_ref(), Native::N(_)), "{name} native type");
    let actual = num(value);
    if expected.is_nan() {
        assert!(actual.is_nan(), "{name}")
    } else {
        assert_eq!(actual.to_bits(), expected.to_bits(), "{name}")
    }
}
fn fixture(method: &'static str, case: &'static str) -> Value {
    let bal = array(if case == "empty" {
        vec![]
    } else if case == "duplicate-currency" {
        vec![
            object(vec![("ccy", s("USDT")), ("eq", s("100"))]),
            object(vec![("ccy", s("USDT")), ("eq", s("50"))]),
            object(vec![("ccy", s("BTC")), ("eq", s("2"))]),
        ]
    } else {
        vec![
            object(vec![("ccy", s("USDT")), ("eq", s("100"))]),
            object(vec![("ccy", s("BTC")), ("eq", s("2"))]),
        ]
    });
    let p0 = object(vec![
        ("instId", s("M0")),
        ("ccy", s("USDT")),
        ("imr", s("5")),
        ("upl", s("2")),
        ("markPx", s("10")),
    ]);
    let p1 = object(vec![
        ("instId", s("M1")),
        ("ccy", s("BTC")),
        ("imr", s("1")),
        ("upl", s("0.25")),
        ("markPx", s("10")),
    ]);
    let raw_positions = array(if case == "empty" {
        vec![]
    } else {
        vec![
            object(vec![("info", p0)]),
            object(vec![("info", p1)]),
            object(vec![("info", Rc::new(Native::Null))]),
        ]
    });
    let order = object(vec![
        ("info", object(vec![("ccy", s("USDT")), ("lever", s("4"))])),
        ("type", s("limit")),
        ("status", s("open")),
        ("reduceOnly", b(false)),
        ("remaining", n(2.)),
        ("price", n(20.)),
        ("symbol", s("SYM")),
    ]);
    let orders = array(if case == "empty" {
        vec![]
    } else {
        let mut rows = vec![order];
        for key in ["closed", "market", "reduce"] {
            rows.push(object(vec![
                ("info", object(vec![("ccy", s("USDT")), ("lever", s("1"))])),
                ("type", s(if key == "market" { "market" } else { "limit" })),
                ("status", s(if key == "closed" { "closed" } else { "open" })),
                ("reduceOnly", b(key == "reduce")),
                ("remaining", n(10000.)),
                ("price", n(10000.)),
            ]))
        }
        rows
    });
    let m0 = market_info("M0", "SYM");
    let m1 = market_info("M1", "BTCSYM");
    let by_id = object(vec![("M0", m0.clone()), ("M1", m1.clone())]);
    let mut entries = vec![
        ("SYM".to_owned(), m0.clone()),
        ("BTCSYM".to_owned(), m1.clone()),
    ];
    for i in 0..41 {
        entries.push((
            format!("S{i}"),
            if i % 2 == 0 { m0.clone() } else { m1.clone() },
        ));
    }
    let by_symbol = Rc::new(Native::O(RefCell::new(entries)));
    let t = Rc::new(State {
        method,
        case,
        err: Rc::new(Error {
            name: "TypeError",
            message: "down",
            fresh: false,
            code: Some("READ"),
            ids: Some([9, 4]),
        }),
        markets: array(vec![s("CACHED-MARKET")]),
        tickers: array(vec![s("CACHED-TICKER")]),
        balance: object(vec![("cached", n(777.))]),
        positions: array(vec![
            object(vec![("symbol", s("SYM")), ("id", s("OPEN"))]),
            object(vec![("symbol", s("OUTSIDE")), ("id", s("OUT"))]),
        ]),
        raw_bal: bal.clone(),
        by_id,
        by_symbol,
        orders: orders.clone(),
        events: RefCell::new(vec![]),
        captures: RefCell::new(vec![]),
        emissions: Cell::new(0),
        calls: RefCell::new(vec![]),
        raw_rows: RefCell::new(vec![]),
        requests: RefCell::new(vec![]),
        sleeps: RefCell::new(vec![]),
        changed_min: Cell::new(false),
        binding_reads: Cell::new(0),
        binding_closes: Cell::new(0),
        metadata_args: RefCell::new(vec![]),
    });
    let h = ops(t.clone());
    let symbols = if case == "no-symbols" {
        u()
    } else if case == "empty" {
        array(vec![])
    } else if case == "batch-boundary" {
        array((0..41).map(|i| s(&format!("S{i}"))).collect())
    } else {
        array(vec![
            s("SYM"),
            s(if method == "leverage" {
                "BTCSYM"
            } else {
                "MISSING"
            }),
        ])
    };
    let result = match method {
        "markets" => ready(Box::pin(port::fetch_markets(h.clone()))),
        "raw" => ready(Box::pin(port::fetch_tickers_raw(h.clone()))),
        "tickers" => {
            let data = if case == "empty" {
                array(vec![])
            } else if case == "unknown" {
                array(vec![raw_ticker("UNKNOWN")])
            } else {
                let row = raw_ticker("M0");
                if case == "zero-open" {
                    put(&row, "open24h", s("0"))
                }
                array(vec![row, raw_ticker("UNKNOWN")])
            };
            port::parse_tickers(h.clone(), data)
        }
        "metadata" => ready(Box::pin(port::fetch_position_metadata(
            h.clone(),
            symbols.clone(),
        ))),
        "combined" => ready(Box::pin(port::fetch_balance_and_positions(
            h.clone(),
            symbols.clone(),
        ))),
        "open" => ready(Box::pin(port::fetch_balance_and_open_positions(h.clone()))),
        "balance" => ready(Box::pin(port::format_balance_used_avail(
            h.clone(),
            bal.clone(),
            raw_positions.clone(),
            orders,
        ))),
        _ => ready(Box::pin(port::fetch_leverages(h.clone(), symbols.clone()))),
    };
    let propagates = matches!(method, "metadata" | "combined" | "leverage") && case == "reject"
        || method == "open" && case == "mode-missing";
    assert_eq!(result.is_err(), propagates, "READ native failure scope");
    let caught = case == "reject" && matches!(method, "markets" | "raw" | "open" | "balance")
        || method == "tickers" && case == "get-error"
        || method == "open" && case == "binding-error-close";
    assert_eq!(
        t.emissions.get(),
        usize::from(caught),
        "ERROR native independent emission count"
    );
    if let Ok(value) = &result {
        if caught && method != "raw" && method != "open" {
            assert!(
                Rc::ptr_eq(
                    value,
                    if method == "markets" {
                        &t.markets
                    } else if method == "tickers" {
                        &t.tickers
                    } else {
                        &t.balance
                    }
                ),
                "READ native current cache identity"
            );
        } else {
            match method {
                "markets" => {
                    let rows = items(value);
                    assert_eq!(
                        rows.len(),
                        if case == "empty" {
                            0
                        } else if matches!(case, "inverse" | "callee-reenter") {
                            1
                        } else {
                            2
                        },
                        "MARKET native independent instrument count"
                    );
                    let ids = if case == "inverse" {
                        vec!["M1"]
                    } else if case == "callee-reenter" {
                        vec!["M0"]
                    } else {
                        vec!["M0", "M1"]
                    };
                    for (i, row) in rows.into_iter().enumerate() {
                        assert!(
                            matches!(get(&row,"id").as_ref(),Native::S(x)if x==ids[i]),
                            "MARKET native independent id order"
                        );
                        let inverse = ids[i] == "M1";
                        assert!(
                            matches!(get(&row,"symbol").as_ref(),Native::S(x)if x==if inverse{"BTCSYM"}else{"SYM"}),
                            "MARKET native symbol"
                        );
                        if case != "no-pretty" {
                            assert!(
                                matches!(get(&row,"prettySymbol").as_ref(),Native::S(x)if x==if inverse{"BTC/USD"}else{"BTC/USDT"}),
                                "MARKET native pretty symbol"
                            );
                        }

                        assert!(
                            matches!(get(&row, "active").as_ref(), Native::B(true)),
                            "MARKET native active flag"
                        );

                        assert_keys(
                            &row,
                            if case == "no-pretty" {
                                &[
                                    "id",
                                    "symbol",
                                    "base",
                                    "quote",
                                    "isInverse",
                                    "active",
                                    "precision",
                                    "limits",
                                    "contractSize",
                                    "info",
                                ]
                            } else {
                                &[
                                    "id",
                                    "symbol",
                                    "prettySymbol",
                                    "base",
                                    "quote",
                                    "isInverse",
                                    "active",
                                    "precision",
                                    "limits",
                                    "contractSize",
                                    "info",
                                ]
                            },
                            "MARKET native ordered fields",
                        );
                        assert_eq!(text(&get(&row, "base")), "BTC", "MARKET native base");
                        assert_eq!(
                            text(&get(&row, "quote")),
                            if inverse { "USD" } else { "USDT" },
                            "MARKET native inverse quote"
                        );
                        assert!(
                            matches!(get(&row,"isInverse").as_ref(),Native::B(x)if *x==inverse)
                        );
                        assert_number(
                            &get(&get(&row, "precision"), "amount"),
                            3.,
                            "MARKET native amount precision",
                        );
                        assert_number(
                            &get(&get(&row, "precision"), "price"),
                            0.25,
                            "MARKET native price precision",
                        );
                        let limits = get(&row, "limits");
                        assert_number(
                            &get(&get(&limits, "leverage"), "min"),
                            1.,
                            "MARKET native minimum leverage",
                        );
                        assert_number(
                            &get(&get(&limits, "leverage"), "max"),
                            9.,
                            "MARKET native maximum leverage",
                        );

                        assert_number(
                            &get(&get(&limits, "amount"), "min"),
                            1.,
                            "MARKET native minimum contract conversion",
                        );
                        assert_number(
                            &get(&get(&limits, "amount"), "max"),
                            if case == "max-nan" { f64::NAN } else { 101. },
                            "MARKET native minimum maximum",
                        );
                        assert_number(
                            &get(&row, "contractSize"),
                            0.5,
                            "MARKET native contract size",
                        );
                        assert!(
                            t.raw_rows
                                .borrow()
                                .iter()
                                .any(|x| Rc::ptr_eq(x, &get(&row, "info"))),
                            "MARKET native raw info identity"
                        );
                    }
                }
                "raw" => {
                    let rows = items(value);
                    assert_eq!(
                        rows.len(),
                        if matches!(case, "empty" | "reject") {
                            0
                        } else {
                            2
                        },
                        "RAW native independent row count"
                    );
                    for row in rows {
                        assert!(
                            t.raw_rows.borrow().iter().any(|x| Rc::ptr_eq(x, &row)),
                            "RAW native original row identity"
                        )
                    }
                }
                "tickers" => {
                    if !caught {
                        let rows = items(value);
                        assert_eq!(
                            rows.len(),
                            if matches!(case, "empty" | "unknown") {
                                0
                            } else {
                                1
                            },
                            "TICKER native independent known-market count"
                        );
                        for row in rows {
                            assert!(
                                matches!(get(&row,"id").as_ref(),Native::S(x)if x=="M0"),
                                "TICKER native id"
                            );
                            assert!(
                                matches!(get(&row,"symbol").as_ref(),Native::S(x)if x=="SYM"),
                                "TICKER native symbol"
                            );
                            assert_keys(
                                &row,
                                &[
                                    "id",
                                    "symbol",
                                    "bid",
                                    "ask",
                                    "last",
                                    "mark",
                                    "index",
                                    "percentage",
                                    "fundingRate",
                                    "fundingTime",
                                    "volume",
                                    "quoteVolume",
                                    "openInterest",
                                ],
                                "TICKER native ordered fields",
                            );
                            for (key, x) in [
                                ("bid", 11.),
                                ("ask", 13.),
                                ("last", 12.),
                                ("mark", 12.),
                                ("index", 12.),
                                ("volume", 14.),
                                ("quoteVolume", 15.),
                                ("fundingRate", 0.),
                                ("fundingTime", 0.),
                                ("openInterest", 0.),
                            ] {
                                assert_number(&get(&row, key), x, "TICKER native distinct numbers")
                            }
                            assert_number(
                                &get(&row, "percentage"),
                                if case == "zero-open" {
                                    f64::INFINITY
                                } else {
                                    20.
                                },
                                "TICKER native percentage",
                            )
                        }
                    }
                }
                "metadata" => {
                    let rows = items(value);
                    assert_eq!(
                        rows.len(),
                        if case == "empty" { 0 } else { 4 },
                        "METADATA native independent pair count"
                    );
                    for (i, row) in rows.iter().enumerate() {
                        assert!(
                            matches!(get(row,"symbol").as_ref(),Native::S(x)if x==if i<2{"SYM"}else{"MISSING"}),
                            "METADATA native symbol ordering"
                        );

                        assert_keys(
                            row,
                            &["symbol", "side", "leverage", "positionMode", "marginType"],
                            "METADATA native ordered fields",
                        );
                        assert_eq!(
                            text(&get(row, "side")),
                            if i % 2 == 0 { "long" } else { "short" },
                            "METADATA native side ordering"
                        );
                        assert_number(
                            &get(row, "leverage"),
                            if i < 2 { 9. } else { 0. },
                            "METADATA native fallback leverage",
                        );
                        assert_eq!(
                            text(&get(row, "positionMode")),
                            if case == "hedged" { "hedged" } else { "oneway" }
                        );
                        assert_eq!(text(&get(row, "marginType")), "cross");
                    }
                }
                "combined" => {
                    assert!(
                        Rc::ptr_eq(&get(value, "balance"), &t.balance),
                        "COMBINED native balance identity"
                    );
                    let positions = get(value, "positions");
                    if case == "no-symbols" {
                        assert!(
                            Rc::ptr_eq(&positions, &t.positions),
                            "COMBINED native no-symbol position identity"
                        )
                    } else {
                        let rows = items(&positions);
                        assert_eq!(
                            rows.len(),
                            if case == "empty" { 0 } else { 3 },
                            "COMBINED native selected plus fake count"
                        );
                        assert_eq!(
                            items(&t.metadata_args.borrow()[0])
                                .iter()
                                .map(text)
                                .collect::<Vec<_>>(),
                            if case == "empty" {
                                vec![]
                            } else {
                                vec!["MISSING".to_owned()]
                            },
                            "COMBINED native missing symbols"
                        );
                        for row in rows.iter().skip(1) {
                            assert_eq!(
                                text(&get(row, "id")),
                                "OVERRIDE",
                                "COMBINED native metadata spread overwrites id"
                            );
                            for key in [
                                "entryPrice",
                                "notional",
                                "unrealizedPnl",
                                "contracts",
                                "liquidationPrice",
                            ] {
                                assert_number(&get(row, key), 0., "COMBINED native fake zeros")
                            }
                        }
                    }
                }
                "open" => {
                    assert_keys(
                        value,
                        &["balance", "positions"],
                        "OPEN native ordered result fields",
                    );
                    assert!(
                        Rc::ptr_eq(&get(value, "balance"), &t.balance),
                        "OPEN native current balance identity"
                    );
                    assert!(
                        Rc::ptr_eq(&get(value, "positions"), &t.positions),
                        "OPEN native mapped/fallback positions identity"
                    );
                    if case == "binding-close" {
                        assert_eq!(t.binding_reads.get(), 1, "OPEN native field binding count");
                        assert_eq!(
                            t.binding_closes.get(),
                            1,
                            "OPEN native iterator close count"
                        );
                    }
                }
                "balance" => {
                    if !caught {
                        assert_keys(
                            value,
                            &["used", "free", "total", "upnl", "currencies", "info"],
                            "BALANCE native ordered fields",
                        );
                        let empty = case == "empty";
                        let duplicate = case == "duplicate-currency";
                        for (key, x) in [
                            ("used", if empty { 0. } else { 15. }),
                            (
                                "free",
                                if empty {
                                    0.
                                } else if duplicate {
                                    44.
                                } else {
                                    94.
                                },
                            ),
                            (
                                "total",
                                if empty {
                                    0.
                                } else if duplicate {
                                    65.5
                                } else {
                                    115.5
                                },
                            ),
                            ("upnl", if empty { 0. } else { 4.5 }),
                        ] {
                            assert_number(
                                &get(value, key),
                                x,
                                "BALANCE native independent dollar values",
                            )
                        }
                        assert!(
                            Rc::ptr_eq(&get(value, "info"), &t.raw_bal),
                            "BALANCE native raw info identity"
                        );
                        let currencies = get(value, "currencies");
                        if matches!(case, "unified" | "level-string" | "empty") {
                            assert_keys(&currencies, &[], "BALANCE native noncurrency account mode")
                        } else {
                            assert_keys(
                                &currencies,
                                &["USDT", "BTC"],
                                "BALANCE native currency keys",
                            );
                            let usd = get(&currencies, "USDT");
                            for (key, x) in [
                                ("used", 16.),
                                ("free", if duplicate { 34. } else { 84. }),
                                ("total", if duplicate { 48. } else { 98. }),
                                ("upnl", 2.),
                            ] {
                                assert_number(
                                    &get(&usd, key),
                                    x,
                                    "BALANCE native per currency values",
                                )
                            }
                            let btc = get(&currencies, "BTC");
                            for (key, x) in
                                [("used", 1.), ("free", 1.), ("total", 1.75), ("upnl", 0.25)]
                            {
                                assert_number(
                                    &get(&btc, key),
                                    x,
                                    "BALANCE native converted currency values",
                                )
                            }
                        }
                    }
                }
                "leverage" => {
                    let batches = if matches!(case, "empty" | "disposed") {
                        0
                    } else if case == "batch-boundary" {
                        3
                    } else {
                        1
                    };
                    assert_eq!(
                        t.requests.borrow().len(),
                        batches,
                        "LEVERAGE native independent batch count"
                    );
                    assert_eq!(
                        t.sleeps.borrow().as_slice(),
                        if batches == 3 {
                            &[0., 250., 500.][..]
                        } else if batches == 1 {
                            &[0.][..]
                        } else {
                            &[][..]
                        },
                        "LEVERAGE native stagger delays"
                    );
                    if batches > 0 {
                        assert_keys(value, &["SYM", "BTCSYM"], "LEVERAGE native result keys");
                        assert_number(&get(value, "SYM"), 7., "LEVERAGE native decoded value");
                        assert_number(
                            &get(value, "BTCSYM"),
                            7.,
                            "LEVERAGE native second decoded value",
                        )
                    } else {
                        assert_keys(value, &[], "LEVERAGE native empty result")
                    }
                }
                _ => panic!("method"),
            }
        }
    } else {
        let e = result.as_ref().err().unwrap();
        if e.fresh {
            assert!(!Rc::ptr_eq(e, &t.err));
            assert_eq!(
                (e.code, e.ids),
                (None, None),
                "OPEN native absent fresh error fields"
            );
            assert_eq!(
                (e.name, e.message),
                ("Error", "Position mode not loaded"),
                "OPEN native fresh failure"
            )
        } else {
            assert!(
                Rc::ptr_eq(e, &t.err),
                "READ native original propagated error"
            );
            assert_eq!(
                (e.name, e.message, e.code, e.ids),
                ("TypeError", "down", Some("READ"), Some([9, 4])),
                "ERROR native propagated fields"
            )
        }
    }
    if method == "open" && matches!(case, "binding-close" | "binding-error-close") {
        assert_eq!(
            t.binding_reads.get(),
            1,
            "OPEN native independent binding read count"
        );
        assert_eq!(
            t.binding_closes.get(),
            1,
            "OPEN native independent binding close count"
        );
    }
    if matches!(method, "markets" | "raw") {
        let expected = if case == "empty" {
            vec![]
        } else if case == "inverse" {
            vec!["FUTURES"]
        } else if case == "callee-reenter" {
            vec!["SWAP"]
        } else {
            vec!["SWAP", "FUTURES"]
        };
        let actual = t
            .requests
            .borrow()
            .iter()
            .map(|(_, p)| text(&get(p, "instType")))
            .collect::<Vec<_>>();
        assert_eq!(
            actual, expected,
            "REQUEST native independent instrument sequence"
        );
        let prefix = if method == "markets" {
            vec![
                "Promise.all",
                "this.getInstrumentTypes",
                "this.xhr.get",
                "this.getMarketInstrumentsEndpoint",
            ]
        } else {
            vec![
                "Promise.all",
                "this.getInstrumentTypes",
                "this.xhr.get:ENDPOINTS.TICKERS",
            ]
        };
        let expected = if case == "empty" {
            &prefix[..2]
        } else {
            &prefix[..]
        };
        assert_eq!(
            &t.captures.borrow()[..expected.len()],
            expected,
            "REQUEST native pre-argument callee capture order"
        );
    }
    if method == "open" {
        let route_types = t
            .requests
            .borrow()
            .iter()
            .map(|(route, p)| (route.clone(), text(&get(p, "instType"))))
            .collect::<Vec<_>>();
        let expected: Vec<(String, String)> = if case == "mode-missing" {
            vec![]
        } else if case == "empty" {
            vec![("BALANCE".into(), "undefined".into())]
        } else if matches!(case, "reject" | "binding-error-close") {
            vec![("BALANCE".into(), "SWAP".into())]
        } else {
            vec![
                ("BALANCE".into(), "SWAP".into()),
                ("POSITIONS".into(), "SWAP".into()),
                ("POSITIONS".into(), "FUTURES".into()),
            ]
        };
        assert_eq!(
            route_types, expected,
            "OPEN native independent request sequence"
        );
    }
    if method == "leverage" {
        assert_eq!(
            &t.captures.borrow()[..3],
            &["flatten", "p-iteration.map", "chunk"],
            "LEVERAGE native outer callee capture order"
        );
        if case == "batch-boundary" {
            let widths = t
                .requests
                .borrow()
                .iter()
                .map(|(_, p)| text(&get(p, "instId")).split(',').count())
                .collect::<Vec<_>>();
            assert_eq!(
                widths,
                [20, 20, 1],
                "LEVERAGE native independent batch widths"
            );
        }
    }
    let output = match result {
        Ok(v) => json!({"value":pack(&v)}),
        Err(e) => {
            json!({"error":{"name":e.name,"message":e.message,"code":e.code.map(|x|json!(x)).unwrap_or(json!({"u":true})),"ids":e.ids.map(|x|json!(x)).unwrap_or(json!({"u":true})),"original":Rc::ptr_eq(&e,&t.err)}})
        }
    };
    json!({"method":method,"case":case,"output":output,"events":t.events.borrow().clone()})
}
#[test]
fn market_account_native_before_source() {
    let mut cases = vec![];
    for case in [
        "normal",
        "empty",
        "reject",
        "inverse",
        "no-pretty",
        "max-nan",
        "callee-reenter",
    ] {
        cases.push(fixture("markets", case));
    }
    for case in ["normal", "empty", "reject", "non-array"] {
        cases.push(fixture("raw", case));
    }
    for case in ["normal", "empty", "get-error", "unknown", "zero-open"] {
        cases.push(fixture("tickers", case));
    }
    for case in ["normal", "empty", "reject", "hedged"] {
        cases.push(fixture("metadata", case));
    }
    for case in ["normal", "empty", "reject", "no-symbols"] {
        cases.push(fixture("combined", case));
    }
    for case in [
        "normal",
        "empty",
        "reject",
        "mode-missing",
        "binding-close",
        "binding-error-close",
    ] {
        cases.push(fixture("open", case));
    }
    for case in [
        "normal",
        "empty",
        "reject",
        "unified",
        "level-string",
        "level-zero",
        "duplicate-currency",
    ] {
        cases.push(fixture("balance", case));
    }
    for case in ["normal", "empty", "reject", "disposed", "batch-boundary"] {
        cases.push(fixture("leverage", case));
    }
    assert_eq!(cases.len(), 42, "independent account read fixture floor");
    let input = json!({"cases":cases});
    let mut child = Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/market-account-reads-oracle.mjs"),
        )
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
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "source oracle {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        source.as_array().unwrap().len(),
        42,
        "independent source fixture floor"
    );
    for (i, (native, source)) in input["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(source.as_array().unwrap())
        .enumerate()
    {
        assert!(
            native == source,
            "source mismatch {i} {}/{}",
            native["method"],
            native["case"]
        );
    }
}
