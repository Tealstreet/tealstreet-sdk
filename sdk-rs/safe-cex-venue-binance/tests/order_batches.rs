#[path = "support/canonical_value.rs"]
mod canonical_value;
use canonical_value::category;
use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_num::{JsJsonValue, PrecisionInput, js_json_stringify};
use safe_cex_types::{OrderSide, OrderStopTriggerType, OrderType, PositionSide};
use safe_cex_venue_binance::{
    order_batches::*,
    order_operations::{Method, Operation, Sync, Venue, array},
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
    serde_json::from_str(include_str!("fixtures/order-batches.json")).unwrap()
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
                    (1, "sell") => PrivateEnum::Side(OrderSide::Sell),
                    (1, "long") => PrivateEnum::PositionSide(PositionSide::Long),
                    (1, "short") => PrivateEnum::PositionSide(PositionSide::Short),
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
        EventValue::Array(a) => array(
            array_values(a)
                .expect("fixture array reads")
                .iter()
                .map(snap)
                .collect(),
        ),
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
    assert_eq!(a.len(), n, "{l}: native array count");
    array_values(a).expect("fixture array reads")
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
        Method::RegularBatch => "placeRegularOrderBatch",
        Method::AlgoBatch => "placeAlgoOrders",
        Method::Batch => "placeOrderBatch",
        Method::Cancel => "cancelOrders",
        _ => panic!("batch method floor"),
    }
}
fn encoded_input(v: &EventValue) -> JsJsonValue {
    match v {
        EventValue::Undefined => JsJsonValue::Undefined,
        EventValue::Null => JsJsonValue::Null,
        EventValue::Bool(v) => JsJsonValue::Bool(*v),
        EventValue::Number(v) => JsJsonValue::Number(*v),
        EventValue::String(v) => JsJsonValue::String(String::from_utf16(v).unwrap()),
        EventValue::Array(a) => JsJsonValue::Array(
            array_values(a)
                .expect("fixture array reads")
                .iter()
                .map(encoded_input)
                .collect(),
        ),
        EventValue::Object(_) => JsJsonValue::Object(
            v.spread()
                .iter()
                .map(|(k, v)| (k.clone(), encoded_input(v)))
                .collect(),
        ),
        _ => panic!("input codec excludes inherited callables"),
    }
}
fn classifier_input(v: &EventValue) -> Value {
    match v {
        EventValue::Undefined => Value::Null,
        _ => wire(v),
    }
} // INPUT adapter only, never native output proof.
#[derive(Clone, Debug)]
enum Trace {
    Capture(&'static str),
    CaptureMethod(Method, usize),
    Assert(EventValue),
    Format(EventValue),
    Call(Method, usize, EventValue),
    Settle(Method, usize),
    Post(&'static str, EventValue),
    PostSettle(usize),
    Stringify(EventValue),
    Positions,
    Markets,
    Adjust(EventValue, EventValue),
    Orders,
    Classify(Rejection),
    Emit(EventValue),
    Log(String, LogSeverity),
}
struct PostReceipt {
    endpoint: &'static str,
    arg: EventValue,
    sent: EventValue,
}
struct MethodReceipt {
    method: Method,
    version: usize,
    arg: EventValue,
    sent: EventValue,
}
struct State {
    family: String,
    venue: Venue,
    name: String,
    payloads: EventValue,
    orders: EventValue,
    positions: EventValue,
    opts: EventValue,
    conflicts: EventValue,
    market: EventValue,
    data: EventValue,
    error: Failure,
    replacement: Failure,
    returned: EventValue,
    trace: Vec<Trace>,
    posts: Vec<PostReceipt>,
    methods: Vec<MethodReceipt>,
    formats: Vec<EventValue>,
    format_outputs: Vec<EventValue>,
    classifications: Vec<Rejection>,
    emissions: Vec<EventValue>,
    logs: Vec<(String, LogSeverity)>,
    stringifies: Vec<EventValue>,
    position_reads: usize,
    market_reads: usize,
    adjusts: Vec<(EventValue, EventValue)>,
    order_reads: usize,
    assert_refs: Option<bool>,
    cancel_refs: Option<bool>,
    version: usize,
}
type Shared = Rc<RefCell<State>>;
struct Bindings(Shared);
fn error(name: &str) -> Failure {
    let mut e = ExchangeError::new("batch fixture");
    e.name = "RangeError".into();
    e.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        if name == "post-only-reject" {
            -5022
        } else {
            -1000
        }
    )));
    e.response = Some(json!({"status":503,"data":{"msg":"venue detail"}}));
    Rc::new(e)
}
fn state(family: &str, name: &str) -> Shared {
    let f = fixture();
    let venue = if matches!(family, "pm" | "many_pm") {
        Venue::PM
    } else if matches!(family, "spot" | "many_spot") {
        Venue::Spot
    } else {
        Venue::Futures
    };
    let mut payloads = f["payloads"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .map(decoded)
        .collect::<Vec<_>>();
    let mut orders = f["orders"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .map(decoded)
        .collect::<Vec<_>>();
    if name == "empty" {
        payloads.clear();
        orders.clear()
    }
    if name == "one" {
        payloads.truncate(1)
    }
    if name == "seven" {
        let p = decoded(f["payloads"][0].clone());
        p.write("newClientOrderId", string("c6"));
        payloads.push(p)
    }
    if name == "null-payload" {
        payloads = vec![EventValue::Null]
    }
    if name == "missing-client-id" {
        remove(&payloads[0], "newClientOrderId")
    }
    let positions = f["positions"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .map(decoded)
        .collect::<Vec<_>>();
    if name == "zero-contracts" {
        for p in &positions {
            p.write("contracts", number(0.0))
        }
    }
    if name == "nonnumeric-quantity" {
        for i in [1, 3, 4] {
            payloads[i].write("quantity", string("bad"))
        }
    }
    if name == "raw-quantity-array" {
        payloads[1].write("quantity", array(vec![number(3.6)]))
    }
    if name == "strategy-override" {
        payloads[1].write("strategyType", string("OVERRIDE"))
    }
    let payloads = if name == "null-input" {
        EventValue::Null
    } else {
        array(payloads)
    };
    let orders = if name == "null-input" {
        EventValue::Null
    } else {
        array(orders)
    };
    let positions = if name == "no-positions" {
        array(vec![])
    } else {
        array(positions)
    };
    let opts = decoded(f["conflict_opts"].clone());
    match name {
        "non-stop" => opts.write(
            "type",
            EventValue::enum_value(PrivateEnum::Type(OrderType::Limit)),
        ),
        "sized" => {
            opts.write("close", EventValue::Bool(false));
            opts.write("amount", number(3.6))
        }
        "adjust-zero" => {
            opts.write("close", EventValue::Bool(false));
            opts.write("amount", number(0.04))
        }
        "raw-close-string" => opts.write("close", string("yes")),
        "raw-amount-string" => {
            opts.write("close", EventValue::Bool(false));
            opts.write("amount", string("0"))
        }
        _ => {}
    }
    let opts = if name == "null-opts" {
        EventValue::Null
    } else {
        opts
    };
    let conflicts = if name == "no-matches" {
        array(vec![])
    } else {
        array(
            f["conflict_orders"]
                .as_array()
                .unwrap()
                .iter()
                .cloned()
                .map(decoded)
                .collect(),
        )
    };
    let market = if name == "no-market" {
        EventValue::Null
    } else {
        record([(
            "precision",
            record([(
                "amount",
                if name == "precision-string" {
                    string("0.1")
                } else {
                    number(0.1)
                },
            )]),
        )])
    };
    let data = match name {
        "data-null" => EventValue::Null,
        "data-object" => record([]),
        "bad-foreach" => record([("forEach", number(0.0))]),
        "null-row" => array(vec![EventValue::Null]),
        "code-string-zero" => array(vec![record([
            ("code", string("0")),
            ("msg", string("string zero")),
        ])]),
        "missing-id" => array(vec![record([])]),
        "noncall-id" => array(vec![record([(
            "orderId",
            record([("toString", number(0.0))]),
        )])]),
        _ => array(
            f["batch_response"]
                .as_array()
                .unwrap()
                .iter()
                .cloned()
                .map(decoded)
                .collect(),
        ),
    };
    let mut replacement = ExchangeError::new("emitter fixture");
    replacement.name = "SyntaxError".into();
    replacement.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "emit-code"
    )));
    replacement.response = Some(json!({"status":504,"data":{"msg":"emit detail"}}));
    Rc::new(RefCell::new(State {
        family: family.into(),
        venue,
        name: name.into(),
        payloads,
        orders,
        positions,
        opts,
        conflicts,
        market,
        data,
        error: error(name),
        replacement: Rc::new(replacement),
        returned: array(vec![
            string("return-A"),
            EventValue::Null,
            string("return-C"),
        ]),
        trace: vec![],
        posts: vec![],
        methods: vec![],
        formats: vec![],
        format_outputs: vec![],
        classifications: vec![],
        emissions: vec![],
        logs: vec![],
        stringifies: vec![],
        position_reads: 0,
        market_reads: 0,
        adjusts: vec![],
        order_reads: 0,
        assert_refs: None,
        cancel_refs: None,
        version: 1,
    }))
}
fn append_payload(s: &State) {
    let EventValue::Array(a) = &s.payloads else {
        panic!()
    };
    let p = decoded(fixture()["payloads"][0].clone());
    p.write("newClientOrderId", string("c6"));
    EventValue::Array(a.clone()).push(p).unwrap()
}
fn added_order() -> EventValue {
    let o = decoded(fixture()["orders"][0].clone());
    o.write("id", string("added"));
    o
}
impl BatchBindings for Bindings {
    fn capture_assert_ids(&mut self) -> Result<Sync, Failure> {
        let s = self.0.clone();
        s.borrow_mut()
            .trace
            .push(Trace::Capture("assertCallerClientOrderIds"));
        Ok(Rc::new(move |o| {
            let mut b = s.borrow_mut();
            b.assert_refs = Some(o.same(&b.orders));
            b.trace.push(Trace::Assert(snap(&o)));
            if b.name == "assert-reject" {
                return Err(b.error.clone());
            }
            if b.name == "assert-appends" {
                let EventValue::Array(a) = o else { panic!() };
                EventValue::Array(a.clone()).push(added_order()).unwrap()
            }
            Ok(EventValue::Undefined)
        }))
    }
    fn capture_format(&mut self) -> Result<Sync, Failure> {
        let s = self.0.clone();
        s.borrow_mut()
            .trace
            .push(Trace::Capture("formatCreateOrder"));
        Ok(Rc::new(move |o| {
            let mut b = s.borrow_mut();
            b.formats.push(o.clone());
            b.trace.push(Trace::Format(snap(&o)));
            if b.name == "format-reject" {
                return Err(b.error.clone());
            }
            if b.name == "format-appends" && b.formats.len() == 1 {
                let EventValue::Array(a) = &b.orders else {
                    panic!()
                };
                EventValue::Array(a.clone()).push(added_order()).unwrap()
            }
            if b.name == "format-replaces-next" && b.formats.len() == 1 {
                let EventValue::Array(a) = &b.orders else {
                    panic!()
                };
                let replaced = decoded(fixture()["orders"][1].clone());
                replaced.write("id", string("replaced"));
                EventValue::Array(a.clone()).set("1", replaced).unwrap();
            }
            let id = o.read("id")?.text()?;
            let regular = record([
                ("symbol", o.read("symbol")?),
                ("price", o.read("price")?),
                ("quantity", o.read("quantity")?),
                ("newClientOrderId", string(format!("r-{id}"))),
                ("nested", o.read("nested")?),
            ]);
            let algo = EventValue::record_object(regular.spread());
            algo.write("newClientOrderId", string(format!("a-{id}")));
            let out = if b.venue == Venue::Futures {
                record([
                    ("regular", array(vec![regular])),
                    ("algo", array(vec![algo])),
                ])
            } else if b.name == "flat-value" {
                regular
            } else {
                array(vec![regular])
            };
            b.format_outputs.push(out.clone());
            Ok(out)
        }))
    }
    fn capture_method(&mut self, m: Method) -> Result<Operation, Failure> {
        let s = self.0.clone();
        let version = {
            let mut b = s.borrow_mut();
            let v = b.version;
            b.trace.push(Trace::CaptureMethod(m, v));
            v
        };
        Ok(Rc::new(move |arg| {
            let mut b = s.borrow_mut();
            if m == Method::Cancel {
                let rows = values(
                    &arg,
                    if b.name == "adjust-mutates" { 1 } else { 2 },
                    "native configured cancel rows",
                );
                let originals = if let EventValue::Array(a) = &b.conflicts {
                    array_values(a).expect("fixture array reads")
                } else {
                    panic!()
                };
                b.cancel_refs = Some(rows.iter().all(|o| originals.iter().any(|p| o.same(p))))
            }
            b.trace.push(Trace::Call(m, version, snap(&arg)));
            b.methods.push(MethodReceipt {
                method: m,
                version,
                arg: arg.clone(),
                sent: snap(&arg),
            });
            if b.name == "batch-sync" {
                return Err(b.error.clone());
            }
            let s = s.clone();
            Ok(Box::pin(async move {
                let mut b = s.borrow_mut();
                b.trace.push(Trace::Settle(m, version));
                if matches!(b.name.as_str(), "batch-reject" | "cancel-reject") {
                    return Err(b.error.clone());
                }
                if b.name == "current-algo" && m == Method::RegularBatch {
                    b.version = 2
                }
                Ok(match m {
                    Method::RegularBatch => array(vec![string("regular-ID")]),
                    Method::AlgoBatch => array(vec![string("algo-ID")]),
                    _ => b.returned.clone(),
                })
            }))
        }))
    }
    fn capture_post(&mut self, venue: Venue) -> Result<Post, Failure> {
        let s = self.0.clone();
        {
            let mut b = s.borrow_mut();
            assert_eq!(venue, b.venue, "native POST venue");
            b.trace.push(Trace::Capture("post"));
        }
        Ok(Rc::new(move |ep, payload| {
            let mut b = s.borrow_mut();
            b.trace.push(Trace::Post(ep, snap(&payload)));
            b.posts.push(PostReceipt {
                endpoint: ep,
                arg: payload.clone(),
                sent: snap(&payload),
            });
            let n = b.posts.len();
            if b.name == "post-sync" && n == 1 {
                return Err(b.error.clone());
            }
            let s = s.clone();
            Ok(Box::pin(async move {
                let mut b = s.borrow_mut();
                b.trace.push(Trace::PostSettle(n));
                if (matches!(b.name.as_str(), "post-reject" | "post-only-reject")
                    || (b.name == "emit-reject" && b.family != "regular"))
                    && n == 1
                {
                    return Err(b.error.clone());
                }
                if b.name == "append-payloads" && n == 1 {
                    append_payload(&b)
                }
                if b.name == "mutate-client-ids" {
                    payload.write("newClientOrderId", string(format!("late-{n}")))
                }
                Ok(if b.family == "regular" {
                    if ep.ends_with("batchOrders") {
                        if n == 1 {
                            b.data.clone()
                        } else {
                            array(vec![
                                record([("orderId", number(91.0))]),
                                record([("orderId", number(92.0))]),
                            ])
                        }
                    } else {
                        record([("orderId", number(77.0))])
                    }
                } else if b.family == "algo" {
                    match b.name.as_str() {
                        "data-null" => EventValue::Null,
                        "missing-id" => record([]),
                        "noncall-id" => record([("algoId", record([("toString", number(0.0))]))]),
                        _ => record([(
                            "algoId",
                            match n {
                                1 => string("9007199254740993"),
                                2 => EventValue::Null,
                                3 => number(42.0),
                                _ => string(format!("algo-{n}")),
                            },
                        )]),
                    }
                } else {
                    record([("ignored", number(99.0))])
                })
            }))
        }))
    }
    fn capture_emit(&mut self) -> Result<Sync, Failure> {
        let s = self.0.clone();
        s.borrow_mut().trace.push(Trace::Capture("emitAndLogError"));
        Ok(Rc::new(move |v| {
            let mut b = s.borrow_mut();
            b.trace.push(Trace::Emit(snap(&v)));
            b.emissions.push(snap(&v));
            if b.name == "emit-reject" && b.emissions.len() == 1 {
                return Err(b.replacement.clone());
            }
            Ok(EventValue::Undefined)
        }))
    }
    fn capture_post_only(&mut self) -> Result<Classifier, Failure> {
        let s = self.0.clone();
        s.borrow_mut()
            .trace
            .push(Trace::Capture("isPostOnlyRejectionError"));
        Ok(Rc::new(move |v| {
            let mut b = s.borrow_mut();
            b.trace.push(Trace::Classify(v.clone()));
            b.classifications.push(v.clone());
            if b.name == "classifier-reject" && b.classifications.len() == 1 {
                return Err(b.error.clone());
            }
            if b.name == "mutate-response" && b.classifications.len() == 1 {
                let EventValue::Array(a) = &b.data else {
                    panic!()
                };
                native_array_fixture::slots(&a)[4]
                    .as_ref()
                    .unwrap()
                    .write("msg", string("changed row error"));
                EventValue::Array(a.clone())
                    .push(record([("orderId", string("appended"))]))
                    .unwrap();
            }
            let input = match &v {
                Rejection::Row(row) => classifier_input(row),
                Rejection::Error(e) => {
                    let mut fields = serde_json::Map::new();
                    fields.insert("message".into(), json!(e.message));
                    if let Some(c) = &observed_error_code(&e.code) {
                        fields.insert("code".into(), c.clone());
                    }
                    if let Some(r) = &e.response {
                        fields.insert("response".into(), r.clone());
                    }
                    Value::Object(fields)
                }
            };
            safe_cex_venue_binance::order_logic::is_post_only_rejection_error(Some(&input)).map_err(
                |e| {
                    let mut x = ExchangeError::new(e.message);
                    x.name = e.name.into();
                    Rc::new(x)
                },
            )
        }))
    }
    fn stringify(&mut self, v: EventValue) -> Result<EventValue, Failure> {
        let mut b = self.0.borrow_mut();
        b.trace.push(Trace::Stringify(snap(&v)));
        b.stringifies.push(v.clone());
        if b.name == "stringify-reject" {
            return Err(b.error.clone());
        }
        let encoded = js_json_stringify(&encoded_input(&v)).unwrap().unwrap();
        Ok(string(encoded))
    }
    fn positions(&mut self) -> Result<EventValue, Failure> {
        let mut b = self.0.borrow_mut();
        b.position_reads += 1;
        b.trace.push(Trace::Positions);
        if b.name == "positions-reject" {
            return Err(b.error.clone());
        }
        if b.name == "position-get-mutates" {
            b.payloads.read("1")?.write("side", string("BUY"))
        }
        Ok(b.positions.clone())
    }
    fn market(&mut self, _opts: &EventValue) -> Result<EventValue, Failure> {
        let mut b = self.0.borrow_mut();
        b.market_reads += 1;
        b.trace.push(Trace::Markets);
        Ok(b.market.clone())
    }
    fn adjust(&mut self, value: EventValue, precision: EventValue) -> Result<EventValue, Failure> {
        let mut b = self.0.borrow_mut();
        b.adjusts.push((value.clone(), precision.clone()));
        b.trace
            .push(Trace::Adjust(value.clone(), precision.clone()));
        if b.name == "adjust-reject" {
            return Err(b.error.clone());
        }
        if b.name == "adjust-mutates" {
            b.opts.write(
                "type",
                EventValue::enum_value(PrivateEnum::Type(OrderType::TakeProfit)),
            );
            b.opts.write(
                "side",
                EventValue::enum_value(PrivateEnum::Side(OrderSide::Buy)),
            );
        }
        let p = match precision {
            EventValue::String(s) => PrecisionInput::String(String::from_utf16(&s).unwrap()),
            v => PrecisionInput::Number(v.number()?),
        };
        Ok(number(safe_cex_num::adjust(value.number()?, p)))
    }
    fn orders(&mut self) -> Result<EventValue, Failure> {
        let mut b = self.0.borrow_mut();
        b.order_reads += 1;
        b.trace.push(Trace::Orders);
        if b.name == "orders-reject" {
            return Err(b.error.clone());
        }
        Ok(b.conflicts.clone())
    }
    fn capture_log(&mut self) -> Result<Log, Failure> {
        let s = self.0.clone();
        s.borrow_mut().trace.push(Trace::Capture("log"));
        Ok(Rc::new(move |msg, severity| {
            let mut b = s.borrow_mut();
            b.logs.push((msg.clone(), severity));
            b.trace.push(Trace::Log(msg, severity));
            if b.name == "log-reject" {
                return Err(b.error.clone());
            }
            Ok(())
        }))
    }
}
fn generated(e: &Failure, b: &State, msg: &str, l: &str) {
    assert!(
        !Rc::ptr_eq(e, &b.error) && !Rc::ptr_eq(e, &b.replacement),
        "{l}: native generated Error identity"
    );
    assert_eq!(e.name, "TypeError", "{l}: native generated name");
    assert_eq!(e.message, msg, "{l}: native generated message");
    assert!(
        e.code.is_none() && e.response.is_none(),
        "{l}: native generated metadata"
    );
}
fn original(e: &Failure, b: &State, replaced: bool, l: &str) {
    let target = if replaced { &b.replacement } else { &b.error };
    assert!(Rc::ptr_eq(e, target), "{l}: native original Error Rc");
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
            "batch fixture"
        },
        "{l}"
    );
    assert_eq!(
        observed_error_code(&e.code),
        Some(if replaced {
            json!("emit-code")
        } else {
            json!(if b.name == "post-only-reject" {
                -5022
            } else {
                -1000
            })
        }),
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
fn error_wire(e: &Failure, b: &State) -> Value {
    json!({"original":Rc::ptr_eq(e,&b.error),"replacement":Rc::ptr_eq(e,&b.replacement),"name":e.name,"message":e.message,"code":observed_error_code(&e.code).clone().unwrap_or(json!({"$undefined":true})),"response":e.response.clone().unwrap_or(json!({"$undefined":true}))})
}
impl Trace {
    fn wire(&self, b: &State) -> Value {
        match self {
            Self::Capture(k) => json!(["capture", k]),
            Self::CaptureMethod(m, v) => json!(["capture", key(*m), v]),
            Self::Assert(o) => json!(["assert", wire(o)]),
            Self::Format(o) => json!(["format", wire(o)]),
            Self::Call(m, v, o) => json!(["call", key(*m), v, wire(o)]),
            Self::Settle(m, v) => json!(["settle", key(*m), v]),
            Self::Post(ep, o) => json!(["post", ep, wire(o)]),
            Self::PostSettle(n) => json!(["post_settle", n]),
            Self::Stringify(o) => json!(["stringify", wire(o)]),
            Self::Positions => json!(["positions"]),
            Self::Markets => json!(["markets"]),
            Self::Adjust(v, p) => json!(["adjust", wire(v), wire(p)]),
            Self::Orders => json!(["orders"]),
            Self::Classify(v) => match v {
                Rejection::Row(v) => json!(["classify", "row", wire(v)]),
                Rejection::Error(e) => json!(["classify", "error", error_wire(e, b)]),
            },
            Self::Emit(v) => json!(["emit", wire(v)]),
            Self::Log(msg, severity) => json!(["log", msg, severity]),
        }
    }
}
fn state_wire(b: &State, result: &Result<EventValue, Failure>) -> Value {
    json!({"name":b.name,"trace":b.trace.iter().map(|t|t.wire(b)).collect::<Vec<_>>(),"posts":b.posts.iter().map(|p|json!({"endpoint":p.endpoint,"payload":wire(&p.sent)})).collect::<Vec<_>>(),"emissions":b.emissions.iter().map(wire).collect::<Vec<_>>(),"classifications":b.classifications.iter().map(|v|match v{Rejection::Row(v)=>{let EventValue::Array(a)=&b.data else{panic!()};json!({"kind":"row","original":array_values(&a).expect("fixture array reads").iter().any(|o|o.same(v)),"value":wire(v)})},Rejection::Error(e)=>json!({"kind":"error","original":Rc::ptr_eq(e,&b.error)||Rc::ptr_eq(e,&b.replacement),"value":error_wire(e,b)})}).collect::<Vec<_>>(),"logs":b.logs.iter().map(|(m,s)|json!({"message":m,"severity":s})).collect::<Vec<_>>(),"methods":b.methods.iter().map(|m|json!({"method":key(m.method),"version":m.version,"args":wire(&m.sent)})).collect::<Vec<_>>(),"formats":b.formats.iter().map(wire).collect::<Vec<_>>(),"stringifies":b.stringifies.iter().map(wire).collect::<Vec<_>>(),"positions":b.position_reads,"markets":b.market_reads,"adjusts":b.adjusts.len(),"orderReads":b.order_reads,"assertRefs":b.assert_refs,"cancelRefs":b.cancel_refs,"returnedRef":result.as_ref().is_ok_and(|v|v.same(&b.returned)),"value":result.as_ref().map(wire).unwrap_or(json!({"$undefined":true})),"failure":result.as_ref().err().map(|e|error_wire(e,b))})
}
fn compare(family: &str, actual: Vec<Value>, floor: usize) {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let p = Command::new("node")
        .current_dir(repo)
        .arg("sdk-rs/safe-cex-venue-binance/scripts/order-batches.mjs")
        .arg(family)
        .output()
        .unwrap();
    assert!(
        p.status.success(),
        "source oracle failure {}",
        String::from_utf8_lossy(&p.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&p.stdout).unwrap();
    assert_eq!(source.len(), floor, "source authored case floor");
    assert_eq!(actual.len(), floor, "native authored case floor");
    for (i, (a, b)) in actual.iter().zip(source).enumerate() {
        assert!(
            equal(a, &b),
            "source {family} case{i}: native {a} source {b}"
        );
    }
}
fn failed_native(
    result: &Result<EventValue, Failure>,
    b: &State,
    generated_message: Option<&str>,
    l: &str,
) {
    if let Err(e) = result {
        if let Some(msg) = generated_message {
            generated(e, b, msg, l)
        } else {
            original(e, b, b.name == "emit-reject", l)
        }
    }
}
fn ids_native(v: &EventValue, expected: Vec<Option<String>>, l: &str) {
    let rows = values(v, expected.len(), &format!("{l}: native ID count"));
    for (i, (v, expected)) in rows.iter().zip(expected).enumerate() {
        if let Some(id) = expected {
            text(v, &id, &format!("{l}: native ID{i}"))
        } else {
            assert!(
                matches!(v, EventValue::Undefined),
                "{l}: native own undefined ID{i}"
            )
        }
    }
}
fn regular_counts(name: &str) -> (usize, usize, usize, usize, bool) {
    match name {
        "empty" => (0, 0, 0, 0, false),
        "one" => (1, 0, 0, 1, false),
        "seven" => (2, 2, 1, 5, false),
        "post-reject" | "post-sync" | "bad-foreach" | "null-row" | "noncall-id" => {
            (1, 1, 1, 0, true)
        }
        "post-only-reject" => (1, 1, 0, 0, true),
        "data-null" | "data-object" => (2, 0, 0, 1, false),
        "code-string-zero" => (2, 1, 1, 1, false),
        "missing-id" => (2, 0, 0, 2, false),
        "classifier-reject" => (1, 2, 1, 0, true),
        "emit-reject" => (1, 3, 2, 0, true),
        "stringify-reject" => (0, 1, 1, 0, true),
        _ => (2, 2, 1, 4, false),
    }
}
fn pm_positions(name: &str) -> usize {
    match name {
        "empty" | "null-payload" | "null-input" => 0,
        "positions-reject" | "raw-quantity-array" => 1,
        "nonnumeric-quantity" => 3,
        _ => 2,
    }
}
fn http_test(family: &str) {
    let f = fixture();
    let floor = match family {
        "regular" => 19,
        "algo" => 10,
        "pm" => 16,
        "spot" => 10,
        _ => panic!(),
    };
    assert_eq!(f["cases"][family].as_array().unwrap().len(), floor);
    let mut actual = vec![];
    for (i, name) in f["cases"][family].as_array().unwrap().iter().enumerate() {
        let name = name.as_str().unwrap();
        let s = state(family, name);
        let payloads = s.borrow().payloads.clone();
        let mut b = Bindings(s.clone());
        let result = settle(async {
            match family {
                "regular" => regular_futures(payloads, &mut b).await,
                "algo" => algo_futures(payloads, &mut b).await,
                "pm" => batch_pm(payloads, &mut b).await,
                _ => batch_spot(payloads, &mut b).await,
            }
        });
        let b = s.borrow();
        let l = format!("native {family} case{i} {name}");
        let (posts, classes, emits, ids, failed) = if family == "regular" {
            regular_counts(name)
        } else {
            let failed = matches!(
                name,
                "post-reject" | "post-sync" | "post-only-reject" | "emit-reject"
            ) || (family == "algo" && matches!(name, "data-null" | "noncall-id"))
                || (family == "pm"
                    && matches!(name, "null-payload" | "positions-reject" | "null-input"))
                || (family == "spot" && name == "null-payload");
            let posts = if name == "empty"
                || (family == "pm"
                    && matches!(name, "null-payload" | "positions-reject" | "null-input"))
                || (family == "spot" && name == "null-input")
            {
                0
            } else if failed {
                1
            } else if family == "algo" && name == "append-payloads" {
                7
            } else {
                6
            };
            let emits = usize::from(
                failed
                    && !(family == "pm"
                        && matches!(name, "null-payload" | "positions-reject" | "null-input")),
            );
            (posts, 0, emits, if failed { 0 } else { posts }, failed)
        };
        assert_eq!(b.posts.len(), posts, "{l}: native POST count");
        assert_eq!(
            b.classifications.len(),
            classes,
            "{l}: native classifier count"
        );
        assert_eq!(b.emissions.len(), emits, "{l}: native emit count");
        assert_eq!(result.is_err(), failed, "{l}: native outcome");
        assert!(
            b.methods.is_empty()
                && b.formats.is_empty()
                && b.market_reads == 0
                && b.adjusts.is_empty()
                && b.order_reads == 0
                && b.logs.is_empty(),
            "{l}: native HTTP family scope"
        );
        assert_eq!(
            b.position_reads,
            if family == "pm" {
                pm_positions(name)
            } else {
                0
            },
            "{l}: native position getter count"
        );
        let generated_msg = match (family, name) {
            ("regular", "bad-foreach") => Some("data?.forEach is not a function"),
            ("regular", "null-row") => Some("Cannot read properties of null (reading 'code')"),
            ("regular", "noncall-id") => Some("o.orderId?.toString is not a function"),
            ("algo", "data-null") => Some("Cannot read properties of null (reading 'algoId')"),
            ("algo", "noncall-id") => Some("data.algoId?.toString is not a function"),
            ("pm", "null-payload") => Some("Cannot read properties of null (reading 'type')"),
            ("pm", "null-input") => Some("payloads is not iterable"),
            ("spot", "null-payload") => {
                Some("Cannot read properties of null (reading 'newClientOrderId')")
            }
            _ => None,
        };
        failed_native(&result, &b, generated_msg, &l);
        if let Ok(v) = &result {
            assert!(!v.same(&b.returned), "{l}: native fresh HTTP result");
            let expected = if family == "regular" {
                match name {
                    "empty" => vec![],
                    "one" | "data-null" | "data-object" | "code-string-zero" => {
                        vec![Some("77".into())]
                    }
                    "missing-id" => vec![None, Some("77".into())],
                    "seven" => vec![
                        Some("9007199254740993".into()),
                        Some("42".into()),
                        None,
                        Some("91".into()),
                        Some("92".into()),
                    ],
                    _ => vec![
                        Some("9007199254740993".into()),
                        Some("42".into()),
                        None,
                        Some("77".into()),
                    ],
                }
            } else if family == "algo" {
                (1..=ids)
                    .map(|n| {
                        if name == "missing-id" || n == 2 {
                            None
                        } else if n == 1 {
                            Some("9007199254740993".into())
                        } else if n == 3 {
                            Some("42".into())
                        } else {
                            Some(format!("algo-{n}"))
                        }
                    })
                    .collect()
            } else {
                let order = if family == "pm" {
                    vec![0, 2, 5, 1, 3, 4]
                } else {
                    vec![0, 1, 2, 3, 4, 5]
                };
                order
                    .into_iter()
                    .take(ids)
                    .enumerate()
                    .map(|(n, id)| {
                        if family == "spot" && name == "missing-client-id" && id == 0 {
                            None
                        } else {
                            Some(if name == "mutate-client-ids" {
                                format!("late-{}", n + 1)
                            } else {
                                format!("c{id}")
                            })
                        }
                    })
                    .collect()
            };
            assert_eq!(expected.len(), ids, "authored expected ID floor");
            ids_native(v, expected, &l);
        }
        let strings = if family == "regular" {
            if matches!(name, "empty" | "one") {
                0
            } else if name == "seven" {
                2
            } else {
                1
            }
        } else {
            0
        };
        assert_eq!(b.stringifies.len(), strings, "{l}: native stringify count");
        for (j, lot) in b.stringifies.iter().enumerate() {
            let rows = values(
                lot,
                if j == 0 { 5 } else { 2 },
                &format!("{l}: native serialized lot{j}"),
            );
            for row in rows {
                num(
                    &row.read("price").unwrap(),
                    12.3,
                    &format!("{l}: native codec input price"),
                );
                assert!(
                    matches!(
                        row.read("quantity").unwrap(),
                        EventValue::String(_) | EventValue::Number(_)
                    ),
                    "{l}: native codec input quantity kind"
                );
                let EventValue::Array(original) = &b.payloads else {
                    panic!()
                };
                assert!(
                    array_values(&original)
                        .expect("fixture array reads")
                        .iter()
                        .any(|p| p.same(&row)),
                    "{l}: native codec original payload referent"
                );
            }
        }
        for (j, p) in b.posts.iter().enumerate() {
            let expected = match family {
                "regular" => {
                    if name == "one" || (j == 1 && name != "seven") {
                        "/fapi/v1/order"
                    } else {
                        "/fapi/v1/batchOrders"
                    }
                }
                "algo" => "/fapi/v1/algoOrder",
                "pm" => {
                    if j < 3 {
                        "/papi/v1/um/order"
                    } else {
                        "/papi/v1/um/conditional/order"
                    }
                }
                _ => "/api/v3/order",
            };
            assert_eq!(p.endpoint, expected, "{l}: native endpoint{j}");
            if family == "regular" && p.endpoint.ends_with("batchOrders") {
                assert_eq!(p.arg.spread().len(), 1, "{l}: native batch param count");
                assert!(
                    matches!(p.arg.read("batchOrders").unwrap(), EventValue::String(_)),
                    "{l}: native encoded batch String"
                );
                let EventValue::String(encoded) = p.arg.read("batchOrders").unwrap() else {
                    panic!("native encoded String")
                };
                // Decode ONCE and assert authored native fields; never re-encode.
                let decoded_batch = EventValue::from_json(
                    serde_json::from_str(&String::from_utf16(&encoded).unwrap())
                        .expect("native batchOrders valid JSON"),
                );
                let rows = values(
                    &decoded_batch,
                    if j == 0 { 5 } else { 2 },
                    &format!("{l}: native decoded batch count"),
                );
                for (k, row) in rows.iter().enumerate() {
                    let index = if j == 0 { k } else { k + 5 };
                    assert_eq!(row.spread().len(), 7, "{l}: native decoded field count");
                    text(&row.read("symbol").unwrap(), "BTCUSDT", &l);
                    text(
                        &row.read("type").unwrap(),
                        [
                            "LIMIT",
                            "STOP_MARKET",
                            "LIMIT",
                            "TAKE_PROFIT_MARKET",
                            "TRAILING_STOP_MARKET",
                            "LIMIT",
                            "LIMIT",
                        ][index],
                        &format!("{l}: native decoded type"),
                    );
                    text(
                        &row.read("side").unwrap(),
                        if index == 1 { "SELL" } else { "BUY" },
                        &l,
                    );
                    num(
                        &row.read("price").unwrap(),
                        12.3,
                        &format!("{l}: native decoded price"),
                    );
                    if index == 4 {
                        num(
                            &row.read("quantity").unwrap(),
                            0.0,
                            &format!("{l}: native decoded quantity"),
                        )
                    } else {
                        text(
                            &row.read("quantity").unwrap(),
                            if index == 1 {
                                "0"
                            } else if index == 3 {
                                "2.5"
                            } else {
                                "3.6"
                            },
                            &format!("{l}: native decoded quantity"),
                        )
                    };
                    text(
                        &row.read("newClientOrderId").unwrap(),
                        &format!("c{index}"),
                        &format!("{l}: native decoded client ID"),
                    );
                    num(
                        &row.read("nested").unwrap().read("marker").unwrap(),
                        4.2,
                        &format!("{l}: native decoded nested marker"),
                    );
                }
                continue;
            }
            if name == "null-payload" {
                assert!(
                    matches!(p.arg, EventValue::Null),
                    "{l}: native raw null POST body"
                );
                continue;
            }
            num(
                &p.arg.read("price").unwrap(),
                12.3,
                &format!("{l}: native POST price{j}"),
            );
            let original_index = if family == "pm" {
                [0, 2, 5, 1, 3, 4][j]
            } else if family == "regular" {
                if j == 0 { 0 } else { 5 }
            } else {
                j
            };
            let original = b.payloads.get(&original_index.to_string()).unwrap();
            if family == "pm" && j >= 3 {
                assert!(!p.arg.same(&original), "{l}: native fresh conditional body");
                assert!(!own(&p.arg, "type"), "{l}: native conditional removed type");
                text(
                    &p.arg.read("strategyType").unwrap(),
                    if name == "strategy-override" && j == 3 {
                        "OVERRIDE"
                    } else {
                        ["STOP_MARKET", "TAKE_PROFIT_MARKET", "TRAILING_STOP_MARKET"][j - 3]
                    },
                    &format!("{l}: native strategyType"),
                );
                let q = p.arg.read("quantity").unwrap();
                if name == "raw-quantity-array" && j == 3 {
                    assert!(
                        q.same(&original.read("quantity").unwrap()),
                        "{l}: native raw quantity array identity"
                    );
                    num(&values(&q, 1, &l)[0], 3.6, &l)
                } else if j == 4 && name != "nonnumeric-quantity" {
                    text(&q, "2.5", &format!("{l}: native truthy raw quantity"))
                } else if matches!(name, "no-positions" | "zero-contracts") {
                    num(&q, 0.0, &format!("{l}: native fallback quantity{j}"))
                } else if j == 3 && name != "position-get-mutates" {
                    text(&q, "4.2", &format!("{l}: native fallback quantity{j}"))
                } else {
                    num(&q, 3.6, &format!("{l}: native fallback quantity{j}"))
                };
                if name == "position-get-mutates" && j == 3 {
                    text(&original.read("side").unwrap(), "BUY", &l);
                    text(
                        &p.arg.read("side").unwrap(),
                        "SELL",
                        &format!("{l}: native side snapshot"),
                    );
                }
            } else {
                assert!(
                    p.arg.same(&original),
                    "{l}: native original POST payload{j}"
                );
            }
            assert!(
                p.arg
                    .read("nested")
                    .unwrap()
                    .same(&original.read("nested").unwrap()),
                "{l}: native nested payload alias{j}"
            );
            if name == "mutate-client-ids" {
                text(
                    &p.arg.read("newClientOrderId").unwrap(),
                    &format!("late-{}", j + 1),
                    &format!("{l}: native late client ID{j}"),
                );
            }
        }
        for r in &b.classifications {
            if let Rejection::Row(row) = r {
                let EventValue::Array(a) = &b.data else {
                    panic!()
                };
                assert!(
                    array_values(&a)
                        .expect("fixture array reads")
                        .iter()
                        .any(|o| o.same(row)),
                    "{l}: native classifier original row"
                );
                assert!(
                    matches!(
                        row.read("code").unwrap(),
                        EventValue::Number(_) | EventValue::String(_)
                    ),
                    "{l}: native classifier code kind"
                );
            } else if let Rejection::Error(e) = r {
                let returned = result
                    .as_ref()
                    .expect_err("authored classifier Error cases reject");
                assert!(
                    Rc::ptr_eq(e, returned),
                    "{l}: native classifier caught Error identity"
                );
                if let Some(msg) = generated_msg {
                    generated(e, &b, msg, &l)
                } else {
                    original(e, &b, name == "emit-reject", &l)
                }
            }
        }
        for (j, m) in b.emissions.iter().enumerate() {
            let expected = if let Some(g) = generated_msg {
                g
            } else if name == "emit-reject" {
                if family == "regular" && j == 0 {
                    "row error"
                } else if family == "regular" {
                    "emit detail"
                } else {
                    "venue detail"
                }
            } else if matches!(
                name,
                "post-reject"
                    | "post-sync"
                    | "post-only-reject"
                    | "classifier-reject"
                    | "stringify-reject"
            ) {
                "venue detail"
            } else if name == "code-string-zero" {
                "string zero"
            } else if name == "mutate-response" {
                "changed row error"
            } else {
                "row error"
            };
            text(m, expected, &format!("{l}: native emission{j}"));
        }
        if name == "mutate-response" {
            values(&b.data, 6, &format!("{l}: native appended response floor"));
        }
        if name == "append-payloads" {
            values(&b.payloads, 7, &format!("{l}: native appended input floor"));
        }
        actual.push(state_wire(&b, &result));
    }
    compare(family, actual, floor);
}
fn many_test(family: &str) {
    let f = fixture();
    assert_eq!(f["cases"]["many"].as_array().unwrap().len(), 12);
    let mut actual = vec![];
    for (i, name) in f["cases"]["many"].as_array().unwrap().iter().enumerate() {
        let name = name.as_str().unwrap();
        let s = state(family, name);
        let orders = s.borrow().orders.clone();
        let mut bindings = Bindings(s.clone());
        let result = settle(async {
            match family {
                "many_futures" => many_futures(orders, &mut bindings).await,
                "many_pm" => many_pm(orders, &mut bindings).await,
                _ => many_spot(orders, &mut bindings).await,
            }
        });
        let b = s.borrow();
        let l = format!("native {family} case{i} {name}");
        let futures = family == "many_futures";
        assert_eq!(
            b.assert_refs,
            Some(true),
            "{l}: native original assertion orders"
        );
        let formats = match name {
            "empty" | "assert-reject" | "null-input" => 0,
            "format-reject" => 1,
            "assert-appends" => 3,
            "format-appends" if futures => 3,
            _ => 2,
        };
        assert_eq!(b.formats.len(), formats, "{l}: native formatter count");
        let methods = if matches!(name, "assert-reject" | "format-reject" | "null-input")
            || (futures && name == "empty")
        {
            vec![]
        } else if futures {
            if matches!(name, "batch-reject" | "batch-sync") {
                vec![Method::RegularBatch]
            } else {
                vec![Method::RegularBatch, Method::AlgoBatch]
            }
        } else {
            vec![Method::Batch]
        };
        assert_eq!(
            b.methods.iter().map(|m| m.method).collect::<Vec<_>>(),
            methods,
            "{l}: native batch method order/count"
        );
        let failed = matches!(
            name,
            "assert-reject" | "format-reject" | "batch-reject" | "batch-sync" | "null-input"
        );
        assert_eq!(result.is_err(), failed, "{l}: native outcome");
        failed_native(
            &result,
            &b,
            if name == "null-input" {
                Some(if futures {
                    "orders is not iterable"
                } else {
                    "Cannot read properties of null (reading 'flatMap')"
                })
            } else {
                None
            },
            &l,
        );
        if let Ok(result) = &result {
            if futures {
                assert!(!result.same(&b.returned), "{l}: native fresh aggregate");
                let rows = values(
                    result,
                    if name == "empty" { 0 } else { 2 },
                    &format!("{l}: native result count"),
                );
                if !rows.is_empty() {
                    text(&rows[0], "regular-ID", &l);
                    text(&rows[1], "algo-ID", &l)
                }
            } else {
                assert!(
                    result.same(&b.returned),
                    "{l}: native original batch result"
                );
                let rows = values(result, 3, &l);
                text(&rows[0], "return-A", &l);
                assert!(
                    matches!(rows[1], EventValue::Null),
                    "{l}: native raw null result"
                );
                text(&rows[2], "return-C", &l);
            }
        }
        for (j, m) in b.methods.iter().enumerate() {
            assert_eq!(
                m.version,
                if futures && name == "current-algo" && j == 1 {
                    2
                } else {
                    1
                },
                "{l}: native current method version"
            );
            let rows = values(&m.arg, formats, &format!("{l}: native batch payload count"));
            let kind = if m.method == Method::AlgoBatch {
                "algo"
            } else {
                "regular"
            };
            for (k, p) in rows.iter().enumerate() {
                let original = b.formats[k].clone();
                let id = if k == 0 {
                    "A"
                } else if k == 1 && name == "format-replaces-next" {
                    "replaced"
                } else if k == 1 {
                    "B"
                } else {
                    "added"
                };
                text(
                    &original.read("id").unwrap(),
                    id,
                    &format!("{l}: native reread input ID{k}"),
                );
                text(
                    &p.read("newClientOrderId").unwrap(),
                    &format!("{}-{id}", if kind == "algo" { "a" } else { "r" }),
                    &format!("{l}: native batch client ID{k}"),
                );
                num(
                    &p.read("price").unwrap(),
                    if k == 1 { 12.4 } else { 12.3 },
                    &format!("{l}: native payload price{k}"),
                );
                num(
                    &p.read("quantity").unwrap(),
                    if k == 1 { 4.2 } else { 3.6 },
                    &format!("{l}: native payload quantity{k}"),
                );
                assert!(
                    p.read("nested")
                        .unwrap()
                        .same(&original.read("nested").unwrap()),
                    "{l}: native nested input alias{k}"
                );
                let formatted = &b.format_outputs[k];
                let expected = if futures {
                    formatted.get(kind).unwrap().read("0").unwrap()
                } else if name == "flat-value" {
                    formatted.clone()
                } else {
                    formatted.read("0").unwrap()
                };
                assert!(
                    p.same(&expected),
                    "{l}: native original formatted payload{k}"
                );
            }
        }
        if name == "format-appends" || name == "assert-appends" {
            values(&b.orders, 3, &l);
        }
        assert!(
            b.posts.is_empty()
                && b.emissions.is_empty()
                && b.classifications.is_empty()
                && b.logs.is_empty()
                && b.adjusts.is_empty()
                && b.position_reads == 0
                && b.market_reads == 0
                && b.order_reads == 0,
            "{l}: native multi-entry scope"
        );
        actual.push(state_wire(&b, &result));
    }
    compare(family, actual, 12);
}
fn conflict_test() {
    let f = fixture();
    assert_eq!(f["cases"]["conflict"].as_array().unwrap().len(), 15);
    let mut actual = vec![];
    for (i, name) in f["cases"]["conflict"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let name = name.as_str().unwrap();
        let s = state("conflict", name);
        let opts = s.borrow().opts.clone();
        let result = settle(cancel_close(opts, &mut Bindings(s.clone())));
        let b = s.borrow();
        let l = format!("native conflict case{i} {name}");
        let (markets, adjusts, reads, logs, cancels) = match name {
            "non-stop" | "null-opts" => (0, 0, 0, 0, 0),
            "no-market" => (1, 0, 0, 0, 0),
            "sized" | "adjust-reject" => (1, 1, 0, 0, 0),
            "no-matches" | "orders-reject" => (1, 1, 1, 0, 0),
            "log-reject" => (1, 1, 1, 1, 0),
            _ => (1, 1, 1, 1, 1),
        };
        assert_eq!(b.market_reads, markets, "{l}: native market count");
        assert_eq!(b.adjusts.len(), adjusts, "{l}: native adjust count");
        assert_eq!(b.order_reads, reads, "{l}: native orders count");
        assert_eq!(b.logs.len(), logs, "{l}: native Info log count");
        assert_eq!(b.methods.len(), cancels, "{l}: native cancel count");
        assert_eq!(
            result.is_err(),
            matches!(
                name,
                "log-reject" | "cancel-reject" | "orders-reject" | "adjust-reject" | "null-opts"
            ),
            "{l}: native outcome"
        );
        failed_native(
            &result,
            &b,
            if name == "null-opts" {
                Some("Cannot read properties of null (reading 'type')")
            } else {
                None
            },
            &l,
        );
        if let Ok(v) = &result {
            assert!(
                matches!(v, EventValue::Undefined),
                "{l}: native undefined return"
            )
        }
        if adjusts > 0 {
            let (v, p) = &b.adjusts[0];
            if name == "raw-amount-string" {
                text(v, "0", &l)
            } else {
                num(
                    v,
                    if name == "sized" {
                        3.6
                    } else if name == "adjust-zero" {
                        0.04
                    } else {
                        0.0
                    },
                    &format!("{l}: native raw adjust value"),
                )
            };
            if name == "precision-string" {
                text(p, "0.1", &l)
            } else {
                num(p, 0.1, &format!("{l}: native precision"))
            }
        }
        for (msg, severity) in &b.logs {
            assert_eq!(*severity, LogSeverity::Info, "{l}: native severity");
            assert_eq!(
                msg,
                &format!(
                    "Canceling {} existing closePosition order(s) before placing new one",
                    if name == "adjust-mutates" { 1 } else { 2 }
                ),
                "{l}: native log message"
            )
        }
        if cancels > 0 {
            let m = &b.methods[0];
            assert_eq!(
                m.method,
                Method::Cancel,
                "{l}: native current cancel method"
            );
            assert_eq!(
                b.cancel_refs,
                Some(true),
                "{l}: native original matching row references"
            );
            let rows = values(&m.arg, if name == "adjust-mutates" { 1 } else { 2 }, &l);
            for (k, row) in rows.iter().enumerate() {
                text(
                    &row.read("id").unwrap(),
                    if name == "adjust-mutates" {
                        "tp"
                    } else if k == 0 {
                        "close-A"
                    } else {
                        "close-B"
                    },
                    &format!("{l}: native matching ID{k}"),
                );
                assert!(
                    matches!(category::<safe_cex_types::OrderType>(&(row.read("type").unwrap())), t if t==if name=="adjust-mutates"{OrderType::TakeProfit}else{OrderType::StopLoss}),
                    "{l}: native matching type"
                );
                assert!(
                    matches!(category::<safe_cex_types::OrderSide>(&(row.read("side").unwrap())), t if t==if name=="adjust-mutates"{OrderSide::Buy}else{OrderSide::Sell}),
                    "{l}: native matching side"
                );
                assert!(
                    matches!(row.read("close").unwrap(), EventValue::Bool(true)),
                    "{l}: native strict close flag"
                );
                assert!(
                    !row.read("amount").unwrap().truthy(),
                    "{l}: native false raw amount"
                );
            }
        }
        assert!(
            b.posts.is_empty()
                && b.formats.is_empty()
                && b.emissions.is_empty()
                && b.classifications.is_empty()
                && b.position_reads == 0,
            "{l}: native conflict scope"
        );
        actual.push(state_wire(&b, &result));
    }
    compare("conflict", actual, 15);
}
#[test]
#[ignore = "requires TypeScript oracle"]
fn regular() {
    http_test("regular")
}
#[test]
#[ignore = "requires TypeScript oracle"]
fn algo() {
    http_test("algo")
}
#[test]
#[ignore = "requires TypeScript oracle"]
fn pm() {
    http_test("pm")
}
#[test]
#[ignore = "requires TypeScript oracle"]
fn spot() {
    http_test("spot")
}
#[test]
#[ignore = "requires TypeScript oracle"]
fn many_futures_target() {
    many_test("many_futures")
}
#[test]
#[ignore = "requires TypeScript oracle"]
fn many_pm_target() {
    many_test("many_pm")
}
#[test]
#[ignore = "requires TypeScript oracle"]
fn many_spot_target() {
    many_test("many_spot")
}
#[test]
#[ignore = "requires TypeScript oracle"]
fn conflict() {
    conflict_test()
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

#[path = "support/native_array_fixture.rs"]
#[allow(dead_code)] // Different integration binaries consume distinct fixture operations.
mod native_array_fixture;
