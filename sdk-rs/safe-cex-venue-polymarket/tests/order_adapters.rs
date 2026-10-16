use safe_cex_venue_polymarket::{order_adapters::*, ws_dispatch::Runtime};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
#[derive(Clone, Debug)]
enum V {
    U,
    Text(Vec<u16>),
    Number(f64),
    Symbol,
    Proto,
    Object(Rc<Object>),
    Array(Array),
}
#[derive(Debug)]
struct Object {
    marker: u32,
    fields: RefCell<BTreeMap<&'static str, V>>,
    reads: RefCell<BTreeMap<String, usize>>,
}
type Array = Rc<RefCell<Vec<Option<V>>>>;
#[derive(Debug)]
struct Failure {
    marker: Option<u32>,
    name: &'static str,
    message: Vec<u16>,
}
type E = Rc<Failure>;
#[derive(Debug, Clone)]
struct Event {
    tag: String,
    values: Vec<V>,
    numbers: Vec<f64>,
    text: Option<Vec<u16>>,
    severity: Option<&'static str>,
}
impl Event {
    fn new(tag: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            values: vec![],
            numbers: vec![],
            text: None,
            severity: None,
        }
    }
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text(s: &str) -> V {
    V::Text(u(s))
}
fn obj(marker: u32, fields: Vec<(&'static str, V)>) -> V {
    V::Object(Rc::new(Object {
        marker,
        fields: RefCell::new(fields.into_iter().collect()),
        reads: RefCell::new(BTreeMap::new()),
    }))
}
fn array(rows: Vec<Option<V>>) -> Array {
    Rc::new(RefCell::new(rows))
}
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap() as u16)
        .collect()
}
struct Op {
    ready: Cell<bool>,
    result: RefCell<Result<V, E>>,
    polls: Cell<usize>,
}
impl Completion<V, E> for Op {
    fn poll(&self, _: &mut Context<'_>) -> Poll<Result<V, E>> {
        self.polls.set(self.polls.get() + 1);
        if self.ready.get() {
            Poll::Ready(self.result.borrow().clone())
        } else {
            Poll::Pending
        }
    }
}
struct Host {
    venue: &'static str,
    name: &'static str,
    i: usize,
    events: RefCell<Vec<Event>>,
    error: E,
    type_error: E,
    input: V,
    orders: Array,
    bids: Array,
    op: Rc<Op>,
    tasks: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
    stored: RefCell<Option<Rc<RefCell<NativeOrder<V>>>>>,
    fill: RefCell<Option<NativeFill<V>>>,
    callback: Rc<u32>,
    cleanup: Rc<Cell<usize>>,
}
impl Host {
    fn new(venue: &'static str, name: &'static str, i: usize) -> Self {
        fn input(i: usize, marker: u32, name: &str) -> V {
            obj(
                marker,
                vec![
                    (
                        "id",
                        V::Text(
                            u(&format!("id-{marker}"))
                                .into_iter()
                                .chain([0xdfff])
                                .collect(),
                        ),
                    ),
                    ("asset_id", text(&format!("asset-{marker}"))),
                    (
                        "status",
                        text(match i {
                            2 => "unknown",
                            3 => "constructor",
                            4 => "MATCHED",
                            6 if name == "handleOrderUpdate" => "matched",
                            8 => "matched",
                            _ => "live",
                        }),
                    ),
                    (
                        "order_type",
                        text(match i {
                            2 => "unknown",
                            3 => "constructor",
                            _ => "GTC",
                        }),
                    ),
                    (
                        "side",
                        text(match i {
                            2 => "unknown",
                            3 => "constructor",
                            _ => "BUY",
                        }),
                    ),
                    (
                        "price",
                        match i {
                            4 => text("-0"),
                            7 => text("1e-4000"),
                            10 => V::Number(0.5),
                            6 if name == "handleOrderUpdate" => text("0.125tail"),
                            6 | 11 => V::Symbol,
                            _ => text("0.125tail"),
                        },
                    ),
                    ("original_size", text(if i == 8 { "8tail" } else { "3" })),
                    ("size_matched", text("1")),
                ],
            )
        }
        let one = input(i, 71, name);
        let two = input(i, 73, name);
        let orders = array(if i == 2 {
            vec![]
        } else if i == 9 {
            vec![Some(one.clone()), None, Some(two)]
        } else {
            vec![Some(one.clone()), Some(two)]
        });
        fn level(price: V, size: &str) -> V {
            obj(0, vec![("price", price), ("size", text(size))])
        }
        let bids = array(vec![
            Some(level(
                match i {
                    7 => text("-0"),
                    6 => V::Symbol,
                    _ => text("0.1"),
                },
                "2",
            )),
            if i == 9 {
                None
            } else {
                Some(level(text("0.2"), "3"))
            },
            Some(level(text("0.3"), "4")),
        ]);
        let asks = array(vec![
            Some(level(text("0.8"), "5")),
            Some(level(text("0.9"), "6")),
        ]);
        let response = if name == "fetchOrderBook" {
            obj(
                80,
                vec![("bids", V::Array(bids.clone())), ("asks", V::Array(asks))],
            )
        } else {
            V::Array(orders.clone())
        };
        let error = Rc::new(Failure {
            marker: Some(47),
            name: "Error",
            message: u("boundary failure"),
        });
        let type_error = Rc::new(Failure {
            marker: None,
            name: "TypeError",
            message: u("Cannot convert a Symbol value to a string"),
        });
        Self {
            venue,
            name,
            i,
            events: RefCell::new(vec![]),
            error: error.clone(),
            type_error,
            input: one,
            orders,
            bids,
            op: Rc::new(Op {
                ready: Cell::new(false),
                result: RefCell::new(if i == 4 { Err(error) } else { Ok(response) }),
                polls: Cell::new(0),
            }),
            tasks: RefCell::new(vec![]),
            stored: RefCell::new(None),
            fill: RefCell::new(None),
            callback: Rc::new(37),
            cleanup: Rc::new(Cell::new(0)),
        }
    }
    fn hit(&self, e: Event) -> Result<(), E> {
        let tag = e.tag.clone();
        let severity = e.severity;
        self.events.borrow_mut().push(e);
        if (self.i == 10 && tag.starts_with("call:"))
            || (self.i == 5 && tag == "log" && severity == Some("info"))
            || (self.name == "handleOrderUpdate"
                && ((self.i == 5 && tag == "add")
                    || (self.i == 6 && tag == "fill")
                    || (self.i == 7 && tag == "log" && severity == Some("debug"))))
            || (self.i == 11 && tag == "log" && severity == Some("error"))
            || (self.name == "listenOrderBook"
                && ((self.i == 4 && tag == "subscribe") || (self.i == 5 && tag == "connect")))
        {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn finish(&self) {
        if self.i == 8 {
            if self.name == "cancelOrders" {
                self.orders.borrow_mut().push(Some(obj(
                    79,
                    vec![(
                        "id",
                        V::Text(u("id-79").into_iter().chain([0xdfff]).collect()),
                    )],
                )));
            }
            if self.name == "fetchOrders" {
                if let V::Object(o) = &self.input {
                    o.fields.borrow_mut().insert(
                        "id",
                        V::Text(u("late").into_iter().chain([0xd800]).collect()),
                    );
                }
            }
            if self.name == "fetchOrderBook" {
                self.bids.borrow_mut()[0] =
                    Some(obj(0, vec![("price", text("0.125")), ("size", text("2"))]));
            }
        }
        self.op.ready.set(true);
        self.drive();
    }
    fn drive(&self) {
        let tasks = std::mem::take(&mut *self.tasks.borrow_mut());
        for mut task in tasks {
            if task
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
            {
                self.tasks.borrow_mut().push(task);
            }
        }
    }
}
impl Runtime for Host {
    type Value = V;
    type Error = E;
    fn undefined(&self) -> V {
        V::U
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(v, V::U)
    }
    fn truthy(&self, v: &V) -> bool {
        match v {
            V::U => false,
            V::Text(t) => !t.is_empty(),
            V::Number(n) => *n != 0.0 && !n.is_nan(),
            _ => true,
        }
    }
    fn is_text(&self, v: &V, s: &str) -> bool {
        matches!(v,V::Text(t) if *t==u(s))
    }
    fn field(&self, v: &V, key: &str) -> Result<V, E> {
        self.hit(Event::new(format!("field:{key}")))?;
        let V::Object(o) = v else {
            panic!("fixture object binding")
        };
        if (key == "price" && o.marker != 0 && self.i == 5 && self.name != "handleOrderUpdate")
            || (key == "id" && self.i == 6 && self.name == "cancelOrders")
        {
            return Err(self.error.clone());
        }
        let mut reads = o.reads.borrow_mut();
        let n = reads.entry(key.to_owned()).or_default();
        let prior = *n;
        *n += 1;
        if self.i == 8 && prior > 0 {
            if key == "original_size" {
                return Ok(text("9tail"));
            }
            if key == "size_matched" {
                return Ok(text("2"));
            }
        }
        Ok(o.fields.borrow().get(key).cloned().unwrap_or(V::U))
    }
    fn string(&self, v: &V) -> Result<Vec<u16>, E> {
        Ok(match v {
            V::U => u("undefined"),
            V::Text(t) => t.clone(),
            V::Number(n) => u(&safe_cex_num::js_number_string(*n)),
            V::Symbol => return Err(self.type_error.clone()),
            V::Proto => u("function Object() { [native code] }"),
            V::Object(_) | V::Array(_) => u("[object Object]"),
        })
    }
    fn log(&self, message: &[u16], severity: &'static str) -> Result<(), E> {
        let mut e = Event::new("log");
        e.text = Some(message.to_vec());
        e.severity = Some(severity);
        self.hit(e)
    }
}
impl OrderHost for Host {
    type TokenMap = u32;
    fn token_map(&self, optional: bool) -> Result<Option<u32>, E> {
        let mut e = Event::new("token-map");
        e.numbers.push(if optional { 1.0 } else { 0.0 });
        self.hit(e)?;
        let absent = (self.venue == "private" && self.i == 3)
            || (self.venue == "exchange" && self.name == "mapOrder" && self.i == 10);
        if absent && !optional {
            return Err(Rc::new(Failure {
                marker: None,
                name: "TypeError",
                message: u("Cannot read properties of undefined (reading 'get')"),
            }));
        }
        Ok(if absent { None } else { Some(91) })
    }
    fn token_symbol(&self, _: &u32, token: &V) -> Result<V, E> {
        let mut e = Event::new("token-symbol");
        e.values.push(token.clone());
        self.hit(e)?;
        Ok(match self.i {
            2 => text(""),
            3 => V::U,
            _ => V::Text(u("mapped").into_iter().chain([0xd800]).collect()),
        })
    }
    fn enum_lookup(&self, table: &'static str, key: &V) -> Result<V, E> {
        let mut e = Event::new(format!("enum:{table}"));
        e.values.push(key.clone());
        self.hit(e)?;
        if self.is_text(key, "constructor") {
            return Ok(V::Proto);
        }
        let V::Text(k) = key else { return Ok(V::U) };
        let k = String::from_utf16_lossy(k);
        use safe_cex_venue_polymarket::constants::*;
        Ok(match table {
            "ORDER_STATUS_MAP" => ORDER_STATUS_MAP
                .iter()
                .find(|(name, _)| *name == k)
                .map(|(_, v)| text(v)),
            "ORDER_TYPE_MAP" => ORDER_TYPE_MAP
                .iter()
                .find(|(name, _)| *name == k)
                .map(|(_, v)| {
                    text(match v {
                        safe_cex_types::OrderType::Limit => "limit",
                        _ => panic!("fixture type binding"),
                    })
                }),
            "ORDER_SIDE_MAP" => ORDER_SIDE_MAP
                .iter()
                .find(|(name, _)| *name == k)
                .map(|(_, v)| {
                    text(match v {
                        safe_cex_types::OrderSide::Buy => "buy",
                        safe_cex_types::OrderSide::Sell => "sell",
                    })
                }),
            _ => unreachable!(),
        }
        .unwrap_or(V::U))
    }
    fn literal(&self, s: &'static str) -> V {
        text(s)
    }
    fn clock(&self) -> Result<f64, E> {
        let v = match self.i {
            4 => -0.0,
            9 => f64::NAN,
            _ => 1700000000123.0,
        };
        let mut e = Event::new("clock");
        e.numbers.push(v);
        self.hit(e)?;
        Ok(v)
    }
    fn error_string(&self, error: &E) -> Result<Vec<u16>, E> {
        Ok(u(error.name)
            .into_iter()
            .chain(u(": "))
            .chain(error.message.clone())
            .collect())
    }
    fn lowercase(&self, v: &V) -> Result<V, E> {
        let V::Text(t) = v else {
            panic!("fixture lowercase binding")
        };
        Ok(text(&String::from_utf16_lossy(t).to_lowercase()))
    }
    fn map_order(&self, v: &V) -> Result<Rc<RefCell<NativeOrder<V>>>, E> {
        let mut e = Event::new("map");
        e.values.push(v.clone());
        self.hit(e)?;
        let order = if self.venue == "private" {
            map_private_order(self, v)?
        } else {
            map_exchange_order(self, v)?
        };
        let result = Rc::new(RefCell::new(order));
        *self.stored.borrow_mut() = Some(result.clone());
        Ok(result)
    }
    fn add_orders(&self, orders: Vec<Rc<RefCell<NativeOrder<V>>>>) -> Result<(), E> {
        assert_eq!(orders.len(), 1, "independent native one-order store update");
        assert!(
            Rc::ptr_eq(&orders[0], self.stored.borrow().as_ref().unwrap()),
            "native original mapped object"
        );
        self.hit(Event::new("add"))?;
        if self.i == 8 {
            let mut o = orders[0].borrow_mut();
            o.id = V::Text(u("changed").into_iter().chain([0xd800]).collect());
            o.status = text("changed-status");
            o.side = text("sell");
            o.filled = 42.0;
        }
        Ok(())
    }
    fn emit_fill(&self, fill: NativeFill<V>) -> Result<(), E> {
        *self.fill.borrow_mut() = Some(fill);
        self.hit(Event::new("fill"))
    }
}
impl AdapterHost for Host {
    type Array = Array;
    type Operation = Op;
    type Callback = Rc<u32>;
    type Cleanup = Rc<Cell<usize>>;
    fn array(&self, v: &V) -> Result<Array, E> {
        let V::Array(a) = v else {
            panic!("fixture array binding")
        };
        Ok(a.clone())
    }
    fn length(&self, a: &Array) -> Result<usize, E> {
        let length = a.borrow().len();
        let mut e = Event::new("length");
        e.numbers.push(length as f64);
        self.hit(e)?;
        Ok(length)
    }
    fn at(&self, a: &Array, index: usize) -> Result<Option<V>, E> {
        let mut e = Event::new("at");
        e.numbers.push(index as f64);
        self.hit(e)?;
        Ok(a.borrow().get(index).cloned().flatten())
    }
    fn start_task(&self, mut task: Pin<Box<dyn Future<Output = ()>>>) {
        if task
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
        {
            self.tasks.borrow_mut().push(task);
        }
    }
    fn call(&self, call: ApiCall<V>) -> Result<Rc<Op>, E> {
        let mut e = Event::new(match &call {
            ApiCall::OpenOrders => "call:open",
            ApiCall::CancelOrders(_) => "call:cancel",
            ApiCall::CancelAllOrders => "call:cancel-all",
            ApiCall::OrderBook(_) => "call:book",
        });
        match call {
            ApiCall::OrderBook(v) => e.values.push(v),
            ApiCall::CancelOrders(ids) => {
                assert_eq!(
                    ids.len(),
                    if self.i == 9 { 3 } else { 2 },
                    "independent native cancel payload count"
                );
                for (j, id) in ids.into_iter().enumerate() {
                    let expected = if self.i == 9 && j == 1 {
                        None
                    } else {
                        Some(V::Text(
                            u(if j == 0 { "id-71" } else { "id-73" })
                                .into_iter()
                                .chain([0xdfff])
                                .collect(),
                        ))
                    };
                    match (id, expected) {
                        (Some(a), Some(b)) => {
                            same_value(&a, &b, "native cancellation id");
                            e.values.push(a);
                        }
                        (None, None) => e.values.push(V::U),
                        _ => panic!("native cancellation hole"),
                    }
                }
            }
            _ => {}
        }
        self.hit(e)?;
        Ok(self.op.clone())
    }
    fn token(&self, symbol: &[u16]) -> Result<V, E> {
        let mut e = Event::new("token");
        e.text = Some(symbol.to_vec());
        self.hit(e)?;
        Ok(match self.i {
            2 => text(""),
            3 => V::U,
            _ => V::Text(u("token").into_iter().chain([0xd800]).collect()),
        })
    }
    fn new_error(&self, message: Vec<u16>) -> E {
        Rc::new(Failure {
            marker: None,
            name: "Error",
            message,
        })
    }
    fn subscribe_book(&self, token: V, callback: Rc<u32>) -> Result<Rc<Cell<usize>>, E> {
        assert!(
            Rc::ptr_eq(&callback, &self.callback),
            "native original callback identity"
        );
        let mut e = Event::new("subscribe");
        e.values.push(token);
        self.hit(e)?;
        Ok(self.cleanup.clone())
    }
    fn connect(&self) -> Result<(), E> {
        self.hit(Event::new("connect"))
    }
}
fn oracle() -> &'static Value {
    static O: OnceLock<Value> = OnceLock::new();
    O.get_or_init(|| {
        let r = Command::new("node")
            .arg(format!(
                "{}/tests/order_adapters_oracle.cjs",
                env!("CARGO_MANIFEST_DIR")
            ))
            .output()
            .unwrap();
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        serde_json::from_slice(&r.stdout).unwrap()
    })
}
fn bits(n: f64, s: &Value, label: &str) {
    if n.is_nan() {
        assert_eq!(s["kind"], "nan", "{label}:native NaN");
    } else {
        assert_eq!(s["kind"], "number");
        assert_eq!(
            format!("{:016x}", n.to_bits()),
            s["bits"],
            "{label}:native Number bits"
        );
    }
}
fn value(v: &V, s: &Value, label: &str) {
    match v {
        V::U => assert_eq!(s["kind"], "undefined", "{label}"),
        V::Text(t) => {
            assert_eq!(s["kind"], "text");
            assert_eq!(*t, units(&s["units"]), "{label}:native UTF16");
        }
        V::Number(n) => bits(*n, s, label),
        V::Symbol => assert_eq!(s["kind"], "symbol"),
        V::Proto => {
            assert_eq!(s["kind"], "object");
            assert_eq!(s["marker"], 91);
        }
        V::Object(o) => {
            assert_eq!(s["kind"], "object");
            assert_eq!(s["marker"], o.marker, "{label}:native object marker");
        }
        V::Array(_) => panic!("array snapshot not expected"),
    }
}
fn same_value(a: &V, b: &V, label: &str) {
    match (a, b) {
        (V::U, V::U) | (V::Proto, V::Proto) | (V::Symbol, V::Symbol) => {}
        (V::Text(a), V::Text(b)) => assert_eq!(a, b, "{label}"),
        (V::Number(a), V::Number(b)) => assert_eq!(a.to_bits(), b.to_bits(), "{label}"),
        (V::Object(a), V::Object(b)) => assert!(Rc::ptr_eq(a, b), "{label}"),
        _ => panic!("{label}:native values differ"),
    }
}
fn error(result: Result<(), E>, s: &Value, h: &Host) {
    match result {
        Ok(()) => assert!(s.is_null(), "source unexpectedly failed: {s}"),
        Err(e) => {
            assert!(!s.is_null(), "native unexpectedly failed: {e:?}");
            assert_eq!(e.name, s["name"]);
            assert_eq!(e.message, units(&s["message"]), "native error message");
            assert_eq!(
                s["same"],
                Rc::ptr_eq(&e, &h.error),
                "native original error identity"
            );
            assert_eq!(s["marker"], e.marker.map_or(Value::Null, Value::from));
        }
    }
}
fn order_assert(o: &NativeOrder<V>, s: &Value, input: &V) {
    let rows = s.as_array().unwrap();
    let keys = [
        "id",
        "status",
        "symbol",
        "type",
        "side",
        "price",
        "amount",
        "filled",
        "remaining",
        "reduceOnly",
        "info",
        "lastUpdate",
    ];
    assert_eq!(rows.len(), 12, "independent mapped Order fields");
    for (j, key) in keys.iter().enumerate() {
        assert_eq!(rows[j][0], *key);
    }
    for (j, v) in [
        (0, &o.id),
        (1, &o.status),
        (2, &o.symbol),
        (3, &o.order_type),
        (4, &o.side),
        (10, &o.info),
    ] {
        value(v, &rows[j][1], &format!("native mapped Order {j}"));
    }
    for (j, n) in [
        (5, o.price),
        (6, o.amount),
        (7, o.filled),
        (8, o.remaining),
        (11, o.last_update),
    ] {
        bits(n, &rows[j][1], &format!("native mapped Order {j}"));
    }
    assert!(!o.reduce_only, "native mapped reduceOnly");
    assert_eq!(rows[9][1]["value"], false);
    same_value(&o.info, input, "native original info identity");
}
fn mapped_tags(venue: &str, name: &str, i: usize) -> (Vec<String>, bool) {
    let mut t = vec!["token-map"];
    if venue == "exchange" && name == "mapOrder" && i == 10 {
        return (t.into_iter().map(str::to_owned).collect(), true);
    }
    if !(venue == "private" && i == 3) {
        t.extend(["field:asset_id", "token-symbol"]);
    }
    if i == 2 || i == 3 {
        t.push("field:asset_id");
    }
    t.extend(["field:id", "field:status", "enum:ORDER_STATUS_MAP"]);
    if i == 2 {
        t.push("field:status");
    }
    t.extend([
        "field:order_type",
        "enum:ORDER_TYPE_MAP",
        "field:side",
        "enum:ORDER_SIDE_MAP",
        "field:price",
    ]);
    let failed = (i == 5 && name != "handleOrderUpdate")
        || (i == 6 && name != "handleOrderUpdate")
        || i == 11;
    if !failed {
        t.extend([
            "field:original_size",
            "field:size_matched",
            "field:original_size",
            "field:size_matched",
            "clock",
        ]);
    }
    (t.into_iter().map(str::to_owned).collect(), failed)
}
fn expected_tags(venue: &str, name: &str, i: usize) -> Vec<String> {
    let mut tags = vec![];
    fn add(tags: &mut Vec<String>, s: &str) {
        tags.push(s.to_owned());
    }
    match name {
        "mapOrder" => return mapped_tags(venue, name, i).0,
        "fetchOrders" => {
            add(&mut tags, "call:open");
            if i == 10 || i == 4 {
                add(&mut tags, "log");
                return tags;
            }
            add(&mut tags, "length");
            for j in 0..if i == 2 {
                0
            } else if i == 9 {
                3
            } else {
                2
            } {
                add(&mut tags, "at");
                if i == 9 && j == 1 {
                    continue;
                }
                add(&mut tags, "map");
                let (t, failed) = mapped_tags(venue, name, i);
                tags.extend(t);
                if failed {
                    tags.push("log".into());
                    break;
                }
            }
        }
        "cancelOrders" => {
            add(&mut tags, "length");
            if i == 2 {
                return tags;
            }
            add(&mut tags, "length");
            for j in 0..if i == 9 { 3 } else { 2 } {
                add(&mut tags, "at");
                if i == 9 && j == 1 {
                    continue;
                }
                add(&mut tags, "field:id");
                if i == 6 {
                    return tags;
                }
            }
            add(&mut tags, "call:cancel");
            if i != 4 && i != 10 {
                add(&mut tags, "length");
                add(&mut tags, "log");
            }
        }
        "cancelAllOrders" => {
            add(&mut tags, "call:cancel-all");
            if i != 4 && i != 10 {
                add(&mut tags, "log");
            }
        }
        "fetchOrderBook" => {
            add(&mut tags, "token");
            if i == 2 || i == 3 {
                return tags;
            }
            add(&mut tags, "call:book");
            if i == 4 || i == 10 {
                return tags;
            }
            add(&mut tags, "field:bids");
            add(&mut tags, "length");
            for j in 0..3 {
                add(&mut tags, "at");
                if i == 9 && j == 1 {
                    continue;
                }
                add(&mut tags, "field:price");
                if i == 6 {
                    return tags;
                }
                add(&mut tags, "field:size");
            }
            add(&mut tags, "field:asks");
            add(&mut tags, "length");
            for _ in 0..2 {
                add(&mut tags, "at");
                add(&mut tags, "field:price");
                add(&mut tags, "field:size");
            }
        }
        "listenOrderBook" => {
            add(&mut tags, "token");
            if i == 2 || i == 3 {
                return tags;
            }
            add(&mut tags, "subscribe");
            if i == 4 {
                return tags;
            }
            add(&mut tags, "connect");
            if i != 5 {
                add(&mut tags, "cleanup");
                add(&mut tags, "cleanup");
            }
        }
        "handleOrderUpdate" => {
            add(&mut tags, "map");
            let (t, failed) = mapped_tags(venue, name, i);
            tags.extend(t);
            if failed {
                tags.push("log".into());
                return tags;
            }
            tags.push("add".into());
            if i == 5 {
                tags.push("log".into());
                return tags;
            }
            tags.push("field:status".into());
            if [4, 6, 8].contains(&i) {
                tags.push("fill".into());
                if i == 6 {
                    tags.push("log".into());
                    return tags;
                }
            }
            tags.push("log".into());
            if i == 7 {
                tags.push("log".into());
            }
        }
        _ => unreachable!(),
    };
    tags
}
fn expected_before(venue: &str, name: &str, i: usize) -> Vec<String> {
    let mut t = expected_tags(venue, name, i);
    match name {
        "fetchOrders" => {
            t.truncate(if i == 10 { 2 } else { 1 });
        }
        "cancelOrders" => {
            if i != 2 && i != 6 && i != 4 && i != 10 {
                t.truncate(t.len() - 2);
            }
        }
        "cancelAllOrders" => {
            t.truncate(1);
        }
        "fetchOrderBook" => {
            t.truncate(if i == 2 || i == 3 { 1 } else { 2 });
        }
        "listenOrderBook" => {
            if ![2, 3, 4, 5].contains(&i) {
                t.truncate(t.len() - 2);
            }
        }
        _ => {}
    };
    t
}
enum Outcome {
    None,
    Order(NativeOrder<V>),
    Orders(Vec<Option<Rc<RefCell<NativeOrder<V>>>>>),
    Book(NativeBook),
    Cleanup(Rc<Cell<usize>>),
}
fn execute(h: Rc<Host>, symbol: &[u16]) -> Pin<Box<dyn Future<Output = Result<Outcome, E>>>> {
    match (h.venue, h.name) {
        ("exchange", "mapOrder") => {
            let r = map_exchange_order(&*h, &h.input).map(Outcome::Order);
            Box::pin(async move { r })
        }
        ("private", "mapOrder") => {
            let r = map_private_order(&*h, &h.input).map(Outcome::Order);
            Box::pin(async move { r })
        }
        (_, "fetchOrders") => {
            let r = fetch_orders(h);
            Box::pin(async move { r.await.map(Outcome::Orders) })
        }
        (_, "cancelOrders") => {
            let r = cancel_orders(h.clone(), h.orders.clone());
            Box::pin(async move { r.await.map(|_| Outcome::None) })
        }
        (_, "cancelAllOrders") => {
            let r = cancel_all_orders(h);
            Box::pin(async move { r.await.map(|_| Outcome::None) })
        }
        (_, "fetchOrderBook") => {
            let r = fetch_order_book(h, symbol);
            Box::pin(async move { r.await.map(Outcome::Book) })
        }
        (_, "listenOrderBook") => {
            let r = listen_order_book(&*h, symbol, h.callback.clone()).map(Outcome::Cleanup);
            Box::pin(async move { r })
        }
        (_, "handleOrderUpdate") => {
            let r = handle_order_update(&*h, &h.input).map(|_| Outcome::None);
            Box::pin(async move { r })
        }
        _ => unreachable!(),
    }
}
fn event_assert(e: &Event, s: &Value) {
    assert_eq!(e.tag, s["tag"]);
    match e.tag.as_str() {
        "token-map" => assert_eq!(s["optional"], e.numbers[0] != 0.0, "native token map mode"),
        "token-symbol" | "call:book" | "subscribe" => {
            value(&e.values[0], &s["token"], "native token argument")
        }
        "map" => value(&e.values[0], &s["input"], "native current mapper input"),
        "clock" => bits(e.numbers[0], &s["value"], "native clock"),
        "length" => assert_eq!(
            s["length"], e.numbers[0] as usize,
            "native current array length"
        ),
        "at" => assert_eq!(s["index"], e.numbers[0] as usize, "native live array index"),
        "token" => assert_eq!(
            *e.text.as_ref().unwrap(),
            units(&s["symbol"]),
            "native symbol units"
        ),
        "call:cancel" => {
            assert_eq!(s["ids"].as_array().unwrap().len(), e.values.len());
            for (j, v) in e.values.iter().enumerate() {
                if matches!(v, V::U) {
                    assert_eq!(s["ids"][j]["kind"], "hole");
                } else {
                    value(v, &s["ids"][j], "native cancel request id");
                }
            }
        }
        "log" => {
            assert_eq!(
                *e.text.as_ref().unwrap(),
                units(&s["text"]),
                "native log text"
            );
            assert_eq!(s["severity"], e.severity.unwrap(), "native log severity");
        }
        t if t.starts_with("enum:") => value(&e.values[0], &s["key"], "native enum key"),
        _ => {}
    }
}
fn check(venue: &'static str, name: &'static str) {
    let all = oracle()["cases"].as_array().unwrap();
    assert_eq!(all.len(), 96, "independent eight by twelve floor");
    assert_eq!(all[95]["name"], "handleOrderUpdate");
    assert_eq!(all[95]["index"], 11);
    let cases: Vec<_> = all
        .iter()
        .filter(|c| c["venue"] == venue && c["name"] == name)
        .collect();
    assert_eq!(cases.len(), 12, "independent per-body floor");
    for (i, c) in cases.iter().enumerate() {
        assert_eq!(c["index"], i);
        let h = Rc::new(Host::new(venue, name, i));
        let mut result = execute(h.clone(), &units(&c["symbol"]));
        let before: Vec<_> = h.events.borrow().iter().map(|e| e.tag.clone()).collect();
        let source_before: Vec<_> = c["before"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let wanted = expected_before(venue, name, i);
        assert_eq!(
            before, wanted,
            "native synchronous prefix:{venue}.{name} index{i}"
        );
        assert_eq!(
            source_before, wanted,
            "independent source synchronous prefix:{venue}.{name} index{i}"
        );
        let first = result
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()));
        h.finish();
        let output = match first {
            Poll::Ready(v) => v,
            Poll::Pending => match result
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            {
                Poll::Ready(v) => v,
                Poll::Pending => panic!("native retained method task did not complete"),
            },
        };
        let mut root = Ok(());
        match output {
            Err(e) => root = Err(e),
            Ok(Outcome::None) => {}
            Ok(Outcome::Order(o)) => {
                order_assert(&o, &c["output"], &h.input);
                assert_eq!(
                    o.amount.to_bits(),
                    (if i == 8 { 8.0_f64 } else { 3.0 }).to_bits(),
                    "independent native order amount"
                );
                assert_eq!(
                    o.remaining.to_bits(),
                    (if i == 8 { 7.0_f64 } else { 2.0 }).to_bits(),
                    "independent native order remaining"
                );
            }
            Ok(Outcome::Orders(orders)) => {
                let expected = if i == 2 {
                    0
                } else if i == 9 {
                    3
                } else {
                    2
                };
                assert_eq!(
                    orders.len(),
                    expected,
                    "independent native mapped-order count"
                );
                assert_eq!(
                    c["output"].as_array().unwrap().len(),
                    expected,
                    "independent source mapped-order count"
                );
                for (j, o) in orders.into_iter().enumerate() {
                    if i == 9 && j == 1 {
                        assert!(o.is_none(), "native mapped slot hole");
                        assert!(c["output"][j].is_null());
                    } else {
                        assert!(o.is_some(), "native mapped slot:fetchOrders index{i}");
                        let o = o.unwrap();
                        let input = h.orders.borrow()[j].clone().unwrap();
                        order_assert(&o.borrow(), &c["output"][j], &input);
                    }
                }
            }
            Ok(Outcome::Book(book)) => {
                assert_eq!(book.bids.len(), 3, "independent native book bid count");
                assert_eq!(book.asks.len(), 2, "independent native book ask count");
                for (side, rows, count) in [("bids", book.bids, 3), ("asks", book.asks, 2)] {
                    assert_eq!(c["output"][side].as_array().unwrap().len(), count);
                    for (j, level) in rows.into_iter().enumerate() {
                        if side == "bids" && i == 9 && j == 1 {
                            assert!(level.is_none(), "native book hole");
                            assert!(c["output"][side][j].is_null());
                        } else {
                            let level = level.unwrap();
                            bits(
                                level.price,
                                &c["output"][side][j]["price"],
                                "native book price",
                            );
                            bits(
                                level.amount,
                                &c["output"][side][j]["amount"],
                                "native book amount",
                            );
                            bits(
                                level.total,
                                &c["output"][side][j]["total"],
                                "native book running total",
                            );
                            let total: f64 = if side == "asks" {
                                if j == 0 { 5.0 } else { 11.0 }
                            } else if j == 0 {
                                2.0
                            } else if j == 1 {
                                5.0
                            } else if i == 9 {
                                6.0
                            } else {
                                9.0
                            };
                            assert_eq!(
                                level.total.to_bits(),
                                total.to_bits(),
                                "independent native side total"
                            );
                        }
                    }
                }
            }
            Ok(Outcome::Cleanup(cb)) => {
                assert!(Rc::ptr_eq(&cb, &h.cleanup), "native exact cleanup identity");
                for _ in 0..2 {
                    h.hit(Event::new("cleanup")).unwrap();
                    cb.set(cb.get() + 1);
                }
            }
        };
        error(root, &c["error"], &h);
        if name == "handleOrderUpdate" {
            if let Some(o) = h.stored.borrow().as_ref() {
                order_assert(&o.borrow(), &c["output"], &h.input);
            }
        }
        if let Some(fill) = h.fill.borrow().as_ref() {
            value(&fill.id, &c["fill"]["id"], "native fill id");
            value(&fill.side, &c["fill"]["side"], "native fill side");
            value(&fill.symbol, &c["fill"]["symbol"], "native fill symbol");
            bits(fill.price, &c["fill"]["price"], "native fill price");
            bits(fill.amount, &c["fill"]["amount"], "native fill amount");
            assert_eq!(
                fill.amount.to_bits(),
                (if i == 8 { 42.0_f64 } else { 1.0 }).to_bits(),
                "independent native filled amount"
            );
        } else {
            assert!(c["fill"].is_null());
        }
        assert_eq!(
            h.cleanup.get(),
            if name == "listenOrderBook" && ![2, 3, 4, 5].contains(&i) {
                2
            } else {
                0
            },
            "independent native cleanup replays"
        );
        assert_eq!(c["cleanupTicks"], h.cleanup.get());
        let expected = expected_tags(venue, name, i);
        let events = h.events.borrow();
        assert_eq!(
            events.len(),
            expected.len(),
            "native adapter trace:{venue}.{name} index{i}"
        );
        assert_eq!(
            c["trace"].as_array().unwrap().len(),
            expected.len(),
            "independent source trace count:{venue}.{name} index{i}"
        );
        for (j, (e, tag)) in events.iter().zip(expected).enumerate() {
            assert_eq!(e.tag, tag, "native adapter trace:{venue}.{name} index{i}");
            event_assert(e, &c["trace"][j]);
        }
        assert!(
            h.tasks.borrow().is_empty(),
            "native completed task ownership"
        );
    }
    assert_eq!(cases[11]["index"], 11, "last per-body fixture");
}
macro_rules! row {
    ($t:ident,$v:literal,$n:literal) => {
        #[test]
        fn $t() {
            check($v, $n)
        }
    };
}
row!(exchange_map, "exchange", "mapOrder");
row!(private_map, "private", "mapOrder");
row!(orders_read, "exchange", "fetchOrders");
row!(cancel_many, "exchange", "cancelOrders");
row!(cancel_all, "exchange", "cancelAllOrders");
row!(book_read, "exchange", "fetchOrderBook");
row!(book_listen, "exchange", "listenOrderBook");
row!(order_update, "private", "handleOrderUpdate");
#[test]
fn abandoned_observers_keep_method_effects() {
    let source = oracle()["abandoned"].as_array().unwrap();
    assert_eq!(source.len(), 4, "independent abandoned source floor");
    for (index, name) in [
        "fetchOrders",
        "cancelOrders",
        "cancelAllOrders",
        "fetchOrderBook",
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(source[index]["name"], name);
        assert_eq!(source[index]["index"], 0);
        let h = Rc::new(Host::new("exchange", name, 0));
        let observer = execute(h.clone(), &u("symbol"));
        drop(observer);
        h.finish();
        let events = h.events.borrow();
        let expected = expected_tags("exchange", name, 0);
        assert_eq!(
            events.len(),
            expected.len(),
            "native abandoned method effects:{name}"
        );
        assert_eq!(
            source[index]["trace"].as_array().unwrap().len(),
            expected.len(),
            "independent abandoned source trace floor"
        );
        for (j, (e, tag)) in events.iter().zip(expected).enumerate() {
            assert_eq!(e.tag, tag, "native abandoned method effects:{name}");
            event_assert(e, &source[index]["trace"][j]);
        }
        assert!(h.tasks.borrow().is_empty());
    }
    assert_eq!(source[3]["name"], "fetchOrderBook");
}

