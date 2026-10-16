use safe_cex_venue_orderly::{
    account_order_reads::{self as reads, AccountReadHost},
    ws_controls::WsHost,
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    sync::OnceLock,
};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};
#[derive(Debug)]
enum Raw {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Array(RefCell<Vec<Rc<Raw>>>),
    Object(RefCell<Vec<(Vec<u16>, Rc<Raw>)>>),
    Function(&'static str),
    Promise(Promise),
}
type V = Rc<Raw>;
type E = Rc<&'static str>;
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text(s: &str) -> V {
    Rc::new(Raw::Text(units(s)))
}
fn array(a: Vec<V>) -> V {
    Rc::new(Raw::Array(RefCell::new(a)))
}
fn object(a: Vec<(&str, V)>) -> V {
    Rc::new(Raw::Object(RefCell::new(
        a.into_iter().map(|(k, v)| (units(k), v)).collect(),
    )))
}
fn decode(v: &Value) -> V {
    Rc::new(match v {
        Value::Null => Raw::Null,
        Value::Bool(b) => Raw::Bool(*b),
        Value::Number(n) => Raw::Number(n.as_f64().unwrap()),
        Value::String(s) => Raw::Text(units(s)),
        Value::Array(a) => Raw::Array(RefCell::new(a.iter().map(decode).collect())),
        Value::Object(o) => {
            if o.get("undefined") == Some(&Value::Bool(true)) {
                Raw::Undefined
            } else if let Some(n) = o.get("number") {
                Raw::Number(f64::from_bits(
                    u64::from_str_radix(n.as_str().unwrap(), 16).unwrap(),
                ))
            } else {
                Raw::Object(RefCell::new(
                    o.iter().map(|(k, v)| (units(k), decode(v))).collect(),
                ))
            }
        }
    })
}
fn get(v: &Raw, key: &str) -> V {
    if let Raw::Object(o) = v {
        o.borrow()
            .iter()
            .find(|(k, _)| *k == units(key))
            .map(|(_, v)| v.clone())
            .unwrap_or(Rc::new(Raw::Undefined))
    } else {
        Rc::new(Raw::Undefined)
    }
}
fn set(v: &V, key: &str, next: V) {
    let Raw::Object(o) = &**v else {
        panic!("ordinary fixture object")
    };
    let mut o = o.borrow_mut();
    if let Some((_, value)) = o.iter_mut().find(|(k, _)| *k == units(key)) {
        *value = next
    } else {
        o.push((units(key), next))
    }
}
fn boolean(b: bool) -> V {
    Rc::new(Raw::Bool(b))
}
fn flag(v: &Raw, key: &str) -> bool {
    matches!(&*get(v, key), Raw::Bool(true))
}
fn items(v: &Raw) -> Vec<V> {
    let Raw::Array(a) = v else {
        panic!("fixture array")
    };
    a.borrow().clone()
}
fn string(v: &Raw) -> Vec<u16> {
    match v {
        Raw::Undefined => units("undefined"),
        Raw::Null => units("null"),
        Raw::Bool(b) => units(if *b { "true" } else { "false" }),
        Raw::Number(n) => units(&safe_cex_num::js_number_string(*n)),
        Raw::Text(s) => s.clone(),
        Raw::Object(_) => units("[object Object]"),
        Raw::Array(a) => a
            .borrow()
            .iter()
            .map(|v| String::from_utf16(&string(v)).unwrap())
            .collect::<Vec<_>>()
            .join(",")
            .encode_utf16()
            .collect(),
        Raw::Function(_) | Raw::Promise(_) => panic!("outside fixture coercion floor"),
    }
}
fn ordered(v: &Raw) -> Vec<(Vec<u16>, V)> {
    let Raw::Object(o) = v else {
        panic!("fixture own entries")
    };
    let mut a = o.borrow().clone();
    a.sort_by_key(|(k, _)| {
        let s = String::from_utf16(k).unwrap();
        let n = s
            .parse::<u32>()
            .ok()
            .filter(|n| *n != u32::MAX && n.to_string() == s);
        (n.is_none(), n.unwrap_or(0))
    });
    a
}
fn native(v: &Raw, expected: &Value, path: &str) {
    match v {
        Raw::Undefined => assert_eq!(
            expected.get("undefined"),
            Some(&Value::Bool(true)),
            "native undefined {path}"
        ),
        Raw::Null => assert!(expected.is_null(), "native null {path}"),
        Raw::Bool(b) => assert_eq!(expected.as_bool(), Some(*b), "native boolean {path}"),
        Raw::Number(n) => {
            let bits = u64::from_str_radix(expected["number"].as_str().unwrap(), 16).unwrap();
            if n.is_nan() {
                assert!(f64::from_bits(bits).is_nan(), "native NaN {path}")
            } else {
                assert_eq!(n.to_bits(), bits, "native bits {path}")
            }
        }
        Raw::Text(s) => {
            assert!(expected.is_string(), "native text kind {path}");
            assert_eq!(
                s,
                &units(expected.as_str().unwrap()),
                "native UTF-16 {path}"
            );
        }
        Raw::Array(a) => {
            assert!(expected.is_array(), "native array kind {path}");
            let a = a.borrow();
            let e = expected.as_array().unwrap();
            assert_eq!(a.len(), e.len(), "native array cardinality {path}");
            for (i, (a, e)) in a.iter().zip(e).enumerate() {
                native(a, e, &format!("{path}[{i}]"))
            }
        }
        Raw::Object(o) => {
            assert!(expected.is_object(), "native object kind {path}");
            let o = o.borrow();
            let e = expected.as_object().unwrap();
            assert_eq!(o.len(), e.len(), "native object cardinality {path}");
            for (k, v) in o.iter() {
                let k = String::from_utf16(k).unwrap();
                assert!(e.contains_key(&k), "native object key {path}.{k}");
                native(v, &e[&k], &format!("{path}.{k}"))
            }
        }
        Raw::Function(_) | Raw::Promise(_) => panic!("no callback encoding assertion"),
    }
}
fn snapshot(value: &V) -> V {
    Rc::new(match &**value {
        Raw::Undefined => Raw::Undefined,
        Raw::Null => Raw::Null,
        Raw::Bool(b) => Raw::Bool(*b),
        Raw::Number(n) => Raw::Number(*n),
        Raw::Text(s) => Raw::Text(s.clone()),
        Raw::Function(f) => Raw::Function(f),
        Raw::Promise(_) => panic!("no promise trace snapshot"),
        Raw::Array(a) => Raw::Array(RefCell::new(a.borrow().iter().map(snapshot).collect())),
        Raw::Object(o) => Raw::Object(RefCell::new(
            o.borrow()
                .iter()
                .map(|(k, v)| (k.clone(), snapshot(v)))
                .collect(),
        )),
    })
}
#[derive(Clone, Copy, Debug)]
enum Promise {
    Request(&'static str),
    Limit,
    Algo,
    IntegratedLimit,
    IntegratedAlgo,
}
struct Host {
    cfg: V,
    receiver: V,
    xhr: V,
    map: V,
    original: E,
    events: RefCell<Vec<V>>,
    cache: RefCell<Vec<V>>,
    appended: Cell<bool>,
}
impl Host {
    fn effect(&self, kind: &str, fields: Vec<(&str, V)>) -> Result<(), E> {
        let mut fields = vec![("kind", text(kind))]
            .into_iter()
            .chain(fields.into_iter().map(|(k, v)| (k, snapshot(&v))))
            .collect::<Vec<_>>();
        self.events
            .borrow_mut()
            .push(object(std::mem::take(&mut fields)));
        if string(&get(&self.cfg, "fail")) == units(kind) {
            Err(self.original.clone())
        } else {
            Ok(())
        }
    }
    fn apply(&self) {
        let changes = get(&self.cfg, "afterAwait");
        if matches!(&*changes, Raw::Object(_)) {
            for (k, v) in ordered(&changes) {
                let name = String::from_utf16(&k).unwrap();
                let v = if string(&v) == units("alternate") {
                    Rc::new(Raw::Function(if name == "mapLimitOrder" {
                        "mapLimitAlt"
                    } else {
                        "mapAlgoAlt"
                    }))
                } else {
                    v
                };
                set(&self.receiver, &name, v)
            }
        }
    }
    fn append(&self) {
        if flag(&self.cfg, "append") && !self.appended.replace(true) {
            let rows = get(&self.cfg, "rows");
            let Raw::Array(a) = &*rows else { panic!() };
            let first = a.borrow()[0].clone();
            a.borrow_mut().push(first)
        }
    }
    fn run<'a>(&'a self, name: &str) -> Pin<Box<dyn Future<Output = Result<V, E>> + 'a>> {
        let r = self.receiver.clone();
        match name {
            "getPositionMode" => Box::pin(reads::position_mode(self, r)),
            "getAccount" => Box::pin(reads::account(self, r)),
            "validateAccount" => Box::pin(reads::validate(self, r)),
            "fetchAndUpdateAccountLeverage" => Box::pin(reads::leverage(self, r)),
            "fetchAndUpdateMarginTypes" => Box::pin(reads::margin_types(self, r)),
            "fetchLimitOrders" => Box::pin(reads::limit_orders(self, r)),
            "fetchAlgoOrders" => Box::pin(reads::algo_orders(self, r)),
            "fetchOrders" => Box::pin(reads::orders(self, r)),
            _ => panic!("outside account-read floor"),
        }
    }
}
impl WsHost for Host {
    type Value = Raw;
    type Error = E;
    type Iterator = (V, usize);
    fn get(&self, v: &V, key: &[u16]) -> Result<V, E> {
        Ok(get(v, &String::from_utf16(key).unwrap()))
    }
    fn set(&self, v: &V, key: &[u16], value: V) -> Result<(), E> {
        set(v, &String::from_utf16(key).unwrap(), value);
        Ok(())
    }
    fn truthy(&self, v: &Raw) -> bool {
        match v {
            Raw::Undefined | Raw::Null => false,
            Raw::Bool(b) => *b,
            Raw::Number(n) => *n != 0.0 && !n.is_nan(),
            Raw::Text(s) => !s.is_empty(),
            _ => true,
        }
    }
    fn string(&self, v: &Raw) -> Result<Vec<u16>, E> {
        Ok(string(v))
    }
    fn text(&self, s: &[u16]) -> V {
        Rc::new(Raw::Text(s.to_vec()))
    }
    fn object(&self, entries: Vec<(&'static str, V)>) -> Result<V, E> {
        Ok(object(entries))
    }
    fn stringify(&self, _: V) -> Result<V, E> {
        panic!("outside account-read floor")
    }
    fn parse(&self, _: V) -> Result<V, E> {
        panic!("outside account-read floor")
    }
    fn includes(&self, _: &V, _: &[u16]) -> Result<bool, E> {
        panic!("outside account-read floor")
    }
    fn own_values(&self, _: V) -> Result<Vec<V>, E> {
        panic!("outside account-read floor")
    }
    fn flatten_one(&self, _: Vec<V>) -> Result<Vec<V>, E> {
        panic!("outside account-read floor")
    }
    fn entries(&self, v: V) -> Result<Vec<(Vec<u16>, V)>, E> {
        Ok(ordered(&v))
    }
    fn iterator(&self, v: V) -> Result<Self::Iterator, E> {
        assert!(matches!(&*v, Raw::Array(_)), "native array iterator floor");
        Ok((v, 0))
    }
    fn next(&self, i: &mut Self::Iterator) -> Result<Option<V>, E> {
        let Raw::Array(a) = &*i.0 else { panic!() };
        let v = a.borrow().get(i.1).cloned();
        i.1 += 1;
        assert!(i.1 <= 12, "finite live iteration floor");
        Ok(v)
    }
    fn call(&self, f: V, receiver: Option<V>, args: Vec<V>) -> Result<V, E> {
        let Raw::Function(name) = &*f else {
            panic!("original callable floor")
        };
        let is = |value: &V| receiver.as_ref().is_some_and(|r| Rc::ptr_eq(r, value));
        match *name {
            "get" => {
                self.effect(
                    "request",
                    vec![
                        ("receiver", boolean(is(&self.xhr))),
                        ("url", args[0].clone()),
                        ("config", args.get(1).cloned().unwrap_or(self.undefined())),
                        ("argc", self.number(args.len() as f64)),
                    ],
                )?;
                if flag(&self.cfg, "syncFail") {
                    return Err(self.original.clone());
                }
                let path = String::from_utf16(&string(&args[0])).unwrap();
                let path = match path.as_str() {
                    "/v1/client/info" => "/v1/client/info",
                    "/v1/client/margin_modes" => "/v1/client/margin_modes",
                    "/v1/orders" => "/v1/orders",
                    "/v1/algo/orders" => "/v1/algo/orders",
                    _ => panic!(),
                };
                return Ok(Rc::new(Raw::Promise(Promise::Request(path))));
            }
            "mapLimit" | "mapAlgo" | "mapLimitAlt" | "mapAlgoAlt" => {
                let identity = items(&get(&self.cfg, "rows"))
                    .iter()
                    .any(|row| Rc::ptr_eq(row, &args[0]));
                self.effect(
                    name,
                    vec![
                        ("receiver", boolean(is(&self.receiver))),
                        ("identity", boolean(identity)),
                        ("row", args[0].clone()),
                    ],
                )?;
                self.append();
                return Ok(get(
                    &self.cfg,
                    match *name {
                        "mapLimit" => "mapped",
                        "mapAlgo" => "mappedChildren",
                        "mapLimitAlt" => "altMapped",
                        _ => "altChildren",
                    },
                ));
            }
            "mapMarginType" => {
                self.effect(
                    "mapMargin",
                    vec![
                        ("receiver", boolean(is(&self.receiver))),
                        ("raw", args[0].clone()),
                    ],
                )?;
                return Ok(text(if self.text_eq(&args[0], "ISOLATED") {
                    "isolated"
                } else {
                    "cross"
                }));
            }
            "set" => {
                self.effect(
                    "mapSet",
                    vec![
                        ("receiver", boolean(is(&self.map))),
                        ("symbol", args[0].clone()),
                        ("value", args[1].clone()),
                    ],
                )?;
                let pair = array(args);
                let key = items(&pair)[0].clone();
                let mut cache = self.cache.borrow_mut();
                if let Some(previous) = cache
                    .iter_mut()
                    .find(|p| string(&items(p)[0]) == string(&key))
                {
                    *previous = pair;
                } else {
                    cache.push(pair)
                }
                drop(cache);
                self.append();
            }
            "emitAndLogError" => {
                let error = get(&self.cfg, "error");
                let response = get(&get(&error, "response"), "data");
                let identity = Rc::ptr_eq(&args[0], &get(&response, "message"))
                    || Rc::ptr_eq(&args[0], &get(&error, "message"));
                self.effect(
                    "log",
                    vec![
                        ("receiver", boolean(is(&self.receiver))),
                        ("message", args[0].clone()),
                        ("identity", boolean(identity)),
                    ],
                )?;
                let changes = get(&self.cfg, "afterLog");
                if matches!(&*changes, Raw::Object(_)) {
                    for (k, v) in ordered(&changes) {
                        self.set(&error, &k, v)?;
                    }
                }
            }
            "fetchLimitOrders" | "fetchAlgoOrders" => {
                let limit = *name == "fetchLimitOrders";
                self.effect(
                    if limit { "callLimit" } else { "callAlgo" },
                    vec![("receiver", boolean(is(&self.receiver)))],
                )?;
                if flag(&self.cfg, if limit { "syncLimit" } else { "syncAlgo" }) {
                    return Err(self.original.clone());
                }
                let kind = match (limit, flag(&self.cfg, "integrate")) {
                    (true, true) => Promise::IntegratedLimit,
                    (false, true) => Promise::IntegratedAlgo,
                    (true, false) => Promise::Limit,
                    (false, false) => Promise::Algo,
                };
                return Ok(Rc::new(Raw::Promise(kind)));
            }
            _ => panic!("outside account read callable floor {name}"),
        };
        Ok(self.undefined())
    }
    fn monotonic_now(&self) -> Result<V, E> {
        panic!("outside account-read floor")
    }
    fn virtual_now(&self) -> Result<V, E> {
        panic!("outside account-read floor")
    }
    fn sign(&self, _: V, _: V) -> Result<V, E> {
        panic!("outside account-read floor")
    }
    fn reconnecting(&self) -> bool {
        false
    }
}
impl AccountReadHost for Host {
    fn await_value(&self, value: V) -> impl Future<Output = Result<V, E>> {
        Box::pin(async move {
            let Raw::Promise(kind) = &*value else {
                panic!("original promise floor")
            };
            match kind {
                Promise::IntegratedLimit => reads::limit_orders(self, self.receiver.clone()).await,
                Promise::IntegratedAlgo => reads::algo_orders(self, self.receiver.clone()).await,
                Promise::Request(path) => {
                    yield_once().await;
                    self.effect("await", vec![("url", text(path))])?;
                    self.apply();
                    if flag(&self.cfg, "reject") {
                        return Err(self.original.clone());
                    }
                    let data = if *path == "/v1/client/info" {
                        get(&self.cfg, "account")
                    } else {
                        object(vec![("rows", get(&self.cfg, "rows"))])
                    };
                    Ok(object(vec![("data", object(vec![("data", data)]))]))
                }
                Promise::Limit | Promise::Algo => {
                    yield_once().await;
                    let limit = matches!(kind, Promise::Limit);
                    self.effect(if limit { "awaitLimit" } else { "awaitAlgo" }, vec![])?;
                    if flag(&self.cfg, if limit { "rejectLimit" } else { "rejectAlgo" }) {
                        return Err(self.original.clone());
                    }
                    Ok(get(
                        &self.cfg,
                        if limit { "limitValues" } else { "algoValues" },
                    ))
                }
            }
        }) as Pin<Box<dyn Future<Output = Result<V, E>> + '_>>
    }
    fn error_value(&self, error: &E) -> V {
        assert!(
            Rc::ptr_eq(error, &self.original),
            "native original error allocation on catch"
        );
        get(&self.cfg, "error")
    }
    fn nullish(&self, v: &Raw) -> bool {
        matches!(v, Raw::Undefined | Raw::Null)
    }
    fn is_null(&self, v: &Raw) -> bool {
        matches!(v, Raw::Null)
    }
    fn is_string(&self, v: &Raw) -> bool {
        matches!(v, Raw::Text(_))
    }
    fn text_eq(&self, v: &Raw, s: &str) -> bool {
        matches!(v,Raw::Text(t) if *t==units(s))
    }
    fn undefined(&self) -> V {
        Rc::new(Raw::Undefined)
    }
    fn null(&self) -> V {
        Rc::new(Raw::Null)
    }
    fn number(&self, n: f64) -> V {
        Rc::new(Raw::Number(n))
    }
    fn array(&self, a: Vec<V>) -> V {
        array(a)
    }
    fn is_array(&self, v: &Raw) -> bool {
        matches!(v, Raw::Array(_))
    }
    fn spread(&self, v: V) -> Result<Vec<V>, E> {
        Ok(items(&v))
    }
    fn reduce(
        &self,
        rows: V,
        mut acc: V,
        mut callback: impl FnMut(V, V) -> Result<V, E>,
    ) -> Result<V, E> {
        let Raw::Array(a) = &*rows else {
            panic!("ordinary reduce floor")
        };
        let length = a.borrow().len();
        assert!(length <= 12, "finite native reduce floor");
        for i in 0..length {
            let row = a.borrow().get(i).cloned();
            if let Some(row) = row {
                acc = callback(acc, row)?
            }
        }
        Ok(acc)
    }
    fn normalize_symbol(&self, v: V) -> Result<V, E> {
        let Raw::Text(s) = &*v else { panic!() };
        let parts = s.split(|c| *c == b'_' as u16).collect::<Vec<_>>();
        Ok(self.text(
            &[
                parts.get(1).copied().unwrap_or(&units("undefined")),
                parts.get(2).copied().unwrap_or(&units("undefined")),
            ]
            .concat(),
        ))
    }
    fn get_key(&self, v: V, key: &str) -> Result<V, E> {
        if self.nullish(&v) {
            return Ok(self.undefined());
        }
        let entries = ordered(&v);
        if let Some((_, value)) = entries.iter().find(|(k, _)| *k == units(key)) {
            return Ok(value.clone());
        }
        let camel = key
            .split('_')
            .enumerate()
            .map(|(i, p)| {
                if i == 0 {
                    p.to_string()
                } else {
                    let mut chars = p.chars();
                    chars
                        .next()
                        .map(|c| c.to_uppercase().to_string())
                        .unwrap_or_default()
                        + chars.as_str()
                }
            })
            .collect::<String>();
        Ok(get(&v, &camel))
    }
}
async fn yield_once() {
    let mut first = true;
    std::future::poll_fn(move |_| {
        if first {
            first = false;
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await
}
fn setup(cfg: &Value) -> Host {
    let cfg = decode(cfg);
    let xhr = object(vec![("get", Rc::new(Raw::Function("get")))]);
    let map = object(vec![("set", Rc::new(Raw::Function("set")))]);
    let receiver = object(vec![
        ("xhr", xhr.clone()),
        ("marginTypeBySymbol", map.clone()),
        ("_accountPositionMode", get(&cfg, "mode")),
        ("accountLeverage", get(&cfg, "leverage")),
        ("isDisposed", boolean(false)),
        ("mapLimitOrder", Rc::new(Raw::Function("mapLimit"))),
        ("mapAlgoOrder", Rc::new(Raw::Function("mapAlgo"))),
    ]);
    for name in [
        "mapMarginType",
        "emitAndLogError",
        "fetchLimitOrders",
        "fetchAlgoOrders",
    ] {
        set(&receiver, name, Rc::new(Raw::Function(name)))
    }
    Host {
        cfg,
        receiver,
        xhr,
        map,
        original: Rc::new("original account failure"),
        events: RefCell::new(vec![]),
        cache: RefCell::new(vec![]),
        appended: Cell::new(false),
    }
}
fn drive(h: &Host, name: &str) -> Result<V, E> {
    let mut future = h.run(name);
    let mut context = Context::from_waker(Waker::noop());
    for _ in 0..12 {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
    panic!("finite native continuation floor")
}
fn oracle() -> &'static Value {
    static OUT: OnceLock<Value> = OnceLock::new();
    OUT.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/account-order-reads-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "source oracle {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            value["hashes"].as_object().unwrap().len(),
            5,
            "independent source file floor"
        );
        value
    })
}
fn check(name: &str, floor: usize) {
    let rows = oracle()["rows"][name].as_array().unwrap();
    assert_eq!(rows.len(), floor, "independent account read floor");
    for (i, row) in rows.iter().enumerate() {
        let h = setup(&row["cfg"]);
        let mut future = h.run(name);
        let mut context = Context::from_waker(Waker::noop());
        let first = future.as_mut().poll(&mut context);
        assert_eq!(
            h.events.borrow().len(),
            row["prefix"].as_u64().unwrap() as usize,
            "native initial synchronous prefix"
        );
        let mut result = match first {
            Poll::Ready(v) => v,
            Poll::Pending => {
                let mut result = None;
                for _ in 0..12 {
                    if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
                        result = Some(value);
                        break;
                    }
                }
                result.expect("finite native continuation floor")
            }
        };
        drop(future);
        if result.is_ok() && flag(&h.cfg, "repeat") {
            result = drive(&h, name);
        }
        match result {
            Ok(value) => {
                assert!(row["error"].is_null(), "source error {row}");
                let identity = match name {
                    "getPositionMode" => {
                        Rc::ptr_eq(&value, &get(&h.receiver, "_accountPositionMode"))
                    }
                    "getAccount" => {
                        let id = get(&value, "userId");
                        let original = get(&get(&h.cfg, "account"), "account_id");
                        Rc::ptr_eq(&id, &original)
                            || matches!((&*id, &*original), (Raw::Undefined, Raw::Undefined))
                    }
                    "fetchLimitOrders" => items(&value).iter().all(|v| {
                        Rc::ptr_eq(v, &get(&h.cfg, "mapped"))
                            || Rc::ptr_eq(v, &get(&h.cfg, "altMapped"))
                    }),
                    "fetchAlgoOrders" => items(&value).iter().all(|v| {
                        items(&get(&h.cfg, "mappedChildren"))
                            .iter()
                            .chain(items(&get(&h.cfg, "altChildren")).iter())
                            .any(|o| Rc::ptr_eq(v, o))
                    }),
                    _ => true,
                };
                assert_eq!(
                    identity,
                    row["outputIdentity"].as_bool().unwrap(),
                    "native original output handle"
                );
                native(&value, &row["result"], &format!("{name}[{i}] result"));
            }
            Err(error) => {
                assert!(
                    Rc::ptr_eq(&error, &h.original),
                    "native original rejection allocation"
                );
                assert_eq!(row["error"]["identity"], true);
                native(
                    &get(&h.error_value(&error), "message"),
                    &row["error"]["message"],
                    "original error metadata",
                );
            }
        }
        native(
            &Raw::Array(RefCell::new(h.events.borrow().clone())),
            &row["events"],
            &format!("{name}[{i}] effects"),
        );
        let state = object(vec![
            ("mode", get(&h.receiver, "_accountPositionMode")),
            ("leverage", get(&h.receiver, "accountLeverage")),
            ("disposed", get(&h.receiver, "isDisposed")),
            ("cache", array(h.cache.borrow().clone())),
        ]);
        native(&state, &row["state"], "native final account state");
    }
}
#[test]
fn position_mode_native() {
    check("getPositionMode", 8)
}
#[test]
fn account_native() {
    check("getAccount", 8)
}
#[test]
fn validation_native() {
    check("validateAccount", 8)
}
#[test]
fn leverage_native() {
    check("fetchAndUpdateAccountLeverage", 9)
}
#[test]
fn margin_types_native() {
    check("fetchAndUpdateMarginTypes", 8)
}
#[test]
fn limit_orders_native() {
    check("fetchLimitOrders", 12)
}
#[test]
fn algo_orders_native() {
    check("fetchAlgoOrders", 9)
}
#[test]
fn orders_native() {
    check("fetchOrders", 9)
}
