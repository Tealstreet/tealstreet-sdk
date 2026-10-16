use safe_cex_venue_lighter::{
    public_ingress::{self as port, IngressCallback, IngressOps},
    ws_public,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};
type V = Rc<Native>;
type E = V;
enum Native {
    U,
    Null,
    B(bool),
    N(f64),
    S(Vec<u16>),
    O(RefCell<Vec<(V, V)>>),
    A(RefCell<Vec<V>>),
    F(String, usize, IngressCallback<V, E>),
    Call(String),
    Map(RefCell<Vec<(V, V)>>),
    Set(Vec<V>),
    Error(&'static str, Vec<u16>),
}
fn u() -> V {
    Rc::new(Native::U)
}
fn s(x: &str) -> V {
    Rc::new(Native::S(x.encode_utf16().collect()))
}
fn n(x: f64) -> V {
    Rc::new(Native::N(x))
}
fn b(x: bool) -> V {
    Rc::new(Native::B(x))
}
fn array(x: Vec<V>) -> V {
    Rc::new(Native::A(RefCell::new(x)))
}
fn object(x: Vec<(&str, V)>) -> V {
    Rc::new(Native::O(RefCell::new(
        x.into_iter().map(|(k, v)| (s(k), v)).collect(),
    )))
}
fn text(v: &V) -> String {
    match v.as_ref() {
        Native::U => "undefined".into(),
        Native::Null => "null".into(),
        Native::B(x) => x.to_string(),
        Native::N(x) => safe_cex_num::js_number_string(*x),
        Native::S(x) => String::from_utf16_lossy(x),
        Native::Error(name, msg) => format!("{name}: {}", String::from_utf16_lossy(msg)),
        _ => "[object Object]".into(),
    }
}
fn number(v: &V) -> f64 {
    match v.as_ref() {
        Native::N(x) => *x,
        Native::B(x) => {
            if *x {
                1.
            } else {
                0.
            }
        }
        Native::Null => 0.,
        Native::S(x) => text(&Rc::new(Native::S(x.clone())))
            .parse()
            .unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}
fn truthy(v: &V) -> bool {
    match v.as_ref() {
        Native::U | Native::Null => false,
        Native::B(x) => *x,
        Native::N(x) => *x != 0. && !x.is_nan(),
        Native::S(x) => !x.is_empty(),
        _ => true,
    }
}
fn equal(a: &V, b: &V) -> bool {
    match (a.as_ref(), b.as_ref()) {
        (Native::U, Native::U) | (Native::Null, Native::Null) => true,
        (Native::B(a), Native::B(b)) => a == b,
        (Native::N(a), Native::N(b)) => a == b,
        (Native::S(a), Native::S(b)) => a == b,
        _ => Rc::ptr_eq(a, b),
    }
}
fn raw(v: &V, key: &V) -> V {
    match v.as_ref() {
        Native::O(fields) => fields
            .borrow()
            .iter()
            .find(|(k, _)| equal(k, key))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(u),
        Native::A(a) => {
            let k = text(key);
            if k == "length" {
                n(a.borrow().len() as f64)
            } else {
                k.parse::<usize>()
                    .ok()
                    .and_then(|i| a.borrow().get(i).cloned())
                    .unwrap_or_else(u)
            }
        }
        Native::S(x) if text(key) == "length" => n(x.len() as f64),
        Native::Error(name, msg) => match text(key).as_str() {
            "name" => s(name),
            "message" => Rc::new(Native::S(msg.clone())),
            _ => u(),
        },
        _ => u(),
    }
}
fn get(v: &V, key: &str) -> V {
    raw(v, &s(key))
}
fn put(v: &V, key: V, value: V) {
    let Native::O(fields) = v.as_ref() else {
        panic!("native object Set")
    };
    let mut fields = fields.borrow_mut();
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| equal(k, &key)) {
        *v = value
    } else {
        fields.push((key, value));
    }
}
fn values(v: &V) -> Vec<V> {
    let Native::A(a) = v.as_ref() else {
        panic!("native array")
    };
    a.borrow().clone()
}
fn copy(v: &V) -> V {
    match v.as_ref() {
        Native::O(a) => Rc::new(Native::O(RefCell::new(
            a.borrow()
                .iter()
                .map(|(k, v)| (k.clone(), copy(v)))
                .collect(),
        ))),
        Native::A(a) => array(a.borrow().iter().map(copy).collect()),
        _ => v.clone(),
    }
}
fn decode(v: &Value) -> V {
    match v {
        Value::Null => Rc::new(Native::Null),
        Value::Bool(x) => b(*x),
        Value::Number(x) => n(x.as_f64().unwrap()),
        Value::String(x) => s(x),
        Value::Array(x) => array(x.iter().map(decode).collect()),
        Value::Object(x) => {
            if x.get("$undefined") == Some(&json!(true)) {
                return u();
            }
            if let Some(x) = x.get("$number").and_then(Value::as_str) {
                return n(match x {
                    "NaN" => f64::NAN,
                    "Infinity" => f64::INFINITY,
                    "-Infinity" => f64::NEG_INFINITY,
                    "-0" => -0.,
                    _ => panic!("number tag"),
                });
            }
            if let Some(x) = x.get("$units").and_then(Value::as_array) {
                return Rc::new(Native::S(
                    x.iter().map(|v| v.as_u64().unwrap() as u16).collect(),
                ));
            }
            Rc::new(Native::O(RefCell::new(
                x.iter().map(|(k, v)| (s(k), decode(v))).collect(),
            )))
        }
    }
}
struct Host {
    spec: Value,
    receiver: V,
    events: RefCell<Vec<V>>,
    timers: RefCell<Vec<(V, V, f64)>>,
    interceptors: RefCell<Vec<V>>,
    client: RefCell<Option<V>>,
    error: V,
    constructed_error: RefCell<Option<V>>,
    mapped: RefCell<Vec<V>>,
    input: RefCell<V>,
    parsed: RefCell<V>,
    reads: Cell<usize>,
}
impl Host {
    fn event(&self, name: &str, args: Vec<V>) {
        self.events
            .borrow_mut()
            .push(array(std::iter::once(s(name)).chain(args).collect()));
    }
    fn fail(&self, key: &str) -> Result<(), E> {
        if self.spec["fail"] == key {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn property(&self, v: &V, key: &str) -> Result<V, E> {
        self.reads.set(self.reads.get() + 1);
        if matches!(v.as_ref(), Native::U | Native::Null) {
            return Err(self.error.clone());
        }
        let found = get(v, key);
        if !matches!(found.as_ref(), Native::U) {
            return Ok(found);
        }
        let builtin = match v.as_ref() {
            Native::A(_) => matches!(key, "map" | "filter" | "forEach" | "find" | "sort" | "push"),
            Native::S(_) => matches!(key, "trim" | "toLowerCase" | "includes"),
            Native::Map(_) => matches!(key, "get" | "set" | "values"),
            _ => false,
        };
        Ok(if builtin {
            Rc::new(Native::Call(key.into()))
        } else {
            u()
        })
    }
    fn invoke(self: &Rc<Self>, f: V, receiver: V, args: Vec<V>) -> Result<V, E> {
        match f.as_ref() {
            Native::F(_, _, body) => body(args),
            Native::Call(name) => self.builtin(name, receiver, args),
            _ => Err(self.error.clone()),
        }
    }
    fn iterator(
        self: &Rc<Self>,
        value: V,
        visit: Rc<dyn Fn(V) -> Result<bool, E>>,
    ) -> Result<(), E> {
        let mut index = 0;
        loop {
            let next = match value.as_ref() {
                Native::A(a) => a.borrow().get(index).cloned(),
                Native::Set(a) => a.get(index).cloned(),
                _ => panic!("native iterable"),
            };
            let Some(next) = next else { return Ok(()) };
            index += 1;
            match visit(next) {
                Ok(true) => {}
                Ok(false) => return Ok(()),
                Err(error) => {
                    if self.spec["iteratorClose"] == true {
                        self.event("iterator-close", vec![]);
                    }
                    return Err(error);
                }
            }
        }
    }
    fn builtin(self: &Rc<Self>, name: &str, r: V, args: Vec<V>) -> Result<V, E> {
        let arg = |i| args.get(i).cloned().unwrap_or_else(u);
        match name {
            "String"=>Ok(match arg(0).as_ref(){Native::S(_)=>arg(0),_=>s(&text(&arg(0)))}),
            "Boolean"=>Ok(b(truthy(&arg(0)))),
            "parseFloat"=>Ok(n(safe_cex_num::parse_float(&text(&arg(0))))),
            "isFinite"=>Ok(b(matches!(arg(0).as_ref(),Native::N(x)if x.is_finite()))),
            "trim"=>{let Native::S(x)=r.as_ref()else{panic!("trim string")};Ok(s(safe_cex_num::js_trim(&String::from_utf16_lossy(x))))},
            "toLowerCase"=>Ok(s(&text(&r).to_lowercase())),
            "includes"=>{let Native::S(raw)=r.as_ref()else{panic!("includes string")};let needle_value=arg(0);let Native::S(needle)=needle_value.as_ref()else{panic!("includes needle")};Ok(b(needle.is_empty()||raw.windows(needle.len()).any(|x|x==needle.as_slice())))},
            "keys"|"Object.values"|"entries"=>{
                let object=arg(0);let Native::O(fields)=object.as_ref()else{return Ok(array(vec![]))};
                let fields=fields.borrow();let names=fields.iter().map(|(k,_)|text(k)).collect::<Vec<_>>();let keys=safe_cex_num::js_own_keys(names.iter().map(String::as_str));
                Ok(array(keys.into_iter().map(|key|{let value=get(&object,&key);if name=="keys"{s(&key)}else if name=="entries"{array(vec![s(&key),value])}else{value}}).collect()))
            },
            "map"|"filter"|"find"|"forEach"=>{
                let length=values(&r).len();let mut result=vec![];
                for i in 0..length{let value=raw(&r,&n(i as f64));let returned=self.invoke(arg(0),u(),vec![value.clone(),n(i as f64),r.clone()])?;
                    match name{"map"=>result.push(returned),"filter" if truthy(&returned)=>result.push(value),"find" if truthy(&returned)=>return Ok(value),_=>{}}}
                Ok(if name=="find"||name=="forEach"{u()}else{array(result)})
            },
            "push"=>{let Native::A(a)=r.as_ref()else{panic!("push")};a.borrow_mut().extend(args);Ok(n(a.borrow().len()as f64))},
            "sort"=>{let mut items=values(&r);let mut error=None;items.sort_by(|a,b|match self.invoke(arg(0),u(),vec![a.clone(),b.clone()]){Ok(v)=>number(&v).partial_cmp(&0.).unwrap_or(std::cmp::Ordering::Equal),Err(e)=>{error=Some(e);std::cmp::Ordering::Equal}});if let Some(e)=error{return Err(e)}let Native::A(a)=r.as_ref()else{unreachable!()};a.replace(items);Ok(r)},
            "flatten"=>{let mut out=vec![];for v in values(&arg(0)){if matches!(v.as_ref(),Native::A(_)){out.extend(values(&v));}else{out.push(v);}}Ok(array(out))},
            "from"=>Ok(array(match arg(0).as_ref(){Native::Set(x)=>x.clone(),_=>values(&arg(0))})),
            "get"=>{let Native::Map(map)=r.as_ref()else{panic!("Map get")};Ok(map.borrow().iter().find(|(k,_)|equal(k,&arg(0))).map(|(_,v)|v.clone()).unwrap_or_else(u))},
            "set"=>{let Native::Map(map)=r.as_ref()else{panic!("Map set")};let mut map=map.borrow_mut();if let Some((_,v))=map.iter_mut().find(|(k,_)|equal(k,&arg(0))){*v=arg(1)}else{map.push((arg(0),arg(1)));}Ok(r.clone())},
            "values"=>{let Native::Map(map)=r.as_ref()else{panic!("Map values")};Ok(array(map.borrow().iter().map(|(_,v)|v.clone()).collect()))},
            "add"=>Ok(n(safe_cex_num::add(number(&arg(0)),number(&arg(1))))),
            "toOrderBookLevel"=>{self.fail("mapper")?;let value=port::to_order_book_level(&ops(self.clone()),&arg(0))?;self.mapped.borrow_mut().push(value.clone());Ok(value)},
            "aggregateOrderBookSnapshotLevels"=>port::aggregate_snapshot_levels(ops(self.clone()),self.receiver.clone(),arg(0)),
            "parseLighterWsNumber"=>{for v in args {match v.as_ref(){Native::N(x)if x.is_finite()=>return Ok(v.clone()),Native::S(x)if !x.is_empty()=>{let value=safe_cex_num::parse_float(&text(&v));if value.is_finite(){return Ok(n(value))}},_=>{}}}Ok(u())},
            "parseLighterWsBookSidePrice"=>{let value=arg(0);if !truthy(&value){return Ok(u())}let args=match value.as_ref(){Native::A(x)if !x.borrow().is_empty()=>vec![x.borrow()[0].clone()],_=>vec![value.clone(),get(&value,"price"),get(&value,"p"),get(&value,"bestPrice"),get(&value,"best_price")]};self.builtin("parseLighterWsNumber",u(),args)},
            "extractLighterTickerMarketIdFromChannel"=>Ok(safe_cex_venue_lighter::ws_public_parsers::extract_lighter_ticker_market_id_from_channel(&text(&arg(0))).map(s).unwrap_or_else(u)),
            "normalizeLighterFundingRate"=>Ok(n(ws_public::normalize_lighter_funding_rate(number(&arg(0))))),
            "updateTicker"=>{let ticker=arg(0);let tickers=values(&get(&get(&self.receiver,"store"),"tickers"));self.event("ticker",vec![b(tickers.iter().any(|t|Rc::ptr_eq(t,&ticker))),copy(&arg(1))]);self.fail("ticker")?;if self.spec["mutatePatch"]==true{put(&arg(1),s("mark"),n(31.));}Ok(u())},
            "updatePosition"=>{assert!(values(&get(&get(&self.receiver,"store"),"positions")).iter().any(|p|Rc::ptr_eq(p,&arg(0))),"LIGHTER-NATIVE-POSITION-ARGUMENT-IDENTITY");self.event("position",vec![get(&arg(0),"side"),copy(&arg(1))]);self.fail("position")?;Ok(u())},
            "getLighterApiBaseUrl"=>{self.event("base-url",vec![arg(0)]);Ok(s(if truthy(&get(&arg(0),"testnet")){"https://test.example"}else{"https://prod.example"}))},
            "addRequestInterceptor"|"addResponseInterceptor"|"addErrorInterceptor"=>{assert!(self.client.borrow().as_ref().is_some_and(|c|Rc::ptr_eq(c,&r)),"NATIVE-CLIENT-REGISTRATION-RECEIVER");self.interceptors.borrow_mut().push(arg(0));self.event("register",vec![s(name)]);Ok(u())},
            "emitAndLogError"=>{self.event("emit",vec![arg(0)]);self.fail("emit")?;Ok(u())},
            "delete"=>{self.event("delete",vec![arg(0)]);let Native::O(fields)=r.as_ref()else{panic!("header delete")};fields.borrow_mut().retain(|(k,_)|!equal(k,&arg(0)));self.fail("delete")?;Ok(u())},
            "clearPendingSubscribeTimeouts"=>{self.event("clear",vec![]);put(&self.receiver,s("pendingSubscribeTimeouts"),array(vec![]));Ok(u())},
            "sendPayload"=>{self.event("send",vec![arg(0)]);self.fail("send")?;Ok(u())},
            "log"=>{self.event("log",args);self.fail("log")?;Ok(u())},
            "setTimeout"=>{let handle=object(vec![("tag",n(self.timers.borrow().len()as f64+1.))]);self.event("timer",vec![arg(1)]);self.timers.borrow_mut().push((handle.clone(),arg(0),number(&arg(1))));Ok(handle)},
            "trackLastReceived"=>{self.event("track",vec![]);self.fail("track")?;Ok(u())},
            "recordMessageHandlerStats"=>{self.event("stats",vec![arg(0)]);self.fail("stats")?;Ok(u())},
            "syncTickerRestFallbackToPriceHydration"=>{self.event("hydrate",vec![arg(0)]);Ok(u())},
            "jsonParse"=>{self.event("parse",vec![]);self.fail("parse")?;if self.spec["replaceHandlers"]==true{put(&self.receiver,s("messageHandlers"),object(vec![]));}Ok(self.parsed.borrow().clone())},
            "shouldDispatchLighterPublicMessage"=>Ok(b(safe_cex_venue_lighter::ws_public_parsers::should_dispatch_lighter_public_message(&text(&arg(0)),&text(&arg(1))))),
            "getCurrentTimeMs"=>{self.event("clock",vec![]);Ok(n(777.))},
            "updateOrderBook"=>{self.event("delta",vec![get(&arg(2),"price"),get(&arg(2),"amount")]);let rows=get(&arg(0),&text(&arg(1)));let Native::A(rows)=rows.as_ref()else{panic!("delta rows")};let mut rows=rows.borrow_mut();let at=rows.iter().position(|v|equal(&get(v,"price"),&get(&arg(2),"price")));if let Some(at)=at{if number(&get(&arg(2),"amount"))==0.{rows.remove(at);}else{put(&rows[at],s("amount"),get(&arg(2),"amount"));}}else if number(&get(&arg(2),"amount"))!=0.{rows.push(arg(2));rows.sort_by(|a,b|{let cmp=number(&get(a,"price")).total_cmp(&number(&get(b,"price")));if text(&arg(1))=="bids"{cmp.reverse()}else{cmp}});}Ok(n(0.))},
            _=>panic!("unsupported test builtin {name}"),
        }
    }
}
fn ops(host: Rc<Host>) -> Rc<IngressOps<V, E>> {
    let property = host.clone();
    let index = host.clone();
    let calls = host.clone();
    let constructors = host.clone();
    let global = host.clone();
    let iter = host.clone();
    let until = host.clone();
    let tdz = host.clone();
    Rc::new(IngressOps {
        undefined: Rc::new(u),
        boolean: Rc::new(b),
        number: Rc::new(n),
        literal: Rc::new(s),
        object: Rc::new(object),
        array: Rc::new(array),
        get: Rc::new(move |v, key| property.property(v, key)),
        index: Rc::new(move |v, key| index.property(v, &text(key))),
        key: Rc::new(|v| Ok(v.clone())),
        set: Rc::new(|v, key, x| {
            put(v, s(key), x);
            Ok(())
        }),
        set_index: Rc::new(|v, key, x| {
            put(v, key.clone(), x);
            Ok(())
        }),
        delete: Rc::new(|v, key| {
            let Native::O(fields) = v.as_ref() else {
                panic!("native Delete")
            };
            fields.borrow_mut().retain(|(k, _)| !equal(k, key));
            Ok(true)
        }),
        global: Rc::new(move |name| {
            Ok(match name {
                "Object" => object(vec![
                    ("keys", Rc::new(Native::Call("keys".into()))),
                    ("values", Rc::new(Native::Call("Object.values".into()))),
                    ("entries", Rc::new(Native::Call("entries".into()))),
                ]),
                "Number" => object(vec![("isFinite", Rc::new(Native::Call("isFinite".into())))]),
                "Array" => object(vec![("from", Rc::new(Native::Call("from".into())))]),
                "PositionSide" => object(vec![("Long", s("long"))]),
                "LogSeverity" => object(vec![("Warning", s("warning"))]),
                "virtualClock" => object(vec![(
                    "getCurrentTimeMs",
                    Rc::new(Native::Call("getCurrentTimeMs".into())),
                )]),
                "RECV_WINDOW" => n(20000.),
                "UNSUPPORTED_LIGHTER_REST_HEADERS" => {
                    array(vec![s("x-timestamp"), s("x-account-index")])
                }
                "LighterPublicWebsocket" => object(vec![
                    ("MAX_IMMEDIATE_SUBSCRIBE_MESSAGES", n(24.)),
                    ("SUBSCRIBE_STAGGER_DELAY_MS", n(15.)),
                ]),
                _ => {
                    let _ = &global;
                    Rc::new(Native::Call(name.into()))
                }
            })
        }),
        invoke: Rc::new(move |f, r, args| calls.invoke(f, r, args)),
        function: Rc::new(|name, length, body| Rc::new(Native::F(name.into(), length, body))),
        truthy: Rc::new(truthy),
        nullish: Rc::new(|v| matches!(v.as_ref(), Native::U | Native::Null)),
        equal: Rc::new(equal),
        text: Rc::new(|v| {
            Ok(match v.as_ref() {
                Native::S(x) => x.clone(),
                _ => text(v).encode_utf16().collect(),
            })
        }),
        utf16: Rc::new(|x| Rc::new(Native::S(x))),
        error_value: Rc::new(|error| error),
        throw: Rc::new(|value| value),
        spread: Rc::new(|v| Ok(values(v))),
        tuple_two: Rc::new(|v| {
            let x = values(v);
            Ok((
                x.first().cloned().unwrap_or_else(u),
                x.get(1).cloned().unwrap_or_else(u),
            ))
        }),
        construct: Rc::new(move |ctor, args| {
            let Native::Call(name) = ctor.as_ref() else {
                panic!("native constructor")
            };
            match name.as_str() {
                "Map" => Ok(Rc::new(Native::Map(RefCell::new(vec![])))),
                "Set" => {
                    let mut unique = vec![];
                    for v in values(&args[0]) {
                        if !unique.iter().any(|x| equal(x, &v)) {
                            unique.push(v);
                        }
                    }
                    Ok(Rc::new(Native::Set(unique)))
                }
                "LighterApiClient" => {
                    constructors.event("client-config", vec![args[0].clone()]);
                    constructors.fail("client")?;
                    let client = object(vec![("tag", s("client"))]);
                    for method in [
                        "addRequestInterceptor",
                        "addResponseInterceptor",
                        "addErrorInterceptor",
                    ] {
                        put(&client, s(method), Rc::new(Native::Call(method.into())));
                    }
                    constructors.client.replace(Some(client.clone()));
                    Ok(client)
                }
                "Error" => {
                    constructors.event("new-error", vec![args[0].clone()]);
                    let error = Rc::new(Native::Error(
                        "Error",
                        text(&args[0]).encode_utf16().collect(),
                    ));
                    constructors.constructed_error.replace(Some(error.clone()));
                    Ok(error)
                }
                _ => panic!("unknown constructor {name}"),
            }
        }),
        is_function: Rc::new(|v| matches!(v.as_ref(), Native::F(..) | Native::Call(_))),
        is_object: Rc::new(|v| {
            matches!(
                v.as_ref(),
                Native::O(_)
                    | Native::A(_)
                    | Native::Map(_)
                    | Native::Set(_)
                    | Native::Error(..)
                    | Native::Null
            )
        }),
        for_of: Rc::new(move |v, visit| {
            iter.iterator(
                v,
                Rc::new(move |v| {
                    visit(v)?;
                    Ok(true)
                }),
            )
        }),
        for_of_until: Rc::new(move |v, visit| until.iterator(v, visit)),
        less: Rc::new(|a, b| Ok(number(a) < number(b))),
        greater: Rc::new(|a, b| Ok(number(a) > number(b))),
        less_equal: Rc::new(|a, b| Ok(number(a) <= number(b))),
        subtract: Rc::new(|a, b| Ok(n(number(&a) - number(&b)))),
        multiply: Rc::new(|a, b| Ok(n(number(&a) * number(&b)))),
        add_primitive: Rc::new(|a, b| Ok(n(number(&a) + number(&b)))),
        tdz_error: Rc::new(move |name| {
            let _ = &tdz;
            Rc::new(Native::Error(
                "ReferenceError",
                format!("Cannot access '{name}' before initialization")
                    .encode_utf16()
                    .collect(),
            ))
        }),
    })
}
fn host(spec: &Value) -> Rc<Host> {
    let store = decode(
        &json!({"tickers":[{"symbol":"BTC","id":"7"},{"symbol":"ETH","id":"8"}],"positions":[{"symbol":"BTC","side":"long","contracts":2,"entryPrice":10},{"symbol":"BTC","side":"short","contracts":3,"entryPrice":20},{"symbol":"ETH","side":"long","contracts":5,"entryPrice":9}],"markets":[{"symbol":"BTC","isInverse":spec["inverse"]==true},{"symbol":"ETH","isInverse":false}]}),
    );
    let parent = object(vec![("store", store.clone()), ("name", s("lighter"))]);
    let receiver = object(vec![
        ("parent", parent.clone()),
        ("store", store.clone()),
        ("isDisposed", b(spec["disposed"] == true)),
        ("isConnected", b(spec["disconnected"] != true)),
        ("reconnecting", b(spec["reconnecting"] == true)),
        (
            "hasReceivedMessageSinceConnect",
            b(spec["received"] == true),
        ),
        ("connectedViaProxyRoute", b(spec["proxy"] == true)),
        ("failedConnections", n(9.)),
        ("onCloseCount", n(11.)),
        ("directRouteFailureCount", n(4.)),
        (
            "topics",
            decode(
                spec.get("topics")
                    .unwrap_or(&json!({"all":"market_stats/all"})),
            ),
        ),
        ("pendingSubscribeTimeouts", array(vec![])),
        ("messageHandlers", object(vec![])),
    ]);
    let h = Rc::new(Host {
        spec: spec.clone(),
        receiver,
        events: RefCell::new(vec![]),
        timers: RefCell::new(vec![]),
        interceptors: RefCell::new(vec![]),
        client: RefCell::new(None),
        error: Rc::new(Native::Error("Error", "boom".encode_utf16().collect())),
        constructed_error: RefCell::new(None),
        mapped: RefCell::new(vec![]),
        input: RefCell::new(u()),
        parsed: RefCell::new(decode(spec.get("parsed").unwrap_or(&Value::Null))),
        reads: Cell::new(0),
    });
    for name in [
        "trackLastReceived",
        "recordMessageHandlerStats",
        "syncTickerRestFallbackToPriceHydration",
        "clearPendingSubscribeTimeouts",
        "sendPayload",
        "toOrderBookLevel",
        "aggregateOrderBookSnapshotLevels",
    ] {
        put(&h.receiver, s(name), Rc::new(Native::Call(name.into())));
    }
    for name in ["updateTicker", "updatePosition"] {
        put(&store, s(name), Rc::new(Native::Call(name.into())));
    }
    for name in ["log", "emitAndLogError"] {
        put(&parent, s(name), Rc::new(Native::Call(name.into())));
    }
    let handlers = get(&h.receiver, "messageHandlers");
    for topic in ["ticker", "market_stats:all", "pong"] {
        let host = h.clone();
        let name = topic.to_owned();
        let f = Rc::new(Native::F(
            "".into(),
            1,
            Rc::new(move |args| {
                host.event(
                    "callback",
                    vec![s(&name), b(Rc::ptr_eq(&args[0], &host.parsed.borrow()))],
                );
                host.fail("callback")?;
                Ok(s("ignored"))
            }),
        ));
        put(&handlers, s(topic), array(vec![f]));
    }
    h
}
fn run(h: Rc<Host>) -> Result<V, E> {
    let spec = &h.spec;
    let lang = ops(h.clone());
    let input = decode(spec.get("input").unwrap_or(&json!({})));
    h.input.replace(input.clone());
    match spec["op"].as_str().unwrap() {
        "headers" => {
            if spec["deleteMethod"] == true {
                put(&input, s("delete"), Rc::new(Native::Call("delete".into())));
            }
            port::strip_unsupported_headers(lang, input.clone())?;
            h.event("headers", vec![copy(&input)]);
            Ok(u())
        }
        "client" => {
            let exchange = get(&h.receiver, "parent");
            let client = port::create_sdk_client(lang, input, exchange)?;
            assert!(
                Rc::ptr_eq(&client, h.client.borrow().as_ref().unwrap()),
                "LIGHTER-NATIVE-CLIENT-RETURN-IDENTITY"
            );
            let interceptors = h.interceptors.borrow().clone();
            assert_eq!(interceptors.len(), 3, "LIGHTER-NATIVE-INTERCEPTOR-COUNT");
            if let Some(config) = spec.get("request") {
                let config = decode(config);
                let returned = h.invoke(interceptors[0].clone(), u(), vec![config.clone()])?;
                assert!(
                    Rc::ptr_eq(&returned, &config),
                    "LIGHTER-NATIVE-REQUEST-RETURN-IDENTITY"
                );
                h.event("request", vec![copy(&returned)]);
            }
            if let Some(response) = spec.get("response") {
                let response = decode(response);
                let returned = h.invoke(interceptors[1].clone(), u(), vec![response.clone()])?;
                assert!(
                    Rc::ptr_eq(&returned, &response),
                    "LIGHTER-NATIVE-RESPONSE-RETURN-IDENTITY"
                );
                h.event("response", vec![copy(&returned)]);
            }
            if let Some(error) = spec.get("error") {
                let error = decode(error);
                let returned = h.invoke(interceptors[2].clone(), u(), vec![error.clone()])?;
                assert!(
                    Rc::ptr_eq(&returned, &error),
                    "LIGHTER-NATIVE-ERROR-RETURN-IDENTITY"
                );
                h.event("error-return", vec![copy(&returned)]);
            }
            Ok(u())
        }
        "level" => {
            let value = port::to_order_book_level(&lang, &input)?;
            h.event("level", vec![value]);
            Ok(u())
        }
        "aggregate" => {
            let mapped_levels = port::aggregate_snapshot_levels(lang, h.receiver.clone(), input)?;
            let first = h.mapped.borrow().iter().find(|v| truthy(v)).cloned();
            let actual = values(&mapped_levels).first().cloned();
            if let (Some(first), Some(actual)) = (first, actual) {
                assert!(
                    Rc::ptr_eq(&first, &actual),
                    "LIGHTER-NATIVE-AGGREGATE-FIRST-MAPPED-IDENTITY"
                );
            }
            h.event("levels", vec![copy(&mapped_levels)]);
            Ok(u())
        }
        "apply" => {
            let book = decode(
                &json!({"bids":[{"price":10,"amount":2,"total":0}],"asks":[{"price":20,"amount":3,"total":0}]}),
            );
            let old_bids = get(&book, "bids");
            port::apply_book_levels(
                lang,
                h.receiver.clone(),
                book.clone(),
                s(spec["side"].as_str().unwrap_or("bids")),
                input,
                s(spec["mode"].as_str().unwrap_or("snapshot")),
            )?;
            h.event(
                "book",
                vec![copy(&book), b(Rc::ptr_eq(&old_bids, &get(&book, "bids")))],
            );
            Ok(u())
        }
        "ticker" => port::ticker_event(lang, h.receiver.clone(), input),
        "stats" => port::market_stats_event(lang, h.receiver.clone(), input),
        "message" => port::on_message(
            lang,
            h.receiver.clone(),
            &object(vec![("data", s(spec["raw"].as_str().unwrap()))]),
        ),
        "subscribe" => {
            port::subscribe(lang, h.receiver.clone())?;
            let pending = get(&h.receiver, "pendingSubscribeTimeouts");
            h.event("pending", vec![n(values(&pending).len() as f64)]);
            if spec["fire"] == true {
                let timers = h.timers.borrow().clone();
                if spec["disposeBeforeFire"] == true {
                    put(&h.receiver, s("isDisposed"), b(true));
                }
                for (_, callback, _) in timers {
                    h.invoke(callback, u(), vec![])?;
                }
                h.event(
                    "pending-after",
                    vec![n(
                        values(&get(&h.receiver, "pendingSubscribeTimeouts")).len() as f64,
                    )],
                );
            }
            Ok(u())
        }
        _ => panic!("unknown fixture operation"),
    }
}
fn fixtures() -> Vec<Value> {
    let book = json!([{"price":"11","remaining_base_amount":"0.1"},{"price":"11","remainingBaseAmount":"0.2"},{"price":"11.0001","size":"4"},{"price":"9","quantity":"0"},{"price":"-1","size":"7"}]);
    let ticker = json!({"symbol":"BTC","lastPrice":"17","mark_price":"18","indexPrice":"19","bidPrice":"11","askPrice":"23","volume24h":"29","quoteVolume24h":"31","openInterest":"37","fundingRate":"0.0012","priceChange24h":"41"});
    let mut rows = vec![
        json!({"name":"headers-case-fold","op":"headers","input":{"X-Timestamp":"one","x-ACCOUNT-index":"two","Content-Type":"application/json","Other":"kept"}}),
        json!({"name":"headers-delete-method","op":"headers","input":{"x-timestamp":1,"x-account-index":2,"Keep":3},"deleteMethod":true}),
        json!({"name":"headers-null","op":"headers","input":null}),
        json!({"name":"headers-scalar","op":"headers","input":"x-timestamp"}),
        json!({"name":"headers-delete-throw","op":"headers","input":{"x-timestamp":1,"Keep":3},"deleteMethod":true,"fail":"delete"}),
        json!({"name":"client-default","op":"client","input":{},"request":{"headers":{"X-TIMESTAMP":7,"Content-Type":"application/json"}},"response":{"data":{"success":true}}}),
        json!({"name":"client-zero-timeout","op":"client","input":{"testnet":true,"extra":{"recvWindow":0}}}),
        json!({"name":"client-null-timeout","op":"client","input":{"extra":{"recvWindow":null}}}),
        json!({"name":"client-response-error","op":"client","input":{},"response":{"data":{"error":{"code":17,"message":"bad-response"},"success":false}}}),
        json!({"name":"client-response-false","op":"client","input":{},"response":{"data":{"success":false}}}),
        json!({"name":"client-error-suppressed","op":"client","input":{},"error":{"message":"ignored","config":{"__suppressExchangeError":true}}}),
        json!({"name":"client-sendtx-error","op":"client","input":{},"error":{"message":"retry","config":{"url":"/api/v1/sendTx?foo=bar"}}}),
        json!({"name":"client-visible-error","op":"client","input":{},"error":{"message":"visible","config":{"url":"/api/v1/tickers"}}}),
        json!({"name":"client-constructor-throw","op":"client","input":{},"fail":"client"}),
        json!({"name":"level-alias-precedence","op":"level","input":{"price":"11","0":"19","remaining_base_amount":"0","remainingBaseAmount":"3","size":"5","quantity":"7","1":"9"}}),
        json!({"name":"level-tuple","op":"level","input":["13.5","2.25"]}),
        json!({"name":"level-nan","op":"level","input":{"price":"bad","size":"3"}}),
        json!({"name":"level-zero-price","op":"level","input":{"price":"0","size":"3"}}),
        json!({"name":"level-negative-amount","op":"level","input":{"price":"11","size":"-3"}}),
        json!({"name":"level-nullish","op":"level","input":{"price":null,"0":"17","remaining_base_amount":null,"remainingBaseAmount":"5"}}),
        json!({"name":"aggregate-duplicates-near-prices","op":"aggregate","input":book}),
        json!({"name":"aggregate-empty","op":"aggregate","input":[]}),
        json!({"name":"aggregate-mapper-throw-close","op":"aggregate","input":[{"price":"11","size":"2"}],"fail":"mapper","iteratorClose":true}),
        json!({"name":"apply-bid-snapshot","op":"apply","input":book}),
        json!({"name":"apply-ask-snapshot","op":"apply","side":"asks","input":[["23","2"],["19","3"],["23","5"]]}),
        json!({"name":"apply-zero-removal-delta","op":"apply","mode":"delta","input":[{"price":"10","size":"0"},{"price":"9","size":"7"},{"price":"11","size":"3"}]}),
        json!({"name":"apply-empty-snapshot","op":"apply","input":[]}),
        json!({"name":"ticker-fields-pnl","op":"ticker","input":ticker}),
        json!({"name":"ticker-channel-index-fallback","op":"ticker","input":{"channel":"ticker/7","data":{"index":"25","bid":["13","2"],"ask":{"price":"29"}}}}),
        json!({"name":"ticker-inverse-return","op":"ticker","input":ticker,"inverse":true}),
        json!({"name":"ticker-live-patch-reference","op":"ticker","input":ticker,"mutatePatch":true}),
        json!({"name":"ticker-missing","op":"ticker","input":{"symbol":"NONE","last":"5"}}),
        json!({"name":"stats-fields","op":"stats","input":{"market_stats":{"btc":{"market_id":7,"mark_price":"11","indexPrice":"13","last":"17","daily_base_token_volume":"19","daily_quote_token_volume":"23","open_interest":"29","current_funding_rate":"0.0012","daily_price_change":"31","best_bid_price":"7","best_ask_price":"37"}}}}),
        json!({"name":"stats-no-mark-fallback","op":"stats","input":{"data":{"market_stats":{"btc":{"symbol":"BTC","index":"13"},"missing":{"market_id":99,"mark":"7"}}}}}),
        json!({"name":"stats-empty","op":"stats","input":{}}),
        json!({"name":"message-ticker","op":"message","raw":"{\"channel\":\"ticker:7\"}","parsed":{"channel":"ticker:7"}}),
        json!({"name":"message-ping-error","op":"message","raw":"{\"type\":\"ping\",\"channel\":\"market_stats/all\"}","parsed":{"error":{"code":0,"message":""}}}),
        json!({"name":"message-proxy-first","op":"message","raw":"{\"channel\":\"ticker/7\"}","parsed":{},"proxy":true}),
        json!({"name":"message-existing-streak","op":"message","raw":"{\"channel\":\"ticker/7\"}","parsed":{},"received":true}),
        json!({"name":"message-falsy-json","op":"message","raw":"{\"channel\":\"ticker/7\"}","parsed":false}),
        json!({"name":"message-captured-handlers","op":"message","raw":"{\"channel\":\"ticker/7\"}","parsed":{},"replaceHandlers":true}),
        json!({"name":"message-disposed","op":"message","raw":"ping","disposed":true}),
        json!({"name":"message-track-throw","op":"message","raw":"ping","fail":"track"}),
        json!({"name":"message-callback-throw","op":"message","raw":"{\"channel\":\"ticker/7\"}","parsed":{},"fail":"callback"}),
        json!({"name":"subscribe-normalize-dedup","op":"subscribe","topics":{"one":[" ticker:7 ","ticker:7"," ",0,false,null],"two":"market_stats/all"}}),
        json!({"name":"subscribe-disposed","op":"subscribe","disposed":true}),
        json!({"name":"subscribe-send-catch","op":"subscribe","fail":"send"}),
    ];
    let many = (0..26).map(|i| format!("ticker:{i}")).collect::<Vec<_>>();
    rows.push(json!({"name":"subscribe-stagger-live-removal","op":"subscribe","topics":{"all":many},"fire":true}));
    rows.push(json!({"name":"subscribe-stagger-disposed-before-fire","op":"subscribe","topics":{"all":many},"fire":true,"disposeBeforeFire":true}));
    rows
}
fn assert_native(v: &V, expected: &Value, label: &str) {
    match v.as_ref() {
        Native::U => assert_eq!(*expected, json!({"$undefined":true}), "{label} undefined"),
        Native::Null => assert!(expected.is_null(), "{label} null"),
        Native::B(x) => assert_eq!(Some(*x), expected.as_bool(), "{label} bool"),
        Native::N(x) => {
            if let Some(tag) = expected.get("$number").and_then(Value::as_str) {
                match tag {
                    "NaN" => assert!(x.is_nan(), "{label} NaN"),
                    "Infinity" => assert_eq!(*x, f64::INFINITY, "{label} infinity"),
                    "-0" => assert_eq!(x.to_bits(), (-0f64).to_bits(), "{label} signedzero"),
                    _ => panic!("number marker"),
                }
            } else {
                assert_eq!(
                    x.to_bits(),
                    expected.as_f64().unwrap().to_bits(),
                    "{label} native number"
                )
            }
        }
        Native::S(x) => assert_eq!(
            *x,
            expected["$units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_u64().unwrap() as u16)
                .collect::<Vec<_>>(),
            "{label} UTF16"
        ),
        Native::A(a) => {
            let a = a.borrow();
            let e = expected.as_array().unwrap();
            assert_eq!(a.len(), e.len(), "{label} array count");
            for (v, e) in a.iter().zip(e) {
                assert_native(v, e, label)
            }
        }
        Native::O(a) => {
            let a = a.borrow();
            let e = expected["$object"].as_array().unwrap();
            assert_eq!(a.len(), e.len(), "{label} own field count");
            for ((k, v), e) in a.iter().zip(e) {
                assert_native(k, &e[0], label);
                assert_native(v, &e[1], label)
            }
        }
        Native::F(name, length, _) => {
            assert_eq!(
                *expected,
                json!({"$callable":true}),
                "{label} callable {name} length{length}"
            )
        }
        Native::Call(name) => assert_eq!(
            *expected,
            json!({"$callable":true}),
            "{label} captured callable {name}"
        ),
        _ => panic!("unprojected native handle {label}"),
    }
}
fn native_state(h: &Host) -> V {
    object(vec![
        (
            "hasReceivedMessageSinceConnect",
            get(&h.receiver, "hasReceivedMessageSinceConnect"),
        ),
        ("failedConnections", get(&h.receiver, "failedConnections")),
        ("onCloseCount", get(&h.receiver, "onCloseCount")),
        (
            "directRouteFailureCount",
            get(&h.receiver, "directRouteFailureCount"),
        ),
    ])
}
fn independent(h: &Host, outcome: &Result<V, E>, index: usize) {
    let name = h.spec["name"].as_str().unwrap();
    let label = format!("LIGHTER-INGRESS-INDEPENDENT {name}");
    let original = matches!(
        name,
        "headers-delete-throw"
            | "client-constructor-throw"
            | "aggregate-mapper-throw-close"
            | "message-track-throw"
            | "message-callback-throw"
    );
    let constructed = matches!(name, "client-response-error" | "client-response-false");
    if original {
        assert!(
            outcome.as_ref().is_err_and(|e| Rc::ptr_eq(e, &h.error)),
            "{label} original error identity"
        );
    } else if constructed {
        let error = outcome.as_ref().err().expect("constructed error");
        assert!(
            h.constructed_error
                .borrow()
                .as_ref()
                .is_some_and(|created| Rc::ptr_eq(created, error)),
            "{label} LIGHTER-NATIVE-CONSTRUCTED-ERROR-IDENTITY"
        );
        let Native::Error(kind, msg) = error.as_ref() else {
            panic!("{label} native Error")
        };
        assert_eq!(*kind, "Error", "{label} Error name");
        assert_eq!(
            *msg,
            if name == "client-response-error" {
                "[17] bad-response"
            } else {
                "Request failed"
            }
            .encode_utf16()
            .collect::<Vec<_>>(),
            "{label} Error message"
        );
    } else {
        assert!(
            outcome
                .as_ref()
                .is_ok_and(|v| matches!(v.as_ref(), Native::U)),
            "{label} void return"
        );
    }
    const EFFECTS: [usize; 49] = [
        1, 3, 1, 1, 1, 7, 5, 5, 7, 7, 6, 6, 7, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 4, 1, 3, 3, 1,
        3, 0, 1, 1, 0, 5, 9, 5, 5, 4, 5, 0, 1, 4, 4, 2, 4, 31, 29,
    ];
    assert_eq!(
        h.events.borrow().len(),
        EFFECTS[index],
        "{label} literal effect count"
    );
    let events = h.events.borrow();
    let event = |kind: &str| {
        events
            .iter()
            .find(|e| text(&values(e)[0]) == kind)
            .map(values)
            .unwrap()
    };
    let numbers = |object: &V, fields: &[(&str, f64)]| {
        for (key, value) in fields {
            let actual = get(object, key);
            let Native::N(actual) = actual.as_ref() else {
                panic!("{label} native Number {key}")
            };
            assert_eq!(
                actual.to_bits(),
                value.to_bits(),
                "{label} numeric field {key}"
            );
        }
    };
    match name {
        "headers-case-fold" => {
            let headers = event("headers")[1].clone();
            assert!(
                matches!(get(&headers, "X-Timestamp").as_ref(), Native::U),
                "{label} removed timestamp"
            );
            assert!(
                matches!(get(&headers, "x-ACCOUNT-index").as_ref(), Native::U),
                "{label} removed account index"
            );
            assert_eq!(
                text(&get(&headers, "Other")),
                "kept",
                "{label} retained other header"
            );
        }
        "client-default" | "client-zero-timeout" | "client-null-timeout" => {
            let config = event("client-config")[1].clone();
            numbers(
                &config,
                &[(
                    "timeout",
                    if name == "client-zero-timeout" {
                        0.
                    } else {
                        20000.
                    },
                )],
            );
            assert!(!truthy(&get(&config, "debug")), "{label} debug false");
            assert_eq!(
                text(&get(&get(&config, "headers"), "Content-Type")),
                "application/json",
                "{label} header literal"
            );
        }
        "level-alias-precedence" => numbers(
            &event("level")[1],
            &[("price", 11.), ("amount", 0.), ("total", 0.)],
        ),
        "level-tuple" => numbers(
            &event("level")[1],
            &[("price", 13.5), ("amount", 2.25), ("total", 0.)],
        ),
        "level-nan" | "level-zero-price" | "level-negative-amount" => assert!(
            matches!(event("level")[1].as_ref(), Native::U),
            "{label} invalid level refused"
        ),
        "aggregate-duplicates-near-prices" => {
            let levels = values(&event("levels")[1]);
            assert_eq!(levels.len(), 2, "{label} independent price count");
            numbers(
                &levels[0],
                &[("price", 11.), ("amount", 0.3), ("total", 0.)],
            );
            numbers(&levels[1], &[("price", 11.0001), ("amount", 4.)]);
        }
        "apply-bid-snapshot" => {
            let book = event("book")[1].clone();
            let bids = values(&get(&book, "bids"));
            assert_eq!(bids.len(), 2, "{label} snapshot bid count");
            numbers(&bids[0], &[("price", 11.0001), ("amount", 4.)]);
            numbers(&bids[1], &[("price", 11.), ("amount", 0.3)]);
            assert!(!truthy(&event("book")[2]), "{label} new side array");
        }
        "apply-zero-removal-delta" => {
            let book = event("book")[1].clone();
            let bids = values(&get(&book, "bids"));
            assert_eq!(bids.len(), 2, "{label} delta count after removal");
            numbers(&bids[0], &[("price", 11.), ("amount", 3.)]);
            numbers(&bids[1], &[("price", 9.), ("amount", 7.)]);
            assert!(
                truthy(&event("book")[2]),
                "{label} retained delta side array"
            );
        }
        "ticker-fields-pnl" => {
            numbers(
                &event("ticker")[2],
                &[
                    ("last", 17.),
                    ("mark", 18.),
                    ("index", 19.),
                    ("bid", 11.),
                    ("ask", 23.),
                    ("volume", 29.),
                    ("quoteVolume", 31.),
                    ("openInterest", 37.),
                    ("fundingRate", 0.0001),
                    ("percentage", 41.),
                ],
            );
            let positions = events
                .iter()
                .filter(|e| text(&values(e)[0]) == "position")
                .map(values)
                .collect::<Vec<_>>();
            assert_eq!(positions.len(), 2, "{label} matching position count");
            numbers(&positions[0][2], &[("unrealizedPnl", 16.)]);
            numbers(&positions[1][2], &[("unrealizedPnl", 6.)]);
        }
        "ticker-channel-index-fallback" => numbers(
            &event("ticker")[2],
            &[("index", 25.), ("mark", 25.), ("bid", 13.), ("ask", 29.)],
        ),
        "ticker-live-patch-reference" => {
            let positions = events
                .iter()
                .filter(|e| text(&values(e)[0]) == "position")
                .map(values)
                .collect::<Vec<_>>();
            numbers(&positions[0][2], &[("unrealizedPnl", 42.)]);
            numbers(&positions[1][2], &[("unrealizedPnl", -33.)]);
        }
        "stats-fields" => numbers(
            &event("ticker")[2],
            &[
                ("mark", 11.),
                ("index", 13.),
                ("last", 17.),
                ("volume", 19.),
                ("quoteVolume", 23.),
                ("openInterest", 29.),
                ("fundingRate", 0.0001),
                ("percentage", 31.),
                ("bid", 7.),
                ("ask", 37.),
            ],
        ),
        "stats-no-mark-fallback" => {
            let patch = event("ticker")[2].clone();
            numbers(&patch, &[("index", 13.)]);
            assert!(
                matches!(get(&patch, "mark").as_ref(), Native::U),
                "{label} no stats mark fallback"
            );
        }
        "message-ticker"
        | "message-ping-error"
        | "message-proxy-first"
        | "message-falsy-json"
        | "message-captured-handlers"
        | "message-track-throw"
        | "message-callback-throw" => {
            assert!(
                truthy(&get(&h.receiver, "hasReceivedMessageSinceConnect")),
                "{label} first traffic flag"
            );
            numbers(
                &h.receiver,
                &[
                    ("failedConnections", 0.),
                    ("onCloseCount", 0.),
                    (
                        "directRouteFailureCount",
                        if name == "message-proxy-first" {
                            4.
                        } else {
                            0.
                        },
                    ),
                ],
            );
        }
        "subscribe-stagger-live-removal" | "subscribe-stagger-disposed-before-fire" => {
            let timers = h.timers.borrow();
            assert_eq!(timers.len(), 2, "{label} delayed count");
            assert_eq!(timers[0].2, 15., "{label} first delayed15");
            assert_eq!(timers[1].2, 30., "{label} second delayed30");
            assert_eq!(
                values(&get(&h.receiver, "pendingSubscribeTimeouts")).len(),
                0,
                "{label} removed original handles"
            );
        }
        _ => {}
    }
}
fn capture() -> &'static Value {
    static CAPTURE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    CAPTURE.get_or_init(|| {
        let specs = fixtures();
        assert_eq!(specs.len(), 49, "LIGHTER-INGRESS-INDEPENDENT-FIXTURE-FLOOR");
        let mut child = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/public_ingress_oracle.cjs"
            ))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(serde_json::to_string(&specs).unwrap().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "source oracle failed {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let data: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            data["rows"].as_array().unwrap().len(),
            49,
            "LIGHTER-INGRESS-INDEPENDENT-SOURCE-FLOOR"
        );
        assert_eq!(
            data["limits"],
            json!({"MAX_IMMEDIATE_SUBSCRIBE_MESSAGES":24,"SUBSCRIBE_STAGGER_DELAY_MS":15}),
            "LIGHTER-INGRESS-ACTUAL-CLASS-LIMITS"
        );
        assert_eq!(
            data["recvWindow"], 20000,
            "LIGHTER-INGRESS-ACTUAL-RECV-WINDOW"
        );
        data
    })
}
fn compare(op: &str, floor: usize) {
    let specs = fixtures();
    let natives = specs
        .iter()
        .enumerate()
        .filter(|(_, s)| s["op"] == op)
        .map(|(i, s)| {
            let h = host(s);
            let outcome = run(h.clone());
            independent(&h, &outcome, i);
            (i, s, h, outcome)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        natives.len(),
        floor,
        "LIGHTER-INGRESS-INDEPENDENT-GROUP-FLOOR {op}"
    );
    let source = capture();
    for (i, spec, h, outcome) in natives {
        let label = format!("LIGHTER-INGRESS-NATIVE {}", spec["name"].as_str().unwrap());
        let expected = &source["rows"][i];
        match expected["result"].as_str().unwrap() {
            "void" => assert!(
                outcome.is_ok_and(|v| matches!(v.as_ref(), Native::U)),
                "{label} void"
            ),
            "original" => assert!(
                Rc::ptr_eq(&outcome.err().unwrap(), &h.error),
                "{label} original error"
            ),
            "constructed" => {
                let error = outcome.err().unwrap();
                let Native::Error(name, msg) = error.as_ref() else {
                    panic!("{label} constructed Error")
                };
                assert_native(
                    &object(vec![
                        ("name", s(name)),
                        ("message", Rc::new(Native::S(msg.clone()))),
                    ]),
                    &expected["thrown"],
                    &label,
                );
            }
            _ => panic!("source outcome"),
        };
        assert_native(
            &array(h.events.borrow().clone()),
            &expected["events"],
            &label,
        );
        assert_native(&native_state(&h), &expected["state"], &label);
        assert_eq!(spec["name"], expected["name"], "{label} admitted identity");
    }
}
#[test]
fn headers() {
    compare("headers", 5);
}
#[test]
fn client() {
    compare("client", 9);
}
#[test]
fn level() {
    compare("level", 6);
}
#[test]
fn aggregate() {
    compare("aggregate", 3);
}
#[test]
fn apply() {
    compare("apply", 4);
}
#[test]
fn ticker() {
    compare("ticker", 5);
}
#[test]
fn stats() {
    compare("stats", 3);
}
#[test]
fn message() {
    compare("message", 9);
}
#[test]
fn subscribe() {
    compare("subscribe", 5);
}
