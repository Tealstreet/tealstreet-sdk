use super::{
    cache::{self, N, arr, eq, object, s},
    native::{decode, error},
};
use safe_cex_venue_hyperliquid::{
    helper_tasks::{HelperObserver, HelperTaskHost},
    json_boundaries::JsPayloadHost,
    spot_account_reads::*,
    spot_json::JsString,
    ws_ping_lifecycle::{PingCallback, WsPingHost},
};
use std::{
    cell::RefCell,
    collections::HashMap,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
pub fn get(v: &N, k: &str) -> N {
    JsPayloadHost::get(&mut cache::Host::default(), v, k).unwrap_or(N::U)
}
pub fn flag(v: &N, k: &str) -> bool {
    matches!(get(v, k), N::B(true))
}
fn truthy(v: &N) -> bool {
    !matches!(v, N::U | N::Null | N::B(false))
        && !matches!(v,N::Num(n) if *n==0.||n.is_nan())
        && !matches!(v,N::S(t) if t.0.is_empty())
}
fn string(v: &N) -> JsString {
    match v {
        N::S(t) => t.clone(),
        N::Num(n) => JsString(safe_cex_num::js_number_string(*n).encode_utf16().collect()),
        N::U => JsString("undefined".encode_utf16().collect()),
        N::Null => JsString("null".encode_utf16().collect()),
        N::B(b) => JsString(b.to_string().encode_utf16().collect()),
        N::O(o) if o.borrow().fields.iter().any(|p| p.0 == "name" && !p.2) => {
            let mut t = string(&get(v, "name")).0;
            t.extend(": ".encode_utf16());
            t.extend(string(&get(v, "message")).0);
            JsString(t)
        }
        _ => JsString("[object Object]".encode_utf16().collect()),
    }
}
fn freeze(v: &N) -> N {
    match v {
        N::A(a) => arr(a.borrow().iter().map(freeze).collect()),
        N::O(o) => {
            let v = object(vec![]);
            let N::O(r) = &v else { unreachable!() };
            r.borrow_mut().fields = o
                .borrow()
                .fields
                .iter()
                .map(|(k, v, e)| (k.clone(), freeze(v), *e))
                .collect();
            v
        }
        _ => v.clone(),
    }
}
struct Shared {
    input: N,
    effects: Vec<N>,
    tasks: Vec<Pin<Box<dyn Future<Output = ()>>>>,
    promises: Vec<(N, Rc<RefCell<Option<Result<N, N>>>>)>,
    callbacks: HashMap<String, AccountCallback<N>>,
    next: usize,
    posted: usize,
    position_reads: usize,
    swapped: N,
    loop_fn: Option<N>,
    replacement: Option<N>,
    loop_result_identity: bool,
    orders_fresh: bool,
    generated: Option<N>,
}
#[derive(Clone)]
pub struct Host {
    inner: Rc<RefCell<Shared>>,
    pub exchange: N,
    pub markets: N,
    pub tickers: N,
    pub positions: N,
    pub response: N,
    pub enums: N,
    pub endpoints: N,
}
impl Host {
    pub fn new(input: N, enums: N, endpoints: N) -> Self {
        let markets = match get(&input, "markets") {
            N::U => arr(vec![object(vec![
                ("id", s("BTC")),
                ("symbol", s("BTC/USDC")),
            ])]),
            v => v,
        };
        let tickers = arr(vec![object(vec![("id", s("BTC"))])]);
        let positions = match get(&input, "positions") {
            N::U => arr(vec![]),
            v => v,
        };
        let store = object(vec![
            ("markets", markets.clone()),
            ("tickers", tickers.clone()),
            ("positions", positions.clone()),
            (
                "loaded",
                object(vec![("markets", N::B(false)), ("custom", N::B(true))]),
            ),
            (
                "marketsBySymbol",
                object(vec![("BTC/USDC", get(&markets, "0"))]),
            ),
            ("update", N::Builtin("update".into())),
        ]);
        let options = object(vec![
            ("secret", s("secret")),
            ("key", s("key")),
            (
                "vault",
                if matches!(get(&input, "vault"), N::U | N::Null) {
                    s("vault")
                } else {
                    get(&input, "vault")
                },
            ),
            (
                "applicationId",
                if matches!(get(&input, "applicationId"), N::U | N::Null) {
                    s("app")
                } else {
                    get(&input, "applicationId")
                },
            ),
        ]);
        let exchange = object(vec![
            ("options", options),
            ("store", store),
            ("isDisposed", N::B(flag(&input, "disposed"))),
            ("isPublicDataOnly", N::B(flag(&input, "only"))),
            (
                "lastSpotBalanceRefreshAt",
                if truthy(&get(&input, "last")) {
                    get(&input, "last")
                } else {
                    N::Num(0.)
                },
            ),
            (
                "lastForcedSpotBalanceRefreshAt",
                if truthy(&get(&input, "lastForced")) {
                    get(&input, "lastForced")
                } else {
                    N::Num(0.)
                },
            ),
            ("spotBalanceRefreshInFlight", N::Null),
            ("xhr", object(vec![("post", N::Builtin("post".into()))])),
            (
                "publicWebsocket",
                object(vec![(
                    "connectAndSubscribe",
                    N::Builtin("public-connect".into()),
                )]),
            ),
            (
                "privateWebsocket",
                object(vec![(
                    "connectAndSubscribe",
                    N::Builtin("private-connect".into()),
                )]),
            ),
        ]);
        for (key, binding) in [
            ("emitAndLogError", "emit"),
            ("log", "log"),
            ("initExternalPublicData", "init"),
            ("getTickInterval", "interval"),
            ("fetchMarketsAndTickers", "markets"),
            ("_getSymbolConfigurationFromAssetData", "config"),
            ("generateFakePositionsFromMarkets", "fake"),
            ("getFakePositions", "get-fake"),
            ("refreshSpotBalance", "refresh"),
            ("fetchSpotClearinghouseState", "state"),
            ("applySpotBalances", "balances"),
            ("tick", "tick"),
        ] {
            cache::Host::default()
                .put(&exchange, key, &N::Builtin(binding.into()), true)
                .unwrap()
        }
        let response = if flag(&input, "responseNull") {
            N::Null
        } else {
            object(vec![("data", get(&input, "data"))])
        };
        Self {
            inner: Rc::new(RefCell::new(Shared {
                input,
                effects: vec![],
                tasks: vec![],
                promises: vec![],
                callbacks: HashMap::new(),
                next: 0,
                posted: 0,
                position_reads: 0,
                swapped: arr(vec![object(vec![("symbol", s("NEW"))])]),
                loop_fn: None,
                replacement: None,
                loop_result_identity: false,
                orders_fresh: false,
                generated: None,
            })),
            exchange,
            markets,
            tickers,
            positions,
            response,
            enums,
            endpoints,
        }
    }
    pub fn mark_orders_fresh(&self, value: bool) {
        self.inner.borrow_mut().orders_fresh = value
    }
    pub fn fire_loop(&mut self) {
        let expected = self.ready(Ok(s("replacement")));
        self.inner.borrow_mut().replacement = Some(expected.clone());
        let callback = self.inner.borrow().loop_fn.clone().unwrap();
        self.set(
            &self.exchange.clone(),
            "tick",
            &N::Builtin("replacement-tick".into()),
        )
        .unwrap();
        let returned = self.call(&callback, &object(vec![]), &[]).unwrap();
        self.inner.borrow_mut().loop_result_identity = eq(&returned, &expected);
        self.drain();
    }
    pub fn input(&self) -> N {
        self.inner.borrow().input.clone()
    }
    pub fn effects(&self) -> N {
        arr(self.inner.borrow().effects.clone())
    }
    fn record(&self, label: &str, args: &[N]) {
        let mut a = vec![s(label)];
        a.extend(args.iter().map(freeze));
        self.inner.borrow_mut().effects.push(arr(a))
    }
    pub fn promise(&self, observer: HelperObserver<N, N>) -> N {
        self.spawn_native(Box::pin(observer))
    }
    fn ready(&self, r: Result<N, N>) -> N {
        self.spawn_native(Box::pin(std::future::ready(r)))
    }
    pub fn native_result(&self, value: &N) -> Result<N, N> {
        self.inner
            .borrow()
            .promises
            .iter()
            .find(|(p, _)| eq(p, value))
            .unwrap()
            .1
            .borrow()
            .clone()
            .expect("ACCOUNT native promise unresolved")
    }
    pub fn drain(&self) {
        for _ in 0..100 {
            let tasks = std::mem::take(&mut self.inner.borrow_mut().tasks);
            if tasks.is_empty() {
                return;
            }
            let mut pending = vec![];
            let mut cx = Context::from_waker(Waker::noop());
            for mut task in tasks {
                if task.as_mut().poll(&mut cx).is_pending() {
                    pending.push(task)
                }
            }
            self.inner.borrow_mut().tasks.extend(pending)
        }
        panic!("ACCOUNT finite task floor exceeded iteration99")
    }
    pub fn final_state(&self, flight_alias: bool, outer_distinct: bool) -> N {
        object(vec![
            ("last", get(&self.exchange, "lastSpotBalanceRefreshAt")),
            (
                "lastForced",
                get(&self.exchange, "lastForcedSpotBalanceRefreshAt"),
            ),
            (
                "inFlight",
                N::B(truthy(&get(&self.exchange, "spotBalanceRefreshInFlight"))),
            ),
            ("flightAlias", N::B(flight_alias)),
            ("outerDistinct", N::B(outer_distinct)),
            ("posted", N::Num(self.inner.borrow().posted as f64)),
        ])
    }
    pub fn aliases(&self, family: &str, result: &N) -> N {
        let original = if flag(&self.input(), "positionsSwap") {
            self.inner.borrow().swapped.clone()
        } else {
            self.positions.clone()
        };
        let positions_original = family == "positions"
            && cache::num(&get(&self.positions, "length")) > 0.
            && matches!(result,N::A(a) if a.borrow().iter().all(|p|matches!(&original,N::A(o) if o.borrow().iter().any(|q|eq(p,q)))));
        let positions_fresh =
            family == "positions" && matches!(result, N::A(_)) && !eq(result, &original);
        let generated = self.inner.borrow().generated.clone();
        let generated_rows = if family == "positions" {
            if let (N::A(a), Some(N::A(b))) = (result, generated.as_ref()) {
                arr(a
                    .borrow()
                    .iter()
                    .map(|p| {
                        N::Num(
                            b.borrow()
                                .iter()
                                .position(|q| eq(p, q))
                                .map(|i| i as f64)
                                .unwrap_or(-1.),
                        )
                    })
                    .collect())
            } else {
                arr(vec![])
            }
        } else {
            arr(vec![])
        };
        let generated_fresh = family == "positions"
            && matches!(result, N::A(_))
            && generated.as_ref().is_some_and(|g| !eq(g, result));
        object(vec![
            (
                "loopResultIdentity",
                N::B(self.inner.borrow().loop_result_identity),
            ),
            ("ordersFresh", N::B(self.inner.borrow().orders_fresh)),
            ("generatedRows", generated_rows),
            ("generatedFresh", N::B(generated_fresh)),
            (
                "validationErrorAlias",
                N::B(
                    family == "validate"
                        && eq(
                            &get(result, "error"),
                            &get(
                                &get(
                                    &get(&get(&self.input(), "validationError"), "response"),
                                    "data",
                                ),
                                "msg",
                            ),
                        ),
                ),
            ),
            ("stateData", N::B(eq(result, &get(&self.response, "data")))),
            ("positionsOriginal", N::B(positions_original)),
            ("positionsFresh", N::B(positions_fresh)),
        ])
    }
}
struct Await {
    host: Host,
    value: N,
    yielded: bool,
}
impl Future for Await {
    type Output = Result<N, N>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.yielded {
            self.yielded = true;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        let inner = self.host.inner.borrow();
        if let Some((_, result)) = inner.promises.iter().find(|(p, _)| eq(p, &self.value)) {
            if let Some(v) = result.borrow().clone() {
                Poll::Ready(v)
            } else {
                Poll::Pending
            }
        } else {
            Poll::Ready(Ok(self.value.clone()))
        }
    }
}
impl HelperTaskHost<N, N> for Host {
    fn start_task(&self, mut task: Pin<Box<dyn Future<Output = ()>>>) {
        let mut cx = Context::from_waker(Waker::noop());
        if task.as_mut().poll(&mut cx).is_pending() {
            self.inner.borrow_mut().tasks.push(task)
        }
    }
    fn settle_return(&self, r: Result<N, N>) -> AccountFuture<N, N> {
        match r {
            Err(e) => Box::pin(std::future::ready(Err(e))),
            Ok(v) => Box::pin(Await {
                host: self.clone(),
                value: v,
                yielded: false,
            }),
        }
    }
}
impl WsPingHost for Host {
    type Value = N;
    type Error = N;
    fn undefined(&self) -> N {
        N::U
    }
    fn literal(&self, t: &JsString) -> N {
        N::S(t.clone())
    }
    fn truthy(&self, v: &N) -> bool {
        truthy(v)
    }
    fn global(&mut self, k: &str) -> Result<N, N> {
        Ok(if k == "Error" || k == "parseFloat" {
            N::Builtin(k.into())
        } else {
            object(vec![(
                match k {
                    "Date" => "now",
                    "Math" => "min",
                    _ => "stringify",
                },
                N::Builtin(
                    match k {
                        "Date" => "now",
                        "Math" => "min",
                        _ => "stringify",
                    }
                    .into(),
                ),
            )])
        })
    }
    fn get(&mut self, v: &N, k: &str) -> Result<N, N> {
        if k == "positions"
            && flag(&self.input(), "positionsSwap")
            && eq(v, &get(&self.exchange, "store"))
        {
            let mut i = self.inner.borrow_mut();
            let value = if i.position_reads == 0 {
                self.positions.clone()
            } else {
                i.swapped.clone()
            };
            i.position_reads += 1;
            return Ok(value);
        }
        if let N::O(_) = v {
            if !matches!(get(v, "$date"), N::U) && matches!(k, "subtract" | "valueOf") {
                return Ok(N::Builtin(k.into()));
            }
        }
        if matches!(v, N::S(_)) && k == "split" {
            return Ok(N::Builtin("split".into()));
        }
        if matches!(v, N::Null | N::U) {
            return Err(error(
                "TypeError",
                &format!(
                    "Cannot read properties of {} (reading '{k}')",
                    if matches!(v, N::Null) {
                        "null"
                    } else {
                        "undefined"
                    }
                ),
            ));
        }
        JsPayloadHost::get(&mut cache::Host::default(), v, k).map_err(|e| s(&e))
    }
    fn set(&mut self, v: &N, k: &str, value: &N) -> Result<(), N> {
        cache::Host::default()
            .put(v, k, value, true)
            .map_err(|e| s(&e))
    }
    fn define_data(&mut self, v: &N, k: &str, value: &N) -> Result<(), N> {
        self.set(v, k, value)
    }
    fn object(&mut self) -> Result<N, N> {
        Ok(object(vec![]))
    }
    fn callback(&mut self, _: PingCallback<N>) -> Result<N, N> {
        panic!("unused ping capture")
    }
    fn call(&mut self, f: &N, r: &N, args: &[N]) -> Result<N, N> {
        let N::Builtin(id) = f else {
            panic!("unexpected callee {f:?}")
        };
        let capture = self.inner.borrow().callbacks.get(id).cloned();
        if let Some(c) = capture {
            return run_account_callback(self, &c, args);
        }
        let first = args.first().cloned().unwrap_or(N::U);
        let input = self.input();
        match id.as_str() {
            "validateWalletAndPrivateKey" => {
                self.record("validate", args);
                if flag(&input, "validateReject") {
                    return Err(get(&input, "validationError"));
                }
                return Ok(N::B(!flag(&input, "invalid")));
            }
            "emit" | "log" => {
                self.record(id, args);
                if flag(
                    &input,
                    if id == "emit" {
                        "emitThrow"
                    } else {
                        "logThrow"
                    },
                ) {
                    return Err(s(id));
                }
            }
            "init" => {
                self.record("init", &[]);
                return Ok(self.ready(if flag(&input, "initReject") {
                    Err(N::U)
                } else {
                    Ok(N::B(flag(&input, "external")))
                }));
            }
            "markets" => {
                self.record("markets", &[]);
                if flag(&input, "disposedAfterMarkets") {
                    self.set(&self.exchange.clone(), "isDisposed", &N::B(true))?
                }
                return Ok(self.ready(if flag(&input, "marketsReject") {
                    Err(s("markets"))
                } else {
                    Ok(arr(vec![self.markets.clone(), self.tickers.clone()]))
                }));
            }
            "public-connect" | "private-connect" => self.record(
                "connect",
                &[s(if id == "public-connect" {
                    "public"
                } else {
                    "private"
                })],
            ),
            "post" => {
                self.inner.borrow_mut().posted += 1;
                self.record("post", args);
                if flag(&input, "disposedAfterPost") {
                    self.set(&self.exchange.clone(), "isDisposed", &N::B(true))?
                }
                return Ok(self.ready(if flag(&input, "postReject") {
                    Err(N::U)
                } else {
                    Ok(self.response.clone())
                }));
            }
            "state" => {
                if flag(&input, "fetchSyncThrow") {
                    return Err(N::U);
                }
                let observer = fetch_spot_clearinghouse_state(self.clone(), r.clone());
                return Ok(self.promise(observer));
            }
            "refresh" => {
                let observer = refresh_spot_balance(self.clone(), r.clone(), first);
                return Ok(self.promise(observer));
            }
            "tick" => {
                let observer = tick(self.clone(), r.clone());
                return Ok(self.promise(observer));
            }
            "fake" => return generate_fake_positions_from_markets(self, r, &first),
            "get-fake" => {
                let markets = get(&get(r, "store"), "markets");
                let result = generate_fake_positions_from_markets(self, r, &markets)?;
                self.inner.borrow_mut().generated = Some(result.clone());
                return Ok(result);
            }
            "balances" => self.record(
                "balances",
                &[
                    first.clone(),
                    N::B(eq(&first, &get(&get(&self.response, "data"), "balances"))),
                ],
            ),
            "update" => {
                self.record(
                    "update",
                    &[
                        first.clone(),
                        N::B(!eq(&get(&first, "positions"), &self.positions)),
                        N::B(eq(&get(&first, "markets"), &self.markets)),
                        N::B(eq(&get(&first, "tickers"), &self.tickers)),
                    ],
                );
                let N::O(o) = &first else { panic!("update") };
                for (k, v, _) in o.borrow().fields.clone() {
                    self.set(r, &k, &v)?
                }
            }
            "interval" => {
                self.record("interval", &[]);
                return Ok(N::Num(2000.));
            }
            "loop" => {
                self.record("loop", &args[1..]);
                self.inner.borrow_mut().loop_fn = Some(first)
            }
            "replacement-tick" => {
                self.record("loop-fired", &[N::B(eq(r, &self.exchange))]);
                return Ok(self.inner.borrow().replacement.clone().unwrap());
            }
            "now" => return Ok(N::Num(100000.)),
            "config" => {
                self.record("config", args);
                let c = if flag(&input, "two") {
                    object(vec![
                        ("marginType", s("isolated")),
                        ("positionMode", s("hedged")),
                        ("leverage", N::Num(2.)),
                    ])
                } else {
                    object(vec![
                        ("marginType", get(&get(&self.enums, "MarginType"), "Cross")),
                        (
                            "positionMode",
                            get(&get(&self.enums, "PositionMode"), "OneWay"),
                        ),
                        ("leverage", N::Num(1.)),
                    ])
                };
                if flag(&input, "configOverwrite") {
                    for (k, v) in [
                        ("id", s("override")),
                        ("symbol", s("override")),
                        ("side", s("long")),
                        ("extra", N::Num(7.)),
                    ] {
                        self.set(&c, k, &v)?
                    }
                }
                return Ok(c);
            }
            "createPositionId" => {
                let one = get(&get(&self.enums, "PositionMode"), "OneWay");
                return Ok(if eq(&args[2], &one) || !truthy(&args[1]) {
                    first
                } else {
                    let mut t = string(&first).0;
                    t.push(58);
                    t.extend(string(&args[1]).0);
                    N::S(JsString(t))
                });
            }
            "map" | "filter" => {
                let N::A(a) = r else { panic!("array receiver") };
                let len = a.borrow().len();
                let mut values = vec![];
                for index in 0..len {
                    let v = a.borrow().get(index).cloned().unwrap_or(N::U);
                    let mapped =
                        self.call(&first, &N::U, &[v.clone(), N::Num(index as f64), r.clone()])?;
                    if id == "map" {
                        values.push(mapped)
                    } else if truthy(&mapped) {
                        values.push(v)
                    }
                }
                return Ok(arr(values));
            }
            "flat" => {
                let N::A(a) = r else { panic!("flat") };
                let mut values = vec![];
                for v in a.borrow().iter() {
                    if let N::A(a) = v {
                        values.extend(a.borrow().clone())
                    } else {
                        values.push(v.clone())
                    }
                }
                return Ok(arr(values));
            }
            "includes" => {
                let N::A(a) = r else { panic!("includes") };
                return Ok(N::B(a.borrow().iter().any(|v| eq(v, &first))));
            }
            "parseFloat" => {
                let t = String::from_utf16_lossy(&string(&first).0);
                let t = t.trim_start();
                let mut n = f64::NAN;
                if t.starts_with("Infinity") {
                    n = f64::INFINITY
                } else if t.starts_with("-Infinity") {
                    n = f64::NEG_INFINITY
                } else {
                    for end in (1..=t.len()).rev().filter(|end| t.is_char_boundary(*end)) {
                        if let Ok(value) = t[..end].parse() {
                            n = value;
                            break;
                        }
                    }
                }
                return Ok(N::Num(n));
            }
            "min" => {
                let (a, b) = (cache::num(&first), cache::num(&args[1]));
                return Ok(N::Num(if a.is_nan() || b.is_nan() {
                    f64::NAN
                } else {
                    a.min(b)
                }));
            }
            "dayjs" => {
                return Ok(object(vec![(
                    "$date",
                    if args.is_empty() {
                        N::Num(100000.)
                    } else {
                        first
                    },
                )]));
            }
            "valueOf" => return Ok(get(r, "$date")),
            "subtract" => {
                let unit = String::from_utf16_lossy(&string(&args[1]).0);
                let scale = match unit.as_str() {
                    "m" => 60000.,
                    "h" => 3600000.,
                    "d" => 86400000.,
                    "w" => 604800000.,
                    "s" => 1000.,
                    _ => 1.,
                };
                return Ok(object(vec![(
                    "$date",
                    N::Num(cache::num(&get(r, "$date")) - cache::num(&first) * scale),
                )]));
            }
            "split" => {
                let N::S(t) = r else { panic!("split receiver") };
                let start = t.0.iter().position(|u| (*u >= 48) && (*u <= 57)).unwrap();
                let end = t.0[start..]
                    .iter()
                    .position(|u| *u < 48 || *u > 57)
                    .map(|i| start + i)
                    .unwrap_or(t.0.len());
                return Ok(arr(vec![
                    N::S(JsString(t.0[..start].to_vec())),
                    N::S(JsString(t.0[start..end].to_vec())),
                    N::S(JsString(t.0[end..].to_vec())),
                ]));
            }
            "stringify" => {
                self.record("serialize", args);
                let t = JsPayloadHost::json_stringify(&mut cache::Host::default(), &first).unwrap();
                return Ok(t.map(N::S).unwrap_or(N::U));
            }
            _ => panic!("unbound callee {id}"),
        };
        Ok(N::U)
    }
}
impl SpotAccountHost for Host {
    fn null(&self) -> N {
        N::Null
    }
    fn boolean(&self, v: bool) -> N {
        N::B(v)
    }
    fn number(&self, v: f64) -> N {
        N::Num(v)
    }
    fn is_nullish(&self, v: &N) -> bool {
        matches!(v, N::Null | N::U)
    }
    fn is_undefined(&self, v: &N) -> bool {
        matches!(v, N::U)
    }
    fn is_array(&mut self, v: &N) -> Result<bool, N> {
        Ok(matches!(v, N::A(_)))
    }
    fn strict_equal(&self, a: &N, b: &N) -> bool {
        eq(a, b)
    }
    fn imported(&mut self, k: &str) -> Result<N, N> {
        Ok(match k {
            "ENDPOINTS" => self.endpoints.clone(),
            "PositionSide" | "PositionMode" | "LogSeverity" => get(&self.enums, k),
            "HyperliquidSpotExchange" => object(vec![
                ("SPOT_BALANCE_REFRESH_MS", N::Num(10000.)),
                ("SPOT_BALANCE_FORCE_REFRESH_COOLDOWN_MS", N::Num(1500.)),
            ]),
            _ => N::Builtin(k.into()),
        })
    }
    fn array(&mut self, v: &[N]) -> Result<N, N> {
        Ok(arr(v.to_vec()))
    }
    fn spread_array(&mut self, v: &N) -> Result<N, N> {
        let N::A(a) = v else { panic!("spread") };
        Ok(arr(a.borrow().clone()))
    }
    fn spread_into(&mut self, target: &N, source: &N) -> Result<(), N> {
        if let N::O(o) = source {
            for (k, v, e) in o.borrow().fields.clone() {
                if e {
                    self.define_data(target, &k, &v)?
                }
            }
        }
        Ok(())
    }
    fn template_string(&mut self, v: &N) -> Result<JsString, N> {
        Ok(string(v))
    }
    fn binary(&mut self, op: &str, a: &N, b: &N) -> Result<N, N> {
        let (a, b) = (cache::num(a), cache::num(b));
        Ok(match op {
            "-" => N::Num(a - b),
            "*" => N::Num(a * b),
            "/" => N::Num(a / b),
            "<" => N::B(a < b),
            ">" => N::B(a > b),
            _ => panic!("operator"),
        })
    }
    fn interval_binding(&mut self, v: &N) -> Result<[N; 3], N> {
        Ok([get(v, "0"), get(v, "1"), get(v, "2")])
    }
    fn pair_binding(&mut self, v: &N) -> Result<[N; 2], N> {
        Ok([get(v, "0"), get(v, "1")])
    }
    fn index(&mut self, v: &N, k: &N) -> Result<N, N> {
        self.get(v, &String::from_utf16_lossy(&string(k).0))
    }
    fn digit_regex(&mut self) -> Result<N, N> {
        Ok(N::Builtin("digit-regex".into()))
    }
    fn response_data(&mut self, v: &N) -> Result<N, N> {
        if matches!(v, N::Null | N::U) {
            return Err(error(
                "TypeError",
                &format!(
                    "Cannot destructure property 'data' of '(intermediate value)' as it is {}.",
                    if matches!(v, N::Null) {
                        "null"
                    } else {
                        "undefined"
                    }
                ),
            ));
        }
        self.get(v, "data")
    }
    fn candle_binding(&mut self, v: &N) -> Result<[N; 6], N> {
        if matches!(v, N::Null | N::U) {
            return Err(error(
                "TypeError",
                if matches!(v, N::Null) {
                    "Cannot destructure property 't' of 'object null' as it is null."
                } else {
                    "Cannot destructure property 't' of 'undefined' as it is undefined."
                },
            ));
        }
        Ok([
            self.get(v, "t")?,
            self.get(v, "o")?,
            self.get(v, "h")?,
            self.get(v, "l")?,
            self.get(v, "c")?,
            self.get(v, "v")?,
        ])
    }
    fn construct(&mut self, _: &N, args: &[N]) -> Result<N, N> {
        Ok(error(
            "Error",
            &String::from_utf16_lossy(&string(&args[0]).0),
        ))
    }
    fn throw_value(&self, v: N) -> N {
        v
    }
    fn error_value(&self, e: &N) -> N {
        e.clone()
    }
    fn callback(&mut self, c: AccountCallback<N>) -> Result<N, N> {
        let mut inner = self.inner.borrow_mut();
        let id = format!("account-callback-{}", inner.next);
        inner.next += 1;
        inner.callbacks.insert(id.clone(), c);
        Ok(N::Builtin(id))
    }
    fn await_value(&mut self, v: N) -> AccountFuture<N, N> {
        Box::pin(Await {
            host: self.clone(),
            value: v,
            yielded: false,
        })
    }
    fn spawn_native(&self, body: AccountFuture<N, N>) -> N {
        let v = object(vec![]);
        let result = Rc::new(RefCell::new(None));
        self.inner
            .borrow_mut()
            .promises
            .push((v.clone(), result.clone()));
        let h = self.clone();
        self.start_task(Box::pin(async move {
            let value = body.await;
            let value = h.settle_return(value).await;
            *result.borrow_mut() = Some(value)
        }));
        v
    }
}
pub fn input(row: &serde_json::Value) -> N {
    decode(&row["input"])
}
