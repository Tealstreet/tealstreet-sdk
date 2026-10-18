use safe_cex_exchange::{client_order_id::order_carries_client_order_id, runtime::ExchangeError};
use safe_cex_num::ProtoMember;
use safe_cex_types::{Order, OrderSide, OrderStatus, OrderStopTriggerType, OrderType};
use safe_cex_venue_binance::{private_event_value::*, private_rest::*, request_weight::Failure};
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
fn value(v: Value) -> EventValue {
    if v == json!("UNDEFINED") {
        EventValue::Undefined
    } else {
        EventValue::from_json(v)
    }
}
fn wire(v: &EventValue) -> Value {
    match v {
        EventValue::Undefined => json!({"$undefined":true}),
        EventValue::Null => Value::Null,
        EventValue::Bool(b) => json!(b),
        EventValue::Number(n) => {
            if n.is_nan() {
                json!("nan")
            } else if n.is_infinite() {
                json!(if n.is_sign_negative() {
                    "-infinity"
                } else {
                    "infinity"
                })
            } else if *n == 0.0 && n.is_sign_negative() {
                json!("-zero")
            } else {
                json!(n)
            }
        }
        EventValue::String(s) => json!(String::from_utf16(s).unwrap()),
        EventValue::Builtin(reference) => {
            if reference.is_callable() {
                json!({"$function":reference.js_string()})
            } else {
                json!({"$prototype":true})
            }
        }
        EventValue::Array(a) => Value::Array(
            array_values(a)
                .expect("fixture array reads")
                .iter()
                .map(wire)
                .collect(),
        ),
        EventValue::Object(o) => {
            let mut out = v
                .spread()
                .iter()
                .map(|(k, v)| (k.clone(), wire(v)))
                .collect::<serde_json::Map<_, _>>();
            if let Some(p) = &*o.proto.borrow() {
                if !matches!(p, EventValue::Builtin(reference) if reference.member() == safe_cex_num::BuiltinIdentity::Object(ProtoMember::Prototype))
                {
                    out.insert("$customPrototype".into(), wire(p));
                }
            }
            Value::Object(out)
        }
        _ => panic!("fixture projection requires decoded data"),
    }
}
fn prop(v: &EventValue, key: &str) -> EventValue {
    v.get(key).unwrap()
}
fn num(v: EventValue, n: f64, label: &str) {
    let EventValue::Number(a) = v else {
        panic!("{label}: native Number variant")
    };
    assert!(
        if n.is_nan() {
            a.is_nan()
        } else if n == 0.0 {
            a == 0.0 && a.is_sign_negative() == n.is_sign_negative()
        } else {
            a == n
        },
        "{label}: native value {a:?} vs expected {n:?}"
    )
}
fn val(v: EventValue, n: EventValue, label: &str) {
    match (v, n) {
        (EventValue::String(a), EventValue::String(b)) => {
            assert_eq!(a, b, "{label}: native string")
        }
        (EventValue::Bool(a), EventValue::Bool(b)) => assert_eq!(a, b, "{label}: native bool"),
        (EventValue::Undefined, EventValue::Undefined) => {}
        (EventValue::Builtin(actual), EventValue::Builtin(expected)) => {
            assert_eq!(
                actual.member(),
                expected.member(),
                "{label}: native inherited table member identity"
            );
            assert!(
                Rc::ptr_eq(actual.realm(), &safe_cex_num::PrototypeRealm::current()),
                "{label}: native inherited originating realm"
            );
        }
        _ => panic!("{label}: native variant mismatch"),
    }
}
fn items(v: &EventValue) -> Vec<EventValue> {
    let EventValue::Array(a) = v else {
        panic!("native Array")
    };
    array_values(a).expect("fixture array reads")
}
fn own(v: &EventValue, k: &str) -> bool {
    if matches!(v, EventValue::Object(_)) {
        v.has_own(k)
    } else {
        false
    }
}
#[derive(Clone)]
struct Bind(Rc<State>);
struct State {
    f: Value,
    venue: &'static str,
    family: &'static str,
    payload: EventValue,
    algo: EventValue,
    trace: RefCell<Vec<Value>>,
    notices: RefCell<Vec<(SnapshotMethod, Vec<SnapshotArgument>)>>,
    requests: RefCell<Vec<(SnapshotClient, &'static str, Option<EventValue>)>>,
    clocks: Cell<usize>,
    calls: Cell<usize>,
    stale: Cell<bool>,
    error: Failure,
    algo_error: Failure,
    orders: EventValue,
    balance: EventValue,
    positions: EventValue,
}
fn setup(f: Value, venue: &'static str, family: &'static str) -> Bind {
    let mut payload = if family == "balance" {
        value(f[venue].clone())
    } else if family == "combined" {
        EventValue::Undefined
    } else {
        value(f["payload"].clone())
    };
    if venue == "spot" && family == "orders" {
        if let EventValue::Array(a) = &mut payload {
            if let Some(o) = native_array_fixture::slots(&a)
                .first()
                .and_then(Option::as_ref)
            {
                if f["name"] == "stop" {
                    o.write("type", string("STOP_LOSS_LIMIT"))
                }
                if f["name"] == "take-profit" {
                    o.write("type", string("TAKE_PROFIT_LIMIT"))
                }
            }
        }
    }
    let mut e = ExchangeError::new(if f["name"] == "empty-message" {
        ""
    } else {
        "snapshot fixture"
    });
    e.name = "RangeError".into();
    e.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "snapshot-code"
    )));
    e.response = Some(
        json!({"status":503,"data":{"msg":"venue detail","code":if f["name"]=="not-found-code"{json!(-2013)}else if f["name"]=="string-not-found-code"{json!("-2013")}else{json!(-1000)}}}),
    );
    let mut e2 = e.clone();
    e2.message = "algo first".into();
    e2.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "algo-code"
    )));
    e2.response = Some(json!({"status":503,"data":{"msg":"algo detail","code":-1001}}));
    let orders = value(
        json!([{"id":"cached","symbol":"BTCUSDT","status":"open","type":"limit","side":"buy","price":8,"amount":2,"filled":1,"remaining":1,"reduceOnly":false,"lastUpdate":9,"info":{"clientOrderId":"client-A"}}]),
    );
    // Store graphs carry actual native enum variants, not source string spellings.
    for o in items(&orders) {
        o.write(
            "status",
            EventValue::enum_value(PrivateEnum::Status(OrderStatus::Open)),
        );
        o.write(
            "type",
            EventValue::enum_value(PrivateEnum::Type(OrderType::Limit)),
        );
        o.write(
            "side",
            EventValue::enum_value(PrivateEnum::Side(OrderSide::Buy)),
        );
    }
    let stale = f["name"] == "already-stale";
    let algo = value(f["algo"].clone());
    Bind(Rc::new(State {
        f,
        venue,
        family,
        payload,
        algo,
        trace: Default::default(),
        notices: Default::default(),
        requests: Default::default(),
        clocks: Cell::new(0),
        calls: Cell::new(0),
        stale: Cell::new(stale),
        error: Rc::new(e),
        algo_error: Rc::new(e2),
        orders,
        balance: value(json!({"free":1,"used":2,"total":3,"upnl":4})),
        positions: value(json!([{"id":"cached-position","contracts":7}])),
    }))
}
impl State {
    fn name(&self) -> &str {
        self.f["name"].as_str().unwrap()
    }
    fn t(&self, v: Value) {
        self.trace.borrow_mut().push(v)
    }
    fn fails(&self) -> bool {
        matches!(
            self.name(),
            "get-reject"
                | "notice-fail"
                | "fallback-fail"
                | "capture-fail"
                | "not-found-code"
                | "string-not-found-code"
                | "empty-message"
        )
    }
}
impl SnapshotBindings for Bind {
    fn get(
        &mut self,
        client: SnapshotClient,
        ep: &'static str,
        params: Option<EventValue>,
    ) -> Result<SnapshotRequestFuture, Failure> {
        let s = self.0.clone();
        let call = s.calls.get() + 1;
        s.calls.set(call);
        s.t(json!([
            "get",
            match client {
                SnapshotClient::Private => "private",
                SnapshotClient::Papi => "papi",
                SnapshotClient::Primary => "primary",
            },
            ep,
            params.as_ref().map(wire).unwrap_or(Value::Null)
        ]));
        s.requests.borrow_mut().push((client, ep, params));
        if s.name() == "get-sync-fail" {
            return Err(s.error.clone());
        }
        if s.name() == "earlier-algo-reject" && s.venue == "futures" {
            if call == 2 {
                s.t(json!(["resolve", 2]));
                s.t(json!(["resolve", 1]));
            }
            return Ok(Box::pin(async move {
                SnapshotCompletion {
                    reaction_order: if call == 1 { 2 } else { 1 },
                    result: Err(if call == 1 {
                        s.error.clone()
                    } else {
                        s.algo_error.clone()
                    }),
                }
            }));
        }
        Ok(Box::pin(async move {
            s.t(json!(["resolve", call]));
            SnapshotCompletion {
                reaction_order: call as u64,
                result: if s.fails() {
                    Err(s.error.clone())
                } else {
                    Ok(
                        if s.family == "orders" && s.venue == "futures" && call == 2 {
                            s.algo.clone()
                        } else {
                            s.payload.clone()
                        },
                    )
                },
            }
        }))
    }
    fn capture(&mut self, m: SnapshotMethod) -> Result<SnapshotCall, Failure> {
        let s = self.0.clone();
        let name = method(m);
        s.t(json!(["capture", name]));
        if s.name() == "capture-fail" {
            return Err(s.error.clone());
        }
        Ok(Rc::new(move |args| {
            s.t(json!(["call", name]));
            s.notices.borrow_mut().push((m, args));
            if s.name() == "notice-fail" {
                Err(s.error.clone())
            } else {
                Ok(())
            }
        }))
    }
    fn now(&mut self) -> Result<f64, Failure> {
        let s = &self.0;
        s.t(json!(["clock"]));
        s.clocks.set(s.clocks.get() + 1);
        if s.name() == "clock-fail" {
            return Err(s.error.clone());
        }
        if let EventValue::Array(a) = &s.payload {
            let first = native_array_fixture::slots(&a)
                .first()
                .and_then(Option::as_ref)
                .cloned();
            if let Some(o) = first {
                if s.name() == "clock-mutates" {
                    o.write("origQty", string("7.10"));
                    o.write("executedQty", string("0.20"));
                    o.write("type", string("STOP_MARKET"));
                    items(&s.algo)[0].write("quantity", string("9"));
                }
                if s.name() == "clock-bad-toString" {
                    o.write("origQty", record([("toString", number(0.0))]));
                }
                if s.name() == "clock-appends" && s.clocks.get() == 1 {
                    let append = value(s.f["payload"][0].clone());
                    EventValue::Array(a.clone()).push(append).unwrap();
                }
            }
        }
        Ok(10000.0 + s.clocks.get() as f64)
    }
    fn orders(&mut self) -> Result<EventValue, Failure> {
        let s = &self.0;
        s.t(json!(["orders"]));
        if matches!(s.name(), "fallback-fail" | "orders-fail") {
            return Err(s.error.clone());
        }
        Ok(if s.family == "lookup" && s.name() != "store-hit" {
            EventValue::array(vec![])
        } else {
            s.orders.clone()
        })
    }
    fn balance(&mut self) -> Result<EventValue, Failure> {
        let s = &self.0;
        s.t(json!(["balance"]));
        if s.name() == "fallback-fail" {
            Err(s.error.clone())
        } else {
            Ok(s.balance.clone())
        }
    }
    fn positions(&mut self) -> Result<EventValue, Failure> {
        self.0.t(json!(["positions"]));
        Ok(self.0.positions.clone())
    }
    fn asset_price(&mut self, asset: EventValue) -> Result<f64, Failure> {
        let s = &self.0;
        s.t(json!(["price", wire(&asset)]));
        if s.name() == "price-fail" {
            return Err(s.error.clone());
        }
        if s.name() == "price-appends" {
            if let EventValue::Array(a) = prop(&s.payload, "balances") {
                if a.len() == 2 {
                    EventValue::Array(a.clone())
                        .push(value(json!({"asset":"ETH","free":"1","locked":"1"})))
                        .unwrap();
                }
            }
        }
        Ok(match asset.text()?.as_str() {
            "BTC" => 10.0,
            "USDT" => 1.0,
            "ETH" => 5.0,
            _ => 0.0,
        })
    }
    fn market(&mut self, symbol: &str) -> Result<EventValue, Failure> {
        self.0.t(json!(["market", symbol]));
        Ok(if self.0.name() == "market-miss" {
            EventValue::Undefined
        } else {
            record([])
        })
    }
    fn has_primary(&mut self) -> Result<bool, Failure> {
        self.0.t(json!(["primary"]));
        Ok(self.0.name() == "primary")
    }
    fn carries_client_id(&mut self, o: &EventValue, id: &str) -> Result<bool, Failure> {
        self.0.t(json!(["carries", id]));
        let order: Order = serde_json::from_value(wire(o)).expect("authored typed Store order");
        order_carries_client_order_id(&order, id)
            .map_err(|error| Rc::new(safe_cex_exchange::runtime::ExchangeError::from(error)))
    }
    fn map_history(&mut self, data: EventValue) -> Result<EventValue, Failure> {
        let s = &self.0;
        s.t(json!(["history"]));
        if s.name() == "map-fail" {
            return Err(s.error.clone());
        }
        safe_cex_venue_binance::order_history::map_binance_historical_order(
            &data,
            &safe_cex_num::PrototypeRealm::current(),
        )
        .map(|value| value.unwrap_or(EventValue::Null))
    }
}
fn method(m: SnapshotMethod) -> &'static str {
    match m {
        SnapshotMethod::ConsoleError => "console",
        SnapshotMethod::Log => "log",
        SnapshotMethod::Emit => "emit",
    }
}
fn error_wire(e: &Failure) -> Value {
    json!({"$error":true,"name":e.name,"message":e.message,"code":observed_error_code(&e.code).clone().unwrap_or(json!({"$undefined":true})),"response":e.response.clone().unwrap_or(json!({"$undefined":true}))})
}
fn project(s: &State, result: &Result<EventValue, Failure>) -> Value {
    let v = result
        .as_ref()
        .ok()
        .cloned()
        .unwrap_or(EventValue::Undefined);
    let list = if let EventValue::Array(a) = &v {
        array_values(a).expect("fixture array reads")
    } else {
        vec![]
    };
    let rows = if let EventValue::Array(a) = &s.payload {
        array_values(a).expect("fixture array reads")
    } else {
        vec![]
    };
    let algos = if let EventValue::Array(a) = &s.algo {
        array_values(a).expect("fixture array reads")
    } else {
        vec![]
    };
    let maybe = |key| {
        if v.nullish() {
            EventValue::Undefined
        } else {
            prop(&v, key)
        }
    };
    let bal = maybe("balance");
    let ord = maybe("order");
    let info = if s.family == "lookup" {
        json!(own(&ord, "info") && prop(&ord, "info").same(&s.payload))
    } else if s.family == "balance" {
        json!(own(&bal, "info") && prop(&bal, "info").same(&s.payload))
    } else {
        Value::Null
    };
    let notices=s.notices.borrow().iter().map(|(m,args)|json!({"method":method(*m),"args":args.iter().map(|a|match a{SnapshotArgument::Value(v)=>wire(v),SnapshotArgument::Error(e)=>error_wire(e)}).collect::<Vec<_>>()})).collect::<Vec<_>>();
    json!({"name":s.name(),"trace":*s.trace.borrow(),"notices":notices,"clocks":s.clocks.get(),"stale":s.stale.get(),"refs":{"fallbackOrders":v.same(&s.orders),"balance":bal.same(&s.balance),"positions":maybe("positions").same(&s.positions),"order":ord.same(&items(&s.orders)[0]),"info":info,"regularInfo":list.iter().filter(|o|!o.nullish()&&own(o,"info")&&rows.iter().any(|r|prop(o,"info").same(r))).count(),"algoInfo":list.iter().filter(|o|!o.nullish()&&own(o,"info")&&algos.iter().any(|a|!prop(o,"info").same(a)&&prop(&prop(o,"info"),"nested").same(&prop(a,"nested")))).count()},"value":wire(&v),"failure":result.as_ref().err().map(|e|json!({"original":Rc::ptr_eq(e,&s.error),"name":e.name,"message":e.message,"code":observed_error_code(&e.code).clone().unwrap_or(json!({"$undefined":true})),"response":e.response.clone().unwrap_or(json!({"$undefined":true}))}))})
}
fn fixtures(family: &str, n: usize) -> Vec<Value> {
    let all: Vec<Value> = serde_json::from_str(include_str!("fixtures/private-rest.json")).unwrap();
    let rows = all
        .into_iter()
        .filter(|f| f["family"] == family)
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), n, "independent authored case floor");
    rows
}
fn compare(rows: Vec<Value>, venue: &str, family: &str) {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-rest.mjs"
        ))
        .args([venue, family])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        expected.len(),
        rows.len(),
        "source case count (native floor asserted independently first)"
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
        )
    }
}
fn original_error(e: &Failure, s: &State, label: &str) {
    assert!(
        Rc::ptr_eq(e, &s.error),
        "{label}: native original error identity"
    );
    assert_eq!(e.name, "RangeError", "{label}: native error name");
    assert_eq!(
        e.message,
        if s.name() == "empty-message" {
            ""
        } else {
            "snapshot fixture"
        },
        "{label}: native error message"
    );
    assert_eq!(
        observed_error_code(&e.code),
        Some(json!("snapshot-code")),
        "{label}: native error code"
    );
    assert_eq!(
        e.response,
        Some(
            json!({"status":503,"data":{"msg":"venue detail","code":if s.name()=="not-found-code"{json!(-2013)}else if s.name()=="string-not-found-code"{json!("-2013")}else{json!(-1000)}}})
        ),
        "{label}: native authored error response"
    );
}
fn generated_message(venue: &str, family: &str, name: &str) -> String {
    if name == "null-row" {
        return "Cannot read properties of null (reading 'symbol')".into();
    }
    if name == "primitive-row" {
        return format!(
            "Cannot use 'in' operator to search for '{}' in 7",
            if venue == "futures" {
                "orderId"
            } else if family == "conditional" {
                "newClientStrategyId"
            } else {
                "clientOrderId"
            }
        );
    }
    if name == "bad-id" {
        return "v(...)?.toString is not a function".into();
    }
    if name == "clock-bad-toString" {
        return "num?.toString is not a function".into();
    }
    if venue == "spot" {
        return if name == "nonarray" {
            "data.map is not a function".into()
        } else {
            format!(
                "Cannot read properties of {} (reading 'map')",
                if name == "missing-data" {
                    "undefined"
                } else {
                    "null"
                }
            )
        };
    }
    let ep = if family == "conditional" {
        "/papi/v1/um/conditional/openOrders"
    } else if family == "balance" {
        "/papi/v1/balance"
    } else {
        "/papi/v1/um/openOrders"
    };
    format!(
        "Binance PM {ep} response malformed: expected array, received {}",
        match name {
            "nonarray" => "object(code,msg) (code=-1 shape)",
            "missing-data" => "undefined",
            _ => "null",
        }
    )
}
fn generated_error(e: &Failure, s: &State, name: &str, message: &str, label: &str) {
    assert!(
        !Rc::ptr_eq(e, &s.error) && !Rc::ptr_eq(e, &s.algo_error),
        "{label}: native generated error not original"
    );
    assert_eq!(e.name, name, "{label}: native generated error name");
    assert_eq!(
        e.message, message,
        "{label}: native generated error message"
    );
    assert_eq!(
        observed_error_code(&e.code),
        None,
        "{label}: native generated error code absent"
    );
    assert_eq!(
        e.response, None,
        "{label}: native generated error response absent"
    );
}
fn native_order(v: &EventValue, s: &State, family: &str, label: &str) {
    let name = s.name();
    let conditional = family == "conditional";
    let futures = s.venue == "futures";
    let id = if futures {
        string("101")
    } else if conditional {
        string("strategy-A")
    } else {
        string("client-A")
    };
    val(prop(v, "id"), id, &format!("{label}: native order id"));
    val(
        prop(v, "status"),
        EventValue::enum_value(PrivateEnum::Status(OrderStatus::Open)),
        &format!("{label}: native status"),
    );
    val(
        prop(v, "symbol"),
        string("BTCUSDT"),
        &format!("{label}: native symbol"),
    );
    val(
        prop(v, "side"),
        EventValue::enum_value(PrivateEnum::Side(OrderSide::Buy)),
        &format!("{label}: native side"),
    );
    let typ = if name == "prototype-type" {
        EventValue::inherited(ProtoMember::Constructor)
    } else if name == "unknown-type" {
        EventValue::Undefined
    } else {
        EventValue::enum_value(PrivateEnum::Type(if conditional {
            if name == "stop" {
                OrderType::TakeProfit
            } else {
                OrderType::StopLoss
            }
        } else if name == "stop" {
            OrderType::StopLoss
        } else if name == "take-profit" {
            OrderType::TakeProfit
        } else {
            OrderType::Limit
        }))
    };
    val(prop(v, "type"), typ, &format!("{label}: native order type"));
    num(
        prop(v, "price"),
        if name == "zero-fields" { 0.0 } else { 12.3 },
        &format!("{label}: native price"),
    );
    num(
        prop(v, "amount"),
        if name == "zero-fields" { 0.0 } else { 0.1 },
        &format!("{label}: native amount"),
    );
    num(
        prop(v, "filled"),
        if name == "zero-fields" || name == "missing-qty" && conditional {
            0.0
        } else if name == "missing-qty" {
            f64::NAN
        } else {
            0.03
        },
        &format!("{label}: native filled"),
    );
    num(
        prop(v, "remaining"),
        if name == "clock-mutates" {
            6.9
        } else if name == "zero-fields" {
            0.0
        } else if name == "missing-qty" {
            if conditional { 0.1 } else { f64::NAN }
        } else {
            0.07
        },
        &format!("{label}: native remainder"),
    );
    num(
        prop(v, "lastUpdate"),
        10001.0,
        &format!("{label}: native timestamp"),
    );
    val(
        prop(v, "reduceOnly"),
        EventValue::Bool(false),
        &format!("{label}: native reduceOnly"),
    );
    assert_eq!(
        own(v, "close"),
        futures,
        "{label}: native close own-key policy"
    );
    if futures {
        val(
            prop(v, "close"),
            EventValue::Bool(name != "zero-fields"),
            &format!("{label}: native close"),
        );
    }
    let row = items(&s.payload)[0].clone();
    assert!(
        prop(v, "info").same(&row),
        "{label}: native original row info identity"
    );
    let trigger_type = if name == "clock-mutates" || name == "stop" {
        Some("slTriggerType")
    } else if name == "take-profit" {
        Some("tpTriggerType")
    } else {
        None
    };
    for k in ["slTriggerType", "tpTriggerType"] {
        let expect = s.venue != "spot" && trigger_type == Some(k);
        assert_eq!(
            own(v, k),
            expect,
            "{label}: native conditional trigger own-key policy"
        );
        if expect {
            val(
                prop(v, k),
                EventValue::enum_value(PrivateEnum::Trigger(OrderStopTriggerType::MarkPrice)),
                &format!("{label}: native trigger enum"),
            );
        }
    }
}
fn argument_value(args: &[SnapshotArgument], index: usize, label: &str) -> EventValue {
    let Some(SnapshotArgument::Value(v)) = args.get(index) else {
        panic!("{label}: native value argument")
    };
    v.clone()
}
fn order_tests(venue: &'static str, family: &'static str) {
    let mut projections = vec![];
    for (i, f) in fixtures("orders", 24).into_iter().enumerate() {
        let mut b = setup(f, venue, family);
        let s = b.0.clone();
        let name = s.name();
        let label = format!("native {venue} {family} case{i} {name}");
        let result = match (venue, family) {
            ("futures", _) => run(fetch_futures_orders(&s.stale, &mut b)),
            ("pm", "regular") => run(fetch_pm_regular_orders(&mut b)),
            ("pm", "conditional") => run(fetch_pm_conditional_orders(&mut b)),
            _ => run(fetch_spot_orders(&mut b)),
        };
        let generated = matches!(name, "null-row" | "primitive-row" | "clock-bad-toString")
            || venue == "futures" && name == "bad-id";
        let malformed = matches!(name, "nonarray" | "missing-data" | "null-data");
        let failed_get = matches!(
            name,
            "get-sync-fail" | "get-reject" | "capture-fail" | "empty-message"
        ) || venue == "futures" && name == "earlier-algo-reject";
        let mapper_fail = generated || name == "clock-fail";
        let err = if venue == "futures" {
            name == "capture-fail"
        } else {
            malformed || mapper_fail || failed_get
        };
        assert_eq!(result.is_err(), err, "{label}: native error disposition");
        let fallback = venue == "futures" && (mapper_fail || failed_get) && !err;
        assert_eq!(s.stale.get(), fallback, "{label}: native stale state");
        assert_eq!(
            s.requests.borrow().len(),
            if venue == "futures" && name != "get-sync-fail" {
                2
            } else {
                1
            },
            "{label}: native independent request count"
        );
        let expected_endpoint = if venue == "pm" {
            if family == "conditional" {
                "/papi/v1/um/conditional/openOrders"
            } else {
                "/papi/v1/um/openOrders"
            }
        } else if venue == "spot" {
            "/api/v3/openOrders"
        } else {
            "/fapi/v1/openOrders"
        };
        assert_eq!(
            s.requests.borrow()[0].1,
            expected_endpoint,
            "{label}: native request endpoint"
        );
        assert_eq!(
            s.requests.borrow()[0].0,
            if venue == "pm" {
                SnapshotClient::Papi
            } else {
                SnapshotClient::Private
            },
            "{label}: native client variant"
        );
        if let Some((_, ep, params)) = s.requests.borrow().get(1) {
            assert_eq!(
                *ep, "/fapi/v1/openAlgoOrders",
                "{label}: native algo endpoint"
            );
            val(
                prop(params.as_ref().unwrap(), "algoType"),
                string("CONDITIONAL"),
                &format!("{label}: native algo request type"),
            );
        }
        let clocks = if failed_get
            || malformed && venue != "futures"
            || generated && name != "clock-bad-toString"
        {
            0
        } else if name == "clock-fail" || name == "clock-bad-toString" {
            1
        } else if venue == "futures" {
            if name == "empty" || malformed { 1 } else { 2 }
        } else if name == "empty" {
            0
        } else {
            1
        };
        assert_eq!(s.clocks.get(), clocks, "{label}: native clock count");
        if let Err(e) = &result {
            if failed_get || name == "clock-fail" {
                original_error(e, &s, &label)
            } else {
                generated_error(
                    e,
                    &s,
                    if venue == "pm" && malformed {
                        "Error"
                    } else {
                        "TypeError"
                    },
                    &generated_message(venue, family, name),
                    &label,
                );
                assert_eq!(
                    e.name,
                    if venue == "pm" && malformed {
                        "Error"
                    } else {
                        "TypeError"
                    },
                    "{label}: native generated error name"
                );
                if name == "clock-bad-toString" {
                    assert_eq!(
                        e.message, "num?.toString is not a function",
                        "{label}: native raw subtraction optional-call error"
                    );
                }
                if name == "null-row" {
                    assert_eq!(
                        e.message, "Cannot read properties of null (reading 'symbol')",
                        "{label}: native null row error"
                    );
                }
            }
        }
        if venue == "futures" && malformed {
            let notices = s.notices.borrow();
            assert_eq!(
                notices.len(),
                2,
                "{label}: native malformed diagnostics count"
            );
            assert_eq!(
                notices[0].0,
                SnapshotMethod::ConsoleError,
                "{label}: native malformed console method"
            );
            assert_eq!(
                notices[1].0,
                SnapshotMethod::Log,
                "{label}: native malformed log method"
            );
            val(
                argument_value(&notices[0].1, 0, &label),
                string("[Binance] fetchOrders received non-array response:"),
                &format!("{label}: native malformed console label"),
            );
            assert!(
                argument_value(&notices[0].1, 1, &label).same(&s.payload),
                "{label}: native original malformed response argument"
            );
            let json = match name {
                "nonarray" => "{\"code\":-1,\"msg\":\"shape\"}",
                "missing-data" => "undefined",
                _ => "null",
            };
            val(
                argument_value(&notices[1].1, 0, &label),
                string(format!("fetchOrders received non-array response: {json}")),
                &format!("{label}: native malformed JSON.stringify log"),
            );
            val(
                argument_value(&notices[1].1, 1, &label),
                string("error"),
                &format!("{label}: native log severity"),
            );
        }
        if fallback {
            let v = result.as_ref().unwrap();
            assert!(
                v.same(&s.orders),
                "{label}: native fallback Store array identity"
            );
            assert_eq!(items(v).len(), 1, "{label}: native fallback count");
            val(
                prop(&items(v)[0], "id"),
                string("cached"),
                &format!("{label}: native fallback id"),
            );
            assert_eq!(
                s.notices.borrow().len(),
                3,
                "{label}: native catch notice count"
            );
            let notices = s.notices.borrow();
            val(
                argument_value(&notices[0].1, 0, &label),
                string("[Binance] fetchOrders error:"),
                &format!("{label}: native catch console label"),
            );
            val(
                argument_value(&notices[1].1, 0, &label),
                string("[Binance] fetchOrders error response:"),
                &format!("{label}: native response console label"),
            );
            val(
                argument_value(&notices[2].1, 1, &label),
                string("error"),
                &format!("{label}: native catch log severity"),
            );
            let msg = if name == "earlier-algo-reject" {
                "algo first".into()
            } else if name == "empty-message" {
                "RangeError".into()
            } else if failed_get || name == "clock-fail" {
                "snapshot fixture".into()
            } else {
                generated_message(venue, family, name)
            };
            val(
                argument_value(&notices[2].1, 0, &label),
                string(format!("fetchOrders error: {msg}")),
                &format!("{label}: native catch log message"),
            );
            let response = argument_value(&notices[1].1, 1, &label);
            if failed_get || name == "clock-fail" {
                val(
                    prop(&response, "msg"),
                    string(if name == "earlier-algo-reject" {
                        "algo detail"
                    } else {
                        "venue detail"
                    }),
                    &format!("{label}: native response msg"),
                );
                num(
                    prop(&response, "code"),
                    if name == "earlier-algo-reject" {
                        -1001.0
                    } else {
                        -1000.0
                    },
                    &format!("{label}: native response code"),
                );
            } else {
                val(
                    response,
                    EventValue::Undefined,
                    &format!("{label}: native generated error response"),
                );
            }
            let SnapshotArgument::Error(e) = &notices[0].1[1] else {
                panic!("{label}: native diagnostic Error variant")
            };
            if name == "earlier-algo-reject" {
                assert!(
                    Rc::ptr_eq(e, &s.algo_error),
                    "{label}: native earliest aggregate rejection identity"
                );
                assert_eq!(
                    e.message, "algo first",
                    "{label}: native earliest rejection message"
                );
                assert_eq!(
                    e.name, "RangeError",
                    "{label}: native earlier rejection name"
                );
                assert_eq!(
                    observed_error_code(&e.code),
                    Some(json!("algo-code")),
                    "{label}: native earlier rejection code"
                );
                assert_eq!(
                    e.response,
                    Some(json!({"status":503,"data":{"msg":"algo detail","code":-1001}})),
                    "{label}: native earlier rejection response"
                )
            } else if failed_get || name == "clock-fail" {
                original_error(e, &s, &label)
            } else {
                generated_error(
                    e,
                    &s,
                    "TypeError",
                    &generated_message(venue, family, name),
                    &label,
                );
                assert_eq!(
                    e.name, "TypeError",
                    "{label}: native caught generated error name"
                );
                if name == "bad-id" {
                    assert_eq!(
                        e.message, "v(...)?.toString is not a function",
                        "{label}: native optional ID error"
                    );
                }
            }
            if name == "empty-message" {
                let SnapshotArgument::Value(msg) = &notices[2].1[0] else {
                    panic!()
                };
                val(
                    msg.clone(),
                    string("fetchOrders error: RangeError"),
                    &format!("{label}: native falsy-message Error coercion"),
                );
            }
        }
        if let Ok(v) = &result {
            if !fallback {
                let rows = items(v);
                let expected = if name == "empty" {
                    usize::from(venue == "futures")
                } else if malformed {
                    1
                } else {
                    1 + usize::from(venue == "futures")
                };
                assert_eq!(
                    rows.len(),
                    expected,
                    "{label}: native independent snapshot count"
                );
                if name != "empty" && !malformed {
                    native_order(&rows[0], &s, family, &label)
                }
                if venue == "futures" {
                    let a = rows.last().unwrap();
                    val(
                        prop(a, "id"),
                        string("501"),
                        &format!("{label}: native algo id"),
                    );
                    val(
                        prop(a, "type"),
                        EventValue::enum_value(PrivateEnum::Type(OrderType::TakeProfit)),
                        &format!("{label}: native algo type"),
                    );
                    val(
                        prop(a, "status"),
                        EventValue::enum_value(PrivateEnum::Status(OrderStatus::Open)),
                        &format!("{label}: native algo status"),
                    );
                    num(
                        prop(a, "amount"),
                        if name == "zero-fields" {
                            0.0
                        } else if name == "clock-mutates" {
                            9.0
                        } else {
                            0.1
                        },
                        &format!("{label}: native algo amount"),
                    );
                    let original = items(&s.algo)[0].clone();
                    assert!(
                        !prop(a, "info").same(&original),
                        "{label}: native algo info shallow-copy identity"
                    );
                    assert!(
                        prop(&prop(a, "info"), "nested").same(&prop(&original, "nested")),
                        "{label}: native algo nested identity"
                    );
                    num(
                        prop(&prop(a, "info"), "algoId"),
                        501.0,
                        &format!("{label}: native algo info id"),
                    );
                }
            }
        }
        projections.push(project(&s, &result));
    }
    compare(projections, venue, family)
}
fn bound_method(
    s: Rc<State>,
    which: &'static str,
) -> impl FnMut() -> Result<SnapshotFuture, Failure> {
    move || {
        s.t(json!(["capture", which]));
        let part = if which == "first" {
            "regular"
        } else {
            "conditional"
        };
        if s.name() == format!("{part}-sync-fail") {
            return Err(s.error.clone());
        }
        s.t(json!(["start", which]));
        let s = s.clone();
        Ok(Box::pin(async move {
            s.t(json!(["resolve-method", which]));
            if s.name() == format!("{part}-reject")
                || which == "first"
                    && matches!(s.name(), "already-stale" | "notice-fail" | "fallback-fail")
            {
                return Err(s.error.clone());
            }
            Ok(if s.name() == format!("{part}-noniterable") {
                record([])
            } else if s.name() == "string-iterables" {
                string(if which == "first" { "ab" } else { "c" })
            } else if s.name() == "empty" {
                EventValue::array(vec![])
            } else {
                value(s.f[which].clone())
            })
        }))
    }
}
fn combined_test() {
    let mut projections = vec![];
    for (i, f) in fixtures("combined", 12).into_iter().enumerate() {
        let mut b = setup(f, "pm", "combined");
        let s = b.0.clone();
        let name = s.name();
        let label = format!("native pm combined case{i} {name}");
        let result = run(fetch_pm_orders(
            &s.stale,
            bound_method(s.clone(), "first"),
            bound_method(s.clone(), "second"),
            &mut b,
        ));
        let failed = !matches!(name, "normal" | "empty" | "string-iterables");
        let err = matches!(name, "notice-fail" | "fallback-fail");
        assert_eq!(result.is_err(), err, "{label}: native error disposition");
        assert_eq!(
            s.stale.get(),
            failed && name != "notice-fail",
            "{label}: native stale"
        );
        if let Err(e) = &result {
            original_error(e, &s, &label)
        } else {
            let v = result.as_ref().unwrap();
            if failed {
                assert!(v.same(&s.orders), "{label}: native Store fallback identity");
                assert_eq!(items(v).len(), 1, "{label}: native fallback count");
            } else {
                let rows = items(v);
                assert_eq!(
                    rows.len(),
                    if name == "empty" {
                        0
                    } else if name == "string-iterables" {
                        3
                    } else {
                        2
                    },
                    "{label}: native independent count"
                );
                if name == "normal" {
                    val(
                        prop(&rows[0], "id"),
                        string("regular"),
                        &format!("{label}: native first order id"),
                    );
                    num(
                        prop(&rows[0], "amount"),
                        2.5,
                        &format!("{label}: native first amount"),
                    );
                    val(
                        prop(&rows[1], "id"),
                        string("conditional"),
                        &format!("{label}: native second id"),
                    );
                }
                if name == "string-iterables" {
                    for (i, c) in ["a", "b", "c"].iter().enumerate() {
                        val(
                            rows[i].clone(),
                            string(*c),
                            &format!("{label}: native iterable character"),
                        )
                    }
                }
            }
        }
        let notices = s.notices.borrow();
        assert_eq!(
            notices.len(),
            usize::from(failed),
            "{label}: native notice count"
        );
        if failed {
            let SnapshotArgument::Value(message) = &notices[0].1[0] else {
                panic!("{label}: native notice message variant")
            };
            assert_eq!(
                notices[0].1.len(),
                if name == "already-stale" { 2 } else { 1 },
                "{label}: native notice argument count"
            );
            if name == "already-stale" {
                val(
                    argument_value(&notices[0].1, 1, &label),
                    string("warning"),
                    &format!("{label}: native warning severity"),
                );
            }
            let msg = if name == "regular-noniterable" || name == "conditional-noniterable" {
                "(intermediate value) is not iterable"
            } else {
                "venue detail"
            };
            val(
                message.clone(),
                string(if name == "already-stale" {
                    format!("fetchOrders error: {msg}")
                } else {
                    msg.into()
                }),
                &format!("{label}: native notification message"),
            );
            assert_eq!(
                notices[0].0,
                if name == "already-stale" {
                    SnapshotMethod::Log
                } else {
                    SnapshotMethod::Emit
                },
                "{label}: native notification method variant"
            );
        }
        drop(notices);
        projections.push(project(&s, &result));
    }
    compare(projections, "pm", "combined")
}
fn balance_test(venue: &'static str) {
    let mut projections = vec![];
    for (i, f) in fixtures("balance", 14).into_iter().enumerate() {
        let mut b = setup(f, venue, "balance");
        let s = b.0.clone();
        let name = s.name();
        let label = format!("native {venue} balance case{i} {name}");
        let result = if venue == "pm" {
            run(fetch_pm_balance(&mut b))
        } else {
            run(fetch_spot_positions_balance(&mut b))
        };
        let failed = matches!(
            name,
            "null-row"
                | "nonarray"
                | "missing-data"
                | "get-reject"
                | "notice-fail"
                | "fallback-fail"
        ) || venue == "spot" && name == "price-fail";
        let err = matches!(name, "notice-fail" | "fallback-fail");
        assert_eq!(result.is_err(), err, "{label}: native error disposition");
        assert_eq!(
            s.requests.borrow().len(),
            1,
            "{label}: native independent request count"
        );
        assert_eq!(
            s.requests.borrow()[0].1,
            if venue == "pm" {
                "/papi/v1/balance"
            } else {
                "/api/v3/account"
            },
            "{label}: native endpoint"
        );
        assert_eq!(
            s.notices.borrow().len(),
            usize::from(failed),
            "{label}: native notification count"
        );
        if failed {
            let notices = s.notices.borrow();
            assert_eq!(
                notices[0].0,
                SnapshotMethod::Emit,
                "{label}: native emit method"
            );
            let SnapshotArgument::Value(message) = &notices[0].1[0] else {
                panic!("{label}: native emission message variant")
            };
            let msg = if name == "null-row" {
                "Cannot read properties of null (reading 'asset')".into()
            } else if venue == "pm" && matches!(name, "nonarray" | "missing-data") {
                generated_message(venue, "balance", name)
            } else if venue == "spot" && name == "missing-data" {
                "Cannot read properties of undefined (reading 'balances')".into()
            } else if venue == "spot" && name == "nonarray" {
                "object is not iterable (cannot read property Symbol(Symbol.iterator))".into()
            } else {
                "venue detail".into()
            };
            val(
                message.clone(),
                string(msg),
                &format!("{label}: native emitted error message"),
            );
        }
        if let Err(e) = &result {
            original_error(e, &s, &label)
        } else {
            let v = result.as_ref().unwrap();
            let balance = prop(v, "balance");
            if failed {
                assert!(
                    balance.same(&s.balance),
                    "{label}: native fallback balance identity"
                );
                if venue == "spot" {
                    assert!(
                        prop(v, "positions").same(&s.positions),
                        "{label}: native fallback positions identity"
                    )
                }
            } else {
                assert!(
                    prop(&balance, "info").same(&s.payload),
                    "{label}: native original response info identity"
                );
                let (t, f, u, p) = if (matches!(name, "empty" | "unsupported")
                    || name == "proto-asset" && venue == "spot")
                {
                    (0.0, 0.0, 0.0, 0.0)
                } else if venue == "pm" {
                    if name == "negative" {
                        (-4.5, -0.25, -4.25, 0.5)
                    } else if name == "missing-amounts" {
                        (f64::NAN, f64::NAN, f64::NAN, f64::NAN)
                    } else {
                        (16.5, 6.25, 10.25, 0.5)
                    }
                } else if name == "negative" {
                    (-17.0, -24.0, 7.0, 0.0)
                } else if name == "missing-amounts" {
                    (0.0, 0.0, 0.0, 0.0)
                } else if name == "price-appends" {
                    (43.0, 31.0, 12.0, 0.0)
                } else {
                    (33.0, 26.0, 7.0, 0.0)
                };
                num(
                    prop(&balance, "total"),
                    t,
                    &format!("{label}: native total"),
                );
                num(prop(&balance, "free"), f, &format!("{label}: native free"));
                num(prop(&balance, "used"), u, &format!("{label}: native used"));
                num(prop(&balance, "upnl"), p, &format!("{label}: native upnl"));
                if venue == "spot" {
                    assert_eq!(
                        items(&prop(v, "positions")).len(),
                        0,
                        "{label}: native independent empty positions count"
                    );
                    let currencies = prop(&balance, "currencies");
                    let coins = prop(&balance, "coins");
                    let rows = items(&prop(&s.payload, "balances"));
                    assert_eq!(
                        rows.len(),
                        if name == "empty" {
                            0
                        } else if name == "price-appends" {
                            3
                        } else if matches!(name, "unsupported" | "missing-amounts" | "proto-asset")
                        {
                            1
                        } else {
                            2
                        },
                        "{label}: native independent asset count"
                    );
                    if let Some(first) = rows.first() {
                        let asset = prop(first, "asset").text().unwrap();
                        if asset == "__proto__" {
                            let EventValue::Object(c) = &currencies else {
                                panic!()
                            };
                            assert!(
                                !c.props.borrow().contains_key(
                                    &"__proto__".encode_utf16().collect::<Vec<_>>()[..]
                                ),
                                "{label}: native prototype setter own-key policy"
                            );
                            assert!(
                                matches!(&*c.proto.borrow(), Some(EventValue::Object(_))),
                                "{label}: native prototype assigned"
                            );
                            let proto = c.proto.borrow().clone().unwrap();
                            assert!(
                                prop(&proto, "info").same(first),
                                "{label}: native prototype asset info identity"
                            );
                        } else {
                            let curr = prop(&currencies, &asset);
                            assert!(
                                prop(&curr, "info").same(first),
                                "{label}: native original asset info identity"
                            );
                            if name == "normal" {
                                num(
                                    prop(&curr, "total"),
                                    3.0,
                                    &format!("{label}: native BTC amount"),
                                );
                                num(
                                    prop(&prop(&coins, "BTC"), "notional"),
                                    30.0,
                                    &format!("{label}: native BTC notional"),
                                );
                            }
                        }
                    }
                } else {
                    assert!(
                        !own(v, "positions"),
                        "{label}: native no positions property"
                    );
                }
            }
        }
        projections.push(project(&s, &result));
    }
    compare(projections, venue, "balance")
}
fn lookup_test() {
    let mut projections = vec![];
    for (i, f) in fixtures("lookup", 16).into_iter().enumerate() {
        let mut b = setup(f, "futures", "lookup");
        let s = b.0.clone();
        let name = s.name();
        let label = format!("native futures lookup case{i} {name}");
        let result = run(find_futures_order_by_client_id(
            "BTCUSDT", "client-A", &mut b,
        ));
        let err = matches!(
            name,
            "string-not-found-code" | "get-reject" | "map-fail" | "orders-fail"
        );
        assert_eq!(result.is_err(), err, "{label}: native error disposition");
        assert_eq!(
            s.requests.borrow().len(),
            usize::from(!matches!(name, "store-hit" | "market-miss" | "orders-fail")),
            "{label}: native independent request count"
        );
        if let Some((client, ep, params)) = s.requests.borrow().first() {
            assert_eq!(
                *client,
                if name == "primary" {
                    SnapshotClient::Primary
                } else {
                    SnapshotClient::Private
                },
                "{label}: native selected client variant"
            );
            assert_eq!(*ep, "/fapi/v1/order", "{label}: native endpoint");
            val(
                prop(params.as_ref().unwrap(), "symbol"),
                string("BTCUSDT"),
                &format!("{label}: native params symbol"),
            );
            val(
                prop(params.as_ref().unwrap(), "origClientOrderId"),
                string("client-A"),
                &format!("{label}: native params client ID"),
            );
        }
        if let Err(e) = &result {
            original_error(e, &s, &label)
        } else {
            let v = result.as_ref().unwrap();
            let found = !matches!(
                name,
                "market-miss" | "null-data" | "invalid-history" | "not-found-code"
            );
            val(
                prop(v, "found"),
                EventValue::Bool(found),
                &format!("{label}: native found boolean"),
            );
            if !found {
                val(
                    prop(v, "definitive"),
                    EventValue::Bool(name == "not-found-code"),
                    &format!("{label}: native definitive boolean"),
                );
                assert!(!own(v, "order"), "{label}: native absent order");
            } else {
                let o = prop(v, "order");
                if name == "store-hit" {
                    assert!(
                        o.same(&items(&s.orders)[0]),
                        "{label}: native original Store order identity"
                    );
                } else {
                    val(
                        prop(&o, "id"),
                        string("701"),
                        &format!("{label}: native id"),
                    );
                    val(
                        prop(&o, "symbol"),
                        string("BTCUSDT"),
                        &format!("{label}: native query symbol override"),
                    );
                    val(
                        prop(&o, "status"),
                        EventValue::enum_value(PrivateEnum::Status(if name == "closed" {
                            OrderStatus::Closed
                        } else {
                            OrderStatus::Open
                        })),
                        &format!("{label}: native status enum"),
                    );
                    val(
                        prop(&o, "type"),
                        EventValue::enum_value(PrivateEnum::Type(OrderType::Limit)),
                        &format!("{label}: native type enum"),
                    );
                    val(
                        prop(&o, "side"),
                        EventValue::enum_value(PrivateEnum::Side(OrderSide::Buy)),
                        &format!("{label}: native side enum"),
                    );
                    num(
                        prop(&o, "price"),
                        if name == "price-zero" { 0.0 } else { 12.3 },
                        &format!("{label}: native price"),
                    );
                    num(prop(&o, "amount"), 0.1, &format!("{label}: native amount"));
                    num(
                        prop(&o, "filled"),
                        if name == "overfilled" { 2.0 } else { 0.03 },
                        &format!("{label}: native filled"),
                    );
                    num(
                        prop(&o, "remaining"),
                        if name == "overfilled" { 0.0 } else { 0.07 },
                        &format!("{label}: native remaining"),
                    );
                    num(
                        prop(&o, "lastUpdate"),
                        if name == "updated-zero" { 123.0 } else { 456.0 },
                        &format!("{label}: native lastUpdate"),
                    );
                    assert!(
                        prop(&o, "info").same(&s.payload),
                        "{label}: native original HTTP info identity"
                    );
                }
            }
        }
        projections.push(project(&s, &result));
    }
    compare(projections, "futures", "lookup")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn futures_order_snapshots() {
    order_tests("futures", "orders")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn pm_regular_order_snapshots() {
    order_tests("pm", "regular")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn pm_conditional_order_snapshots() {
    order_tests("pm", "conditional")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn spot_order_snapshots() {
    order_tests("spot", "orders")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn pm_combined_order_snapshots() {
    combined_test()
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn pm_balance_snapshots() {
    balance_test("pm")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn spot_balance_snapshots() {
    balance_test("spot")
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn futures_client_order_lookup() {
    lookup_test()
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
