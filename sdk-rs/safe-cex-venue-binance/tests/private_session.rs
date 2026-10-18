//! All native assertions for a family run before its actual TypeScript comparison.
#[path = "support/canonical_value.rs"]
mod canonical_value;
use canonical_value::category;
use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_types::{PositionMode, PositionSide};
use safe_cex_venue_binance::{private_event_value::*, private_session::*, request_weight::Failure};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn ev(v: Value) -> EventValue {
    EventValue::from_json(v)
}
fn arr(v: Vec<EventValue>) -> EventValue {
    EventValue::array(v)
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/private-session.json")).unwrap()
}
fn cases(key: &str, n: usize) -> Vec<String> {
    let f = fixture();
    let a = f[key].as_array().unwrap();
    assert_eq!(a.len(), n, "independent fixture case floor");
    a.iter().map(|x| x.as_str().unwrap().into()).collect()
}
fn wire(v: &EventValue) -> Value {
    match v {
        EventValue::Undefined => json!({"$undefined":true}),
        EventValue::Null => Value::Null,
        EventValue::Number(n) => {
            if n.is_nan() {
                json!({"$nan":true})
            } else if n.is_infinite() {
                json!({"$infinity":if n.is_sign_positive(){1}else{-1}})
            } else if *n == 0.0 && n.is_sign_negative() {
                json!({"$negativeZero":true})
            } else {
                json!(n)
            }
        }
        EventValue::Bool(x) => json!(x),
        EventValue::String(x) => json!(String::from_utf16(x).unwrap()),
        EventValue::Array(a) => array_values(a)
            .expect("fixture array reads")
            .iter()
            .map(wire)
            .collect(),
        EventValue::Object(_) => v
            .spread()
            .iter()
            .map(|(k, v)| (k.clone(), wire(v)))
            .collect(),
        _ => panic!("fixture is own data"),
    }
}
fn number_is(v: EventValue, n: f64, label: &str) {
    assert!(
        matches!(v,EventValue::Number(x) if x==n),
        "{label}: expected native f64 {n}, got {v:?}"
    )
}
fn error() -> Failure {
    let mut e = ExchangeError::new("session fixture");
    e.name = "RangeError".into();
    e.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        -1000
    )));
    e.response = Some(json!({"status":503,"data":{"msg":"session detail"}}));
    Rc::new(e)
}
fn assert_error(r: &Result<(), Failure>, wanted: bool, e: &Failure, label: &str) {
    assert_eq!(r.is_err(), wanted, "{label}: native outcome");
    if let Err(x) = r {
        assert!(Rc::ptr_eq(x, e), "{label}: original native error identity");
        assert_eq!(x.name, "RangeError");
        assert_eq!(x.message, "session fixture");
        assert_eq!(observed_error_code(&x.code), Some(json!(-1000)));
        assert_eq!(
            x.response,
            Some(json!({"status":503,"data":{"msg":"session detail"}})),
            "native full Error response metadata"
        );
    }
}
fn norm(v: &Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => a.iter().map(norm).collect(),
        Value::Object(o) => o.iter().map(|(k, v)| (k.clone(), norm(v))).collect(),
        x => x.clone(),
    }
}
fn compare(target: &str, values: Vec<Value>, n: usize) {
    assert_eq!(values.len(), n, "independent native floor");
    let out = Command::new("node")
        .arg(format!(
            "{}/scripts/private-session.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg(target)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "actual source oracle {target}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    use serde::Deserialize;
    let mut decoder = serde_json::Deserializer::from_slice(&out.stdout);
    decoder.disable_recursion_limit();
    let source = Value::deserialize(&mut decoder).unwrap();
    decoder.end().unwrap();
    assert_eq!(
        source.as_array().unwrap().len(),
        n,
        "independent actual source floor"
    );
    for (i, (a, b)) in values.iter().zip(source.as_array().unwrap()).enumerate() {
        assert_eq!(
            norm(a),
            norm(b),
            "actual source comparison {target} case {i}"
        )
    }
}
struct Ctor {
    venue: Venue,
    case: String,
    owner: EventValue,
    opts: EventValue,
    store: EventValue,
    clients: Vec<EventValue>,
    split: EventValue,
    trace: RefCell<Vec<String>>,
    rates: RefCell<Vec<(EventValue, f64, usize)>>,
    hook: RefCell<Option<RateHook>>,
    version: Cell<usize>,
    guards: RefCell<Vec<EventValue>>,
    err: Failure,
}
impl Ctor {
    fn new(case: &str, v: Venue) -> Rc<Self> {
        let clients: Vec<_> = [12.3, 3.6, 4.2, 9.1, 2.5, 6.7, 8.1, 8.2]
            .into_iter()
            .map(|m| ev(json!({"marker":m})))
            .collect();
        let parts = if case == "alias" {
            vec![clients[0].clone(); 3]
        } else {
            clients[..3].to_vec()
        };
        Rc::new(Self {
            venue: v,
            case: case.into(),
            owner: ev(json!({"base":7.2,"options":{"normalized":true}})),
            opts: ev(json!({"marker":12.3})),
            store: ev(json!({"marker":4.2})),
            split: if v == Venue::Spot {
                clients[0].clone()
            } else {
                arr(parts)
            },
            clients,
            trace: Default::default(),
            rates: Default::default(),
            hook: Default::default(),
            version: Cell::new(1),
            guards: Default::default(),
            err: error(),
        })
    }
    fn stage(&self, s: &str) -> Result<(), Failure> {
        self.trace.borrow_mut().push(s.into());
        if self.case == format!("{s}-error") {
            Err(self.err.clone())
        } else {
            Ok(())
        }
    }
}
impl ConstructorBindings for Ctor {
    fn super_fields(
        &self,
        name: EventValue,
        opts: EventValue,
        store: EventValue,
    ) -> Result<EventValue, Failure> {
        assert!(name.same(&string("account")));
        assert!(opts.same(&self.opts));
        assert!(store.same(&self.store));
        self.stage("super")?;
        Ok(self.owner.clone())
    }
    fn split(
        &self,
        v: Venue,
        opts: EventValue,
        owner: EventValue,
        f: Factory,
        rate: Option<RateHook>,
    ) -> Result<EventValue, Failure> {
        assert_eq!(v, self.venue, "native original constructor venue enum");
        assert!(opts.same(&self.opts) && owner.same(&self.owner));
        assert_eq!(
            f,
            if v == Venue::PM {
                Factory::PAPI
            } else {
                Factory::API
            },
            "native constructor factory role"
        );
        assert_eq!(rate.is_some(), v != Venue::PM);
        self.stage("split")?;
        *self.hook.borrow_mut() = rate.clone();
        if self.case != "delayed-rate" {
            if let Some(r) = rate {
                r(self.clients[1].clone())?;
            }
        }
        Ok(self.split.clone())
    }
    fn api(
        &self,
        v: Venue,
        k: Factory,
        opts: EventValue,
        owner: EventValue,
    ) -> Result<EventValue, Failure> {
        assert!(
            opts.same(&self.opts) && owner.same(&self.owner),
            "native constructor original options and self"
        );
        let (i, s) = match k {
            Factory::PublicAPI => (3, "public-api"),
            Factory::FAPI => (5, "fapi"),
            Factory::API | Factory::PAPI => (4, "unlimited"),
        };
        assert_eq!(v, self.venue, "native constructor API venue enum");
        let expected = match s {
            "public-api" => Factory::PublicAPI,
            "fapi" => Factory::FAPI,
            _ => {
                if self.venue == Venue::PM {
                    Factory::PAPI
                } else {
                    Factory::API
                }
            }
        };
        assert_eq!(k, expected, "native constructor exact API factory enum");
        self.stage(s)?;
        if self.case == "tuple-mutates" && k == Factory::PublicAPI {
            if let EventValue::Array(a) = &self.split {
                EventValue::Array(a.clone())
                    .set("1", self.clients[2].clone())
                    .unwrap();
            }
        }
        Ok(self.clients[i].clone())
    }
    fn rate_limit(&self, x: EventValue, max: f64) -> Result<EventValue, Failure> {
        self.rates
            .borrow_mut()
            .push((x.clone(), max, self.version.get()));
        self.stage("rate")?;
        Ok(x)
    }
    fn guards(&self, owner: EventValue, clients: EventValue) -> Result<(), Failure> {
        assert!(owner.same(&self.owner));
        if let EventValue::Array(a) = clients {
            *self.guards.borrow_mut() = array_values(&a).expect("fixture array reads")
        } else {
            panic!("native constructor guard array")
        };
        self.stage("guard")
    }
    fn child(&self, v: Venue, k: Child, owner: EventValue) -> Result<EventValue, Failure> {
        assert!(owner.same(&self.owner));
        assert_eq!(v, self.venue, "native constructor child venue enum");
        let (i, s) = if k == Child::Public {
            (6, "public")
        } else {
            (7, "private")
        };
        self.stage(s)?;
        Ok(self.clients[i].clone())
    }
}
fn ctor_case(v: Venue, name: &str) -> Value {
    let h = Ctor::new(name, v);
    let f = match v {
        Venue::Futures => construct_futures,
        Venue::PM => construct_pm,
        Venue::Spot => construct_spot,
    };
    let r = f(
        string("account"),
        h.opts.clone(),
        h.store.clone(),
        h.clone(),
    );
    let error_wanted = matches!(
        name,
        "super-error" | "split-error" | "public-error" | "private-error"
    ) || (name == "rate-error" && v != Venue::PM)
        || (name == "guard-error" && v == Venue::Futures);
    let outcome = r.as_ref().map(|_| ()).map_err(Clone::clone);
    assert_error(
        &outcome,
        error_wanted,
        &h.err,
        &format!("native constructor {v:?} {name}"),
    );
    if let Ok(owner) = r {
        assert!(owner.same(&h.owner), "native constructor original owner");
    }
    let stages: Vec<&str> = match v {
        Venue::Futures => vec![
            "super",
            "split",
            "rate",
            "public-api",
            "unlimited",
            "guard",
            "public",
            "private",
        ],
        Venue::PM => vec![
            "super",
            "split",
            "fapi",
            "unlimited",
            "public-api",
            "public",
            "private",
        ],
        Venue::Spot => vec!["super", "split", "rate", "unlimited", "public", "private"],
    };
    let mut expected = vec![];
    for s in stages {
        if s == "rate" && name == "delayed-rate" {
            continue;
        }
        expected.push(s.to_string());
        if name == format!("{s}-error") {
            break;
        }
    }
    assert_eq!(
        *h.trace.borrow(),
        expected,
        "native constructor sequence {v:?} {name}"
    );
    let fields = [
        (if v == Venue::PM { "papiXhr" } else { "xhr" }, 0, "split"),
        ("_primaryXhr", if name == "alias" { 0 } else { 1 }, "split"),
        (
            "_secondaryXhr",
            if name == "alias" { 0 } else { 2 },
            "split",
        ),
        ("publicXhr", 3, "public-api"),
        ("unlimitedXHR", 4, "unlimited"),
        ("fapiXhr", 5, "fapi"),
        ("publicWebsocket", 6, "public"),
        ("privateWebsocket", 7, "private"),
    ];
    for (key, i, stage) in fields {
        let valid = !(v == Venue::Spot
            && matches!(
                key,
                "_primaryXhr" | "_secondaryXhr" | "publicXhr" | "fapiXhr"
            ))
            && !(v == Venue::Futures && key == "fapiXhr");
        let reached = expected.iter().position(|s| s == stage).is_some_and(|p| {
            !(name == format!("{stage}-error")
                || (stage == "split" && name == "rate-error" && v != Venue::PM)
                || expected[p..].contains(&"rate".into())
                    && name == "rate-error"
                    && stage == "split")
        });
        let got = h.owner.get(key).unwrap();
        if valid && reached {
            assert!(
                got.same(&h.clients[i]),
                "native constructor {v:?} {name} {key}: original role identity"
            );
            number_is(
                got.read("marker").unwrap(),
                [12.3, 3.6, 4.2, 9.1, 2.5, 6.7, 8.1, 8.2][i],
                &format!("native constructor {v:?} {name} {key} role value"),
            )
        } else {
            assert!(
                matches!(got, EventValue::Undefined),
                "native constructor absent {key} {name}"
            );
        }
    }
    assert_eq!(
        h.guards.borrow().len(),
        if v == Venue::Futures && expected.iter().any(|s| s == "guard") {
            4
        } else {
            0
        },
        "native independent constructor guard count"
    );
    if !h.guards.borrow().is_empty() {
        assert_eq!(h.guards.borrow().len(), 4, "native constructor guard count");
        for (i, k) in [
            if name == "alias" { 0 } else { 1 },
            if name == "alias" { 0 } else { 2 },
            3,
            4,
        ]
        .into_iter()
        .enumerate()
        {
            assert!(
                h.guards.borrow()[i].same(&h.clients[k]),
                "native constructor captured guard role {i} {name}"
            );
            number_is(
                h.guards.borrow()[i].read("marker").unwrap(),
                [12.3, 3.6, 4.2, 9.1, 2.5, 6.7, 8.1, 8.2][k],
                "native constructor captured guard marker",
            );
        }
    }
    if name == "delayed-rate" {
        h.version.set(2);
        let hook = h.hook.borrow().clone();
        assert_eq!(
            hook.is_some(),
            v != Venue::PM,
            "native delayed rate hook required"
        );
        if let Some(rate) = hook {
            assert!(rate(h.clients[1].clone()).unwrap().same(&h.clients[1]));
        }
    }
    assert_eq!(
        h.rates.borrow().len(),
        usize::from(
            expected.iter().any(|s| s == "rate") || (name == "delayed-rate" && v != Venue::PM)
        ),
        "native independent rate callback count"
    );
    for (x, max, version) in h.rates.borrow().iter() {
        assert!(x.same(&h.clients[1]));
        assert_eq!(*max, 3.0, "native constructor rate maxRPS: 3 vs mutation 4");
        assert_eq!(
            *version,
            if name == "delayed-rate" { 2 } else { 1 },
            "native constructor current rate helper"
        );
    }
    json!({"trace":*h.trace.borrow(),"owner":wire(&h.owner),"error":error_wanted,"rates":h.rates.borrow().iter().map(|(_,m,v)|json!({"max":m,"version":v})).collect::<Vec<_>>(),"guards":h.guards.borrow().iter().map(wire).collect::<Vec<_>>()})
}
#[test]
fn constructors_futures() {
    compare(
        "constructors_futures",
        cases("constructors", 10)
            .iter()
            .map(|c| ctor_case(Venue::Futures, c))
            .collect(),
        10,
    )
}
#[test]
fn constructors_pm() {
    compare(
        "constructors_pm",
        cases("constructors", 10)
            .iter()
            .map(|c| ctor_case(Venue::PM, c))
            .collect(),
        10,
    )
}
#[test]
fn constructors_spot() {
    compare(
        "constructors_spot",
        cases("constructors", 10)
            .iter()
            .map(|c| ctor_case(Venue::Spot, c))
            .collect(),
        10,
    )
}
struct Conn {
    case: String,
    state: ConnectState,
    opts: EventValue,
    ready: Rc<Cell<bool>>,
    trace: RefCell<Vec<String>>,
    url: RefCell<Option<String>>,
    created: EventValue,
    alternate: EventValue,
    listeners: RefCell<Vec<(String, EventValue)>>,
    emits: RefCell<Vec<String>>,
    warnings: RefCell<Vec<String>>,
    timer: RefCell<Option<(RetryAction, f64)>>,
    version: Cell<usize>,
    retried: Cell<usize>,
    err: Failure,
}
impl Conn {
    fn new(c: &str) -> Rc<Self> {
        let h = Rc::new(Self {
            case: c.into(),
            state: Default::default(),
            opts: ev(json!({"key":"K","secret":"S"})),
            ready: Rc::new(Cell::new(false)),
            trace: Default::default(),
            url: Default::default(),
            created: ev(json!({"marker":12.3})),
            alternate: ev(json!({"marker":3.6})),
            listeners: Default::default(),
            emits: Default::default(),
            warnings: Default::default(),
            timer: Default::default(),
            version: Cell::new(1),
            retried: Cell::new(0),
            err: error(),
        });
        h.state.disposed.set(c == "disposed");
        h.state.connecting.set(c == "connecting");
        h.state.force_closed.set(c == "force-closed");
        h.state.retry_delay.set(if c == "retry-streak" {
            10000.0
        } else if c == "cap" {
            40000.0
        } else {
            5000.0
        });
        *h.state.retry_timer.borrow_mut() = number(42.0);
        match c {
            "payload-signer" => h.opts.assign("payloadSigner", ev(json!({}))),
            "testnet" => h.opts.assign("testnet", EventValue::Bool(true)),
            "missing-key" => h.opts.assign("key", string("")),
            "missing-secret" | "emit-error" => h.opts.assign("secret", EventValue::Undefined),
            "remote" => {
                h.opts.assign("secret", EventValue::Undefined);
                h.opts.assign("remoteSigner", ev(json!({})))
            }
            _ => {}
        }
        h
    }
    fn trace(&self, s: &str) {
        self.trace.borrow_mut().push(s.into());
    }
    fn key_fails(&self) -> bool {
        matches!(
            self.case.as_str(),
            "key-reject"
                | "key-sync"
                | "retry-streak"
                | "cap"
                | "emit-error"
                | "timer-error"
                | "retry-current"
                | "retry-disposed"
        )
    }
}
impl ConnectBindings for Conn {
    fn state(&self) -> &ConnectState {
        &self.state
    }
    fn options(&self) -> Result<EventValue, Failure> {
        Ok(self.opts.clone())
    }
    fn clear_timeout(&self, t: EventValue) -> Result<(), Failure> {
        number_is(t, 42.0, "native connect cleared original retry ticket");
        self.trace("clear");
        *self.state.retry_timer.borrow_mut() = EventValue::Undefined;
        Ok(())
    }
    fn fetch_native_key(&self) -> Result<NativeKeyFuture, Failure> {
        self.trace("key");
        assert!(
            self.state.connecting.get(),
            "native connecting during fetch"
        );
        if self.case == "key-sync" {
            return Err(self.err.clone());
        }
        let ready = self.ready.clone();
        let r = if self.key_fails() {
            Err(self.err.clone())
        } else {
            Ok(string("k é/?&"))
        };
        Ok(Box::pin(std::future::poll_fn(move |_| {
            if ready.get() {
                Poll::Ready(r.clone())
            } else {
                Poll::Pending
            }
        })))
    }
    fn emit(&self, m: String) -> Result<(), Failure> {
        self.trace("emit");
        self.emits.borrow_mut().push(m);
        if self.case == "emit-error" {
            Err(self.err.clone())
        } else {
            Ok(())
        }
    }
    fn log(&self, m: String, s: LogSeverity) -> Result<(), Failure> {
        assert_eq!(s, LogSeverity::Warning, "native connect warning enum");
        self.trace("log");
        self.warnings.borrow_mut().push(m);
        Ok(())
    }
    fn set_timeout(&self, a: RetryAction, d: f64) -> Result<EventValue, Failure> {
        self.trace("timer");
        if self.case == "timer-error" {
            return Err(self.err.clone());
        }
        *self.timer.borrow_mut() = Some((a, d));
        Ok(number(7.0))
    }
    fn start_connect(&self) -> Result<(), Failure> {
        self.trace("retry");
        self.retried.set(self.version.get());
        Ok(())
    }
    fn detach(&self) -> Result<(), Failure> {
        self.trace("detach");
        Ok(())
    }
    fn create(&self, u: String) -> Result<EventValue, Failure> {
        self.trace("create");
        *self.url.borrow_mut() = Some(u);
        Ok(self.created.clone())
    }
    fn add_listener(&self, w: EventValue, e: &'static str) -> Result<(), Failure> {
        self.trace(e);
        self.listeners.borrow_mut().push((e.into(), w));
        if self.case == "listener-replace" && e == "message" {
            *self.state.ws.borrow_mut() = self.alternate.clone();
        }
        Ok(())
    }
}
fn conn_case(v: Venue, name: &str) -> Value {
    let h = Conn::new(name);
    let mut future: Keyless = match v {
        Venue::Futures => Box::pin(connect_futures(h.clone())),
        Venue::PM => Box::pin(connect_pm(h.clone())),
        Venue::Spot => Box::pin(connect_spot(h.clone())),
    };
    let mut cx = Context::from_waker(Waker::noop());
    let first = future.as_mut().poll(&mut cx);
    let r = match first {
        Poll::Ready(r) => r,
        Poll::Pending => {
            assert!(
                h.state.connecting.get(),
                "native pending key connecting=true"
            );
            if name == "testnet-after" {
                h.opts.assign("testnet", EventValue::Bool(true));
            }
            if name == "disposed-after" {
                h.state.disposed.set(true);
            }
            h.ready.set(true);
            let Poll::Ready(r) = future.as_mut().poll(&mut cx) else {
                panic!("fixture ready")
            };
            r
        }
    };
    assert_error(
        &r,
        matches!(name, "emit-error" | "timer-error"),
        &h.err,
        &format!("native connect {v:?} {name}"),
    );
    assert_eq!(
        h.state.connecting.get(),
        name == "connecting",
        "native finally clears connecting"
    );
    let guarded =
        matches!(name, "payload-signer" | "disposed") || (name == "connecting" && v != Venue::Spot);
    let rejected = v != Venue::Spot && h.key_fails();
    let socket = !guarded
        && !rejected
        && !matches!(name, "force-closed" | "disposed-after")
        && !(v == Venue::Spot && matches!(name, "missing-key" | "missing-secret" | "emit-error"));
    assert_eq!(
        h.url.borrow().is_some(),
        socket,
        "native connect {v:?} {name}: socket decision"
    );
    assert_eq!(
        h.listeners.borrow().len(),
        if socket { 3 } else { 0 },
        "native independent connect listener count"
    );
    let timer_expected = rejected && !matches!(name, "emit-error" | "timer-error");
    assert_eq!(
        h.timer.borrow().is_some(),
        timer_expected,
        "native independent retry timer count"
    );
    assert_eq!(
        h.emits.borrow().len(),
        usize::from(
            (rejected && !matches!(name, "retry-streak" | "cap"))
                || (v == Venue::Spot
                    && matches!(name, "missing-key" | "missing-secret" | "emit-error"))
        ),
        "native independent error emission count"
    );
    assert_eq!(
        h.warnings.borrow().len(),
        usize::from(rejected && matches!(name, "retry-streak" | "cap")),
        "native independent Warning count"
    );
    let mut trace: Vec<String> = vec![];
    if !guarded {
        if v != Venue::Spot {
            trace.extend(["clear".into(), "key".into()]);
        }
        if rejected {
            trace.push(
                if matches!(name, "retry-streak" | "cap") {
                    "log"
                } else {
                    "emit"
                }
                .into(),
            );
            if name != "emit-error" {
                trace.push("timer".into());
            }
            if name == "retry-current" {
                trace.push("retry".into());
            }
        } else if socket {
            if v != Venue::Spot {
                trace.push("detach".into());
            }
            trace.extend(["create", "message", "close", "open"].map(String::from));
        } else if v == Venue::Spot {
            trace.push("emit".into());
        }
    }
    if socket {
        let expected = match (v, name == "testnet-after" || name == "testnet") {
            (Venue::Futures, false) => {
                "wss://fstream.binance.com/private/ws?listenKey=k%20%C3%A9%2F%3F%26"
            }
            (Venue::Futures, true) => {
                "wss://demo-fstream.binance.com/private/ws?listenKey=k%20%C3%A9%2F%3F%26"
            }
            (Venue::PM, false) => "wss://fstream.binance.com/pm/ws/k é/?&",
            (Venue::PM, true) => "wss://demo-fstream.binance.com/pm/ws/k é/?&",
            (Venue::Spot, false) => "wss://ws-api.binance.com:443/ws-api/v3",
            (Venue::Spot, true) => "wss://ws-api.testnet.binance.vision/ws-api/v3",
        };
        assert_eq!(
            h.url.borrow().as_deref(),
            Some(expected),
            "native connect literal URL {v:?} {name}"
        );
        assert_eq!(
            h.listeners.borrow().len(),
            3,
            "native connect independent listener count"
        );
        for (i, (e, w)) in h.listeners.borrow().iter().enumerate() {
            assert_eq!(e, ["message", "close", "open"][i]);
            assert!(
                w.same(if name == "listener-replace" && i > 0 {
                    &h.alternate
                } else {
                    &h.created
                }),
                "native connect current socket listener identity"
            );
        }
    }
    if rejected {
        let early = name == "emit-error";
        let d = if name == "retry-streak" {
            10000.0
        } else if name == "cap" {
            40000.0
        } else {
            5000.0
        };
        assert_eq!(
            h.state.resync_orders.get(),
            !early,
            "native resync on failed key"
        );
        assert_eq!(
            h.state.retry_delay.get(),
            if early {
                d
            } else if name == "cap" {
                60000.0
            } else {
                d * 2.0
            },
            "native retry backoff: 10000 vs mutation 15000"
        );
        if !early && name != "timer-error" {
            let (a, delay) = h.timer.borrow().clone().unwrap();
            assert_eq!(delay, d, "native retry schedules prior delay");
            number_is(
                h.state.retry_timer.borrow().clone(),
                7.0,
                "native retry assigned timer ticket",
            );
            if name == "retry-current" {
                h.version.set(2);
                a().unwrap();
                assert_eq!(h.retried.get(), 2, "native retry invokes current method");
            }
            if name == "retry-disposed" {
                h.state.disposed.set(true);
                a().unwrap();
                assert_eq!(h.retried.get(), 0, "native disposed suppresses retry");
            }
        }
        let msgs = if name == "retry-streak" || name == "cap" {
            h.warnings.borrow().clone()
        } else {
            h.emits.borrow().clone()
        };
        assert_eq!(
            msgs,
            vec!["Binance user data stream unavailable: session fixture"],
            "native key failure message"
        );
    }
    if v == Venue::Spot && matches!(name, "missing-key" | "missing-secret" | "emit-error") {
        assert_eq!(
            *h.emits.borrow(),
            vec!["Binance Spot private WebSocket requires an API key and signing secret"],
            "native Spot credentials message"
        );
    }
    if !guarded && !rejected && v != Venue::Spot {
        assert_eq!(
            h.state.retry_delay.get(),
            5000.0,
            "native successful key resets backoff"
        );
    }
    assert_eq!(*h.trace.borrow(), trace, "native connect ordered effects");
    if guarded || v == Venue::Spot {
        number_is(
            h.state.retry_timer.borrow().clone(),
            42.0,
            "native unchanged retry ticket",
        );
    } else if !timer_expected {
        assert!(
            matches!(*h.state.retry_timer.borrow(), EventValue::Undefined),
            "native clear ticket partial effect"
        );
    }
    json!({"trace":*h.trace.borrow(),"url":*h.url.borrow(),"listeners":h.listeners.borrow().iter().map(|(e,w)|json!({"event":e,"marker":wire(&w.read("marker").unwrap())})).collect::<Vec<_>>(),"emits":*h.emits.borrow(),"warnings":*h.warnings.borrow(),"connecting":h.state.connecting.get(),"resync":h.state.resync_orders.get(),"delay":h.state.retry_delay.get(),"ticket":wire(&h.state.retry_timer.borrow()),"timerDelay":h.timer.borrow().as_ref().map(|(_,d)|*d),"retried":h.retried.get(),"error":r.is_err()})
}
type Keyless = std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), Failure>>>>;
#[test]
fn connect_futures_cases() {
    compare(
        "legacy_futures",
        cases("legacy", 16)
            .iter()
            .map(|c| conn_case(Venue::Futures, c))
            .collect(),
        16,
    )
}
#[test]
fn connect_pm_cases() {
    compare(
        "legacy_pm",
        cases("legacy", 16)
            .iter()
            .map(|c| conn_case(Venue::PM, c))
            .collect(),
        16,
    )
}
#[test]
fn connect_spot_cases() {
    compare(
        "spotconnect_spot",
        cases("spotconnect", 8)
            .iter()
            .map(|c| conn_case(Venue::Spot, c))
            .collect(),
        8,
    )
}
fn message_data(c: &str) -> String {
    match c{"account"|"disposed"|"stats-error"|"mutate-account"=>r#"{"e":"outboundAccountPosition","price":12.3,"quantity":3.6,"zero":-0}"#.into(),"wrapped"=>r#"{"event":{"e":"executionReport","price":12.3,"quantity":3.6,"zero":-0},"subscriptionId":2}"#.into(),"auth"=>r#"{"id":"tealstreet-user-data","status":200,"result":{"subscriptionId":2}}"#.into(),"auth-inf"=>r#"{"id":"tealstreet-user-data","status":200,"result":{"subscriptionId":1e400}}"#.into(),"auth-string"|"emit-error"=>r#"{"id":"tealstreet-user-data","status":200,"result":{"subscriptionId":"2"}}"#.into(),"rejected"=>r#"{"id":"tealstreet-user-data","status":401,"error":{"msg":"fixture rejected"}}"#.into(),"missing-status"=>r#"{"id":"tealstreet-user-data"}"#.into(),"heartbeat"=>r#"{"id":42,"status":200,"result":{"userDataStream":true}}"#.into(),"inactive"=>r#"{"id":42,"status":200,"result":{"userDataStream":false}}"#.into(),"invalid"=>"bad".into(),"deep"=>format!("{{\"e\":\"executionReport\",\"price\":12.3,\"quantity\":3.6,\"zero\":-0,\"overflow\":1e400,\"deep\":{}0{}}}","[".repeat(150),"]".repeat(150)),_=>panic!("fixture")}
}
struct Msg {
    case: String,
    ops: RefCell<Vec<(MessageOp, Vec<EventValue>)>>,
    err: Failure,
    read: Cell<usize>,
}
impl SpotMessageBindings for Msg {
    fn disposed(&self) -> bool {
        self.case == "disposed"
    }
    fn call(&self, op: MessageOp, args: Vec<EventValue>) -> Result<(), Failure> {
        self.ops.borrow_mut().push((op, args.clone()));
        if (op == MessageOp::Stats && self.case == "stats-error")
            || (op == MessageOp::Emit && self.case == "emit-error")
        {
            return Err(self.err.clone());
        }
        if op == MessageOp::Account && self.case == "mutate-account" {
            let event = args[0].read("0")?;
            event.assign("e", string("executionReport"));
            event.assign("id", number(42.0));
            event.assign("status", number(200.0));
            event.assign("result", ev(json!({"userDataStream":true})));
        }
        Ok(())
    }
}
fn message_case(name: &str) -> Value {
    let h = Msg {
        case: name.into(),
        ops: Default::default(),
        err: error(),
        read: Cell::new(0),
    };
    let data = string(message_data(name));
    let r = on_spot_message(
        || {
            h.read.set(h.read.get() + 1);
            Ok(data.clone())
        },
        &h,
    );
    assert_eq!(
        h.read.get(),
        1,
        "native message parameter read before disposed guard"
    );
    assert_error(
        &r,
        matches!(name, "stats-error" | "emit-error"),
        &h.err,
        "native message error",
    );
    use MessageOp::*;
    let expected: Vec<_> = match name {
        "disposed" => vec![],
        "account" => vec![Track, Diagnostic, Stats, Account],
        "wrapped" | "deep" => vec![Track, Diagnostic, Stats, Order],
        "mutate-account" => vec![Track, Diagnostic, Stats, Account, Stats, Order, Pong],
        "auth" | "auth-inf" => vec![Track, Diagnostic, Ping],
        "auth-string" | "rejected" | "missing-status" => vec![Track, Diagnostic, Emit, ForceClose],
        "heartbeat" => vec![Track, Diagnostic, Pong],
        "inactive" => vec![Track, Diagnostic, Emit, Reconnect],
        "invalid" => vec![Track, Diagnostic],
        "stats-error" => vec![Track, Diagnostic, Stats],
        "emit-error" => vec![Track, Diagnostic, Emit],
        _ => panic!("fixture"),
    };
    assert_eq!(
        h.ops.borrow().iter().map(|(o, _)| *o).collect::<Vec<_>>(),
        expected,
        "native Spot message {name}: route sequence"
    );
    for (index, (op, args)) in h.ops.borrow().iter().enumerate() {
        assert_eq!(
            args.len(),
            usize::from(matches!(op, Diagnostic | Stats | Account | Order | Emit)),
            "native Spot operation independent argument count"
        );
        if *op == Stats {
            assert!(
                args[0].same(&string(
                    if index == 2 && !matches!(name, "wrapped" | "deep") {
                        "position"
                    } else {
                        "order"
                    }
                )),
                "native Spot statistics category"
            );
        }
        if *op == Diagnostic {
            assert!(args[0].same(&data), "native original raw diagnostic data");
        }
        if matches!(op, Account | Order) {
            let EventValue::Array(rows) = &args[0] else {
                panic!("native event array")
            };
            assert_eq!(rows.len(), 1, "native routed count from expectation");
            let row = EventValue::Array(rows.clone())
                .array_items()
                .unwrap()
                .unwrap()[0]
                .clone()
                .unwrap();
            number_is(
                row.read("price").unwrap(),
                12.3,
                "native Spot parsed price: 12.3 vs mutation 12",
            );
            number_is(
                row.read("quantity").unwrap(),
                3.6,
                "native Spot parsed quantity",
            );
            let EventValue::Number(z) = row.read("zero").unwrap() else {
                panic!("native signed zero")
            };
            assert!(z.is_sign_negative() && z == 0.0, "native JSON signed zero");
            if name == "deep" {
                let mut d = row.read("deep").unwrap();
                for _ in 0..150 {
                    let EventValue::Array(a) = d else {
                        panic!("native deep JSON array depth")
                    };
                    assert_eq!(a.len(), 1);
                    d = EventValue::Array(a.clone()).array_items().unwrap().unwrap()[0]
                        .clone()
                        .unwrap();
                }
                number_is(d, 0.0, "native deep JSON leaf");
                assert!(
                    matches!(row.read("overflow").unwrap(),EventValue::Number(x) if x.is_infinite() && x.is_sign_positive()),
                    "native parsed numeric overflow is positive Infinity"
                );
            }
        }
    }
    if name == "mutate-account" {
        let ops = h.ops.borrow();
        assert!(
            ops[3].1[0]
                .read("0")
                .unwrap()
                .same(&ops[5].1[0].read("0").unwrap()),
            "native account/order same parsed reference after mutation"
        );
    }
    if let Some((_, a)) = h.ops.borrow().iter().find(|(o, _)| *o == Emit) {
        let expected = match name {
            "rejected" => "Binance Spot user-data subscription failed: fixture rejected",
            "missing-status" => {
                "Binance Spot user-data subscription failed: subscription rejected with status unknown"
            }
            "inactive" => "Binance Spot user-data subscription is no longer active; reconnecting",
            _ => {
                "Binance Spot user-data subscription failed: subscription rejected with status 200"
            }
        };
        assert!(a[0].same(&string(expected)), "native Spot error message");
    }
    json!({"ops":h.ops.borrow().iter().map(|(op,a)|json!({"op":format!("{op:?}"),"args":a.iter().map(wire).collect::<Vec<_>>()})).collect::<Vec<_>>(),"error":r.is_err(),"reads":h.read.get()})
}
#[test]
fn spot_message_cases() {
    compare(
        "message_spot",
        cases("message", 15)
            .iter()
            .map(|c| message_case(c))
            .collect(),
        15,
    )
}
struct Mode {
    case: String,
    markets: EventValue,
    leverage: EventValue,
    positions: EventValue,
    ids: RefCell<Vec<(EventValue, EventValue, PositionMode)>>,
    settings: RefCell<Vec<bool>>,
    updates: RefCell<Vec<EventValue>>,
    err: Failure,
}
impl Mode {
    fn new(c: &str) -> Self {
        let positions = arr(vec![
            ("BTC", "0", PositionSide::Long, "oldzero"),
            ("BTC", "3.6", PositionSide::Short, "oldactive"),
            ("ETH", "", PositionSide::Short, "oldeth"),
            ("BTC", "bad", PositionSide::Long, "oldbad"),
        ]
        .into_iter()
        .map(|(s, q, side, id)| {
            let p = ev(json!({"symbol":s,"contracts":q,"id":id,"info":{"marker":4.2}}));
            p.assign(
                "side",
                EventValue::enum_value(PrivateEnum::PositionSide(side)),
            );
            p
        })
        .collect());
        Self {
            case: c.into(),
            markets: ev(json!([{"symbol":"BTC"},{"symbol":"ETH"}])),
            leverage: if c == "null-leverage" {
                EventValue::Null
            } else {
                ev(
                    json!({"BTC":{"buyLeverage":3.6,"sellLeverage":null,"nested":{"marker":12.3}},"ETH":{"buyLeverage":false,"sellLeverage":""}}),
                )
            },
            positions,
            ids: Default::default(),
            settings: Default::default(),
            updates: Default::default(),
            err: error(),
        }
    }
}
impl ModeBindings for Mode {
    fn markets(&self) -> Result<EventValue, Failure> {
        Ok(self.markets.clone())
    }
    fn leverage(&self) -> Result<EventValue, Failure> {
        Ok(self.leverage.clone())
    }
    fn positions(&self) -> Result<EventValue, Failure> {
        Ok(self.positions.clone())
    }
    fn id(&self, s: EventValue, side: EventValue, m: PositionMode) -> Result<EventValue, Failure> {
        self.ids.borrow_mut().push((s.clone(), side.clone(), m));
        if self.case == "id-error" {
            return Err(self.err.clone());
        }
        let side = category::<safe_cex_types::PositionSide>(&side);
        Ok(string(safe_cex_types::model_utils::create_position_id(
            &s.text()?,
            Some(side),
            Some(m),
        )))
    }
    fn setting(&self, h: bool) -> Result<(), Failure> {
        self.settings.borrow_mut().push(h);
        if self.case == "setting-error" {
            Err(self.err.clone())
        } else {
            Ok(())
        }
    }
    fn update(&self, v: EventValue) -> Result<(), Failure> {
        self.updates.borrow_mut().push(v);
        if self.case == "update-error" {
            Err(self.err.clone())
        } else {
            Ok(())
        }
    }
}
fn mode_case(name: &str) -> Value {
    let h = Mode::new(name);
    let hedged = name != "oneway";
    let symbol = match name {
        "symbol" => Some("ETH".into()),
        "empty-symbol" => Some("".into()),
        _ => None,
    };
    let r = apply_confirmed_mode(hedged, symbol, &h);
    assert_error(
        &r,
        matches!(name, "setting-error" | "update-error" | "id-error"),
        &h.err,
        "native mode error",
    );
    let mode = if hedged {
        PositionMode::Hedged
    } else {
        PositionMode::OneWay
    };
    let n = if matches!(name, "symbol" | "id-error") {
        1
    } else {
        2
    };
    assert_eq!(
        h.ids.borrow().len(),
        n,
        "native mode independent ID helper count"
    );
    for (s, side, m) in h.ids.borrow().iter() {
        assert_eq!(*m, mode, "native confirmed position mode enum");
        assert!(matches!(s, EventValue::String(_)));
        assert!(matches!(
            category::<safe_cex_types::PositionSide>(&(side)),
            _
        ));
    }
    assert_eq!(
        *h.settings.borrow(),
        if name == "id-error" {
            vec![]
        } else {
            vec![hedged]
        },
        "native confirmed setting sequencing"
    );
    assert_eq!(
        h.updates.borrow().len(),
        if matches!(name, "setting-error" | "id-error") {
            0
        } else {
            1
        },
        "native confirmed update count"
    );
    if let Some(update) = h.updates.borrow().first() {
        let rows = update.read("positions").unwrap();
        let EventValue::Array(a) = &rows else {
            panic!("native mode positions")
        };
        assert_eq!(
            a.len(),
            4,
            "native confirmed four positions from independent fixture"
        );
        for i in 0..4 {
            let row = rows.get(&i.to_string()).unwrap();
            let original = h.positions.get(&i.to_string()).unwrap();
            let changed = (i == 0 && name != "symbol") || i == 2;
            assert_eq!(
                row.same(&original),
                !changed,
                "native confirmed row {i} identity {name}"
            );
            assert!(
                row.read("info")
                    .unwrap()
                    .same(&original.read("info").unwrap()),
                "native shallow info reference"
            );
            if changed {
                assert!(
                    matches!(category::<safe_cex_types::PositionMode>(&(row.read("positionMode").unwrap())), m if m==mode),
                    "native confirmed row typed mode"
                );
                assert!(
                    row.read("id").unwrap().same(&string(match (i, hedged) {
                        (0, true) => "BTC:long",
                        (0, false) => "BTC",
                        (2, true) => "ETH:short",
                        (2, false) => "ETH",
                        _ => unreachable!(),
                    })),
                    "native confirmed zero row ID"
                );
            }
        }
        let leverage = update.read("fetchedLeverage").unwrap();
        for s in ["BTC", "ETH"] {
            let meta = leverage.get(s).unwrap();
            let updated = s == "ETH" || name != "symbol";
            if !updated {
                assert!(
                    meta.same(&h.leverage.get(s).unwrap()),
                    "native nonselected metadata identity"
                );
                continue;
            }
            assert!(
                matches!(category::<safe_cex_types::PositionMode>(&(meta.read("positionMode").unwrap())), m if m==mode),
                "native leverage typed mode"
            );
            if name == "null-leverage" {
                number_is(
                    meta.read("buyLeverage").unwrap(),
                    0.0,
                    "native null leverage default",
                );
                number_is(
                    meta.read("sellLeverage").unwrap(),
                    0.0,
                    "native null leverage default",
                );
            } else if s == "BTC" {
                number_is(
                    meta.read("buyLeverage").unwrap(),
                    3.6,
                    "native confirmed buy leverage 3.6",
                );
                number_is(
                    meta.read("sellLeverage").unwrap(),
                    0.0,
                    "native null sell default",
                );
                assert!(
                    meta.read("nested")
                        .unwrap()
                        .same(&h.leverage.get(s).unwrap().read("nested").unwrap()),
                    "native shallow metadata reference"
                );
            } else {
                assert!(
                    meta.read("buyLeverage")
                        .unwrap()
                        .same(&EventValue::Bool(false)),
                    "native false leverage preserved"
                );
                assert!(
                    meta.read("sellLeverage").unwrap().same(&string("")),
                    "native empty leverage preserved"
                );
            }
        }
    }
    json!({"ids":h.ids.borrow().iter().map(|(s,side,m)|json!([wire(s),wire(side),serde_json::to_value(m).unwrap()])).collect::<Vec<_>>(),"settings":*h.settings.borrow(),"updates":h.updates.borrow().iter().map(wire).collect::<Vec<_>>(),"original":wire(&h.positions),"error":r.is_err()})
}
#[test]
fn confirmed_mode_cases() {
    compare(
        "mode_futures",
        cases("mode", 8).iter().map(|c| mode_case(c)).collect(),
        8,
    )
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
