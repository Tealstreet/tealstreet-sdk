use safe_cex_venue_orderly::{
    stream_reports::{self as reports, ReportHost},
    ws_controls::{self as ws, WsHost},
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    sync::OnceLock,
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
        Raw::Function(_) => panic!("outside fixture coercion floor"),
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
        Raw::Function(_) => panic!("no callback encoding assertion"),
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
        Raw::Array(a) => Raw::Array(RefCell::new(a.borrow().iter().map(snapshot).collect())),
        Raw::Object(o) => Raw::Object(RefCell::new(
            o.borrow()
                .iter()
                .map(|(k, v)| (k.clone(), snapshot(v)))
                .collect(),
        )),
    })
}
struct Host {
    cfg: V,
    receiver: V,
    parent: V,
    store: V,
    emitter: V,
    mapped_orders: V,
    original: E,
    events: RefCell<Vec<V>>,
    appended: Cell<bool>,
}
impl Host {
    fn effect(&self, kind: &str, fields: Vec<(&str, V)>) -> Result<(), E> {
        let mut all = vec![("kind", text(kind))];
        all.extend(fields.into_iter().map(|(k, v)| (k, snapshot(&v))));
        self.events.borrow_mut().push(object(all));
        if string(&get(&self.cfg, "fail")) == units(kind) {
            Err(self.original.clone())
        } else {
            Ok(())
        }
    }
    fn mutate(&self) {
        if flag(&self.cfg, "append") && !self.appended.replace(true) {
            let data = get(&self.cfg, "data");
            if let Raw::Array(a) = &*data {
                let first = a.borrow()[0].clone();
                a.borrow_mut().push(first)
            } else {
                let positions = get(&data, "positions");
                if let Raw::Array(a) = &*positions {
                    a.borrow_mut().push(object(vec![("id", text("appended"))]))
                }
            }
        }
    }
    fn raw_identity(&self, v: &V) -> bool {
        let data = get(&self.cfg, "data");
        if Rc::ptr_eq(v, &data) {
            return true;
        }
        if let Raw::Array(a) = &*data {
            return a.borrow().iter().any(|r| Rc::ptr_eq(r, v));
        }
        let positions = get(&data, "positions");
        if let Raw::Array(a) = &*positions {
            return a.borrow().iter().any(|r| Rc::ptr_eq(r, v));
        }
        Rc::ptr_eq(v, &get(&get(&self.cfg, "parsed"), "data"))
    }
    fn run(&self, name: &str, data: V) -> Result<(), E> {
        let r = self.receiver.clone();
        match name {
            "handleTickersStreamEvents" => reports::tickers(self, r, data),
            "handleBBOStreamEvents" => reports::bbo(self, r, data),
            "handleMarkPricesStreamEvents" => reports::mark_prices(self, r, data),
            "handleFundingRateStreamEvents" => reports::funding_rates(self, r, data),
            "onMessage" => reports::private_message(self, r, data),
            "handlePositionReport" => reports::positions(self, r, data),
            "handleExecutionReport" => reports::execution(self, r, data),
            "handleAlgoExecutionReport" => reports::algo_execution(self, r, data),
            _ => panic!("outside report floor"),
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
    fn stringify(&self, payload: V) -> Result<V, E> {
        self.effect(
            "encode",
            vec![
                ("payload", payload.clone()),
                (
                    "keys",
                    array(
                        ordered(&payload)
                            .iter()
                            .map(|(k, _)| self.text(k))
                            .collect(),
                    ),
                ),
            ],
        )?;
        Ok(payload)
    }
    fn parse(&self, data: V) -> Result<V, E> {
        self.effect(
            "parse",
            vec![(
                "identity",
                boolean(Rc::ptr_eq(&data, &get(&self.cfg, "wire"))),
            )],
        )?;
        Ok(get(&self.cfg, "parsed"))
    }
    fn includes(&self, _: &V, _: &[u16]) -> Result<bool, E> {
        panic!("outside report floor")
    }
    fn own_values(&self, _: V) -> Result<Vec<V>, E> {
        panic!("outside report floor")
    }
    fn flatten_one(&self, _: Vec<V>) -> Result<Vec<V>, E> {
        panic!("outside report floor")
    }
    fn entries(&self, v: V) -> Result<Vec<(Vec<u16>, V)>, E> {
        Ok(ordered(&v))
    }
    fn iterator(&self, v: V) -> Result<Self::Iterator, E> {
        assert!(matches!(&*v, Raw::Array(_)), "native fixture iterator");
        Ok((v, 0))
    }
    fn next(&self, i: &mut Self::Iterator) -> Result<Option<V>, E> {
        let Raw::Array(a) = &*i.0 else { panic!() };
        let v = a.borrow().get(i.1).cloned();
        i.1 += 1;
        assert!(i.1 <= 12, "finite native live iterator floor");
        Ok(v)
    }
    fn call(&self, f: V, receiver: Option<V>, args: Vec<V>) -> Result<V, E> {
        let Raw::Function(name) = &*f else {
            panic!("native original callable")
        };
        let is = |v: &V| receiver.as_ref().is_some_and(|r| Rc::ptr_eq(r, v));
        match *name {
            "updateTicker" => {
                let tickers = get(&self.cfg, "tickers");
                let identity = ordered(&tickers)
                    .iter()
                    .any(|(_, t)| Rc::ptr_eq(t, &args[0]));
                self.effect(
                    "updateTicker",
                    vec![
                        ("store", boolean(is(&self.store))),
                        ("identity", boolean(identity)),
                        ("patch", args[1].clone()),
                        (
                            "keys",
                            array(
                                ordered(&args[1])
                                    .iter()
                                    .map(|(k, _)| self.text(k))
                                    .collect(),
                            ),
                        ),
                    ],
                )?;
                for (k, v) in ordered(&args[1]) {
                    self.set(&args[0], &k, v)?;
                }
                self.mutate();
            }
            "mapPosition" | "mapLimitOrder" | "mapAlgoOrder" => {
                self.effect(
                    name,
                    vec![
                        ("parent", boolean(is(&self.parent))),
                        ("identity", boolean(self.raw_identity(&args[0]))),
                        ("raw", args[0].clone()),
                    ],
                )?;
                if *name == "mapLimitOrder" {
                    let status = get(&self.cfg, "mutateStatus");
                    if self.truthy(&status) {
                        set(&args[0], "status", status)
                    }
                } else {
                    self.mutate();
                }
                return Ok(if *name == "mapAlgoOrder" {
                    self.mapped_orders.clone()
                } else {
                    get(&self.cfg, "mapped")
                });
            }
            "addOrUpdatePosition" => self.effect(
                "addPosition",
                vec![
                    ("store", boolean(is(&self.store))),
                    (
                        "identity",
                        boolean(Rc::ptr_eq(&args[0], &get(&self.cfg, "mapped"))),
                    ),
                    ("position", args[0].clone()),
                ],
            )?,
            "addOrUpdateOrder" => {
                let identity = Rc::ptr_eq(&args[0], &get(&self.cfg, "mapped"))
                    || items(&self.mapped_orders)
                        .iter()
                        .any(|v| Rc::ptr_eq(v, &args[0]));
                self.effect(
                    "addOrder",
                    vec![
                        ("store", boolean(is(&self.store))),
                        ("identity", boolean(identity)),
                        ("order", args[0].clone()),
                    ],
                )?
            }
            "updateOrderStatus" => self.effect(
                "status",
                vec![
                    ("store", boolean(is(&self.store))),
                    ("selector", args[0].clone()),
                    ("status", args[1].clone()),
                ],
            )?,
            "emit" => self.effect(
                "fill",
                vec![
                    ("emitter", boolean(is(&self.emitter))),
                    ("name", args[0].clone()),
                    ("payload", args[1].clone()),
                    (
                        "keys",
                        array(
                            ordered(&args[1])
                                .iter()
                                .map(|(k, _)| self.text(k))
                                .collect(),
                        ),
                    ),
                ],
            )?,
            "trackLastReceived" => {
                assert!(is(&self.receiver), "native socket receiver");
                self.effect("received", vec![])?
            }
            "emitWsDiagnosticReceive" => {
                assert!(is(&self.receiver), "native socket receiver");
                self.effect(
                    "diagnostic",
                    vec![(
                        "identity",
                        boolean(Rc::ptr_eq(&args[0], &get(&self.cfg, "wire"))),
                    )],
                )?
            }
            "sendWithLog" => {
                assert!(is(&self.receiver), "native socket receiver");
                self.effect("send", vec![("payload", args[0].clone())])?
            }
            "handlePongEvent" => {
                assert!(is(&self.receiver), "native socket receiver");
                self.effect("pong", vec![])?
            }
            "subscribe" => {
                assert!(is(&self.receiver));
                ws::private_subscribe(self, self.receiver.clone())?
            }
            "ping" => {
                assert!(is(&self.receiver));
                ws::private_ping(self, self.receiver.clone())?
            }
            "handlePositionReport" | "handleExecutionReport" | "handleAlgoExecutionReport" => {
                assert!(is(&self.receiver), "native socket receiver");
                self.effect(
                    "route",
                    vec![
                        ("method", text(name)),
                        (
                            "identity",
                            boolean(Rc::ptr_eq(
                                &args[0],
                                &get(&get(&self.cfg, "parsed"), "data"),
                            )),
                        ),
                    ],
                )?;
                if flag(&self.cfg, "integrate") {
                    self.run(name, args[0].clone())?
                }
            }
            _ => panic!("outside report callable floor {name}"),
        };
        Ok(Rc::new(Raw::Undefined))
    }
    fn monotonic_now(&self) -> Result<V, E> {
        self.effect("clock", vec![])?;
        Ok(Rc::new(Raw::Number(19.25)))
    }
    fn virtual_now(&self) -> Result<V, E> {
        panic!("outside report floor")
    }
    fn sign(&self, _: V, _: V) -> Result<V, E> {
        panic!("outside report floor")
    }
    fn reconnecting(&self) -> bool {
        false
    }
}
impl ReportHost for Host {
    fn ticker_patch_refusal(&self, _: &E, _: &[u16], _: &[(&str, V)]) -> Result<Option<String>, E> {
        Ok(None)
    }
    fn ticker_refusal_error(&self, _: Vec<String>) -> E {
        panic!("source-only Store accepts raw ticker values")
    }
    fn nullish(&self, v: &Raw) -> bool {
        matches!(v, Raw::Undefined | Raw::Null)
    }
    fn undefined(&self) -> V {
        Rc::new(Raw::Undefined)
    }
    fn text_eq(&self, v: &Raw, s: &str) -> bool {
        matches!(v,Raw::Text(t) if *t==units(s))
    }
    fn is_array(&self, v: &Raw) -> bool {
        matches!(v, Raw::Array(_))
    }
    fn array(&self, a: Vec<V>) -> V {
        array(a)
    }
    fn for_each(&self, value: V, mut callback: impl FnMut(V) -> Result<(), E>) -> Result<(), E> {
        let Raw::Array(a) = &*value else {
            panic!("array forEach floor")
        };
        let length = a.borrow().len();
        assert!(length <= 12, "finite native forEach floor");
        for i in 0..length {
            let value = a.borrow().get(i).cloned();
            if let Some(v) = value {
                callback(v)?
            }
        }
        Ok(())
    }
    fn starts_with(&self, v: V, prefix: &str) -> Result<bool, E> {
        let Raw::Text(s) = &*v else {
            panic!("source startsWith binding")
        };
        Ok(s.starts_with(&units(prefix)))
    }
    fn normalize_symbol(&self, v: V) -> Result<Vec<u16>, E> {
        let Raw::Text(s) = &*v else {
            panic!("source normalizeSymbol binding")
        };
        let parts = s.split(|c| *c == b'_' as u16).collect::<Vec<_>>();
        Ok([
            parts.get(1).copied().unwrap_or(&units("undefined")),
            parts.get(2).copied().unwrap_or(&units("undefined")),
        ]
        .concat())
    }
    fn get_key(&self, v: V, key: &str) -> Result<V, E> {
        if self.nullish(&v) {
            return Ok(self.undefined());
        }
        let exact = get(&v, key);
        if !matches!(&*exact, Raw::Undefined) || ordered(&v).iter().any(|(k, _)| *k == units(key)) {
            return Ok(exact);
        }
        let mut snake = String::new();
        for ch in key.chars() {
            if ch.is_ascii_uppercase() {
                snake.push('_');
                snake.push(ch.to_ascii_lowercase())
            } else {
                snake.push(ch)
            }
        }
        Ok(get(&v, &snake))
    }
    fn parse_float(&self, v: V) -> Result<V, E> {
        Ok(Rc::new(Raw::Number(safe_cex_num::parse_float(
            &String::from_utf16(&string(&v)).unwrap(),
        ))))
    }
    fn order_side(&self, v: V) -> Result<V, E> {
        Ok(match &*v {
            Raw::Text(s) if *s == units("BUY") => text("buy"),
            Raw::Text(s) if *s == units("SELL") => text("sell"),
            _ => self.undefined(),
        })
    }
}
fn setup(cfg: &Value) -> Host {
    let cfg = decode(cfg);
    set(&cfg, "wire", text("original wire"));
    if flag(&cfg, "removeCamel") {
        for row in items(&get(&cfg, "data")) {
            let Raw::Object(o) = &*row else { panic!() };
            o.borrow_mut()
                .retain(|(k, _)| *k != units("fundingRate") && *k != units("fundingTs"));
        }
    }
    let mapped = get(&cfg, "mapped");
    let mapped_orders = get(&cfg, "mappedOrders");
    let mapped_orders = if matches!(&*mapped_orders, Raw::Null | Raw::Undefined) {
        let second = if matches!(&*mapped, Raw::Object(_)) {
            Rc::new(Raw::Object(RefCell::new(ordered(&mapped))))
        } else {
            object(vec![])
        };
        set(&second, "id", text("SECOND"));
        array(vec![mapped, second])
    } else {
        mapped_orders
    };
    let store = object(vec![]);
    for name in [
        "updateTicker",
        "addOrUpdatePosition",
        "addOrUpdateOrder",
        "updateOrderStatus",
    ] {
        set(&store, name, Rc::new(Raw::Function(name)))
    }
    let emitter = object(vec![("emit", Rc::new(Raw::Function("emit")))]);
    let parent = object(vec![
        (
            "store",
            object(vec![("tickersBySymbol", get(&cfg, "tickers"))]),
        ),
        ("emitter", emitter.clone()),
    ]);
    for name in ["mapPosition", "mapLimitOrder", "mapAlgoOrder"] {
        set(&parent, name, Rc::new(Raw::Function(name)))
    }
    let receiver = object(vec![
        ("parent", parent.clone()),
        ("store", store.clone()),
        ("isDisposed", boolean(flag(&cfg, "disposed"))),
    ]);
    for name in [
        "trackLastReceived",
        "emitWsDiagnosticReceive",
        "sendWithLog",
        "handlePongEvent",
        "subscribe",
        "ping",
        "handlePositionReport",
        "handleExecutionReport",
        "handleAlgoExecutionReport",
    ] {
        set(&receiver, name, Rc::new(Raw::Function(name)))
    }
    Host {
        cfg,
        receiver,
        parent,
        store,
        emitter,
        mapped_orders,
        original: Rc::new("controlled failure"),
        events: RefCell::new(vec![]),
        appended: Cell::new(false),
    }
}
fn oracle() -> &'static Value {
    static OUT: OnceLock<Value> = OnceLock::new();
    OUT.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/stream-reports-oracle.cjs"
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
            6,
            "independent source file floor"
        );
        value
    })
}
fn check(name: &str, floor: usize) {
    let rows = oracle()["rows"][name].as_array().unwrap();
    assert_eq!(rows.len(), floor, "independent report case floor");
    for (i, row) in rows.iter().enumerate() {
        let h = setup(&row["cfg"]);
        let arg = if name == "onMessage" {
            object(vec![("data", get(&h.cfg, "wire"))])
        } else {
            get(&h.cfg, "data")
        };
        let result = h.run(name, arg);
        if let Err(e) = result {
            assert!(
                Rc::ptr_eq(&e, &h.original),
                "native original error allocation"
            );
            assert_eq!(row["error"]["identity"], true);
            assert_eq!(row["error"]["message"], *e)
        } else {
            assert!(row["error"].is_null(), "source error {row}")
        };
        native(
            &Raw::Array(RefCell::new(h.events.borrow().clone())),
            &row["events"],
            &format!("{name}[{i}] effects"),
        );
        native(&get(&h.cfg, "tickers"), &row["tickers"], "store tickers");
    }
}
#[test]
fn tickers_native() {
    check("handleTickersStreamEvents", 8)
}
#[test]
fn bbo_native() {
    check("handleBBOStreamEvents", 8)
}
#[test]
fn mark_native() {
    check("handleMarkPricesStreamEvents", 8)
}
#[test]
fn funding_native() {
    check("handleFundingRateStreamEvents", 8)
}
#[test]
fn private_message_native() {
    check("onMessage", 16)
}
#[test]
fn positions_native() {
    check("handlePositionReport", 8)
}
#[test]
fn execution_native() {
    check("handleExecutionReport", 15)
}
#[test]
fn algo_native() {
    check("handleAlgoExecutionReport", 15)
}
