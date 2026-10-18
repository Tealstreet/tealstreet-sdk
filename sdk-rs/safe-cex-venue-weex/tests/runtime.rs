#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
#[allow(dead_code)]
mod pager_runtime;
use safe_cex_exchange::{income_paging::IncomePageParams, runtime::ExchangeError};
use safe_cex_types::IncomeRecord;
use safe_cex_venue_weex::{
    api_binding as a, income, income_reads as i, lifecycle as l, types, utils::WeexUtilityInput,
};
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
    Map(Rc<RefCell<Vec<(V, V)>>>),
    Function(&'static str),
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
            (Self::Function(a), Self::Function(b)) => a == b,
            (Self::Map(a), Self::Map(b)) => *a.borrow() == *b.borrow(),
            _ => false,
        }
    }
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
        "function" => V::Function(Box::leak(
            j["name"].as_str().unwrap().to_owned().into_boxed_str(),
        )),
        "map" => V::Map(Rc::new(RefCell::new(vec![]))),
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
#[derive(Clone, Debug)]
struct Error {
    value: V,
    name: &'static str,
}
type H = Rc<RefCell<State>>;
type F<'a, T = V> = Pin<Box<dyn Future<Output = Result<T, Error>> + 'a>>;
struct State {
    input: V,
    instance: V,
    store: V,
    disposed: bool,
    requests: Vec<V>,
    updates: Vec<V>,
    logs: Vec<V>,
    reported: Vec<V>,
    calls: Vec<V>,
    sleeps: Vec<V>,
    registrations: Vec<V>,
    creates: Vec<V>,
    callbacks: Vec<a::WeexApiCallback<V>>,
    page: usize,
}
fn input(h: &H, k: &str) -> V {
    raw(&h.borrow().input, k)
}
fn set(v: &V, k: &str, x: V) {
    let V::Object(o) = v else { panic!("set") };
    let mut o = o.borrow_mut();
    if let Some((_, v)) = o.iter_mut().find(|(key, _)| key == k) {
        *v = x
    } else {
        o.push((k.into(), x))
    }
}
fn truthy(v: &V) -> bool {
    match v {
        V::Undefined | V::Null => false,
        V::Bool(x) => *x,
        V::Number(x) => *x != 0.0 && !x.is_nan(),
        V::String(x) => !x.is_empty(),
        _ => true,
    }
}
fn nullish(v: &V) -> bool {
    matches!(v, V::Undefined | V::Null)
}
fn error(s: &str) -> Error {
    Error {
        name: "Error",
        value: object(vec![("message", text(s))]),
    }
}
fn get(h: &H, v: &V, k: &str) -> Result<V, Error> {
    if nullish(v) {
        return Err(Error {
            name: "TypeError",
            value: object(vec![(
                "message",
                text(&format!(
                    "Cannot read properties of {} (reading '{k}')",
                    if matches!(v, V::Null) {
                        "null"
                    } else {
                        "undefined"
                    }
                )),
            )]),
        });
    }
    if same(v, &h.borrow().instance) && k == "isDisposed" {
        return Ok(V::Bool(h.borrow().disposed));
    }
    Ok(raw(v, k))
}
fn optional(h: &H, v: &V, k: &str) -> Result<V, Error> {
    if nullish(v) {
        Ok(V::Undefined)
    } else {
        get(h, v, k)
    }
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Object(a), V::Object(b)) => Rc::ptr_eq(a, b),
        (V::Array(a), V::Array(b)) => Rc::ptr_eq(a, b),
        _ => a == b,
    }
}
fn units(v: &V) -> Vec<u16> {
    match v {
        V::String(x) => x.clone(),
        V::Undefined => "undefined".encode_utf16().collect(),
        V::Null => "null".encode_utf16().collect(),
        V::Number(x) => safe_cex_num::js_number_string(*x).encode_utf16().collect(),
        V::Bool(x) => x.to_string().encode_utf16().collect(),
        _ => "[object Object]".encode_utf16().collect(),
    }
}
fn numeric(_: &H, v: &V) -> Result<f64, Error> {
    Ok(match v {
        V::Number(x) => *x,
        V::String(x) => safe_cex_num::number_from_string(&String::from_utf16_lossy(x)),
        V::Bool(x) => {
            if *x {
                1.0
            } else {
                0.0
            }
        }
        V::Null => 0.0,
        _ => f64::NAN,
    })
}
fn copy(_: &H, v: &V) -> Result<V, Error> {
    Ok(match v {
        V::Object(x) => V::Object(Rc::new(RefCell::new(x.borrow().clone()))),
        _ => object(vec![]),
    })
}
fn define(_: &H, v: &V, k: &'static str, x: V) -> Result<(), Error> {
    set(v, k, x);
    Ok(())
}
fn call_name(h: &H, name: &str) {
    h.borrow_mut().calls.push(text(name));
}
fn failure(h: &H) -> Error {
    let e = input(h, "error");
    let value = object(vec![("message", text("failure"))]);
    if let V::Object(o) = e {
        for (k, v) in o.borrow().iter() {
            set(&value, k, v.clone())
        }
    }
    Error {
        name: "Error",
        value,
    }
}
fn ready<'a>(h: &'a H, name: &'static str, value: V) -> F<'a> {
    call_name(h, name);
    Box::pin(async move {
        if input(h, "fail") == text(name) {
            return Err(failure(h));
        }
        if input(h, "disposeAt") == text(name) {
            h.borrow_mut().disposed = true;
        }
        Ok(value)
    })
}
fn method(h: &H, target: &V, k: &'static str) -> Result<V, Error> {
    let prefix = if same(target, &h.borrow().store) {
        "store"
    } else if raw(target, "kind") == text("public") {
        "public"
    } else if raw(target, "kind") == text("private") {
        "private"
    } else if same(target, &h.borrow().instance) {
        "this"
    } else {
        "xhr"
    };
    Ok(V::Function(Box::leak(
        format!("{prefix}:{k}").into_boxed_str(),
    )))
}
fn call(h: &H, f: V, args: Vec<V>) -> Result<V, Error> {
    let V::Function(name) = f else {
        panic!("function")
    };
    match name {
        "this:createXhrWithSecondaryKey" => {
            call_name(h, "xhr");
            let instance = h.borrow().instance.clone();
            let V::Object(fields) = instance else {
                panic!("instance")
            };
            let names = fields.borrow().iter().map(|(name, _)| text(name)).collect();
            h.borrow_mut()
                .creates
                .push(object(vec![("constructorFields", array(names))]));
            assert_eq!(args[1], V::Function("createAPI"));
            assert!(
                same(&args[0], &raw(&h.borrow().instance, "options")),
                "runtime::constructor_options_identity"
            );
            Ok(text("xhr"))
        }
        "public:connectAndSubscribe" => {
            call_name(h, "connectPublic");
            Ok(V::Undefined)
        }
        "private:connectAndSubscribe" => {
            call_name(h, "connectPrivate");
            Ok(V::Undefined)
        }
        "store:update" => {
            h.borrow_mut().updates.push(args[0].clone());
            let loaded = raw(&args[0], "loaded");
            if !matches!(loaded, V::Undefined) {
                set(&h.borrow().store, "loaded", loaded)
            }
            Ok(V::Undefined)
        }
        "store:setSetting" => {
            h.borrow_mut().updates.push(object(vec![
                ("setting", args[0].clone()),
                ("value", args[1].clone()),
            ]));
            Ok(V::Undefined)
        }
        "store:addOrUpdatePositionsFull" => {
            h.borrow_mut()
                .updates
                .push(object(vec![("fullPositions", args[0].clone())]));
            Ok(V::Undefined)
        }
        "this:log" => {
            h.borrow_mut().logs.push(object(vec![
                ("message", args[0].clone()),
                ("severity", args.get(1).cloned().unwrap_or(V::Undefined)),
            ]));
            Ok(V::Undefined)
        }
        "this:emitAndLogError" => {
            h.borrow_mut().reported.push(args[0].clone());
            Ok(V::Undefined)
        }
        "this:getTickInterval" => Ok(V::Number(17.0)),
        "this:tick" => {
            call_name(h, "tick");
            Ok(V::Undefined)
        }
        _ => panic!("call {name}"),
    }
}
fn call_async<'a>(h: &'a H, f: V, args: Vec<V>) -> Result<F<'a>, Error> {
    let V::Function(name) = f else {
        panic!("asyncfn")
    };
    assert_eq!(
        args.len(),
        if name == "xhr:get" { 1 } else { 0 },
        "runtime::method_argument_floor {name}"
    );
    Ok(match name {
        "this:initExternalPublicData" => ready(
            h,
            "external",
            if matches!(input(h, "external"), V::Undefined) {
                V::Bool(false)
            } else {
                input(h, "external")
            },
        ),
        "this:fetchMarkets" => ready(
            h,
            "markets",
            array(vec![
                object(vec![("symbol", text("BTC"))]),
                object(vec![("symbol", text("ETH"))]),
            ]),
        ),
        "this:fetchTickers" => ready(
            h,
            "tickers",
            array(vec![object(vec![("symbol", text("BTC"))])]),
        ),
        "this:fetchSymbolConfigs" => ready(h, "symbols", V::Undefined),
        "this:fetchBalanceAndPositions" => ready(
            h,
            "balance",
            object(vec![
                (
                    "balance",
                    object(vec![("total", V::Number(3.0)), ("free", V::Number(2.0))]),
                ),
                ("positions", array(vec![object(vec![("id", text("p"))])])),
            ]),
        ),
        "this:fetchOrders" => ready(h, "orders", array(vec![object(vec![("id", text("o"))])])),
        "this:validateAccount" => ready(
            h,
            "validate",
            if matches!(input(h, "validation"), V::Undefined) {
                object(vec![("error", text(""))])
            } else {
                input(h, "validation")
            },
        ),
        "this:tick" => ready(h, "tick", V::Undefined),
        "xhr:get" => {
            h.borrow_mut().requests.push(object(vec![
                ("method", text("GET")),
                ("endpoint", args[0].clone()),
            ]));
            ready(
                h,
                "config",
                object(vec![(
                    "data",
                    if matches!(input(h, "config"), V::Undefined) {
                        object(vec![("canTrade", V::Bool(true))])
                    } else {
                        input(h, "config")
                    },
                )]),
            )
        }
        _ => panic!("async {name}"),
    })
}
fn rt() -> &'static l::WeexLifecycleRuntime<H, V, Error> {
    &l::WeexLifecycleRuntime {
        base_construct: |h, _, options, store| {
            let instance = object(vec![
                ("options", options),
                ("store", store),
                ("isPublicDataOnly", V::Bool(truthy(&input(h, "publicOnly")))),
            ]);
            h.borrow_mut().instance = instance.clone();
            Ok(instance)
        },
        client_rule: |_, _| {
            Ok(object(vec![
                ("label", text("WEEX")),
                ("requirement", text("match /^[.A-Z:/a-z0-9_-]{1,36}$/")),
                ("isValid", V::Function("isValid")),
                ("mint", V::Function("mint")),
            ]))
        },
        api_factory: |_| Ok(V::Function("createAPI")),
        bind: |_, _, name| Ok(V::Function(name)),
        new_map: |_| Ok(V::Map(Rc::new(RefCell::new(vec![])))),
        define,
        assign: define,
        constant: |_, name| {
            Ok(array(
                if name == "GRAFANA_ENDPOINTS_EVERY" {
                    types::GRAFANA_ENDPOINTS_EVERY
                } else {
                    types::GRAFANA_ENDPOINTS_PERIODIC
                }
                .iter()
                .map(|x| text(x))
                .collect(),
            ))
        },
        this: |h| h.borrow().instance.clone(),
        get,
        optional_get: optional,
        method,
        call,
        call_async,
        construct: |h, name, _| {
            let kind = if name == "WeexPublicWebsocket" {
                "public"
            } else {
                "private"
            };
            call_name(h, kind);
            Ok(object(vec![("kind", text(kind))]))
        },
        then: |h, f, callback| {
            Ok(Box::pin(async move {
                let value = f.await?;
                l::lifecycle_callback(h, rt(), callback, value)
            }))
        },
        all: |_, a, b| {
            Ok(Box::pin(async move {
                let a = a.await;
                let b = b.await;
                a?;
                b?;
                Ok(V::Undefined)
            }))
        },
        loop_callback: |_, _| Ok(V::Function("tick")),
        schedule_loop: |h, _, ms| {
            call_name(h, "loop");
            h.borrow_mut()
                .updates
                .push(object(vec![("loopInterval", ms)]));
            Ok(())
        },
        object: |x| object(x),
        undefined: || V::Undefined,
        text,
        string_units: V::String,
        boolean: V::Bool,
        number: V::Number,
        truthy,
        strict_false: |x| *x == V::Bool(false),
        copy,
        template: |_, v| Ok(units(v)),
        numeric,
        equals_text: |v, s| *v == text(s),
        hash_input: |_, v| match v {
            V::String(x) => Ok(String::from_utf16_lossy(&x).into_bytes()),
            _ => Err(error("invalid hash input")),
        },
        error_value: |e| e.value.clone(),
        new_error: |_, v| Error {
            name: "Error",
            value: object(vec![("message", v)]),
        },
    }
}
fn settle_range(
    runtime: &pager_runtime::Runtime,
    mut future: safe_cex_exchange::runtime::ExchangeFuture<'static, Vec<IncomeRecord>>,
) -> Result<Vec<IncomeRecord>, ExchangeError> {
    for _ in 0..128 {
        if let Poll::Ready(result) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return result;
        }
        runtime.advance(0);
    }
    panic!("WEEX range did not settle");
}
fn block_on<T>(mut f: Pin<Box<dyn Future<Output = T> + '_>>) -> T {
    let w = Waker::noop().clone();
    loop {
        if let Poll::Ready(x) = f.as_mut().poll(&mut Context::from_waker(&w)) {
            return x;
        }
    }
}
fn to_json(v: &V) -> Json {
    match v {
        V::Undefined | V::Null => Json::Null,
        V::Bool(x) => Json::Bool(*x),
        V::Number(x) => Json::from(*x),
        V::String(x) => Json::from(String::from_utf16_lossy(x)),
        V::Array(x) => Json::Array(
            x.borrow()
                .iter()
                .map(|v| to_json(v.as_ref().unwrap_or(&V::Undefined)))
                .collect(),
        ),
        V::Object(x) => Json::Object(
            x.borrow()
                .iter()
                .filter(|(_, v)| !matches!(v, V::Undefined))
                .map(|(k, v)| (k.clone(), to_json(v)))
                .collect(),
        ),
        _ => panic!("json"),
    }
}
fn json_text(v: &V) -> String {
    match v {
        V::Object(o) => format!(
            "{{{}}}",
            o.borrow()
                .iter()
                .filter(|(_, v)| !matches!(v, V::Undefined))
                .map(|(k, v)| format!("{}:{}", Json::from(k.clone()), json_text(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        V::Array(a) => format!(
            "[{}]",
            a.borrow()
                .iter()
                .map(|v| json_text(v.as_ref().unwrap_or(&V::Null)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        V::Number(n) => {
            if n.is_finite() {
                safe_cex_num::js_number_string(*n)
            } else {
                "null".into()
            }
        }
        _ => to_json(v).to_string(),
    }
}
fn from_json(v: &Json) -> V {
    match v {
        Json::Null => V::Null,
        Json::Bool(x) => V::Bool(*x),
        Json::Number(x) => V::Number(x.as_f64().unwrap()),
        Json::String(x) => text(x),
        Json::Array(a) => array(a.iter().map(from_json).collect()),
        Json::Object(o) => object(o.iter().map(|(k, v)| (k.as_str(), from_json(v))).collect()),
    }
}
fn api_rt() -> &'static a::WeexApiRuntime<H, V, Error> {
    &a::WeexApiRuntime {
        get,
        optional_get: optional,
        truthy,
        nullish,
        is_undefined: |v| matches!(v, V::Undefined),
        is_string: |v| matches!(v, V::String(_)),
        text,
        string_units: V::String,
        number: V::Number,
        object: |v| object(v),
        template: |_, v| Ok(units(&v)),
        copy,
        define,
        starts_with: |_, v, s| Ok(String::from_utf16_lossy(&units(v)).starts_with(s)),
        uppercase_method: |_, v| {
            Ok(if nullish(v) {
                V::Undefined
            } else {
                text(&String::from_utf16_lossy(&units(v)).to_uppercase())
            })
        },
        locale_compare: |_, a, b| {
            Ok(V::Number(match units(&a).cmp(&units(&b)) {
                std::cmp::Ordering::Less => -1.0,
                std::cmp::Ordering::Equal => 0.0,
                std::cmp::Ordering::Greater => 1.0,
            }))
        },
        qs_stringify: |h, v, format, compare| {
            let mut keys = vec![];
            fn collect(v: &V, out: &mut Vec<String>) {
                match v {
                    V::Object(o) => {
                        for (k, v) in o.borrow().iter() {
                            out.push(k.clone());
                            collect(v, out)
                        }
                    }
                    V::Array(a) => {
                        for (i, v) in a.borrow().iter().enumerate() {
                            out.push(i.to_string());
                            if let Some(v) = v {
                                collect(v, out)
                            }
                        }
                    }
                    _ => {}
                }
            }
            collect(&v, &mut keys);
            keys.sort();
            keys.dedup();
            let comparisons: Vec<_> = keys
                .iter()
                .flat_map(|a| keys.iter().map(move |b| (a, b)))
                .map(|(a, b)| {
                    let V::Number(value) = compare(h, text(a), text(b)).unwrap() else {
                        panic!("cmp")
                    };
                    serde_json::json!([a, b, value])
                })
                .collect();
            let p = Path::new(std::env::var("SAFE_CEX_TYPESCRIPT_PATH").unwrap().as_str())
                .parent()
                .unwrap()
                .join("qs");
            let out=Command::new("node").arg("-e").arg("const qs=require(process.argv[1]);process.stdout.write(qs.stringify(JSON.parse(process.argv[2]),{arrayFormat:process.argv[3],sort:(a,b)=>JSON.parse(process.argv[4]).find(x=>x[0]===a&&x[1]===b)[2]}));").arg(p).arg(to_json(&v).to_string()).arg(format).arg(serde_json::to_string(&comparisons).unwrap()).output().unwrap();
            assert!(out.status.success());
            Ok(text(&String::from_utf8(out.stdout).unwrap()))
        },
        stringify: |_, v| Ok(text(&json_text(&v))),
        callback: |h, c| {
            let name = match c {
                a::WeexApiCallback::Params => "serializeWeexParams",
                a::WeexApiCallback::Request { .. } => "request",
                a::WeexApiCallback::Response => "response",
            };
            h.borrow_mut().callbacks.push(c);
            Ok(V::Function(name))
        },
        transform_response: |_| Ok(V::Function("transform")),
        create: |h, c, options, exchange| {
            h.borrow_mut().creates.push(object(vec![
                ("config", c),
                ("options", options),
                ("exchange", exchange),
            ]));
            Ok(text("xhr"))
        },
        install_retry: |h, _| {
            h.borrow_mut().registrations.push(text("retry"));
            Ok(())
        },
        register: |h, _, name, _| {
            h.borrow_mut().registrations.push(text(name));
            Ok(())
        },
        clock_ms: |_| Ok(V::Number(123456.0)),
        sign: |_, secret, message| {
            Ok(text(&safe_cex_venue_weex::signing::sign_weex_message(
                &units(&secret),
                &message,
            )))
        },
        axios_headers: |_, v| Ok(v),
        new_error: |_, v| Ok(object(vec![("message", v)])),
        throw: |v| Error {
            name: "Error",
            value: v,
        },
    }
}
fn income_rt() -> &'static i::WeexIncomeRuntime<H, V, Error> {
    &i::WeexIncomeRuntime {
        public_only: |h| Ok(truthy(&input(h, "publicOnly"))),
        request_factory: |_| Ok(V::Function("post")),
        post: |h, _, endpoint, payload| {
            h.borrow_mut().requests.push(object(vec![
                ("method", text("POST")),
                ("endpoint", text(endpoint)),
                ("payload", payload),
            ]));
            Ok(ready(
                h,
                "income",
                object(vec![(
                    "data",
                    if matches!(input(h, "data"), V::Undefined) {
                        array(vec![])
                    } else {
                        input(h, "data")
                    },
                )]),
            ))
        },
        optional_get: optional,
        get,
        truthy,
        nullish,
        undefined: || V::Undefined,
        null: || V::Null,
        text,
        number: V::Number,
        numeric: |h, v| numeric(h, &v),
        object: |v| object(v),
        array,
        extract_array: |_, v, keys| {
            if matches!(v, V::Array(_)) {
                return Ok(v);
            }
            let data = raw(&v, "data");
            if matches!(data, V::Array(_)) {
                return Ok(data);
            }
            for key in keys {
                let x = raw(&v, key);
                if matches!(x, V::Array(_)) {
                    return Ok(x);
                }
                let x = raw(&data, key);
                if matches!(x, V::Array(_)) {
                    return Ok(x);
                }
            }
            Ok(array(vec![]))
        },
        map: |_, v, mut f| {
            let V::Array(a) = v else { panic!("map") };
            let vals = a.borrow().clone();
            Ok(array(
                vals.into_iter()
                    .flatten()
                    .map(&mut f)
                    .collect::<Result<Vec<_>, _>>()?,
            ))
        },
        normalize: |_, v| {
            let raw: WeexUtilityInput = serde_json::from_value(to_json(&v)).unwrap();
            let record = income::normalize_income(&raw, |x| x).map_err(|e| error(&e.message))?;
            Ok(record_value(&record))
        },
    }
}
fn record_value(v: &IncomeRecord) -> V {
    object(vec![
        ("symbol", text(&v.symbol)),
        ("incomeType", text(&v.income_type)),
        ("income", V::Number(v.income)),
        ("asset", text(&v.asset)),
        ("timestamp", V::Number(v.timestamp)),
        ("tranId", text(&v.tran_id)),
        (
            "tradeId",
            v.trade_id.as_deref().map(text).unwrap_or(V::Undefined),
        ),
        (
            "category",
            from_json(&serde_json::to_value(&v.category).unwrap()),
        ),
    ])
}
fn page_record(v: &V) -> IncomeRecord {
    serde_json::from_value(to_json(v)).unwrap()
}
fn range_rt() -> &'static i::WeexIncomeRangeRuntime<H, V> {
    &i::WeexIncomeRangeRuntime {
        fetch_page: |h, p: IncomePageParams, income_type| {
            h.borrow_mut().requests.push(object(vec![
                ("method", text("PAGE")),
                (
                    "params",
                    object(vec![
                        ("startTime", V::Number(p.start_time)),
                        ("endTime", V::Number(p.end_time)),
                        ("incomeType", income_type),
                        ("limit", V::Number(p.limit)),
                    ]),
                ),
            ]));
            call_name(h, "range");
            let index = h.borrow().page;
            h.borrow_mut().page += 1;
            Ok(Box::pin(async move {
                if input(h, "fail") == text("range") {
                    return Err(ExchangeError::new("failure"));
                }
                let pages = input(h, "pages");
                let page = raw(&pages, &index.to_string());
                let values = if let V::Array(a) = page {
                    a.borrow().iter().flatten().map(page_record).collect()
                } else {
                    vec![]
                };
                Ok(values)
            }))
        },
        sleep: |h, ms| {
            h.borrow_mut().sleeps.push(V::Number(ms));
            Ok(Box::pin(std::future::ready(Ok(()))))
        },
        warning_sink: |_| Ok(V::Function("log")),
        warning: |h, _, msg| {
            h.borrow_mut().logs.push(object(vec![
                ("message", V::String(msg)),
                ("severity", text("warning")),
            ]));
            Ok(())
        },
        iso_date: |_, ms| {
            assert!((0.0..1000.0).contains(&ms));
            Ok(format!("1970-01-01T00:00:00.{:03}Z", ms as u32)
                .encode_utf16()
                .collect())
        },
    }
}
fn snapshot(h: &H) -> V {
    let s = h.borrow();
    object(vec![
        ("requests", array(s.requests.clone())),
        ("calls", array(s.calls.clone())),
        ("updates", array(s.updates.clone())),
        ("sleeps", array(s.sleeps.clone())),
    ])
}
fn run(row: &Json) -> V {
    let captured_input = decode(&row["input"]);
    let store = object(vec![(
        "loaded",
        object(vec![
            ("markets", V::Bool(false)),
            ("tickers", V::Bool(false)),
            ("balance", V::Bool(false)),
            ("positions", V::Bool(false)),
            ("orders", V::Bool(false)),
        ]),
    )]);
    let h = Rc::new(RefCell::new(State {
        input: captured_input,
        instance: object(vec![]),
        store: store.clone(),
        disposed: false,
        requests: vec![],
        updates: vec![],
        logs: vec![],
        reported: vec![],
        calls: vec![],
        sleeps: vec![],
        registrations: vec![],
        creates: vec![],
        callbacks: vec![],
        page: 0,
    }));
    let name = String::from_utf16_lossy(&units(&input(&h, "method")));
    let mut initial = V::Undefined;
    let auth = object(vec![
        ("passphrase", text("p")),
        ("key", text("k")),
        ("secret", text("s")),
    ]);
    let outcome: Result<V, Error> = if ["serialize", "api", "request", "response"]
        .contains(&name.as_str())
    {
        if name == "serialize" {
            a::serialize_weex_params(&h, api_rt(), input(&h, "params"))
        } else {
            let options = if matches!(input(&h, "options"), V::Undefined) {
                auth
            } else {
                input(&h, "options")
            };
            match a::create_api(&h, api_rt(), options, object(vec![])) {
                Err(e) => Err(e),
                Ok(xhr) => {
                    if name == "api" {
                        Ok(xhr)
                    } else {
                        let callback =
                            h.borrow().callbacks[if name == "request" { 1 } else { 2 }].clone();
                        let value = input(
                            &h,
                            if name == "request" {
                                "config"
                            } else {
                                "response"
                            },
                        );
                        a::run_api_callback(&h, api_rt(), callback, value.clone()).map(|result| {
                            object(vec![
                                (
                                    if name == "request" {
                                        "config"
                                    } else {
                                        "response"
                                    },
                                    result.clone(),
                                ),
                                ("same", V::Bool(same(&result, &value))),
                            ])
                        })
                    }
                }
            }
        }
    } else {
        let options = if matches!(input(&h, "options"), V::Undefined) {
            object(vec![])
        } else {
            input(&h, "options")
        };
        let instance = l::construct_exchange(&h, rt(), text("WEEX"), options, store).unwrap();
        if name == "constructor" {
            Ok(instance)
        } else {
            h.borrow_mut().calls.clear();
            h.borrow_mut().creates.clear();
            set(&instance, "xhr", text("xhr"));
            set(
                &instance,
                "publicWebsocket",
                object(vec![("kind", text("public"))]),
            );
            set(
                &instance,
                "privateWebsocket",
                object(vec![("kind", text("private"))]),
            );
            if name == "fetchIncomeHistoryRange" {
                let pager_runtime = Rc::new(pager_runtime::Runtime::default());
                let future = i::fetch_income_history_range(
                    Rc::new(h.clone()),
                    range_rt(),
                    pager_runtime.clone(),
                    numeric(&h, &input(&h, "start")).unwrap(),
                    numeric(&h, &input(&h, "end")).unwrap(),
                    input(&h, "incomeType"),
                );
                initial = snapshot(&h);
                settle_range(&pager_runtime, future)
                    .map(|records| {
                        let expected = decode(&row["expected"]);
                        let source_records = raw(&expected, "result");
                        let V::Array(expected_records) = source_records else {
                            panic!("source records")
                        };
                        let id = row["id"].as_str().unwrap();
                        let floor = if id == "range_short" { 1 } else { 0 };
                        assert_eq!(records.len(), floor, "runtime::range_native_count {id}");
                        for (native, source) in records
                            .iter()
                            .zip(expected_records.borrow().iter().flatten())
                        {
                            assert_eq!(
                                native.symbol.encode_utf16().collect::<Vec<_>>(),
                                units(&raw(source, "symbol")),
                                "runtime::range_native_symbol {id}"
                            );
                            assert_eq!(
                                native.income_type.encode_utf16().collect::<Vec<_>>(),
                                units(&raw(source, "incomeType")),
                                "runtime::range_native_type {id}"
                            );
                            assert_eq!(
                                native.income.to_bits(),
                                numeric(&h, &raw(source, "income")).unwrap().to_bits(),
                                "runtime::range_native_income {id}"
                            );
                            assert_eq!(
                                native.asset.encode_utf16().collect::<Vec<_>>(),
                                units(&raw(source, "asset")),
                                "runtime::range_native_asset {id}"
                            );
                            assert_eq!(
                                native.timestamp.to_bits(),
                                numeric(&h, &raw(source, "timestamp")).unwrap().to_bits(),
                                "runtime::range_native_timestamp {id}"
                            );
                            assert_eq!(
                                native.tran_id.encode_utf16().collect::<Vec<_>>(),
                                units(&raw(source, "tranId")),
                                "runtime::range_native_id {id}"
                            );
                            assert_eq!(
                                native.trade_id, None,
                                "runtime::range_native_trade_id {id}"
                            );
                            assert_eq!(
                                native.category,
                                Some(safe_cex_types::IncomeCategory::Funding),
                                "runtime::range_native_category {id}"
                            );
                        }
                        array(
                            records
                                .iter()
                                .map(|r| {
                                    let value = record_value(r);
                                    if r.trade_id.is_none() {
                                        let V::Object(o) = &value else { unreachable!() };
                                        o.borrow_mut().retain(|(k, _)| k != "tradeId");
                                    }
                                    value
                                })
                                .collect(),
                        )
                    })
                    .map_err(|e| error(&e.message))
            } else {
                let f = match name.as_str() {
                    "validateAccount" => l::validate_account(&h, rt()),
                    "getAccount" => l::get_account(&h, rt()),
                    "start" => l::start(&h, rt()),
                    "tick" => l::tick(&h, rt()),
                    "fetchIncomeHistory" => {
                        i::fetch_income_history(&h, income_rt(), input(&h, "opts"))
                    }
                    _ => panic!("method"),
                };
                initial = snapshot(&h);
                block_on(f)
            }
        }
    };
    let (result, err) = match outcome {
        Ok(v) => (v, V::Null),
        Err(e) => {
            let v = object(vec![
                ("name", text(e.name)),
                ("message", raw(&e.value, "message")),
            ]);
            let response = raw(&e.value, "response");
            if !matches!(response, V::Undefined) {
                set(&v, "response", response)
            }
            (V::Undefined, v)
        }
    };
    let state = h.borrow();
    object(vec![
        ("result", result),
        ("error", err),
        ("requests", array(state.requests.clone())),
        ("updates", array(state.updates.clone())),
        ("logs", array(state.logs.clone())),
        ("reported", array(state.reported.clone())),
        ("calls", array(state.calls.clone())),
        ("sleeps", array(state.sleeps.clone())),
        ("registrations", array(state.registrations.clone())),
        ("creates", array(state.creates.clone())),
        ("initial", initial),
    ])
}
fn source_rows() -> &'static Vec<Json> {
    static ROWS: std::sync::OnceLock<Vec<Json>> = std::sync::OnceLock::new();
    ROWS.get_or_init(|| {
        let out = Command::new("node")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/runtime-oracle.cjs"))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "oracle {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let rows: Vec<Json> = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(rows.len(), 42, "runtime::fixture_floor");
        rows
    })
}
fn assert_native(row: &Json) {
    let actual = run(row);
    let expected = decode(&row["expected"]);
    let id = row["id"].as_str().unwrap();
    if id == "constructor_fields" {
        let V::Object(o) = raw(&actual, "result") else {
            panic!("constructor")
        };
        assert_eq!(
            o.borrow().len(),
            56,
            "runtime::constructor_native_field_floor"
        );
    }
    if id == "range_short" {
        let V::Array(a) = raw(&actual, "result") else {
            panic!("range")
        };
        assert_eq!(a.borrow().len(), 1, "runtime::range_native_record_floor");
    }
    assert_eq!(actual, expected, "runtime::native {id}");
}
#[test]
fn runtime_native_source_floor() {
    for row in source_rows() {
        assert_native(row)
    }
}
fn control_fixture(id: &str) {
    let rows: Vec<_> = source_rows().iter().filter(|r| r["id"] == id).collect();
    assert_eq!(rows.len(), 1, "runtime::control_fixture_floor");
    assert_native(rows[0]);
}
#[test]
fn runtime_serialize() {
    control_fixture("serialize_repeat")
}
#[test]
fn runtime_api() {
    control_fixture("api_timeout")
}
#[test]
fn runtime_constructor() {
    control_fixture("constructor_fields")
}
#[test]
fn runtime_validation() {
    control_fixture("validate_disabled")
}
#[test]
fn runtime_account() {
    control_fixture("account_hash")
}
#[test]
fn runtime_start() {
    control_fixture("start_public")
}
#[test]
fn runtime_tick() {
    control_fixture("tick_success")
}
#[test]
fn runtime_income_page() {
    control_fixture("income_symbol")
}
#[test]
fn runtime_income_range() {
    control_fixture("range_short")
}

#[test]
fn replay_signing_uses_already_serialized_body_bytes() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/replay-signing-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Json> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| row["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "object",
            "serialized",
            "escaped",
            "plain",
            "empty",
            "null",
            "false",
            "zero"
        ]
    );
    let h = Rc::new(RefCell::new(State {
        input: V::Undefined,
        instance: object(vec![]),
        store: object(vec![]),
        disposed: false,
        requests: vec![],
        updates: vec![],
        logs: vec![],
        reported: vec![],
        calls: vec![],
        sleeps: vec![],
        registrations: vec![],
        creates: vec![],
        callbacks: vec![],
        page: 0,
    }));
    let options = object(vec![
        ("key", text("k")),
        ("secret", text("s")),
        ("passphrase", text("p")),
    ]);
    for row in rows {
        let data = from_json(&row["data"]);
        let config = object(vec![
            ("url", text("/private")),
            ("method", text("post")),
            ("data", data.clone()),
        ]);
        let result = a::run_api_callback(
            &h,
            api_rt(),
            a::WeexApiCallback::Request {
                options: options.clone(),
            },
            config,
        )
        .unwrap();
        assert_eq!(
            raw(&result, "data"),
            data,
            "replay_signing::body_identity {}",
            row["id"]
        );
        assert_eq!(
            raw(&raw(&result, "headers"), "ACCESS-SIGN"),
            from_json(&row["signature"]),
            "replay_signing::signature {}",
            row["id"]
        );
    }
}
