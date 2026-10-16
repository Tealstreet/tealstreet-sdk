use safe_cex_venue_phemex::rest_data::{self as data, DataHost};
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
    Object(Rc<Vec<(Vec<u16>, V)>>),
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn array(v: Vec<V>) -> V {
    V::Array(Rc::new(RefCell::new(v)))
}
fn object(v: Vec<(&str, V)>) -> V {
    V::Object(Rc::new(v.into_iter().map(|(k, v)| (u(k), v)).collect()))
}
impl V {
    fn decode(v: &Value) -> Self {
        if let Some(a) = v.get("$units").and_then(Value::as_array) {
            return Self::Text(a.iter().map(|v| v.as_u64().unwrap() as u16).collect());
        }
        if let Some(bits) = v.get("$bits").and_then(Value::as_str) {
            return Self::Num(f64::from_bits(u64::from_str_radix(bits, 16).unwrap()));
        }
        if v.get("$undefined").is_some() {
            return Self::Undefined;
        }
        match v {
            Value::Null => Self::Null,
            Value::Bool(b) => Self::Bool(*b),
            Value::Number(n) => Self::Num(n.as_f64().unwrap()),
            Value::String(s) => Self::Text(u(s)),
            Value::Array(a) => array(a.iter().map(Self::decode).collect()),
            Value::Object(o) => Self::Object(Rc::new(
                o.iter().map(|(k, v)| (u(k), Self::decode(v))).collect(),
            )),
        }
    }
    fn get(&self, key: &str) -> V {
        match self {
            Self::Object(o) => o
                .iter()
                .find(|(k, _)| *k == u(key))
                .map(|(_, v)| v.clone())
                .unwrap_or(Self::Undefined),
            _ => Self::Undefined,
        }
    }
    fn truthy(&self) -> bool {
        match self {
            Self::Undefined | Self::Null => false,
            Self::Bool(b) => *b,
            Self::Num(n) => *n != 0.0 && !n.is_nan(),
            Self::Text(s) => !s.is_empty(),
            _ => true,
        }
    }
    fn text(&self) -> Option<Vec<u16>> {
        if let Self::Text(s) = self {
            Some(s.clone())
        } else {
            None
        }
    }
    fn values(&self) -> Vec<V> {
        if let Self::Array(v) = self {
            v.borrow().clone()
        } else {
            vec![]
        }
    }
    fn number(&self) -> f64 {
        match self {
            Self::Undefined => f64::NAN,
            Self::Null => 0.0,
            Self::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            Self::Num(n) => *n,
            Self::Text(s) => String::from_utf16(s)
                .map(|s| safe_cex_num::number_from_string(&s))
                .unwrap_or(f64::NAN),
            Self::Object(_) => f64::NAN,
            Self::Array(a) => {
                let a = a.borrow();
                if a.is_empty() {
                    0.0
                } else if a.len() == 1 {
                    a[0].number()
                } else {
                    f64::NAN
                }
            }
        }
    }
    fn obs(&self) -> Value {
        match self {
            Self::Undefined => json!({"undefined":true}),
            Self::Null => Value::Null,
            Self::Bool(b) => json!(b),
            Self::Num(n) => {
                json!({"bits":format!("{:016x}",if n.is_nan(){f64::NAN.to_bits()}else{n.to_bits()})})
            }
            Self::Text(s) => json!({"units":s}),
            Self::Array(a) => Value::Array(a.borrow().iter().map(Self::obs).collect()),
            Self::Object(o) => {
                let mut entries = o
                    .iter()
                    .map(|(k, v)| json!([{"units":k},v.obs()]))
                    .collect::<Vec<_>>();
                entries.sort_by_key(|a| a[0].to_string());
                json!({"object":entries})
            }
        }
    }
}
#[derive(Clone, Debug)]
struct E {
    value: V,
    token: Rc<()>,
}
struct Deferred {
    output: Option<Result<V, E>>,
    wait: usize,
    effects: Rc<RefCell<Vec<V>>>,
    currency: Option<V>,
    pending: Rc<Cell<usize>>,
}
impl Future for Deferred {
    type Output = Result<V, E>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.wait > 0 {
            self.wait -= 1;
            self.pending.set(self.pending.get() + 1);
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        if let Some(currency) = self.currency.take() {
            self.effects
                .borrow_mut()
                .push(array(vec![V::Text(u("settle")), currency]));
            self.pending.set(self.pending.get() + 1);
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        Poll::Ready(self.output.take().unwrap())
    }
}
fn complete<F: Future>(f: F) -> F::Output {
    let mut f = Box::pin(f);
    let waker = Waker::noop().clone();
    let mut cx = Context::from_waker(&waker);
    for _ in 0..100 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("RESTDATA bounded continuation")
}
fn has(v: &V, key: &str) -> bool {
    matches!(v,V::Object(o)if o.iter().any(|(k,_)|*k==u(key)))
}
fn text(v: V) -> Vec<u16> {
    match v {
        V::Undefined => u("undefined"),
        V::Null => u("null"),
        V::Text(s) => s,
        V::Num(n) => u(&safe_cex_num::js_number_string(n)),
        V::Bool(b) => u(if b { "true" } else { "false" }),
        _ => u("[object Object]"),
    }
}
fn ascii(v: &V) -> String {
    String::from_utf16_lossy(&text(v.clone()))
}
struct Host {
    input: V,
    name: String,
    effects: Rc<RefCell<Vec<V>>>,
    original: Rc<()>,
    pending: Rc<Cell<usize>>,
    read_index: usize,
    now: f64,
    markets: V,
    orders: V,
    positions: V,
    balance: V,
}
impl Host {
    fn new(input: V, name: &str) -> Self {
        let orders = if has(&input, "orders") {
            input.get("orders")
        } else {
            array(vec![])
        };
        let positions = if has(&input, "storedPositions") {
            input.get("storedPositions")
        } else {
            array(vec![])
        };
        let currencies = if has(&input, "previous") {
            input.get("previous")
        } else {
            object(vec![])
        };
        Self {
            input,
            name: name.into(),
            effects: Rc::new(RefCell::new(vec![])),
            original: Rc::new(()),
            pending: Rc::new(Cell::new(0)),
            read_index: 0,
            now: 1000.0,
            markets: array(vec![object(vec![("id", V::Text(u("cached")))])]),
            orders,
            positions,
            balance: object(vec![("total", V::Num(99.0)), ("currencies", currencies)]),
        }
    }
    fn error_original(&self) -> E {
        E {
            value: object(vec![
                ("message", V::Text(u("boom"))),
                (
                    "response",
                    object(vec![
                        ("msg", V::Text(u("market-nested"))),
                        ("data", object(vec![("msg", V::Text(u("nested")))])),
                    ]),
                ),
            ]),
            token: self.original.clone(),
        }
    }
    fn intrinsic(&self, message: String) -> E {
        E {
            value: object(vec![("message", V::Text(u(&message)))]),
            token: Rc::new(()),
        }
    }
    fn effect_units(&mut self, name: Vec<u16>, args: Vec<V>) -> Result<(), E> {
        let failure = self.input.get("failure").text();
        let mut row = vec![V::Text(name.clone())];
        row.extend(args);
        self.effects.borrow_mut().push(array(row));
        if failure == Some(name) {
            Err(self.error_original())
        } else {
            Ok(())
        }
    }
    fn effect(&mut self, name: &str, args: Vec<V>) -> Result<(), E> {
        self.effect_units(u(name), args)
    }
    fn deferred(&self, output: Result<V, E>) -> Deferred {
        Deferred {
            output: Some(output),
            wait: 1,
            effects: self.effects.clone(),
            currency: None,
            pending: self.pending.clone(),
        }
    }
    fn market() -> V {
        object(vec![
            ("symbol", V::Text(u("BTCUSDT"))),
            ("quote", V::Text(u("USDT"))),
        ])
    }
}
impl DataHost for Host {
    type V = V;
    type E = E;
    type Call = ();
    type Read = Deferred;
    fn get(&mut self, v: &V, key: &str) -> Result<V, E> {
        if self.name == "candles" && key == "from" && self.input.get("fromThrows").truthy() {
            return Err(self.error_original());
        }
        if matches!(v, V::Null | V::Undefined) {
            return Err(self.intrinsic(format!(
                "Cannot read properties of {} (reading '{key}')",
                if matches!(v, V::Null) {
                    "null"
                } else {
                    "undefined"
                }
            )));
        }
        Ok(v.get(key))
    }
    fn optional(&mut self, v: &V, key: &str) -> Result<V, E> {
        Ok(v.get(key))
    }
    fn key(&mut self, v: &V, key: &str) -> Result<V, E> {
        if has(v, key) {
            return Ok(v.get(key));
        }
        let mut snake = String::new();
        for (i, c) in key.chars().enumerate() {
            if c.is_ascii_uppercase() {
                if i > 0 {
                    snake.push('_')
                }
                snake.push(c.to_ascii_lowercase())
            } else {
                snake.push(c)
            }
        }
        Ok(v.get(&snake))
    }
    fn truthy(&mut self, v: &V) -> Result<bool, E> {
        Ok(v.truthy())
    }
    fn equals(&mut self, a: &V, b: &V) -> Result<bool, E> {
        Ok(match (a, b) {
            (V::Num(a), V::Num(b)) => a == b,
            (V::Object(a), V::Object(b)) => Rc::ptr_eq(a, b),
            (V::Array(a), V::Array(b)) => Rc::ptr_eq(a, b),
            _ => a.obs() == b.obs(),
        })
    }
    fn is_text(&mut self, v: &V) -> Result<bool, E> {
        Ok(matches!(v, V::Text(_)))
    }
    fn coerce(&mut self, v: V) -> Result<f64, E> {
        Ok(v.number())
    }
    fn number_value(&mut self, v: &V) -> Result<Option<f64>, E> {
        Ok(if let V::Num(n) = v { Some(*n) } else { None })
    }
    fn parse(&mut self, v: V) -> Result<f64, E> {
        Ok(safe_cex_num::parse_float(&String::from_utf16_lossy(&text(
            v,
        ))))
    }
    fn text(&mut self, v: V) -> Result<Vec<u16>, E> {
        Ok(text(v))
    }
    fn upper(&mut self, s: Vec<u16>) -> Result<Vec<u16>, E> {
        let mut out = vec![];
        for c in char::decode_utf16(s) {
            match c {
                Ok(c) => {
                    for c in c.to_uppercase() {
                        let mut b = [0; 2];
                        out.extend_from_slice(c.encode_utf16(&mut b));
                    }
                }
                Err(e) => out.push(e.unpaired_surrogate()),
            }
        }
        Ok(out)
    }
    fn interval_text(&mut self, v: V) -> Result<Vec<u16>, E> {
        if let V::Text(s) = v {
            Ok(s)
        } else {
            Err(self.intrinsic(format!(
                "Cannot read properties of {} (reading 'split')",
                if matches!(v, V::Null) {
                    "null"
                } else {
                    "undefined"
                }
            )))
        }
    }
    fn lower(&mut self, s: Vec<u16>) -> Result<Vec<u16>, E> {
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
    fn new_text(&mut self, s: Vec<u16>) -> V {
        V::Text(s)
    }
    fn num(&mut self, n: f64) -> V {
        V::Num(n)
    }
    fn boolean(&mut self, b: bool) -> V {
        V::Bool(b)
    }
    fn undefined(&mut self) -> V {
        V::Undefined
    }
    fn array(&mut self, v: Vec<V>) -> Result<V, E> {
        Ok(array(v))
    }
    fn object(&mut self, v: Vec<(&'static str, V)>) -> Result<V, E> {
        Ok(object(v))
    }
    fn spread(&mut self, v: &V, fields: Vec<(&'static str, V)>) -> Result<V, E> {
        let V::Object(v) = v else { panic!() };
        let mut out = v.as_ref().clone();
        for (k, v) in fields {
            if let Some((_, old)) = out.iter_mut().find(|(key, _)| *key == u(k)) {
                *old = v
            } else {
                out.push((u(k), v))
            }
        }
        Ok(V::Object(Rc::new(out)))
    }
    fn rows(&mut self, v: V) -> Result<Vec<V>, E> {
        if let V::Array(v) = v {
            Ok(v.borrow().clone())
        } else {
            let method = if self.name == "markets" {
                "filter"
            } else {
                "map"
            };
            let message = if matches!(v, V::Null | V::Undefined) {
                format!(
                    "Cannot read properties of {} (reading '{method}')",
                    if matches!(v, V::Null) {
                        "null"
                    } else {
                        "undefined"
                    }
                )
            } else {
                format!("rows.{method} is not a function")
            };
            Err(self.intrinsic(message))
        }
    }
    fn is_array(&mut self, v: &V) -> Result<bool, E> {
        Ok(matches!(v, V::Array(_)))
    }
    fn length(&mut self, v: &V) -> Result<usize, E> {
        Ok(v.values().len())
    }
    fn at(&mut self, v: &V, i: usize) -> Result<V, E> {
        Ok(v.values().get(i).cloned().unwrap_or(V::Undefined))
    }
    fn store(&mut self, key: &str) -> Result<V, E> {
        Ok(match key {
            "markets" => self.markets.clone(),
            "orders" => self.orders.clone(),
            "positions" => self.positions.clone(),
            "balance" => self.balance.clone(),
            _ => panic!(),
        })
    }
    fn cached(&mut self, table: &str, symbol: &V) -> Result<V, E> {
        if ascii(symbol) != "BTCUSDT" {
            return Ok(V::Undefined);
        }
        Ok(if table == "marketsBySymbol" {
            Self::market()
        } else {
            self.input.get("ticker")
        })
    }
    fn format_in(&mut self, symbol: V) -> Result<V, E> {
        self.effect("formatIn", vec![symbol.clone()])?;
        Ok(symbol)
    }
    fn format_out(&mut self, symbol: V) -> Result<V, E> {
        self.effect("formatOut", vec![symbol.clone()])?;
        Ok(symbol)
    }
    fn table(&mut self, table: &str, key: V) -> Result<V, E> {
        let key = ascii(&key);
        let value = match table {
            "ORDER_STATUS" => match key.as_str() {
                "new" | "untriggered" | "partiallyfilled" | "partially_filled" => Some("open"),
                "filled" | "triggered" => Some("closed"),
                "canceled" | "rejected" => Some("canceled"),
                _ => None,
            },
            "TIME_IN_FORCE" => match key.as_str() {
                "GoodTillCancel" => Some("GoodTillCancel"),
                "ImmediateOrCancel" => Some("ImmediateOrCancel"),
                "FillOrKill" => Some("FillOrKill"),
                _ => None,
            },
            "timeframes" => match key.as_str() {
                "1m" => Some("60"),
                "1h" => Some("3600"),
                "1d" => Some("86400"),
                _ => None,
            },
            _ => panic!(),
        };
        Ok(value.map(|s| V::Text(u(s))).unwrap_or(V::Undefined))
    }
    fn position_id(&mut self, symbol: &V, side: &str, mode: &str) -> Result<V, E> {
        Ok(if mode == "oneway" {
            symbol.clone()
        } else {
            V::Text(
                text(symbol.clone())
                    .into_iter()
                    .chain([58])
                    .chain(u(side))
                    .collect(),
            )
        })
    }
    fn price_diff(&mut self, entry: f64, last: f64, side: &str) -> Result<f64, E> {
        Ok(if side == "long" {
            safe_cex_num::subtract(last, entry)
        } else {
            safe_cex_num::subtract(entry, last)
        })
    }
    fn map_position(&mut self, row: V) -> Result<V, E> {
        self.effect("map", vec![row.clone()])?;
        if row.get("size").text() == Some(u("0")) {
            return Ok(V::Undefined);
        }
        Ok(object(vec![
            ("symbol", row.get("symbol")),
            ("unrealizedPnl", V::Num(3.0)),
            ("info", row),
        ]))
    }
    fn account_positions(&mut self) -> impl Future<Output = Result<V, E>> {
        let e = self.effect("accountPositions", vec![]);
        self.deferred(e.map(|_| {
            object(vec![(
                "positions",
                array(vec![object(vec![("id", V::Text(u("all")))])]),
            )])
        }))
    }
    fn symbol_positions(&mut self, symbol: V) -> impl Future<Output = Result<V, E>> {
        let name = u("symbol:")
            .into_iter()
            .chain(text(symbol.clone()))
            .collect();
        let e = self.effect_units(name, vec![]);
        let value = self.input.get("symbolResults").get(&ascii(&symbol));
        self.deferred(e.map(|_| {
            if matches!(value, V::Undefined) {
                array(vec![])
            } else {
                value
            }
        }))
    }
    fn flatten(&mut self, arrays: Vec<V>) -> Result<V, E> {
        Ok(array(
            arrays
                .into_iter()
                .flat_map(|v| {
                    if matches!(v, V::Array(_)) {
                        v.values()
                    } else {
                        vec![v]
                    }
                })
                .collect(),
        ))
    }
    fn read_target(&mut self) -> Result<(), E> {
        Ok(())
    }
    fn read(
        &mut self,
        _: (),
        endpoint: &'static str,
        params: Vec<(&'static str, V)>,
    ) -> Result<Deferred, E> {
        let params = if params.is_empty() {
            V::Undefined
        } else {
            object(params)
        };
        self.effect("get", vec![V::Text(u(endpoint)), params.clone()])?;
        let index = self.read_index;
        self.read_index += 1;
        if self.name == "balance" {
            let currency = params.get("currency");
            let key = ascii(&currency);
            let rejected = self
                .input
                .get("reject")
                .values()
                .iter()
                .any(|v| ascii(v) == key);
            let reply = self.input.get("replies").get(&key);
            let reply = if matches!(reply, V::Undefined) {
                object(vec![
                    ("code", V::Num(0.0)),
                    (
                        "data",
                        object(vec![
                            (
                                "account",
                                object(vec![
                                    ("accountBalanceRv", V::Text(u("1"))),
                                    ("totalUsedBalanceRv", V::Text(u("0"))),
                                ]),
                            ),
                            ("positions", array(vec![])),
                        ]),
                    ),
                ])
            } else {
                reply
            };
            let mut future = self.deferred(if rejected {
                Err(self.error_original())
            } else {
                Ok(object(vec![("data", reply)]))
            });
            future.currency = Some(currency);
            if self.input.get("reverseCompletion").truthy() && index == 0 {
                future.wait = 2
            }
            return Ok(future);
        }
        let reply = if has(&self.input, "reply") {
            self.input.get("reply")
        } else {
            let key = if self.name == "markets" {
                "perpProductsV2"
            } else {
                "rows"
            };
            let input = if self.name == "markets" {
                self.input.get("products")
            } else if self.name == "fills" {
                self.input
                    .get(if index == 0 { "primary" } else { "legacy" })
            } else {
                self.input.get("rows")
            };
            let rows = if matches!(input, V::Undefined) {
                array(vec![])
            } else {
                input
            };
            object(vec![(
                "data",
                object(vec![("data", object(vec![(key, rows)]))]),
            )])
        };
        Ok(self.deferred(Ok(reply)))
    }
    fn error_value(&mut self, error: &E) -> V {
        error.value.clone()
    }
    fn error(&mut self, message: V) -> Result<E, E> {
        self.effect("newError", vec![message.clone()])?;
        Ok(self.intrinsic(ascii(&message)))
    }
    fn emit(&mut self, message: V) -> Result<(), E> {
        self.effect("emit", vec![message])
    }
    fn emit_candle_error(&mut self, message: V) -> Result<(), E> {
        self.effect("candleError", vec![V::Text(u("error")), message])
    }
    fn now(&mut self) -> Result<V, E> {
        self.now += 1.0;
        Ok(V::Num(self.now))
    }
    fn includes(&mut self, value: &V, needle: &str) -> Result<bool, E> {
        Ok(match value {
            V::Text(s) => s.windows(u(needle).len()).any(|w| w == u(needle)),
            V::Array(v) => v.borrow().iter().any(|v| v.text() == Some(u(needle))),
            _ => false,
        })
    }
    fn nullish(&mut self, v: &V) -> Result<bool, E> {
        Ok(matches!(v, V::Undefined | V::Null))
    }
    fn public_only(&mut self) -> Result<bool, E> {
        Ok(self.input.get("publicOnly").truthy())
    }
    fn date_seconds(&mut self, value: V) -> Result<Option<f64>, E> {
        if matches!(value, V::Null | V::Undefined) {
            return Ok(None);
        }
        let t = text(value.clone());
        let digits = !t.is_empty() && t.iter().all(|c| *c >= 48 && *c <= 57);
        if matches!(value, V::Text(_)) && !digits {
            let date = safe_cex_num::js_date_parse_utf16(&t, |v| v);
            return Ok(if date.is_finite() {
                Some((date / 1000.0).floor())
            } else {
                None
            });
        }
        let n = safe_cex_num::parse_int(&String::from_utf16_lossy(&t), Some(10));
        Ok(if !n.is_finite() || n <= 0.0 {
            None
        } else {
            Some(if n > 1e15 {
                (n / 1e9).floor()
            } else if n > 1e12 {
                (n / 1000.0).floor()
            } else {
                n
            })
        })
    }
    fn end_time(&mut self, to: V, daily: bool) -> Result<V, E> {
        let mut end = if to.truthy() {
            to.number()
        } else {
            1700000000000.0
        };
        end = safe_cex_num::js_time_clip(end);
        if daily {
            end = (end / 86400000.0).floor() * 86400000.0 - 86400000.0;
        }
        Ok(V::Num(end))
    }
    fn start_time(
        &mut self,
        end: &V,
        from: V,
        use_from: bool,
        amount: f64,
        unit: Vec<u16>,
    ) -> Result<V, E> {
        let n = if use_from {
            from.number()
        } else {
            let scale = match String::from_utf16_lossy(&unit).as_str() {
                "m" => 60000.0,
                "h" => 3600000.0,
                "d" => 86400000.0,
                _ => 1.0,
            };
            let amount = if unit == u("d") {
                safe_cex_num::math_round(-amount)
            } else {
                -amount
            };
            end.number() + amount * scale
        };
        Ok(V::Num(safe_cex_num::js_time_clip(n)))
    }
    fn milliseconds(&mut self, date: &V) -> Result<f64, E> {
        Ok(date.number())
    }
    fn round_usd(&mut self, n: f64) -> Result<f64, E> {
        Ok(safe_cex_num::math_round(n * 100.0) / 100.0)
    }
}
fn run(name: &str, count: usize) {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/rest-data-oracle.cjs"
        ))
        .arg(name)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "RESTDATA actual source oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), count, "RESTDATA source case census");
    for (index, case) in cases.into_iter().enumerate() {
        let input = V::decode(&case["input"]);
        let mut h = Host::new(input.clone(), name);
        let outcome = match name {
            "markets" => complete(data::fetch_markets(&mut h)),
            "position" => data::map_position(&mut h, input.get("row")),
            "positions" => complete(data::fetch_positions(&mut h, input.get("symbols"))),
            "orders" => complete(data::fetch_orders(&mut h)),
            "order_update" => data::map_order_update(&mut h, input.get("row")),
            "balance" => complete(data::fetch_positions_and_balance(&mut h)),
            "fills" => complete(data::fetch_historical_fills(&mut h, input.get("opts"))),
            "candles" => complete(data::fetch_ohlcv(&mut h, input.get("opts"))),
            _ => panic!(),
        };
        let (result, thrown) = match outcome {
            Ok(v) => (v, V::Undefined),
            Err(e) => {
                let original = Rc::ptr_eq(&e.token, &h.original);
                (
                    V::Undefined,
                    V::Text(u(if original { "original" } else { "intrinsic" })),
                )
            }
        };
        if name == "orders" {
            let (V::Array(a), V::Array(b)) = (&result, &h.orders) else {
                panic!()
            };
            assert!(Rc::ptr_eq(a, b), "RESTDATA native orders identity")
        }
        if name == "markets"
            && h.effects
                .borrow()
                .iter()
                .any(|e| e.values()[0].text() == Some(u("emit")))
        {
            let (V::Array(a), V::Array(b)) = (&result, &h.markets) else {
                panic!()
            };
            assert!(Rc::ptr_eq(a, b), "RESTDATA native cached market identity")
        }
        if name == "position" && result.truthy() {
            let (V::Object(info), V::Object(original)) = (result.get("info"), input.get("row"))
            else {
                panic!()
            };
            assert!(
                Rc::ptr_eq(&info, &original),
                "RESTDATA native position info identity"
            );
        }
        if name == "order_update" {
            for order in result.values() {
                let (V::Object(info), V::Object(original)) = (order.get("info"), input.get("row"))
                else {
                    panic!()
                };
                assert!(
                    Rc::ptr_eq(&info, &original),
                    "RESTDATA native order info identity"
                );
            }
        }
        let expected = &case["expected"];
        let effects = expected["object"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e[0]["units"] == json!(u("effects")))
            .unwrap()[1]
            .as_array()
            .unwrap();
        for (a, b) in h.effects.borrow().iter().zip(effects) {
            assert_eq!(
                a.obs(),
                *b,
                "RESTDATA native effect {} family {name} case {index}",
                ascii(&a.values()[0])
            );
        }
        let observed = object(vec![
            ("effects", array(h.effects.borrow().clone())),
            ("result", result),
            ("thrown", thrown),
        ])
        .obs();
        assert_eq!(
            observed, *expected,
            "RESTDATA native complete observation family {name} case {index}"
        );
        if ["markets", "balance", "candles"].contains(&name)
            && h.effects
                .borrow()
                .iter()
                .any(|e| e.values()[0].text() == Some(u("get")))
            && input.get("failure").text() != Some(u("get"))
        {
            assert!(h.pending.get() > 0, "RESTDATA native actual Pending");
        }
    }
}
#[test]
fn actual_typescript_markets() {
    run("markets", 6)
}
#[test]
fn actual_typescript_position() {
    run("position", 9)
}
#[test]
fn actual_typescript_positions() {
    run("positions", 6)
}
#[test]
fn actual_typescript_orders() {
    run("orders", 2)
}
#[test]
fn actual_typescript_order_update() {
    run("order_update", 11)
}
#[test]
fn actual_typescript_balance() {
    run("balance", 9)
}
#[test]
fn actual_typescript_fills() {
    run("fills", 17)
}
#[test]
fn actual_typescript_candles() {
    run("candles", 12)
}
