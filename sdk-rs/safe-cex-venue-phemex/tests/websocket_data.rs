use safe_cex_venue_phemex::websocket_data::{self as ws, WsHost};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
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
    Num(f64),
    Text(Vec<u16>),
    Array(Rc<RefCell<Vec<V>>>),
    Object(Rc<RefCell<Vec<(Vec<u16>, V)>>>),
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn arr(v: Vec<V>) -> V {
    V::Array(Rc::new(RefCell::new(v)))
}
fn obj(v: Vec<(&str, V)>) -> V {
    V::Object(Rc::new(RefCell::new(
        v.into_iter().map(|(k, v)| (u(k), v)).collect(),
    )))
}
fn txt(s: &str) -> V {
    V::Text(u(s))
}
impl V {
    fn decode(v: &Value) -> Self {
        if let Some(a) = v.get("$units").and_then(Value::as_array) {
            return Self::Text(a.iter().map(|v| v.as_u64().unwrap() as u16).collect());
        }
        if let Some(b) = v.get("$bits").and_then(Value::as_str) {
            return Self::Num(f64::from_bits(u64::from_str_radix(b, 16).unwrap()));
        }
        if v.get("$undefined").is_some() {
            return Self::Undefined;
        }
        match v {
            Value::Null => Self::Null,
            Value::Bool(b) => Self::Bool(*b),
            Value::Number(n) => Self::Num(n.as_f64().unwrap()),
            Value::String(s) => txt(s),
            Value::Array(v) => arr(v.iter().map(Self::decode).collect()),
            Value::Object(v) => Self::Object(Rc::new(RefCell::new(
                v.iter().map(|(k, v)| (u(k), Self::decode(v))).collect(),
            ))),
        }
    }
    fn prop(&self, k: &[u16]) -> V {
        match self {
            Self::Object(o) => o
                .borrow()
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
                .unwrap_or(Self::Undefined),
            Self::Array(a) if k == u("length") => Self::Num(a.borrow().len() as f64),
            Self::Text(t) if k == u("length") => Self::Num(t.len() as f64),
            _ => Self::Undefined,
        }
    }
    fn get(&self, k: &str) -> V {
        self.prop(&u(k))
    }
    fn set(&self, k: &[u16], v: V) {
        if let Self::Object(o) = self {
            let mut o = o.borrow_mut();
            if let Some((_, old)) = o.iter_mut().find(|(key, _)| key == k) {
                *old = v
            } else {
                o.push((k.to_vec(), v))
            }
        } else {
            panic!("native mutable object")
        }
    }
    fn rows(&self) -> Vec<V> {
        if let Self::Array(a) = self {
            a.borrow().clone()
        } else {
            vec![]
        }
    }
    fn truthy(&self) -> bool {
        match self {
            Self::Undefined | Self::Null => false,
            Self::Bool(v) => *v,
            Self::Num(v) => *v != 0.0 && !v.is_nan(),
            Self::Text(t) => !t.is_empty(),
            _ => true,
        }
    }
    fn number(&self) -> f64 {
        match self {
            Self::Undefined => f64::NAN,
            Self::Null => 0.0,
            Self::Bool(v) => {
                if *v {
                    1.0
                } else {
                    0.0
                }
            }
            Self::Num(n) => *n,
            Self::Text(t) => String::from_utf16(t)
                .map(|s| safe_cex_num::number_from_string(&s))
                .unwrap_or(f64::NAN),
            Self::Array(v) => {
                let v = v.borrow();
                if v.is_empty() {
                    0.0
                } else if v.len() == 1 {
                    v[0].number()
                } else {
                    f64::NAN
                }
            }
            _ => f64::NAN,
        }
    }
    fn text(&self) -> Vec<u16> {
        match self {
            Self::Undefined => u("undefined"),
            Self::Null => u("null"),
            Self::Bool(v) => u(if *v { "true" } else { "false" }),
            Self::Num(n) => u(&safe_cex_num::js_number_string(*n)),
            Self::Text(t) => t.clone(),
            _ => u("[object Object]"),
        }
    }
    fn parse(&self) -> f64 {
        safe_cex_num::parse_float(&String::from_utf16_lossy(&self.text()))
    }
    fn snapshot(&self) -> V {
        match self {
            Self::Array(v) => arr(v.borrow().iter().map(Self::snapshot).collect()),
            Self::Object(v) => Self::Object(Rc::new(RefCell::new(
                v.borrow()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.snapshot()))
                    .collect(),
            ))),
            _ => self.clone(),
        }
    }
    fn obs(&self) -> Value {
        match self {
            Self::Undefined => json!({"undefined":true}),
            Self::Null => Value::Null,
            Self::Bool(v) => json!(v),
            Self::Num(n) => {
                json!({"bits":format!("{:016x}",if n.is_nan(){f64::NAN.to_bits()}else{n.to_bits()})})
            }
            Self::Text(t) => json!({"units":t}),
            Self::Array(v) => Value::Array(v.borrow().iter().map(Self::obs).collect()),
            Self::Object(v) => {
                let mut entries = v
                    .borrow()
                    .iter()
                    .map(|(k, v)| json!([{"units":k},v.obs()]))
                    .collect::<Vec<_>>();
                entries.sort_by_key(|v| v[0].to_string());
                json!({"object":entries})
            }
        }
    }
}
fn eq(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Undefined, V::Undefined) | (V::Null, V::Null) => true,
        (V::Bool(a), V::Bool(b)) => a == b,
        (V::Num(a), V::Num(b)) => a == b,
        (V::Text(a), V::Text(b)) => a == b,
        (V::Object(a), V::Object(b)) => Rc::ptr_eq(a, b),
        (V::Array(a), V::Array(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn ascii(v: &V) -> String {
    String::from_utf16_lossy(&v.text())
}
#[derive(Clone, Debug)]
struct E {
    original: bool,
}
fn poll(task: &mut Pin<Box<dyn Future<Output = Result<(), E>>>>) -> Poll<Result<(), E>> {
    let w = Waker::noop().clone();
    task.as_mut().poll(&mut Context::from_waker(&w))
}
type Task = Pin<Box<dyn Future<Output = Result<(), E>>>>;
type Timer = Box<dyn FnOnce() -> Result<(), E>>;
struct State {
    store: V,
    fields: V,
    effects: Vec<V>,
    tasks: Vec<Task>,
    timers: Vec<(usize, Timer)>,
    serial: usize,
    parser: usize,
    timer_id: usize,
    now: f64,
    books: Vec<V>,
}
#[derive(Clone)]
struct Host {
    input: V,
    state: Rc<RefCell<State>>,
    pending: Rc<Cell<usize>>,
}
impl Host {
    fn new(input: V) -> Self {
        let market = obj(vec![
            ("id", txt("btc")),
            ("symbol", txt("BTCUSDT")),
            ("quote", txt("USDT")),
        ]);
        let tickers = if matches!(input.get("tickers"), V::Undefined) {
            arr(vec![])
        } else {
            input.get("tickers")
        };
        let index = obj(vec![]);
        for t in tickers.rows() {
            index.set(&t.get("symbol").text(), t)
        }
        let positions = if matches!(input.get("positions"), V::Undefined) {
            arr(vec![])
        } else {
            input.get("positions")
        };
        let balance = if matches!(input.get("balance"), V::Undefined) {
            obj(vec![("currencies", obj(vec![])), ("info", obj(vec![]))])
        } else {
            input.get("balance")
        };
        let store = obj(vec![
            ("markets", arr(vec![market.clone()])),
            ("marketsBySymbol", obj(vec![("BTCUSDT", market)])),
            ("tickers", tickers),
            ("tickersBySymbol", index),
            ("positions", positions),
            ("orders", arr(vec![])),
            ("loaded", obj(vec![("positions", V::Bool(false))])),
            ("balance", balance),
        ]);
        let connected = if matches!(input.get("connected"), V::Undefined) {
            V::Bool(true)
        } else {
            input.get("connected")
        };
        let fields = obj(vec![
            ("disposed", V::Bool(input.get("disposed").truthy())),
            ("connected", connected),
            ("topics", obj(vec![])),
            (
                "handlers",
                obj(vec![(
                    "perp_market24h_pack_p.update",
                    arr(vec![txt("bound")]),
                )]),
            ),
            (
                "timeframes",
                obj(vec![("1m", txt("60")), ("1h", txt("3600"))]),
            ),
            ("orderBookInnerState", obj(vec![])),
            ("snapshotLogs", obj(vec![])),
            ("requestId", V::Num(1200.0)),
        ]);
        Self {
            input,
            state: Rc::new(RefCell::new(State {
                store,
                fields,
                effects: vec![],
                tasks: vec![],
                timers: vec![],
                serial: 0,
                parser: 0,
                timer_id: 0,
                now: 100000.0,
                books: vec![],
            })),
            pending: Rc::new(Cell::new(0)),
        }
    }
    fn effect(&mut self, name: &str, args: Vec<V>) -> Result<(), E> {
        let mut record = vec![txt(name)];
        record.extend(args);
        self.state.borrow_mut().effects.push(arr(record).snapshot());
        if eq(&self.input.get("failure"), &txt(name)) {
            Err(E { original: true })
        } else {
            Ok(())
        }
    }
    fn store_value(&self) -> V {
        self.state.borrow().store.clone()
    }
    fn fields(&self) -> V {
        self.state.borrow().fields.clone()
    }
    fn drain(&mut self) -> Result<(), E> {
        for _ in 0..100 {
            let tasks = std::mem::take(&mut self.state.borrow_mut().tasks);
            if tasks.is_empty() {
                return Ok(());
            }
            for mut task in tasks {
                match poll(&mut task) {
                    Poll::Ready(v) => v?,
                    Poll::Pending => self.state.borrow_mut().tasks.push(task),
                }
            }
        }
        panic!("WS bounded continuation")
    }
    fn run_timer(&mut self) -> Result<(), E> {
        let timer = {
            let mut state = self.state.borrow_mut();
            if state.timers.is_empty() {
                None
            } else {
                Some(state.timers.remove(0).1)
            }
        };
        if let Some(timer) = timer {
            timer()?
        }
        Ok(())
    }
    fn handler_counts(&self) -> V {
        let V::Object(o) = self.fields().get("handlers") else {
            panic!()
        };
        V::Object(Rc::new(RefCell::new(
            o.borrow()
                .iter()
                .map(|(k, v)| (k.clone(), V::Num(v.rows().len() as f64)))
                .collect(),
        )))
    }
    fn packed(&mut self, input: V) -> V {
        let last = input.get("lastRp").parse();
        let volume = input.get("volumeRq").parse();
        let open = input.get("openRp").parse();
        let result = obj(vec![
            ("last", V::Num(last)),
            ("volume", V::Num(volume)),
            ("quoteVolume", V::Num(volume * last)),
            ("percentage", V::Num((last - open) / open * 100.0)),
            ("fundingRate", V::Num(input.get("fundingRateRr").parse())),
            ("mark", V::Num(input.get("markRp").parse())),
            ("index", V::Num(input.get("indexRp").parse())),
            ("openInterest", V::Num(input.get("openInterestRv").parse())),
        ]);
        for key in ["bid", "ask"] {
            let v = input.get(key);
            if v.truthy() {
                result.set(&u(key), V::Num(v.parse()))
            }
        }
        result
    }
    fn update_by_price(&mut self, args: Vec<V>) -> Result<V, E> {
        self.effect("updateByPrice", args.clone())?;
        let book = &args[0];
        let side = ascii(&args[1]);
        let price = args[2].number();
        let amount = args[3].number();
        let V::Array(levels) = book.get(&side) else {
            return Err(E { original: false });
        };
        let mut levels = levels.borrow_mut();
        let mut left = 0i64;
        let mut right = levels.len() as i64 - 1;
        let mut found = None;
        while left <= right {
            let mid = (left + right) / 2;
            let p = levels[mid as usize].get("price").number();
            if p == price {
                found = Some(mid as usize);
                break;
            }
            if if side == "bids" { p > price } else { p < price } {
                left = mid + 1
            } else {
                right = mid - 1
            }
        }
        if let Some(i) = found {
            if amount == 0.0 {
                levels.remove(i);
            } else {
                levels[i].set(&u("amount"), V::Num(amount));
            }
        } else if amount != 0.0 {
            levels.insert(
                left as usize,
                obj(vec![
                    ("price", V::Num(price)),
                    ("amount", V::Num(amount)),
                    ("total", V::Num(0.0)),
                ]),
            );
        }
        Ok(V::Undefined)
    }
    fn process_original(&mut self, book: V, data: V) -> Result<(), E> {
        for side in ["bids", "asks"] {
            let values = data.get(side);
            if !values.truthy() {
                continue;
            }
            let rows = if let V::Array(_) = values {
                values.rows()
            } else {
                vec![]
            };
            for row in rows {
                let v = row.rows();
                let p = v.first().cloned().unwrap_or(V::Undefined).parse();
                let a = v.get(1).cloned().unwrap_or(V::Undefined).parse();
                self.update_by_price(vec![book.clone(), txt(side), V::Num(p), V::Num(a)])?;
            }
        }
        Ok(())
    }
}
struct Read {
    output: Option<Result<V, E>>,
    pending: Rc<Cell<usize>>,
    wait: bool,
}
impl Future for Read {
    type Output = Result<V, E>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.wait {
            self.wait = false;
            self.pending.set(self.pending.get() + 1);
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(self.output.take().unwrap())
        }
    }
}
impl WsHost for Host {
    type V = V;
    type E = E;
    fn get(&mut self, v: &V, key: &[u16]) -> Result<V, E> {
        if matches!(v, V::Null | V::Undefined) {
            Err(E { original: false })
        } else {
            Ok(v.prop(key))
        }
    }
    fn optional(&mut self, v: &V, key: &[u16]) -> Result<V, E> {
        Ok(v.prop(key))
    }
    fn set(&mut self, v: &V, key: &[u16], new: V) -> Result<(), E> {
        v.set(key, new);
        Ok(())
    }
    fn remove(&mut self, v: &V, key: &[u16]) -> Result<(), E> {
        if let V::Object(o) = v {
            o.borrow_mut().retain(|(k, _)| k != key)
        }
        Ok(())
    }
    fn truthy(&mut self, v: &V) -> bool {
        v.truthy()
    }
    fn nullish(&mut self, v: &V) -> bool {
        matches!(v, V::Null | V::Undefined)
    }
    fn same(&mut self, a: &V, b: &V) -> bool {
        eq(a, b)
    }
    fn set_same(&mut self, a: &V, b: &V) -> bool {
        eq(a, b) || matches!((a,b),(V::Num(a),V::Num(b))if a.is_nan()&&b.is_nan())
    }
    fn number(&mut self, v: V) -> Result<f64, E> {
        Ok(v.number())
    }
    fn parse(&mut self, v: V) -> Result<f64, E> {
        Ok(v.parse())
    }
    fn text(&mut self, v: V) -> Result<Vec<u16>, E> {
        Ok(v.text())
    }
    fn lower(&mut self, v: V) -> Result<Vec<u16>, E> {
        let V::Text(s) = v else {
            return Err(E { original: false });
        };
        let mut out = vec![];
        for c in char::decode_utf16(s) {
            match c {
                Ok(c) => {
                    for c in c.to_lowercase() {
                        let mut b = [0; 2];
                        out.extend_from_slice(c.encode_utf16(&mut b));
                    }
                }
                Err(e) => out.push(e.unpaired_surrogate()),
            }
        }
        Ok(out)
    }
    fn num(&mut self, v: f64) -> V {
        V::Num(v)
    }
    fn string(&mut self, v: Vec<u16>) -> V {
        V::Text(v)
    }
    fn undefined(&mut self) -> V {
        V::Undefined
    }
    fn boolean(&mut self, v: bool) -> V {
        V::Bool(v)
    }
    fn array(&mut self, v: Vec<V>) -> V {
        arr(v)
    }
    fn object(&mut self, v: Vec<(&'static str, V)>) -> V {
        obj(v)
    }
    fn spread(&mut self, v: &V) -> Result<V, E> {
        Ok(if let V::Object(o) = v {
            V::Object(Rc::new(RefCell::new(o.borrow().clone())))
        } else {
            obj(vec![])
        })
    }
    fn rows(&mut self, v: V) -> Result<Vec<V>, E> {
        if let V::Array(a) = v {
            Ok(a.borrow().clone())
        } else {
            Err(E { original: false })
        }
    }
    fn values(&mut self, v: &V) -> Result<Vec<V>, E> {
        Ok(if let V::Object(o) = v {
            o.borrow().iter().map(|(_, v)| v.clone()).collect()
        } else {
            vec![]
        })
    }
    fn at(&mut self, v: &V, i: usize) -> Result<V, E> {
        if let V::Array(a) = v {
            Ok(a.borrow().get(i).cloned().unwrap_or(V::Undefined))
        } else {
            Err(E { original: false })
        }
    }
    fn is_array(&mut self, v: &V) -> bool {
        matches!(v, V::Array(_))
    }
    fn store(&mut self, name: &'static str) -> Result<V, E> {
        Ok(self.store_value().get(name))
    }
    fn field(&mut self, name: &'static str) -> Result<V, E> {
        Ok(self.fields().get(name))
    }
    fn call(&mut self, name: &'static str, args: Vec<V>) -> Result<V, E> {
        match name {
            "boundTicker" => return Ok(txt("bound")),
            "newParser" => {
                let mut s = self.state.borrow_mut();
                s.parser += 1;
                return Ok(V::Num(s.parser as f64));
            }
            "orderStatus" => {
                let key = ascii(&args[0]);
                return Ok(match key.as_str() {
                    "new" | "untriggered" | "partially_filled" | "partiallyfilled" => txt("open"),
                    "filled" | "triggered" => txt("closed"),
                    "rejected" | "canceled" => txt("canceled"),
                    _ => V::Undefined,
                });
            }
            "roundUSD" => {
                return Ok(V::Num(
                    safe_cex_num::math_round(args[0].number() * 100.0) / 100.0,
                ));
            }
            "lodashAdd" => {
                let a = &args[0];
                let b = &args[1];
                return Ok(if matches!(b, V::Undefined) {
                    a.clone()
                } else if matches!(a, V::Undefined) {
                    b.clone()
                } else if matches!(a, V::Text(_)) || matches!(b, V::Text(_)) {
                    V::Text(a.text().into_iter().chain(b.text()).collect())
                } else {
                    V::Num(a.number() + b.number())
                });
            }
            "greater" => {
                return Ok(V::Bool(
                    if let (V::Text(a), V::Text(b)) = (&args[0], &args[1]) {
                        a > b
                    } else {
                        args[0].number() > args[1].number()
                    },
                ));
            }
            "nextId" => {
                let fields = self.fields();
                let id = fields.get("requestId");
                fields.set(&u("requestId"), V::Num(id.number() + 1.0));
                return Ok(id);
            }
            "addHandler" => {
                let fields = self.fields().get("handlers");
                let key = args[0].text();
                let old = fields.prop(&key);
                let mut old = old.rows();
                old.push(args[1].clone());
                fields.set(&key, arr(old));
                return Ok(V::Undefined);
            }
            "removeHandler" => {
                let fields = self.fields().get("handlers");
                let key = args[0].text();
                let old = fields.prop(&key);
                fields.set(
                    &key,
                    arr(old
                        .rows()
                        .into_iter()
                        .filter(|v| !eq(v, &args[1]))
                        .collect()),
                );
                return Ok(V::Undefined);
            }
            "handlerCount" => {
                return Ok(V::Num(
                    self.fields()
                        .get("handlers")
                        .prop(&args[0].text())
                        .rows()
                        .len() as f64,
                ));
            }
            "push" => {
                if let V::Array(a) = &args[0] {
                    a.borrow_mut().push(args[1].clone())
                }
                return Ok(V::Undefined);
            }
            "updateByPrice" => return self.update_by_price(args),
            _ => {}
        }
        if name == "send" || name == "sendWithLog" {
            self.effect("serialize", args.clone())?;
            let request = wire(&args[0], false);
            self.effect(name, vec![request])?;
            return Ok(V::Undefined);
        }
        self.effect(name, args.clone())?;
        Ok(match name {
            "formatIn" => args[0].clone(),
            "mapPacked" => self.packed(args[0].clone()),
            "mapOrder" => {
                if !matches!(self.input.get("mappedOrders"), V::Undefined) {
                    self.input.get("mappedOrders")
                } else {
                    let o = &args[0];
                    let raw = o.get("ordStatus");
                    let raw = V::Text(self.lower(raw)?);
                    let s = self.call("orderStatus", vec![raw])?;
                    let s = if s.truthy() { s } else { txt("canceled") };
                    arr(vec![obj(vec![
                        ("id", o.get("orderID")),
                        ("status", s),
                        ("side", o.get("side")),
                        ("symbol", o.get("symbol")),
                        ("price", o.get("price")),
                        ("filled", o.get("filled")),
                        ("info", o.clone()),
                    ])])
                }
            }
            "mapPosition" => {
                if args[0].get("skip").truthy() {
                    V::Undefined
                } else {
                    args[0].clone()
                }
            }
            "parseFrame" => parse_frame(&args[0].text()),
            "update" => {
                let update = &args[0];
                let store = self.store_value();
                if let V::Object(o) = update {
                    for (k, v) in o.borrow().iter() {
                        store.set(k, v.clone())
                    }
                }
                let tickers = update.get("tickers");
                if tickers.truthy() {
                    for t in tickers.rows() {
                        store.get("tickersBySymbol").set(&t.get("symbol").text(), t)
                    }
                }
                V::Undefined
            }
            "updateTicker" => {
                if let V::Object(o) = &args[1] {
                    for (k, v) in o.borrow().iter() {
                        args[0].set(k, v.clone())
                    }
                }
                V::Undefined
            }
            "upsertOrders" => {
                self.store_value().set(&u("orders"), args[0].clone());
                V::Undefined
            }
            "upsertPositions" => {
                let store = self.store_value();
                let mut current = store.get("positions").rows();
                for p in args[0].rows() {
                    if let Some(i) = current
                        .iter()
                        .position(|old| eq(&old.get("id"), &p.get("id")))
                    {
                        current[i] = p
                    } else {
                        current.push(p)
                    }
                }
                store.set(&u("positions"), arr(current));
                V::Undefined
            }
            "processBook" => {
                self.process_original(args[0].clone(), args[1].clone())?;
                V::Undefined
            }
            "calcTotal" => {
                for side in ["bids", "asks"] {
                    let mut total = 0.0;
                    for level in args[0].get(side).rows() {
                        total += level.get("amount").number();
                        level.set(&u("total"), V::Num(total))
                    }
                }
                V::Undefined
            }
            "callback" => {
                let book = args[1].clone();
                if eq(&self.input.get("family"), &txt("book")) {
                    let mut s = self.state.borrow_mut();
                    if !s.books.iter().any(|b| eq(b, &book)) {
                        s.books.push(book.clone())
                    }
                    if !self.input.get("duplicate").truthy() {
                        assert!(eq(&s.books[0], &book), "WS native book callback identity")
                    }
                }
                V::Undefined
            }
            "nanoid" => {
                let mut s = self.state.borrow_mut();
                s.serial += 1;
                txt(&format!("inner{}", s.serial))
            }
            "dateNow" => {
                let mut s = self.state.borrow_mut();
                let now = s.now;
                let step = self.input.get("clockStep");
                s.now += if step.truthy() { step.number() } else { 0.0 };
                V::Num(now)
            }
            "clearTimer" => {
                self.state
                    .borrow_mut()
                    .timers
                    .retain(|(id, _)| *id as f64 != args[0].number());
                V::Undefined
            }
            "received" | "diagnostic" | "pong" | "stats" | "cleanup" | "send" | "sendWithLog"
            | "error" | "fill" | "sync" | "warning" | "log" => V::Undefined,
            _ => panic!("unknown host member {name}"),
        })
    }
    fn read(
        &mut self,
        endpoint: &'static str,
        params: V,
    ) -> Result<Pin<Box<dyn Future<Output = Result<V, E>>>>, E> {
        self.effect("get", vec![txt(endpoint), params])?;
        let reply = if matches!(self.input.get("reply"), V::Undefined) {
            obj(vec![(
                "data",
                obj(vec![(
                    "result",
                    obj(vec![
                        ("sequence", V::Num(10.0)),
                        (
                            "orderbook_p",
                            obj(vec![
                                ("bids", arr(vec![arr(vec![txt("10"), txt("2")])])),
                                ("asks", arr(vec![arr(vec![txt("12"), txt("2")])])),
                            ]),
                        ),
                    ]),
                )]),
            )])
        } else {
            self.input.get("reply")
        };
        let output = if self.input.get("reject").truthy() {
            Err(E { original: true })
        } else {
            Ok(reply)
        };
        Ok(Box::pin(Read {
            output: Some(output),
            pending: self.pending.clone(),
            wait: true,
        }))
    }
    fn spawn(&mut self, mut task: Task) -> Result<(), E> {
        match poll(&mut task) {
            Poll::Ready(v) => v,
            Poll::Pending => {
                self.state.borrow_mut().tasks.push(task);
                Ok(())
            }
        }
    }
    fn timer(&mut self, delay: f64, task: Timer) -> Result<V, E> {
        let id = {
            let mut s = self.state.borrow_mut();
            s.timer_id += 1;
            s.timer_id
        };
        self.effect("timer", vec![V::Num(delay), V::Num(id as f64)])?;
        self.state.borrow_mut().timers.push((id, task));
        Ok(V::Num(id as f64))
    }
    fn error_text(&mut self, error: E) -> Result<Vec<u16>, E> {
        Ok(u(if error.original {
            "boom"
        } else {
            "Cannot read properties of null (reading 'map')"
        }))
    }
}
fn wire(v: &V, in_array: bool) -> V {
    match v {
        V::Undefined => {
            if in_array {
                V::Null
            } else {
                V::Undefined
            }
        }
        V::Num(n) if !n.is_finite() => V::Null,
        V::Num(n) if *n == 0.0 => V::Num(0.0),
        V::Array(a) => arr(a.borrow().iter().map(|v| wire(v, true)).collect()),
        V::Object(o) => V::Object(Rc::new(RefCell::new(
            o.borrow()
                .iter()
                .filter(|(_, v)| !matches!(v, V::Undefined))
                .map(|(k, v)| (k.clone(), wire(v, false)))
                .collect(),
        ))),
        _ => v.clone(),
    }
}
fn parse_frame(raw: &[u16]) -> V {
    let raw = String::from_utf16_lossy(raw);
    let chars = raw.chars().collect::<Vec<_>>();
    let mut i = 0;
    let mut out = String::new();
    while i < chars.len() {
        if chars[i] != '"' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        let mut units = vec![];
        let mut valid = true;
        while i < chars.len() && chars[i] != '"' {
            let c = chars[i];
            i += 1;
            if c == '\\' {
                if i >= chars.len() {
                    valid = false;
                    break;
                }
                let e = chars[i];
                i += 1;
                if e == 'u' {
                    if i + 4 > chars.len() {
                        valid = false;
                        break;
                    }
                    let hex = chars[i..i + 4].iter().collect::<String>();
                    let Ok(n) = u16::from_str_radix(&hex, 16) else {
                        valid = false;
                        break;
                    };
                    units.push(n);
                    i += 4
                } else {
                    let c = match e {
                        '"' => '"',
                        '\\' => '\\',
                        '/' => '/',
                        'b' => '\x08',
                        'f' => '\x0c',
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        _ => {
                            valid = false;
                            break;
                        }
                    };
                    units.extend(c.encode_utf16(&mut [0; 2]).iter().copied());
                }
            } else {
                units.extend(c.encode_utf16(&mut [0; 2]).iter().copied());
            }
        }
        if !valid || i >= chars.len() {
            return V::Null;
        }
        i += 1;
        let next = chars[i..].iter().find(|c| !c.is_whitespace());
        if next == Some(&':') {
            out.extend(chars[start..i].iter())
        } else {
            out.push_str(&json!({"$units":units}).to_string())
        }
    }
    serde_json::from_str::<Value>(&out)
        .map(|v| V::decode(&v))
        .unwrap_or(V::Null)
}
fn run(name: &str, input: V) -> (Host, V) {
    let mut host = Host::new(input.clone());
    let result = (|| -> Result<V, E> {
        match name {
            "public_class" => {
                let s = ws::PublicSocket::new(&mut host)?;
                let V::Object(o) = s.message_handlers else {
                    panic!()
                };
                let handlers = V::Object(Rc::new(RefCell::new(
                    o.borrow()
                        .iter()
                        .map(|(k, v)| (k.clone(), V::Num(v.rows().len() as f64)))
                        .collect(),
                )));
                Ok(obj(vec![
                    (
                        "maxSubscriptions",
                        V::Num(ws::PublicSocket::<V>::MAX_SUBSCRIPTIONS),
                    ),
                    (
                        "warningThreshold",
                        V::Num(ws::PublicSocket::<V>::SUBSCRIPTION_WARNING_THRESHOLD),
                    ),
                    (
                        "logInterval",
                        V::Num(ws::PublicSocket::<V>::SNAPSHOT_ERROR_LOG_INTERVAL_MS),
                    ),
                    ("hasWarned", V::Bool(s.has_warned_subscription_limit)),
                    ("snapshotLogs", s.snapshot_error_log_at),
                    ("handlers", handlers),
                    ("topics", s.topics),
                    ("requestId", V::Num(s.request_id)),
                    (
                        "klineCache",
                        s.kline_interval_by_resolution.unwrap_or(V::Undefined),
                    ),
                ]))
            }
            "private_class" => {
                let s = ws::PrivateSocket::<V>::default();
                Ok(obj(vec![
                    ("requestId", V::Num(s.request_id)),
                    ("timeout", s.sync_timeout.unwrap_or(V::Undefined)),
                    ("symbols", arr(s.sync_symbols)),
                    ("debounce", V::Num(s.sync_debounce_ms)),
                ]))
            }
            "packed" => ws::map_packed_ticker_changes(&mut host, input.get("row")),
            "ticker" => {
                ws::handle_ticker_stream_events(&mut host, input.get("events"))?;
                Ok(V::Undefined)
            }
            "private" => {
                let frame = input.get("raw");
                ws::private_on_message(&mut host, frame)?;
                Ok(V::Undefined)
            }
            "delta" => {
                let book = input.get("book");
                ws::process_order_book_update(&mut host, book.clone(), input.get("update"))?;
                Ok(book)
            }
            "candles" | "book" => {
                let symbol = if matches!(input.get("symbol"), V::Undefined) {
                    txt("BTCUSDT")
                } else {
                    input.get("symbol")
                };
                let callback = txt("cb");
                if name == "candles" {
                    let interval = if matches!(input.get("interval"), V::Undefined) {
                        txt("1m")
                    } else {
                        input.get("interval")
                    };
                    let opts = obj(vec![("symbol", symbol), ("interval", interval)]);
                    let mut subscriptions =
                        vec![ws::listen_ohlcv(&mut host, opts.clone(), callback.clone())?];
                    if input.get("duplicate").truthy() {
                        subscriptions.push(ws::listen_ohlcv(&mut host, opts, callback)?)
                    }
                    if input.get("disposeBeforeTimer").truthy() {
                        subscriptions[0].dispose(&mut host)?
                    }
                    if input.get("runTimer").truthy() {
                        host.fields().set(&u("connected"), V::Bool(true));
                        host.run_timer()?
                    }
                    let rows = if matches!(input.get("rows"), V::Undefined | V::Null) {
                        arr(vec![])
                    } else {
                        input.get("rows")
                    };
                    let handler = host
                        .fields()
                        .get("handlers")
                        .prop(&subscriptions[0].handler);
                    for parser in handler.rows() {
                        if let Some(s) = subscriptions.iter().find(|s| eq(&s.parser, &parser)) {
                            s.parse(&mut host, rows.clone())?
                        }
                    }
                    if input.get("dispose").truthy() {
                        subscriptions[0].dispose(&mut host)?
                    }
                } else {
                    let mut subscriptions = vec![ws::listen_order_book(
                        &mut host,
                        symbol.clone(),
                        callback.clone(),
                    )?];
                    if input.get("duplicate").truthy() {
                        subscriptions.push(ws::listen_order_book(&mut host, symbol, callback)?)
                    }
                    if input.get("disposeBeforeTimer").truthy() {
                        subscriptions[0].dispose(&mut host)?
                    }
                    if input.get("runTimer").truthy() {
                        host.fields().set(&u("connected"), V::Bool(true));
                        host.run_timer()?
                    }
                    for update in input.get("updates").rows() {
                        for s in &subscriptions {
                            s.parse(&mut host, update.clone())?
                        }
                        if input.get("flushEach").truthy() {
                            host.drain()?
                        }
                    }
                    if input.get("disposeBeforeResolve").truthy() {
                        subscriptions[0].dispose(&mut host)?
                    }
                    if input.get("disposedBeforeResolve").truthy() {
                        host.fields().set(&u("disposed"), V::Bool(true));
                    }
                    host.drain()?;
                    if input.get("dispose").truthy() {
                        subscriptions[0].dispose(&mut host)?
                    }
                }
                let fields = host.fields();
                let books = arr(host.state.borrow().books.clone());
                Ok(obj(vec![
                    ("topics", fields.get("topics")),
                    ("handlers", host.handler_counts()),
                    ("inner", fields.get("orderBookInnerState")),
                    ("logs", fields.get("snapshotLogs")),
                    ("books", books),
                ]))
            }
            _ => panic!(),
        }
    })();
    let (result, thrown) = match result {
        Ok(v) => (v, V::Undefined),
        Err(e) => (
            V::Undefined,
            txt(if e.original { "original" } else { "intrinsic" }),
        ),
    };
    let effects = arr(host.state.borrow().effects.clone());
    let observation = obj(vec![
        ("effects", effects),
        ("result", result),
        ("thrown", thrown),
        ("state", host.store_value()),
    ]);
    (host, observation)
}
fn oracle(name: &str) -> Vec<Value> {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/websocket-data-oracle.cjs");
    let output = Command::new("node").arg(path).arg(name).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn verify(name: &str, floor: usize) {
    let cases = oracle(name);
    assert_eq!(cases.len(), floor, "WS source case floor");
    for (i, case) in cases.iter().enumerate() {
        let input = V::decode(&case["input"]);
        input.set(&u("family"), txt(name));
        let (host, native) = run(name, input);
        let actual = native.obs();
        let expected = &case["expected"];
        let ae = actual["object"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v[0] == json!({"units":u("effects")}))
            .unwrap()[1]
            .as_array()
            .unwrap();
        let ee = expected["object"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v[0] == json!({"units":u("effects")}))
            .unwrap()[1]
            .as_array()
            .unwrap();
        assert_eq!(ae.len(), ee.len(), "WS native effect count {name} {i}");
        for (a, e) in ae.iter().zip(ee) {
            assert_eq!(a, e, "WS native individual effect {name} {i}")
        }
        assert_eq!(
            &actual, expected,
            "WS native complete observation {name} {i}"
        );
        if name == "book"
            && !eq(&host.input.get("failure"), &txt("get"))
            && host
                .state
                .borrow()
                .effects
                .iter()
                .any(|v| eq(&v.rows()[0], &txt("get")))
        {
            assert!(host.pending.get() > 0, "WS native actual Pending");
        }
    }
}
#[test]
fn actual_typescript_public_class() {
    verify("public_class", 1)
}
#[test]
fn actual_typescript_private_class() {
    verify("private_class", 1)
}
#[test]
fn actual_typescript_packed() {
    verify("packed", 7)
}
#[test]
fn actual_typescript_ticker() {
    verify("ticker", 9)
}
#[test]
fn actual_typescript_private() {
    verify("private", 31)
}
#[test]
fn actual_typescript_candles() {
    verify("candles", 11)
}
#[test]
fn actual_typescript_delta() {
    verify("delta", 6)
}
#[test]
fn actual_typescript_book() {
    verify("book", 17)
}