#[test]
fn ready_child_operations_yield_before_effects() {
    let source = oracle()["resolved"].as_array().unwrap();
    assert_eq!(source.len(), 4, "independent ready-child source floor");
    for (index, name) in [
        "fetchOrders",
        "cancelOrders",
        "cancelAllOrders",
        "fetchOrderBook",
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(source[index]["name"], name);
        let h = Rc::new(Host::new("exchange", name, 0));
        h.op.ready.set(true);
        let mut observer = execute(h.clone(), &u("symbol"));
        let before: Vec<_> = h.events.borrow().iter().map(|e| e.tag.clone()).collect();
        let wanted = expected_before("exchange", name, 0);
        assert_eq!(before, wanted, "native ready-child prefix:{name}");
        let source_before: Vec<_> = source[index]["before"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            source_before, wanted,
            "independent source ready-child prefix"
        );
        assert!(
            observer
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending(),
            "native ready-child observer yields"
        );
        h.drive();
        assert!(
            matches!(
                observer
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop())),
                Poll::Ready(Ok(_))
            ),
            "native ready-child observer completes"
        );
        let events = h.events.borrow();
        let expected = expected_tags("exchange", name, 0);
        assert_eq!(
            events.len(),
            expected.len(),
            "native ready-child effect count"
        );
        assert_eq!(
            source[index]["trace"].as_array().unwrap().len(),
            expected.len()
        );
        for (j, (e, tag)) in events.iter().zip(expected).enumerate() {
            assert_eq!(e.tag, tag);
            event_assert(e, &source[index]["trace"][j]);
        }
        assert!(h.tasks.borrow().is_empty());
    }
    assert_eq!(source[3]["name"], "fetchOrderBook");
}
