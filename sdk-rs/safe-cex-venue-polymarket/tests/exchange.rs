use safe_cex_venue_polymarket::{
    account_reads::*, auth_init::*, exchange::*, order_adapters::Completion, ws_dispatch::Runtime,
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
    Function(Option<u32>, String, Rc<()>),
    Bytes(Vec<u8>),
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
        "function" => V::Function(
            j["id"].as_u64().map(|x| x as u32),
            j["name"].as_str().unwrap().into(),
            Rc::new(()),
        ),
        "bytes" => V::Bytes(
            j["bytes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as u8)
                .collect(),
        ),
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
        V::Function(id, name, _) => json!({"kind":"function","id":id,"name":name}),
        V::Bytes(bytes) => json!({"kind":"bytes","bytes":bytes}),
        V::U => json!({"kind":"undefined"}),
        V::Null => json!({"kind":"null"}),
        V::Bool(b) => json!({"kind":"bool","value":b}),
        V::Number(n) => {
            json!({"kind":"number","bits":format!("{:016x}",if n.is_nan(){0x7ff8000000000000}else{n.to_bits()})})
        }
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
        (V::Function(_, _, a), V::Function(_, _, b)) => Rc::ptr_eq(a, b),
        (V::Object(_, a), V::Object(_, b)) => Rc::ptr_eq(a, b),
        (V::Array(_, a), V::Array(_, b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
type Callback = Rc<dyn Fn() -> Result<V, E>>;
type AsyncBody = Rc<dyn Fn(Vec<V>) -> PinTask<V, E>>;
#[derive(Clone)]
struct Call {
    namespace: String,
    method: String,
    receiver: Option<V>,
    function: Option<V>,
}
struct Op(Result<V, E>);
impl Completion<V, E> for Op {
    fn poll(&self, _: &mut Context<'_>) -> Poll<Result<V, E>> {
        Poll::Ready(self.0.clone())
    }
}
struct Getter {
    object: V,
    key: String,
    value: V,
    enumerable: bool,
    configurable: bool,
}
struct Host {
    values: V,
    receiver: V,
    tasks: Rc<RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>>,
    events: Rc<RefCell<Vec<(String, Vec<V>)>>>,
    logs: Rc<RefCell<Vec<(String, String)>>>,
    schedules: Rc<RefCell<Vec<(Rc<dyn Fn()>, f64)>>>,
    callbacks: Rc<RefCell<Vec<Callback>>>,
    async_bodies: Rc<RefCell<Vec<AsyncBody>>>,
    definitions: Rc<RefCell<Vec<String>>>,
    prototype: Rc<RefCell<Vec<String>>>,
    class_prototypes: Rc<RefCell<Vec<(String, V)>>>,
    instance_prototypes: Rc<RefCell<Vec<(V, V)>>>,
    class_count: Rc<Cell<usize>>,
    base_prototype: V,
    getters: Rc<RefCell<Vec<Getter>>>,
    constructors: Rc<RefCell<Vec<(String, Vec<V>)>>>,
    services: Rc<RefCell<Vec<(String, V)>>>,
    sync_bodies: Rc<RefCell<Vec<completion::SyncBody>>>,
    clock: Rc<Cell<f64>>,
    delays: Rc<RefCell<Vec<f64>>>,
    error: E,
}
impl Host {
    fn value(&self, k: &str) -> V {
        raw_field(&self.values, k)
    }
    fn index(&self) -> usize {
        numeric(&self.value("i")) as usize
    }
    fn fail(&self, s: &str) -> bool {
        self.is_text(&self.value("failure"), s)
    }
    fn drain(&self) {
        for _ in 0..64 {
            let pending = std::mem::take(&mut *self.tasks.borrow_mut());
            if pending.is_empty() {
                return;
            }
            for mut task in pending {
                if task
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
                {
                    self.tasks.borrow_mut().push(task);
                }
            }
        }
        panic!("native executor 64-iteration floor exhausted")
    }
    fn event(&self, n: &str, args: Vec<V>) {
        self.events.borrow_mut().push((n.into(), args));
    }
    fn new(case: &J) -> Rc<Self> {
        let values = object(
            None,
            case["fixture"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), decode(v)))
                .collect(),
        );
        let store = raw_field(&values, "store");
        let receiver = object(
            None,
            vec![
                ("store".into(), store),
                ("api".into(), object(None, vec![])),
                ("wallet".into(), raw_field(&values, "wallet")),
                (
                    "credentials".into(),
                    object(None, vec![("apiKey".into(), text("api-k"))]),
                ),
                ("signerType".into(), V::Number(2.)),
                ("funderAddress".into(), text("funder")),
                ("marketMetadata".into(), object(None, vec![])),
                ("tokenToSymbol".into(), object(None, vec![])),
                (
                    "symbolToToken".into(),
                    object(None, vec![("event-Yes".into(), text("17"))]),
                ),
                ("costBasis".into(), object(None, vec![])),
            ],
        );
        let clock = numeric(&raw_field(&values, "now"));
        Rc::new(Self {
            values,
            receiver,
            tasks: Rc::default(),
            events: Rc::default(),
            logs: Rc::default(),
            schedules: Rc::default(),
            callbacks: Rc::default(),
            async_bodies: Rc::default(),
            definitions: Rc::default(),
            prototype: Rc::default(),
            class_prototypes: Rc::default(),
            instance_prototypes: Rc::default(),
            class_count: Rc::default(),
            getters: Rc::default(),
            base_prototype: object(
                None,
                vec![("inheritedMarker".into(), text("base-prototype-marker"))],
            ),
            constructors: Rc::default(),
            services: Rc::default(),
            sync_bodies: Rc::default(),
            clock: Rc::new(Cell::new(clock)),
            delays: Rc::default(),
            error: Rc::new(Failure {
                marker: Some(47),
                message: text("bound failure"),
            }),
        })
    }
    fn fork(&self, r: V) -> Rc<Self> {
        Rc::new(Self {
            receiver: r,
            values: self.values.clone(),
            tasks: self.tasks.clone(),
            events: self.events.clone(),
            logs: self.logs.clone(),
            schedules: self.schedules.clone(),
            callbacks: self.callbacks.clone(),
            async_bodies: self.async_bodies.clone(),
            definitions: self.definitions.clone(),
            prototype: self.prototype.clone(),
            class_prototypes: self.class_prototypes.clone(),
            instance_prototypes: self.instance_prototypes.clone(),
            class_count: self.class_count.clone(),
            base_prototype: self.base_prototype.clone(),
            getters: self.getters.clone(),
            constructors: self.constructors.clone(),
            services: self.services.clone(),
            sync_bodies: self.sync_bodies.clone(),
            clock: self.clock.clone(),
            delays: self.delays.clone(),
            error: self.error.clone(),
        })
    }
    fn function(&self, c: Callback) -> V {
        let index = self.callbacks.borrow().len();
        self.callbacks.borrow_mut().push(c);
        V::Function(Some(index as u32), "binding".into(), Rc::new(()))
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
            V::Text(t) => !t.is_empty(),
            _ => true,
        }
    }
    fn is_text(&self, v: &V, t: &str) -> bool {
        matches!(v,V::Text(s)if*s==units(t))
    }
    fn field(&self, v: &V, k: &str) -> Result<V, E> {
        if let Some(getter) = self
            .getters
            .borrow()
            .iter()
            .find(|g| same(&g.object, v) && g.key == k)
        {
            if let V::Function(Some(id), _, _) = getter.value {
                return (self.callbacks.borrow()[id as usize])();
            }
        }

        if self.nullish(v) {
            Err(self.new_error("nullish field"))
        } else {
            let own = if let V::Object(_, fields) = v {
                fields.borrow().iter().any(|(key, _)| key == k)
            } else {
                false
            };
            if own {
                return Ok(raw_field(v, k));
            }
            if let Some((_, prototype)) = self
                .instance_prototypes
                .borrow()
                .iter()
                .find(|(object, _)| same(object, v))
            {
                return self.field(prototype, k);
            }
            if self
                .class_prototypes
                .borrow()
                .iter()
                .any(|(_, prototype)| same(prototype, v))
            {
                return self.field(&self.base_prototype, k);
            }
            Ok(raw_field(v, k))
        }
    }
    fn string(&self, v: &V) -> Result<Vec<u16>, E> {
        Ok(match v {
            V::Text(t) => t.clone(),
            V::U => units("undefined"),
            V::Null => units("null"),
            V::Bool(b) => units(if *b { "true" } else { "false" }),
            V::Number(n) => units(&safe_cex_num::js_number_string(*n)),
            V::Object(_, _) if matches!(raw_field(v, "marker"), V::Number(_)) => {
                let mut s = units("Error: ");
                s.extend(self.string(&raw_field(v, "message"))?);
                s
            }
            _ => units("[object Object]"),
        })
    }
    fn log(&self, m: &[u16], s: &'static str) -> Result<(), E> {
        self.logs
            .borrow_mut()
            .push((String::from_utf16_lossy(m), s.into()));
        Ok(())
    }
}
impl AccountHost for Host {
    type Callable = Call;
    type Operation = Op;
    fn start_task(&self, mut task: Pin<Box<dyn Future<Output = ()>>>) {
        if task
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
        {
            self.tasks.borrow_mut().push(task);
        }
    }
    fn capture(&self, n: &'static str, m: &'static str) -> Result<Call, E> {
        Ok(Call {
            namespace: n.into(),
            method: m.into(),
            receiver: None,
            function: None,
        })
    }
    fn invoke(&self, c: Call, args: Vec<V>) -> Result<Rc<Op>, E> {
        if let Some(result) = completion::invoke(self, &c, &args) {
            return result;
        }
        let n = format!("{}.{}", c.namespace, c.method);
        self.event(&n, args.clone());
        let answer = match n.as_str() {
            "wallet.getAddress" => Ok(raw_field(&raw_field(&self.receiver, "wallet"), "address")),
            "api.getGammaMarkets" => {
                if self.fail("read") {
                    Err(self.error.clone())
                } else {
                    Ok(self.value("gamma"))
                }
            }
            "api.getMidpoints" => {
                if self.fail("read") {
                    Err(self.error.clone())
                } else {
                    let out = object(None, vec![]);
                    for i in 0..self.length(&args[0])? {
                        let id = self.at(&args[0], i)?.unwrap();
                        field_set(
                            &out,
                            &String::from_utf16_lossy(&self.string(&id)?),
                            V::Number(numeric(&id) / 1000.),
                        );
                    }
                    Ok(out)
                }
            }
            "imports.createSignedOrder" => {
                if self.fail("sign") {
                    Err(self.error.clone())
                } else {
                    Ok(self.value("signed"))
                }
            }
            "api.postOrder" => Ok(self.value("response")),
            "api.cancelOrder" => {
                if self.fail("read") {
                    Err(self.error.clone())
                } else {
                    if self.index() == 3 {
                        field_set(&self.value("update"), "amount", V::Number(9.123));
                    }
                    Ok(V::U)
                }
            }
            "exchange.placeOrder" => Ok(self.value("replacement")),
            "api.getPriceHistory" => {
                if self.fail("read") {
                    Err(self.error.clone())
                } else {
                    Ok(self.value("history"))
                }
            }
            "exchange.fetchOHLCV" => {
                if self.fail("poll") {
                    Err(self.error.clone())
                } else {
                    Ok(self.value("candles"))
                }
            }
            _ => panic!("bound native async call {n}"),
        };
        Ok(Rc::new(Op(answer)))
    }
    fn invoke_log(&self, _: Call, m: &[u16], s: &'static str) -> Result<(), E> {
        self.log(m, s)
    }
    fn resolve(&self, v: V) -> Result<Rc<Op>, E> {
        Ok(Rc::new(Op(Ok(v))))
    }
    fn credentials(&self) -> Result<V, E> {
        self.exchange_credentials()
    }
    fn wallet(&self) -> Result<V, E> {
        self.field(&self.receiver, "wallet")
    }
    fn exchange_credentials(&self) -> Result<V, E> {
        self.field(&self.receiver, "credentials")
    }
    fn signer_type(&self) -> Result<V, E> {
        self.field(&self.receiver, "signerType")
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
    fn assign(&self, v: &V, k: &'static str, x: V) -> Result<(), E> {
        if k == "credentials" {
            let V::Object(_, fields) = v else {
                panic!("native receiver")
            };
            if !fields.borrow().iter().any(|(key, _)| key == k) {
                return Err(self.new_error("inherited credentials setter must be shadowed"));
            }
        }
        field_set(v, k, x);
        Ok(())
    }
    fn spread_params(&self, _: &V, _: V, _: V) -> Result<V, E> {
        unreachable!()
    }
    fn array(&self, v: Vec<Option<V>>) -> V {
        list(None, v)
    }
    fn map_output(&self, _: &V, n: usize) -> Result<V, E> {
        Ok(list(None, vec![None; n]))
    }
    fn map_set(&self, out: &V, i: usize, v: V) -> Result<(), E> {
        let V::Array(_, a) = out else {
            panic!("native map output")
        };
        a.borrow_mut()[i] = Some(v);
        Ok(())
    }
    fn is_array(&self, v: &V) -> bool {
        matches!(v, V::Array(..))
    }
    fn length(&self, v: &V) -> Result<usize, E> {
        let V::Array(_, a) = v else {
            return Err(self.new_error("non array"));
        };
        Ok(a.borrow().len())
    }
    fn at(&self, v: &V, i: usize) -> Result<Option<V>, E> {
        let V::Array(_, a) = v else {
            return Err(self.new_error("non array"));
        };
        Ok(a.borrow().get(i).cloned().flatten())
    }
    fn push_spread(&self, out: &V, iter: &V) -> Result<(), E> {
        let V::Array(_, a) = out else {
            panic!("native push")
        };
        for i in 0..self.length(iter)? {
            a.borrow_mut().push(Some(self.at(iter, i)?.unwrap_or(V::U)));
        }
        Ok(())
    }
    fn less(&self, l: &V, r: &V) -> Result<bool, E> {
        Ok(numeric(l) < numeric(r))
    }
    fn greater_equal(&self, l: &V, r: &V) -> Result<bool, E> {
        Ok(numeric(l) >= numeric(r))
    }
    fn add(&self, l: &V, r: &V) -> Result<V, E> {
        Ok(V::Number(numeric(l) + numeric(r)))
    }
    fn slice(&self, _: &V, _: &V) -> Result<V, E> {
        unreachable!()
    }
    fn order_to_json(&self, _: V, _: V, _: V) -> Result<V, E> {
        unreachable!()
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
impl AuthHost for Host {
    type Constructor = String;
    fn capture_constructor(&self, n: &'static str) -> Result<String, E> {
        Ok(n.into())
    }
    fn construct(&self, n: String, args: Vec<V>) -> Result<V, E> {
        if let Some(result) = completion::construct(self, &n, &args) {
            return result;
        }
        self.constructors
            .borrow_mut()
            .push((n.clone(), args.clone()));
        if let Some((_, prototype)) = self
            .class_prototypes
            .borrow()
            .iter()
            .find(|(name, _)| *name == n)
        {
            let receiver = object(
                None,
                vec![
                    ("store".into(), args[2].clone()),
                    ("isDisposed".into(), self.value("disposed")),
                ],
            );
            self.instance_prototypes
                .borrow_mut()
                .push((receiver.clone(), prototype.clone()));
            return Ok(receiver);
        }
        match n.as_str() {
            "BaseExchange" => Ok(object(
                None,
                vec![
                    ("store".into(), args[2].clone()),
                    ("isDisposed".into(), self.value("disposed")),
                ],
            )),
            "Map" => Ok(object(None, vec![])),
            "PolymarketAPI" => {
                self.callbacks.borrow_mut();
                Ok(object(None, vec![]))
            }
            "PolymarketPublicWebSocket" | "PolymarketPrivateWebSocket" => Ok(object(None, vec![])),
            _ => panic!("constructor {n}"),
        }
    }
    fn capture_value(&self, r: V, m: &'static str) -> Result<Call, E> {
        let is_instance = self
            .instance_prototypes
            .borrow()
            .iter()
            .any(|(instance, _)| same(instance, &r));
        let function = if is_instance {
            let function = self.field(&r, m)?;
            if !matches!(function, V::Function(..)) {
                return Err(self.new_error("missing composed callable"));
            }
            Some(function)
        } else {
            None
        };
        Ok(Call {
            namespace: "value".into(),
            method: m.into(),
            receiver: Some(r),
            function,
        })
    }
    fn call_sync(&self, c: Call, args: Vec<V>) -> Result<V, E> {
        if let Some(result) = completion::sync(self, &c, &args) {
            return result;
        }
        if let Some(r) = c.receiver {
            match c.method.as_str() {
                "set" => {
                    let key = String::from_utf16_lossy(&self.string(&args[0])?);
                    field_set(&r, &key, args[1].clone());
                    return Ok(r);
                }
                "get" => return self.field(&r, &String::from_utf16_lossy(&self.string(&args[0])?)),
                _ => panic!("map intrinsic"),
            }
        }
        let n = format!("{}.{}", c.namespace, c.method);
        match n.as_str() {
            "imports.parseFloat" => Ok(V::Number(safe_cex_num::parse_float(
                &String::from_utf16_lossy(&self.string(&args[0])?),
            ))),
            "exchange.resolveCallerClientOrderId" | "exchange.emitAndLogError" => {
                self.event(&n, args);
                Ok(V::U)
            }
            "imports.createWallet" => {
                self.event(&n, args);
                if self.fail("wallet") {
                    Err(self.error.clone())
                } else {
                    Ok(self.value("wallet"))
                }
            }
            _ => panic!("bound native sync call {n}"),
        }
    }
    fn clob_auth_types(&self) -> Result<V, E> {
        unreachable!()
    }
    fn signer(&self, _: WalletSigner<Self>) -> Result<V, E> {
        unreachable!()
    }
    fn sdk_side(&self, b: bool) -> V {
        text(if b { "BUY" } else { "SELL" })
    }
    fn string_value(&self, t: Vec<u16>) -> V {
        V::Text(t)
    }
    fn uppercase(&self, v: V) -> Result<V, E> {
        Ok(text(
            &String::from_utf16_lossy(&self.string(&v)?).to_uppercase(),
        ))
    }
    fn replace_ascii(&self, _: V, _: u16, _: u16) -> Result<V, E> {
        unreachable!()
    }
    fn is_undefined(&self, v: &V) -> bool {
        matches!(v, V::U)
    }
    fn date_now(&self) -> Result<f64, E> {
        Ok(self.clock.get())
    }
    fn retry_delay(&self, callback: Rc<dyn Fn(V) -> Result<V, E>>) -> Result<V, E> {
        Ok(
            safe_cex_venue_polymarket::api_factory::FactoryHost::function(
                self,
                Rc::new(move |args| callback(args.first().cloned().unwrap_or(V::U))),
            ),
        )
    }
    fn multiply(&self, l: V, r: V) -> Result<V, E> {
        Ok(V::Number(numeric(&l) * numeric(&r)))
    }
}
impl ExchangeHost for Host {
    fn bind_receiver(&self, r: V) -> Result<Rc<Self>, E> {
        Ok(self.fork(r))
    }
    fn derive(&self, b: String, members: Vec<(&'static str, V)>) -> Result<String, E> {
        assert_eq!(b, "BaseExchange", "native heritage constructor");
        self.class_count.set(self.class_count.get() + 1);
        *self.prototype.borrow_mut() = members.iter().map(|(name, _)| (*name).into()).collect();
        let name = format!("PolymarketExchangeClass{}", self.class_count.get());
        let mut fields = vec![(
            "constructor".into(),
            V::Function(None, format!("class-constructor:{name}"), Rc::new(())),
        )];
        let mut accessor = None;
        for (key, value) in members {
            if key == "name" {
                accessor = Some(value);
                fields.push((key.into(), V::U));
            } else {
                fields.push((key.into(), value));
            }
        }
        let prototype = object(None, fields);
        if let Some(value) = accessor {
            self.getters.borrow_mut().push(Getter {
                object: prototype.clone(),
                key: "name".into(),
                value,
                enumerable: false,
                configurable: true,
            });
        }
        self.class_prototypes
            .borrow_mut()
            .push((name.clone(), prototype));
        Ok(name)
    }
    fn prototype_method(&self, n: &'static str) -> Result<V, E> {
        if n == "name" {
            return Ok(self.function(Rc::new(|| Ok(text("polymarket")))));
        }
        if n == "getPositionMode" {
            return Ok(self.async_method(Rc::new(|_| {
                Box::pin(async {
                    let mode = safe_cex_venue_polymarket::spot_contracts::get_position_mode().await;
                    assert_eq!(mode, safe_cex_types::PositionMode::OneWay);
                    Ok(text("oneway"))
                })
            })));
        }
        Ok(V::Function(None, n.into(), Rc::new(())))
    }
    fn property(&self, o: &V, key: V) -> Result<V, E> {
        self.field(o, &String::from_utf16_lossy(&self.string(&key)?))
    }
    type Iterator = (V, usize);
    fn iterate(&self, v: V) -> Result<Self::Iterator, E> {
        Ok((v, 0))
    }
    fn next(&self, it: &mut Self::Iterator) -> Result<Option<V>, E> {
        if it.1 >= self.length(&it.0)? {
            return Ok(None);
        }
        let v = self.at(&it.0, it.1)?.unwrap_or(V::U);
        it.1 += 1;
        Ok(Some(v))
    }
    fn spread(&self, v: &V) -> Result<V, E> {
        let V::Object(_, fields) = v else {
            panic!("native spread")
        };
        Ok(object(None, fields.borrow().clone()))
    }
    fn greater(&self, l: &V, r: &V) -> Result<bool, E> {
        Ok(numeric(l) > numeric(r))
    }
    fn receiver(&self) -> V {
        self.receiver.clone()
    }
    fn define(&self, o: &V, k: &'static str, v: V) -> Result<(), E> {
        self.definitions.borrow_mut().push(k.into());
        field_set(o, k, v);
        Ok(())
    }
    fn method(&self, receiver: &V, n: &'static str) -> Result<V, E> {
        if n == "getAccount" {
            let h = self.fork(receiver.clone());
            return Ok(self.async_method(Rc::new(move |_| Box::pin(get_account(h.clone())))));
        }
        Ok(V::Function(None, n.into(), Rc::new(())))
    }
    fn async_method(&self, body: AsyncBody) -> V {
        let i = self.async_bodies.borrow().len();
        self.async_bodies.borrow_mut().push(body);
        V::Function(Some(i as u32), "async".into(), Rc::new(()))
    }
    fn callback(&self, c: Callback) -> V {
        self.function(c)
    }
    fn boolean(&self, b: bool) -> V {
        V::Bool(b)
    }
    fn strict_true(&self, v: &V) -> bool {
        matches!(v, V::Bool(true))
    }
    fn strict_false(&self, v: &V) -> bool {
        matches!(v, V::Bool(false))
    }
    fn finite(&self, v: &V) -> bool {
        matches!(v,V::Number(n)if n.is_finite())
    }
    fn to_number(&self, v: &V) -> Result<f64, E> {
        Ok(numeric(v))
    }
    fn fixed(&self, v: &V, d: &V) -> Result<V, E> {
        safe_cex_num::js_to_fixed(numeric(v), numeric(d))
            .map(|s| text(&s))
            .map_err(|m| self.new_error(m))
    }
    fn adjust(&self, v: V, s: V) -> Result<V, E> {
        Ok(V::Number(safe_cex_num::adjust(
            numeric(&v),
            String::from_utf16_lossy(&self.string(&s)?),
        )))
    }
    fn error_value(&self, e: &E) -> V {
        object(
            None,
            vec![
                (
                    "marker".into(),
                    e.marker.map_or(V::Null, |m| V::Number(m as f64)),
                ),
                ("message".into(), e.message.clone()),
            ],
        )
    }
    fn error_from_value(&self, m: V) -> E {
        Rc::new(Failure {
            marker: None,
            message: m,
        })
    }
    fn error_is_instance(&self, _: &E) -> bool {
        true
    }
    fn has(&self, o: &V, k: &'static str) -> Result<bool, E> {
        let V::Object(_, f) = o else {
            return Err(self.new_error("non object in"));
        };
        Ok(f.borrow().iter().any(|(key, _)| key == k))
    }
    fn entries(&self, v: &V) -> Result<Vec<(V, V)>, E> {
        let V::Object(_, f) = v else {
            panic!("native entries")
        };
        let mut entries = f.borrow().clone();
        entries.sort_by_key(|(k, _)| {
            k.parse::<u32>()
                .ok()
                .filter(|x| x.to_string() == *k && *x < u32::MAX)
                .map_or((1, 0), |x| (0, x))
        });
        Ok(entries.into_iter().map(|(k, v)| (text(&k), v)).collect())
    }
    fn object_assign(&self, o: &V, v: V) -> Result<(), E> {
        for (k, v) in self.entries(&v)? {
            field_set(o, &String::from_utf16_lossy(&self.string(&k)?), v);
        }
        Ok(())
    }
    fn map_keys(&self, v: &V) -> Result<V, E> {
        let V::Object(_, f) = v else {
            panic!("native Map")
        };
        Ok(list(
            None,
            f.borrow().iter().map(|(k, _)| Some(text(k))).collect(),
        ))
    }
    fn token_pairs(&self, v: V) -> Result<Vec<(V, V)>, E> {
        let ids = raw_field(&v, "clobTokenIds");
        let outcomes = raw_field(&v, "outcomes");
        let string = |v: &V| match v {
            V::Text(t) => Some(String::from_utf16_lossy(t)),
            _ => None,
        };
        let a = string(&ids);
        let b = string(&outcomes);
        Ok(
            safe_cex_venue_polymarket::utilities::parse_gamma_market_tokens(
                a.as_deref(),
                b.as_deref(),
            )
            .into_iter()
            .map(|(a, b)| (text(&a), text(&b)))
            .collect(),
        )
    }
    fn tick_precision(&self, v: V) -> Result<V, E> {
        let input = match v {
            V::Number(n) => Some(safe_cex_num::PrecisionInput::Number(n)),
            V::Text(t) => Some(safe_cex_num::PrecisionInput::String(
                String::from_utf16_lossy(&t),
            )),
            _ => None,
        };
        Ok(V::Number(
            safe_cex_venue_polymarket::utilities::precision_from_tick_size(input.as_ref()),
        ))
    }
    fn normalize_tick(&self, v: V) -> Result<V, E> {
        let input = match v {
            V::Number(n) => Some(safe_cex_num::PrecisionInput::Number(n)),
            V::Text(t) => Some(safe_cex_num::PrecisionInput::String(
                String::from_utf16_lossy(&t),
            )),
            _ => None,
        };
        safe_cex_venue_polymarket::utilities::normalize_tick_size(input.as_ref())
            .map(|s| text(&s))
            .map_err(|m| self.error_from_value(text(&m)))
    }
    fn disposed(&self) -> bool {
        self.strict_true(&self.value("disposed"))
    }
    fn schedule(&self, c: Rc<dyn Fn()>, d: f64) -> Result<(), E> {
        self.schedules.borrow_mut().push((c, d));
        Ok(())
    }
    fn detach(&self, task: PinTask<V, E>) {
        self.start_task(Box::pin(async move {
            let _ = task.await;
        }));
    }
}
fn oracle() -> &'static J {
    static DATA: OnceLock<J> = OnceLock::new();
    DATA.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/exchange_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "source oracle {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let j: J = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            j["cases"].as_array().unwrap().len(),
            128,
            "independent source fixture floor"
        );
        j
    })
}
fn finish(h: &Host, f: impl Future<Output = Result<V, E>>) -> Result<V, E> {
    let mut f = Box::pin(f);
    h.drain();
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("native observer not complete"),
    }
}
fn num(v: &V, want: f64, label: &str) {
    let V::Number(got) = v else {
        panic!("native number {label}")
    };
    assert_eq!(got.to_bits(), want.to_bits(), "native bits {label}")
}
fn txt(v: &V, want: &str, label: &str) {
    let V::Text(got) = v else {
        panic!("native text {label}")
    };
    assert_eq!(*got, units(want), "native text {label}")
}
fn len(h: &Host, v: &V, want: usize, label: &str) {
    assert_eq!(h.length(v).unwrap(), want, "native length {label}")
}
fn run_family(name: &str) {
    let cases = oracle()["cases"].as_array().unwrap();
    for i in 0..16 {
        let case = &cases[cases
            .iter()
            .position(|c| c["name"] == name && c["i"] == i)
            .unwrap()];
        let h = Host::new(case);
        let result = match name {
            "class" | "constructor" => {
                let class = PolymarketExchangeClass::new(h.clone()).unwrap();
                let e = PolymarketExchange::new(
                    &class,
                    text("polymarket"),
                    h.value("options"),
                    h.value("store"),
                )
                .expect("native declarations shadow inherited setter");
                let r = e.receiver();
                assert_eq!(
                    *h.prototype.borrow(),
                    [
                        "name",
                        "getPositionMode",
                        "deriveApiCredentials",
                        "mapOrder"
                    ],
                    "native derived prototype members"
                );
                let V::Object(_, f) = &r else {
                    panic!("native instance")
                };
                assert_eq!(f.borrow().len(), 36, "native independent class own slots");
                let V::Object(_, a) = raw_field(&r, "marketMetadata") else {
                    panic!("native metadata map")
                };
                let V::Object(_, b) = raw_field(&r, "costBasis") else {
                    panic!("native cost basis map")
                };
                assert!(!Rc::ptr_eq(&a, &b), "native independent maps");
                assert!(
                    a.borrow().is_empty() && b.borrow().is_empty(),
                    "native empty maps"
                );
                let initial_wallet = raw_field(&r, "wallet");
                if i == 0 || (name == "constructor" && i == 3) {
                    assert!(
                        matches!(initial_wallet, V::Null),
                        "native constructor missing or failed wallet stays null"
                    );
                } else {
                    assert!(
                        same(&initial_wallet, &h.value("wallet")),
                        "native original initialized wallet alias"
                    );
                }
                let initial_credentials = raw_field(&r, "credentials");
                if i == 1 || (name == "constructor" && i == 4) {
                    assert!(
                        matches!(initial_credentials, V::Null),
                        "native constructor incomplete credentials stay null"
                    );
                } else {
                    txt(
                        &raw_field(&initial_credentials, "apiKey"),
                        "api-k",
                        "constructor API key",
                    );
                    txt(
                        &raw_field(&initial_credentials, "secret"),
                        "secret",
                        "constructor API secret",
                    );
                    txt(
                        &raw_field(&initial_credentials, "passphrase"),
                        "pass",
                        "constructor API passphrase",
                    );
                }
                if i == 1 {
                    assert!(
                        matches!(raw_field(&r, "funderAddress"), V::Null),
                        "native default funder null"
                    );
                } else {
                    txt(
                        &raw_field(&r, "funderAddress"),
                        "funder",
                        "constructor funder",
                    );
                }
                let constructors = h.constructors.borrow();
                let children = constructors
                    .iter()
                    .filter(|(n, _)| {
                        n.starts_with("Polymarket") && !n.starts_with("PolymarketExchange")
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    children.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
                    [
                        "PolymarketAPI",
                        "PolymarketPublicWebSocket",
                        "PolymarketPrivateWebSocket"
                    ],
                    "native child constructor order"
                );
                assert!(
                    same(&children[0].1[0], &h.value("options")),
                    "native original API options alias"
                );
                for args in [&children[0].1[1], &children[1].1[0], &children[2].1[0]] {
                    assert!(
                        same(args, &r),
                        "native original child parent receiver alias"
                    );
                }
                drop(constructors);
                let initial_logs = h
                    .logs
                    .borrow()
                    .iter()
                    .map(|(m, s)| json!([m, s]))
                    .collect::<Vec<_>>();
                let initial_state = [
                    ("wallet", project(&initial_wallet)),
                    ("credentials", project(&initial_credentials)),
                    ("signerType", project(&raw_field(&r, "signerType"))),
                    ("funderAddress", project(&raw_field(&r, "funderAddress"))),
                ];
                let callbacks = h.callbacks.borrow();
                assert_eq!(
                    callbacks.len(),
                    4,
                    "native three credential/address callbacks"
                );
                let later = object(None, vec![("apiKey".into(), text("late"))]);
                field_set(&r, "credentials", later.clone());
                assert!(
                    same(&(callbacks[1])().unwrap(), &later),
                    "native live API credentials"
                );
                assert!(
                    same(&(callbacks[3])().unwrap(), &later),
                    "native live private credentials"
                );
                field_set(
                    &r,
                    "wallet",
                    object(None, vec![("address".into(), text("late-address"))]),
                );
                txt(
                    &(callbacks[2])().unwrap(),
                    "late-address",
                    "live wallet address",
                );
                // Native constructor defaults/options, before all wire projections.
                if i == 1 {
                    num(&raw_field(&r, "signerType"), 0., "constructor default EOA");
                } else if i == 2 {
                    assert!(
                        matches!(raw_field(&r, "signerType"), V::Null),
                        "native explicit null signer"
                    );
                } else if name == "constructor" && i == 5 {
                    num(&raw_field(&r, "signerType"), 0., "undefined signer default");
                } else {
                    num(&raw_field(&r, "signerType"), 2., "constructor signer");
                }
                let own = f
                    .borrow()
                    .iter()
                    .map(|(k, _)| J::String(k.clone()))
                    .collect::<Vec<_>>();
                drop(callbacks);
                if i == 0 {
                    let V::Function(Some(id), _, _) = raw_field(&r, "fetchMarkets") else {
                        panic!("native executable market binding")
                    };
                    let body = h.async_bodies.borrow()[id as usize].clone();
                    let rows = finish(&h, body(vec![])).unwrap();
                    len(&h, &rows, 2, "composed market call");
                    txt(
                        &raw_field(&raw_field(&r, "symbolToToken"), "event-Yes"),
                        "17",
                        "composed instance cache",
                    );
                    let second = PolymarketExchange::new(
                        &class,
                        text("polymarket"),
                        h.value("options"),
                        h.value("store"),
                    )
                    .unwrap();
                    assert_eq!(
                        h.class_count.get(),
                        1,
                        "native shared evaluated class definition"
                    );
                    let prototypes = h.instance_prototypes.borrow();
                    assert_eq!(prototypes.len(), 2, "native two constructed instances");
                    assert!(
                        same(&prototypes[0].1, &prototypes[1].1),
                        "native shared prototype identity"
                    );
                    let prototype = prototypes[0].1.clone();
                    drop(prototypes);
                    txt(
                        &h.field(&r, "inheritedMarker").unwrap(),
                        "base-prototype-marker",
                        "derived prototype inherits BaseExchange",
                    );
                    txt(
                        &h.field(&r, "name").unwrap(),
                        "polymarket",
                        "native derived getter lookup",
                    );
                    txt(
                        &h.field(&prototype, "name").unwrap(),
                        "polymarket",
                        "prototype getter access",
                    );
                    let descriptors = h.getters.borrow();
                    let descriptor = descriptors
                        .iter()
                        .find(|g| same(&g.object, &prototype) && g.key == "name")
                        .unwrap();
                    assert!(
                        !descriptor.enumerable && descriptor.configurable,
                        "native class getter descriptor flags"
                    );
                    drop(descriptors);
                    let V::Function(Some(method), _, _) = raw_field(&prototype, "getPositionMode")
                    else {
                        panic!("native executable position mode prototype")
                    };
                    let body = h.async_bodies.borrow()[method as usize].clone();
                    let mode = finish(&h, body(vec![])).unwrap();
                    txt(&mode, "oneway", "prototype position mode body");
                    field_set(
                        &second.receiver(),
                        "wallet",
                        object(None, vec![("address".into(), text("other-instance"))]),
                    );
                    let V::Function(Some(id), _, _) = raw_field(&r, "getAccount") else {
                        panic!("native executable previous body binding")
                    };
                    let body = h.async_bodies.borrow()[id as usize].clone();
                    let account = finish(&h, body(vec![])).unwrap();
                    txt(
                        &raw_field(&account, "userId"),
                        "late-address",
                        "original instance retained after second constructor",
                    );
                    assert_eq!(
                        case["classProtocol"]["name"], "polymarket",
                        "source composed getter"
                    );
                    assert_eq!(
                        case["classProtocol"]["mode"], "oneway",
                        "source composed prototype method"
                    );
                    assert_eq!(
                        case["classProtocol"]["markets"], 2,
                        "source composed market call"
                    );
                    assert_eq!(
                        case["classProtocol"]["account"], "late-address",
                        "source composed account receiver"
                    );
                }
                assert_eq!(
                    own,
                    case["state"]["keys"].as_array().unwrap().clone(),
                    "source class own slots after native checks"
                );
                for (key, value) in initial_state {
                    assert_eq!(
                        value, case["state"][key],
                        "source initial constructor state {key}"
                    );
                }
                assert_eq!(
                    initial_logs,
                    case["logs"].as_array().unwrap().clone(),
                    "source initial constructor logging"
                );
                Ok(V::U)
            }
            "markets" => {
                let result = finish(&h, fetch_markets(h.clone()));
                let v = result.as_ref().unwrap();
                let count = match i {
                    6 => 1,
                    7 | 9 | 14 => 0,
                    8 => 4,
                    _ => 2,
                };
                len(&h, v, count, "outcome market rows");
                if count > 0 {
                    let m = h.at(v, 0).unwrap().unwrap();
                    txt(&raw_field(&m, "quote"), "pUSD", "prediction collateral");
                    num(&raw_field(&m, "contractSize"), 1., "binary contract size");
                    let leverage = raw_field(&raw_field(&m, "limits"), "leverage");
                    num(&raw_field(&leverage, "min"), 1., "leverage minimum");
                    num(&raw_field(&leverage, "max"), 1., "leverage maximum");
                    let info = raw_field(&m, "info");
                    txt(&raw_field(&info, "outcome"), "Yes", "outcome token");
                    txt(&raw_field(&m, "id"), "17", "outcome ID");
                    let metadata = raw_field(&h.receiver, "marketMetadata");
                    assert!(
                        same(
                            &raw_field(&metadata, "cond-1"),
                            &h.at(&h.value("gamma"), 0).unwrap().unwrap()
                        ),
                        "native cached original Gamma alias"
                    );
                }
                result
            }
            "tickers" => {
                let count = match i {
                    0 => 0,
                    1 => 1,
                    2 => 100,
                    3 => 101,
                    4 => 201,
                    14 => 0,
                    _ => 3,
                };
                let n = if i == 14 { 3 } else { count };
                let map = raw_field(&h.receiver, "tokenToSymbol");
                for k in 0..n {
                    field_set(&map, &(17 + k).to_string(), text(&format!("event-{k}")));
                }
                let result = finish(&h, fetch_tickers(h.clone()));
                let v = result.as_ref().unwrap();
                len(&h, v, count, "ticker rows");
                if count > 0 {
                    let row = h.at(v, 0).unwrap().unwrap();
                    for key in ["bid", "ask", "last", "mark", "index"] {
                        num(&raw_field(&row, key), 0.017, key);
                    }
                    num(
                        &raw_field(&row, "fundingRate"),
                        0.,
                        "prediction funding zero",
                    );
                }
                let calls = h
                    .events
                    .borrow()
                    .iter()
                    .filter(|(n, _)| n == "api.getMidpoints")
                    .count();
                assert_eq!(
                    calls,
                    if i == 14 { 1 } else { n.div_ceil(100) },
                    "native independent 100-token batches"
                );
                result
            }
            "place" => {
                let result = finish(&h, place_order(h.clone(), h.value("opts")));
                let events = h.events.borrow();
                assert_eq!(
                    events[0].0, "exchange.resolveCallerClientOrderId",
                    "native caller ID before validation"
                );
                if i < 2 {
                    let call = events
                        .iter()
                        .find(|(n, _)| n == "imports.createSignedOrder")
                        .unwrap();
                    let params = &call.1[1];
                    num(&raw_field(params, "price"), 0.44, "price tick axis");
                    num(&raw_field(params, "size"), 12.346, "amount decimal axis");
                    txt(
                        &raw_field(params, "side"),
                        if i == 0 { "BUY" } else { "SELL" },
                        "signed side",
                    );
                    assert!(
                        matches!(raw_field(params, "negRisk"), V::Bool(true)),
                        "native neg-risk grouping"
                    );
                    txt(&raw_field(params, "tickSize"), "0.005", "signed tick size");
                    len(&h, result.as_ref().unwrap(), 1, "placed IDs");
                } else {
                    assert!(result.is_err(), "native rejected unsupported order");
                    if i < 14 {
                        assert!(
                            !events.iter().any(|(n, _)| n == "api.postOrder"),
                            "native rejected before posting"
                        );
                    }
                    if i == 14 {
                        assert!(
                            Rc::ptr_eq(result.as_ref().unwrap_err(), &h.error),
                            "native original signing error"
                        );
                    }
                }
                result
            }
            "update" => {
                let opts = object(
                    None,
                    vec![
                        ("order".into(), h.value("order")),
                        ("update".into(), h.value("update")),
                    ],
                );
                let result = finish(&h, update_order(h.clone(), opts));
                let events = h.events.borrow();
                assert_eq!(events[0].0, "api.cancelOrder", "native cancellation first");
                txt(&events[0].1[0], "old-order", "cancelled ID");
                if i != 14 {
                    let params = &events[1].1[0];
                    txt(
                        &raw_field(params, "symbol"),
                        "event-Yes",
                        "replacement symbol",
                    );
                    if i == 3 {
                        num(
                            &raw_field(params, "amount"),
                            9.123,
                            "live update after cancellation",
                        );
                    } else if i % 2 == 1 {
                        assert!(
                            matches!(raw_field(params, "amount"), V::U),
                            "native explicit undefined overrides amount"
                        );
                    } else {
                        num(
                            &raw_field(params, "amount"),
                            4.51,
                            "original amount fallback",
                        );
                    }
                    assert!(
                        same(result.as_ref().unwrap(), &h.value("replacement")),
                        "native replacement ID array alias"
                    );
                } else {
                    assert_eq!(
                        events.len(),
                        1,
                        "native cancel failure prevents replacement"
                    );
                }
                result
            }
            "ohlcv" => {
                let result = finish(&h, fetch_ohlcv(h.clone(), h.value("opts")));
                let v = result.as_ref().unwrap();
                let count = if [1, 3, 14].contains(&i) {
                    0
                } else if i == 2 {
                    1
                } else {
                    2
                };
                len(&h, v, count, "OHLCV rows");
                if i == 0 {
                    let c = h.at(v, 0).unwrap().unwrap();
                    num(
                        &raw_field(&c, "timestamp"),
                        1700000000000.,
                        "candle milliseconds",
                    );
                    for key in ["open", "high", "low", "close"] {
                        num(&raw_field(&c, key), 0.42, key);
                    }
                    num(&raw_field(&c, "volume"), 0., "synthetic candle zero volume");
                    let events = h.events.borrow();
                    let args = &events[0].1;
                    txt(&args[0], "17", "history token ID");
                    num(&args[4], 60., "one minute fidelity");
                    assert!(matches!(args[3], V::U), "native history interval undefined");
                }
                result
            }
            "listen" => {
                let emissions = Rc::new(RefCell::new(vec![]));
                let out = emissions.clone();
                let error = h.error.clone();
                let fail = h.fail("callback");
                let append = h.index() == 13;
                let candles = h.value("candles");
                let stop = listen_ohlcv(
                    h.clone(),
                    h.value("opts"),
                    Rc::new(move |v| {
                        out.borrow_mut().push(v);
                        if append && out.borrow().len() == 1 {
                            let V::Array(_, a) = &candles else {
                                panic!("native candles")
                            };
                            a.borrow_mut().push(Some(object(
                                None,
                                vec![
                                    ("timestamp".into(), V::Number(300.)),
                                    ("open".into(), V::Number(0.9)),
                                ],
                            )));
                        }
                        if fail { Err(error.clone()) } else { Ok(()) }
                    }),
                );
                if i == 15 {
                    stop();
                }
                h.drain();
                let expected = if [1, 2, 4].contains(&i) {
                    0
                } else if i == 3 {
                    1
                } else if i == 13 {
                    3
                } else {
                    2
                };
                assert_eq!(
                    emissions.borrow().len(),
                    expected,
                    "native strictly newer candle emissions"
                );
                if expected == 2 {
                    num(
                        &raw_field(&emissions.borrow()[0], "timestamp"),
                        100.,
                        "first candle timestamp",
                    );
                    num(
                        &raw_field(&emissions.borrow()[1], "timestamp"),
                        200.,
                        "second distinct candle timestamp",
                    );
                }
                if i != 1 {
                    num(
                        &h.events.borrow()[0].1[0].clone().pipe_field("from"),
                        1700000000.,
                        "poll lookback seconds",
                    );
                    assert_eq!(
                        h.schedules.borrow().len(),
                        if i == 15 { 0 } else { 1 },
                        "native in-flight cleanup scheduling"
                    );
                    if i != 15 {
                        assert_eq!(h.schedules.borrow()[0].1, 5000., "native polling delay");
                    }
                }
                stop();
                let before = h.events.borrow().len();
                if let Some((timer, _)) = h.schedules.borrow().first() {
                    timer();
                }
                h.drain();
                assert_eq!(
                    h.events.borrow().len(),
                    before,
                    "native cleanup stops queued poll"
                );
                assert_eq!(
                    emissions.borrow().iter().map(project).collect::<Vec<_>>(),
                    case["emissions"].as_array().unwrap().clone(),
                    "source emitted values after native assertions"
                );
                Ok(V::U)
            }
            _ => unreachable!(),
        };
        if name == "class" || name == "constructor" {
            continue;
        }
        // All independent native checks above precede any decoded-source/wire assertion.
        let expected = &case["result"];
        match &result {
            Ok(v) => assert_eq!(project(v), expected["value"], "source result {name}/{i}"),
            Err(e) => {
                assert_eq!(
                    e.marker.map(|x| x as u64),
                    expected["error"]["marker"].as_u64(),
                    "source error identity {name}/{i}"
                );
                assert_eq!(
                    project(&e.message),
                    expected["error"]["message"],
                    "source error message {name}/{i}"
                );
            }
        }
        assert_eq!(
            h.events
                .borrow()
                .iter()
                .map(|(n, a)| json!([n, a.iter().map(project).collect::<Vec<_>>()]))
                .collect::<Vec<_>>(),
            case["events"].as_array().unwrap().clone(),
            "source calls {name}/{i}"
        );
        assert_eq!(
            h.logs
                .borrow()
                .iter()
                .map(|(m, s)| json!([m, s]))
                .collect::<Vec<_>>(),
            case["logs"].as_array().unwrap().clone(),
            "source logs {name}/{i}"
        );
    }
}
trait FieldHelper {
    fn pipe_field(&self, k: &str) -> V;
}
impl FieldHelper for V {
    fn pipe_field(&self, k: &str) -> V {
        raw_field(self, k)
    }
}
#[test]
fn class_native() {
    run_family("class")
}
#[test]
fn constructor_native() {
    run_family("constructor")
}
#[test]
fn markets_native() {
    run_family("markets")
}
#[test]
fn tickers_native() {
    run_family("tickers")
}
#[test]
fn place_native() {
    run_family("place")
}
#[test]
fn update_native() {
    run_family("update")
}
#[test]
fn ohlcv_native() {
    run_family("ohlcv")
}
#[test]
fn listen_native() {
    run_family("listen")
}

#[test]
fn dropped_observers_retain_exchange_work() {
    let case = &oracle()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "markets" && c["i"] == 0)
        .unwrap();
    let h = Host::new(case);
    let observer = fetch_markets(h.clone());
    assert_eq!(h.events.borrow().len(), 1, "native eager request prefix");
    drop(observer);
    h.drain();
    txt(
        &raw_field(&raw_field(&h.receiver, "symbolToToken"), "event-Yes"),
        "17",
        "native dropped market observer retains cache work",
    );
    let case = &oracle()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "place" && c["i"] == 0)
        .unwrap();
    let h = Host::new(case);
    let observer = place_order(h.clone(), h.value("opts"));
    drop(observer);
    h.drain();
    assert!(
        h.events.borrow().iter().any(|(n, _)| n == "api.postOrder"),
        "native dropped placement observer retains posting"
    );
    let h = Host::new(case);
    let opts = object(
        None,
        vec![
            ("order".into(), h.value("order")),
            ("update".into(), h.value("update")),
        ],
    );
    let observer = update_order(h.clone(), opts);
    drop(observer);
    h.drain();
    assert_eq!(
        h.events.borrow().last().unwrap().0,
        "exchange.placeOrder",
        "native dropped replacement observer retains replacement"
    );
}

#[path = "support/completion.rs"]
mod completion;

#[test]
fn source_oracle_guard_rejects_price_mutation() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/exchange_oracle.cjs"
        ))
        .env("POLYMARKET_SOURCE_CONTROL", "price-axis")
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "source guard must reject production source mutation"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("source valid limit order succeeds"),
        "named source price axis assertion missing"
    );
}
