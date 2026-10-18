// These adapters call the compiler-selected production function. They script
// only its declared caller ports; none of the venue algorithms is copied here.
use crate::{
    account_leverage_overlay::LeverageOverlay,
    lifecycle::{
        LifecycleAsync, LifecycleAsyncCall, LifecycleBindings, LifecycleCall, LifecycleRead,
        LifecycleSync, TickResolver, TickTimer,
    },
    request_weight::Failure,
};
use safe_cex_exchange::{js_runtime::value::Js, runtime::ExchangeError};
use safe_cex_types::{ClosedPnlEntry, HistoricalTradeOptions, Market};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

pub fn bits(n: f64) -> String {
    format!("{:016x}", n.to_bits())
}
pub fn run<T>(f: impl Future<Output = T>) -> T {
    let mut f = std::pin::pin!(f);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("ready terminal port unexpectedly pending"),
    }
}
pub fn overlay(
    input: &Value,
    f: impl FnOnce(&[Value]) -> Result<LeverageOverlay, Failure>,
) -> Value {
    let rows = input["rows"].as_array().unwrap();
    match f(rows) {
        Err(e) => json!({"error": e.name, "message": e.message}),
        Ok(overlay) => {
            let markets: Vec<Market> = ["BTCUSDT", "ETHUSDT", "untouched"].into_iter().map(|symbol| {
                serde_json::from_value(json!({"id":symbol,"symbol":symbol,"base":"base","quote":"quote","active":true,
                    "precision":{"amount":0.01,"price":0.1},"limits":{"amount":{"min":0.1,"max":100},"leverage":{"min":1,"max":3}}})).unwrap()
            }).collect();
            let original = markets.clone();
            let out = overlay.apply(markets);
            assert_eq!(out.len(), original.len());
            let maxima = out
                .iter()
                .map(|market| bits(market.limits.leverage.max))
                .collect::<Vec<_>>();
            for (mut market, original) in out.into_iter().zip(original) {
                market.limits.leverage.max = original.limits.leverage.max;
                assert_eq!(market, original, "overlay changes only leverage maximum");
            }
            json!({"maxBits":maxima})
        }
    }
}

