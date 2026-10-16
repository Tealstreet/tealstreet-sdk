use safe_cex_venue_phemex::{
    start_order_formatting::{self as order, ExchangeState, OrderHost},
    websocket_data::WsHost,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Clone, Debug)]
enum V {
    Undefined,
    Null,
    Bool(bool),
    Num(f64),
    Text(Vec<u16>),
    Array(Rc<RefCell<Vec<V>>>),
    Object(Rc<RefCell<Vec<(Vec<u16>, V)>>>),
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn arr(v: Vec<V>) -> V {
    V::Array(Rc::new(RefCell::new(v)))
}
fn obj(v: Vec<(&str, V)>) -> V {
    V::Object(Rc::new(RefCell::new(
        v.into_iter().map(|(k, v)| (u(k), v)).collect(),
    )))
}
fn txt(s: &str) -> V {
    V::Text(u(s))
}
impl V {
    fn decode(v: &Value) -> Self {
        if let Some(a) = v.get("$units").and_then(Value::as_array) {
            return Self::Text(a.iter().map(|v| v.as_u64().unwrap() as u16).collect());
        }
        if let Some(b) = v.get("$bits").and_then(Value::as_str) {
            return Self::Num(f64::from_bits(u64::from_str_radix(b, 16).unwrap()));
        }
        if v.get("$undefined").is_some() {
            return Self::Undefined;
        }
        match v {
            Value::Null => Self::Null,
            Value::Bool(b) => Self::Bool(*b),
            Value::Number(n) => Self::Num(n.as_f64().unwrap()),
            Value::String(s) => txt(s),
            Value::Array(v) => arr(v.iter().map(Self::decode).collect()),
            Value::Object(v) => Self::Object(Rc::new(RefCell::new(
                v.iter().map(|(k, v)| (u(k), Self::decode(v))).collect(),
            ))),
        }
    }
    fn prop(&self, k: &[u16]) -> V {
        match self {
            Self::Object(o) => o
                .borrow()
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
                .unwrap_or(Self::Undefined),
            Self::Array(a) if k == u("length") => Self::Num(a.borrow().len() as f64),
            Self::Text(t) if k == u("length") => Self::Num(t.len() as f64),
            _ => Self::Undefined,
        }
    }
    fn get(&self, k: &str) -> V {
        self.prop(&u(k))
    }
    fn set(&self, k: &[u16], v: V) {
        if let Self::Object(o) = self {
            let mut o = o.borrow_mut();
            if let Some((_, old)) = o.iter_mut().find(|(key, _)| key == k) {
                *old = v
            } else {
                o.push((k.to_vec(), v))
            }
        } else {
            panic!("native mutable object")
        }
    }
    fn rows(&self) -> Vec<V> {
        if let Self::Array(a) = self {
            a.borrow().clone()
        } else {
            vec![]
        }
    }
    fn truthy(&self) -> bool {
        match self {
            Self::Undefined | Self::Null => false,
            Self::Bool(v) => *v,
            Self::Num(v) => *v != 0.0 && !v.is_nan(),
            Self::Text(t) => !t.is_empty(),
            _ => true,
        }
    }
    fn number(&self) -> f64 {
        match self {
            Self::Undefined => f64::NAN,
            Self::Null => 0.0,
            Self::Bool(v) => {
                if *v {
                    1.0
                } else {
                    0.0
                }
            }
            Self::Num(n) => *n,
            Self::Text(t) => String::from_utf16(t)
                .map(|s| safe_cex_num::number_from_string(&s))
                .unwrap_or(f64::NAN),
            Self::Array(v) => {
                let v = v.borrow();
                if v.is_empty() {
                    0.0
                } else if v.len() == 1 {
                    v[0].number()
                } else {
                    f64::NAN
                }
            }
            _ => f64::NAN,
        }
    }
    fn text(&self) -> Vec<u16> {
        match self {
            Self::Undefined => u("undefined"),
            Self::Null => u("null"),
            Self::Bool(v) => u(if *v { "true" } else { "false" }),
            Self::Num(n) => u(&safe_cex_num::js_number_string(*n)),
            Self::Text(t) => t.clone(),
            _ => u("[object Object]"),
        }
    }
    fn parse(&self) -> f64 {
        safe_cex_num::parse_float(&String::from_utf16_lossy(&self.text()))
    }
    fn snapshot(&self) -> V {
        match self {
            Self::Array(v) => arr(v.borrow().iter().map(Self::snapshot).collect()),
            Self::Object(v) => Self::Object(Rc::new(RefCell::new(
                v.borrow()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.snapshot()))
                    .collect(),
            ))),
            _ => self.clone(),
        }
    }
    fn obs(&self) -> Value {
        match self {
            Self::Undefined => json!({"undefined":true}),
            Self::Null => Value::Null,
            Self::Bool(v) => json!(v),
            Self::Num(n) => {
                json!({"bits":format!("{:016x}",if n.is_nan(){f64::NAN.to_bits()}else{n.to_bits()})})
            }
            Self::Text(t) => json!({"units":t}),
            Self::Array(v) => Value::Array(v.borrow().iter().map(Self::obs).collect()),
            Self::Object(v) => {
                let mut entries = v
                    .borrow()
                    .iter()
                    .map(|(k, v)| json!([{"units":k},v.obs()]))
                    .collect::<Vec<_>>();
                entries.sort_by_key(|v| v[0].to_string());
                json!({"object":entries})
            }
        }
    }
}
fn eq(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Undefined, V::Undefined) | (V::Null, V::Null) => true,
        (V::Bool(a), V::Bool(b)) => a == b,
        (V::Num(a), V::Num(b)) => a == b,
        (V::Text(a), V::Text(b)) => a == b,
        (V::Object(a), V::Object(b)) => Rc::ptr_eq(a, b),
        (V::Array(a), V::Array(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn ascii(v: &V) -> String {
    String::from_utf16_lossy(&v.text())
}
#[derive(Clone, Debug)]
struct E {
    original: bool,
}
fn poll(task: &mut Pin<Box<dyn Future<Output = Result<(), E>>>>) -> Poll<Result<(), E>> {
    let w = Waker::noop().clone();
    task.as_mut().poll(&mut Context::from_waker(&w))
}
type Task = Pin<Box<dyn Future<Output = Result<(), E>>>>;
type Timer = Box<dyn FnOnce() -> Result<(), E>>;
struct State {
    store: V,
    fields: V,
    effects: Vec<V>,
    tasks: Vec<Task>,
    uuid: usize,
    api: usize,
    put: usize,
}
#[derive(Clone)]
struct Host {
    input: V,
    state: Rc<RefCell<State>>,
    pending: Rc<Cell<usize>>,
}
fn market() -> V {
    obj(vec![
        ("symbol", txt("BTCUSDT")),
        (
            "precision",
            obj(vec![("price", V::Num(0.1)), ("amount", V::Num(0.1))]),
        ),
        (
            "limits",
            obj(vec![("amount", obj(vec![("max", V::Num(10.0))]))]),
        ),
    ])
}
fn base_order() -> V {
    obj(vec![
        ("id", txt("a")),
        ("symbol", txt("BTCUSDT")),
        ("type", txt("limit")),
        ("side", txt("buy")),
        ("amount", V::Num(2.0)),
        ("price", V::Num(10.0)),
        ("reduceOnly", V::Bool(false)),
    ])
}
impl Host {
    fn new(input: V) -> Self {
        let opts = input.get("opts");
        let symbol = if matches!(opts.get("symbol"), V::Undefined) {
            txt("BTCUSDT")
        } else {
            opts.get("symbol")
        };
        let market = if matches!(input.get("market"), V::Undefined) {
            market()
        } else {
            input.get("market")
        };
        let markets = obj(vec![]);
        let tickers = obj(vec![]);
        if !input.get("missingMarket").truthy() {
            markets.set(&symbol.text(), market.clone())
        }
        if !input.get("missingTicker").truthy() {
            tickers.set(&symbol.text(), obj(vec![("last", V::Num(10.0))]))
        }
        let store = obj(vec![
            (
                "orders",
                if matches!(input.get("orders"), V::Undefined) {
                    arr(vec![base_order()])
                } else {
                    input.get("orders")
                },
            ),
            ("markets", arr(vec![market])),
            ("marketsBySymbol", markets),
            ("tickersBySymbol", tickers),
            (
                "positions",
                if matches!(input.get("positions"), V::Undefined) {
                    arr(vec![])
                } else {
                    input.get("positions")
                },
            ),
            ("loaded", obj(vec![("positions", V::Bool(false))])),
        ]);
        let fields = obj(vec![
            ("disposed", V::Bool(input.get("disposed").truthy())),
            ("publicOnly", V::Bool(input.get("publicOnly").truthy())),
        ]);
        Self {
            input,
            state: Rc::new(RefCell::new(State {
                store,
                fields,
                effects: vec![],
                tasks: vec![],
                uuid: 0,
                api: 0,
                put: 0,
            })),
            pending: Rc::new(Cell::new(0)),
        }
    }
    fn effect(&mut self, name: &str, args: Vec<V>) -> Result<(), E> {
        let mut values = vec![txt(name)];
        values.extend(args);
        self.state.borrow_mut().effects.push(arr(values).snapshot());
        if eq(&self.input.get("failure"), &txt(name)) {
            Err(E { original: true })
        } else {
            Ok(())
        }
    }
    fn store_value(&self) -> V {
        self.state.borrow().store.clone()
    }
    fn fields(&self) -> V {
        self.state.borrow().fields.clone()
    }
    fn reject(&mut self, msg: String) -> Result<V, E> {
        self.effect("emit", vec![txt(&msg)])?;
        Err(self.error(txt(&msg))?)
    }
    fn caller(&mut self, opts: &V) -> Result<V, E> {
        let id = opts.get("clientOrderId");
        if matches!(id, V::Undefined) {
            return Ok(id);
        }
        let type_ = ascii(&opts.get("type"));
        let shape = (type_ == "limit" || type_ == "market")
            && ["triggerPrice", "stopLoss", "takeProfit"]
                .iter()
                .all(|k| matches!(opts.get(k), V::Undefined));
        if !shape {
            let mut msg = format!(
                "Phemex clientOrderId cannot be honored for this order shape (type={type_}"
            );
            for k in ["stopLoss", "takeProfit", "triggerPrice"] {
                if !matches!(opts.get(k), V::Undefined) {
                    msg.push_str(&format!(" +{k}"))
                }
            }
            if opts.get("close").truthy() {
                msg.push_str(" +close")
            }
            msg.push_str(
                "); caller client order IDs are supported only on plain Limit/Market orders",
            );
            return self.reject(msg);
        }
        if !matches!(&id,V::Text(s)if valid(s)) {
            return self.reject("Phemex clientOrderId must match /^.{1,40}$/".into());
        }
        let market = self
            .store_value()
            .get("marketsBySymbol")
            .prop(&opts.get("symbol").text());
        if market.truthy() {
            let max = market.get("limits").get("amount").get("max").number();
            let precision = market.get("precision").get("amount");
            if max > 0.0 && adjust(&opts.get("amount"), &precision) > max {
                return self.reject("Phemex order exceeds the market's max size for one order; a caller clientOrderId cannot span several orders".into());
            }
        }
        Ok(id)
    }
    fn defaults(&mut self) -> Result<ExchangeState<V>, E> {
        let rule = self.call("clientRule", vec![])?;
        let periodic = self.call("grafanaPeriodic", vec![])?;
        let every = self.call("grafanaEvery", vec![])?;
        let frames=serde_json::from_str::<Value>(r#"{"1m":"60","3m":"180","5m":"300","15m":"900","30m":"1800","1h":"3600","2h":"7200","3h":"10800","4h":"14400","6h":"21600","12h":"43200","1d":"86400","1w":"604800","1M":"2592000","3M":"7776000","1Y":"31104000"}"#).unwrap();
        Ok(ExchangeState {
            client_order_id_rule: rule,
            xhr: None,
            unlimited_xhr: None,
            public_websocket: None,
            private_websocket: None,
            grafana_endpoints_periodic: periodic,
            grafana_endpoints_every: every,
            timeframes: V::decode(&frames),
        })
    }
    fn drive(&mut self) -> Result<(), E> {
        let tasks = std::mem::take(&mut self.state.borrow_mut().tasks);
        for mut task in tasks {
            match poll(&mut task) {
                Poll::Ready(v) => v?,
                Poll::Pending => self.state.borrow_mut().tasks.push(task),
            }
        }
        Ok(())
    }
}
fn valid(s: &[u16]) -> bool {
    !s.is_empty() && s.len() <= 40 && !s.iter().any(|v| [10, 13, 0x2028, 0x2029].contains(v))
}
fn adjust(v: &V, step: &V) -> f64 {
    if let V::Text(s) = step {
        safe_cex_num::adjust(v.number(), String::from_utf16_lossy(s))
    } else {
        safe_cex_num::adjust(v.number(), step.number())
    }
}
struct Read {
    host: Host,
    member: &'static str,
    result: Option<V>,
    wait: usize,
    settled: bool,
}
impl Future for Read {
    type Output = Result<V, E>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.wait > 0 {
            self.wait -= 1;
            self.host.pending.set(self.host.pending.get() + 1);
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        if !self.settled {
            self.settled = true;
            let member = self.member;
            self.host.effect("settle", vec![txt(member)])?;
            let fields = self.host.fields();
            if eq(&self.host.input.get("disposeOn"), &txt(member)) {
                fields.set(&u("disposed"), V::Bool(true));
            }
            if self.host.input.get("mutatePublicOnly").truthy() && member == "fetchMarkets" {
                fields.set(&u("publicOnly"), V::Bool(false));
            }
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        if eq(&self.host.input.get("reject"), &txt(self.member)) {
            Poll::Ready(Err(E { original: true }))
        } else {
            Poll::Ready(Ok(self.result.take().unwrap()))
        }
    }
}
impl WsHost for Host {
    type V = V;
    type E = E;
    fn get(&mut self, v: &V, key: &[u16]) -> Result<V, E> {
        if matches!(v, V::Null | V::Undefined) {
            Err(E { original: false })
        } else {
            Ok(v.prop(key))
        }
    }
    fn optional(&mut self, v: &V, key: &[u16]) -> Result<V, E> {
        Ok(v.prop(key))
    }
    fn set(&mut self, v: &V, key: &[u16], new: V) -> Result<(), E> {
        v.set(key, new);
        Ok(())
    }
    fn remove(&mut self, v: &V, key: &[u16]) -> Result<(), E> {
        if let V::Object(o) = v {
            o.borrow_mut().retain(|(k, _)| k != key)
        }
        Ok(())
    }
    fn truthy(&mut self, v: &V) -> bool {
        v.truthy()
    }
    fn nullish(&mut self, v: &V) -> bool {
        matches!(v, V::Undefined | V::Null)
    }
    fn same(&mut self, a: &V, b: &V) -> bool {
        eq(a, b)
    }
    fn set_same(&mut self, a: &V, b: &V) -> bool {
        eq(a, b) || matches!((a,b),(V::Num(a),V::Num(b))if a.is_nan()&&b.is_nan())
    }
    fn number(&mut self, v: V) -> Result<f64, E> {
        Ok(v.number())
    }
    fn parse(&mut self, v: V) -> Result<f64, E> {
        Ok(v.parse())
    }
    fn text(&mut self, v: V) -> Result<Vec<u16>, E> {
        Ok(v.text())
    }
    fn lower(&mut self, v: V) -> Result<Vec<u16>, E> {
        Ok(u(&ascii(&v).to_lowercase()))
    }
    fn num(&mut self, v: f64) -> V {
        V::Num(v)
    }
    fn string(&mut self, v: Vec<u16>) -> V {
        V::Text(v)
    }
    fn undefined(&mut self) -> V {
        V::Undefined
    }
    fn boolean(&mut self, v: bool) -> V {
        V::Bool(v)
    }
    fn array(&mut self, v: Vec<V>) -> V {
        arr(v)
    }
    fn object(&mut self, v: Vec<(&'static str, V)>) -> V {
        let result = obj(vec![]);
        for (k, v) in v {
            result.set(&u(k), v)
        }
        result
    }
    fn spread(&mut self, v: &V) -> Result<V, E> {
        Ok(if let V::Object(o) = v {
            V::Object(Rc::new(RefCell::new(o.borrow().clone())))
        } else {
            obj(vec![])
        })
    }
    fn rows(&mut self, v: V) -> Result<Vec<V>, E> {
        if let V::Array(a) = v {
            Ok(a.borrow().clone())
        } else {
            Err(E { original: false })
        }
    }
    fn values(&mut self, v: &V) -> Result<Vec<V>, E> {
        Ok(if let V::Object(o) = v {
            o.borrow().iter().map(|(_, v)| v.clone()).collect()
        } else {
            vec![]
        })
    }
    fn at(&mut self, v: &V, i: usize) -> Result<V, E> {
        if let V::Array(a) = v {
            Ok(a.borrow().get(i).cloned().unwrap_or(V::Undefined))
        } else {
            Err(E { original: false })
        }
    }
    fn is_array(&mut self, v: &V) -> bool {
        matches!(v, V::Array(_))
    }
    fn store(&mut self, name: &'static str) -> Result<V, E> {
        Ok(self.store_value().get(name))
    }
    fn field(&mut self, name: &'static str) -> Result<V, E> {
        Ok(self.fields().get(name))
    }
    fn call(&mut self, name: &'static str, args: Vec<V>) -> Result<V, E> {
        match name {
            "null" => return Ok(V::Null),
            "adjust" => return Ok(V::Num(adjust(&args[0], &args[1]))),
            "capitalize" => {
                let s = if matches!(args[0], V::Undefined | V::Null) {
                    String::new()
                } else {
                    ascii(&args[0]).to_lowercase()
                };
                let mut chars = s.chars();
                let capital = if let Some(c) = chars.next() {
                    c.to_uppercase().collect::<String>() + chars.as_str()
                } else {
                    s
                };
                return Ok(txt(&capital));
            }
            "omitUndefined" => {
                let V::Object(o) = &args[0] else { panic!() };
                return Ok(V::Object(Rc::new(RefCell::new(
                    o.borrow()
                        .iter()
                        .filter(|(_, v)| !matches!(v, V::Undefined))
                        .cloned()
                        .collect(),
                ))));
            }
            "grafanaPeriodic" | "grafanaEvery" => {
                let values = if name == "grafanaPeriodic" {
                    &safe_cex_venue_phemex::constants::GRAFANA_ENDPOINTS_PERIODIC[..]
                } else {
                    &safe_cex_venue_phemex::constants::GRAFANA_ENDPOINTS_EVERY[..]
                };
                return Ok(arr(values.iter().map(|s| txt(s)).collect()));
            }
            "clientRule" => {
                self.effect("rule", vec![txt("Phemex"), txt("/^.{1,40}$/")])?;
                return Ok(obj(vec![
                    ("label", txt("Phemex")),
                    ("requirement", txt("match /^.{1,40}$/")),
                ]));
            }
            "createAPI" => {
                let n = {
                    let mut s = self.state.borrow_mut();
                    s.api += 1;
                    s.api
                };
                assert!(
                    eq(&args[0], &self.fields().get("opts")),
                    "ORDERS native constructor opts identity"
                );
                assert_eq!(
                    self.fields().get("xhr").obs(),
                    if n == 1 {
                        V::Undefined.obs()
                    } else {
                        txt("limited:api1").obs()
                    },
                    "ORDERS native constructor assignment before API"
                );
                assert_eq!(
                    self.fields().get("timeframes").get("1m").obs(),
                    txt("60").obs(),
                    "ORDERS native fields before API"
                );
                self.effect(&format!("createAPI:{n}"), args)?;
                return Ok(txt(&format!("api{n}")));
            }
            _ => {}
        }
        self.effect(name, args.clone())?;
        Ok(match name {
            "uuid" => {
                let mut s = self.state.borrow_mut();
                s.uuid += 1;
                txt(&format!("id{}", s.uuid))
            }
            "resolveCaller" => self.caller(&args[0])?,
            "assertSingle" => {
                if !matches!(args[0], V::Undefined) && args[1].number() != 1.0 {
                    return self.reject("Phemex clientOrderId is supported only when one order maps to one native order".into());
                }
                V::Undefined
            }
            "publicSocket" => {
                assert!(
                    eq(&self.fields().get("unlimitedXHR"), &txt("api2")),
                    "ORDERS native constructor API before socket"
                );
                txt("public")
            }
            "privateSocket" => {
                assert!(
                    eq(&self.fields().get("publicWebsocket"), &txt("public")),
                    "ORDERS native constructor socket assignment"
                );
                txt("private")
            }
            "formatOut" => args[0].clone(),
            "parseTickers" => arr(vec![obj(vec![("id", txt("ticker"))])]),
            "mapOrder" => {
                if matches!(self.input.get("mappedOrders"), V::Undefined) {
                    arr(vec![obj(vec![
                        ("id", args[0].get("orderID")),
                        ("status", txt("open")),
                    ])])
                } else {
                    self.input.get("mappedOrders")
                }
            }
            "update" => {
                let V::Object(o) = &args[0] else { panic!() };
                for (k, v) in o.borrow().iter() {
                    self.store_value().set(k, v.clone())
                }
                V::Undefined
            }
            "upsertOrders" => {
                self.store_value().set(&u("orders"), args[0].clone());
                V::Undefined
            }
            "emit" | "connectPublic" => V::Undefined,
            _ => panic!("unknown original member {name}"),
        })
    }
    fn read(
        &mut self,
        _: &'static str,
        _: V,
    ) -> Result<Pin<Box<dyn Future<Output = Result<V, E>>>>, E> {
        panic!("unused websocket read")
    }
    fn spawn(&mut self, mut task: Task) -> Result<(), E> {
        match poll(&mut task) {
            Poll::Ready(v) => v,
            Poll::Pending => {
                self.state.borrow_mut().tasks.push(task);
                Ok(())
            }
        }
    }
    fn timer(&mut self, _: f64, _: Timer) -> Result<V, E> {
        panic!("unused websocket timer")
    }
    fn error_text(&mut self, _: E) -> Result<Vec<u16>, E> {
        panic!("unused websocket error text")
    }
}
impl OrderHost for Host {
    fn async_call(
        &mut self,
        member: &'static str,
        args: Vec<V>,
    ) -> Result<Pin<Box<dyn Future<Output = Result<V, E>>>>, E> {
        self.effect(member, args.clone())?;
        let result = match member {
            "initExternal" => V::Bool(self.input.get("external").truthy()),
            "fetchMarkets" => arr(vec![obj(vec![("id", txt("loaded"))])]),
            "fetchTickersRaw" => obj(vec![("raw", V::Num(1.0))]),
            "startPrivate" | "startPublicOnly" | "cancelOrders" => V::Undefined,
            "placeOrder" => arr(vec![txt("new")]),
            "formatCreate" => {
                if matches!(self.input.get("payloads"), V::Undefined) {
                    arr(vec![obj(vec![("n", V::Num(1.0))])])
                } else {
                    self.input.get("payloads")
                }
            }
            "put" => {
                let n = {
                    let mut s = self.state.borrow_mut();
                    let n = s.put;
                    s.put += 1;
                    n
                };
                self.input
                    .get("replies")
                    .rows()
                    .get(n)
                    .cloned()
                    .unwrap_or(obj(vec![(
                        "data",
                        obj(vec![
                            ("code", V::Num(0.0)),
                            ("data", obj(vec![("orderID", txt("a"))])),
                        ]),
                    )]))
            }
            "market" => {
                if self.input.get("missingMarket").truthy() {
                    V::Undefined
                } else if matches!(self.input.get("market"), V::Undefined) {
                    market()
                } else {
                    self.input.get("market")
                }
            }
            "positionSide" => txt("Merged"),
            "positions" => {
                if matches!(self.input.get("positions"), V::Undefined) {
                    arr(vec![])
                } else {
                    self.input.get("positions")
                }
            }
            "formatTrailing" => arr(vec![obj(vec![("trail", V::Bool(true))])]),
            _ => panic!("unknown asynchronous member {member}"),
        };
        let wait = if member == "fetchMarkets" && self.input.get("reverse").truthy() {
            2
        } else {
            1
        };
        Ok(Box::pin(Read {
            host: self.clone(),
            member,
            result: Some(result),
            wait,
            settled: false,
        }))
    }
    fn error(&mut self, message: V) -> Result<E, E> {
        self.effect("newError", vec![message])?;
        Ok(E { original: false })
    }
    fn has(&mut self, v: &V, k: &'static str) -> Result<bool, E> {
        if let V::Object(o) = v {
            Ok(o.borrow().iter().any(|(key, _)| key == &u(k)))
        } else {
            Err(E { original: false })
        }
    }
    fn method_text(&mut self, v: V) -> Result<Vec<u16>, E> {
        if let V::Text(t) = v {
            Ok(t)
        } else {
            Err(E { original: false })
        }
    }
    fn table(&mut self, name: &'static str, key: V, inverse: bool) -> Result<V, E> {
        let entries = match name {
            "side" => vec![("BUY", "buy"), ("SELL", "sell")],
            "type" => safe_cex_venue_phemex::constants::ORDER_TYPE.to_vec(),
            "timeInForce" => vec![
                ("GoodTillCancel", "GoodTillCancel"),
                ("ImmediateOrCancel", "ImmediateOrCancel"),
                ("FillOrKill", "FillOrKill"),
                ("PostOnly", "PostOnly"),
            ],
            _ => panic!(),
        };
        let k = key.text();
        Ok(entries
            .iter()
            .find(|(a, b)| u(if inverse { b } else { a }) == k)
            .map(|(a, b)| txt(if inverse { a } else { b }))
            .unwrap_or(V::Undefined))
    }
    fn times_count(&mut self, n: f64) -> Result<usize, E> {
        Ok(if !n.is_finite() || n < 1.0 || n > 9007199254740991.0 {
            0
        } else {
            n.trunc() as usize
        })
    }
    fn base_construct(&mut self, name: V, opts: V, store: V) -> Result<(), E> {
        assert!(
            eq(&store, &self.store_value()),
            "ORDERS native constructor store identity"
        );
        self.fields().set(&u("opts"), opts.clone());
        self.effect("base", vec![name, opts, store])
    }
    fn initialize(&mut self) -> Result<ExchangeState<V>, E> {
        let state = self.defaults()?;
        self.fields()
            .set(&u("timeframes"), state.timeframes.clone());
        Ok(state)
    }
    fn assign(&mut self, field: &'static str, value: V) -> Result<(), E> {
        self.fields().set(&u(field), value);
        Ok(())
    }
    fn secondary_xhr(&mut self, opts: V, max_rps: f64) -> Result<V, E> {
        self.effect("secondary", vec![opts.clone()])?;
        let api = self.call("createAPI", vec![opts])?;
        self.effect(
            "rateLimit",
            vec![api.clone(), obj(vec![("maxRPS", V::Num(max_rps))])],
        )?;
        Ok(txt(&format!("limited:{}", ascii(&api))))
    }
}
fn complete<T>(host: &mut Host, future: impl Future<Output = Result<T, E>>) -> Result<T, E> {
    let mut task = Box::pin(future);
    let w = Waker::noop().clone();
    let mut result = None;
    for _ in 0..100 {
        if result.is_none() {
            if let Poll::Ready(v) = task.as_mut().poll(&mut Context::from_waker(&w)) {
                result = Some(v)
            }
        }
        host.drive()?;
        if host.state.borrow().tasks.is_empty() {
            if let Some(v) = result.take() {
                return v;
            }
        }
    }
    panic!("ORDERS bounded continuation")
}
fn state_result(host: &mut Host, s: ExchangeState<V>) -> Result<V, E> {
    let rule = s.client_order_id_rule;
    let valids = arr([
        "",
        "x",
        &"x".repeat(40),
        &"x".repeat(41),
        "a\nb",
        "\u{d7ff}",
    ]
    .iter()
    .enumerate()
    .map(|(i, s)| {
        V::Bool(if i == 5 {
            valid(&[0xd800])
        } else {
            valid(&u(s))
        })
    })
    .collect());
    let id = host.call("uuid", vec![])?;
    let mint = V::Text(u("Tealstreet_").into_iter().chain(id.text()).collect());
    Ok(obj(vec![
        ("xhr", s.xhr.unwrap_or(V::Undefined)),
        ("unlimited", s.unlimited_xhr.unwrap_or(V::Undefined)),
        ("public", s.public_websocket.unwrap_or(V::Undefined)),
        ("private", s.private_websocket.unwrap_or(V::Undefined)),
        ("periodic", s.grafana_endpoints_periodic),
        ("every", s.grafana_endpoints_every),
        ("timeframes", s.timeframes),
        (
            "rule",
            obj(vec![
                ("label", rule.get("label")),
                ("requirement", rule.get("requirement")),
                ("valids", valids),
                ("mint", mint),
            ]),
        ),
    ]))
}
fn run(name: &str, input: V) -> (Host, V) {
    let mut h = Host::new(input.clone());
    let result = (|| -> Result<V, E> {
        let mut actor = h.clone();
        match name {
            "class" => {
                let s = ExchangeState::initialize(&mut actor)?;
                state_result(&mut actor, s)
            }
            "constructor" => {
                let opts = if matches!(input.get("opts"), V::Undefined) {
                    obj(vec![("key", txt("key"))])
                } else {
                    input.get("opts")
                };
                let store = h.store_value();
                let state = order::new_exchange(&mut actor, txt("phemex"), opts, store)?;
                state_result(&mut actor, state)
            }
            "start" => {
                complete(&mut h, order::start(&mut actor))?;
                Ok(V::Undefined)
            }
            "update" => {
                let args = obj(vec![
                    ("order", input.get("order")),
                    ("update", input.get("update")),
                ]);
                complete(&mut h, order::update_order(&mut actor, args))
            }
            "place" => complete(&mut h, order::place_order(&mut actor, input.get("opts"))),
            "format" => complete(
                &mut h,
                order::format_create_order(&mut actor, input.get("opts")),
            ),
            "trailing" => complete(
                &mut h,
                order::format_create_trailing_stop_loss_order(&mut actor, input.get("opts")),
            ),
            _ => panic!(),
        }
    })();
    let (result, thrown) = match result {
        Ok(v) => (v, V::Undefined),
        Err(e) => (
            V::Undefined,
            txt(if e.original { "original" } else { "intrinsic" }),
        ),
    };
    let observation = obj(vec![
        ("effects", arr(h.state.borrow().effects.clone())),
        ("result", result),
        ("thrown", thrown),
        ("store", h.store_value()),
    ]);
    (h, observation)
}
fn oracle(name: &str) -> Vec<Value> {
    let output = Command::new("node")
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/start-order-oracle.cjs"))
        .arg(name)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn verify(name: &str, floor: usize) {
    let cases = oracle(name);
    assert_eq!(cases.len(), floor, "ORDERS actual-source case floor");
    for (i, case) in cases.iter().enumerate() {
        let (h, result) = run(name, V::decode(&case["input"]));
        let native = result.obs();
        let expected = &case["expected"];
        let effects = |v: &Value| -> Vec<Value> {
            v["object"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry[0] == json!({"units":u("effects")}))
                .unwrap()[1]
                .as_array()
                .unwrap()
                .clone()
        };
        let a = effects(&native);
        let b = effects(expected);
        assert_eq!(a.len(), b.len(), "ORDERS native effect count {name} {i}");
        for (a, b) in a.iter().zip(b) {
            assert_eq!(a, &b, "ORDERS native individual effect {name} {i}")
        }
        assert_eq!(
            &native, expected,
            "ORDERS native complete observation {name} {i}"
        );
        if ["start", "update", "place", "format", "trailing"].contains(&name)
            && h.state
                .borrow()
                .effects
                .iter()
                .any(|v| eq(&v.rows()[0], &txt("settle")))
        {
            assert!(h.pending.get() > 0, "ORDERS native actual Pending")
        }
    }
}
#[test]
fn actual_typescript_class() {
    verify("class", 1)
}
#[test]
fn actual_typescript_constructor() {
    verify("constructor", 9)
}
#[test]
fn actual_typescript_start() {
    verify("start", 17)
}
#[test]
fn actual_typescript_update() {
    verify("update", 12)
}
#[test]
fn actual_typescript_place() {
    verify("place", 14)
}
#[test]
fn actual_typescript_format() {
    verify("format", 22)
}
#[test]
fn actual_typescript_trailing() {
    verify("trailing", 9)
}
