// Per-case expectation tables. Arms that currently coincide are kept separate
// because each names an independent case; collapsing them merges two
// expectations into one, which is the homogenized-corpus false green the
// cardinality audits in this tree exist to prevent.
#![allow(clippy::if_same_then_else)]
#[path = "support/canonical_value.rs"]
mod canonical_value;
use canonical_value::category;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{OrderSide, OrderStatus, OrderStopTriggerType, OrderType};
use safe_cex_venue_binance::{
    order_operations::*,
    private_event_value::{
        EventOps, EventValue, PrivateEnum, array_values, number, record, string,
    },
    request_weight::Failure,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn settle<F: Future>(f: F) -> F::Output {
    let w = Waker::noop().clone();
    let mut cx = Context::from_waker(&w);
    let mut f = pin!(f);
    for _ in 0..20 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("executor floor stopped poll20")
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/order-operations.json")).unwrap()
}
fn decoded(v: Value) -> EventValue {
    let out = EventValue::from_json(v);
    if let EventValue::Object(_) = &out {
        for (key, kind) in [
            ("type", 0),
            ("side", 1),
            ("tpTriggerType", 2),
            ("slTriggerType", 2),
        ] {
            if let EventValue::String(s) = out.get(key).unwrap() {
                let spelling = String::from_utf16(&s).unwrap();
                let e = match (kind, spelling.as_str()) {
                    (0, "limit") => PrivateEnum::Type(OrderType::Limit),
                    (0, "market") => PrivateEnum::Type(OrderType::Market),
                    (0, "stop_market") => PrivateEnum::Type(OrderType::StopLoss),
                    (0, "take_profit_market") => PrivateEnum::Type(OrderType::TakeProfit),
                    (1, "buy") => PrivateEnum::Side(OrderSide::Buy),
                    (2, "mark_price") => PrivateEnum::Trigger(OrderStopTriggerType::MarkPrice),
                    (2, "last_price") => PrivateEnum::Trigger(OrderStopTriggerType::LastPrice),
                    _ => continue,
                };
                out.write(key, EventValue::enum_value(e));
            }
        }
    }
    out
}
fn snap(v: &EventValue) -> EventValue {
    match v {
        EventValue::Array(a) => array(array_values(a).iter().map(snap).collect()),
        EventValue::Object(_) => EventValue::record_object(
            v.spread()
                .iter()
                .map(|(k, v)| (k.clone(), snap(v)))
                .collect(),
        ),
        _ => v.clone(),
    }
}
fn wire(v: &EventValue) -> Value {
    match v {
        EventValue::Undefined => json!({"$undefined":true}),
        EventValue::Null => Value::Null,
        EventValue::Number(n) => {
            if n.is_nan() {
                json!({"$nan":true})
            } else {
                json!(n)
            }
        }
        EventValue::Bool(v) => json!(v),
        EventValue::String(s) => json!(String::from_utf16(s).unwrap()),
        EventValue::Array(a) => array_values(a).iter().map(wire).collect(),
        EventValue::Object(_) => v
            .spread()
            .iter()
            .map(|(k, v)| (k.clone(), wire(v)))
            .collect(),
        _ => panic!("projection floor excludes inherited callable"),
    }
}
fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equal(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(k, a)| b.get(k).is_some_and(|b| equal(a, b)))
        }
        _ => a == b,
    }
}
fn num(v: &EventValue, n: f64, l: &str) {
    let EventValue::Number(v) = v else {
        panic!("{l}: native Number")
    };
    assert_eq!(*v, n, "{l}")
}
fn text(v: &EventValue, s: &str, l: &str) {
    let EventValue::String(v) = v else {
        panic!("{l}: expected native String {s:?}, got {v:?}")
    };
    assert_eq!(v.as_ref(), s.encode_utf16().collect::<Vec<_>>(), "{l}")
}
fn values(v: &EventValue, n: usize, l: &str) -> Vec<EventValue> {
    let EventValue::Array(a) = v else {
        panic!("{l}: native Array")
    };
    assert_eq!(a.borrow().len(), n, "{l}: native array count");
    array_values(a)
}
fn own(v: &EventValue, k: &str) -> bool {
    let EventValue::Object(o) = v else {
        panic!("native Object")
    };
    o.props
        .borrow()
        .contains_key(safe_cex_exchange::js_runtime::value::utf16(k).as_ref())
}
fn remove(v: &EventValue, k: &str) {
    let EventValue::Object(o) = v else { panic!() };
    o.props
        .borrow_mut()
        .shift_remove(safe_cex_exchange::js_runtime::value::utf16(k).as_ref());
}
fn key(m: Method) -> &'static str {
    match m {
        Method::Cancel => "cancelOrders",
        Method::Place => "placeOrder",
        Method::ConflictCancel => "cancelExistingClosePositionOrders",
        Method::RegularBatch => "placeRegularOrderBatch",
        Method::AlgoBatch => "placeAlgoOrders",
        Method::Batch => "placeOrderBatch",
    }
}
#[derive(Clone, Debug)]
enum Trace {
    Capture(&'static str),
    IsAlgo(EventValue),
    Delete(&'static str, EventValue),
    DeleteSettle(usize),
    Status(bool, EventValue, OrderStatus),
    Emit(EventValue),
    Resolve(EventValue),
    Call(Method, EventValue),
    Settle(Method),
    Format(EventValue),
}
impl Trace {
    fn wire(&self) -> Value {
        match self {
            Self::Capture(k) => json!(["capture", k]),
            Self::IsAlgo(v) => json!(["is_algo", wire(v)]),
            Self::Delete(ep, v) => json!(["delete", ep, wire(v)]),
            Self::DeleteSettle(n) => json!(["delete_settle", n]),
            Self::Status(one, v, status) => json!([
                "status",
                one,
                wire(v),
                wire(&EventValue::enum_value(PrivateEnum::Status(*status)))
            ]),
            Self::Emit(v) => json!(["emit", wire(v)]),
            Self::Resolve(v) => json!(["resolve_id", wire(v)]),
            Self::Call(m, v) => json!(["call", key(*m), wire(v)]),
            Self::Settle(m) => json!(["settle", key(*m)]),
            Self::Format(v) => json!(["format", wire(v)]),
        }
    }
}
struct State {
    name: String,
    venue: Venue,
    orders: EventValue,
    order: EventValue,
    update: EventValue,
    opts: EventValue,
    formatted: EventValue,
    returned: EventValue,
    error: Failure,
    replacement: Failure,
    trace: Vec<Trace>,
    deletes: Vec<(&'static str, EventValue)>,
    statuses: Vec<(bool, EventValue, OrderStatus)>,
    emissions: Vec<EventValue>,
    methods: Vec<(Method, EventValue)>,
    formats: Vec<EventValue>,
    resolves: Vec<EventValue>,
    algo_count: usize,
    cancel_refs: Option<bool>,
    format_refs: Option<bool>,
    batch_refs: Option<bool>,
    forwarded: EventValue,
}
type Shared = Rc<RefCell<State>>;
struct Bindings(Shared);
fn fail(name: &str) -> Failure {
    let mut e = ExchangeError::new("operation fixture");
    e.name = "RangeError".into();
    e.code = Some(if matches!(name, "unknown-number" | "status-unknown") {
        safe_cex_exchange::js_runtime::value::from_json(&json!(-2011))
    } else if name == "unknown-string" {
        safe_cex_exchange::js_runtime::value::from_json(&json!("-2011"))
    } else {
        safe_cex_exchange::js_runtime::value::from_json(&json!(-1000))
    });
    e.response = Some(json!({"status":503,"data":{"msg":"venue detail"}}));
    Rc::new(e)
}
fn state(venue: Venue, name: &str) -> Shared {
    let f = fixture();
    let mut rows = f["orders"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .map(decoded)
        .collect::<Vec<_>>();
    match name {
        "empty" => rows.clear(),
        "one" => rows.truncate(1),
        "chunk-eleven" => rows = (0..11)
            .map(|i| {
                decoded(
                    json!({"id":format!("900719925474099{i}"),"symbol":"BTCUSDT","type":"limit"}),
                )
            })
            .collect(),
        "algo-reject" => {
            rows = vec![
                rows[2].clone(),
                decoded(json!({"id":"5","symbol":"ETHUSDT","type":"take_profit_market"})),
            ]
        }
        "numeric-groups" => {
            rows = vec![
                decoded(json!({"id":"ten","symbol":"10","type":"limit"})),
                decoded(json!({"id":"two","symbol":"2","type":"limit"})),
                decoded(json!({"id":"proto","symbol":"__proto__","type":"limit"})),
            ]
        }
        "null-order" => rows = vec![EventValue::Null],
        _ => {}
    }
    let orders = if name == "null-input" {
        EventValue::Null
    } else {
        array(rows)
    };
    let order = decoded(f["order"].clone());
    let update = match name {
        "missing-update-fields" => record([]),
        "own-undefined" => record([
            ("price", EventValue::Undefined),
            ("amount", EventValue::Undefined),
        ]),
        "null-overrides" => record([("price", EventValue::Null), ("amount", EventValue::Null)]),
        "null-update" => EventValue::Null,
        _ => record([("price", number(12.4)), ("amount", number(4.2))]),
    };
    if name == "raw-flags" {
        order.write("reduceOnly", string("yes"));
        order.write("close", string("close"))
    }
    if name == "no-triggers" {
        for k in ["tpTriggerType", "slTriggerType", "close"] {
            remove(&order, k)
        }
    }
    let opts = record([
        ("symbol", string("BTCUSDT")),
        ("price", number(12.3)),
        ("amount", number(3.6)),
        ("nested", record([("marker", number(4.2))])),
    ]);
    let payloads = || {
        array(vec![
            record([("native", number(12.3))]),
            record([("native", number(3.6))]),
            record([("native", number(4.2))]),
        ])
    };
    let formatted = if name == "format-null" {
        EventValue::Null
    } else if venue == Venue::Futures {
        record([
            (
                "regular",
                if name == "empty" {
                    array(vec![])
                } else {
                    array(vec![
                        record([("native", number(12.3))]),
                        record([("native", number(3.6))]),
                    ])
                },
            ),
            (
                "algo",
                if name == "empty" {
                    array(vec![])
                } else {
                    array(vec![record([("native", number(4.2))])])
                },
            ),
        ])
    } else if name == "empty" {
        array(vec![])
    } else {
        payloads()
    };
    let mut replacement = ExchangeError::new("emitter fixture");
    replacement.name = "SyntaxError".into();
    replacement.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "emit-code"
    )));
    replacement.response = Some(json!({"status":504,"data":{"msg":"emit detail"}}));
    Rc::new(RefCell::new(State {
        name: name.into(),
        venue,
        orders,
        order,
        update,
        opts,
        formatted,
        returned: array(vec![string("batch-A"), EventValue::Null, string("batch-C")]),
        error: fail(name),
        replacement: Rc::new(replacement),
        trace: vec![],
        deletes: vec![],
        statuses: vec![],
        emissions: vec![],
        methods: vec![],
        formats: vec![],
        resolves: vec![],
        algo_count: 0,
        cancel_refs: None,
        format_refs: None,
        batch_refs: None,
        forwarded: EventValue::Undefined,
    }))
}
impl OrderBindings for Bindings {
    fn capture_delete(&mut self, venue: Venue) -> Result<Delete, Failure> {
        let s = self.0.clone();
        {
            let mut b = s.borrow_mut();
            assert_eq!(b.venue, venue, "native current delete venue");
            b.trace.push(Trace::Capture("delete"));
        }
        Ok(Rc::new(move |ep, params| {
            let mut b = s.borrow_mut();
            b.deletes.push((ep, snap(&params)));
            b.trace.push(Trace::Delete(ep, snap(&params)));
            let n = b.deletes.len();
            if b.name == "delete-sync" && n == 1 {
                return Err(b.error.clone());
            }
            let s = s.clone();
            Ok(Box::pin(async move {
                let mut b = s.borrow_mut();
                b.trace.push(Trace::DeleteSettle(n));
                if matches!(
                    b.name.as_str(),
                    "delete-reject"
                        | "unknown-number"
                        | "unknown-string"
                        | "algo-reject"
                        | "emit-reject"
                ) && n == 1
                {
                    return Err(b.error.clone());
                }
                if b.name == "mutate-after-delete" && n == 1 {
                    let EventValue::Array(a) = &b.orders else {
                        panic!()
                    };
                    for (i, o) in array_values(&a).iter().enumerate() {
                        o.write("id", string(format!("changed-{i}")))
                    }
                    EventValue::Array(a.clone())
                        .push(decoded(
                            json!({"id":"appended","symbol":"BTCUSDT","type":"limit"}),
                        ))
                        .unwrap();
                }
                Ok(EventValue::Undefined)
            }))
        }))
    }
    fn capture_status(&mut self, single: bool) -> Result<Status, Failure> {
        let s = self.0.clone();
        s.borrow_mut().trace.push(Trace::Capture(if single {
            "updateOrderStatus"
        } else {
            "updateOrdersStatus"
        }));
        Ok(Rc::new(move |rows, status| {
            let mut b = s.borrow_mut();
            b.statuses.push((single, snap(&rows), status));
            b.trace.push(Trace::Status(single, snap(&rows), status));
            if matches!(b.name.as_str(), "status-reject" | "status-unknown")
                && b.statuses.len() == 1
            {
                return Err(b.error.clone());
            }
            Ok(())
        }))
    }
    fn capture_emit(&mut self) -> Result<Sync, Failure> {
        let s = self.0.clone();
        s.borrow_mut().trace.push(Trace::Capture("emitAndLogError"));
        Ok(Rc::new(move |value| {
            let mut b = s.borrow_mut();
            b.emissions.push(snap(&value));
            b.trace.push(Trace::Emit(snap(&value)));
            if b.name == "emit-reject" && b.emissions.len() == 1 {
                return Err(b.replacement.clone());
            }
            Ok(EventValue::Undefined)
        }))
    }
    fn capture_is_algo(&mut self) -> Result<Sync, Failure> {
        let s = self.0.clone();
        s.borrow_mut().trace.push(Trace::Capture("isAlgoOrderType"));
        Ok(Rc::new(move |v| {
            let mut b = s.borrow_mut();
            b.algo_count += 1;
            b.trace.push(Trace::IsAlgo(v.clone()));
            if b.name == "filter-appends" && b.algo_count == 1 {
                let EventValue::Array(a) = &b.orders else {
                    panic!()
                };
                EventValue::Array(a.clone())
                    .push(decoded(
                        json!({"id":"later","symbol":"BTCUSDT","type":"limit"}),
                    ))
                    .unwrap();
            }
            let algo = match &v {
                EventValue::String(_) => safe_cex_venue_binance::order_logic::is_algo_order_type(
                    category::<OrderType>(&v),
                ),
                _ => false,
            };
            Ok(EventValue::Bool(algo))
        }))
    }
    fn capture_resolve_id(&mut self) -> Result<Sync, Failure> {
        let s = self.0.clone();
        s.borrow_mut()
            .trace
            .push(Trace::Capture("resolveCallerClientOrderId"));
        Ok(Rc::new(move |opts| {
            let mut b = s.borrow_mut();
            assert!(opts.same(&b.opts), "native resolve original opts");
            b.resolves.push(opts.clone());
            b.trace.push(Trace::Resolve(snap(&opts)));
            if b.name == "resolve-reject" {
                return Err(b.error.clone());
            }
            if b.name == "mutate-opts" {
                opts.write("price", number(12.4))
            }
            Ok(string("ignored"))
        }))
    }
    fn capture_format(&mut self) -> Result<Sync, Failure> {
        let s = self.0.clone();
        s.borrow_mut()
            .trace
            .push(Trace::Capture("formatCreateOrder"));
        Ok(Rc::new(move |opts| {
            let mut b = s.borrow_mut();
            b.format_refs = Some(opts.same(&b.opts));
            b.formats.push(opts.clone());
            b.trace.push(Trace::Format(snap(&opts)));
            if b.name == "format-reject" {
                return Err(b.error.clone());
            }
            Ok(b.formatted.clone())
        }))
    }
    fn capture_method(&mut self, m: Method) -> Result<Operation, Failure> {
        let s = self.0.clone();
        {
            let mut b = s.borrow_mut();
            b.trace.push(Trace::Capture(key(m)));
            if b.name == "batch-capture-mutates"
                && matches!(m, Method::Batch | Method::RegularBatch | Method::AlgoBatch)
            {
                let target = if b.venue == Venue::Futures {
                    b.formatted.read("regular").unwrap()
                } else {
                    b.formatted.clone()
                };
                let EventValue::Array(a) = target else {
                    panic!()
                };
                EventValue::Array(a.clone())
                    .push(record([("native", number(9.9))]))
                    .unwrap();
            }
        }
        Ok(Rc::new(move |value| {
            let mut b = s.borrow_mut();
            b.methods.push((m, value.clone()));
            b.trace.push(Trace::Call(m, snap(&value)));
            match m {
                Method::Cancel => {
                    let rows = values(&value, 1, "native cancel argument");
                    b.cancel_refs = Some(rows[0].same(&b.order));
                }
                Method::Place => b.forwarded = value.clone(),
                Method::ConflictCancel => {
                    assert!(value.same(&b.opts), "native conflict original opts")
                }
                Method::Batch | Method::RegularBatch | Method::AlgoBatch => {
                    let expected = match m {
                        Method::RegularBatch => b.formatted.read("regular").unwrap(),
                        Method::AlgoBatch => b.formatted.read("algo").unwrap(),
                        _ => b.formatted.clone(),
                    };
                    b.batch_refs = Some(value.same(&expected));
                }
            }
            if b.name == "batch-sync"
                && matches!(m, Method::Batch | Method::RegularBatch | Method::AlgoBatch)
            {
                return Err(b.error.clone());
            }
            let s = s.clone();
            Ok(Box::pin(async move {
                let mut b = s.borrow_mut();
                b.trace.push(Trace::Settle(m));
                if (b.name == "cancel-reject" && m == Method::Cancel)
                    || (b.name == "place-reject" && m == Method::Place)
                    || (b.name == "conflict-reject" && m == Method::ConflictCancel)
                    || (b.name == "batch-reject"
                        && matches!(m, Method::Batch | Method::RegularBatch | Method::AlgoBatch))
                    || (b.name == "algo-reject" && m == Method::AlgoBatch)
                {
                    return Err(b.error.clone());
                }
                if b.name == "after-cancel-mutation" && m == Method::Cancel {
                    b.order.write("price", number(99.0));
                    b.update.write("price", number(99.0))
                }
                if b.name == "mutate-opts" && m == Method::ConflictCancel {
                    b.opts.write("amount", number(4.2))
                }
                if b.name == "regular-mutates-algo" && m == Method::RegularBatch {
                    let EventValue::Array(a) = b.formatted.read("algo").unwrap() else {
                        panic!()
                    };
                    a.borrow_mut().clear();
                }
                Ok(match m {
                    Method::RegularBatch => array(vec![string("regular-A"), string("regular-B")]),
                    Method::AlgoBatch => array(vec![string("algo-A")]),
                    Method::Batch | Method::Place => b.returned.clone(),
                    _ => EventValue::Undefined,
                })
            }))
        }))
    }
}
fn generated(e: &Failure, b: &State, msg: &str, l: &str) {
    assert!(
        !Rc::ptr_eq(e, &b.error) && !Rc::ptr_eq(e, &b.replacement),
        "{l}: native generated Error identity"
    );
    assert_eq!(e.name, "TypeError", "{l}: native generated Error");
    assert_eq!(e.message, msg, "{l}: native generated message");
    assert!(
        e.code.is_none() && e.response.is_none(),
        "{l}: native absent generated metadata"
    );
}
fn original(e: &Failure, b: &State, replaced: bool, l: &str) {
    let expected = if replaced { &b.replacement } else { &b.error };
    assert!(Rc::ptr_eq(e, expected), "{l}: native Error Rc");
    assert_eq!(
        e.name,
        if replaced {
            "SyntaxError"
        } else {
            "RangeError"
        },
        "{l}"
    );
    assert_eq!(
        e.message,
        if replaced {
            "emitter fixture"
        } else {
            "operation fixture"
        },
        "{l}"
    );
    assert_eq!(
        observed_error_code(&e.code),
        observed_error_code(&expected.code),
        "{l}: native Error code"
    );
    assert_eq!(
        e.response,
        Some(if replaced {
            json!({"status":504,"data":{"msg":"emit detail"}})
        } else {
            json!({"status":503,"data":{"msg":"venue detail"}})
        }),
        "{l}: native Error response"
    );
}
fn error_wire(e: Option<&Failure>, b: &State) -> Value {
    e.map_or(Value::Null,|e|json!({"original":Rc::ptr_eq(e,&b.error),"replacement":Rc::ptr_eq(e,&b.replacement),"name":e.name,"message":e.message,"code":observed_error_code(&e.code).clone().unwrap_or(json!({"$undefined":true})),"response":e.response.clone().unwrap_or(json!({"$undefined":true}))}))
}
fn state_wire(b: &State, result: &Result<EventValue, Failure>) -> Value {
    json!({"name":b.name,"trace":b.trace.iter().map(Trace::wire).collect::<Vec<_>>(),"calls":b.deletes.iter().map(|(ep,p)|json!({"endpoint":ep,"params":wire(p)})).collect::<Vec<_>>(),"statuses":b.statuses.iter().map(|(one,v,s)|json!({"single":one,"rows":wire(v),"status":wire(&EventValue::enum_value(PrivateEnum::Status(*s)))})).collect::<Vec<_>>(),"emissions":b.emissions.iter().map(wire).collect::<Vec<_>>(),"value":result.as_ref().map(wire).unwrap_or(json!({"$undefined":true})),"forwarded":wire(&b.forwarded),"cancelRefs":b.cancel_refs,"formatRefs":b.format_refs,"batchRefs":b.batch_refs,"returnedRef":result.as_ref().is_ok_and(|v|v.same(&b.returned)),"failure":error_wire(result.as_ref().err(),b)})
}
fn compare(venue: &str, op: &str, actual: Vec<Value>, floor: usize) {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let output = Command::new("node")
        .current_dir(repo)
        .arg("sdk-rs/safe-cex-venue-binance/scripts/order-operations.mjs")
        .args([venue, op])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual source oracle failure {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), floor, "source authored case floor");
    assert_eq!(actual.len(), floor, "native authored case floor");
    for (i, (a, b)) in actual.iter().zip(&source).enumerate() {
        assert!(
            equal(a, b),
            "source {venue} {op} case{i}: native {a} source {b}"
        );
    }
}
fn cancel_counts(v: Venue, n: &str) -> (usize, usize, usize, bool) {
    let f = v == Venue::Futures;
    let p = v == Venue::PM;
    match n {
        "empty" => (0, usize::from(p), 0, false),
        "one" => (1, 1, 0, false),
        "chunk-eleven" => (if f { 2 } else { 11 }, 1, 0, false),
        "delete-reject" | "delete-sync" | "unknown-string" => (1, 0, if p { 2 } else { 1 }, true),
        "unknown-number" => {
            if f {
                (3, 3, 0, false)
            } else {
                (1, 0, if p { 2 } else { 1 }, true)
            }
        }
        "algo-reject" => (1, 0, if v == Venue::Spot { 1 } else { 2 }, true),
        "status-reject" => (
            if f {
                1
            } else if p {
                4
            } else {
                2
            },
            1,
            1,
            true,
        ),
        "status-unknown" => {
            if f {
                (3, 4, 0, false)
            } else {
                (if p { 4 } else { 2 }, 1, 1, true)
            }
        }
        "emit-reject" => (1, 0, if p { 2 } else { 1 }, true),
        "numeric-groups" => (3, if p { 1 } else { 3 }, 0, false),
        "null-order" => (0, 0, 1, true),
        "null-input" => (0, 0, usize::from(v != Venue::Spot), v != Venue::Spot),
        _ => (
            if f { 3 } else { 4 },
            if p {
                1
            } else if f {
                3
            } else {
                2
            },
            0,
            false,
        ),
    }
}
fn expected_requests(v: Venue, n: &str) -> Vec<(&'static str, &'static str, Vec<String>)> {
    let f = v == Venue::Futures;
    let p = v == Venue::PM;
    let ep = if f {
        "/fapi/v1/order"
    } else if p {
        "/papi/v1/um/order"
    } else {
        "/api/v3/order"
    };
    if matches!(n, "empty" | "null-order" | "null-input") {
        return vec![];
    }
    if n == "chunk-eleven" {
        if f {
            return vec![
                (
                    "/fapi/v1/batchOrders",
                    "BTCUSDT",
                    (0..10).map(|i| format!("900719925474099{i}")).collect(),
                ),
                (
                    "/fapi/v1/batchOrders",
                    "BTCUSDT",
                    vec!["90071992547409910".into()],
                ),
            ];
        }
        return (0..11)
            .map(|i| (ep, "BTCUSDT", vec![format!("900719925474099{i}")]))
            .collect();
    }
    if n == "numeric-groups" {
        return vec![
            (ep, "2", vec!["two".into()]),
            (ep, "10", vec!["ten".into()]),
            (ep, "__proto__", vec!["proto".into()]),
        ];
    }
    if n == "algo-reject" {
        return vec![(
            if f {
                "/fapi/v1/algoOrder"
            } else if p {
                "/papi/v1/um/conditional/order"
            } else {
                ep
            },
            "ETHUSDT",
            vec!["3".into()],
        )];
    }
    if n == "one" {
        return vec![(ep, "BTCUSDT", vec!["9007199254740993".into()])];
    }
    if f {
        let mut btc = vec!["9007199254740993".into(), "2".into()];
        if n == "filter-appends" {
            btc.push("later".into())
        }
        vec![
            ("/fapi/v1/batchOrders", "BTCUSDT", btc),
            (ep, "ETHUSDT", vec!["4".into()]),
            (
                "/fapi/v1/algoOrder",
                "ETHUSDT",
                vec![
                    if n == "mutate-after-delete" {
                        "changed-2"
                    } else {
                        "3"
                    }
                    .into(),
                ],
            ),
        ]
    } else {
        vec![
            (ep, "BTCUSDT", vec!["9007199254740993".into()]),
            (
                ep,
                "BTCUSDT",
                vec![
                    if p && n == "mutate-after-delete" {
                        "changed-1"
                    } else {
                        "2"
                    }
                    .into(),
                ],
            ),
            (
                if p {
                    "/papi/v1/um/conditional/order"
                } else {
                    ep
                },
                "ETHUSDT",
                vec![
                    if p && n == "mutate-after-delete" {
                        "changed-2"
                    } else {
                        "3"
                    }
                    .into(),
                ],
            ),
            (
                ep,
                "ETHUSDT",
                vec![
                    if p && n == "mutate-after-delete" {
                        "changed-3"
                    } else {
                        "4"
                    }
                    .into(),
                ],
            ),
        ]
    }
}
fn cancel_test(v: Venue, label: &str) {
    let f = fixture();
    assert_eq!(f["cases"]["cancel"].as_array().unwrap().len(), 17);
    let mut actual = vec![];
    for (i, name) in f["cases"]["cancel"].as_array().unwrap().iter().enumerate() {
        let name = name.as_str().unwrap();
        let shared = state(v, name);
        let orders = shared.borrow().orders.clone();
        let mut b = Bindings(shared.clone());
        let result = settle(async {
            match v {
                Venue::Futures => cancel_futures(orders, &mut b).await,
                Venue::PM => cancel_pm(orders, &mut b).await,
                Venue::Spot => cancel_spot(orders, &mut b).await,
            }
        });
        let s = shared.borrow();
        let l = format!("native {label} cancel case{i} {name}");
        let (d, st, em, failed) = cancel_counts(v, name);
        assert_eq!(s.deletes.len(), d, "{l}: native DELETE count");
        assert_eq!(s.statuses.len(), st, "{l}: native status count");
        assert_eq!(s.emissions.len(), em, "{l}: native emit count");
        assert_eq!(result.is_err(), failed, "{l}: native outcome");
        if let Err(e) = &result {
            if matches!(name, "null-order" | "null-input") {
                let key = if name == "null-order" {
                    if v == Venue::Futures { "type" } else { "id" }
                } else if v == Venue::Futures {
                    "filter"
                } else {
                    "map"
                };
                generated(
                    e,
                    &s,
                    &format!("Cannot read properties of null (reading '{key}')"),
                    &l,
                );
            } else {
                original(e, &s, name == "emit-reject", &l)
            }
        } else {
            assert!(
                matches!(result.as_ref().unwrap(), EventValue::Undefined),
                "{l}: native undefined return"
            )
        }
        for (j, ((ep, params), (want, symbol, ids))) in
            s.deletes.iter().zip(expected_requests(v, name)).enumerate()
        {
            let l = format!("{l} request{j}");
            assert_eq!(*ep, want, "{l}: native endpoint");
            if ep.ends_with("algoOrder") {
                assert_eq!(params.spread().len(), 1, "{l}: native params count");
                text(
                    &params.read("algoId").unwrap(),
                    &ids[0],
                    &format!("{l}: native algoId"),
                );
            } else {
                assert_eq!(params.spread().len(), 2, "{l}: native params count");
                text(&params.read("symbol").unwrap(), symbol, &l);
                let k = if ep.ends_with("batchOrders") {
                    "orderIdList"
                } else if ep.ends_with("conditional/order") {
                    "newClientStrategyId"
                } else if v == Venue::Futures {
                    "orderId"
                } else {
                    "origClientOrderId"
                };
                text(
                    &params.get(k).unwrap(),
                    &if k == "orderIdList" {
                        format!("[{}]", ids.join(","))
                    } else {
                        ids[0].clone()
                    },
                    &format!("{l}: native {k}"),
                );
            }
        }
        for (j, (one, rows, status)) in s.statuses.iter().enumerate() {
            assert_eq!(*status, OrderStatus::Canceled, "{l}: native status enum");
            if *one {
                assert_eq!(v, Venue::Futures, "{l}: native single status owner");
                text(
                    &rows.read("id").unwrap(),
                    if name == "mutate-after-delete" {
                        "changed-2"
                    } else {
                        "3"
                    },
                    &format!("{l}: native late algo status ID"),
                );
                continue;
            }
            let expected: Vec<String> = match name {
                "empty" => vec![],
                "one" => vec!["9007199254740993".into()],
                "chunk-eleven" => (0..11).map(|i| format!("900719925474099{i}")).collect(),
                "numeric-groups" => {
                    if v == Venue::PM {
                        vec!["ten".into(), "two".into(), "proto".into()]
                    } else {
                        vec![
                            if j == 0 {
                                "two"
                            } else if j == 1 {
                                "ten"
                            } else {
                                "proto"
                            }
                            .into(),
                        ]
                    }
                }
                _ => {
                    if v == Venue::PM {
                        if name == "mutate-after-delete" {
                            vec![
                                "changed-0".into(),
                                "changed-1".into(),
                                "changed-2".into(),
                                "changed-3".into(),
                                "appended".into(),
                            ]
                        } else {
                            vec![
                                "9007199254740993".into(),
                                "2".into(),
                                "3".into(),
                                "4".into(),
                            ]
                        }
                    } else {
                        let btc =
                            j == 0 || (v == Venue::Futures && name == "status-unknown" && j == 1);
                        if btc {
                            let mut ids = if v == Venue::Futures && name == "mutate-after-delete" {
                                vec!["changed-0".into(), "changed-1".into()]
                            } else {
                                vec!["9007199254740993".into(), "2".into()]
                            };
                            if v == Venue::Futures && name == "filter-appends" {
                                ids.push("later".into())
                            }
                            ids
                        } else if v == Venue::Futures {
                            vec![
                                if name == "mutate-after-delete" {
                                    "changed-3"
                                } else {
                                    "4"
                                }
                                .into(),
                            ]
                        } else {
                            vec!["3".into(), "4".into()]
                        }
                    }
                }
            };
            let items = values(
                rows,
                expected.len(),
                &format!("{l}: native status row count"),
            );
            for (row, id) in items.iter().zip(expected) {
                assert_eq!(row.spread().len(), 1, "{l}: native ID-only status");
                text(
                    &row.read("id").unwrap(),
                    &id,
                    &format!("{l}: native status ID"),
                );
            }
        }
        if name == "filter-appends" {
            assert_eq!(
                s.algo_count,
                if v == Venue::Futures { 9 } else { 0 },
                "{l}: native independent filter visits"
            );
            values(&s.orders, if v == Venue::Futures { 5 } else { 4 }, &l);
        }
        if name == "mutate-after-delete" {
            values(&s.orders, 5, &l);
        }
        for (j, m) in s.emissions.iter().enumerate() {
            let expected = if matches!(name, "null-order" | "null-input") {
                let key = if name == "null-order" {
                    if v == Venue::Futures { "type" } else { "id" }
                } else if v == Venue::Futures {
                    "filter"
                } else {
                    "map"
                };
                format!("Cannot read properties of null (reading '{key}')")
            } else if v == Venue::PM
                && j == 0
                && matches!(
                    name,
                    "delete-reject"
                        | "delete-sync"
                        | "unknown-number"
                        | "unknown-string"
                        | "algo-reject"
                        | "emit-reject"
                )
            {
                format!(
                    "Failed to cancel order {}: venue detail",
                    if name == "algo-reject" {
                        "3"
                    } else {
                        "9007199254740993"
                    }
                )
            } else if name == "emit-reject" && j > 0 {
                "emit detail".into()
            } else {
                "venue detail".into()
            };
            text(m, &expected, &format!("{l}: native emission text"));
        }
        actual.push(state_wire(&s, &result));
    }
    compare(label, "cancel", actual, 17);
}
fn update_test(v: Venue, label: &str) {
    let f = fixture();
    assert_eq!(f["cases"]["update"].as_array().unwrap().len(), 11);
    let mut actual = vec![];
    for (i, name) in f["cases"]["update"].as_array().unwrap().iter().enumerate() {
        let name = name.as_str().unwrap();
        let shared = state(v, name);
        let args = {
            let s = shared.borrow();
            if name == "null-args" {
                EventValue::Null
            } else {
                record([("order", s.order.clone()), ("update", s.update.clone())])
            }
        };
        let mut b = Bindings(shared.clone());
        let result = settle(async {
            match v {
                Venue::Futures => update_futures(args, &mut b).await,
                Venue::PM => update_pm(args, &mut b).await,
                Venue::Spot => update_spot(args, &mut b).await,
            }
        });
        let s = shared.borrow();
        let l = format!("native {label} update case{i} {name}");
        let count = if matches!(name, "null-args" | "null-update") {
            0
        } else if name == "cancel-reject" {
            1
        } else {
            2
        };
        assert_eq!(s.methods.len(), count, "{l}: native method count");
        assert_eq!(
            result.is_err(),
            matches!(
                name,
                "cancel-reject" | "place-reject" | "null-args" | "null-update"
            ),
            "{l}: native outcome"
        );
        assert!(
            s.deletes.is_empty()
                && s.statuses.is_empty()
                && s.emissions.is_empty()
                && s.formats.is_empty()
                && s.resolves.is_empty(),
            "{l}: native direct delegate scope"
        );
        if let Err(e) = &result {
            if matches!(name, "null-args" | "null-update") {
                generated(
                    e,
                    &s,
                    if name == "null-args" {
                        "Cannot destructure property 'order' of 'object null' as it is null."
                    } else {
                        "Cannot use 'in' operator to search for 'price' in null"
                    },
                    &l,
                );
            } else {
                original(e, &s, false, &l)
            }
        } else {
            assert!(
                result.as_ref().unwrap().same(&s.returned),
                "{l}: native placed result identity"
            );
            let rows = values(result.as_ref().unwrap(), 3, &l);
            text(&rows[0], "batch-A", &l);
            assert!(
                matches!(rows[1], EventValue::Null),
                "{l}: native raw null ID"
            );
            text(&rows[2], "batch-C", &l);
        }
        if count > 0 {
            assert_eq!(
                s.methods[0].0,
                Method::Cancel,
                "{l}: native cancel before place"
            );
            assert_eq!(
                s.cancel_refs,
                Some(true),
                "{l}: native original order identity"
            );
        }
        if count == 2 {
            assert_eq!(s.methods[1].0, Method::Place, "{l}: native place method");
            let new = &s.forwarded;
            assert!(!new.same(&s.order), "{l}: native fresh new order");
            assert_eq!(
                new.spread().len(),
                if v == Venue::Spot {
                    6
                } else if name == "no-triggers" {
                    if v == Venue::Futures { 7 } else { 6 }
                } else if v == Venue::Futures {
                    9
                } else {
                    8
                },
                "{l}: native new order field count"
            );
            text(&new.read("symbol").unwrap(), "BTCUSDT", &l);
            assert!(
                matches!(
                    category::<safe_cex_types::OrderType>(&(new.read("type").unwrap())),
                    OrderType::Limit
                ),
                "{l}: native type enum"
            );
            assert!(
                matches!(
                    category::<safe_cex_types::OrderSide>(&(new.read("side").unwrap())),
                    OrderSide::Buy
                ),
                "{l}: native side enum"
            );
            for (k, normal, original) in [("price", 12.4, 12.3), ("amount", 4.2, 3.6)] {
                let value = new.get(k).unwrap();
                assert!(own(new, k), "{l}: native own {k}");
                match name {
                    "own-undefined" => assert!(
                        matches!(value, EventValue::Undefined),
                        "{l}: native undefined {k}"
                    ),
                    "null-overrides" => {
                        assert!(matches!(value, EventValue::Null), "{l}: native null {k}")
                    }
                    _ => num(
                        &value,
                        if name == "missing-update-fields" {
                            original
                        } else {
                            normal
                        },
                        &format!("{l}: native new {k}"),
                    ),
                }
            }
            if name == "raw-flags" {
                text(&new.read("reduceOnly").unwrap(), "yes", &l)
            } else {
                assert!(
                    matches!(new.read("reduceOnly").unwrap(), EventValue::Bool(false)),
                    "{l}: native reduceOnly false"
                )
            };
            assert_eq!(
                own(new, "close"),
                v == Venue::Futures,
                "{l}: native close presence"
            );
            if v == Venue::Futures {
                match name {
                    "raw-flags" => text(&new.read("close").unwrap(), "close", &l),
                    "no-triggers" => assert!(
                        matches!(new.read("close").unwrap(), EventValue::Bool(false)),
                        "{l}: native close false"
                    ),
                    _ => assert!(
                        matches!(new.read("close").unwrap(), EventValue::Bool(true)),
                        "{l}: native close true"
                    ),
                }
            }
            for (k, t) in [
                ("tpTriggerType", OrderStopTriggerType::MarkPrice),
                ("slTriggerType", OrderStopTriggerType::LastPrice),
            ] {
                let present = v != Venue::Spot && name != "no-triggers";
                assert_eq!(own(new, k), present, "{l}: native {k} presence");
                if present {
                    assert!(
                        matches!(category::<safe_cex_types::OrderStopTriggerType>(&(new.get(k).unwrap())), a if a==t),
                        "{l}: native trigger enum"
                    )
                }
            }
            if name == "after-cancel-mutation" {
                num(&s.order.read("price").unwrap(), 99.0, &l);
                num(
                    &new.read("price").unwrap(),
                    12.4,
                    &format!("{l}: native snapshot price"),
                );
            }
        }
        actual.push(state_wire(&s, &result));
    }
    compare(label, "update", actual, 11);
}
fn place_test(v: Venue, label: &str) {
    let f = fixture();
    assert_eq!(f["cases"]["place"].as_array().unwrap().len(), 12);
    let mut actual = vec![];
    for (i, name) in f["cases"]["place"].as_array().unwrap().iter().enumerate() {
        let name = name.as_str().unwrap();
        let shared = state(v, name);
        let opts = shared.borrow().opts.clone();
        let mut b = Bindings(shared.clone());
        let result = settle(async {
            match v {
                Venue::Futures => place_futures(opts, &mut b).await,
                Venue::PM => place_pm(opts, &mut b).await,
                Venue::Spot => place_spot(opts, &mut b).await,
            }
        });
        let s = shared.borrow();
        let l = format!("native {label} place case{i} {name}");
        let futures = v == Venue::Futures;
        let expected = if futures {
            match name {
                "resolve-reject" => vec![],
                "empty" | "conflict-reject" | "format-reject" | "format-null" => {
                    vec![Method::ConflictCancel]
                }
                "batch-sync" | "batch-reject" | "regular-mutates-algo" => {
                    vec![Method::ConflictCancel, Method::RegularBatch]
                }
                _ => vec![
                    Method::ConflictCancel,
                    Method::RegularBatch,
                    Method::AlgoBatch,
                ],
            }
        } else if name == "format-reject" {
            vec![]
        } else {
            vec![Method::Batch]
        };
        assert_eq!(
            s.methods.iter().map(|(m, _)| *m).collect::<Vec<_>>(),
            expected,
            "{l}: native method order/count"
        );
        assert_eq!(
            s.resolves.len(),
            usize::from(futures),
            "{l}: native explicit resolve count"
        );
        let format_count =
            usize::from(!(futures && matches!(name, "resolve-reject" | "conflict-reject")));
        assert_eq!(s.formats.len(), format_count, "{l}: native format count");
        if format_count == 1 {
            assert_eq!(
                s.format_refs,
                Some(true),
                "{l}: native original format opts"
            );
            let o = &s.formats[0];
            num(
                &o.read("price").unwrap(),
                if futures && name == "mutate-opts" {
                    12.4
                } else {
                    12.3
                },
                &format!("{l}: native format price"),
            );
            num(
                &o.read("amount").unwrap(),
                if futures && name == "mutate-opts" {
                    4.2
                } else {
                    3.6
                },
                &format!("{l}: native format amount"),
            );
            assert!(
                o.read("nested")
                    .unwrap()
                    .same(&s.opts.read("nested").unwrap()),
                "{l}: native nested opts alias"
            );
        }
        let failed = matches!(name, "format-reject" | "batch-sync" | "batch-reject")
            || (futures
                && matches!(
                    name,
                    "resolve-reject" | "conflict-reject" | "algo-reject" | "format-null"
                ));
        assert_eq!(result.is_err(), failed, "{l}: native outcome");
        if let Err(e) = &result {
            if name == "format-null" {
                generated(
                    e,
                    &s,
                    "Cannot destructure property 'regular' of 'this.formatCreateOrder(...)' as it is null.",
                    &l,
                );
            } else {
                original(e, &s, false, &l)
            }
        } else if futures {
            assert!(
                !result.as_ref().unwrap().same(&s.returned),
                "{l}: native fresh aggregate"
            );
            let n = if name == "empty" {
                0
            } else if name == "regular-mutates-algo" {
                2
            } else {
                3
            };
            let rows = values(
                result.as_ref().unwrap(),
                n,
                &format!("{l}: native result count"),
            );
            if n > 0 {
                text(&rows[0], "regular-A", &l);
                text(&rows[1], "regular-B", &l);
            }
            if n == 3 {
                text(&rows[2], "algo-A", &format!("{l}: native algo result"));
            }
        } else {
            assert!(
                result.as_ref().unwrap().same(&s.returned),
                "{l}: native direct batch result identity"
            );
            let rows = values(result.as_ref().unwrap(), 3, &l);
            text(&rows[0], "batch-A", &l);
            assert!(
                matches!(rows[1], EventValue::Null),
                "{l}: native raw null ID"
            );
            text(&rows[2], "batch-C", &l);
        }
        let batches = s
            .methods
            .iter()
            .filter(|(m, _)| matches!(m, Method::Batch | Method::RegularBatch | Method::AlgoBatch))
            .collect::<Vec<_>>();
        if !batches.is_empty() {
            assert_eq!(
                s.batch_refs,
                Some(true),
                "{l}: native original batch payload identity"
            );
            for (m, payloads) in batches {
                let expected = match m {
                    Method::RegularBatch => s.formatted.read("regular").unwrap(),
                    Method::AlgoBatch => s.formatted.read("algo").unwrap(),
                    _ => s.formatted.clone(),
                };
                assert!(payloads.same(&expected), "{l}: native original payloads");
                if name == "format-null" {
                    assert!(
                        matches!(payloads, EventValue::Null),
                        "{l}: native raw null payloads"
                    )
                } else {
                    let count = if name == "empty" {
                        0
                    } else if name == "batch-capture-mutates" {
                        if *m == Method::AlgoBatch {
                            1
                        } else if futures {
                            4
                        } else {
                            4
                        }
                    } else if *m == Method::RegularBatch {
                        2
                    } else if *m == Method::AlgoBatch {
                        1
                    } else {
                        3
                    };
                    let rows = values(payloads, count, &format!("{l}: native batch payload count"));
                    if count > 0 {
                        num(
                            &rows[0].read("native").unwrap(),
                            if *m == Method::AlgoBatch { 4.2 } else { 12.3 },
                            &format!("{l}: native first payload"),
                        );
                    }
                    if name == "batch-capture-mutates" && *m != Method::AlgoBatch {
                        num(
                            &rows[count - 1].read("native").unwrap(),
                            9.9,
                            &format!("{l}: native appended payload"),
                        );
                    }
                }
            }
        }
        assert!(
            s.deletes.is_empty() && s.statuses.is_empty() && s.emissions.is_empty(),
            "{l}: native no catch/store side effects"
        );
        actual.push(state_wire(&s, &result));
    }
    compare(label, "place", actual, 12);
}
#[test]
#[ignore = "requires TypeScript oracle"]
fn futures_cancel() {
    cancel_test(Venue::Futures, "futures")
}
#[test]
#[ignore = "requires TypeScript oracle"]
fn pm_cancel() {
    cancel_test(Venue::PM, "pm")
}
#[test]
#[ignore = "requires TypeScript oracle"]
fn spot_cancel() {
    cancel_test(Venue::Spot, "spot")
}
#[test]
#[ignore = "requires TypeScript oracle"]
fn futures_update() {
    update_test(Venue::Futures, "futures")
}
#[test]
#[ignore = "requires TypeScript oracle"]
fn pm_update() {
    update_test(Venue::PM, "pm")
}
#[test]
#[ignore = "requires TypeScript oracle"]
fn spot_update() {
    update_test(Venue::Spot, "spot")
}
#[test]
#[ignore = "requires TypeScript oracle"]
fn futures_place() {
    place_test(Venue::Futures, "futures")
}
#[test]
#[ignore = "requires TypeScript oracle"]
fn pm_place() {
    place_test(Venue::PM, "pm")
}
#[test]
#[ignore = "requires TypeScript oracle"]
fn spot_place() {
    place_test(Venue::Spot, "spot")
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
