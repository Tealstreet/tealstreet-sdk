use num_bigint::BigInt;
use safe_cex_venue_lighter::{
    transaction_builders::TransactionBuildError,
    transaction_promise::{NativePromise, PromiseResolver, TransactionRuntime},
    transaction_signing::{AuthTokenEncoding, HostThrownValue, TransactionSigningError as E},
    ws_private_feed_helpers as f, ws_private_stream_handlers as t,
};
use serde_json::Value as J;
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    process::Command,
    rc::Rc,
};
#[derive(Clone, Debug)]
enum V {
    U,
    Null,
    Bool(bool),
    Num(f64),
    Big(BigInt),
    Text(Vec<u16>),
    Obj(Rc<Obj>),
    Array(Rc<RefCell<Vec<Option<V>>>>),
    Method(&'static str, Box<V>),
    Symbol(Rc<Vec<u16>>),
}
#[derive(Debug)]
struct Obj {
    fields: RefCell<Vec<(String, V)>>,
}
impl PartialEq for V {
    fn eq(&self, b: &Self) -> bool {
        match (self, b) {
            (Self::U, Self::U) | (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Num(a), Self::Num(b)) => a.to_bits() == b.to_bits(),
            (Self::Big(a), Self::Big(b)) => a == b,
            (Self::Text(a), Self::Text(b)) => a == b,
            (Self::Symbol(a), Self::Symbol(b)) => a == b,
            (Self::Obj(a), Self::Obj(b)) => *a.fields.borrow() == *b.fields.borrow(),
            (Self::Array(a), Self::Array(b)) => *a.borrow() == *b.borrow(),
            _ => false,
        }
    }
}
fn obj(v: Vec<(&str, V)>) -> V {
    V::Obj(Rc::new(Obj {
        fields: RefCell::new(v.into_iter().map(|(k, v)| (k.into(), v)).collect()),
    }))
}
fn array(v: Vec<V>) -> V {
    V::Array(Rc::new(RefCell::new(v.into_iter().map(Some).collect())))
}
fn text(v: &str) -> V {
    V::Text(v.encode_utf16().collect())
}
fn string(v: &V) -> String {
    match v {
        V::Text(v) => String::from_utf16(v).unwrap(),
        V::Big(v) => v.to_string(),
        V::Num(v) => safe_cex_num::js_number_string(*v),
        V::Symbol(v) => format!("Symbol({})", String::from_utf16(v).unwrap()),
        V::U => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(v) => v.to_string(),
        _ => panic!("fixture String conversion"),
    }
}
fn raw(v: &V, k: &str) -> V {
    match v {
        V::Obj(o) => o
            .fields
            .borrow()
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap_or(V::U),
        V::Array(v) if k == "length" => V::Num(v.borrow().len() as f64),
        V::Array(v) => k
            .parse::<usize>()
            .ok()
            .and_then(|i| v.borrow().get(i).cloned().flatten())
            .unwrap_or(V::U),
        _ => V::U,
    }
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Obj(a), V::Obj(b)) => Rc::ptr_eq(a, b),
        (V::Array(a), V::Array(b)) => Rc::ptr_eq(a, b),
        (V::Symbol(a), V::Symbol(b)) => Rc::ptr_eq(a, b),
        _ => a == b,
    }
}
fn truthy(v: &V) -> bool {
    match v {
        V::U | V::Null => false,
        V::Bool(v) => *v,
        V::Num(v) => *v != 0.0 && !v.is_nan(),
        V::Big(v) => *v != BigInt::from(0),
        V::Text(v) => !v.is_empty(),
        _ => true,
    }
}
fn decode(j: &J) -> V {
    match j["kind"].as_str().unwrap() {
        "undefined" => V::U,
        "null" => V::Null,
        "boolean" => V::Bool(j["value"].as_bool().unwrap()),
        "number" => V::Num(f64::from_bits(
            u64::from_str_radix(j["bits"].as_str().unwrap(), 16).unwrap(),
        )),
        "bigint" => V::Big(j["decimal"].as_str().unwrap().parse().unwrap()),
        "string" => V::Text(serde_json::from_value(j["units"].clone()).unwrap()),
        "symbol" => V::Symbol(Rc::new(serde_json::from_value(j["units"].clone()).unwrap())),
        "object" => V::Obj(Rc::new(Obj {
            fields: RefCell::new(
                j["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|f| (f[0].as_str().unwrap().into(), decode(&f[1])))
                    .collect(),
            ),
        })),
        "array" => V::Array(Rc::new(RefCell::new(
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
        ))),
        k => panic!("decode {k}"),
    }
}
fn json(v: &V) -> J {
    match v {
        V::U | V::Null => J::Null,
        V::Bool(v) => (*v).into(),
        V::Num(v) => serde_json::Number::from_f64(*v)
            .map(J::Number)
            .unwrap_or(J::Null),
        V::Text(v) => String::from_utf16(v).unwrap().into(),
        V::Big(v) => v.to_string().into(),
        V::Array(v) => J::Array(
            v.borrow()
                .iter()
                .map(|x| x.as_ref().map(json).unwrap_or(J::Null))
                .collect(),
        ),
        V::Obj(v) => J::Object(
            v.fields
                .borrow()
                .iter()
                .filter(|(_, v)| !matches!(v, V::U))
                .map(|(k, v)| (k.clone(), json(v)))
                .collect(),
        ),
        _ => panic!("fixture imported JSON helper"),
    }
}
fn error(name: &'static str, message: &str) -> E {
    TransactionBuildError {
        name,
        message: message.encode_utf16().collect(),
    }
    .into()
}
fn set(value: &V, key: &str, x: V) {
    let V::Obj(value) = value else {
        panic!("object")
    };
    let mut fields = value.fields.borrow_mut();
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == key) {
        *v = x
    } else {
        fields.push((key.into(), x))
    }
}
#[derive(Default)]
struct Engine {
    queue: RefCell<VecDeque<Box<dyn FnOnce()>>>,
}
impl TransactionRuntime for Engine {
    fn enqueue_microtask(&self, callback: Box<dyn FnOnce()>) {
        self.queue.borrow_mut().push_back(callback)
    }
    fn now_millis(&self) -> f64 {
        panic!("clock capability")
    }
    fn auth_token_encoding(&self) -> AuthTokenEncoding {
        panic!("no signing")
    }
}
fn drain(e: &Engine) {
    for _ in 0..100 {
        let callback = e.queue.borrow_mut().pop_front();
        match callback {
            Some(callback) => callback(),
            None => return,
        }
    }
    panic!("stream::microtask floor")
}
#[derive(Debug)]
struct Injected {
    stage: String,
}
struct State {
    row: J,
    engine: Rc<Engine>,
    owner: V,
    parent: V,
    alternate: V,
    store: V,
    emitter: V,
    input: V,
    mapped_order: V,
    mapped_orders: V,
    mapped_position: V,
    close_pairs: V,
    calls: Vec<V>,
    events: Vec<String>,
    identities: Vec<V>,
    errors: BTreeMap<String, HostThrownValue>,
    token_release: Option<PromiseResolver<V>>,
}
type H = Rc<RefCell<State>>;
type R = t::LighterPrivateStreamRuntime<H, V>;
fn injected(h: &H, stage: &str) -> E {
    let e = HostThrownValue::new(Injected {
        stage: stage.into(),
    });
    h.borrow_mut().errors.insert(stage.into(), e.clone());
    e.into()
}
fn message(e: &E) -> V {
    match e {
        E::Generated(e) => V::Text(e.message.clone()),
        E::Thrown(e) => text(&format!(
            "injected:{}",
            e.value().downcast_ref::<Injected>().unwrap().stage
        )),
    }
}
fn receipt(h: &H, name: &str, args: Vec<V>) {
    let mut state = h.borrow_mut();
    state.events.push(name.into());
    state
        .calls
        .push(obj(vec![("name", text(name)), ("args", array(args))]));
}
fn stringify(value: &V) -> Result<Option<Vec<u16>>, E> {
    let ascii = |s: &str| Some(s.encode_utf16().collect::<Vec<_>>());
    match value {
        V::U => Ok(None),
        V::Null => Ok(ascii("null")),
        V::Bool(v) => Ok(ascii(if *v { "true" } else { "false" })),
        V::Num(n) => Ok(ascii(&if n.is_finite() {
            safe_cex_num::js_number_string(*n)
        } else {
            "null".into()
        })),
        V::Big(_) => Err(error("TypeError", "Do not know how to serialize a BigInt")),
        V::Text(units) => {
            let mut out = vec![34];
            for c in char::decode_utf16(units.iter().copied()) {
                match c {
                    Err(e) => {
                        out.extend(format!("\\u{:04x}", e.unpaired_surrogate()).encode_utf16())
                    }
                    Ok(c) => {
                        let quoted = serde_json::to_string(&c.to_string()).unwrap();
                        out.extend(quoted[1..quoted.len() - 1].encode_utf16());
                    }
                }
            }
            out.push(34);
            Ok(Some(out))
        }
        V::Array(values) => {
            let values = values.borrow().clone();
            let mut out = vec![91];
            for (i, v) in values.iter().enumerate() {
                if i > 0 {
                    out.push(44)
                }
                out.extend(match v {
                    Some(v) => stringify(v)?.unwrap_or_else(|| "null".encode_utf16().collect()),
                    None => "null".encode_utf16().collect(),
                });
            }
            out.push(93);
            Ok(Some(out))
        }
        V::Obj(value) => {
            let fields = value.fields.borrow().clone();
            let keys = safe_cex_num::js_own_keys(fields.iter().map(|(k, _)| k.as_str()));
            let mut out = vec![123];
            let mut added = false;
            for key in keys {
                let v = &fields.iter().find(|(k, _)| *k == key).unwrap().1;
                if let Some(v) = stringify(v)? {
                    if added {
                        out.push(44)
                    }
                    added = true;
                    out.extend(stringify(&text(&key))?.unwrap());
                    out.push(58);
                    out.extend(v)
                }
            }
            out.push(125);
            Ok(Some(out))
        }
        _ => panic!("data stringify"),
    }
}
fn to_string(value: V) -> Vec<u16> {
    match value {
        V::Text(v) => v,
        V::Obj(_) | V::Array(_) => safe_cex_num::js_string(Some(&json(&value)))
            .unwrap()
            .encode_utf16()
            .collect(),
        _ => string(&value).encode_utf16().collect(),
    }
}
fn upper_trim(units: Vec<u16>) -> Vec<u16> {
    let whitespace = |c: &u16| {
        char::from_u32(*c as u32).is_some_and(|c| safe_cex_num::js_trim(&c.to_string()).is_empty())
    };
    let start = units
        .iter()
        .position(|c| !whitespace(c))
        .unwrap_or(units.len());
    let end = units
        .iter()
        .rposition(|c| !whitespace(c))
        .map(|i| i + 1)
        .unwrap_or(start);
    let mut out = vec![];
    for c in char::decode_utf16(units[start..end].iter().copied()) {
        match c {
            Err(e) => out.push(e.unpaired_surrogate()),
            Ok(c) => {
                for c in c.to_uppercase() {
                    out.extend(c.encode_utf16(&mut [0; 2]).iter().copied())
                }
            }
        }
    }
    out
}
fn implicit_string(value: V) -> Result<Vec<u16>, E> {
    if matches!(value, V::Symbol(_)) {
        return Err(error(
            "TypeError",
            "Cannot convert a Symbol value to a string",
        ));
    }
    Ok(to_string(value))
}
fn numeric(value: V) -> Result<f64, E> {
    use safe_cex_num::JsPrimitive;
    let p = match value {
        V::U => JsPrimitive::Undefined,
        V::Null => JsPrimitive::Null,
        V::Bool(v) => JsPrimitive::Boolean(v),
        V::Num(v) => JsPrimitive::Number(v),
        V::Big(v) => JsPrimitive::BigInt(v),
        V::Text(v) => JsPrimitive::String(String::from_utf16_lossy(&v)),
        V::Symbol(_) => {
            return Err(error(
                "TypeError",
                "Cannot convert a Symbol value to a number",
            ));
        }
        _ => panic!("numeric fixture scope"),
    };
    safe_cex_num::js_number_primitive(&p).map_err(|e| error("TypeError", &e.message))
}
fn values(value: &V) -> Vec<V> {
    let V::Array(values) = value else {
        panic!("ordinary dense array scope")
    };
    values.borrow().iter().flatten().cloned().collect()
}
fn elements(method: V, expected: &str) -> Vec<V> {
    let V::Method(name, receiver) = method else {
        panic!("array method")
    };
    assert_eq!(name, expected);
    values(&receiver)
}
fn receiver(h: &H, name: &str, actual: &V) {
    let s = h.borrow();
    let expected = match name {
        "getPrivateWsAccountIndex"
        | "getPrivateWsAuthToken"
        | "mapOrder"
        | "mapPosition"
        | "fetchAndSetPrivateSnapshot"
        | "log" => &s.parent,
        "removeOrders" | "addOrUpdateOrders" | "updatePositions" => &s.store,
        "emit" => &s.emitter,
        "has" => {
            assert!(
                same(actual, &raw(&s.parent, "recentlyCancelledOrders")),
                "stream::native cancelled-set receiver"
            );
            return;
        }
        _ => &s.owner,
    };
    assert!(same(actual, expected), "stream::native receiver {name}");
}
fn call(h: &H, method: V, args: Vec<V>) -> Result<V, E> {
    let V::Method(name, owner) = method else {
        panic!("callee")
    };
    if name == "push" {
        let V::Array(items) = *owner else {
            panic!("push receiver")
        };
        items.borrow_mut().extend(args.into_iter().map(Some));
        return Ok(V::Num(items.borrow().len() as f64));
    }
    receiver(h, name, &owner);
    if matches!(
        name,
        "mapOrder" | "mapPosition" | "resolveExplicitlyClosedPositionUpdates"
    ) {
        assert!(
            values(&h.borrow().input).iter().any(|v| same(v, &args[0])),
            "stream::native original raw argument {name}"
        );
    }
    if h.borrow().row["method"] == "handleAccountTopic"
        && matches!(
            name,
            "handleOrderTopic" | "handlePositionTopic" | "handleFillsTopic"
        )
    {
        let key = match name {
            "handleOrderTopic" => "orders",
            "handlePositionTopic" => "positions",
            _ => "fills",
        };
        assert!(
            same(&args[0], &raw(&raw(&h.borrow().input, "data"), key)),
            "stream::native original dispatch argument {name}"
        );
    }
    if name == "normalizeOrderStatus" {
        return f::normalize_order_status(&feed(h.clone()), &args[0]);
    }
    if name == "isUserCancelledReason" {
        return f::is_user_cancelled_reason(&feed(h.clone()), args[0].clone()).map(V::Bool);
    }
    receipt(h, name, args.clone());
    if h.borrow().row["throwAt"] == name {
        return Err(injected(h, name));
    }
    match name {
        "isWsRateLimited" => Ok(V::Bool(h.borrow().row["rate"] == true)),
        "shouldUseRestOrderReconciliation" => Ok(V::Bool(true)),
        "isRecentStreamEvent" => Ok(V::Bool(h.borrow().row["recent"] != false)),
        "getPrivateWsAccountIndex" => Ok(h
            .borrow()
            .row
            .get("index")
            .map(|x| match x["kind"].as_str() {
                Some("symbol") => V::Symbol(Rc::new(
                    x["value"].as_str().unwrap().encode_utf16().collect(),
                )),
                _ => panic!("index"),
            })
            .unwrap_or(V::Num(7.0))),
        "has" => {
            let s = h.borrow();
            let target = s.row["cancelledId"].as_str().map(text);
            Ok(V::Bool(target.is_some_and(|v| v == args[0])))
        }
        "mapOrder" => Ok(h.borrow().mapped_orders.clone()),
        "mapPosition" => {
            if truthy(&raw(&args[0], "fail")) {
                Err(injected(h, "mapPosition"))
            } else if truthy(&raw(&args[0], "closed")) {
                Ok(V::U)
            } else {
                Ok(h.borrow().mapped_position.clone())
            }
        }
        "resolveExplicitlyClosedPositionUpdates" => Ok(h.borrow().close_pairs.clone()),
        "flattenByMarketPayload" => {
            let V::Obj(object) = &args[0] else {
                panic!("by market")
            };
            let fields = object.fields.borrow().clone();
            let keys = safe_cex_num::js_own_keys(fields.iter().map(|(k, _)| k.as_str()));
            let mut out = vec![];
            for key in keys {
                let v = &fields.iter().find(|(k, _)| *k == key).unwrap().1;
                match v {
                    V::Array(_) => out.extend(values(v)),
                    V::Obj(_) if args.get(1).is_some_and(truthy) => out.push(v.clone()),
                    _ => {}
                }
            }
            Ok(array(out))
        }
        "addOrUpdateOrders" => {
            let s = h.borrow();
            let identity = obj(vec![
                ("name", text(name)),
                ("originalArray", V::Bool(same(&args[0], &s.mapped_orders))),
                (
                    "originalElement",
                    V::Bool(same(&raw(&args[0], "0"), &s.mapped_order)),
                ),
            ]);
            drop(s);
            h.borrow_mut().identities.push(identity);
            Ok(V::U)
        }
        "updatePositions" => {
            let s = h.borrow();
            let pairs = values(&args[0])
                .into_iter()
                .map(|p| {
                    let old = raw(&p, "0");
                    let new = raw(&p, "1");
                    let old_label = if same(&old, &s.mapped_position) {
                        "mapped"
                    } else if same(&old, &raw(&raw(&s.close_pairs, "0"), "0")) {
                        "closed"
                    } else {
                        "other"
                    };
                    let new_label = if same(&new, &s.mapped_position) {
                        "mapped"
                    } else if same(&new, &raw(&raw(&s.close_pairs, "0"), "1")) {
                        "closed"
                    } else {
                        "other"
                    };
                    obj(vec![
                        ("old", text(old_label)),
                        ("same", V::Bool(same(&old, &new))),
                        ("new", text(new_label)),
                    ])
                })
                .collect();
            drop(s);
            h.borrow_mut()
                .identities
                .push(obj(vec![("name", text(name)), ("pairs", array(pairs))]));
            Ok(V::U)
        }
        _ => Ok(V::U),
    }
}
fn feed(h: H) -> Rc<f::LighterPrivateFeedRuntime<H, V>> {
    Rc::new(f::LighterPrivateFeedRuntime {
        promise_runtime: h.borrow().engine.clone(),
        host: h.clone(),
        get: |_, v, k| {
            if matches!(v, V::U | V::Null) {
                Err(error("TypeError", "Cannot read properties of undefined"))
            } else {
                Ok(raw(v, &String::from_utf16(k).unwrap()))
            }
        },
        define: |_, v, k, x| {
            set(v, &String::from_utf16(k).unwrap(), x);
            Ok(())
        },
        method: |_, v, k| Ok(V::Method(k, Box::new(v))),
        call,
        call_async: |h, method, args| {
            let V::Method(name, owner) = method else {
                panic!("async callee")
            };
            assert_eq!(name, "getPrivateWsAuthToken");
            receiver(h, name, &owner);
            receipt(h, name, args);
            if h.borrow().row["throwToken"] == true {
                return Err(injected(h, "token"));
            }
            let (p, out) = NativePromise::pending(h.borrow().engine.clone());
            h.borrow_mut().token_release = Some(out);
            Ok(p)
        },
        clear_timeout: |_, _| panic!("unused timer boundary"),
        set_timeout: |_, _, _| panic!("unused timer boundary"),
        schedule_call: |_, _, _, _, _, _| panic!("unused scheduler boundary"),
        console_error: |_, _, _| panic!("unused console boundary"),
        stringify: |_, v| Ok(stringify(&v)?.map(V::Text).unwrap_or(V::U)),
        to_string: |_, v| Ok(to_string(v)),
        trim_upper: |_, v| Ok(upper_trim(v)),
        to_number: |_, v| numeric(v),
        parse_float: |_, v| Ok(safe_cex_num::parse_float(&String::from_utf16_lossy(&v))),
        copy: |_, v| {
            let V::Obj(v) = v else { panic!("copy") };
            Ok(V::Obj(Rc::new(Obj {
                fields: RefCell::new(v.fields.borrow().clone()),
            })))
        },
        map: |_, method, callback| {
            let mut out = vec![];
            for v in elements(method, "map") {
                out.push(callback(v)?)
            }
            Ok(array(out))
        },
        filter: |_, _, _| panic!("unused filter boundary"),
        now: |h| {
            receipt(h, "now", vec![]);
            if h.borrow().row["clockSwap"] == true {
                let s = h.borrow();
                set(&s.owner, "parent", s.alternate.clone())
            }
            Ok(25000.0)
        },
        number: V::Num,
        text,
        utf16: V::Text,
        boolean: V::Bool,
        undefined: || V::U,
        object: obj,
        array,
        truthy,
        nullish: |v| matches!(v, V::U | V::Null),
        is_undefined: |v| matches!(v, V::U),
        strict_same: |a, b| match (a, b) {
            (V::Num(a), V::Num(b)) => a == b,
            _ => same(a, b),
        },
    })
}
fn runtime(h: H) -> Rc<R> {
    Rc::new(R {
        feed: feed(h),
        some: |_, method, callback| {
            for v in elements(method, "some") {
                if truthy(&callback(v)?) {
                    return Ok(V::Bool(true));
                }
            }
            Ok(V::Bool(false))
        },
        for_each: |_, method, callback| {
            for v in elements(method, "forEach") {
                callback(v)?;
            }
            Ok(V::U)
        },
        for_of: |_, data, callback| {
            for v in values(&data) {
                callback(v)?;
            }
            Ok(())
        },
        spread: |_, v| Ok(values(&v)),
        key_value: |_, v, key| {
            let exact = raw(v, key);
            if !matches!(exact, V::U) {
                return Ok(exact);
            }
            let snake = match key {
                "initialBaseAmount" => "initial_base_amount",
                "updatedAt" => "updated_at",
                other => other,
            };
            Ok(raw(v, snake))
        },
        number_value: |v| if let V::Num(n) = v { Some(*n) } else { None },
        date_now: |h| {
            receipt(h, "date", vec![]);
            Ok(V::Num(30000.0))
        },
        performance_now: |h| {
            receipt(h, "performance", vec![]);
            Ok(V::Num(123.5))
        },
        constant: |h, k| {
            Ok(match k {
                "OrderStatus.Open" => text("open"),
                "OrderStatus.Canceled" => text("canceled"),
                "OrderStatus.Closed" => text("closed"),
                "RATE_LIMIT_PING_INTERVAL_MS" => V::Num(20000.0),
                "RATE_LIMIT_PING_SKIP_LOG_COOLDOWN_MS" => V::Num(15000.0),
                "ENABLE_RECONNECTING_WEBSOCKET" => V::Bool(h.borrow().row["reconnect"] == true),
                _ => panic!("constant"),
            })
        },
        error_value: |h, e| {
            obj(vec![(
                "message",
                if h.borrow().row["errorMessageSymbol"] == true {
                    V::Symbol(Rc::new("s".encode_utf16().collect()))
                } else {
                    message(&e)
                },
            )])
        },
        template_string: |_, v| implicit_string(v),
        parse_float_value: |_, v| {
            Ok(safe_cex_num::parse_float(&String::from_utf16_lossy(
                &implicit_string(v)?,
            )))
        },
        subtract: |_, a, b| {
            if matches!(a, V::Big(_)) != matches!(b, V::Big(_)) {
                Err(error(
                    "TypeError",
                    "Cannot mix BigInt and other types, use explicit conversions",
                ))
            } else {
                Ok(V::Num(numeric(a)? - numeric(b)?))
            }
        },
        less_than: |_, a, b| Ok(numeric(a)? < numeric(b)?),
        less_equal: |_, a, b| Ok(numeric(a)? <= numeric(b)?),
        greater_equal: |_, a, b| Ok(numeric(a)? >= numeric(b)?),
    })
}
fn source() -> &'static J {
    static DATA: std::sync::OnceLock<J> = std::sync::OnceLock::new();
    DATA.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/private-stream-handlers-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "stream::oracle {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let data: J = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(data["count"], 75, "stream::independent total floor");
        data
    })
}
fn host(row: &J) -> H {
    let mapped_order = decode(&row["mappedOrder"]);
    let mapped_orders = if row["mapEmpty"] == true {
        array(vec![])
    } else {
        array(vec![mapped_order.clone()])
    };
    let mapped_position = decode(&row["mappedPosition"]);
    let close_pairs = decode(&row["closePairs"]);
    let store = obj(vec![]);
    let emitter = obj(vec![]);
    let parent = obj(vec![
        ("store", store.clone()),
        ("lastWebsocketPositionsUpdate", V::Num(17.0)),
        ("recentlyCancelledOrders", obj(vec![])),
    ]);
    let alternate = obj(vec![
        ("store", store.clone()),
        ("lastWebsocketPositionsUpdate", V::Num(29.0)),
    ]);
    let owner = obj(vec![
        ("store", store.clone()),
        ("parent", parent.clone()),
        ("emitter", emitter.clone()),
        ("useReadonlyFallback", V::Bool(row["readonly"] == true)),
        (
            "accountAllOrdersChannelEnabled",
            V::Bool(row["enabled"] != false),
        ),
    ]);
    let initial = decode(&row["initialState"]);
    let V::Obj(initial) = initial else {
        panic!("initial")
    };
    for (key, value) in initial.fields.borrow().iter() {
        if key != "parentUpdate" && key != "alternateUpdate" {
            set(&owner, key, value.clone())
        }
    }
    Rc::new(RefCell::new(State {
        row: row.clone(),
        engine: Default::default(),
        owner,
        parent,
        alternate,
        store,
        emitter,
        input: decode(&row["value"]),
        mapped_order,
        mapped_orders,
        mapped_position,
        close_pairs,
        calls: vec![],
        events: vec![],
        identities: vec![],
        errors: Default::default(),
        token_release: None,
    }))
}
fn state(h: &H) -> V {
    let s = h.borrow();
    let mut fields = vec![];
    for key in [
        "isDisposed",
        "openedSinceLastConnect",
        "lastStreamRecoveryAttemptAt",
        "hasPendingAccountAllOrdersSubscribe",
        "hasReceivedAccountAllOrdersAck",
        "lastConnectTime",
        "lastRateLimitedPingAt",
        "lastRateLimitedPingSkipLogAt",
        "lastIdlePingAt",
        "hasWarnedReadonlyFallback",
        "pingAt",
    ] {
        fields.push((key, raw(&s.owner, key)))
    }
    fields.push((
        "parentUpdate",
        raw(&s.parent, "lastWebsocketPositionsUpdate"),
    ));
    fields.push((
        "alternateUpdate",
        raw(&s.alternate, "lastWebsocketPositionsUpdate"),
    ));
    obj(fields)
}
fn outcome(expected: &J, actual: Result<V, E>, h: &H, label: &str) {
    assert_eq!(
        actual.is_err(),
        expected["kind"] == "error",
        "stream::native outcome {label}"
    );
    match actual {
        Ok(value) => assert_eq!(
            value,
            decode(&expected["value"]),
            "stream::native result {label}"
        ),
        Err(e) => {
            assert_eq!(
                message(&e),
                decode(&expected["message"]),
                "stream::native error {label}"
            );
            let name = match &e {
                E::Generated(e) => e.name,
                _ => "Error",
            };
            assert_eq!(
                name,
                expected["name"].as_str().unwrap(),
                "stream::native error name {label}"
            );
            if let Some(stage) = expected["original"].as_str() {
                let E::Thrown(e) = e else {
                    panic!("original error")
                };
                assert!(
                    e.ptr_eq(&h.borrow().errors[stage]),
                    "stream::native original error {label}"
                )
            }
        }
    }
}
fn run(row: &J, detached: bool) {
    let h = host(row);
    let r = runtime(h.clone());
    let owner = h.borrow().owner.clone();
    let input = h.borrow().input.clone();
    let id = row["id"].as_str().unwrap();
    let actual = match row["method"].as_str().unwrap() {
        "handleAccountTopic" => t::handle_account_topic(&r, &owner, &input),
        "handleAccountAllTopic" => t::handle_account_all_topic(&r, &owner, &input),
        "handleOrderTopic" => t::handle_order_topic(&r, &owner, input),
        "handlePositionTopic" => t::handle_position_topic(&r, &owner, input),
        "handleFillsTopic" => t::handle_fills_topic(&r, &owner, input),
        "ping" => t::ping(&r, &owner),
        "onOpen" => t::on_open(&r, &owner),
        "authenticateAndSubscribe" => {
            let p = t::authenticate_and_subscribe(&r, &owner);
            assert_eq!(
                state(&h),
                decode(&row["beforeAwait"]["state"]),
                "stream::native before-await state {id}"
            );
            assert_eq!(
                array(h.borrow().calls.clone()),
                decode(&row["beforeAwait"]["calls"]),
                "stream::native before-await calls {id}"
            );
            assert_eq!(
                h.borrow().token_release.is_some(),
                row["beforeAwait"]["tokenPending"].as_bool().unwrap(),
                "stream::native token suspension {id}"
            );
            let observed = Rc::new(RefCell::new(None));
            if !detached {
                let capture = observed.clone();
                p.subscribe(move |v| *capture.borrow_mut() = Some(v));
            }
            drop(p);
            if row["disposeAfter"] == true {
                set(&owner, "isDisposed", V::Bool(true))
            }
            let release = h.borrow_mut().token_release.take();
            if let Some(out) = release {
                let value = if row["rejectToken"] == true {
                    Err(injected(&h, "token"))
                } else if let Some(token) = row.get("token") {
                    let j = source_input(token);
                    Ok(decode(&j))
                } else {
                    Ok(text("auth-token"))
                };
                out.settle(value);
            }
            let engine = h.borrow().engine.clone();
            drain(&engine);
            if detached {
                Ok(V::U)
            } else {
                let value = observed.borrow_mut().take();
                assert!(value.is_some(), "stream::native auth settled {id}");
                value.unwrap().map(|v| (*v).clone())
            }
        }
        _ => panic!("method"),
    };
    outcome(&row["result"], actual, &h, id);
    assert_eq!(
        state(&h),
        decode(&row["state"]),
        "stream::native state {id}"
    );
    assert_eq!(
        array(h.borrow().identities.clone()),
        decode(&row["identities"]),
        "stream::native retained identities {id}"
    );
    let calls = decode(&row["calls"]);
    let expected = values(&calls);
    let wire = |v: &V| raw(v, "name") == text("sendWithLog");
    assert_eq!(
        array(
            h.borrow()
                .calls
                .iter()
                .filter(|v| !wire(v))
                .cloned()
                .collect()
        ),
        array(expected.iter().filter(|v| !wire(v)).cloned().collect()),
        "stream::native calls {id}"
    );
    assert_eq!(
        array(
            h.borrow()
                .calls
                .iter()
                .filter(|v| wire(v))
                .cloned()
                .collect()
        ),
        array(expected.iter().filter(|v| wire(v)).cloned().collect()),
        "stream::wire calls {id}"
    );
    let trace: Vec<String> = serde_json::from_value(row["events"].clone()).unwrap();
    assert_eq!(h.borrow().events, trace, "stream::trace {id}");
}
fn source_input(value: &J) -> J {
    match value {
        J::Null => serde_json::json!({"kind":"null"}),
        J::Bool(v) => serde_json::json!({"kind":"boolean","value":v}),
        J::Number(v) => {
            serde_json::json!({"kind":"number","bits":format!("{:016x}",v.as_f64().unwrap().to_bits())})
        }
        J::String(s) => {
            serde_json::json!({"kind":"string","units":s.encode_utf16().collect::<Vec<_>>()})
        }
        J::Array(a) => {
            serde_json::json!({"kind":"array","items":a.iter().map(source_input).collect::<Vec<_>>()})
        }
        J::Object(o) => {
            serde_json::json!({"kind":"object","fields":o.iter().map(|(k,v)|serde_json::json!([k,source_input(v)])).collect::<Vec<_>>()})
        }
    }
}
fn family(methods: &[&str], floor: usize) {
    let rows: Vec<_> = source()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| methods.contains(&r["method"].as_str().unwrap()))
        .collect();
    assert_eq!(rows.len(), floor, "stream::independent family floor");
    for row in rows {
        run(row, false)
    }
}
#[test]
fn private_stream_dispatch() {
    family(&["handleAccountTopic", "handleAccountAllTopic"], 12)
}
#[test]
fn private_stream_orders() {
    family(&["handleOrderTopic"], 18)
}
#[test]
fn private_stream_positions_fills() {
    family(&["handlePositionTopic", "handleFillsTopic"], 18)
}
#[test]
fn private_stream_ping_open() {
    family(&["ping", "onOpen"], 15)
}
#[test]
fn private_stream_auth() {
    family(&["authenticateAndSubscribe"], 12)
}
#[test]
fn private_stream_detached() {
    let row = source()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "authenticateAndSubscribe-success")
        .unwrap();
    run(row, true)
}
