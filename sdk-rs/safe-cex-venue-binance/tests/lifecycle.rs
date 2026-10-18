use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_binance::{lifecycle::*, private_event_value::*, request_weight::Failure};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn run<F: Future>(f: F) -> F::Output {
    let w = Waker::noop().clone();
    let mut cx = Context::from_waker(&w);
    let mut f = pin!(f);
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
}
fn decoded(v: Value) -> EventValue {
    EventValue::from_json(v)
}
fn wire(v: &EventValue) -> Value {
    match v {
        EventValue::Undefined => json!({"$undefined":true}),
        EventValue::Null => Value::Null,
        EventValue::Bool(v) => json!(v),
        EventValue::Number(v) => {
            if v.is_nan() {
                json!("nan")
            } else if v.is_infinite() {
                json!(if v.is_sign_negative() {
                    "-infinity"
                } else {
                    "infinity"
                })
            } else if *v == 0.0 && v.is_sign_negative() {
                json!("-zero")
            } else {
                json!(v)
            }
        }
        EventValue::String(v) => json!(String::from_utf16(v).unwrap()),
        EventValue::Array(v) => Value::Array(
            array_values(v)
                .expect("fixture array reads")
                .iter()
                .map(wire)
                .collect(),
        ),
        EventValue::Object(_) => Value::Object(
            v.spread()
                .iter()
                .map(|(k, v)| (k.clone(), wire(v)))
                .collect(),
        ),
        _ => panic!("no unsupported value in authored lifecycle graph"),
    }
}
fn p(v: &EventValue, key: &str) -> EventValue {
    v.get(key).unwrap()
}
fn own(v: &EventValue, key: &str) -> bool {
    if matches!(v, EventValue::Object(_)) {
        v.has_own(key)
    } else {
        false
    }
}
fn n(v: EventValue, expected: f64, label: &str) {
    let EventValue::Number(actual) = v else {
        panic!("{label}: native Number variant")
    };
    assert!(
        if expected.is_nan() {
            actual.is_nan()
        } else if expected == 0.0 {
            actual == 0.0 && actual.is_sign_negative() == expected.is_sign_negative()
        } else {
            actual == expected
        },
        "{label}: native value {actual:?} vs expected {expected:?}"
    );
}
fn s(v: EventValue, expected: &str, label: &str) {
    let EventValue::String(actual) = v else {
        panic!("{label}: native String variant")
    };
    assert_eq!(
        actual.as_ref(),
        expected.encode_utf16().collect::<Vec<_>>(),
        "{label}: native string"
    );
}
fn boolean(v: EventValue, expected: bool, label: &str) {
    let EventValue::Bool(actual) = v else {
        panic!("{label}: native Bool variant")
    };
    assert_eq!(actual, expected, "{label}: native bool");
}
fn arr(v: &EventValue) -> Vec<EventValue> {
    let EventValue::Array(a) = v else {
        panic!("native Array")
    };
    array_values(a).expect("fixture array reads")
}
fn syn(k: LifecycleSync) -> &'static str {
    match k {
        LifecycleSync::Update => "update",
        LifecycleSync::Log => "log",
        LifecycleSync::PublicConnect => "publicConnect",
        LifecycleSync::PrivateConnect => "privateConnect",
        LifecycleSync::SetSetting => "setting",
        LifecycleSync::Paused => "paused",
        LifecycleSync::Version => "version",
        LifecycleSync::TickInterval => "interval",
        LifecycleSync::Emit => "emit",
    }
}
fn asy(k: LifecycleAsync) -> &'static str {
    match k {
        LifecycleAsync::Overlay => "overlay",
        LifecycleAsync::External => "external",
        LifecycleAsync::Markets => "markets",
        LifecycleAsync::Tickers => "tickers",
        LifecycleAsync::StartPrivate => "startPrivate",
        LifecycleAsync::StartPublicOnly => "startPublic",
        LifecycleAsync::PositionMode => "mode",
        LifecycleAsync::Tick => "tick",
        LifecycleAsync::Orders => "orders",
        LifecycleAsync::RetryOrders => "retry",
        LifecycleAsync::Positions => "positions",
        LifecycleAsync::PositionsBalance => "account",
        LifecycleAsync::Balance => "balance",
    }
}
fn rd(k: LifecycleRead) -> &'static str {
    match k {
        LifecycleRead::NeedsOverlay => "overlay",
        LifecycleRead::Disposed => "disposed",
        LifecycleRead::PublicOnly => "publicOnly",
        LifecycleRead::PrivateConnected => "connected",
        LifecycleRead::OrdersStale => "stale",
        LifecycleRead::Loaded => "loaded",
    }
}
#[derive(Clone)]
struct Bind(Rc<State>);
struct Effect {
    k: LifecycleSync,
    args: Vec<EventValue>,
    receiver: u32,
    loaded_before: EventValue,
}
struct State {
    f: Value,
    venue: &'static str,
    family: &'static str,
    trace: RefCell<Vec<Value>>,
    effects: RefCell<Vec<Effect>>,
    async_calls: RefCell<Vec<LifecycleAsync>>,
    timers: RefCell<Vec<(TickTimer, EventValue)>>,
    timer_calls: RefCell<Vec<(u32, Vec<EventValue>)>>,
    clock: Cell<usize>,
    version: Cell<f64>,
    version_calls: Cell<usize>,
    pauses: Cell<usize>,
    setter: Cell<u32>,
    tick_version: Cell<u32>,
    disposed: Cell<bool>,
    public_only: Cell<bool>,
    stale: Cell<bool>,
    loaded: RefCell<EventValue>,
    initial_loaded: EventValue,
    markets: EventValue,
    tickers: EventValue,
    orders: EventValue,
    risk: EventValue,
    positions: EventValue,
    balance: EventValue,
    count: EventValue,
    times: Rc<ReconciliationTimes>,
    error: Failure,
}
impl State {
    fn name(&self) -> &str {
        self.f["name"].as_str().unwrap()
    }
    fn t(&self, v: Value) {
        self.trace.borrow_mut().push(v)
    }
}
fn setup(f: Value, venue: &'static str, family: &'static str) -> Bind {
    let name = f["name"].as_str().unwrap();
    let markets = if name == "length-read-before-coercion" {
        EventValue::Null
    } else {
        decoded(json!([{"id":"market-A"},{"id":"market-B"},{"id":"market-C"}]))
    };
    let tickers = if name == "length-read-before-coercion" {
        record([("length", record([("toString", number(0.0))]))])
    } else if name == "missing-length" {
        record([])
    } else {
        decoded(json!([{"id":"ticker-A","last":12.3},{"id":"ticker-B","last":4.5}]))
    };
    let loaded = decoded(
        json!({"markets":false,"tickers":false,"orders":false,"balance":false,"positions":false,"nested":{"keep":5}}),
    );
    let mut e = ExchangeError::new("lifecycle fixture");
    e.name = "RangeError".into();
    e.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "lifecycle-code"
    )));
    e.response = Some(json!({"status":503,"data":{"msg":"lifecycle detail"}}));
    let count = f
        .get("count")
        .cloned()
        .map(decoded)
        .unwrap_or(EventValue::Undefined);
    let times = Rc::new(ReconciliationTimes {
        risk: Cell::new(f["risk"].as_f64().unwrap_or(0.0)),
        account: Cell::new(f["account"].as_f64().unwrap_or(0.0)),
    });
    let disposed = f["disposed"] == true;
    let public_only = f["public_only"] == true;
    let stale = f["stale"] == true;
    Bind(Rc::new(State {
        f,
        venue,
        family,
        trace: Default::default(),
        effects: Default::default(),
        async_calls: Default::default(),
        timers: Default::default(),
        timer_calls: Default::default(),
        clock: Cell::new(0),
        version: Cell::new(1.0),
        version_calls: Cell::new(0),
        pauses: Cell::new(0),
        setter: Cell::new(1),
        tick_version: Cell::new(1),
        disposed: Cell::new(disposed),
        public_only: Cell::new(public_only),
        stale: Cell::new(stale),
        loaded: RefCell::new(loaded.clone()),
        initial_loaded: loaded,
        markets,
        tickers,
        orders: decoded(json!([{"id":"order-A","amount":0.1}])),
        risk: decoded(json!([{"id":"risk","contracts":3.6,"notional":40.2}])),
        positions: decoded(json!([{"id":"account","contracts":4.2,"notional":50.5}])),
        balance: decoded(json!({"total":20.5,"free":16.9,"used":3.6,"upnl":-0.1})),
        count,
        times,
        error: Rc::new(e),
    }))
}
impl LifecycleBindings for Bind {
    fn read(&mut self, k: LifecycleRead) -> Result<EventValue, Failure> {
        let st = &self.0;
        st.t(json!(["read", rd(k)]));
        if k == LifecycleRead::Loaded && st.name() == "loaded-fail" {
            return Err(st.error.clone());
        }
        Ok(match k {
            LifecycleRead::NeedsOverlay => EventValue::Bool(st.f["overlay"] == true),
            LifecycleRead::Disposed => EventValue::Bool(st.disposed.get()),
            LifecycleRead::PublicOnly => EventValue::Bool(st.public_only.get()),
            LifecycleRead::PrivateConnected => EventValue::Bool(st.f["connected"] != false),
            LifecycleRead::OrdersStale => EventValue::Bool(st.stale.get()),
            LifecycleRead::Loaded => st.loaded.borrow().clone(),
        })
    }
    fn capture(&mut self, k: LifecycleSync) -> Result<LifecycleCall, Failure> {
        let st = self.0.clone();
        let key = syn(k);
        st.t(json!(["capture", key]));
        if k == LifecycleSync::SetSetting && st.name() == "setter-getter-fail" {
            return Err(st.error.clone());
        }
        if k == LifecycleSync::Log && st.name() == "log-mutates-lengths" {
            if let EventValue::Array(a) = &st.tickers {
                EventValue::Array(a.clone())
                    .push(decoded(json!({"id":"ticker-C","last":6.7})))
                    .unwrap()
            }
        }
        if k == LifecycleSync::Update && st.name() == "capture-version-mutation" {
            st.version.set(st.version.get() + 1.0)
        }
        let receiver = if k == LifecycleSync::SetSetting {
            st.setter.get()
        } else {
            1
        };
        Ok(Rc::new(move |args| {
            st.t(json!(["call", key]));
            if matches!(
                k,
                LifecycleSync::Update
                    | LifecycleSync::Log
                    | LifecycleSync::SetSetting
                    | LifecycleSync::Emit
                    | LifecycleSync::PublicConnect
                    | LifecycleSync::PrivateConnect
            ) {
                st.effects.borrow_mut().push(Effect {
                    k,
                    args: args.clone(),
                    receiver,
                    loaded_before: st.loaded.borrow().clone(),
                })
            }
            if k == LifecycleSync::Update && st.name() == "update-fail"
                || k == LifecycleSync::Log && st.name() == "log-fail"
                || k == LifecycleSync::SetSetting && st.name() == "setter-call-fail"
                || k == LifecycleSync::Emit && st.name() == "emit-fail"
                || k == LifecycleSync::PrivateConnect && st.name() == "private-connect-fail"
                || k == LifecycleSync::TickInterval && st.name() == "interval-fail"
            {
                return Err(st.error.clone());
            }
            if k == LifecycleSync::Update && own(&args[0], "loaded") {
                *st.loaded.borrow_mut() = p(&args[0], "loaded")
            }
            if k == LifecycleSync::PublicConnect && st.name() == "connect-disposes"
                || k == LifecycleSync::PrivateConnect && st.name() == "connect-disposes"
                || k == LifecycleSync::Log
                    && st.name() == "log-ready-disposes"
                    && args[0].text()?.starts_with("Ready")
                || k == LifecycleSync::Log
                    && st.name() == "log-loaded-disposes"
                    && args[0].text()?.starts_with("Loaded")
            {
                st.disposed.set(true)
            }
            Ok(match k {
                LifecycleSync::Paused => {
                    st.pauses.set(st.pauses.get() + 1);
                    EventValue::Bool(
                        st.name() == "paused"
                            || st.name() == "paused-after-risk" && st.pauses.get() > 2,
                    )
                }
                LifecycleSync::Version => {
                    st.version_calls.set(st.version_calls.get() + 1);
                    number(if st.name() == "version-nan" {
                        f64::NAN
                    } else {
                        st.version.get()
                    })
                }
                LifecycleSync::TickInterval => number(7200.0),
                _ => EventValue::Undefined,
            })
        }))
    }
    fn capture_async(&mut self, k: LifecycleAsync) -> Result<LifecycleAsyncCall, Failure> {
        let st = self.0.clone();
        let key = asy(k);
        st.t(json!(["captureAsync", key]));
        Ok(Rc::new(move |args| {
            st.t(json!([
                "start",
                key,
                args.iter().map(wire).collect::<Vec<_>>()
            ]));
            st.async_calls.borrow_mut().push(k);
            let st = st.clone();
            Ok(Box::pin(async move {
                st.t(json!(["resolve", key]));
                let name = st.name();
                if name == format!("{key}-fail")
                    || name == "start-private-fail" && k == LifecycleAsync::StartPrivate
                    || name == "retry-fail" && k == LifecycleAsync::RetryOrders
                    || matches!(name, "account-fail" | "emit-fail")
                        && matches!(
                            k,
                            LifecycleAsync::PositionsBalance | LifecycleAsync::Balance
                        )
                {
                    return Err(st.error.clone());
                }
                if name == format!("{key}-disposes")
                    || name == "account-disposes"
                        && matches!(
                            k,
                            LifecycleAsync::PositionsBalance | LifecycleAsync::Balance
                        )
                {
                    st.disposed.set(true)
                }
                if name == "setter-swapped" && k == LifecycleAsync::PositionMode {
                    st.setter.set(2)
                }
                if name == "public-route-after-await" && k == LifecycleAsync::Tickers {
                    st.public_only.set(true)
                }
                if name == "version-changes"
                    && matches!(
                        k,
                        LifecycleAsync::Positions | LifecycleAsync::PositionsBalance
                    )
                {
                    st.version.set(st.version.get() + 1.0)
                }
                Ok(match k {
                    LifecycleAsync::External => EventValue::Bool(st.f["external"] == true),
                    LifecycleAsync::Markets => st.markets.clone(),
                    LifecycleAsync::Tickers => st.tickers.clone(),
                    LifecycleAsync::Orders => st.orders.clone(),
                    LifecycleAsync::Positions => st.risk.clone(),
                    LifecycleAsync::PositionMode => {
                        if name == "mode-missing" {
                            EventValue::Undefined
                        } else {
                            EventValue::Bool(true)
                        }
                    }
                    LifecycleAsync::PositionsBalance | LifecycleAsync::Balance => {
                        if name == "malformed-account" {
                            EventValue::Null
                        } else {
                            record([
                                ("balance", st.balance.clone()),
                                ("positions", st.positions.clone()),
                            ])
                        }
                    }
                    _ => EventValue::Undefined,
                })
            }))
        }))
    }
    fn now(&mut self) -> Result<f64, Failure> {
        let st = &self.0;
        st.clock.set(st.clock.get() + 1);
        st.t(json!(["clock"]));
        if st.name() == "clock-fail-first" && st.clock.get() == 1
            || st.name() == "clock-fail-risk" && st.clock.get() == 2
            || st.name() == "clock-fail-account" && st.clock.get() == 3
        {
            Err(st.error.clone())
        } else {
            Ok(99999.0 + st.clock.get() as f64)
        }
    }
    fn tick_resolver(&mut self) -> TickResolver {
        let st = self.0.clone();
        Rc::new(move || {
            let version = st.tick_version.get();
            st.t(json!(["timerCapture", version]));
            if st.name() == "timer-getter-fail" {
                return Err(st.error.clone());
            }
            if st.name() == "count-array" {
                let EventValue::Array(a) = &st.count else {
                    panic!()
                };
                EventValue::Array(a.clone()).push(number(16.0)).unwrap();
            }
            let st = st.clone();
            Ok(Rc::new(move |args| {
                st.t(json!([
                    "timerStart",
                    version,
                    args.iter().map(wire).collect::<Vec<_>>()
                ]));
                st.timer_calls.borrow_mut().push((version, args));
                if st.name() == "timer-call-fail" {
                    return Err(st.error.clone());
                }
                let st = st.clone();
                Ok(Box::pin(async move {
                    st.t(json!(["timerResolve", version]));
                    Ok(EventValue::Undefined)
                }))
            }))
        })
    }
    fn schedule(&mut self, timer: TickTimer, interval: EventValue) -> Result<(), Failure> {
        self.0.t(json!(["schedule", wire(&interval)]));
        self.0.timers.borrow_mut().push((timer, interval));
        Ok(())
    }
}
fn execute(b: &mut Bind) -> Result<(), Failure> {
    let st = b.0.clone();
    match (st.venue, st.family) {
        ("futures", "start") => run(start_futures(b)),
        ("pm", "start") => run(start_pm(b)),
        ("spot", "start") => run(start_spot(b)),
        ("futures", "private") => run(start_private_futures(b)),
        ("pm", "private") => run(start_private_pm(b)),
        ("spot", "private") => run(start_private_spot(b)),
        ("futures", "tick") => run(tick_futures(&st.times, b)),
        ("pm", "tick") => run(tick_pm(st.count.clone(), b)),
        _ => run(tick_spot(b)),
    }
}
fn original(e: &Failure, st: &State, label: &str) {
    assert!(
        Rc::ptr_eq(e, &st.error),
        "{label}: native original error Rc"
    );
    assert_eq!(e.name, "RangeError", "{label}: native error name");
    assert_eq!(
        e.message, "lifecycle fixture",
        "{label}: native error message"
    );
    assert_eq!(
        observed_error_code(&e.code),
        Some(json!("lifecycle-code")),
        "{label}: native code"
    );
    assert_eq!(
        e.response,
        Some(json!({"status":503,"data":{"msg":"lifecycle detail"}})),
        "{label}: native response"
    );
}
fn failure(e: Option<&Failure>, st: &State) -> Value {
    e.map(|e|json!({"original":Rc::ptr_eq(e,&st.error),"name":e.name,"message":e.message,"code":observed_error_code(&e.code).clone().unwrap_or(json!({"$undefined":true})),"response":e.response.clone().unwrap_or(json!({"$undefined":true}))})).unwrap_or(Value::Null)
}
fn project(st: &State, result: &Result<(), Failure>, timer_error: Option<&Failure>) -> Value {
    let effects=st.effects.borrow().iter().map(|e|{let refs=if e.k==LifecycleSync::Update{let row=&e.args[0];json!({"markets":p(row,"markets").same(&st.markets),"tickers":p(row,"tickers").same(&st.tickers),"orders":p(row,"orders").same(&st.orders),"risk":p(row,"positions").same(&st.risk),"positions":p(row,"positions").same(&st.positions),"balance":p(row,"balance").same(&st.balance),"loadedFresh":if own(row,"loaded"){json!(!p(row,"loaded").same(&e.loaded_before))}else{Value::Null},"loadedNested":own(row,"loaded")&&p(&p(row,"loaded"),"nested").same(&p(&st.initial_loaded,"nested"))})}else{Value::Null};json!({"name":syn(e.k),"args":e.args.iter().map(wire).collect::<Vec<_>>(),"receiver":e.receiver,"refs":refs})}).collect::<Vec<_>>();
    json!({"name":st.name(),"trace":*st.trace.borrow(),"effects":effects,"clocks":st.clock.get(),"version":st.version.get(),"versionCalls":st.version_calls.get(),"pauses":st.pauses.get(),"disposed":st.disposed.get(),"publicOnly":st.public_only.get(),"stale":st.stale.get(),"loaded":wire(&st.loaded.borrow()),"times":{"risk":st.times.risk.get(),"account":st.times.account.get()},"timers":st.timers.borrow().iter().map(|(_,interval)|wire(interval)).collect::<Vec<_>>(),"timerCalls":st.timer_calls.borrow().iter().map(|(version,args)|json!({"version":version,"args":args.iter().map(wire).collect::<Vec<_>>()})).collect::<Vec<_>>(),"failure":failure(result.as_ref().err(),st),"timerFailure":failure(timer_error,st)})
}
fn fixtures(family: &str, count: usize) -> Vec<Value> {
    let rows: Vec<Value> = serde_json::from_str(include_str!("fixtures/lifecycle.json")).unwrap();
    let rows = rows
        .into_iter()
        .filter(|r| r["family"] == family)
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), count, "independent authored case floor");
    rows
}
fn compare(rows: Vec<Value>, venue: &str, family: &str) {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/lifecycle.mjs"
        ))
        .args([venue, family])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "oracle {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        expected.len(),
        rows.len(),
        "source count, after independent native floor"
    );
    fn numeric(v: &Value) -> Value {
        match v {
            Value::Number(n) => json!(n.as_f64().unwrap()),
            Value::Array(a) => Value::Array(a.iter().map(numeric).collect()),
            Value::Object(o) => {
                Value::Object(o.iter().map(|(k, v)| (k.clone(), numeric(v))).collect())
            }
            _ => v.clone(),
        }
    }
    for (i, (a, b)) in rows.iter().zip(expected).enumerate() {
        assert_eq!(
            numeric(a),
            numeric(&b),
            "source {venue} {family} case{i} {}",
            a["name"]
        );
    }
}
fn calls(st: &State, k: LifecycleAsync) -> usize {
    st.async_calls.borrow().iter().filter(|v| **v == k).count()
}
fn effects(st: &State, k: LifecycleSync) -> usize {
    st.effects.borrow().iter().filter(|v| v.k == k).count()
}
fn updates(st: &State) -> Vec<EventValue> {
    st.effects
        .borrow()
        .iter()
        .filter(|v| v.k == LifecycleSync::Update)
        .map(|e| e.args[0].clone())
        .collect()
}
fn native_loaded(st: &State, label: &str) {
    for e in st
        .effects
        .borrow()
        .iter()
        .filter(|e| e.k == LifecycleSync::Update)
    {
        let row = &e.args[0];
        if own(row, "loaded") {
            let flags = p(row, "loaded");
            assert!(
                !flags.same(&e.loaded_before),
                "{label}: native loaded fresh object"
            );
            assert!(
                p(&flags, "nested").same(&p(&st.initial_loaded, "nested")),
                "{label}: native retained loaded nested referent"
            );
            n(
                p(&p(&flags, "nested"), "keep"),
                5.0,
                &format!("{label}: native loaded nested value"),
            );
        }
    }
}
fn start_native(st: &State, result: &Result<(), Failure>, label: &str) {
    let name = st.name();
    let overlay = st.venue != "spot";
    let external = matches!(name, "external" | "external-disposed");
    let stop_overlay = overlay && matches!(name, "overlay-disposes" | "overlay-fail");
    let markets = usize::from(!external && !stop_overlay && name != "external-fail");
    let first_update = markets == 1
        && !matches!(
            name,
            "markets-fail" | "initial-disposed" | "markets-disposes" | "loaded-fail"
        );
    let ticker_call = first_update && name != "update-fail";
    let ticker_update = ticker_call
        && !matches!(
            name,
            "tickers-fail" | "tickers-disposes" | "log-fail" | "length-read-before-coercion"
        );
    let log = ticker_call
        && !matches!(
            name,
            "tickers-fail" | "tickers-disposes" | "length-read-before-coercion"
        );
    let final_call = external || ticker_update;
    let error = matches!(
        name,
        "external-fail"
            | "markets-fail"
            | "tickers-fail"
            | "update-fail"
            | "log-fail"
            | "start-private-fail"
            | "loaded-fail"
            | "length-read-before-coercion"
    ) || overlay && name == "overlay-fail";
    assert_eq!(result.is_err(), error, "{label}: native error disposition");
    if let Err(e) = result {
        if name == "length-read-before-coercion" {
            assert!(
                !Rc::ptr_eq(e, &st.error),
                "{label}: native generated error identity"
            );
            assert_eq!(e.name, "TypeError", "{label}: native generated name");
            assert_eq!(
                e.message, "Cannot read properties of null (reading 'length')",
                "{label}: native length read precedes conversion"
            );
            assert_eq!(
                observed_error_code(&e.code),
                None,
                "{label}: native generated code"
            );
            assert_eq!(e.response, None, "{label}: native generated response");
        } else {
            original(e, st, label)
        }
    }
    assert_eq!(
        calls(st, LifecycleAsync::Overlay),
        usize::from(overlay && st.f["overlay"] == true),
        "{label}: native overlay count"
    );
    assert_eq!(
        calls(st, LifecycleAsync::External),
        usize::from(!stop_overlay),
        "{label}: native external count"
    );
    assert_eq!(
        calls(st, LifecycleAsync::Markets),
        markets,
        "{label}: native markets fetch count"
    );
    assert_eq!(
        calls(st, LifecycleAsync::Tickers),
        usize::from(ticker_call),
        "{label}: native ticker fetch count"
    );
    assert_eq!(
        effects(st, LifecycleSync::Update),
        usize::from(first_update) + usize::from(ticker_update),
        "{label}: native independent update count"
    );
    assert_eq!(
        effects(st, LifecycleSync::Log),
        usize::from(log),
        "{label}: native log count"
    );
    assert_eq!(
        effects(st, LifecycleSync::PublicConnect),
        usize::from(ticker_update),
        "{label}: native public connect count"
    );
    let public = name == "public-only" || name == "public-route-after-await";
    assert_eq!(
        calls(st, LifecycleAsync::StartPublicOnly),
        usize::from(final_call && public),
        "{label}: native public startup count"
    );
    assert_eq!(
        calls(st, LifecycleAsync::StartPrivate),
        usize::from(final_call && !public),
        "{label}: native private startup count"
    );
    for u in updates(st) {
        if own(&u, "markets") {
            assert!(
                p(&u, "markets").same(&st.markets),
                "{label}: native original markets reference"
            );
            boolean(
                p(&p(&u, "loaded"), "markets"),
                true,
                &format!("{label}: native markets loaded"),
            );
        }
        if own(&u, "tickers") {
            assert!(
                p(&u, "tickers").same(&st.tickers),
                "{label}: native original tickers reference"
            );
            boolean(
                p(&p(&u, "loaded"), "tickers"),
                true,
                &format!("{label}: native tickers loaded"),
            );
        }
    }
    for e in st
        .effects
        .borrow()
        .iter()
        .filter(|e| e.k == LifecycleSync::Log)
    {
        assert_eq!(e.args.len(), 1, "{label}: native log arity");
        s(
            e.args[0].clone(),
            if name == "missing-length" {
                "Loaded NaN Binance markets"
            } else if name == "log-mutates-lengths" {
                "Loaded 3 Binance markets"
            } else {
                "Loaded 2 Binance markets"
            },
            &format!("{label}: native loaded-market log"),
        );
    }
    if ticker_update && name != "missing-length" {
        let tickers = arr(&st.tickers);
        assert_eq!(
            tickers.len(),
            if name == "log-mutates-lengths" { 3 } else { 2 },
            "{label}: native authored ticker count"
        );
        n(
            p(&tickers[0], "last"),
            12.3,
            &format!("{label}: native ticker last"),
        );
    }
    native_loaded(st, label);
}
fn private_native(st: &State, result: &Result<(), Failure>, label: &str) {
    let name = st.name();
    let mode = st.venue != "spot";
    let connected = name != "private-connect-fail";
    let got_mode = mode && connected && name != "setter-getter-fail";
    let setting = got_mode && name != "mode-fail";
    let tick = connected
        && (!mode
            || !matches!(
                name,
                "setter-getter-fail" | "mode-fail" | "setter-call-fail"
            ));
    let ready = tick
        && !matches!(
            name,
            "initial-disposed" | "connect-disposes" | "tick-disposes" | "tick-fail"
        );
    let fetched = ready;
    let loaded = ready
        && !matches!(
            name,
            "orders-disposes" | "orders-fail" | "log-ready-disposes"
        )
        && (!mode || name != "stale");
    let update = loaded && name != "loaded-fail";
    let error = matches!(
        name,
        "private-connect-fail" | "tick-fail" | "orders-fail" | "update-fail" | "loaded-fail"
    ) || mode
        && matches!(
            name,
            "mode-fail" | "setter-getter-fail" | "setter-call-fail"
        );
    assert_eq!(result.is_err(), error, "{label}: native error disposition");
    if let Err(e) = result {
        original(e, st, label)
    }
    assert_eq!(
        effects(st, LifecycleSync::PrivateConnect),
        1,
        "{label}: native independent private connect count"
    );
    assert_eq!(
        calls(st, LifecycleAsync::PositionMode),
        usize::from(got_mode),
        "{label}: native mode fetch count"
    );
    assert_eq!(
        effects(st, LifecycleSync::SetSetting),
        usize::from(setting),
        "{label}: native setting count"
    );
    assert_eq!(
        calls(st, LifecycleAsync::Tick),
        usize::from(tick),
        "{label}: native initial tick count"
    );
    assert_eq!(
        calls(st, LifecycleAsync::Orders),
        usize::from(fetched),
        "{label}: native orders fetch count"
    );
    assert_eq!(
        effects(st, LifecycleSync::Update),
        usize::from(update),
        "{label}: native independent update count"
    );
    assert_eq!(
        effects(st, LifecycleSync::Log),
        usize::from(ready) + usize::from(loaded),
        "{label}: native log count"
    );
    for e in st
        .effects
        .borrow()
        .iter()
        .filter(|e| e.k == LifecycleSync::SetSetting)
    {
        assert_eq!(
            e.receiver, 1,
            "{label}: native setter captured before await"
        );
        assert_eq!(e.args.len(), 2, "{label}: native setting argument count");
        s(
            e.args[0].clone(),
            "isHedged",
            &format!("{label}: native setting key"),
        );
        if name == "mode-missing" {
            assert!(
                matches!(e.args[1], EventValue::Undefined),
                "{label}: native undefined mode"
            )
        } else {
            boolean(
                e.args[1].clone(),
                true,
                &format!("{label}: native mode value"),
            );
        }
    }
    for u in updates(st) {
        assert!(
            p(&u, "orders").same(&st.orders),
            "{label}: native original orders reference"
        );
        assert_eq!(
            arr(&p(&u, "orders")).len(),
            1,
            "{label}: native order snapshot count"
        );
        n(
            p(&arr(&p(&u, "orders"))[0], "amount"),
            0.1,
            &format!("{label}: native order amount"),
        );
        boolean(
            p(&p(&u, "loaded"), "orders"),
            true,
            &format!("{label}: native orders loaded"),
        );
    }
    let logs = st
        .effects
        .borrow()
        .iter()
        .filter(|e| e.k == LifecycleSync::Log)
        .map(|e| e.args[0].clone())
        .collect::<Vec<_>>();
    if ready {
        s(
            logs[0].clone(),
            "Ready to trade on Binance",
            &format!("{label}: native ready log"),
        );
    }
    if loaded {
        s(
            logs[1].clone(),
            "Loaded Binance orders",
            &format!("{label}: native orders log"),
        );
    }
    native_loaded(st, label);
}
// Independently authored expected effects, not lengths derived from the produced
// outputs. (risk GET, account GET, updates, emissions, clocks, timers, riskAt, accountAt).
fn futures_expect(name: &str) -> (usize, usize, usize, usize, usize, usize, f64, f64) {
    match name {
        "initial-disposed" => (0, 0, 0, 0, 0, 0, 0.0, 0.0),
        "retry-disposes" => (0, 0, 0, 0, 1, 0, 0.0, 0.0),
        "retry-fail" => (0, 0, 0, 1, 1, 1, 0.0, 0.0),
        "positions-disposes" => (1, 0, 1, 0, 2, 0, 100001.0, 0.0),
        "account-disposes" => (1, 1, 1, 0, 2, 0, 100001.0, 0.0),
        "positions-fail" => (1, 0, 0, 1, 1, 1, 0.0, 0.0),
        "account-fail" => (1, 1, 1, 1, 2, 1, 100001.0, 0.0),
        "update-fail" => (1, 0, 1, 1, 1, 1, 0.0, 0.0),
        "loaded-fail" => (1, 1, 1, 1, 2, 1, 100001.0, 0.0),
        "emit-fail" => (1, 1, 1, 1, 2, 0, 100001.0, 0.0),
        "clock-fail-first" => (0, 0, 0, 0, 1, 0, 0.0, 0.0),
        "clock-fail-risk" => (1, 0, 1, 1, 2, 1, 0.0, 0.0),
        "clock-fail-account" => (1, 1, 2, 1, 3, 1, 100001.0, 0.0),
        "recent" => (0, 0, 0, 0, 1, 1, 70000.0, 90000.0),
        "disconnected" => (0, 1, 1, 0, 2, 1, 70000.0, 100001.0),
        "paused" => (0, 0, 0, 0, 1, 1, 0.0, 0.0),
        "paused-after-risk" => (1, 0, 1, 0, 2, 1, 100001.0, 0.0),
        "version-changes" | "version-nan" => (1, 1, 1, 0, 3, 1, 100001.0, 100002.0),
        "malformed-account" => (1, 1, 1, 1, 2, 1, 100001.0, 0.0),
        "risk-due" => (1, 0, 1, 0, 2, 1, 100001.0, 99999.0),
        "risk-before" => (0, 0, 0, 0, 1, 1, 25001.0, 99999.0),
        "account-due" => (0, 1, 1, 0, 2, 1, 99999.0, 100001.0),
        "account-before" => (0, 0, 0, 0, 1, 1, 99999.0, 75001.0),
        "degraded-due" => (0, 1, 1, 0, 2, 1, 99999.0, 100001.0),
        "degraded-before" => (0, 0, 0, 0, 1, 1, 99999.0, 97001.0),
        _ => (1, 1, 2, 0, 3, 1, 100001.0, 100002.0),
    }
}
fn tick_native(st: &State, result: &Result<(), Failure>, label: &str) {
    let name = st.name();
    let (risk, account, update, emissions, clocks, timers, risk_at, account_at) =
        if st.venue == "futures" {
            futures_expect(name)
        } else {
            let old_risk = st.f["risk"].as_f64().unwrap_or(0.0);
            let old_account = st.f["account"].as_f64().unwrap_or(0.0);
            let (r, a, u, e, t) = if st.venue == "pm" {
                match name {
                    "initial-disposed" => (0, 0, 0, 0, 0),
                    "retry-disposes" => (0, 0, 0, 0, 0),
                    "retry-fail" => (0, 0, 0, 1, 1),
                    "positions-disposes" => (1, 0, 1, 0, 0),
                    "account-disposes" => (1, 1, 1, 0, 0),
                    "positions-fail" => (1, 0, 0, 1, 1),
                    "account-fail" | "loaded-fail" | "malformed-account" => (1, 1, 1, 1, 1),
                    "update-fail" => (1, 0, 1, 1, 1),
                    "emit-fail" => (1, 1, 1, 1, 0),
                    "count-one" => (0, 1, 1, 0, 1),
                    _ => (1, 1, 2, 0, 1),
                }
            } else {
                match name {
                    "initial-disposed" => (0, 0, 0, 0, 0),
                    "account-disposes" => (0, 1, 0, 0, 0),
                    "account-fail" | "loaded-fail" | "malformed-account" => (0, 1, 0, 1, 1),
                    "update-fail" => (0, 1, 1, 1, 1),
                    "emit-fail" => (0, 1, 0, 1, 0),
                    "interval-fail" => (0, 1, 1, 0, 0),
                    _ => (0, 1, 1, 0, 1),
                }
            };
            (r, a, u, e, 0, t, old_risk, old_account)
        };
    assert_eq!(
        calls(st, LifecycleAsync::Positions),
        risk,
        "{label}: native risk request count"
    );
    assert_eq!(
        calls(
            st,
            if st.venue == "pm" {
                LifecycleAsync::Balance
            } else {
                LifecycleAsync::PositionsBalance
            }
        ),
        account,
        "{label}: native account request count"
    );
    assert_eq!(
        effects(st, LifecycleSync::Update),
        update,
        "{label}: native independent update count"
    );
    assert_eq!(
        effects(st, LifecycleSync::Emit),
        emissions,
        "{label}: native emission count"
    );
    assert_eq!(st.clock.get(), clocks, "{label}: native clock count");
    assert_eq!(
        st.timers.borrow().len(),
        timers,
        "{label}: native independent timer count"
    );
    n(
        number(st.times.risk.get()),
        risk_at,
        &format!("{label}: native risk timestamp"),
    );
    n(
        number(st.times.account.get()),
        account_at,
        &format!("{label}: native account timestamp"),
    );
    let error = name == "emit-fail"
        || st.venue == "futures" && name == "clock-fail-first"
        || st.venue == "spot" && name == "interval-fail";
    assert_eq!(result.is_err(), error, "{label}: native error disposition");
    if let Err(e) = result {
        original(e, st, label)
    }
    assert_eq!(
        calls(st, LifecycleAsync::RetryOrders),
        usize::from(
            st.venue != "spot" && matches!(name, "stale" | "retry-disposes" | "retry-fail")
        ),
        "{label}: native retry count"
    );
    for e in st
        .effects
        .borrow()
        .iter()
        .filter(|e| e.k == LifecycleSync::Emit)
    {
        assert_eq!(e.args.len(), 1, "{label}: native emission arity");
        s(
            e.args[0].clone(),
            if name == "malformed-account" {
                "Cannot destructure property 'balance' of '(intermediate value)' as it is null."
            } else {
                "lifecycle fixture"
            },
            &format!("{label}: native emitted message"),
        );
    }
    for u in updates(st) {
        if own(&u, "balance") {
            assert!(
                p(&u, "balance").same(&st.balance),
                "{label}: native original balance referent"
            );
            for (key, value) in [
                ("total", 20.5),
                ("free", 16.9),
                ("used", 3.6),
                ("upnl", -0.1),
            ] {
                n(
                    p(&p(&u, "balance"), key),
                    value,
                    &format!("{label}: native balance {key}"),
                );
            }
            let expect_positions = if st.venue == "futures" {
                !matches!(
                    name,
                    "version-changes" | "version-nan" | "capture-version-mutation"
                )
            } else {
                st.venue == "spot"
            };
            assert_eq!(
                own(&u, "positions"),
                expect_positions,
                "{label}: native account version-guarded positions policy"
            );
            if expect_positions {
                assert!(
                    p(&u, "positions").same(&st.positions),
                    "{label}: native account positions referent"
                );
                n(
                    p(&arr(&p(&u, "positions"))[0], "contracts"),
                    4.2,
                    &format!("{label}: native account contracts"),
                );
            }
            boolean(
                p(&p(&u, "loaded"), "balance"),
                true,
                &format!("{label}: native balance loaded"),
            );
            boolean(
                p(&p(&u, "loaded"), "positions"),
                true,
                &format!("{label}: native positions loaded"),
            );
        } else {
            assert!(
                p(&u, "positions").same(&st.risk),
                "{label}: native original risk positions referent"
            );
            let positions = arr(&p(&u, "positions"));
            assert_eq!(positions.len(), 1, "{label}: native risk position count");
            n(
                p(&positions[0], "contracts"),
                3.6,
                &format!("{label}: native risk contracts"),
            );
            n(
                p(&positions[0], "notional"),
                40.2,
                &format!("{label}: native risk notional"),
            );
            assert!(!own(&u, "loaded"), "{label}: native risk no loaded field");
        }
    }
    for (_, interval) in st.timers.borrow().iter() {
        n(
            interval.clone(),
            if st.venue == "futures" {
                3000.0
            } else if st.venue == "pm" {
                5000.0
            } else {
                7200.0
            },
            &format!("{label}: native timer interval"),
        );
    }
    native_loaded(st, label);
}
fn expected_timers(venue: &str, name: &str) -> usize {
    if venue == "futures" {
        futures_expect(name).5
    } else if venue == "pm" {
        usize::from(!matches!(
            name,
            "initial-disposed"
                | "retry-disposes"
                | "positions-disposes"
                | "account-disposes"
                | "emit-fail"
        ))
    } else {
        usize::from(!matches!(
            name,
            "initial-disposed" | "account-disposes" | "emit-fail" | "interval-fail"
        ))
    }
}
fn test(venue: &'static str, family: &'static str) {
    let mut projections = vec![];
    for (i, f) in fixtures(
        family,
        match family {
            "start" => 22,
            "private" => 18,
            _ => 36,
        },
    )
    .into_iter()
    .enumerate()
    {
        let mut b = setup(f, venue, family);
        let st = b.0.clone();
        let label = format!("native {venue} {family} case{i} {}", st.name());
        let result = execute(&mut b);
        match family {
            "start" => start_native(&st, &result, &label),
            "private" => private_native(&st, &result, &label),
            _ => tick_native(&st, &result, &label),
        };
        let mut timer_error = None;
        st.tick_version.set(2);
        let timers = st
            .timers
            .borrow()
            .iter()
            .map(|(timer, _)| timer.clone())
            .collect::<Vec<_>>();
        for timer in timers {
            match timer() {
                Err(e) => timer_error = Some(e),
                Ok(f) => {
                    if let Err(e) = run(f) {
                        timer_error = Some(e)
                    }
                }
            }
        }
        let authored_timers = if family == "tick" {
            expected_timers(venue, st.name())
        } else {
            0
        };
        assert_eq!(
            st.timers.borrow().len(),
            authored_timers,
            "{label}: native independent timer receipt count"
        );
        let expected_timer_error = family == "tick"
            && authored_timers == 1
            && matches!(st.name(), "timer-getter-fail" | "timer-call-fail");
        assert_eq!(
            timer_error.is_some(),
            expected_timer_error,
            "{label}: native timer error disposition"
        );
        if let Some(e) = &timer_error {
            original(e, &st, &label)
        }
        let expected_calls =
            authored_timers - usize::from(expected_timer_error && st.name() == "timer-getter-fail");
        assert_eq!(
            st.timer_calls.borrow().len(),
            expected_calls,
            "{label}: native timer call count"
        );
        for (version, args) in st.timer_calls.borrow().iter() {
            assert_eq!(*version, 2, "{label}: native timer current method receiver");
            assert_eq!(
                args.len(),
                usize::from(venue == "pm"),
                "{label}: native timer argument count"
            );
            if venue == "pm" {
                if matches!(st.name(), "count-string" | "count-array") {
                    s(
                        args[0].clone(),
                        "161",
                        &format!("{label}: native delayed string count"),
                    )
                } else {
                    n(
                        args[0].clone(),
                        if st.name() == "count-sixteen" {
                            17.0
                        } else if st.name() == "count-one" {
                            2.0
                        } else {
                            1.0
                        },
                        &format!("{label}: native next tick count"),
                    );
                }
            }
        }
        projections.push(project(&st, &result, timer_error.as_ref()));
    }
    compare(projections, venue, family)
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn futures_start() {
    test("futures", "start")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn pm_start() {
    test("pm", "start")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn spot_start() {
    test("spot", "start")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn futures_private_start() {
    test("futures", "private")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn pm_private_start() {
    test("pm", "private")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn spot_private_start() {
    test("spot", "private")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn futures_tick() {
    test("futures", "tick")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn pm_tick() {
    test("pm", "tick")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn spot_tick() {
    test("spot", "tick")
}

fn observed_error_code(
    code: &Option<safe_cex_exchange::js_runtime::value::Js>,
) -> Option<serde_json::Value> {
    code.as_ref().map(|value| {
        let finite =
            safe_cex_exchange::js_runtime::value::to_json(value).expect("finite JSON code fixture");
        let text = safe_cex_num::js_json_stringify(&finite.into())
            .expect("source JSON code fixture")
            .expect("defined code fixture");
        serde_json::from_str(&text).expect("JSON code observation")
    })
}