pub struct Loaded {
    pub value: Js,
    pub error: Failure,
    pub reads: usize,
    pub throws: bool,
}
impl LifecycleBindings for Loaded {
    fn read(&mut self, key: LifecycleRead) -> Result<Js, Failure> {
        assert_eq!(key, LifecycleRead::Loaded);
        self.reads += 1;
        if self.throws {
            Err(self.error.clone())
        } else {
            Ok(self.value.clone())
        }
    }
    fn capture(&mut self, _: LifecycleSync) -> Result<LifecycleCall, Failure> {
        panic!("unexpected capture")
    }
    fn capture_async(&mut self, _: LifecycleAsync) -> Result<LifecycleAsyncCall, Failure> {
        panic!("unexpected async capture")
    }
    fn now(&mut self) -> Result<f64, Failure> {
        panic!("unexpected clock")
    }
    fn tick_resolver(&mut self) -> TickResolver {
        panic!("unexpected timer")
    }
    fn schedule(&mut self, _: TickTimer, _: Js) -> Result<(), Failure> {
        panic!("unexpected schedule")
    }
}
pub fn loaded(input: &Value, f: impl FnOnce(&mut Loaded, &[&str]) -> Result<Js, Failure>) -> Value {
    let nested = Js::object(vec![("marker", Js::Number(-0.0))]);
    let source = match input["kind"].as_str().unwrap() {
        "null" => Js::Null,
        "string" => Js::str("ab"),
        "array" => Js::array(vec![Js::Undefined, nested.clone()]),
        "number" => Js::Number(f64::NAN),
        _ => Js::object(vec![
            ("orders", Js::Bool(false)),
            ("nested", nested.clone()),
            ("undefined", Js::Undefined),
            ("negativeZero", Js::Number(-0.0)),
            ("nan", Js::Number(f64::NAN)),
        ]),
    };
    let error = Rc::new(ExchangeError::new("original loaded failure"));
    let mut host = Loaded {
        value: source.clone(),
        error: error.clone(),
        reads: 0,
        throws: input["throws"] == true,
    };
    let keys = input["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|key| key.as_str().unwrap())
        .collect::<Vec<_>>();
    let result = match f(&mut host, &keys) {
        Err(e) => {
            assert!(Rc::ptr_eq(&e, &error));
            json!({"error":"original"})
        }
        Ok(out) => {
            assert!(!out.strict_equals(&source), "fresh shallow output object");
            let fields = out
                .own_keys()
                .into_iter()
                .map(|key| {
                    let v = out.get_key(&key).unwrap();
                    let name = String::from_utf16(&key).expect("ASCII fixture key");
                    let observed = match v {
                        Js::Undefined => json!("undefined"),
                        Js::Bool(v) => json!({"bool":v}),
                        Js::Number(n) if n.is_nan() => json!("NaN"),
                        Js::Number(n) => json!({"bits":bits(n)}),
                        _ if v.is_string() => json!({"units":v.to_js_string().unwrap()}),
                        _ => {
                            assert!(v.strict_equals(&nested));
                            json!("same-nested")
                        }
                    };
                    json!([name, observed])
                })
                .collect::<Vec<_>>();
            if input["kind"] == "object" {
                assert!(
                    source
                        .get("orders")
                        .unwrap()
                        .strict_equals(&Js::Bool(false)),
                    "source flags unchanged"
                );
                assert!(source.get("nested").unwrap().strict_equals(&nested));
                assert!(source.has_own("undefined"));
            }
            json!({"fields":fields})
        }
    };
    json!({"reads":host.reads,"result":result})
}

pub struct Detach {
    name: String,
    pub socket: Rc<Cell<f64>>,
    pub error: Rc<ExchangeError>,
    trace: RefCell<Vec<Value>>,
    reads: Cell<usize>,
    removals: Cell<usize>,
}
impl Detach {
    pub fn new(input: &Value) -> Self {
        Self {
            name: input["case"].as_str().unwrap().into(),
            socket: Rc::new(Cell::new(input["state"].as_f64().unwrap_or(1.0))),
            error: Rc::new(ExchangeError::new("original socket failure")),
            trace: RefCell::new(vec![]),
            reads: Cell::new(0),
            removals: Cell::new(0),
        }
    }
    pub fn get(&self) -> Result<Option<Rc<Cell<f64>>>, Failure> {
        self.trace.borrow_mut().push(json!("get"));
        if self.name == "get-fail" {
            Err(self.error.clone())
        } else {
            Ok((self.name != "missing").then(|| self.socket.clone()))
        }
    }
    pub fn remove(&self, socket: &Rc<Cell<f64>>, event: &str) -> Result<(), Failure> {
        assert!(Rc::ptr_eq(socket, &self.socket));
        self.removals.set(self.removals.get() + 1);
        self.trace.borrow_mut().push(json!(["remove", event]));
        if self.name == format!("remove{}-fail", self.removals.get()) {
            return Err(self.error.clone());
        }
        if self.name == "removal-changes-state" {
            socket.set(3.0);
        }
        Ok(())
    }
    pub fn ready(&self, socket: &Rc<Cell<f64>>) -> Result<f64, Failure> {
        assert!(Rc::ptr_eq(socket, &self.socket));
        self.reads.set(self.reads.get() + 1);
        self.trace.borrow_mut().push(json!("ready"));
        if self.name == format!("read{}-fail", self.reads.get()) {
            return Err(self.error.clone());
        }
        Ok(if self.name == "changed-ready" && self.reads.get() == 2 {
            0.0
        } else {
            socket.get()
        })
    }
    pub fn close(&self, socket: &Rc<Cell<f64>>) -> Result<(), Failure> {
        assert!(Rc::ptr_eq(socket, &self.socket));
        self.trace.borrow_mut().push(json!("close"));
        if self.name == "close-fail" {
            Err(self.error.clone())
        } else {
            socket.set(3.0);
            Ok(())
        }
    }
    pub fn observe(&self, r: Result<(), Failure>) -> Value {
        let result = match r {
            Ok(()) => "ok",
            Err(e) => {
                assert!(Rc::ptr_eq(&e, &self.error));
                "original"
            }
        };
        json!({"trace":*self.trace.borrow(),"result":result,"stateBits":bits(self.socket.get())})
    }
}

pub struct Reader {
    pub opts: HistoricalTradeOptions,
    pub public: bool,
    input: Value,
    pub error: ExchangeError,
    pub token: Js,
    calls: Cell<usize>,
    clocks: Cell<usize>,
    trace: RefCell<Vec<Value>>,
}
impl Reader {
    pub fn new(input: &Value) -> Self {
        let token = Js::object(vec![]);
        let mut error = ExchangeError::new("original emitter");
        error.code = Some(token.clone());
        Self {
            opts: serde_json::from_value(input["opts"].clone()).unwrap(),
            public: input["public"] == true,
            input: input.clone(),
            error,
            token,
            calls: Cell::new(0),
            clocks: Cell::new(0),
            trace: RefCell::new(vec![]),
        }
    }
    pub fn now(&self) -> f64 {
        self.clocks.set(self.clocks.get() + 1);
        3000000000.0
    }
    pub fn get(
        &self,
        path: &str,
        params: Value,
    ) -> std::future::Ready<Result<Option<Value>, ExchangeError>> {
        self.calls.set(self.calls.get() + 1);
        self.trace.borrow_mut().push(json!({"get":path,"keys":params.as_object().unwrap().keys().collect::<Vec<_>>(),"params":params}));
        std::future::ready(
            if self.input["reject"].as_u64() == Some(self.calls.get() as u64) {
                let mut e = ExchangeError::new("request failed");
                e.response = Some(json!({"data":{"msg":self.input["msg"]}}));
                Err(e)
            } else {
                Ok(self.input.get("page").cloned())
            },
        )
    }
    pub fn emit(&self, v: Value) -> Result<(), ExchangeError> {
        self.trace.borrow_mut().push(json!({"emit":v}));
        if self.input["emitThrows"] == true {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    pub fn observe(&self, result: Result<Vec<ClosedPnlEntry>, ExchangeError>) -> Value {
        let result = match result {
            Err(e) => {
                assert!(e.code.as_ref().unwrap().strict_equals(&self.token));
                json!({"error":"original-emitter"})
            }
            Ok(rows) => {
                json!({"rows":rows.iter().map(|r| json!({"symbol":r.symbol,"account":r.account,"closedTimeBits":bits(r.closed_time)})).collect::<Vec<_>>()})
            }
        };
        json!({"trace":*self.trace.borrow(),"clockCalls":self.clocks.get(),"result":result})
    }
}
