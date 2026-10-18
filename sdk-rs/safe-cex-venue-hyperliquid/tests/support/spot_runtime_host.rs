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
pub fn truthy(v: &N) -> bool {
    !matches!(v, N::U | N::Null | N::B(false))
        && !matches!(v,N::Num(n) if *n==0.||n.is_nan())
        && !matches!(v,N::S(t) if t.0.is_empty())
}
pub fn string(v: &N) -> JsString {
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
pub fn freeze(v: &N) -> N {
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
    runtime_callbacks: HashMap<String, RuntimeCallback>,
    metadata: HashMap<String, (String, u8)>,
    timers: Vec<(N, N, N)>,
    clock: f64,
    originals: Vec<(String, Vec<N>)>,
    data_reads: usize,
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
                runtime_callbacks: HashMap::new(),
                metadata: HashMap::new(),
                timers: vec![],
                clock: 100000.,
                originals: vec![],
                data_reads: 0,
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
    pub fn ready(&self, r: Result<N, N>) -> N {
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
        Ok(match k {
            "Date" => object(vec![("now", N::Builtin("now".into()))]),
            "Math" => object(vec![
                ("min", N::Builtin("min".into())),
                ("floor", N::Builtin("floor".into())),
                ("round", N::Builtin("round".into())),
                ("abs", N::Builtin("abs".into())),
                ("log10", N::Builtin("log10".into())),
            ]),
            "Number" => object(vec![
                ("MAX_SAFE_INTEGER", N::Num(9007199254740991.)),
                ("isFinite", N::Builtin("finite".into())),
            ]),
            "Object" => object(vec![("freeze", N::Builtin("freeze".into()))]),
            "JSON" => object(vec![("stringify", N::Builtin("stringify".into()))]),
            "Infinity" => N::Num(f64::INFINITY),
            "MarginType" | "PositionMode" | "OrderSide" | "OrderType" | "OrderTimeInForce" => {
                Ok(get(&self.enums, k))?
            }
            _ => N::Builtin(k.into()),
        })
    }
    fn get(&mut self, v: &N, k: &str) -> Result<N, N> {
        if matches!(
            k,
            "recordMessageHandlerStats" | "cleanupMessageHandlerStats"
        ) {
            return Ok(N::Builtin("stats".into()));
        }
        if k == "data" && flag(&self.input(), "changingData") {
            let mut i = self.inner.borrow_mut();
            i.data_reads += 1;
            if i.data_reads >= 4 {
                return Ok(N::Null);
            }
        }
        if let N::Builtin(id) = v {
            if let Some((name, len)) = self.inner.borrow().metadata.get(id) {
                return Ok(match k {
                    "name" => s(name),
                    "length" => N::Num(*len as f64),
                    _ => N::U,
                });
            }
        }
        if matches!(v, N::A(_))
            && matches!(k, "find" | "some" | "forEach" | "reduce" | "push" | "join")
        {
            return Ok(N::Builtin(k.into()));
        }
        if let N::O(_) = v {
            if k == "then" && self.inner.borrow().promises.iter().any(|(p, _)| eq(p, v)) {
                return Ok(N::Builtin("then".into()));
            }
        }
        if matches!(v, N::Set(_)) && k == "add" {
            return Ok(N::Builtin("set-add".into()));
        }
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
        let capture = self.inner.borrow().runtime_callbacks.get(id).cloned();
        if let Some(c) = capture {
            return self.run_callback(r, &c, args);
        }
        if let Some(v) = self.runtime_call(id, r, args) {
            return v;
        }
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
                self.inner
                    .borrow_mut()
                    .originals
                    .push(("post".into(), args.to_vec()));
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
            "now" => return Ok(N::Num(self.inner.borrow().clock)),
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
                self.inner
                    .borrow_mut()
                    .originals
                    .push(("serialize".into(), args.to_vec()));
                self.record("serialize", args);
                let t = safe_cex_num::js_json_stringify(&json_value(&first))
                    .unwrap()
                    .map(|t| JsString(t.encode_utf16().collect()));
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
        eq(a, b) || matches!((a,b),(N::Builtin(a),N::Builtin(b)) if a==b)
    }
    fn imported(&mut self, k: &str) -> Result<N, N> {
        Ok(match k {
            "ENDPOINTS" => self.endpoints.clone(),
            "spot.GRAFANA_ENDPOINTS_EVERY" | "spot.GRAFANA_ENDPOINTS_PERIODIC" => arr(vec![]),
            "MarginType" | "OrderType" | "OrderSide" | "OrderTimeInForce" => get(&self.enums, k),
            "PositionSide" | "PositionMode" | "LogSeverity" => get(&self.enums, k),
            "hyperliquidClientOrderIdRule" => object(vec![("rule", s("hyperliquid"))]),
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
            "+" => N::Num(a + b),
            ">=" => N::B(a >= b),
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
    fn construct(&mut self, ctor: &N, args: &[N]) -> Result<N, N> {
        if let N::Builtin(id) = ctor {
            match id.as_str() {
                "Map" => return Ok(N::Map(Rc::new(RefCell::new(vec![])))),
                "Set" => return Ok(N::Set(Rc::new(RefCell::new(vec![])))),
                "Promise" => {
                    let p = self.manual_promise();
                    let resolve = self.register(RuntimeCallback::Resolve(p.clone()), "", 1);
                    self.call(&args[0], &N::U, &[resolve])?;
                    return Ok(p);
                }
                _ => {}
            }
        }
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

use safe_cex_venue_hyperliquid::{
    balance_state::*, lifecycle_delegates::*, market_data_selectors::*, order_request_leaves::*,
    spot_client::*, spot_public_subscriptions::*, spot_runtime::*, spot_trading::*,
    spot_ws_dispatch::*, ws_emission::*, ws_transport_leaves::*,
};
impl SpotClientHost for Host {
    #[allow(unused_variables)]
    fn client_callback(&mut self, body: ClientCallback<N>) -> Result<N, N> {
        Ok(self.register(RuntimeCallback::Client(body), "", 1))
    }
    #[allow(unused_variables)]
    fn class_pair(&mut self, name: &str, length: u8, base: Option<&str>) -> Result<(N, N), N> {
        let proto = object(vec![]);
        let ctor = object(vec![]);
        self.define_hidden(&ctor, "name", &s(name))?;
        self.define_hidden(&ctor, "length", &N::Num(length as f64))?;
        self.define_hidden(&ctor, "prototype", &proto)?;
        self.define_hidden(&proto, "constructor", &ctor)?;
        if let Some(base) = base {
            if let N::O(o) = &ctor {
                o.borrow_mut().prototype = Some(s(base))
            }
            if let N::O(o) = &proto {
                o.borrow_mut().prototype = Some(s(&format!("{base}.prototype")))
            }
        }
        Ok((ctor, proto))
    }
    #[allow(unused_variables)]
    fn define_hidden(&mut self, object: &N, key: &str, value: &N) -> Result<(), N> {
        cache::Host::default()
            .put(object, key, value, false)
            .map_err(|e| s(&e))
    }
    #[allow(unused_variables)]
    fn native_method(&mut self, name: &str, length: u8, body: ClientCallback<N>) -> Result<N, N> {
        Ok(self.register(RuntimeCallback::Client(body), name, length))
    }
    #[allow(unused_variables)]
    fn allocate_class(&mut self, prototype: &N, new_target: &N) -> Result<N, N> {
        let own = get(new_target, "prototype");
        let v = super::cache::object(vec![]);
        if let N::O(o) = &v {
            o.borrow_mut().prototype = Some(if matches!(own, N::O(_)) {
                own
            } else {
                prototype.clone()
            })
        }
        Ok(v)
    }
}
impl SpotTradingHost for Host {
    #[allow(unused_variables)]
    fn trading_callback(&mut self, capture: TradingCallback<N>) -> Result<N, N> {
        Ok(self.register(RuntimeCallback::Trading(capture), "", 1))
    }
    #[allow(unused_variables)]
    fn unary_number(&mut self, value: &N) -> Result<N, N> {
        Ok(N::Num(cache::num(value)))
    }
    #[allow(unused_variables)]
    fn has_property(&mut self, key: &str, value: &N) -> Result<bool, N> {
        Ok(match value {
            N::O(o) => {
                o.borrow().fields.iter().any(|p| p.0 == key)
                    || o.borrow()
                        .prototype
                        .as_ref()
                        .is_some_and(|p| !matches!(get(p, key), N::U))
            }
            _ => false,
        })
    }
    #[allow(unused_variables)]
    fn trading_binding(&mut self, value: &N, keys: &[&str]) -> Result<Vec<N>, N> {
        keys.iter().map(|key| self.get(value, key)).collect()
    }
}
impl SpotPublicHost for Host {
    #[allow(unused_variables)]
    fn append_literal(&mut self, array: &N, value: &N) -> Result<(), N> {
        if let N::A(a) = array {
            a.borrow_mut().push(value.clone());
            Ok(())
        } else {
            panic!("literal array")
        }
    }
    #[allow(unused_variables)]
    fn decrement(&mut self, value: &N) -> Result<N, N> {
        Ok(N::Num(cache::num(value) - 1.))
    }
    #[allow(unused_variables)]
    fn negate(&mut self, value: &N) -> Result<N, N> {
        Ok(N::Num(-cache::num(value)))
    }
    #[allow(unused_variables)]
    fn public_callback(&mut self, capture: PublicCallback<N>) -> Result<N, N> {
        Ok(self.register(RuntimeCallback::Public(capture), "", 1))
    }
    #[allow(unused_variables)]
    fn public_arrow(
        &mut self,
        owner: &N,
        name: &str,
        length: u8,
        body: PublicMethod,
    ) -> Result<N, N> {
        Ok(self.register(
            RuntimeCallback::PublicArrow(owner.clone(), body),
            name,
            length,
        ))
    }
    #[allow(unused_variables)]
    fn base_socket(&mut self, parent: &N, target: &N, prototype: &N) -> Result<N, N> {
        self.make_socket(parent, target, prototype)
    }
    #[allow(unused_variables)]
    fn assign_index(&mut self, object: &N, key: &N, value: &N) -> Result<(), N> {
        self.set(object, &String::from_utf16_lossy(&string(key).0), value)
    }
    #[allow(unused_variables)]
    fn delete_index(&mut self, object: &N, key: &N) -> Result<(), N> {
        if let N::O(o) = object {
            o.borrow_mut()
                .fields
                .retain(|p| p.0 != String::from_utf16_lossy(&string(key).0))
        }
        Ok(())
    }
    #[allow(unused_variables)]
    fn record_binding(&mut self, value: &N, keys: &[&str]) -> Result<Vec<N>, N> {
        keys.iter().map(|key| self.get(value, key)).collect()
    }
}
impl SpotWsHost for Host {
    #[allow(unused_variables)]
    fn event_data(&mut self, event: &N) -> Result<N, N> {
        self.get(event, "data")
    }
    #[allow(unused_variables)]
    fn is_nullish(&self, value: &N) -> bool {
        SpotAccountHost::is_nullish(self, value)
    }
    #[allow(unused_variables)]
    fn is_array(&mut self, value: &N) -> Result<bool, N> {
        SpotAccountHost::is_array(self, value)
    }
    #[allow(unused_variables)]
    fn equal(&self, left: &N, right: &N) -> bool {
        eq(left, right)
    }
    #[allow(unused_variables)]
    fn number(&self, value: f64) -> N {
        SpotAccountHost::number(self, value)
    }
    #[allow(unused_variables)]
    fn less(&mut self, left: &N, right: &N) -> Result<bool, N> {
        Ok(cache::num(left) < cache::num(right))
    }
    #[allow(unused_variables)]
    fn greater(&mut self, left: &N, right: &N) -> Result<bool, N> {
        Ok(cache::num(left) > cache::num(right))
    }
    #[allow(unused_variables)]
    fn index(&mut self, value: &N, key: &N) -> Result<N, N> {
        SpotAccountHost::index(self, value, key)
    }
    #[allow(unused_variables)]
    fn array(&mut self) -> Result<N, N> {
        Ok(arr(vec![]))
    }
    #[allow(unused_variables)]
    fn parse_float(&mut self, value: &N) -> Result<f64, N> {
        panic!("unused runtime primitive SpotWsHost::parse_float")
    }
    #[allow(unused_variables)]
    fn template_string(&mut self, value: &N) -> Result<JsString, N> {
        SpotAccountHost::template_string(self, value)
    }
    #[allow(unused_variables)]
    fn json_parse(&mut self, data: &N) -> Result<N, N> {
        panic!("unused runtime primitive SpotWsHost::json_parse")
    }
    #[allow(unused_variables)]
    fn order_side(&mut self, buy: bool) -> Result<N, N> {
        panic!("unused runtime primitive SpotWsHost::order_side")
    }
    #[allow(unused_variables)]
    fn for_in(&mut self, value: &N) -> Result<N, N> {
        panic!("unused runtime primitive SpotWsHost::for_in")
    }
    #[allow(unused_variables)]
    fn for_in_next(&mut self, iterator: &N) -> Result<Option<N>, N> {
        panic!("unused runtime primitive SpotWsHost::for_in_next")
    }
    #[allow(unused_variables)]
    fn private_class(&mut self) -> Result<(N, N), N> {
        SpotClientHost::class_pair(
            self,
            "HyperliquidSpotPrivateWebSocket",
            1,
            Some("BaseWebSocket"),
        )
    }
    #[allow(unused_variables)]
    fn construct_base(&mut self, parent: &N, new_target: &N, prototype: &N) -> Result<N, N> {
        self.make_socket(parent, new_target, prototype)
    }
    #[allow(unused_variables)]
    fn arrow(&mut self, capture: &N, name: &str, length: u8, body: PrivateMethod) -> Result<N, N> {
        Ok(self.register(
            RuntimeCallback::PrivateArrow(capture.clone(), body),
            name,
            length,
        ))
    }
}
impl WsEmissionHost for Host {
    #[allow(unused_variables)]
    fn boolean(&self, value: bool) -> N {
        SpotAccountHost::boolean(self, value)
    }
    #[allow(unused_variables)]
    fn system_types(&mut self, venue: WsVenue) -> Result<N, N> {
        panic!("unused runtime primitive WsEmissionHost::system_types")
    }
    #[allow(unused_variables)]
    fn iterator(&mut self, value: &N) -> Result<N, N> {
        Ok(N::Iter(Rc::new(RefCell::new((
            if let N::A(a) = value {
                a.borrow().clone()
            } else {
                panic!("iterator")
            },
            0,
        )))))
    }
    #[allow(unused_variables)]
    fn next(&mut self, iterator: &N) -> Result<Option<N>, N> {
        let N::Iter(a) = iterator else {
            panic!("iterator")
        };
        let mut a = a.borrow_mut();
        let value = a.0.get(a.1).cloned();
        a.1 += 1;
        Ok(value)
    }
    #[allow(unused_variables)]
    fn close_after_throw(&mut self, iterator: &N) {}
    #[allow(unused_variables)]
    fn triple(&mut self, value: &N) -> Result<[N; 3], N> {
        panic!("unused runtime primitive WsEmissionHost::triple")
    }
    #[allow(unused_variables)]
    fn string_units<'a>(&self, value: &'a N) -> Option<&'a [u16]> {
        panic!("unused runtime primitive WsEmissionHost::string_units")
    }
    #[allow(unused_variables)]
    fn spread_into(&mut self, target: &N, source: &N) -> Result<(), N> {
        SpotAccountHost::spread_into(self, target, source)
    }
    #[allow(unused_variables)]
    fn formatter(&mut self, scope: DiagnosticScope) -> Result<N, N> {
        panic!("unused runtime primitive WsEmissionHost::formatter")
    }
}
impl WsTransportHost for Host {
    #[allow(unused_variables)]
    fn urls_binding(&mut self, venue: WsVenue) -> Result<N, N> {
        panic!("unused runtime primitive WsTransportHost::urls_binding")
    }
    #[allow(unused_variables)]
    fn boolean(&self, value: bool) -> N {
        SpotAccountHost::boolean(self, value)
    }
}
impl MarketDataHost for Host {
    #[allow(unused_variables)]
    fn null(&self) -> N {
        SpotAccountHost::null(self)
    }
    #[allow(unused_variables)]
    fn number(&self, value: f64) -> N {
        SpotAccountHost::number(self, value)
    }
    #[allow(unused_variables)]
    fn is_nullish(&self, value: &N) -> bool {
        SpotAccountHost::is_nullish(self, value)
    }
    #[allow(unused_variables)]
    fn is_string(&self, value: &N) -> bool {
        matches!(value, N::S(_))
    }
    #[allow(unused_variables)]
    fn strict_equal(&self, left: &N, right: &N) -> bool {
        SpotAccountHost::strict_equal(self, left, right)
    }
    #[allow(unused_variables)]
    fn index(&mut self, object: &N, key: &N) -> Result<N, N> {
        SpotAccountHost::index(self, object, key)
    }
    #[allow(unused_variables)]
    fn iterator(&mut self, value: &N) -> Result<N, N> {
        Ok(N::Iter(Rc::new(RefCell::new((
            if let N::A(a) = value {
                a.borrow().clone()
            } else {
                panic!("iterator")
            },
            0,
        )))))
    }
    #[allow(unused_variables)]
    fn next(&mut self, iterator: &N) -> Result<Option<N>, N> {
        let N::Iter(a) = iterator else {
            panic!("iterator")
        };
        let mut a = a.borrow_mut();
        let value = a.0.get(a.1).cloned();
        a.1 += 1;
        Ok(value)
    }
    #[allow(unused_variables)]
    fn close_after_throw(&mut self, iterator: &N) {}
    #[allow(unused_variables)]
    fn construct(&mut self, function: &N, args: &[N]) -> Result<N, N> {
        SpotAccountHost::construct(self, function, args)
    }
    #[allow(unused_variables)]
    fn first_binding(&mut self, value: &N) -> Result<N, N> {
        self.get(value, "0")
    }
    #[allow(unused_variables)]
    fn unit_regex(&mut self) -> Result<N, N> {
        panic!("unused runtime primitive MarketDataHost::unit_regex")
    }
    #[allow(unused_variables)]
    fn template_string(&mut self, value: &N) -> Result<JsString, N> {
        SpotAccountHost::template_string(self, value)
    }
    #[allow(unused_variables)]
    fn subtract(&mut self, left: &N, right: &N) -> Result<N, N> {
        SpotAccountHost::binary(self, "-", left, right)
    }
}
impl BalanceHost for Host {
    #[allow(unused_variables)]
    fn null(&self) -> N {
        SpotAccountHost::null(self)
    }
    #[allow(unused_variables)]
    fn number(&self, v: f64) -> N {
        SpotAccountHost::number(self, v)
    }
    #[allow(unused_variables)]
    fn boolean(&self, v: bool) -> N {
        SpotAccountHost::boolean(self, v)
    }
    #[allow(unused_variables)]
    fn is_nullish(&self, v: &N) -> bool {
        SpotAccountHost::is_nullish(self, v)
    }
    #[allow(unused_variables)]
    fn is_undefined(&self, v: &N) -> bool {
        SpotAccountHost::is_undefined(self, v)
    }
    #[allow(unused_variables)]
    fn strict_equal(&self, a: &N, b: &N) -> bool {
        SpotAccountHost::strict_equal(self, a, b)
    }
    #[allow(unused_variables)]
    fn index(&mut self, o: &N, key: &N) -> Result<N, N> {
        SpotAccountHost::index(self, o, key)
    }
    #[allow(unused_variables)]
    fn set_index(&mut self, o: &N, key: &N, v: &N) -> Result<(), N> {
        SpotPublicHost::assign_index(self, o, key, v)
    }
    #[allow(unused_variables)]
    fn define_index(&mut self, o: &N, key: &N, v: &N) -> Result<(), N> {
        self.define_data(o, &String::from_utf16_lossy(&string(key).0), v)
    }
    #[allow(unused_variables)]
    fn array(&mut self) -> Result<N, N> {
        Ok(arr(vec![]))
    }
    #[allow(unused_variables)]
    fn array_define(&mut self, o: &N, i: usize, v: &N) -> Result<(), N> {
        if let N::A(a) = o {
            a.borrow_mut().insert(i, v.clone())
        }
        Ok(())
    }
    #[allow(unused_variables)]
    fn iterator(&mut self, v: &N) -> Result<N, N> {
        Ok(N::Iter(Rc::new(RefCell::new((
            if let N::A(a) = v {
                a.borrow().clone()
            } else {
                panic!("iterator")
            },
            0,
        )))))
    }
    #[allow(unused_variables)]
    fn next(&mut self, i: &N) -> Result<Option<N>, N> {
        let N::Iter(a) = i else { panic!("iterator") };
        let mut a = a.borrow_mut();
        let value = a.0.get(a.1).cloned();
        a.1 += 1;
        Ok(value)
    }
    #[allow(unused_variables)]
    fn close_after_throw(&mut self, i: &N) {}
    #[allow(unused_variables)]
    fn close_after_return(&mut self, i: &N) -> Result<(), N> {
        Ok(())
    }
    #[allow(unused_variables)]
    fn pair(&mut self, v: &N) -> Result<[N; 2], N> {
        SpotAccountHost::pair_binding(self, v)
    }
    #[allow(unused_variables)]
    fn template_string(&mut self, v: &N) -> Result<JsString, N> {
        SpotAccountHost::template_string(self, v)
    }
    #[allow(unused_variables)]
    fn operator(&mut self, op: BalanceOperator, a: &N, b: &N) -> Result<N, N> {
        SpotAccountHost::binary(
            self,
            match op {
                BalanceOperator::Add => "+",
                BalanceOperator::Subtract => "-",
                BalanceOperator::Multiply => "*",
                BalanceOperator::Greater => ">",
            },
            a,
            b,
        )
    }
    #[allow(unused_variables)]
    fn spread_into(&mut self, o: &N, source: &N) -> Result<(), N> {
        SpotAccountHost::spread_into(self, o, source)
    }
    #[allow(unused_variables)]
    fn spot_market_predicate(&mut self, capture: SpotMarketPredicate<N>) -> Result<N, N> {
        panic!("unused runtime primitive BalanceHost::spot_market_predicate")
    }
}
impl LifecycleHost for Host {
    #[allow(unused_variables)]
    fn null(&self) -> N {
        SpotAccountHost::null(self)
    }
    #[allow(unused_variables)]
    fn boolean(&self, value: bool) -> N {
        SpotAccountHost::boolean(self, value)
    }
    #[allow(unused_variables)]
    fn number(&self, value: f64) -> N {
        SpotAccountHost::number(self, value)
    }
    #[allow(unused_variables)]
    fn is_nullish(&self, value: &N) -> bool {
        SpotAccountHost::is_nullish(self, value)
    }
    #[allow(unused_variables)]
    fn super_get(&mut self, receiver: &N, name: &str) -> Result<N, N> {
        panic!("unused runtime primitive LifecycleHost::super_get")
    }
    #[allow(unused_variables)]
    fn lifecycle_callback(&mut self, capture: LifecycleCallback<N>) -> Result<N, N> {
        panic!("unused runtime primitive LifecycleHost::lifecycle_callback")
    }
    #[allow(unused_variables)]
    fn subtract(&mut self, left: &N, right: &N) -> Result<N, N> {
        panic!("unused runtime primitive LifecycleHost::subtract")
    }
    #[allow(unused_variables)]
    fn greater_than(&mut self, left: &N, right: &N) -> Result<bool, N> {
        panic!("unused runtime primitive LifecycleHost::greater_than")
    }
    #[allow(unused_variables)]
    fn template_string(&mut self, value: &N) -> Result<JsString, N> {
        SpotAccountHost::template_string(self, value)
    }
}
impl OrderRequestHost for Host {
    #[allow(unused_variables)]
    fn await_value(&mut self, value: N) -> impl Future<Output = Result<N, N>> {
        SpotAccountHost::await_value(self, value)
    }
    #[allow(unused_variables)]
    fn construct_error(&mut self, constructor: &N, args: &[N]) -> Result<N, N> {
        panic!("unused runtime primitive OrderRequestHost::construct_error")
    }
    #[allow(unused_variables)]
    fn instance_of(&mut self, value: &N, constructor: &N) -> Result<bool, N> {
        panic!("unused runtime primitive OrderRequestHost::instance_of")
    }
    #[allow(unused_variables)]
    fn type_is(&self, value: &N, kind: &str) -> bool {
        panic!("unused runtime primitive OrderRequestHost::type_is")
    }
    #[allow(unused_variables)]
    fn unary_number(&mut self, value: &N) -> Result<N, N> {
        panic!("unused runtime primitive OrderRequestHost::unary_number")
    }
    #[allow(unused_variables)]
    fn compare(&mut self, op: RequestComparison, left: &N, right: &N) -> Result<N, N> {
        panic!("unused runtime primitive OrderRequestHost::compare")
    }
}
impl SpotRuntimeHost for Host {
    #[allow(unused_variables)]
    fn base_exchange(&mut self, args: &[N; 3], target: &N, prototype: &N) -> Result<N, N> {
        let v = SpotClientHost::allocate_class(self, prototype, target)?;
        for (k, a) in [
            ("name", &args[0]),
            ("options", &args[1]),
            ("store", &args[2]),
        ] {
            self.define_data(&v, k, a)?;
        }
        Ok(v)
    }
    #[allow(unused_variables)]
    fn exchange_arrow(
        &mut self,
        owner: &N,
        name: &str,
        length: u8,
        method: ExchangeMethod,
    ) -> Result<N, N> {
        Ok(self.register(
            RuntimeCallback::ExchangeArrow(owner.clone(), method),
            name,
            length,
        ))
    }
    #[allow(unused_variables)]
    fn exchange_method(&mut self, name: &str, length: u8, method: ExchangeMethod) -> Result<N, N> {
        Ok(self.register(RuntimeCallback::ExchangeMethod(method), name, length))
    }
}
#[derive(Clone)]
enum RuntimeCallback {
    Client(ClientCallback<N>),
    Trading(TradingCallback<N>),
    Public(PublicCallback<N>),
    PublicArrow(N, PublicMethod),
    PrivateArrow(N, PrivateMethod),
    ExchangeArrow(N, ExchangeMethod),
    ExchangeMethod(ExchangeMethod),
    Resolve(N),
}
impl Host {
    fn register(&self, body: RuntimeCallback, name: &str, length: u8) -> N {
        let mut i = self.inner.borrow_mut();
        let id = format!("runtime-{}", i.next);
        i.next += 1;
        i.runtime_callbacks.insert(id.clone(), body);
        i.metadata.insert(id.clone(), (name.into(), length));
        N::Builtin(id)
    }
    fn run_callback(&mut self, receiver: &N, body: &RuntimeCallback, args: &[N]) -> Result<N, N> {
        match body {
            RuntimeCallback::Client(c) => run_client_callback(self, receiver, c, args),
            RuntimeCallback::Trading(c) => run_trading_callback(self, c, args),
            RuntimeCallback::Public(c) => run_public_callback(self, c, args),
            RuntimeCallback::PublicArrow(o, c) => run_public_method(self, o, *c, args),
            RuntimeCallback::PrivateArrow(o, c) => run_private_callback(self, o, *c, args),
            RuntimeCallback::ExchangeArrow(o, c) => run_exchange_method(self, o, *c, args),
            RuntimeCallback::ExchangeMethod(c) => run_exchange_method(self, receiver, *c, args),
            RuntimeCallback::Resolve(p) => {
                let result = self
                    .inner
                    .borrow()
                    .promises
                    .iter()
                    .find(|(q, _)| eq(p, q))
                    .unwrap()
                    .1
                    .clone();
                *result.borrow_mut() = Some(Ok(args.first().cloned().unwrap_or(N::U)));
                Ok(N::U)
            }
        }
    }
    fn make_socket(&mut self, parent: &N, target: &N, prototype: &N) -> Result<N, N> {
        let v = SpotClientHost::allocate_class(self, prototype, target)?;
        for (k, vv) in [
            ("parent", parent.clone()),
            ("isConnected", N::B(true)),
            ("orderBookInnerState", object(vec![])),
            ("send", N::Builtin("send".into())),
        ] {
            self.define_data(&v, k, &vv)?;
        }
        Ok(v)
    }
    fn manual_promise(&self) -> N {
        let v = object(vec![]);
        self.inner
            .borrow_mut()
            .promises
            .push((v.clone(), Rc::new(RefCell::new(None))));
        v
    }
    pub fn originals(&self) -> Vec<(String, Vec<N>)> {
        self.inner.borrow().originals.clone()
    }
    pub fn metadata(&self, f: &N) -> (String, u8) {
        let N::Builtin(id) = f else {
            panic!("native function")
        };
        self.inner.borrow().metadata[id].clone()
    }
    pub fn clock(&self, now: f64) {
        self.inner.borrow_mut().clock = now
    }
    pub fn fire_timer(&mut self) {
        let (callback, _, _) = self.inner.borrow_mut().timers.remove(0);
        self.call(&callback, &N::U, &[]).unwrap();
    }
    pub fn timer_delays(&self) -> Vec<N> {
        self.inner
            .borrow()
            .timers
            .iter()
            .map(|(_, ms, _)| ms.clone())
            .collect()
    }
    pub fn timer_count(&self) -> usize {
        self.inner.borrow().timers.len()
    }
    pub fn clear_effects(&self) {
        self.inner.borrow_mut().effects.clear();
        self.inner.borrow_mut().originals.clear();
    }
    fn runtime_call(&mut self, id: &str, r: &N, args: &[N]) -> Option<Result<N, N>> {
        let first = args.first().cloned().unwrap_or(N::U);
        let result = (|| match id {
            "stats" => return Ok(N::U),
            "freeze" => return Ok(first),
            "Boolean" => return Ok(N::B(truthy(&first))),
            "setTimeout" => {
                let mut i = self.inner.borrow_mut();
                let handle = N::Num((i.next + 1) as f64);
                i.next += 1;
                i.timers.push((first, args[1].clone(), handle.clone()));
                return Ok(handle);
            }
            "clearTimeout" => {
                self.inner
                    .borrow_mut()
                    .timers
                    .retain(|(_, _, h)| !eq(h, &first));
                return Ok(N::U);
            }
            "then" => {
                let mut runtime = self.clone();
                let receiver = r.clone();
                return Ok(self.spawn_native(Box::pin(async move {
                    let value = SpotAccountHost::await_value(&mut runtime, receiver).await?;
                    runtime.call(&first, &N::U, &[value])
                })));
            }
            "set-add" => {
                let N::Set(s) = r else { panic!("set") };
                if !s.borrow().iter().any(|v| eq(v, &first)) {
                    s.borrow_mut().push(first)
                }
                return Ok(r.clone());
            }
            "floor" => return Ok(N::Num(cache::num(&first).floor())),
            "round" => return Ok(N::Num((cache::num(&first) + 0.5).floor())),
            "abs" => return Ok(N::Num(cache::num(&first).abs())),
            "log10" => return Ok(N::Num(cache::num(&first).log10())),
            "finite" => return Ok(N::B(matches!(first,N::Num(n) if n.is_finite()))),
            "parseInt" => return Ok(N::Num(cache::num(&first).trunc())),
            "find" | "some" | "forEach" => {
                let N::A(a) = r else { panic!("array") };
                let len = a.borrow().len();
                for index in 0..len {
                    let v = a.borrow().get(index).cloned().unwrap_or(N::U);
                    let b =
                        self.call(&first, &N::U, &[v.clone(), N::Num(index as f64), r.clone()])?;
                    if truthy(&b) && id == "find" {
                        return Ok(v);
                    }
                    if truthy(&b) && id == "some" {
                        return Ok(N::B(true));
                    }
                }
                return Ok(if id == "some" { N::B(false) } else { N::U });
            }
            "reduce" => {
                let N::A(a) = r else { panic!("reduce") };
                let values = a.borrow().clone();
                let mut value = args[1].clone();
                for (i, v) in values.iter().enumerate() {
                    value = self.call(
                        &first,
                        &N::U,
                        &[value, v.clone(), N::Num(i as f64), r.clone()],
                    )?
                }
                return Ok(value);
            }
            "push" => {
                let N::A(a) = r else { panic!("push") };
                a.borrow_mut().extend_from_slice(args);
                return Ok(N::Num(a.borrow().len() as f64));
            }
            "join" => {
                let N::A(a) = r else { panic!("join") };
                return Ok(s(&a
                    .borrow()
                    .iter()
                    .map(|v| String::from_utf16_lossy(&string(v).0))
                    .collect::<Vec<_>>()
                    .join(&String::from_utf16_lossy(&string(&first).0))));
            }
            "nanoid" => return Ok(s("inner")),
            "adjust" => return Ok(first),
            "calcOrderBookTotal" => {
                for side in ["bids", "asks"] {
                    let N::A(a) = get(&first, side) else {
                        panic!("book")
                    };
                    let mut total = 0.;
                    for v in a.borrow().iter() {
                        total += cache::num(&get(v, "amount"));
                        self.set(v, "total", &N::Num(total))?
                    }
                }
                return Ok(N::U);
            }
            "send" | "caller" | "refresh-record" | "cancel-record" | "place-record" | "emit" => {
                self.inner
                    .borrow_mut()
                    .originals
                    .push((id.into(), args.to_vec()));
                self.record(id, args);
                if id == "cancel-record" {
                    return Ok(self.ready(Ok(N::U)));
                }
                if id == "place-record" {
                    return Ok(self.ready(Ok(arr(vec![N::Num(99.)]))));
                }
                return Ok(N::U);
            }
            "mintHyperliquidCloid" => {
                self.inner
                    .borrow_mut()
                    .originals
                    .push((id.into(), args.to_vec()));
                self.record(id, args);
                return Ok(s("0xminted"));
            }
            "resolveCallerClientOrderId" => {
                self.inner
                    .borrow_mut()
                    .originals
                    .push((id.into(), args.to_vec()));
                return Ok(s("client"));
            }
            "orderRequestToOrderWire" => {
                self.inner
                    .borrow_mut()
                    .originals
                    .push((id.into(), args.to_vec()));
                return Ok(object(vec![("request", first), ("index", args[1].clone())]));
            }
            "orderWiresToNormalTPSLAction" | "orderWiresToOrderAction" => {
                return Ok(object(vec![
                    (
                        "kind",
                        s(if id == "orderWiresToNormalTPSLAction" {
                            "tpsl"
                        } else {
                            "order"
                        }),
                    ),
                    ("orders", first),
                ]));
            }
            "createWalletFromPrivateKey" => return Ok(s("wallet")),
            "signL1Action" => {
                self.inner
                    .borrow_mut()
                    .originals
                    .push((id.into(), args.to_vec()));
                return Ok(self.ready(Ok(s("signature"))));
            }
            "omitUndefined" => {
                let N::A(a) = first else { panic!("ids") };
                let result = object(vec![]);
                for (index, value) in a.borrow().iter().enumerate() {
                    if !matches!(value, N::U) {
                        self.set(&result, &index.to_string(), value)?;
                    }
                }
                return Ok(result);
            }
            "price-precision" => return Ok(N::Num(cache::num(&args[1]))),
            "v" => return self.get(&first, &String::from_utf16_lossy(&string(&args[1]).0)),
            "safeString" => {
                let v = self.get(&first, &String::from_utf16_lossy(&string(&args[1]).0))?;
                return Ok(N::S(string(&v)));
            }
            "calculatePricePrecision" => return Ok(s("0.05")),
            "parsePrecision" => return Ok(first),
            "numberToString" => return Ok(N::S(string(&first))),
            "perp.createAPI" => {
                self.inner
                    .borrow_mut()
                    .originals
                    .push((id.into(), args.to_vec()));
                return Ok(object(vec![("post", N::Builtin("post".into()))]));
            }
            "rateLimit" => {
                self.inner
                    .borrow_mut()
                    .originals
                    .push((id.into(), args.to_vec()));
                return Ok(first);
            }
            _ => return Err(N::Builtin("unhandled".into())),
        })();
        if matches!(&result,Err(N::Builtin(id)) if id=="unhandled") {
            None
        } else {
            Some(result)
        }
    }
}

fn json_value(v: &N) -> safe_cex_num::JsJsonValue {
    use safe_cex_num::JsJsonValue as J;
    match v {
        N::U => J::Undefined,
        N::Null => J::Null,
        N::B(v) => J::Bool(*v),
        N::Num(v) => J::Number(*v),
        N::S(v) => J::String(
            String::from_utf16(v.0.as_slice()).expect("fixture packet strings are scalar"),
        ),
        N::A(a) => J::Array(a.borrow().iter().map(json_value).collect()),
        N::O(o) => J::Object(
            o.borrow()
                .fields
                .iter()
                .filter(|p| p.2)
                .map(|(k, v, _)| (k.clone(), json_value(v)))
                .collect(),
        ),
        _ => panic!("non-data packet"),
    }
}
