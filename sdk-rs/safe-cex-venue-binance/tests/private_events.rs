// Per-case expectation tables. Arms that currently coincide are kept separate
// because each names an independent case; collapsing them merges two
// expectations into one, which is the homogenized-corpus false green the
// cardinality audits in this tree exist to prevent.
#![allow(clippy::if_same_then_else)]
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::ProtoMember;
use safe_cex_types::{
    MarginType, OrderRejectReason, OrderSide, OrderStatus, OrderStopTriggerType, OrderType,
    PositionMode, PositionSide,
};
use safe_cex_venue_binance::{private_event_value::*, private_events::*, request_weight::Failure};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    rc::Rc,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Venue {
    Futures,
    PM,
    Spot,
}
fn venue(v: Venue) -> &'static str {
    match v {
        Venue::Futures => "futures",
        Venue::PM => "pm",
        Venue::Spot => "spot",
    }
}
fn op_name(v: PrivateOperation) -> &'static str {
    use PrivateOperation::*;
    match v {
        Fill => "fill",
        UpdateOrder => "updateOrder",
        AddOrder => "addOrder",
        OrderStatus => "orderStatus",
        ThrottleOrders => "throttleOrders",
        MarkPosition => "markPosition",
        ThrottlePositions => "throttlePositions",
        AddPosition => "addPosition",
        UpdatePosition => "updatePosition",
        UpdateStore => "updateStore",
    }
}
fn encoded(n: f64) -> Value {
    if n.is_nan() {
        json!("nan")
    } else if n == f64::INFINITY {
        json!("infinity")
    } else if n == f64::NEG_INFINITY {
        json!("-infinity")
    } else if n == 0.0 && n.is_sign_negative() {
        json!("-zero")
    } else {
        json!(n)
    }
}
fn norm(v: &EventValue, active: &mut BTreeSet<usize>) -> Value {
    match v {
        EventValue::Undefined => json!({"$undefined":true}),
        EventValue::Null => Value::Null,
        EventValue::Bool(v) => json!(v),
        EventValue::Number(v) => encoded(*v),
        EventValue::String(v) => json!(String::from_utf16(v).unwrap()),
        EventValue::Builtin(reference) => {
            if reference.is_callable() {
                json!({"$function":reference.js_string()})
            } else {
                json!({"$prototype":true})
            }
        }
        EventValue::Array(a) => {
            let key = Rc::as_ptr(a) as usize;
            if !active.insert(key) {
                return json!({"$cycle":true});
            }
            let out = Value::Array(array_values(a).iter().map(|v| norm(v, active)).collect());
            active.remove(&key);
            out
        }
        EventValue::Object(o) => {
            let key = Rc::as_ptr(o) as usize;
            if !active.insert(key) {
                return json!({"$cycle":true});
            }
            let mut out = Value::Object(
                v.spread()
                    .iter()
                    .map(|(k, v)| (k.clone(), norm(v, active)))
                    .collect(),
            );
            match &*o.proto.borrow() {
                None | Some(EventValue::Null) => out["$nullPrototype"] = json!(true),
                Some(EventValue::Builtin(reference))
                    if reference.member()
                        == safe_cex_num::BuiltinIdentity::Object(ProtoMember::Prototype) => {}
                Some(proto) => out["$customPrototype"] = norm(proto, active),
            }
            active.remove(&key);
            out
        }
        _ => panic!("fixture projection requires decoded data"),
    }
}
fn wire(v: &EventValue) -> Value {
    norm(v, &mut BTreeSet::new())
}
fn numeric(v: &Value) -> Value {
    match v {
        Value::Number(v) => json!(v.as_f64().unwrap()),
        Value::Array(v) => Value::Array(v.iter().map(numeric).collect()),
        Value::Object(v) => Value::Object(v.iter().map(|(k, v)| (k.clone(), numeric(v))).collect()),
        _ => v.clone(),
    }
}
#[derive(Clone)]
struct Effect {
    op: PrivateOperation,
    args: Vec<EventValue>,
    pending: bool,
}
#[derive(Clone)]
enum Trace {
    Capture(PrivateOperation),
    Call(PrivateOperation),
    Clock(usize),
    Orders,
    Positions,
    Balance,
    Reject,
    Price(EventValue),
}
struct State {
    f: Value,
    events: EventArray,
    data: EventValue,
    positions: EventArray,
    orders: EventArray,
    balance: EventValue,
    error: Failure,
    flags: Cell<bool>,
    effects: RefCell<Vec<Effect>>,
    trace: RefCell<Vec<Trace>>,
    clocks: Cell<usize>,
    prices: Cell<usize>,
}
struct Bindings(Rc<State>);
impl State {
    fn name(&self) -> &str {
        self.f["name"].as_str().unwrap()
    }
    fn log(&self, t: Trace) {
        self.trace.borrow_mut().push(t)
    }
    fn count(&self, op: PrivateOperation) -> usize {
        self.effects.borrow().iter().filter(|e| e.op == op).count()
    }
    fn effects(&self, op: PrivateOperation) -> Vec<Effect> {
        self.effects
            .borrow()
            .iter()
            .filter(|e| e.op == op)
            .cloned()
            .collect()
    }
}
fn json_graph(v: Value) -> EventValue {
    EventValue::from_json(v)
}
fn setup(mut f: Value, v: Venue, family: &str) -> Bindings {
    if v == Venue::Spot && family == "regular" {
        if f["name"] == "stop" {
            f["data"]["o"] = json!("STOP_LOSS_LIMIT")
        }
        if f["name"] == "take-profit" {
            f["data"]["o"] = json!("TAKE_PROFIT_LIMIT")
        }
    }
    let data = f
        .get("data")
        .cloned()
        .map(json_graph)
        .unwrap_or(EventValue::Undefined);
    let events = if let Some(events) = f.get("events") {
        let EventValue::Array(a) = json_graph(events.clone()) else {
            panic!()
        };
        a
    } else {
        let entry = if f["null_event"] == true {
            EventValue::Null
        } else if v == Venue::Spot && family == "regular" {
            if f["null_data"] == true {
                EventValue::Null
            } else {
                data.clone()
            }
        } else {
            record([(
                if family == "conditional" { "so" } else { "o" },
                if f["null_data"] == true {
                    EventValue::Null
                } else if f["falsy_data"] == true {
                    EventValue::Bool(false)
                } else {
                    data.clone()
                },
            )])
        };
        Rc::new(RefCell::new(if f["count"] == 0 {
            vec![]
        } else {
            vec![Some(entry)]
        }))
    };
    let positions = json_graph(
        json!([{"id":"BTCUSDT","symbol":"BTCUSDT","side":"short","leverage":10,"liquidationPrice":99,"marginType":"isolated","positionMode":"oneway","entryPrice":8,"contracts":1,"notional":8,"unrealizedPnl":2,"unknown":41,"info":{"keep":17,"nested":{"old":19}}},{"id":"BTCUSDT:long","symbol":"BTCUSDT","side":"long","leverage":11,"liquidationPrice":98,"marginType":"cross","info":{"keep":18}},{"id":"BTCUSDT:short","symbol":"BTCUSDT","side":"short","leverage":12,"liquidationPrice":97,"marginType":"cross","info":{"keep":19}}]),
    );
    let EventValue::Array(positions) = positions else {
        panic!()
    };
    for (i, p) in array_values(&positions).iter().enumerate() {
        p.write(
            "side",
            EventValue::enum_value(PrivateEnum::PositionSide(if i == 1 {
                PositionSide::Long
            } else {
                PositionSide::Short
            })),
        );
        p.write(
            "marginType",
            EventValue::enum_value(PrivateEnum::Margin(if i == 0 {
                MarginType::Isolated
            } else {
                MarginType::Cross
            })),
        );
        if i == 0 {
            p.write(
                "positionMode",
                EventValue::enum_value(PrivateEnum::PositionMode(PositionMode::OneWay)),
            );
        }
    }
    if f["name"] == "metadata-nullish" {
        let p = &array_values(&positions)[0];
        p.write("leverage", EventValue::Null);
        p.write("liquidationPrice", number(0.0));
        p.write("marginType", EventValue::Null);
    }
    let EventValue::Array(orders) =
        json_graph(json!([{"id":"101","extra":{"memo":9,"nested":{"old":27}}}]))
    else {
        panic!()
    };
    let balance = json_graph(
        json!({"currencies":{"ETH":{"free":1,"used":2,"total":3,"upnl":0,"info":{"retained":7}}},"coins":{"ETH":{"base":"ETH","amount":3,"notional":15}}}),
    );
    if f["name"] == "own-proto-asset" {
        balance.read("currencies").unwrap().write(
            "__proto__",
            record([
                ("free", number(1.0)),
                ("used", number(1.0)),
                ("total", number(2.0)),
            ]),
        );
    }
    let mut e = ExchangeError::new("private fixture");
    e.name = "RangeError".into();
    e.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "private-code"
    )));
    e.response = Some(json!({"status":503,"data":{"msg":"private detail"}}));
    Bindings(Rc::new(State {
        f,
        events,
        data,
        positions,
        orders,
        balance,
        error: Rc::new(e),
        flags: Cell::new(false),
        effects: RefCell::new(vec![]),
        trace: RefCell::new(vec![]),
        clocks: Cell::new(0),
        prices: Cell::new(0),
    }))
}
impl PrivateEventBindings for Bindings {
    fn capture(&mut self, op: PrivateOperation) -> Result<PrivateHandler, Failure> {
        let s = self.0.clone();
        s.log(Trace::Capture(op));
        if s.name() == "capture-fail"
            && matches!(
                op,
                PrivateOperation::AddOrder
                    | PrivateOperation::AddPosition
                    | PrivateOperation::UpdatePosition
                    | PrivateOperation::UpdateStore
            )
        {
            return Err(s.error.clone());
        }
        if s.name() == "capture-mutates" && op == PrivateOperation::UpdateOrder {
            s.data.write("c", string("updated"));
            s.data.write("i", number(202.0));
        }
        if s.name() == "cyclic-id" && op == PrivateOperation::AddOrder {
            let a = Rc::new(RefCell::new(vec![]));
            EventValue::Array(a.clone())
                .push(EventValue::Array(a.clone()))
                .unwrap();
            s.data.write("i", EventValue::Array(a));
        }
        Ok(Rc::new(move |args| {
            s.log(Trace::Call(op));
            s.effects.borrow_mut().push(Effect {
                op,
                args,
                pending: s.flags.get(),
            });
            if s.name() == "call-fail"
                && matches!(
                    op,
                    PrivateOperation::Fill
                        | PrivateOperation::AddPosition
                        | PrivateOperation::UpdatePosition
                        | PrivateOperation::UpdateStore
                )
            {
                return Err(s.error.clone());
            }
            if s.name() == "emit-mutates" && op == PrivateOperation::Fill {
                s.data.write("q", string("7"));
            }
            if s.name() == "append-event" && op == PrivateOperation::AddOrder {
                let first = array_values(&s.events)[0].clone();
                s.events.borrow_mut().push(Some(first));
            }
            if s.name() == "throttle-mutates" && op == PrivateOperation::ThrottlePositions {
                array_values(&s.events)[0]
                    .read("a")?
                    .write("P", EventValue::array(vec![]));
            }
            Ok(())
        }))
    }
    fn now(&mut self) -> Result<f64, Failure> {
        let s = &self.0;
        let n = s.clocks.get() + 1;
        s.clocks.set(n);
        s.log(Trace::Clock(n));
        if s.name() == "clock-fail" {
            return Err(s.error.clone());
        }
        if s.name() == "clock-mutates" {
            for (k, v) in [
                ("q", string("9")),
                ("i", number(202.0)),
                ("aid", number(602.0)),
                ("c", string("updated")),
                ("ot", string("TAKE_PROFIT_MARKET")),
                ("o", string("TAKE_PROFIT_MARKET")),
                ("st", string("TAKE_PROFIT_MARKET")),
            ] {
                s.data.write(k, v)
            }
        }
        Ok(10000.0 + n as f64)
    }
    fn orders(&mut self) -> Result<EventArray, Failure> {
        self.0.log(Trace::Orders);
        Ok(self.0.orders.clone())
    }
    fn positions(&mut self) -> Result<EventArray, Failure> {
        self.0.log(Trace::Positions);
        if self.0.name() == "positions-fail" {
            Err(self.0.error.clone())
        } else {
            Ok(self.0.positions.clone())
        }
    }
    fn balance(&mut self) -> Result<EventValue, Failure> {
        self.0.log(Trace::Balance);
        if self.0.name() == "balance-fail" {
            Err(self.0.error.clone())
        } else {
            Ok(self.0.balance.clone())
        }
    }
    fn reject_reason(&mut self, data: &EventValue) -> Result<EventValue, Failure> {
        self.0.log(Trace::Reject);
        get_order_reject_reason(data)
    }
    fn asset_price(&mut self, asset: &EventValue) -> Result<f64, Failure> {
        let s = &self.0;
        s.log(Trace::Price(asset.clone()));
        let n = s.prices.get() + 1;
        s.prices.set(n);
        if s.name() == "price-fail" {
            return Err(s.error.clone());
        }
        if s.name() == "append-balance" && n == 1 {
            let EventValue::Array(a) = array_values(&s.events)[0].read("B")? else {
                panic!()
            };
            EventValue::Array(a.clone())
                .push(record([
                    ("a", string("ETH")),
                    ("f", string("1")),
                    ("l", string("1")),
                ]))
                .unwrap();
        }
        let p = match asset.text()?.as_str() {
            "BTC" => 10.0,
            "ETH" => 5.0,
            "USDT" => 1.0,
            _ => 0.0,
        };
        Ok(if s.name() == "price-changes" {
            p + n as f64
        } else {
            p
        })
    }
}
fn fixtures(family: &str) -> Vec<Value> {
    let all: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/private-events.json")).unwrap();
    assert_eq!(all.len(), 92, "independent26+16+15+18+17 input floor");
    let selected = all
        .into_iter()
        .filter(|c| c["family"] == family)
        .collect::<Vec<_>>();
    assert_eq!(
        selected.len(),
        match family {
            "regular" => 26,
            "algo" => 16,
            "conditional" => 15,
            "account" => 18,
            "spot-account" => 17,
            _ => panic!(),
        },
        "independent family floor"
    );
    selected
}
fn source(v: Venue, family: &str) -> Vec<Value> {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-events.mjs"
        ))
        .args([venue(v), family])
        .output()
        .expect("private source oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        rows.len(),
        match family {
            "regular" => 26,
            "algo" => 16,
            "conditional" => 15,
            "account" => 18,
            "spot-account" => 17,
            _ => panic!(),
        },
        "fixed source fixture floor"
    );
    rows
}
fn identity(s: &State, e: &Effect) -> Value {
    use PrivateOperation::*;
    let get = |v: &EventValue, key: &str| v.get(key).unwrap();
    match e.op {
        AddOrder => {
            let row = &e.args[0];
            let info = get(row, "info");
            json!({"infoData":info.same(&s.data),"infoNested":get(&info,"nested").same(&get(&s.data,"nested")),"infoFresh":!info.same(&s.data)})
        }
        AddPosition => {
            let row = &e.args[0];
            let existing = array_values(&s.positions)
                .iter()
                .find(|p| get(p, "id").same(&get(row, "id")))
                .cloned()
                .unwrap_or(EventValue::Undefined);
            let input = if let EventValue::Array(p) = array_values(&s.events)[0]
                .read("a")
                .unwrap()
                .read("P")
                .unwrap()
            {
                p.borrow()[0].clone().unwrap()
            } else {
                panic!()
            };
            json!({"infoFresh":!get(row,"info").same(&get(&existing,"info")),"infoNestedInput":get(&get(row,"info"),"nested").same(&get(&input,"nested")),"rowFresh":!row.same(&existing)})
        }
        UpdatePosition => {
            json!({"selectorOriginal":array_values(&s.positions).iter().any(|p|p.same(&e.args[0])),"infoOriginal":get(&e.args[1],"info").same(&e.args[0])})
        }
        UpdateStore => {
            let row = get(&e.args[0], "balance");
            let curr = get(&row, "currencies");
            let coins = get(&row, "coins");
            let oldcurr = get(&s.balance, "currencies");
            let oldcoins = get(&s.balance, "coins");
            let evt = array_values(&s.events)[0].clone();
            let asset_info = if let Ok(EventValue::Array(a)) = evt.read("B") {
                if let Some(entry) = a.borrow().first().and_then(Option::as_ref) {
                    if !get(entry, "a").same(&string("__proto__")) {
                        Some(get(&get(&curr, &get(entry, "a").text().unwrap()), "info").same(entry))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            json!({"eventInfo":get(&row,"info").same(&evt),"currenciesFresh":!curr.same(&oldcurr),"coinsFresh":!coins.same(&oldcoins),"retainedCurrency":get(&curr,"ETH").same(&get(&oldcurr,"ETH")),"retainedCoin":get(&coins,"ETH").same(&get(&oldcoins,"ETH")),"assetInfo":asset_info})
        }
        _ => json!({}),
    }
}
fn projected(s: &State, error: Option<&Failure>) -> Value {
    let trace = s
        .trace
        .borrow()
        .iter()
        .map(|t| match t {
            Trace::Capture(o) => json!(["capture", op_name(*o)]),
            Trace::Call(o) => json!(["call", op_name(*o)]),
            Trace::Clock(n) => json!(["clock", n]),
            Trace::Orders => json!(["orders"]),
            Trace::Positions => json!(["positions"]),
            Trace::Balance => json!(["balance"]),
            Trace::Reject => json!(["reject"]),
            Trace::Price(v) => json!(["price", wire(v)]),
        })
        .collect::<Vec<_>>();
    let effects=s.effects.borrow().iter().map(|e|json!({"op":op_name(e.op),"args":e.args.iter().map(wire).collect::<Vec<_>>(),"identity":identity(s,e),"pending":e.pending})).collect::<Vec<_>>();
    json!({"name":s.name(),"trace":trace,"effects":effects,"pending":s.flags.get(),"clocks":s.clocks.get(),"priceCalls":s.prices.get(),"eventLength":s.events.borrow().len(),"failure":error.map(|e|json!({"original":Rc::ptr_eq(e,&s.error),"name":e.name,"message":e.message,"code":observed_error_code(&e.code).clone().unwrap_or(json!({"$undefined":true})),"response":e.response.clone().unwrap_or(json!({"$undefined":true}))}))})
}
fn native_num(value: EventValue, expected: f64, label: &str) {
    let EventValue::Number(actual) = value else {
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
fn native_value(value: EventValue, expected: EventValue, label: &str) {
    match (&value, &expected) {
        (EventValue::Undefined, EventValue::Undefined) | (EventValue::Null, EventValue::Null) => {}
        (EventValue::Bool(a), EventValue::Bool(b)) => assert_eq!(a, b, "{label}: native bool"),
        (EventValue::String(a), EventValue::String(b)) => {
            assert_eq!(a, b, "{label}: native string")
        }
        (EventValue::Number(_), EventValue::Number(n)) => native_num(value, *n, label),
        (EventValue::Builtin(actual), EventValue::Builtin(expected)) => {
            assert_eq!(
                actual.member(),
                expected.member(),
                "{label}: native inherited member identity"
            );
            assert!(
                Rc::ptr_eq(actual.realm(), &safe_cex_num::PrototypeRealm::current()),
                "{label}: native inherited originating realm"
            );
        }
        (EventValue::Object(_), EventValue::Object(_))
        | (EventValue::Array(_), EventValue::Array(_)) => {
            assert!(value.same(&expected), "{label}: native referent identity")
        }
        _ => panic!("{label}: native value variant mismatch"),
    }
}
fn field(row: &EventValue, key: &str) -> EventValue {
    row.get(key).unwrap()
}
fn native_error(
    result: &Result<(), Failure>,
    s: &State,
    expected: Option<(&str, &str)>,
    label: &str,
) {
    match (result, expected) {
        (Ok(()), None) => {}
        (Err(e), Some(("original", _))) => {
            assert!(
                Rc::ptr_eq(e, &s.error),
                "{label}: native original error identity"
            );
            assert_eq!(e.name, "RangeError", "{label}: native error name");
            assert_eq!(
                e.message, "private fixture",
                "{label}: native error message"
            );
            assert_eq!(
                observed_error_code(&e.code),
                Some(json!("private-code")),
                "{label}: native error code"
            );
            assert_eq!(
                e.response,
                Some(json!({"status":503,"data":{"msg":"private detail"}})),
                "{label}: native error response"
            );
        }
        (Err(e), Some((name, message))) => {
            assert!(
                !Rc::ptr_eq(e, &s.error),
                "{label}: native generated error identity"
            );
            assert_eq!(e.name, name, "{label}: native generated name");
            assert_eq!(e.message, message, "{label}: native generated message");
            assert_eq!(
                observed_error_code(&e.code),
                None,
                "{label}: native generated code"
            );
            assert_eq!(e.response, None, "{label}: native generated response");
        }
        _ => panic!("{label}: native result/error path mismatch"),
    }
}
fn compare(actual: Vec<Value>, v: Venue, family: &str) {
    let expected = source(v, family);
    for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            numeric(a),
            numeric(e),
            "source {} {family} case{i} {}",
            venue(v),
            a["name"]
        );
    }
}
fn order_test(v: Venue, family: &str) {
    let mut projections = vec![];
    for (i, f) in fixtures(family).into_iter().enumerate() {
        let mut b = setup(f, v, family);
        let s = b.0.clone();
        let events = s.events.clone();
        let name = s.name();
        let label = format!("native {} {family} case{i} {name}", venue(v));
        let result = match family {
            "regular" => match v {
                Venue::Futures => handle_futures_order_events(&events, &mut b),
                Venue::PM => handle_pm_order_events(&events, &mut b),
                Venue::Spot => handle_spot_order_events(&events, &mut b),
            },
            "algo" => handle_futures_algo_order_events(&events, &mut b),
            "conditional" => handle_pm_conditional_order_events(&events, &mut b),
            _ => panic!(),
        };
        let expected_error = if matches!(name, "call-fail" | "capture-fail" | "clock-fail") {
            Some(("original", ""))
        } else if name == "missing-data" {
            Some((
                "TypeError",
                if v == Venue::Spot {
                    "Cannot read properties of null (reading 'x')"
                } else if family == "conditional" {
                    "Cannot read properties of null (reading 'os')"
                } else {
                    "Cannot read properties of null (reading 'X')"
                },
            ))
        } else if name == "missing-event" {
            Some((
                "TypeError",
                if family == "algo" {
                    "Cannot read properties of null (reading 'o')"
                } else if v == Venue::Spot {
                    "Cannot read properties of null (reading 'x')"
                } else if family == "conditional" {
                    "Cannot destructure property 'so' of 'object null' as it is null."
                } else {
                    "Cannot destructure property 'o' of 'object null' as it is null."
                },
            ))
        } else if name == "noncallable-id" && v == Venue::Futures {
            Some(("TypeError", "data.i?.toString is not a function"))
        } else {
            None
        };
        native_error(&result, &s, expected_error, &label);
        let fills = usize::from(matches!(
            name,
            "partial"
                | "filled"
                | "quantity-closed"
                | "emit-mutates"
                | "capture-mutates"
                | "call-fail"
        ));
        assert_eq!(
            s.count(PrivateOperation::Fill),
            fills,
            "{label}: native fill count"
        );
        for e in s.effects(PrivateOperation::Fill) {
            let p = &e.args[0];
            native_value(
                field(p, "id"),
                if v == Venue::Futures {
                    string("101")
                } else {
                    string("client-101")
                },
                &format!("{label}: native fill id"),
            );
            native_value(
                field(p, "side"),
                EventValue::enum_value(PrivateEnum::Side(OrderSide::Buy)),
                &format!("{label}: native fill side"),
            );
            native_value(
                field(p, "symbol"),
                string("BTCUSDT"),
                &format!("{label}: native fill symbol"),
            );
            native_num(
                field(p, "price"),
                if v == Venue::Spot { 12.3 } else { 12.5 },
                &format!("{label}: native fill price"),
            );
            native_num(
                field(p, "amount"),
                0.2,
                &format!("{label}: native fill quantity"),
            );
        }
        let regular_patch = if family == "regular" && fills == 1 && name != "call-fail" {
            1
        } else {
            0
        };
        let reject_patch =
            usize::from(v == Venue::Futures && family == "regular" && name == "canceled");
        assert_eq!(
            s.count(PrivateOperation::UpdateOrder),
            regular_patch + reject_patch,
            "{label}: native update count"
        );
        for e in s.effects(PrivateOperation::UpdateOrder) {
            let selector = &e.args[0];
            let patch = &e.args[1];
            native_value(
                field(selector, "id"),
                if v == Venue::Futures {
                    string("101")
                } else if v == Venue::PM && name == "capture-mutates" {
                    string("updated")
                } else {
                    string("client-101")
                },
                &format!("{label}: native update selector id"),
            );
            if reject_patch == 1 {
                let extra = field(patch, "extra");
                native_num(
                    field(&extra, "memo"),
                    9.0,
                    &format!("{label}: native reject memo"),
                );
                native_value(
                    field(&extra, "rejectReason"),
                    EventValue::enum_value(PrivateEnum::Reject(OrderRejectReason::PostOnly)),
                    &format!("{label}: native rejection enum"),
                );
                assert!(
                    field(&extra, "nested").same(&field(
                        &field(&array_values(&s.orders)[0], "extra"),
                        "nested"
                    )),
                    "{label}: native preserved extra nested identity"
                );
            } else {
                native_num(
                    field(patch, "filled"),
                    if name == "quantity-closed" { 3.2 } else { 0.7 },
                    &format!("{label}: native cumulative filled"),
                );
                native_num(
                    field(patch, "remaining"),
                    if name == "quantity-closed" {
                        0.0
                    } else if name == "emit-mutates" {
                        6.3
                    } else {
                        2.5
                    },
                    &format!("{label}: native cumulative remainder"),
                );
                native_num(
                    field(patch, "lastUpdate"),
                    10001.0,
                    &format!("{label}: native cumulative timestamp"),
                );
            }
        }
        let add = if expected_error.is_some()
            || matches!(
                name,
                "empty"
                    | "unknown"
                    | "partial"
                    | "filled"
                    | "quantity-closed"
                    | "canceled"
                    | "expired"
                    | "cancelled"
                    | "rejected"
                    | "triggered"
                    | "triggering"
                    | "finished"
                    | "falsy-data"
                    | "emit-mutates"
                    | "capture-mutates"
            ) {
            0
        } else {
            1
        };
        assert_eq!(
            s.count(PrivateOperation::AddOrder),
            add,
            "{label}: native add count"
        );
        for e in s.effects(PrivateOperation::AddOrder) {
            let p = &e.args[0];
            let id = if name == "missing-id" {
                EventValue::Undefined
            } else if name == "cyclic-id" && v == Venue::Futures {
                string("")
            } else if family == "algo" {
                string("501")
            } else if v == Venue::Futures {
                string("101")
            } else {
                string("client-101")
            };
            native_value(field(p, "id"), id, &format!("{label}: native order id"));
            native_value(
                field(p, "status"),
                EventValue::enum_value(PrivateEnum::Status(OrderStatus::Open)),
                &format!("{label}: native order status"),
            );
            native_value(
                field(p, "symbol"),
                string("BTCUSDT"),
                &format!("{label}: native order symbol"),
            );
            let typ = if name == "prototype-type" {
                EventValue::inherited(ProtoMember::Constructor)
            } else if name == "unknown-type" && v != Venue::Spot {
                EventValue::Undefined
            } else {
                EventValue::enum_value(PrivateEnum::Type(match name {
                    "stop" => OrderType::StopLoss,
                    "take-profit" => OrderType::TakeProfit,
                    _ => OrderType::Limit,
                }))
            };
            native_value(
                field(p, "type"),
                typ,
                &format!("{label}: native order type"),
            );
            native_value(
                field(p, "side"),
                EventValue::enum_value(PrivateEnum::Side(OrderSide::Buy)),
                &format!("{label}: native order side"),
            );
            native_num(
                field(p, "price"),
                if family == "algo" {
                    if name == "zero-defaults" { 0.0 } else { 13.4 }
                } else {
                    12.3
                },
                &format!("{label}: native price"),
            );
            native_num(
                field(p, "amount"),
                if name == "zero-defaults" {
                    0.0
                } else if name == "falsy-filled" {
                    f64::NAN
                } else {
                    3.2
                },
                &format!("{label}: native amount"),
            );
            native_num(
                field(p, "filled"),
                if family == "algo" || name == "falsy-filled" {
                    0.0
                } else {
                    0.7
                },
                &format!("{label}: native filled"),
            );
            native_num(
                field(p, "remaining"),
                if name == "zero-defaults" || name == "falsy-filled" {
                    0.0
                } else if family == "algo" {
                    if name == "clock-mutates" { 9.0 } else { 3.2 }
                } else if name == "clock-mutates" {
                    8.3
                } else {
                    2.5
                },
                &format!("{label}: native remainder"),
            );
            native_num(
                field(p, "lastUpdate"),
                10001.0,
                &format!("{label}: native lastUpdate"),
            );
            native_value(
                field(p, "reduceOnly"),
                if matches!(name, "zero-defaults" | "falsy-filled") {
                    string("yes")
                } else {
                    EventValue::Bool(false)
                },
                &format!("{label}: native reduceOnly value"),
            );
            if family == "algo" {
                native_value(
                    field(p, "close"),
                    if name == "zero-defaults" {
                        number(1.0)
                    } else {
                        EventValue::Bool(false)
                    },
                    &format!("{label}: native close value"),
                );
            } else {
                assert!(
                    matches!(field(p, "close"), EventValue::Undefined),
                    "{label}: native absent close"
                );
            }
            let info = field(p, "info");
            if v == Venue::Futures {
                assert!(!info.same(&s.data), "{label}: native info shallow copy");
                assert!(
                    field(&info, "nested").same(&field(&s.data, "nested")),
                    "{label}: native info nested identity"
                );
                if family == "algo" {
                    native_num(
                        field(&info, "algoId"),
                        if name == "overwrite-info" {
                            777.0
                        } else if name == "clock-mutates" {
                            602.0
                        } else {
                            501.0
                        },
                        &format!("{label}: native info algoId"),
                    );
                } else {
                    native_value(
                        field(&info, "orderId"),
                        if name == "missing-id" {
                            EventValue::Undefined
                        } else if name == "cyclic-id" {
                            string("")
                        } else if name == "clock-mutates" {
                            string("202")
                        } else {
                            string("101")
                        },
                        &format!("{label}: native info orderId"),
                    );
                }
            } else {
                assert!(
                    info.same(&s.data),
                    "{label}: native info original data identity"
                );
            }
            let tp = v != Venue::Spot
                && (name == "take-profit" || name == "clock-mutates" && family != "algo");
            let sl = v != Venue::Spot && name == "stop";
            for (key, present) in [("tpTriggerType", tp), ("slTriggerType", sl)] {
                native_value(
                    field(p, key),
                    if present {
                        EventValue::enum_value(PrivateEnum::Trigger(
                            OrderStopTriggerType::MarkPrice,
                        ))
                    } else {
                        EventValue::Undefined
                    },
                    &format!("{label}: native {key} presence/type"),
                );
            }
        }
        let statuses = if expected_error.is_some() {
            0
        } else if family == "algo"
            && matches!(
                name,
                "triggered"
                    | "triggering"
                    | "finished"
                    | "canceled"
                    | "cancelled"
                    | "expired"
                    | "rejected"
            )
        {
            1
        } else if matches!(name, "canceled" | "expired")
            || name == "cancelled" && v != Venue::Spot
            || name == "rejected" && v == Venue::Spot
            || name == "filled"
            || name == "quantity-closed" && v == Venue::Spot
        {
            1
        } else {
            0
        };
        assert_eq!(
            s.count(PrivateOperation::OrderStatus),
            statuses,
            "{label}: native status count"
        );
        for e in s.effects(PrivateOperation::OrderStatus) {
            native_value(
                field(&e.args[0], "id"),
                if family == "algo" {
                    string("501")
                } else if v == Venue::Futures {
                    string("101")
                } else {
                    string("client-101")
                },
                &format!("{label}: native status selector"),
            );
            native_value(
                e.args[1].clone(),
                EventValue::enum_value(PrivateEnum::Status(
                    if matches!(
                        name,
                        "filled" | "quantity-closed" | "triggered" | "triggering" | "finished"
                    ) {
                        OrderStatus::Closed
                    } else {
                        OrderStatus::Canceled
                    },
                )),
                &format!("{label}: native status enum"),
            );
        }
        assert_eq!(
            s.count(PrivateOperation::ThrottleOrders),
            usize::from(v != Venue::Spot && result.is_ok()),
            "{label}: native trailing refresh count"
        );
        assert_eq!(
            s.events.borrow().len(),
            if name == "empty" {
                0
            } else if name == "append-event" {
                2
            } else {
                1
            },
            "{label}: native event iteration length"
        );
        assert!(
            !s.flags.get(),
            "{label}: native unchanged pending positions"
        );
        projections.push(projected(&s, result.as_ref().err()));
    }
    compare(projections, v, family);
}
fn account_test(v: Venue) {
    let mut projections = vec![];
    for (i, f) in fixtures("account").into_iter().enumerate() {
        let mut b = setup(f, v, "account");
        let s = b.0.clone();
        let label = format!("native {} account case{i} {}", venue(v), s.name());
        let name = s.name();
        let result = if v == Venue::Futures {
            handle_futures_account_events(&s.events, &s.flags, &mut b)
        } else {
            handle_pm_account_events(&s.events, &s.flags, &mut b)
        };
        let error = if matches!(name, "positions-fail" | "call-fail" | "capture-fail") {
            Some(("original", ""))
        } else if name == "missing-a" && v == Venue::Futures {
            Some((
                "TypeError",
                "Cannot read properties of undefined (reading 'P')",
            ))
        } else if name == "invalid-P" {
            Some((
                "TypeError",
                if v == Venue::Futures {
                    "event.a.P?.forEach is not a function"
                } else {
                    "event.a?.P?.forEach is not a function"
                },
            ))
        } else if name == "null-row" {
            Some(("TypeError", "Cannot read properties of null (reading 's')"))
        } else {
            None
        };
        native_error(&result, &s, error, &label);
        let pending = !matches!(name, "empty" | "missing-a" | "missing-P")
            && (v == Venue::PM || name != "empty-P");
        assert_eq!(s.flags.get(), pending, "{label}: native pending flag");
        assert_eq!(
            s.count(PrivateOperation::MarkPosition),
            usize::from(v == Venue::Futures && pending),
            "{label}: native mark count"
        );
        assert_eq!(
            s.count(PrivateOperation::ThrottlePositions),
            usize::from(pending),
            "{label}: native throttle count"
        );
        let op = if v == Venue::Futures {
            PrivateOperation::AddPosition
        } else {
            PrivateOperation::UpdatePosition
        };
        let updates = usize::from(!matches!(
            name,
            "zero-unknown"
                | "empty-P"
                | "missing-a"
                | "missing-P"
                | "invalid-P"
                | "throttle-mutates"
                | "positions-fail"
                | "capture-fail"
                | "null-row"
                | "empty"
        ));
        assert_eq!(s.count(op), updates, "{label}: native position count");
        for e in s.effects(op) {
            let row = if v == Venue::Futures {
                e.args[0].clone()
            } else {
                e.args[1].clone()
            };
            native_num(
                field(&row, "entryPrice"),
                12.3,
                &format!("{label}: native entry"),
            );
            native_num(
                field(&row, "contracts"),
                if name == "zero-known" { 0.0 } else { 2.5 },
                &format!("{label}: native contracts"),
            );
            native_num(
                field(&row, "unrealizedPnl"),
                3.6,
                &format!("{label}: native upnl"),
            );
            native_num(
                field(&row, "notional"),
                if v == Venue::PM {
                    if name == "zero-known" { 0.0 } else { 30.75 }
                } else if name == "zero-known" {
                    3.6
                } else if matches!(name, "negative" | "hedged-short") {
                    27.15
                } else {
                    34.35
                },
                &format!("{label}: native notional"),
            );
            let index = if name == "hedged-long" {
                1
            } else if name == "hedged-short" && v == Venue::Futures {
                2
            } else {
                0
            };
            let original = array_values(&s.positions)[index].clone();
            if v == Venue::Futures {
                native_value(
                    field(&row, "id"),
                    string(if index == 1 {
                        "BTCUSDT:long"
                    } else if index == 2 {
                        "BTCUSDT:short"
                    } else {
                        "BTCUSDT"
                    }),
                    &format!("{label}: native position id"),
                );
                native_value(
                    field(&row, "symbol"),
                    string("BTCUSDT"),
                    &format!("{label}: native symbol"),
                );
                native_value(
                    field(&row, "side"),
                    EventValue::enum_value(PrivateEnum::PositionSide(
                        if matches!(name, "negative" | "hedged-short" | "zero-known") {
                            PositionSide::Short
                        } else {
                            PositionSide::Long
                        },
                    )),
                    &format!("{label}: native side"),
                );
                native_value(
                    field(&row, "positionMode"),
                    EventValue::enum_value(PrivateEnum::PositionMode(if index == 0 {
                        PositionMode::OneWay
                    } else {
                        PositionMode::Hedged
                    })),
                    &format!("{label}: native mode"),
                );
                native_num(
                    field(&row, "leverage"),
                    if name == "metadata-nullish" {
                        0.0
                    } else {
                        10.0 + index as f64
                    },
                    &format!("{label}: native leverage preservation"),
                );
                native_num(
                    field(&row, "liquidationPrice"),
                    if name == "metadata-nullish" {
                        0.0
                    } else {
                        99.0 - index as f64
                    },
                    &format!("{label}: native liquidation preservation"),
                );
                native_value(
                    field(&row, "marginType"),
                    EventValue::enum_value(PrivateEnum::Margin(
                        if name == "metadata-nullish" || index != 0 {
                            MarginType::Cross
                        } else {
                            MarginType::Isolated
                        },
                    )),
                    &format!("{label}: native margin preservation"),
                );
                if index == 0 {
                    native_num(
                        field(&row, "unknown"),
                        41.0,
                        &format!("{label}: native retained unknown field"),
                    );
                }
                let info = field(&row, "info");
                assert!(
                    !info.same(&field(&original, "info")),
                    "{label}: native new info object"
                );
                native_num(
                    field(&info, "keep"),
                    17.0 + index as f64,
                    &format!("{label}: native info retained value"),
                );
                let EventValue::Array(p) = array_values(&s.events)[0]
                    .read("a")
                    .unwrap()
                    .read("P")
                    .unwrap()
                else {
                    panic!()
                };
                assert!(
                    field(&info, "nested").same(&field(p.borrow()[0].as_ref().unwrap(), "nested")),
                    "{label}: native incoming nested info identity"
                );
                assert!(
                    !row.same(&original),
                    "{label}: native shallow position copy"
                );
            } else {
                assert!(
                    e.args[0].same(&original),
                    "{label}: native original position selector"
                );
                assert!(
                    field(&row, "info").same(&original),
                    "{label}: native info original position identity"
                );
                if !matches!(name, "hedged-long" | "hedged-short") {
                    native_value(
                        field(&row, "side"),
                        EventValue::enum_value(PrivateEnum::PositionSide(
                            if matches!(name, "negative" | "zero-known" | "suffix-quantity") {
                                PositionSide::Short
                            } else {
                                PositionSide::Long
                            },
                        )),
                        &format!("{label}: native Number quantity side"),
                    );
                } else {
                    assert!(
                        matches!(field(&row, "side"), EventValue::Undefined),
                        "{label}: native hedged side omission"
                    );
                }
            }
        }
        for e in s.effects(PrivateOperation::MarkPosition) {
            assert!(!e.pending, "{label}: native mark before pending write");
        }
        for e in s.effects(PrivateOperation::ThrottlePositions) {
            assert!(e.pending, "{label}: native pending before throttle");
        }
        assert_eq!(s.clocks.get(), 0, "{label}: native no virtual clock");
        projections.push(projected(&s, result.as_ref().err()));
    }
    compare(projections, v, "account");
}
fn spot_account_test() {
    let v = Venue::Spot;
    let mut projections = vec![];
    for (i, f) in fixtures("spot-account").into_iter().enumerate() {
        let mut b = setup(f, v, "spot-account");
        let s = b.0.clone();
        let name = s.name();
        let label = format!("native spot account case{i} {name}");
        let result = handle_spot_account_events(&s.events, &mut b);
        let error = if matches!(
            name,
            "price-fail" | "call-fail" | "capture-fail" | "balance-fail"
        ) {
            Some(("original", ""))
        } else if name == "invalid-B" {
            Some((
                "TypeError",
                "object is not iterable (cannot read property Symbol(Symbol.iterator))",
            ))
        } else if name == "null-row" {
            Some(("TypeError", "Cannot read properties of null (reading 'a')"))
        } else {
            None
        };
        native_error(&result, &s, error, &label);
        let updates = usize::from(!matches!(
            name,
            "empty" | "price-fail" | "capture-fail" | "balance-fail" | "invalid-B" | "null-row"
        ));
        assert_eq!(
            s.count(PrivateOperation::UpdateStore),
            updates,
            "{label}: native balance update count"
        );
        for e in s.effects(PrivateOperation::UpdateStore) {
            let row = field(&e.args[0], "balance");
            let (free, used, total) = match name {
                "empty-B" | "missing-B" | "zero-values" | "proto-asset" | "own-proto-asset" => {
                    (5.0, 10.0, 15.0)
                }
                "negative" => (-20.0, 15.0, -5.0),
                "price-changes" => (67.0, 29.0, 96.0),
                "append-balance" => (34.0, 11.0, 45.0),
                _ => (34.0, 16.0, 50.0),
            };
            native_num(
                field(&row, "free"),
                free,
                &format!("{label}: native aggregate free"),
            );
            native_num(
                field(&row, "used"),
                used,
                &format!("{label}: native aggregate used"),
            );
            native_num(
                field(&row, "total"),
                total,
                &format!("{label}: native aggregate total"),
            );
            native_num(field(&row, "upnl"), 0.0, &format!("{label}: native upnl"));
            assert!(
                field(&row, "info").same(&array_values(&s.events)[0]),
                "{label}: native original account event info"
            );
            let curr = field(&row, "currencies");
            let coins = field(&row, "coins");
            let oldcurr = field(&s.balance, "currencies");
            let oldcoins = field(&s.balance, "coins");
            assert!(
                !curr.same(&oldcurr) && !coins.same(&oldcoins),
                "{label}: native new dictionaries"
            );
            if name != "append-balance" {
                assert!(
                    field(&curr, "ETH").same(&field(&oldcurr, "ETH")),
                    "{label}: native retained currency identity"
                );
                assert!(
                    field(&coins, "ETH").same(&field(&oldcoins, "ETH")),
                    "{label}: native retained coin identity"
                );
            } else {
                assert!(
                    !field(&curr, "ETH").same(&field(&oldcurr, "ETH")),
                    "{label}: native appended currency changed"
                );
            }
            let assets = match array_values(&s.events)[0].read("B").unwrap() {
                EventValue::Array(a) => a,
                EventValue::Undefined | EventValue::Null => Rc::new(RefCell::new(vec![])),
                _ => panic!("{label}: native asset collection"),
            };
            if let Some(first) = assets.borrow().first().and_then(Option::as_ref) {
                let asset = field(first, "a").text().unwrap();
                if asset != "__proto__" {
                    let c = field(&curr, &asset);
                    assert!(
                        field(&c, "info").same(first),
                        "{label}: native original asset info"
                    );
                    let (af, au, at) = if name == "negative" {
                        (-2.5, 0.5, -2.0)
                    } else if name == "zero-values" {
                        (0.0, 0.0, 0.0)
                    } else {
                        (2.5, 0.5, 3.0)
                    };
                    native_num(
                        field(&c, "free"),
                        af,
                        &format!("{label}: native asset free"),
                    );
                    native_num(
                        field(&c, "used"),
                        au,
                        &format!("{label}: native asset used"),
                    );
                    native_num(
                        field(&c, "total"),
                        at,
                        &format!("{label}: native asset total"),
                    );
                    native_num(
                        field(&c, "upnl"),
                        0.0,
                        &format!("{label}: native asset upnl"),
                    );
                    let coin = field(&coins, &asset);
                    native_value(
                        field(&coin, "base"),
                        string("BTC"),
                        &format!("{label}: native coin base"),
                    );
                    native_num(
                        field(&coin, "amount"),
                        at,
                        &format!("{label}: native coin amount"),
                    );
                    native_num(
                        field(&coin, "notional"),
                        at * if name == "price-changes" { 11.0 } else { 10.0 },
                        &format!("{label}: native coin valuation"),
                    );
                } else {
                    let EventValue::Object(curr) = &curr else {
                        panic!()
                    };
                    assert_eq!(
                        curr.props.borrow().contains_key(
                            safe_cex_exchange::js_runtime::value::utf16("__proto__").as_ref()
                        ),
                        name == "own-proto-asset",
                        "{label}: native prototype assignment own-key policy"
                    );
                    assert_eq!(
                        matches!(&*curr.proto.borrow(), Some(EventValue::Object(_))),
                        name == "proto-asset",
                        "{label}: native prototype setter exercised"
                    );
                    let stored = if name == "proto-asset" {
                        curr.proto.borrow().clone().unwrap()
                    } else {
                        EventValue::Object(curr.clone()).get("__proto__").unwrap()
                    };
                    native_num(
                        field(&stored, "total"),
                        5.0,
                        &format!("{label}: native proto currency total"),
                    );
                    assert!(
                        field(&stored, "info").same(first),
                        "{label}: native proto asset info identity"
                    );
                }
            }
        }
        assert!(!s.flags.get(), "{label}: native pending unaffected");
        assert_eq!(s.clocks.get(), 0, "{label}: native no virtual clock");
        projections.push(projected(&s, result.as_ref().err()));
    }
    compare(projections, v, "spot-account");
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn futures_order_events() {
    order_test(Venue::Futures, "regular")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn pm_order_events() {
    order_test(Venue::PM, "regular")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn spot_order_events() {
    order_test(Venue::Spot, "regular")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn futures_algo_events() {
    order_test(Venue::Futures, "algo")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn pm_conditional_events() {
    order_test(Venue::PM, "conditional")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn futures_account_events() {
    account_test(Venue::Futures)
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn pm_account_events() {
    account_test(Venue::PM)
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn spot_account_events() {
    spot_account_test()
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
