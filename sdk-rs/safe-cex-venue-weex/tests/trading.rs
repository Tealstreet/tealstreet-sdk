use safe_cex_venue_weex::{trading as t, trading::WeexTradingRuntime, types};
use serde_json::Value as Json;
use std::{
    cell::RefCell,
    future::Future,
    path::Path,
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
    Number(f64),
    String(Vec<u16>),
    Object(Rc<RefCell<Vec<(String, V)>>>),
    Array(Rc<RefCell<Vec<Option<V>>>>),
    Set(Vec<V>),
    Map(Rc<RefCell<Vec<(V, V)>>>),
    Function(&'static str),
    Iterator(Rc<RefCell<(V, usize)>>),
}
impl PartialEq for V {
    fn eq(&self, b: &Self) -> bool {
        match (self, b) {
            (Self::Undefined, Self::Undefined) | (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a.to_bits() == b.to_bits(),
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Object(a), Self::Object(b)) => {
                let a = a.borrow();
                let b = b.borrow();
                a.len() == b.len()
                    && a.iter()
                        .all(|(k, v)| b.iter().find(|(x, _)| x == k).is_some_and(|(_, w)| v == w))
            }
            (Self::Array(a), Self::Array(b)) => *a.borrow() == *b.borrow(),
            _ => false,
        }
    }
}
#[derive(Clone, Debug)]
struct Error {
    name: &'static str,
    value: V,
}
type H = RefCell<State>;
type RT = WeexTradingRuntime<H, V, Error>;
type F<'a> = Pin<Box<dyn Future<Output = Result<V, Error>> + 'a>>;
type MC<'a> = Box<dyn FnMut(V, usize) -> Result<V, Error> + 'a>;
type FC<'a> = Box<dyn FnMut(V, usize) -> Result<bool, Error> + 'a>;
struct State {
    input: V,
    events: Vec<String>,
    requests: Vec<V>,
    reported: Vec<V>,
    updates: Vec<V>,
    resolved: Vec<V>,
    infer: Vec<V>,
    count: usize,
    append_done: bool,
    error: V,
}
fn text(s: &str) -> V {
    V::String(s.encode_utf16().collect())
}
fn object(v: Vec<(&str, V)>) -> V {
    V::Object(Rc::new(RefCell::new(
        v.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    )))
}
fn array(v: Vec<V>) -> V {
    V::Array(Rc::new(RefCell::new(v.into_iter().map(Some).collect())))
}
fn decode(j: &Json) -> V {
    match j["kind"].as_str().unwrap() {
        "undefined" => V::Undefined,
        "null" => V::Null,
        "boolean" => V::Bool(j["value"] == true),
        "number" => V::Number(f64::from_bits(
            u64::from_str_radix(j["bits"].as_str().unwrap(), 16).unwrap(),
        )),
        "string" => V::String(serde_json::from_value(j["units"].clone()).unwrap()),
        "object" => V::Object(Rc::new(RefCell::new(
            j["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| (f[0].as_str().unwrap().into(), decode(&f[1])))
                .collect(),
        ))),
        "array" => V::Array(Rc::new(RefCell::new(
            j["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    if v["kind"] == "hole" {
                        None
                    } else {
                        Some(decode(v))
                    }
                })
                .collect(),
        ))),
        s => panic!("decode {s}"),
    }
}
fn raw(v: &V, key: &str) -> V {
    match v {
        V::Object(o) => o
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or(V::Undefined),
        V::Array(a) if key == "length" => V::Number(a.borrow().len() as f64),
        V::Array(a) => key
            .parse::<usize>()
            .ok()
            .and_then(|i| a.borrow().get(i).cloned().flatten())
            .unwrap_or(V::Undefined),
        _ => V::Undefined,
    }
}
fn input(h: &H, key: &str) -> V {
    raw(&h.borrow().input, key)
}
fn flag(h: &H, key: &str) -> bool {
    input(h, key) == V::Bool(true)
}
fn set(v: &V, key: &str, x: V) {
    let V::Object(o) = v else { panic!("set") };
    let mut o = o.borrow_mut();
    if let Some((_, old)) = o.iter_mut().find(|(k, _)| k == key) {
        *old = x
    } else {
        o.push((key.into(), x));
    }
}
fn error(name: &'static str, s: &str) -> Error {
    Error {
        name,
        value: object(vec![("message", text(s))]),
    }
}
fn record(h: &H, s: &str) -> Result<(), Error> {
    h.borrow_mut().events.push(s.into());
    if input(h, "fail") == text(s) {
        Err(error("RangeError", &format!("fail:{s}")))
    } else {
        Ok(())
    }
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Object(a), V::Object(b)) => Rc::ptr_eq(a, b),
        (V::Array(a), V::Array(b)) => Rc::ptr_eq(a, b),
        _ => a == b,
    }
}
fn get(h: &H, v: &V, key: &str) -> Result<V, Error> {
    if matches!(v, V::Null | V::Undefined) {
        return Err(error(
            "TypeError",
            &format!(
                "Cannot read properties of {} (reading '{key}')",
                if matches!(v, V::Null) {
                    "null"
                } else {
                    "undefined"
                }
            ),
        ));
    }
    if flag(h, "traceErrors") {
        let e = h.borrow().error.clone();
        // Two distinct shapes - the thrown value itself, and its nested
        // response.data - both trace under the same event name, as the source
        // does. The arms stay separate because the shapes are.
        #[allow(clippy::if_same_then_else)]
        if same(v, &e) {
            h.borrow_mut().events.push(format!("error:{key}"));
        } else if same(v, &raw(&raw(&e, "response"), "data")) {
            h.borrow_mut().events.push(format!("error:{key}"));
        }
    }
    Ok(raw(v, key))
}
fn truthy(v: &V) -> bool {
    match v {
        V::Undefined | V::Null => false,
        V::Bool(b) => *b,
        V::Number(n) => *n != 0.0 && !n.is_nan(),
        V::String(s) => !s.is_empty(),
        _ => true,
    }
}
// `h` is threaded through every helper in this fixture, and several call sites
// are closures whose signature supplies it (`string: |h, v| ...`), so the
// parameter stays even though only the recursive call reads it here.
#[allow(clippy::only_used_in_recursion)]
fn stringify(h: &H, v: &V) -> Result<Vec<u16>, Error> {
    Ok(match v {
        V::Undefined => "undefined".encode_utf16().collect(),
        V::Null => "null".encode_utf16().collect(),
        V::Bool(b) => b.to_string().encode_utf16().collect(),
        V::Number(n) => safe_cex_num::js_number_string(*n).encode_utf16().collect(),
        V::String(s) => s.clone(),
        V::Object(_) => "[object Object]".encode_utf16().collect(),
        V::Array(a) => {
            let mut s = Vec::new();
            for (i, v) in a.borrow().iter().enumerate() {
                if i > 0 {
                    s.push(44)
                }
                if let Some(v) = v {
                    if !matches!(v, V::Null | V::Undefined) {
                        s.extend(stringify(h, v)?);
                    }
                }
            }
            s
        }
        _ => panic!("stringify"),
    })
}
fn to_number(h: &H, v: V, fallback: V) -> Result<V, Error> {
    let fallback = if matches!(fallback, V::Undefined) {
        V::Number(0.0)
    } else {
        fallback
    };
    if matches!(v, V::Undefined | V::Null) || matches!(&v,V::String(s)if s.is_empty()) {
        return Ok(fallback);
    }
    let n = safe_cex_num::parse_float(&String::from_utf16_lossy(&stringify(h, &v)?));
    Ok(if n.is_finite() {
        V::Number(n)
    } else {
        fallback
    })
}
fn number(v: &V) -> f64 {
    let V::Number(n) = v else {
        panic!("number {v:?}")
    };
    *n
}
fn copy(v: &V) -> V {
    match v {
        V::Object(o) => V::Object(Rc::new(RefCell::new(o.borrow().clone()))),
        _ => object(vec![]),
    }
}
fn deep(v: &V) -> V {
    match v {
        V::Object(o) => V::Object(Rc::new(RefCell::new(
            o.borrow()
                .iter()
                .map(|(k, v)| (k.clone(), deep(v)))
                .collect(),
        ))),
        V::Array(a) => V::Array(Rc::new(RefCell::new(
            a.borrow().iter().map(|v| v.as_ref().map(deep)).collect(),
        ))),
        _ => v.clone(),
    }
}
fn table(h: &H, name: &'static str, v: V) -> Result<V, Error> {
    let key = String::from_utf16_lossy(&stringify(h, &v)?);
    macro_rules! forward {
        ($table:expr) => {
            $table
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| {
                    let j = serde_json::to_value(v).unwrap();
                    text(j.as_str().unwrap())
                })
                .unwrap_or(V::Undefined)
        };
    }
    macro_rules! reverse {
        ($table:expr) => {
            $table
                .iter()
                .find(|(k, _)| serde_json::to_value(k).unwrap() == key)
                .map(|(_, v)| text(v))
                .unwrap_or(V::Undefined)
        };
    }
    Ok(match name {
        "ORDER_SIDE" => forward!(types::ORDER_SIDE),
        "ORDER_STATUS" => forward!(types::ORDER_STATUS),
        "ORDER_TYPE" => forward!(types::ORDER_TYPE),
        "ORDER_TRIGGER_TYPE" => forward!(types::ORDER_TRIGGER_TYPE),
        "POSITION_SIDE" => forward!(types::POSITION_SIDE),
        "TIME_IN_FORCE" => forward!(types::TIME_IN_FORCE),
        "POSITION_MODE" => forward!(types::POSITION_MODE),
        "MARGIN_TYPE" => forward!(types::MARGIN_TYPE),
        "REVERSE_ORDER_SIDE" => reverse!(types::REVERSE_ORDER_SIDE),
        "REVERSE_ORDER_TYPE" => reverse!(types::REVERSE_ORDER_TYPE),
        "REVERSE_POSITION_SIDE" => reverse!(types::REVERSE_POSITION_SIDE),
        "REVERSE_TIME_IN_FORCE" => reverse!(types::REVERSE_TIME_IN_FORCE),
        "REVERSE_ORDER_TRIGGER_TYPE" => reverse!(types::REVERSE_ORDER_TRIGGER_TYPE),
        _ => panic!("table {name}"),
    })
}
fn uppercase(_: &H, v: V) -> Result<V, Error> {
    let V::String(s) = v else { panic!("uppercase") };
    let mut out = Vec::new();
    for c in char::decode_utf16(s) {
        match c {
            Ok(c) => {
                for upper in c.to_uppercase() {
                    let mut buffer = [0; 2];
                    out.extend_from_slice(upper.encode_utf16(&mut buffer));
                }
            }
            Err(e) => out.push(e.unpaired_surrogate()),
        }
    }
    Ok(V::String(out))
}
fn normalize(h: &H, kind: &'static str, v: V) -> Result<V, Error> {
    let v = if truthy(&v) { v } else { text("") };
    let v = uppercase(h, V::String(stringify(h, &v)?))?;
    let (table_name, fallback) = match kind {
        "side" => ("ORDER_SIDE", V::Undefined),
        "status" => ("ORDER_STATUS", text("open")),
        "type" => ("ORDER_TYPE", text("limit")),
        "trigger" => ("ORDER_TRIGGER_TYPE", text("last_price")),
        "position_side" => ("POSITION_SIDE", text("long")),
        _ => panic!("kind"),
    };
    let v = table(h, table_name, v)?;
    Ok(if truthy(&v) { v } else { fallback })
}
fn timestamp(h: &H, v: V) -> Result<V, Error> {
    if matches!(v, V::Undefined | V::Null) || matches!(&v,V::String(s)if s.is_empty()) {
        return Ok(V::Number(0.0));
    }
    let n = String::from_utf16_lossy(&stringify(h, &v)?)
        .parse::<f64>()
        .unwrap_or(f64::NAN);
    Ok(V::Number(if n.is_finite() && n > 0.0 {
        if n >= 1e18 {
            (n / 1e6).floor()
        } else if n >= 1e15 {
            (n / 1e3).floor()
        } else if n >= 1e12 {
            n.floor()
        } else {
            (n * 1000.0).floor()
        }
    } else {
        0.0
    }))
}
fn position_id(h: &H, symbol: V, side: V, mode: V) -> Result<V, Error> {
    if mode == text("oneway") || !truthy(&side) {
        return Ok(symbol);
    }
    let mut s = stringify(h, &symbol)?;
    s.push(58);
    s.extend(stringify(h, &side)?);
    Ok(V::String(s))
}
fn adjust(h: &H, v: V, step: V) -> Result<V, Error> {
    let v = String::from_utf16_lossy(&stringify(h, &v)?)
        .parse::<f64>()
        .unwrap_or(f64::NAN);
    let step_s = String::from_utf16_lossy(&stringify(h, &step)?);
    let step = step_s.parse::<f64>().unwrap();
    let mult = 1.0 / step;
    let adjusted = safe_cex_num::math_round(v * mult) / mult;
    let decimals = step_s.split_once('.').map(|(_, s)| s.len()).unwrap_or(0) as i32;
    let factor = 10_f64.powi(decimals);
    Ok(V::Number(
        safe_cex_num::math_round(adjusted * factor) / factor,
    ))
}
fn len(_: &H, v: &V) -> Result<usize, Error> {
    let V::Array(a) = v else { panic!("len {v:?}") };
    Ok(a.borrow().len())
}
fn element(v: &V, index: usize) -> Option<V> {
    let V::Array(a) = v else { panic!("element") };
    a.borrow().get(index).cloned().flatten()
}
fn map<'a>(h: &'a H, v: &V, mut f: MC<'a>) -> Result<V, Error> {
    let n = len(h, v)?;
    let mut out = vec![None; n];
    for (i, x) in out.iter_mut().enumerate() {
        if let Some(v) = element(v, i) {
            *x = Some(f(v, i)?);
        }
    }
    Ok(V::Array(Rc::new(RefCell::new(out))))
}
fn filter<'a>(h: &'a H, v: &V, mut f: FC<'a>) -> Result<V, Error> {
    let n = len(h, v)?;
    let mut out = vec![];
    for i in 0..n {
        if let Some(v) = element(v, i) {
            if f(v.clone(), i)? {
                out.push(v)
            }
        }
    }
    Ok(array(out))
}
fn some<'a>(h: &'a H, v: &V, mut f: FC<'a>) -> Result<bool, Error> {
    let n = len(h, v)?;
    for i in 0..n {
        if let Some(v) = element(v, i) {
            if f(v, i)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
fn each<'a>(h: &'a H, v: &V, mut f: MC<'a>) -> Result<(), Error> {
    let n = len(h, v)?;
    for i in 0..n {
        if let Some(v) = element(v, i) {
            f(v, i)?;
        }
    }
    Ok(())
}
fn flat_map<'a>(h: &'a H, v: &V, mut f: MC<'a>) -> Result<V, Error> {
    let n = len(h, v)?;
    let mut out = vec![];
    for i in 0..n {
        if let Some(v) = element(v, i) {
            let x = f(v, i)?;
            if let V::Array(a) = x {
                out.extend(a.borrow().iter().flatten().cloned());
            } else {
                out.push(x)
            }
        }
    }
    Ok(array(out))
}
fn same_value(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Number(a), V::Number(b)) => a == b || (a.is_nan() && b.is_nan()),
        (V::Object(a), V::Object(b)) => Rc::ptr_eq(a, b),
        _ => a == b,
    }
}
fn includes(h: &H, v: &V, x: &V) -> Result<bool, Error> {
    Ok((0..len(h, v)?).any(|i| same_value(&element(v, i).unwrap_or(V::Undefined), x)))
}
fn new_set(h: &H, v: &V) -> Result<V, Error> {
    let mut a = vec![];
    for i in 0..len(h, v)? {
        let x = element(v, i).unwrap_or(V::Undefined);
        if !a.iter().any(|v| same_value(v, &x)) {
            a.push(x)
        }
    }
    Ok(V::Set(a))
}
fn iterator(_: &H, v: &V) -> Result<V, Error> {
    Ok(V::Iterator(Rc::new(RefCell::new((v.clone(), 0)))))
}
fn next(h: &H, v: &V) -> Result<Option<V>, Error> {
    let V::Iterator(it) = v else {
        panic!("iterator")
    };
    let mut it = it.borrow_mut();
    let n = len(h, &it.0)?;
    if it.1 >= n {
        return Ok(None);
    }
    let x = element(&it.0, it.1).unwrap_or(V::Undefined);
    it.1 += 1;
    Ok(Some(x))
}
fn push(_: &H, _: V, v: &V, x: V) -> Result<(), Error> {
    let V::Array(a) = v else { panic!("push") };
    a.borrow_mut().push(Some(x));
    Ok(())
}
fn spread(h: &H, callee: V, dest: &V, v: V) -> Result<(), Error> {
    let it = iterator(h, &v)?;
    while let Some(x) = next(h, &it)? {
        push(h, callee.clone(), dest, x)?;
    }
    Ok(())
}
fn uuid(h: &H) -> Result<V, Error> {
    record(h, "uuid")?;
    Ok(text("0123456789-0123456789-0123456789-0123456789"))
}
fn emit(h: &H, _: V, msg: V) -> Result<(), Error> {
    record(h, "emit")?;
    h.borrow_mut().reported.push(msg);
    Ok(())
}
fn parse_order(h: &H, v: V, fallback: V) -> Result<V, Error> {
    use safe_cex_venue_weex::order_result::{WeexOrderResultRuntime, parse_order_result};
    let mut handle = h;
    let r: WeexOrderResultRuntime<&H, V, Error> = WeexOrderResultRuntime {
        optional_property: |h, v, k| {
            if matches!(v, V::Null | V::Undefined) {
                Ok(V::Undefined)
            } else {
                get(h, v, k)
            }
        },
        is_false: |v| *v == V::Bool(false),
        truthy,
        string: |_, s| Ok(text(s)),
        emit_error: |h, v| {
            h.borrow_mut().events.push("errorSink".into());
            emit(h, V::Undefined, v.clone())
        },
        new_error: |h, v| {
            error(
                "Error",
                &String::from_utf16_lossy(&stringify(h, v).unwrap()),
            )
        },
        uuid: |h| uuid(h),
        integer_string: |h, v| {
            if matches!(v, V::Null | V::Undefined) {
                Ok(vec![])
            } else {
                stringify(h, v)
            }
        },
    };
    Ok(V::String(parse_order_result(
        &mut handle,
        &r,
        &v,
        fallback,
    )?))
}
fn call(h: &H, method: V, args: Vec<V>) -> Result<V, Error> {
    let V::Function(name) = method else {
        panic!("method")
    };
    match name {
        "resolveCallerClientOrderId" => {
            record(h, "resolve")?;
            h.borrow_mut().resolved.push(deep(&args[0]));
            get(h, &args[0], "clientOrderId")
        }
        "assertCallerClientOrderIds" => {
            record(h, "assertIds")?;
            Ok(V::Undefined)
        }
        "formatOrderPayload" => {
            record(h, "format")?;
            t::format_order_payload(h, runtime(), args[0].clone())
        }
        "inferPositionSide" => {
            record(h, "infer")?;
            h.borrow_mut().infer.push(deep(&args[0]));
            let reduce = truthy(&get(h, &args[0], "reduceOnly")?);
            let buy = get(h, &args[0], "side")? == text("buy");
            Ok(text(if reduce {
                if buy { "short" } else { "long" }
            } else if buy {
                "long"
            } else {
                "short"
            }))
        }
        "parseOrderResult" => parse_order(h, args[0].clone(), args[1].clone()),
        "updateStoredOrder" => {
            record(h, "storeUpdate")?;
            h.borrow_mut()
                .updates
                .push(object(vec![("a", deep(&args[0])), ("b", deep(&args[1]))]));
            Ok(V::Undefined)
        }
        "getSymbolConfigFromCache" => {
            record(h, "config")?;
            lookup(h, &input(h, "configs"), args[0].clone())
        }
        "isBatchableRegularOrder" => {
            let v = &args[0];
            let typ = get(h, v, "type")?;
            if typ != text("limit") && typ != text("market") {
                return Ok(V::Bool(false));
            }
            for key in ["triggerPrice", "stopLoss", "takeProfit"] {
                if !matches!(get(h, v, key)?, V::Undefined) {
                    return Ok(V::Bool(false));
                }
            }
            for key in ["reduceOnly", "close"] {
                if get(h, v, key)? == V::Bool(true) {
                    return Ok(V::Bool(false));
                }
            }
            for key in ["tpTriggerType", "slTriggerType"] {
                if !matches!(get(h, v, key)?, V::Undefined) {
                    return Ok(V::Bool(false));
                }
            }
            Ok(V::Bool(true))
        }
        _ => panic!("call {name}"),
    }
}
fn call_async<'a>(h: &'a H, method: V, args: Vec<V>) -> Result<F<'a>, Error> {
    let V::Function(name) = method else {
        panic!("method")
    };
    Ok(match name {
        "placeOrder" => t::place_order(h, runtime(), args[0].clone()),
        "placeRegularOrderBatch" => t::place_regular_order_batch(h, runtime(), args[0].clone()),
        "cancelOrders" => {
            record(h, "cancel:start")?;
            Box::pin(async move {
                record(h, "cancel:end")?;
                Ok(V::Undefined)
            })
        }
        _ => panic!("async {name}"),
    })
}
fn request_error(h: &H, stage: &str) -> Error {
    let value = object(vec![
        ("message", text(&format!("fail:{stage}"))),
        (
            "response",
            object(vec![(
                "data",
                object(vec![
                    ("errorMessage", text("VENUE")),
                    ("msg", text("REMOTE")),
                    ("message", text("ALT")),
                ]),
            )]),
        ),
    ]);
    h.borrow_mut().error = value.clone();
    Error {
        name: "RangeError",
        value,
    }
}
fn request<'a>(
    h: &'a H,
    method: &'static str,
    url: &'static str,
    payload: V,
) -> Result<F<'a>, Error> {
    h.borrow_mut().events.push("request:start".into());
    if input(h, "fail") == text("request:start") {
        return Err(request_error(h, "request:start"));
    }
    let index = {
        let mut s = h.borrow_mut();
        s.requests.push(object(vec![
            ("method", text(method)),
            ("url", text(url)),
            ("payload", deep(&payload)),
        ]));
        let i = s.count;
        s.count += 1;
        i
    };
    Ok(Box::pin(async move {
        h.borrow_mut().events.push("request:end".into());
        if input(h, "fail") == text("request:end") {
            return Err(request_error(h, "request:end"));
        }
        let replies = input(h, "replies");
        let data = if index < len(h, &replies)? {
            element(&replies, index).unwrap_or(V::Undefined)
        } else if url == endpoint("POSITIONS") {
            input(h, "data")
        } else if url == endpoint("BATCH_ORDERS") {
            let orders = raw(&payload, "batchOrders");
            map(
                h,
                &orders,
                Box::new(|v, i| {
                    Ok(object(vec![
                        ("clientOrderId", raw(&v, "newClientOrderId")),
                        ("success", V::Bool(true)),
                        ("orderId", text(&(100 + i).to_string())),
                    ]))
                }),
            )?
        } else if url == endpoint("MODIFY_TP_SL_ORDER") {
            object(vec![("success", V::Bool(true))])
        } else {
            object(vec![("orderId", text("100")), ("success", V::Bool(true))])
        };
        if flag(h, "reverseReply") {
            let V::Array(a) = &data else {
                panic!("reverse")
            };
            a.borrow_mut().reverse();
        }
        let holes = input(h, "replyHoles");
        if let V::Array(holes) = holes {
            let V::Array(a) = &data else { panic!("holes") };
            for i in holes.borrow().iter().flatten() {
                a.borrow_mut()[number(i) as usize] = None;
            }
        }
        if flag(h, "appendOrder") && !h.borrow().append_done {
            let orders = input(h, "orders");
            let V::Array(a) = orders else {
                panic!("orders")
            };
            a.borrow_mut().push(Some(object(vec![
                ("symbol", text("BTC")),
                ("type", text("limit")),
                ("side", text("buy")),
                ("amount", V::Number(1.23)),
                ("price", V::Number(10.24)),
                ("clientOrderId", text("C9")),
            ])));
            h.borrow_mut().append_done = true;
        }
        Ok(object(vec![("data", data)]))
    }))
}
fn endpoint(name: &str) -> &'static str {
    types::ENDPOINTS.iter().find(|(k, _)| *k == name).unwrap().1
}
fn lookup(h: &H, v: &V, key: V) -> Result<V, Error> {
    let key = String::from_utf16_lossy(&stringify(h, &key)?);
    get(h, v, &key)
}
fn markets_map(h: &H) -> Result<V, Error> {
    let v = input(h, "markets");
    let mut fields = vec![];
    for i in 0..len(h, &v)? {
        let market = element(&v, i).unwrap();
        fields.push((
            String::from_utf16_lossy(&stringify(h, &raw(&market, "symbol"))?),
            market,
        ));
    }
    Ok(V::Object(Rc::new(RefCell::new(fields))))
}
fn fake(h: &H, market: V, mode: V, margin: V, leverage: V) -> Result<V, Error> {
    let mut out = vec![];
    for side in ["long", "short"] {
        let symbol = raw(&market, "symbol");
        out.push(object(vec![
            (
                "id",
                position_id(h, symbol.clone(), text(side), mode.clone())?,
            ),
            ("symbol", symbol),
            ("side", text(side)),
            ("entryPrice", V::Number(0.0)),
            ("notional", V::Number(0.0)),
            ("leverage", leverage.clone()),
            ("unrealizedPnl", V::Number(0.0)),
            ("contracts", V::Number(0.0)),
            ("liquidationPrice", V::Number(0.0)),
            ("marginType", margin.clone()),
            ("positionMode", mode.clone()),
            ("info", object(vec![])),
        ]));
    }
    Ok(array(out))
}
fn runtime() -> &'static RT {
    static RUNTIME: RT = RT {
        get,
        optional_get: |h, v, k| {
            if matches!(v, V::Null | V::Undefined) {
                Ok(V::Undefined)
            } else {
                get(h, v, k)
            }
        },
        truthy,
        nullish: |v| matches!(v, V::Null | V::Undefined),
        is_undefined: |v| matches!(v, V::Undefined),
        undefined: || V::Undefined,
        null: || V::Null,
        text,
        string_units: V::String,
        number: V::Number,
        boolean: V::Bool,
        object,
        array,
        string: |h, v| Ok(V::String(stringify(h, &v)?)),
        template: stringify,
        uppercase,
        equals: |v, s| *v == text(s),
        strict_true: |v| *v == V::Bool(true),
        strict_false: |v| *v == V::Bool(false),
        is_string: |v| matches!(v, V::String(_)),
        is_boolean: |v| matches!(v, V::Bool(_)),
        is_object: |v| matches!(v, V::Object(_) | V::Array(_)),
        is_number: |v| matches!(v, V::Number(_)),
        number_value: number,
        to_number,
        integer_string: |h, v| {
            Ok(if matches!(v, V::Null | V::Undefined) {
                text("")
            } else {
                V::String(stringify(h, &v)?)
            })
        },
        enum_normalize: normalize,
        table,
        timestamp,
        position_id,
        copy: |_, v| Ok(copy(&v)),
        define: |_, v, k, x| {
            set(v, k, x);
            Ok(())
        },
        omit: |_, v| {
            let V::Object(o) = v else { panic!("omit") };
            let fields = o
                .borrow()
                .iter()
                .filter(|(_, v)| !matches!(v, V::Undefined))
                .cloned()
                .collect();
            Ok(V::Object(Rc::new(RefCell::new(fields))))
        },
        adjust,
        uuid,
        method: |_, name| Ok(V::Function(name)),
        call,
        call_async,
        request,
        error_value: |e| e.value.clone(),
        error_sink: |h| {
            h.borrow_mut().events.push("errorSink".into());
            Ok(V::Undefined)
        },
        emit,
        new_error: |h, v| {
            error(
                "Error",
                &String::from_utf16_lossy(&stringify(h, &v).unwrap()),
            )
        },
        map,
        filter,
        some,
        for_each: each,
        flat_map,
        new_set,
        set_has: |_, set, x| {
            let V::Set(a) = set else { panic!("set") };
            Ok(a.iter().any(|v| same_value(v, x)))
        },
        new_map: |_| Ok(V::Map(Rc::new(RefCell::new(vec![])))),
        map_has: |_, map, x| {
            let V::Map(a) = map else { panic!("map") };
            Ok(a.borrow().iter().any(|(k, _)| same_value(k, x)))
        },
        map_get: |_, map, x| {
            let V::Map(a) = map else { panic!("map") };
            Ok(a.borrow()
                .iter()
                .find(|(k, _)| same_value(k, x))
                .map(|(_, v)| v.clone())
                .unwrap_or(V::Undefined))
        },
        map_set: |_, map, k, v| {
            let V::Map(a) = map else { panic!("map") };
            a.borrow_mut().push((k, v));
            Ok(())
        },
        length: len,
        is_array: |_, v| Ok(matches!(v, V::Array(_))),
        includes,
        set_size: |h, v| {
            let V::Set(a) = new_set(h, v)? else {
                panic!("set")
            };
            Ok(a.len())
        },
        push_method: |_, _| Ok(V::Function("push")),
        push,
        spread_push: spread,
        iterator,
        next,
        close_on_error: |h, _, _| {
            if flag(h, "customIterator") {
                record(h, "iteratorClose").unwrap();
            }
        },
        contains: |h, v, s| Ok(String::from_utf16_lossy(&stringify(h, v)?).contains(s)),
        has: |_, v, k| {
            let V::Object(o) = v else { panic!("has") };
            Ok(o.borrow().iter().any(|(x, _)| x == k))
        },
        valid_order_id: |v| {
            let V::String(s) = v else { return false };
            !s.is_empty() && (49..=57).contains(&s[0]) && s.iter().all(|c| (48..=57).contains(c))
        },
        markets_by_symbol: markets_map,
        lookup,
        markets: |h| Ok(input(h, "markets")),
        public_only: |h| Ok(flag(h, "publicOnly")),
        positions: |h| Ok(input(h, "stored")),
        account_mode: |h| Ok(input(h, "accountMode")),
        fake_positions: fake,
        extract_array: |_, v| {
            if matches!(v, V::Array(_)) {
                return Ok(v);
            }
            let x = raw(&v, "data");
            Ok(if matches!(x, V::Array(_)) {
                x
            } else {
                array(vec![])
            })
        },
        join: |h, v, separator| {
            let mut out = vec![];
            for i in 0..len(h, v)? {
                if i > 0 {
                    out.extend(separator.encode_utf16());
                }
                let value = element(v, i).unwrap_or(V::Undefined);
                if !matches!(value, V::Null | V::Undefined) {
                    out.extend(stringify(h, &value)?);
                }
            }
            Ok(V::String(out))
        },
        unique_id: |h, v| {
            let mut out = vec![];
            let mut ids = vec![];
            for i in 0..len(h, &v)? {
                let value = element(&v, i).unwrap_or(V::Undefined);
                let id = raw(&value, "id");
                if !ids.iter().any(|v| same_value(v, &id)) {
                    ids.push(id);
                    out.push(value)
                }
            }
            Ok(array(out))
        },
        concat: |h, a, b| {
            let out = array(vec![]);
            spread(h, V::Function("push"), &out, a)?;
            spread(h, V::Function("push"), &out, b)?;
            Ok(out)
        },
    };
    &RUNTIME
}
fn block_on<T>(future: impl Future<Output = T>) -> T {
    let w = Waker::noop().clone();
    let mut cx = Context::from_waker(&w);
    let mut f = Box::pin(future);
    for _ in 0..10000 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("trading future exceeded 10000 polls")
}
fn initial_values(h: &H) -> V {
    let h = h.borrow();
    object(vec![
        ("requests", array(h.requests.iter().map(deep).collect())),
        ("reported", array(h.reported.iter().map(deep).collect())),
        ("updates", array(h.updates.iter().map(deep).collect())),
        ("resolved", array(h.resolved.iter().map(deep).collect())),
        ("infer", array(h.infer.iter().map(deep).collect())),
    ])
}
fn run(row: &Json) -> (V, Vec<String>, Vec<String>) {
    let arguments = decode(&row["input"]);
    let method = raw(&arguments, "method");
    let V::String(method_units) = method else {
        panic!("method string");
    };
    let method = String::from_utf16_lossy(&method_units);
    let h = H::new(State {
        input: arguments,
        events: vec![],
        requests: vec![],
        reported: vec![],
        updates: vec![],
        resolved: vec![],
        infer: vec![],
        count: 0,
        append_done: false,
        error: V::Undefined,
    });
    let orders = input(&h, "orders");
    if let V::Array(holes) = input(&h, "holes") {
        let V::Array(a) = &orders else {
            panic!("orders")
        };
        for i in holes.borrow().iter().flatten() {
            a.borrow_mut()[number(i) as usize] = None;
        }
    }
    let mut initial = V::Undefined;
    let mut prefix = vec![];
    let outcome = match method.as_str() {
        "normalizeOrder" => {
            t::normalize_order(&h, runtime(), input(&h, "raw"), input(&h, "market"))
        }
        "normalizeAlgoOrder" => {
            t::normalize_algo_order(&h, runtime(), input(&h, "raw"), input(&h, "market"))
        }
        "normalizePosition" => t::normalize_position(
            &h,
            runtime(),
            input(&h, "raw"),
            input(&h, "market"),
            input(&h, "fallback"),
        ),
        "formatOrderPayload" => {
            let x = call(
                &h,
                V::Function("formatOrderPayload"),
                vec![input(&h, "opts")],
            );
            if x.is_ok() {
                prefix = h.borrow().events.clone();
                initial = initial_values(&h);
            }
            x
        }
        name => {
            let f = match name {
                "placeOrder" => t::place_order(&h, runtime(), input(&h, "opts")),
                "placeOrders" => t::place_orders(&h, runtime(), orders),
                "placeRegularOrderBatch" => t::place_regular_order_batch(&h, runtime(), orders),
                "updateOrder" => t::update_order(&h, runtime(), input(&h, "update")),
                "fetchPositions" => t::fetch_positions(&h, runtime(), input(&h, "symbols")),
                _ => panic!("method"),
            };
            prefix = h.borrow().events.clone();
            initial = initial_values(&h);
            block_on(f)
        }
    };
    let (result, error) = match outcome {
        Ok(v) => (v, V::Null),
        Err(e) => {
            let message = get(&h, &e.value, "message").unwrap();
            (
                V::Undefined,
                object(vec![("name", text(e.name)), ("message", message)]),
            )
        }
    };
    let state = h.borrow();
    let value = object(vec![
        ("result", result),
        ("error", error),
        ("requests", array(state.requests.clone())),
        ("reported", array(state.reported.clone())),
        ("updates", array(state.updates.clone())),
        ("resolved", array(state.resolved.clone())),
        ("infer", array(state.infer.clone())),
        ("initial", initial),
    ]);
    (value, prefix, state.events.clone())
}
#[test]
fn trading_native_source_floor() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/trading-oracle.cjs");
    let out = Command::new("node").arg(p).output().unwrap();
    assert!(
        out.status.success(),
        "oracle {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Vec<Json> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.len(), 76, "trading::fixture_floor");
    let native: Vec<_> = rows.iter().map(run).collect();
    for (row, (actual, _, _)) in rows.iter().zip(&native) {
        let expected = decode(&row["expected"]);
        let V::String(method_units) = raw(&decode(&row["input"]), "method") else {
            panic!("source method");
        };
        let method = String::from_utf16_lossy(&method_units);
        if row["id"] == "algo_virtual_pair" {
            let V::Array(a) = raw(actual, "result") else {
                panic!("native algo orders");
            };
            assert_eq!(a.borrow().len(), 3, "trading::algo_native_three_orders");
        }
        if row["id"] == "positions_open_and_fake" {
            let V::Array(a) = raw(actual, "result") else {
                panic!("native positions");
            };
            assert_eq!(a.borrow().len(), 4, "trading::positions_native_four_ids");
        }
        if row["id"] == "orders_chunking" {
            let V::Array(a) = raw(actual, "requests") else {
                panic!("native requests");
            };
            assert_eq!(a.borrow().len(), 2, "trading::orders_native_two_requests");
        }
        assert_eq!(
            *actual,
            expected,
            "trading::{method}_native {}",
            row["id"].as_str().unwrap()
        );
    }
    for (row, (_, prefix, events)) in rows.iter().zip(&native) {
        let expected_prefix: Vec<String> = serde_json::from_value(row["prefix"].clone()).unwrap();
        assert_eq!(
            *prefix, expected_prefix,
            "trading::prefix_trace {}",
            row["id"]
        );
        let expected: Vec<String> = serde_json::from_value(row["events"].clone()).unwrap();
        assert_eq!(*events, expected, "trading::trace {}", row["id"]);
    }
}
