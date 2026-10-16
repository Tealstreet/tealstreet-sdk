use safe_cex_venue_polymarket::{
    account_reads::*,
    order_adapters::{Completion, NativeFill},
    state_sync::*,
    ws_dispatch::Runtime,
};
use serde_json::{Value as J, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
#[derive(Clone, Debug)]
enum V {
    U,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Object(Option<u32>, Rc<RefCell<Vec<(String, V)>>>),
    Array(Option<u32>, Rc<RefCell<Vec<Option<V>>>>),
}
#[derive(Debug)]
struct Failure {
    marker: Option<u32>,
    message: V,
}
type E = Rc<Failure>;
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text(s: &str) -> V {
    V::Text(units(s))
}
fn object(id: Option<u32>, fields: Vec<(String, V)>) -> V {
    V::Object(id, Rc::new(RefCell::new(fields)))
}
fn list(id: Option<u32>, slots: Vec<Option<V>>) -> V {
    V::Array(id, Rc::new(RefCell::new(slots)))
}
fn decode(j: &J) -> V {
    match j["kind"].as_str().unwrap() {
        "undefined" => V::U,
        "null" => V::Null,
        "bool" => V::Bool(j["value"].as_bool().unwrap()),
        "number" => V::Number(f64::from_bits(
            u64::from_str_radix(j["bits"].as_str().unwrap(), 16).unwrap(),
        )),
        "text" => V::Text(
            j["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| u16::try_from(x.as_u64().unwrap()).unwrap())
                .collect(),
        ),
        "object" => object(
            j["id"].as_u64().map(|v| v as u32),
            j["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| (x[0].as_str().unwrap().into(), decode(&x[1])))
                .collect(),
        ),
        "array" => list(
            j["id"].as_u64().map(|v| v as u32),
            j["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| {
                    if x["kind"] == "hole" {
                        None
                    } else {
                        Some(decode(x))
                    }
                })
                .collect(),
        ),
        other => panic!("native fixture {other}"),
    }
}
fn raw_field(v: &V, key: &str) -> V {
    if let V::Object(_, f) = v {
        f.borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map_or(V::U, |(_, v)| v.clone())
    } else {
        V::U
    }
}
fn field_set(v: &V, key: &str, value: V) {
    let V::Object(_, f) = v else {
        panic!("native object")
    };
    let mut f = f.borrow_mut();
    if let Some((_, v)) = f.iter_mut().find(|(k, _)| k == key) {
        *v = value
    } else {
        f.push((key.into(), value));
    }
}
fn numeric(v: &V) -> f64 {
    match v {
        V::Number(n) => *n,
        V::Null => 0.,
        V::U => f64::NAN,
        V::Bool(b) => u8::from(*b) as f64,
        V::Text(s) => String::from_utf16_lossy(s).parse().unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}
fn project(v: &V) -> J {
    match v {
        V::U => json!({"kind":"undefined"}),
        V::Null => json!({"kind":"null"}),
        V::Bool(b) => json!({"kind":"bool","value":b}),
        V::Number(n) => json!({"kind":"number","bits":format!("{:016x}",n.to_bits())}),
        V::Text(t) => json!({"kind":"text","units":t}),
        V::Object(id, f) => {
            json!({"kind":"object","id":id,"fields":f.borrow().iter().map(|(k,v)|json!([k,project(v)])).collect::<Vec<_>>()})
        }
        V::Array(id, slots) => {
            json!({"kind":"array","id":id,"items":slots.borrow().iter().map(|v|v.as_ref().map_or(json!({"kind":"hole"}),project)).collect::<Vec<_>>()})
        }
    }
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Object(_, a), V::Object(_, b)) => Rc::ptr_eq(a, b),
        (V::Array(_, a), V::Array(_, b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
struct Op {
    ready: Cell<bool>,
    result: Result<V, E>,
    index: usize,
}
impl Completion<V, E> for Op {
    fn poll(&self, _: &mut Context<'_>) -> Poll<Result<V, E>> {
        if self.ready.get() {
            Poll::Ready(self.result.clone())
        } else {
            Poll::Pending
        }
    }
}
struct Host {
    name: String,
    i: usize,
    fixture: J,
    values: V,
    credentials: RefCell<V>,
    loaded: RefCell<V>,
    balance: V,
    positions: V,
    holdings: V,
    async_roles: RefCell<Vec<(&'static str, &'static str)>>,
    native_calls: RefCell<Vec<(&'static str, &'static str, Vec<V>)>>,
    events: RefCell<Vec<J>>,
    updates: RefCell<Vec<V>>,
    fills: RefCell<Vec<NativeFill<V>>>,
    cost: RefCell<Vec<(V, V)>>,
    scheduled: RefCell<Vec<Rc<dyn Fn() -> Result<V, E>>>>,
    handlers: RefCell<Vec<(Rc<Op>, Rc<dyn Fn(E) -> Result<(), E>>)>>,
    detached: RefCell<Vec<E>>,
    tasks: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
    ops: RefCell<Vec<Rc<Op>>>,
    calls: Cell<usize>,
    disposed: Cell<bool>,
    ready: Cell<bool>,
    error: E,
    logs: RefCell<Vec<(Vec<u16>, &'static str)>>,
    late: Cell<bool>,
}
impl Host {
    fn new(case: &J) -> Self {
        let f = case["fixture"].clone();
        let values = object(
            None,
            f.as_object()
                .unwrap()
                .iter()
                .filter(|(k, _)| {
                    ![
                        "external",
                        "publicOnly",
                        "initialDisposed",
                        "disposeAfter",
                        "failAt",
                        "logThrows",
                    ]
                    .contains(&k.as_str())
                })
                .map(|(k, v)| (k.clone(), decode(v)))
                .collect(),
        );
        Self {
            name: case["name"].as_str().unwrap().into(),
            i: case["i"].as_u64().unwrap() as usize,
            credentials: RefCell::new(raw_field(&values, "credentials")),
            loaded: RefCell::new(raw_field(&values, "loaded")),
            balance: raw_field(&values, "balance"),
            positions: raw_field(&values, "positions"),
            holdings: raw_field(&values, "holdings"),
            values,
            fixture: f.clone(),
            async_roles: RefCell::new(vec![]),
            native_calls: RefCell::new(vec![]),
            events: RefCell::new(vec![]),
            updates: RefCell::new(vec![]),
            fills: RefCell::new(vec![]),
            cost: RefCell::new(vec![]),
            scheduled: RefCell::new(vec![]),
            handlers: RefCell::new(vec![]),
            detached: RefCell::new(vec![]),
            tasks: RefCell::new(vec![]),
            ops: RefCell::new(vec![]),
            calls: Cell::new(0),
            disposed: Cell::new(f["initialDisposed"].as_bool().unwrap()),
            ready: Cell::new(false),
            error: Rc::new(Failure {
                marker: Some(47),
                message: text("boundary failure"),
            }),
            logs: RefCell::new(vec![]),
            late: Cell::new(false),
        }
    }
    fn event(&self, tag: &str, args: &[V]) {
        let mut event = vec![json!(tag)];
        event.extend(args.iter().map(project));
        self.events.borrow_mut().push(json!(event));
    }
    fn value(&self, key: &str) -> V {
        raw_field(&self.values, key)
    }
    fn drive(&self) {
        let tasks = std::mem::take(&mut *self.tasks.borrow_mut());
        let mut cx = Context::from_waker(Waker::noop());
        for mut task in tasks {
            if task.as_mut().poll(&mut cx).is_pending() {
                self.tasks.borrow_mut().push(task);
            }
        }
        let handlers = std::mem::take(&mut *self.handlers.borrow_mut());
        for (op, cb) in handlers {
            if !op.ready.get() {
                self.handlers.borrow_mut().push((op, cb));
            } else if let Err(e) = &op.result {
                if let Err(e) = cb(e.clone()) {
                    self.detached.borrow_mut().push(e);
                }
            }
        }
    }
    fn finish_all(&self) {
        for _ in 0..80 {
            if !self.ops.borrow().is_empty() {
                let op = self.ops.borrow_mut().remove(0);
                if self.fixture["disposeAfter"].as_u64() == Some(op.index as u64) {
                    self.disposed.set(true);
                }
                op.ready.set(true);
            }
            self.drive();
            if self.tasks.borrow().is_empty() && self.handlers.borrow().is_empty() {
                return;
            }
        }
        panic!("native finite continuation floor")
    }
}
impl Runtime for Host {
    type Value = V;
    type Error = E;
    fn undefined(&self) -> V {
        V::U
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(v, V::U | V::Null)
    }
    fn truthy(&self, v: &V) -> bool {
        match v {
            V::U | V::Null => false,
            V::Bool(b) => *b,
            V::Number(n) => *n != 0. && !n.is_nan(),
            V::Text(s) => !s.is_empty(),
            _ => true,
        }
    }
    fn is_text(&self, v: &V, s: &str) -> bool {
        matches!(v,V::Text(t)if *t==units(s))
    }
    fn field(&self, v: &V, key: &str) -> Result<V, E> {
        if self.nullish(v) {
            Err(self.new_error("nullish field"))
        } else {
            Ok(raw_field(v, key))
        }
    }
    fn string(&self, v: &V) -> Result<Vec<u16>, E> {
        Ok(match v {
            V::Text(t) => t.clone(),
            V::U => units("undefined"),
            V::Null => units("null"),
            V::Number(n) => units(&safe_cex_num::js_number_string(*n)),
            V::Bool(b) => units(if *b { "true" } else { "false" }),
            _ => units("[object Object]"),
        })
    }
    fn log(&self, message: &[u16], severity: &'static str) -> Result<(), E> {
        self.logs.borrow_mut().push((message.to_vec(), severity));
        self.events
            .borrow_mut()
            .push(json!(["log", message, severity]));
        if self.fixture["logThrows"] == true {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
}
impl AccountHost for Host {
    type Callable = (&'static str, &'static str);
    type Operation = Op;
    fn start_task(&self, mut task: Pin<Box<dyn Future<Output = ()>>>) {
        let mut cx = Context::from_waker(Waker::noop());
        if task.as_mut().poll(&mut cx).is_pending() {
            self.tasks.borrow_mut().push(task);
        }
    }
    fn capture(&self, r: &'static str, m: &'static str) -> Result<Self::Callable, E> {
        Ok((r, m))
    }
    fn invoke(&self, (receiver, method): Self::Callable, args: Vec<V>) -> Result<Rc<Op>, E> {
        self.native_calls
            .borrow_mut()
            .push((receiver, method, args.clone()));
        self.async_roles.borrow_mut().push((receiver, method));
        let n = self.calls.get() + 1;
        self.calls.set(n);
        self.event(&format!("{receiver}.{method}"), &args);
        let key = match method {
            "getServerTime" => "server",
            "signApiKeyDerivationMessage" => "signed",
            "deriveApiKey" => "derived",
            "getBalanceAllowance" => "balanceData",
            "getPositions" => "holdings",
            "initExternalPublicData" => "external",
            "fetchMarkets" => "markets",
            "fetchTickers" => "tickers",
            "fetchPositions" | "fetchAndSetPositions" => "positions",
            "fetchBalance" => "balance",
            "fetchOrders" => "orders",
            "tick" => "undefined",
            _ => panic!("native child {method}"),
        };
        let value = if key == "external" {
            V::Bool(self.fixture["external"].as_bool().unwrap())
        } else if key == "undefined" {
            V::U
        } else {
            self.value(key)
        };
        let result = if self.fixture["failAt"].as_u64() == Some(n as u64) {
            Err(self.error.clone())
        } else {
            Ok(value)
        };
        let op = Rc::new(Op {
            ready: Cell::new(self.ready.get()),
            result,
            index: n,
        });
        if op.ready.get() {
            if self.fixture["disposeAfter"].as_u64() == Some(n as u64) {
                self.disposed.set(true)
            }
        } else {
            self.ops.borrow_mut().push(op.clone());
        }
        Ok(op)
    }
    fn invoke_log(
        &self,
        _: Self::Callable,
        message: &[u16],
        severity: &'static str,
    ) -> Result<(), E> {
        self.log(message, severity)
    }
    fn resolve(&self, v: V) -> Result<Rc<Op>, E> {
        Ok(Rc::new(Op {
            ready: Cell::new(true),
            result: Ok(v),
            index: 0,
        }))
    }
    fn credentials(&self) -> Result<V, E> {
        Ok(self.credentials.borrow().clone())
    }
    fn wallet(&self) -> Result<V, E> {
        Ok(self.value("wallet"))
    }
    fn exchange_credentials(&self) -> Result<V, E> {
        Ok(self.credentials.borrow().clone())
    }
    fn signer_type(&self) -> Result<V, E> {
        Ok(self.value("signer"))
    }
    fn text(&self, t: &'static str) -> V {
        text(t)
    }
    fn number(&self, n: f64) -> V {
        V::Number(n)
    }
    fn null(&self) -> V {
        V::Null
    }
    fn object(&self, f: Vec<(&'static str, V)>) -> Result<V, E> {
        Ok(object(
            None,
            f.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        ))
    }
    fn assign(&self, o: &V, k: &'static str, v: V) -> Result<(), E> {
        field_set(o, k, v);
        Ok(())
    }
    fn spread_params(&self, _: &V, _: V, _: V) -> Result<V, E> {
        unreachable!("not used")
    }
    fn array(&self, v: Vec<Option<V>>) -> V {
        list(None, v)
    }
    fn map_output(&self, _: &V, _: usize) -> Result<V, E> {
        unreachable!("not used")
    }
    fn map_set(&self, _: &V, _: usize, _: V) -> Result<(), E> {
        unreachable!("not used")
    }
    fn is_array(&self, v: &V) -> bool {
        matches!(v, V::Array(..))
    }
    fn length(&self, v: &V) -> Result<usize, E> {
        if let V::Array(_, s) = v {
            Ok(s.borrow().len())
        } else {
            Err(self.new_error("invalid length"))
        }
    }
    fn at(&self, v: &V, i: usize) -> Result<Option<V>, E> {
        if let V::Array(_, s) = v {
            Ok(s.borrow()[i].clone())
        } else {
            Err(self.new_error("invalid slot"))
        }
    }
    fn push_spread(&self, out: &V, input: &V) -> Result<(), E> {
        let (V::Array(_, o), V::Array(_, a)) = (out, input) else {
            panic!("native push")
        };
        o.borrow_mut()
            .extend(a.borrow().iter().map(|v| Some(v.clone().unwrap_or(V::U))));
        Ok(())
    }
    fn less(&self, a: &V, b: &V) -> Result<bool, E> {
        Ok(numeric(a) < numeric(b))
    }
    fn greater_equal(&self, a: &V, b: &V) -> Result<bool, E> {
        Ok(numeric(a) >= numeric(b))
    }
    fn add(&self, a: &V, b: &V) -> Result<V, E> {
        Ok(V::Number(numeric(a) + numeric(b)))
    }
    fn slice(&self, _: &V, _: &V) -> Result<V, E> {
        unreachable!("not used")
    }
    fn order_to_json(&self, _: V, _: V, _: V) -> Result<V, E> {
        unreachable!("not used")
    }
    fn new_error(&self, m: &'static str) -> E {
        Rc::new(Failure {
            marker: None,
            message: text(m),
        })
    }
    fn error_message(&self, e: &E) -> Result<V, E> {
        Ok(e.message.clone())
    }
}
impl StateHost for Host {
    type Iterator = (V, usize);
    fn disposed(&self) -> bool {
        self.disposed.get()
    }
    fn public_only(&self) -> bool {
        self.fixture["publicOnly"] == true
    }
    fn assign_credentials(&self, v: V) -> Result<(), E> {
        *self.credentials.borrow_mut() = v;
        Ok(())
    }
    fn funder(&self) -> Result<V, E> {
        Ok(self.value("funder"))
    }
    fn ticker(&self, symbol: &V) -> Result<V, E> {
        let last = if self.is_text(symbol, "S-a") && self.i == 4 {
            0.625
        } else if self.is_text(symbol, "S-b") && self.i == 6 {
            0.875
        } else {
            0.
        };
        Ok(object(None, vec![("last".into(), V::Number(last))]))
    }
    fn loaded(&self) -> Result<V, E> {
        Ok(self.loaded.borrow().clone())
    }
    fn spread_loaded(&self, v: V, flags: &[&'static str]) -> Result<V, E> {
        let V::Object(_, f) = v else { panic!("loaded") };
        let out = object(None, f.borrow().clone());
        for key in flags {
            field_set(&out, key, V::Bool(true));
        }
        Ok(out)
    }
    fn call_sync(&self, (receiver, method): Self::Callable, args: Vec<V>) -> Result<V, E> {
        self.native_calls
            .borrow_mut()
            .push((receiver, method, args.clone()));
        match (receiver, method) {
            ("tokenToSymbol", "get") => {
                self.event("token", &args);
                Ok(if self.is_text(&args[0], "a") {
                    text("S-a")
                } else if self.is_text(&args[0], "b") {
                    text("S-b")
                } else {
                    V::U
                })
            }
            ("costBasis", "set") => {
                self.cost
                    .borrow_mut()
                    .push((args[0].clone(), args[1].clone()));
                self.event("cost", &args);
                Ok(V::U)
            }
            ("imports", "createPositionId") => Ok(args[0].clone()),
            ("store", "update") => {
                let patch = args[0].clone();
                self.updates.borrow_mut().push(patch.clone());
                self.event("update", &args);
                *self.loaded.borrow_mut() = raw_field(&patch, "loaded");
                if self.i == 9 {
                    Err(self.error.clone())
                } else {
                    Ok(V::U)
                }
            }
            ("privateWebsocket", "connectAndSubscribe") => {
                self.event("connect", &[]);
                if self.i == 9 {
                    Err(self.error.clone())
                } else {
                    Ok(V::U)
                }
            }
            ("private", "refreshPositions") => {
                self.event("refresh", &args);
                if self.i == 7 {
                    Err(self.error.clone())
                } else {
                    Ok(V::U)
                }
            }
            ("exchange", "tick") => {
                assert!(self.late.get(), "native scheduled callback current method");
                self.event("lateTick", &[]);
                Ok(text("late"))
            }
            _ => panic!("native sync {receiver}.{method}"),
        }
    }
    fn finite(&self, v: &V) -> bool {
        matches!(v,V::Number(n)if n.is_finite())
    }
    fn less_equal(&self, a: &V, b: &V) -> Result<bool, E> {
        Ok(numeric(a) <= numeric(b))
    }
    fn multiply(&self, a: &V, b: &V) -> Result<V, E> {
        Ok(V::Number(numeric(a) * numeric(b)))
    }
    fn subtract(&self, a: &V, b: &V) -> Result<V, E> {
        Ok(V::Number(numeric(a) - numeric(b)))
    }
    fn iterate(&self, v: V) -> Result<Self::Iterator, E> {
        assert!(
            same(&v, &self.holdings),
            "native original holdings iterator"
        );
        Ok((v, 0))
    }
    fn next(&self, it: &mut Self::Iterator) -> Result<Option<V>, E> {
        let V::Array(_, a) = &it.0 else {
            return Err(self.new_error("invalid iterable"));
        };
        let a = a.borrow();
        if it.1 >= a.len() {
            return Ok(None);
        }
        let v = a[it.1].clone().unwrap_or(V::U);
        it.1 += 1;
        Ok(Some(v))
    }
    fn sum_by(&self, v: &V, key: &'static str) -> Result<V, E> {
        let V::Array(_, a) = v else {
            panic!("positions")
        };
        Ok(V::Number(
            a.borrow()
                .iter()
                .flatten()
                .map(|v| numeric(&raw_field(v, key)))
                .sum(),
        ))
    }
    fn error_value(&self, e: &E) -> V {
        let V::Text(t) = &e.message else {
            panic!("error message")
        };
        let mut value = units("Error: ");
        value.extend(t);
        V::Text(value)
    }
    fn optional_token_get(&self) -> Result<Option<Self::Callable>, E> {
        Ok(if self.i == 4 {
            None
        } else {
            Some(("tokenToSymbol", "get"))
        })
    }
    fn side_lookup(&self, key: V) -> Result<V, E> {
        Ok(if self.is_text(&key, "BUY") {
            text("buy")
        } else if self.is_text(&key, "SELL") {
            text("sell")
        } else {
            V::U
        })
    }
    fn emit_fill(&self, call: Self::Callable, fill: NativeFill<V>) -> Result<(), E> {
        assert_eq!(call, ("emitter", "emit"));
        let value = object(
            None,
            vec![
                ("id".into(), fill.id.clone()),
                ("side".into(), fill.side.clone()),
                ("symbol".into(), fill.symbol.clone()),
                ("price".into(), V::Number(fill.price)),
                ("amount".into(), V::Number(fill.amount)),
            ],
        );
        self.fills.borrow_mut().push(fill);
        self.event("fill", &[value]);
        if self.i == 6 {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn schedule_tick(&self, cb: Rc<dyn Fn() -> Result<V, E>>, ms: f64) -> Result<(), E> {
        assert_eq!(ms, 10000., "native tick interval");
        self.scheduled.borrow_mut().push(cb);
        self.event("loop", &[V::Number(ms)]);
        Ok(())
    }
    fn catch_rejection(&self, op: Rc<Op>, cb: Rc<dyn Fn(E) -> Result<(), E>>) -> Result<(), E> {
        self.handlers.borrow_mut().push((op, cb));
        Ok(())
    }
}
fn oracle() -> &'static J {
    static DATA: OnceLock<J> = OnceLock::new();
    DATA.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/state_sync_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "actual eight state methods: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: J = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            v["cases"].as_array().unwrap().len(),
            80,
            "independent source floor"
        );
        v
    })
}
fn method(h: Rc<Host>) -> Option<Pin<Box<dyn Future<Output = Result<V, E>>>>> {
    Some(match h.name.as_str() {
        "deriveApiCredentials" => Box::pin(derive_api_credentials(h)),
        "start" => Box::pin(start_exchange(h)),
        "tick" => Box::pin(tick(h)),
        "fetchBalance" => Box::pin(fetch_balance(h)),
        "fetchPositions" => Box::pin(fetch_positions(h)),
        "fetchAndSetPositions" => Box::pin(fetch_and_set_positions(h)),
        _ => return None,
    })
}
fn n(v: &V, expected: f64, label: &str) {
    let V::Number(v) = v else {
        panic!("native Number {label}")
    };
    if expected.is_nan() {
        assert!(v.is_nan(), "native NaN {label}")
    } else {
        assert_eq!(v.to_bits(), expected.to_bits(), "native bits {label}")
    }
}
fn t(v: &V, s: &str, label: &str) {
    let V::Text(v) = v else {
        panic!("native text {label}")
    };
    assert_eq!(*v, units(s), "native text {label}")
}
fn keys(v: &V, expected: &[&str], label: &str) {
    let V::Object(_, fields) = v else {
        panic!("native record {label}")
    };
    let f = fields.borrow();
    assert_eq!(f.len(), expected.len(), "native key count {label}");
    for (i, k) in expected.iter().enumerate() {
        assert_eq!(f[i].0, *k, "native key {label}/{i}");
    }
}

fn native(h: &Host, result: &Result<V, E>) {
    let name = h.name.as_str();
    let i = h.i;
    let error = match name {
        "deriveApiCredentials" => matches!(i, 2 | 7 | 9),
        "start" => matches!(i, 7 | 9),
        "tick" => i == 9,
        "fetchBalance" | "fetchPositions" => i == 7,
        "fetchAndSetPositions" => matches!(i, 7 | 9),
        "handleTradeUpdate" => i == 9,
        _ => false,
    };
    assert_eq!(result.is_err(), error, "native outcome {name}/{i}");
    if let Err(e) = result {
        if name == "deriveApiCredentials" && i == 2 {
            assert_eq!(e.marker, None);
            t(&e.message, "No wallet configured", "wallet refusal")
        } else {
            assert!(Rc::ptr_eq(e, &h.error), "native original error {name}/{i}")
        }
    }
    if name == "deriveApiCredentials" {
        let changed = !matches!(i, 2 | 7);
        assert_eq!(
            same(&h.credentials.borrow(), &h.value("derived")),
            changed,
            "native derived credentials original alias"
        );
        if !changed {
            assert!(
                same(&h.credentials.borrow(), &h.value("credentials"))
                    || matches!(*h.credentials.borrow(), V::Null),
                "native credentials retained"
            );
        }
    }
    if let Ok(out) = result {
        match name {
            "fetchBalance" => {
                keys(out, &["total", "free", "used", "upnl"], "balance");
                let amount = match i {
                    0 => 2.5,
                    1 => -0.,
                    2 => f64::NAN,
                    3 | 5 => f64::INFINITY,
                    4 => f64::NEG_INFINITY,
                    6 => 0.,
                    8 => 0.,
                    9 => 0.25 / 1e6,
                    _ => unreachable!(),
                };
                n(&raw_field(out, "total"), amount, "balance.total");
                n(&raw_field(out, "free"), amount, "balance.free");
                n(&raw_field(out, "used"), 0., "balance.used");
                n(&raw_field(out, "upnl"), 0., "balance.upnl");
            }
            "fetchPositions" => {
                let V::Array(_, items) = out else {
                    panic!("native positions array")
                };
                let absent = i == 1 || i == 2;
                assert_eq!(
                    items.borrow().len(),
                    if absent { 0 } else { 2 },
                    "native position count"
                );
                for (index, item) in items.borrow().iter().enumerate() {
                    let item = item.as_ref().unwrap();
                    keys(
                        item,
                        &[
                            "id",
                            "symbol",
                            "side",
                            "entryPrice",
                            "notional",
                            "leverage",
                            "unrealizedPnl",
                            "contracts",
                            "liquidationPrice",
                            "positionMode",
                            "info",
                        ],
                        "position",
                    );
                    let symbol = if index == 0 { "S-a" } else { "S-b" };
                    t(&raw_field(item, "id"), symbol, "position.id");
                    t(&raw_field(item, "symbol"), symbol, "position.symbol");
                    t(&raw_field(item, "side"), "long", "position.side");
                    t(&raw_field(item, "positionMode"), "oneway", "position.mode");
                    let avg = if index == 0 { 0.25 } else { 0.75 };
                    let current = if index == 0 {
                        if i == 4 { 0.625 } else { 0.5 }
                    } else if i == 6 {
                        0.875
                    } else {
                        0.25
                    };
                    let size = if index == 0 { 2. } else { 3. };
                    let pnl = if index == 0 {
                        if i == 5 { -0. } else { (current - avg) * size }
                    } else if i == 7 {
                        (current - avg) * size
                    } else {
                        -1.
                    };
                    n(&raw_field(item, "entryPrice"), avg, "position.entry");
                    n(
                        &raw_field(item, "notional"),
                        size * current,
                        "position.notional",
                    );
                    n(&raw_field(item, "leverage"), 1., "position.leverage");
                    n(&raw_field(item, "unrealizedPnl"), pnl, "position.pnl");
                    n(&raw_field(item, "contracts"), size, "position.contracts");
                    n(
                        &raw_field(item, "liquidationPrice"),
                        0.,
                        "position.liquidation",
                    );
                    let info = raw_field(item, "info");
                    keys(
                        &info,
                        &["tokenId", "conditionId", "redeemable", "mergeable"],
                        "position.info",
                    );
                    t(
                        &raw_field(&info, "tokenId"),
                        if index == 0 { "a" } else { "b" },
                        "info.token",
                    );
                    t(
                        &raw_field(&info, "conditionId"),
                        if index == 0 {
                            "condition-30"
                        } else {
                            "condition-31"
                        },
                        "info.condition",
                    );
                    assert!(
                        matches!(raw_field(&info,"redeemable"),V::Bool(b)if b==(index==0)),
                        "native redeemable"
                    );
                    assert!(
                        matches!(raw_field(&info,"mergeable"),V::Bool(b)if b==(index==1)),
                        "native mergeable"
                    );
                }
                assert_eq!(
                    h.cost.borrow().len(),
                    if absent { 0 } else { 2 },
                    "native cost basis count"
                );
            }
            "fetchAndSetPositions" => {
                assert!(
                    same(out, &h.positions),
                    "native snapshot return original alias"
                );
                assert_eq!(
                    h.updates.borrow().len(),
                    usize::from(!matches!(i, 4 | 8)),
                    "native snapshot commit count"
                );
                if !h.updates.borrow().is_empty() {
                    assert!(
                        same(
                            &raw_field(&h.updates.borrow()[0], "positions"),
                            &h.positions
                        ),
                        "native snapshot committed original alias"
                    );
                    assert!(
                        matches!(raw_field(&h.loaded.borrow(), "positions"), V::Bool(true)),
                        "native loaded positions"
                    );
                }
            }
            _ => assert!(matches!(out, V::U), "native void value {name}/{i}"),
        }
    }
    if name == "tick" {
        assert_eq!(
            h.scheduled.borrow().len(),
            usize::from(!matches!(i, 4 | 5 | 6 | 8 | 9)),
            "native next tick count"
        );
        if i == 0 || i == 2 {
            assert_eq!(h.updates.borrow().len(), 2, "native tick updates");
            assert!(
                same(&raw_field(&h.updates.borrow()[0], "balance"), &h.balance),
                "native original balance alias"
            );
            n(&raw_field(&h.balance, "upnl"), 1., "tick.upnl");
        }
        if i == 3 {
            n(&raw_field(&h.balance, "upnl"), 0., "tick.empty upnl");
        }
    }
    if name == "start" {
        let expected = match i {
            0 | 3 => 3,
            1 => 0,
            2 => 3,
            4 | 5 | 7 | 8 => 0,
            6 | 9 => 1,
            _ => unreachable!(),
        };
        assert_eq!(
            h.updates.borrow().len(),
            expected,
            "native startup commit count"
        );
        if i == 2 {
            let last = h.updates.borrow().last().unwrap().clone();
            keys(&last, &["loaded"], "public-only patch");
            for k in ["balance", "positions", "orders"] {
                assert!(
                    matches!(raw_field(&raw_field(&last, "loaded"), k), V::Bool(true)),
                    "native public loaded {k}"
                );
            }
        }
    }
    if name == "handleTradeUpdate" {
        assert_eq!(h.fills.borrow().len(), 1, "native fill count");
        let fills = h.fills.borrow();
        let fill = &fills[0];
        let trade = h.value("trade");
        let V::Text(id) = raw_field(&trade, "id") else {
            panic!("id")
        };
        let V::Text(got) = &fill.id else {
            panic!("fill.id")
        };
        assert_eq!(*got, id, "native fill id");
        t(&fill.side, if i == 3 { "sell" } else { "buy" }, "fill.side");
        t(
            &fill.symbol,
            if i == 4 { "a" } else { "S-a" },
            "fill.symbol",
        );
        n(
            &V::Number(fill.price),
            if i == 2 { -0. } else { 0.25 },
            "fill.price",
        );
        n(
            &V::Number(fill.amount),
            if i == 5 { f64::INFINITY } else { 2. },
            "fill.amount",
        );
    }
    if name == "refreshPositions" {
        assert_eq!(h.calls.get(), 1, "native refresh child count");
        assert_eq!(
            h.logs.borrow().len(),
            usize::from(i == 7 || i == 9),
            "native refresh rejection logging"
        );
        if i == 7 || i == 9 {
            let V::Text(reason) = h.value("reason") else {
                panic!("reason")
            };
            let mut expected = units("Error refreshing positions after ");
            expected.extend(reason);
            expected.extend(units(": Error: boundary failure"));
            assert_eq!(h.logs.borrow()[0].0, expected, "native refresh error text");
            assert_eq!(h.logs.borrow()[0].1, "error");
            assert_eq!(
                h.detached.borrow().len(),
                usize::from(i == 9),
                "native detached logger rejection count"
            );
        }
    }
}
fn word(i: usize) -> Vec<u16> {
    match i {
        2 => vec![0xd800],
        3 => vec![0xdfff],
        4 => units("🦀"),
        7 => vec![120, 0, 121],
        _ => units(
            [
                "A/one",
                "",
                "",
                "",
                "",
                "__proto__",
                "constructor",
                "",
                "late",
                "last",
            ][i],
        ),
    }
}
fn text_units(v: &V, expected: &[u16], label: &str) {
    let V::Text(actual) = v else {
        panic!("native text arm {label}")
    };
    assert_eq!(actual, expected, "native text {label}");
}
fn native_effects(h: &Host) {
    let i = h.i;
    let name = h.name.as_str();
    let expected_async: Vec<(&str, &str)> = match name {
        "deriveApiCredentials" => {
            if i == 2 {
                vec![]
            } else if i == 7 {
                vec![("api", "getServerTime")]
            } else {
                vec![
                    ("api", "getServerTime"),
                    ("imports", "signApiKeyDerivationMessage"),
                    ("api", "deriveApiKey"),
                ]
            }
        }
        "start" => {
            let methods: &[&str] = match i {
                1 => &["initExternalPublicData", "tick"],
                7 => &["initExternalPublicData"],
                4 | 5 | 8 | 9 => &["initExternalPublicData", "fetchMarkets"],
                2 | 6 => &["initExternalPublicData", "fetchMarkets", "fetchTickers"],
                0 | 3 => &[
                    "initExternalPublicData",
                    "fetchMarkets",
                    "fetchTickers",
                    "tick",
                    "fetchOrders",
                ],
                _ => unreachable!(),
            };
            methods.iter().map(|m| ("exchange", *m)).collect()
        }
        "tick" => {
            let methods: &[&str] = match i {
                1 | 8 => &[],
                4 | 7 => &["fetchAndSetPositions"],
                5 | 9 => &["fetchAndSetPositions", "fetchBalance"],
                0 | 2 | 3 | 6 => &["fetchAndSetPositions", "fetchBalance", "fetchOrders"],
                _ => unreachable!(),
            };
            methods.iter().map(|m| ("exchange", *m)).collect()
        }
        "fetchBalance" => vec![("api", "getBalanceAllowance")],
        "fetchPositions" => {
            if matches!(i, 1 | 2) {
                vec![]
            } else {
                vec![("api", "getPositions")]
            }
        }
        "fetchAndSetPositions" => vec![("exchange", "fetchPositions")],
        "refreshPositions" => vec![("parent", "fetchAndSetPositions")],
        "handleTradeUpdate" => vec![],
        _ => unreachable!(),
    };
    assert_eq!(
        *h.async_roles.borrow(),
        expected_async,
        "native independent child role sequence {name}/{i}"
    );
    for (receiver, method, args) in h.native_calls.borrow().iter() {
        match (*receiver, *method) {
            ("api", "getBalanceAllowance") => {
                assert_eq!(args.len(), 1);
                n(
                    &args[0],
                    if i == 3 { -0. } else { i as f64 },
                    "signer argument",
                );
            }
            ("imports", "signApiKeyDerivationMessage") => {
                assert_eq!(args.len(), 3);
                assert!(
                    same(&args[0], &h.value("wallet")),
                    "native original signer wallet alias"
                );
                n(&args[1], 0., "signer nonce");
                n(
                    &args[2],
                    if i == 3 { -0. } else { 1234. },
                    "signer server time",
                );
            }
            ("api", "deriveApiKey") => {
                assert_eq!(args.len(), 4);
                text_units(&args[0], &word(i), "derived address");
                n(&args[1], 0., "derive nonce");
                let mut sig = units("sig-");
                sig.extend(word(i));
                text_units(&args[2], &sig, "derived signature");
                n(
                    &args[3],
                    if i == 3 { -0. } else { 123. },
                    "derived timestamp",
                );
            }
            ("api", "getPositions") => {
                assert_eq!(args.len(), 1);
                text_units(
                    &args[0],
                    &if i == 3 { units("funder") } else { word(i) },
                    "position user",
                );
            }
            ("imports", "createPositionId") => {
                assert_eq!(args.len(), 3);
                t(&args[1], "long", "position id side argument");
                t(&args[2], "oneway", "position id mode argument");
            }
            _ => {}
        }
    }
    if name == "deriveApiCredentials" {
        assert_eq!(
            h.calls.get(),
            if i == 2 {
                0
            } else if i == 7 {
                1
            } else {
                3
            },
            "native derivation child count"
        );
    }
    if name == "fetchBalance" {
        assert_eq!(h.calls.get(), 1, "native balance child count");
    }
    if name == "fetchPositions" {
        assert_eq!(
            h.calls.get(),
            usize::from(!matches!(i, 1 | 2)),
            "native positions child count"
        );
        for (index, (token, price)) in h.cost.borrow().iter().enumerate() {
            t(token, if index == 0 { "a" } else { "b" }, "cost token");
            n(price, if index == 0 { 0.25 } else { 0.75 }, "cost average");
        }
    }
    let patch_roles: &[&str] = match name {
        "start" => match i {
            0 | 3 => &["markets", "tickers", "orders"],
            2 => &["markets", "tickers", "public"],
            6 | 9 => &["markets"],
            1 | 4 | 5 | 7 | 8 => &[],
            _ => unreachable!(),
        },
        "tick" => match i {
            0 | 2 | 3 => &["balance", "orders"],
            6 | 9 => &["balance"],
            1 | 4 | 5 | 7 | 8 => &[],
            _ => unreachable!(),
        },
        "fetchAndSetPositions" => {
            if matches!(i, 4 | 7 | 8) {
                &[]
            } else {
                &["positions"]
            }
        }
        _ => &[],
    };
    assert_eq!(
        h.updates.borrow().len(),
        patch_roles.len(),
        "native independent patch role count {name}/{i}"
    );
    for (patch_index, patch) in h.updates.borrow().iter().enumerate() {
        let flag = patch_roles[patch_index];
        if flag == "public" {
            keys(patch, &["loaded"], "independent public patch keys");
        } else {
            keys(patch, &[flag, "loaded"], "independent patch role keys");
        }

        let loaded = raw_field(patch, "loaded");
        let V::Object(_, fields) = &loaded else {
            panic!("loaded native record")
        };
        assert!(
            !same(&loaded, &h.value("loaded")),
            "native loaded spread fresh"
        );
        assert!(
            matches!(raw_field(&loaded, "old"), V::Bool(true)),
            "native loaded old retained"
        );
        let (original, label) = match flag {
            "markets" => (h.value("markets"), "native startup original markets alias"),
            "tickers" => (h.value("tickers"), "native startup original tickers alias"),
            "orders" => (h.value("orders"), "native original orders alias"),
            "balance" => (h.balance.clone(), "native balance commit original alias"),
            "positions" => (h.positions.clone(), "native position commit original alias"),
            "public" => (V::U, "public"),
            _ => unreachable!(),
        };
        if flag != "public" {
            assert!(same(&raw_field(patch, flag), &original), "{label}");
        }
        let expected_keys: Vec<&str> = if name == "start" {
            match flag {
                "markets" => vec!["old", "positions", "balance", "markets"],
                "tickers" => vec!["old", "positions", "balance", "markets", "tickers"],
                _ => vec![
                    "old",
                    "positions",
                    "balance",
                    "markets",
                    "tickers",
                    "orders",
                ],
            }
        } else if flag == "orders" {
            vec!["old", "positions", "balance", "orders"]
        } else {
            vec!["old", "positions", "balance"]
        };
        keys(&loaded, &expected_keys, "loaded flags");
        if flag != "public" {
            assert!(
                matches!(raw_field(&loaded, flag), V::Bool(true)),
                "native loaded true {flag}"
            );
        }
        assert!(
            matches!(fields.borrow()[0].1, V::Bool(true)),
            "native first loaded flag"
        );
    }
}
fn family(name: &str) {
    let mut comparisons = vec![];
    for i in 0..10 {
        let case = oracle()["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name && c["i"] == i)
            .unwrap();
        let h = Rc::new(Host::new(case));
        let result = if name == "refreshPositions" {
            let initial = refresh_positions(h.clone(), h.value("reason"));
            assert!(
                h.logs.borrow().is_empty(),
                "native refresh callback initially deferred"
            );
            h.finish_all();
            initial.map(|_| V::U)
        } else if name == "handleTradeUpdate" {
            handle_trade_update(h.as_ref(), h.value("trade")).map(|_| V::U)
        } else {
            let mut observer = method(h.clone()).unwrap();
            h.finish_all();
            let mut cx = Context::from_waker(Waker::noop());
            let Poll::Ready(result) = observer.as_mut().poll(&mut cx) else {
                panic!("native settled body")
            };
            result
        };
        if !h.scheduled.borrow().is_empty() {
            h.late.set(true);
            for cb in h.scheduled.borrow().iter() {
                let _ = cb().unwrap();
            }
        }
        native(h.as_ref(), &result);
        native_effects(h.as_ref());
        comparisons.push((h, result, case));
    }
    for (h, result, case) in comparisons {
        let result = match result {
            Ok(v) => json!({"value":project(&v)}),
            Err(e) => {
                let V::Text(message) = &e.message else {
                    panic!("error")
                };
                json!({"error":{"marker":e.marker,"message":message}})
            }
        };
        assert_eq!(result, case["result"], "source result {name}/{}", h.i);
        assert_eq!(
            json!(*h.events.borrow()),
            case["events"],
            "source events {name}/{}",
            h.i
        );
        assert_eq!(
            json!(h.updates.borrow().iter().map(project).collect::<Vec<_>>()),
            case["updates"],
            "source updates {name}/{}",
            h.i
        );
        assert_eq!(
            project(&h.credentials.borrow()),
            case["credentials"],
            "source credentials {name}/{}",
            h.i
        );
        assert_eq!(
            json!(
                h.detached
                    .borrow()
                    .iter()
                    .map(|e| {
                        let V::Text(m) = &e.message else {
                            panic!("message")
                        };
                        json!({"marker":e.marker,"message":m})
                    })
                    .collect::<Vec<_>>()
            ),
            case["detached"]
        );
    }
}
#[test]
fn derive_native() {
    family("deriveApiCredentials")
}
#[test]
fn start_native() {
    family("start")
}
#[test]
fn tick_native() {
    family("tick")
}
#[test]
fn balance_native() {
    family("fetchBalance")
}
#[test]
fn positions_native() {
    family("fetchPositions")
}
#[test]
fn snapshot_native() {
    family("fetchAndSetPositions")
}
#[test]
fn refresh_native() {
    family("refreshPositions")
}
#[test]
fn trade_native() {
    family("handleTradeUpdate")
}
