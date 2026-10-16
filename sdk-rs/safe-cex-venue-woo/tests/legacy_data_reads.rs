use safe_cex_venue_woo::{legacy_data_reads::*, legacy_metadata::Task};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type JsString = Vec<u16>;
#[derive(Clone, Debug)]
enum Js {
    U,
    Null,
    B(bool),
    N(f64),
    S(JsString),
    A(Vec<Option<V>>),
    O(Vec<(String, V)>),
}
type V = Rc<RefCell<Js>>;
fn val(j: Js) -> V {
    Rc::new(RefCell::new(j))
}
fn u(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn strv(s: &str) -> V {
    val(Js::S(u(s)))
}
fn num(n: f64) -> V {
    val(Js::N(n))
}
fn obj(fields: Vec<(&str, V)>) -> V {
    val(Js::O(
        fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    ))
}
fn arr(a: Vec<V>) -> V {
    val(Js::A(a.into_iter().map(Some).collect()))
}
fn field(v: &V, k: &str) -> V {
    match &*v.borrow() {
        Js::O(o) => o
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| val(Js::U)),
        _ => val(Js::U),
    }
}
fn set(v: &V, k: &str, new: V) {
    let mut v = v.borrow_mut();
    let Js::O(o) = &mut *v else {
        panic!("native object write")
    };
    if let Some((_, v)) = o.iter_mut().find(|(n, _)| n == k) {
        *v = new;
    } else {
        o.push((k.into(), new));
    }
}
fn truthy(v: &V) -> bool {
    match &*v.borrow() {
        Js::U | Js::Null => false,
        Js::B(b) => *b,
        Js::N(n) => *n != 0.0 && !n.is_nan(),
        Js::S(s) => !s.is_empty(),
        _ => true,
    }
}
fn txt(v: &V) -> String {
    match &*v.borrow() {
        Js::U => "undefined".into(),
        Js::Null => "null".into(),
        Js::B(b) => b.to_string(),
        Js::N(n) => safe_cex_num::js_number_string(*n),
        Js::S(s) => String::from_utf16(s).unwrap(),
        Js::O(_) => "[object Object]".into(),
        Js::A(a) => a
            .iter()
            .map(|v| {
                v.as_ref()
                    .filter(|v| !matches!(*v.borrow(), Js::U | Js::Null))
                    .map(txt)
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(","),
    }
}
fn bits(n: f64) -> String {
    format!(
        "{:016x}",
        if n.is_nan() {
            0x7ff8000000000000
        } else {
            n.to_bits()
        }
    )
}
fn enc(v: &V) -> Value {
    match &*v.borrow() {
        Js::U => json!({"u":true}),
        Js::Null => Value::Null,
        Js::B(b) => json!(b),
        Js::N(n) => json!({"n":bits(*n)}),
        Js::S(s) => json!({"s":s}),
        Js::A(a) => Value::Array(
            a.iter()
                .map(|v| v.as_ref().map(enc).unwrap_or(json!({"hole":true})))
                .collect(),
        ),
        Js::O(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), enc(v))).collect()),
    }
}
fn error(s: &str) -> V {
    obj(vec![("message", strv(s))])
}
fn ready<T: 'static>(r: Result<T, V>) -> Task<T, V> {
    Box::pin(std::future::ready(r))
}
fn run<T: 'static>(mut f: Task<T, V>) -> Result<T, V> {
    let w = Waker::noop().clone();
    let mut cx = Context::from_waker(&w);
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("unexpected pending"),
    }
}
fn same(a: &V, b: &V) -> bool {
    if Rc::ptr_eq(a, b) {
        return true;
    }
    match (&*a.borrow(), &*b.borrow()) {
        (Js::U, Js::U) | (Js::Null, Js::Null) => true,
        (Js::N(a), Js::N(b)) => a == b || (a.is_nan() && b.is_nan()),
        (Js::S(a), Js::S(b)) => a == b,
        (Js::B(a), Js::B(b)) => a == b,
        _ => false,
    }
}
fn number(v: &V) -> f64 {
    match &*v.borrow() {
        Js::N(n) => *n,
        Js::Null => 0.,
        Js::U => f64::NAN,
        Js::B(v) => f64::from(*v),
        _ => safe_cex_num::js_number(Some(&Value::String(txt(v)))).unwrap(),
    }
}
fn funding_rows() -> V {
    arr(vec![
        obj(vec![
            ("symbol", strv("PERP_BTC_USDT")),
            ("est_funding_rate", strv("0.001")),
            ("next_funding_time", strv("12000")),
        ]),
        obj(vec![
            ("symbol", strv("SPOT_ETH_USDT")),
            ("est_funding_rate", num(1.)),
            ("next_funding_time", num(1.)),
        ]),
        obj(vec![
            ("symbol", strv("PERP_ETH_USDT")),
            ("est_funding_rate", strv("")),
            ("next_funding_time", num(12.)),
        ]),
        obj(vec![
            ("symbol", strv("PERP_UNKNOWN_USDT")),
            ("est_funding_rate", num(2.)),
            ("next_funding_time", num(12.)),
        ]),
        val(Js::Null),
    ])
}
fn candle_rows() -> V {
    arr(vec![
        obj(vec![
            ("start_timestamp", num(6000000.)),
            ("open", strv("1")),
            ("high", num(3.)),
            ("low", num(0.)),
            ("close", num(2.)),
            ("volume", num(4.)),
        ]),
        obj(vec![
            ("start_timestamp", num(5940000.)),
            ("open", num(4.)),
            ("high", num(5.)),
            ("low", num(1.)),
            ("close", num(3.)),
            ("volume", num(7.)),
        ]),
    ])
}
fn position(symbol: &str, side: &str, mode: &str) -> V {
    obj(vec![
        ("symbol", strv(symbol)),
        ("side", strv(side)),
        ("positionMode", strv(mode)),
        ("contracts", num(2.)),
    ])
}
fn meta(symbol: &str, side: &str, mode: &str) -> V {
    obj(vec![
        ("symbol", strv(symbol)),
        ("side", strv(side)),
        ("positionMode", strv(mode)),
        ("marginType", strv("cross")),
        ("leverage", num(5.)),
        ("extra", strv("kept")),
    ])
}
fn idx(v: &V, i: usize) -> V {
    match &*v.borrow() {
        Js::A(a) => a
            .get(i)
            .and_then(Clone::clone)
            .unwrap_or_else(|| val(Js::U)),
        _ => val(Js::U),
    }
}
struct Iter {
    v: V,
    i: usize,
}
struct H {
    family: &'static str,
    i: usize,
    trace: Vec<String>,
    requests: Vec<(V, V, V)>,
    logs: Vec<(bool, V)>,
    updates: Vec<(bool, V)>,
    timer_calls: Vec<V>,
    timer_callback: Option<Rc<dyn Fn() -> Result<Task<V, V>, V>>>,
    roots: V,
    data: V,
    err: V,
    open: V,
    metadata: V,
    mode: V,
    ticker: V,
    ret: V,
    args: Vec<(bool, bool)>,
    metadata_symbols: Option<V>,
    metadata_results: Vec<V>,
    capture: usize,
    includes_target: Option<V>,
    iterator_calls: usize,
}
impl Host for H {
    type Value = V;
    type Error = V;
    type Prepared = Binding;
    type Iterator = Iter;
    fn prepare(&mut self, b: Binding) -> Result<Binding, V> {
        Ok(b)
    }
    fn call(&mut self, c: Binding, args: Vec<V>) -> Result<Task<V, V>, V> {
        let value = match c {
            Binding::RefreshFunding => {
                self.trace.push("refresh".into());
                if self.capture == 6 {
                    return Err(self.err.clone());
                }
                val(Js::U)
            }
            Binding::PositionMode => {
                self.trace.push("mode".into());
                if self.family == "metadata" && self.i == 5 {
                    return Err(self.err.clone());
                }
                self.mode.clone()
            }
            Binding::OpenPositions => {
                self.trace.push("open".into());
                if self.i == 5 {
                    return Ok(ready(Err(self.err.clone())));
                }
                self.open.clone()
            }
            Binding::Metadata => {
                self.trace.push("metadata".into());
                self.requests
                    .push((strv("metadata"), val(Js::U), args[0].clone()));
                self.metadata.clone()
            }
            Binding::MetadataSymbol => {
                self.trace.push(format!("metadata:{}", txt(&args[0])));
                if self.i == 3 {
                    return Ok(ready(Err(self.err.clone())));
                }
                if self.i == 4 && txt(&args[0]) == "BTCUSDT" {
                    if let Some(symbols) = &self.metadata_symbols {
                        if let Js::A(v) = &mut *symbols.borrow_mut() {
                            v.push(Some(strv("ETHUSDT")));
                        }
                    }
                }
                let row = meta(&txt(&args[0]), "long", &txt(&args[1]));
                self.metadata_results.push(row.clone());
                arr(vec![row])
            }
            _ => panic!("unexpected async binding {c:?}"),
        };
        Ok(ready(Ok(value)))
    }
    fn invoke(&mut self, c: Binding, args: Vec<V>) -> Result<V, V> {
        Ok(match c {
            Binding::Now => num(6000000.),
            Binding::CandleStart => {
                let interval = if txt(&args[0]) == "1m" { 60. } else { f64::NAN };
                num((number(&args[1]) / interval).floor() * interval)
            }
            Binding::Reverse => strv(&format!(
                "PERP_{}_USDT",
                txt(&args[0]).strip_suffix("USDT").unwrap_or(&txt(&args[0]))
            )),
            Binding::Normalize => strv(&txt(&args[0]).replace("PERP_", "").replace('_', "")),
            Binding::ParseFunding => {
                let v = &args[0];
                if matches!(*v.borrow(), Js::N(_) | Js::S(_))
                    && !(matches!(*v.borrow(), Js::S(_)) && txt(v).trim().is_empty())
                    && number(v).is_finite()
                {
                    num(number(v))
                } else {
                    val(Js::U)
                }
            }
            Binding::UpdateTicker => {
                self.trace.push("update".into());
                self.updates
                    .push((Rc::ptr_eq(&args[0], &self.ticker), args[1].clone()));
                val(Js::U)
            }
            Binding::ListenCandle | Binding::ListenBook => {
                self.trace.push(
                    if c == Binding::ListenCandle {
                        "listen:candle"
                    } else {
                        "listen:book"
                    }
                    .into(),
                );
                self.args.push((
                    Rc::ptr_eq(&args[0], &field(&self.roots, "arg")),
                    Rc::ptr_eq(&args[1], &field(&self.roots, "callback")),
                ));
                if self.i == 1 {
                    return Err(self.err.clone());
                }
                self.ret.clone()
            }
            Binding::PositionId => {
                if txt(&args[2]) == "oneway" || !truthy(&args[1]) {
                    args[0].clone()
                } else {
                    strv(&format!("{}:{}", txt(&args[0]), txt(&args[1])))
                }
            }
            Binding::Flatten => {
                let mut out = vec![];
                let Js::A(a) = &*args[0].borrow() else {
                    panic!("flatten array")
                };
                for v in a {
                    if let Some(v) = v {
                        if let Js::A(inner) = &*v.borrow() {
                            out.extend(
                                inner
                                    .iter()
                                    .map(|v| Some(v.clone().unwrap_or_else(|| val(Js::U)))),
                            )
                        } else {
                            out.push(Some(v.clone()));
                        }
                    } else {
                        out.push(Some(val(Js::U)));
                    }
                }
                val(Js::A(out))
            }
            _ => panic!("unexpected sync binding {c:?}"),
        })
    }
    fn get(&mut self, c: Binding, url: V, params: Option<V>) -> Result<Task<V, V>, V> {
        let Binding::Get(transport) = c else {
            panic!("get token")
        };
        let label = if transport == Transport::Main {
            "main"
        } else {
            "proxy"
        };
        self.trace.push(format!("get:{label}"));
        self.requests
            .push((strv(label), url, params.unwrap_or_else(|| val(Js::U))));
        if self.family == "funding" && self.i == 6 {
            set(&self.roots, "isDisposed", val(Js::B(true)));
        }
        if self.capture == 1 || self.family == "touch" && self.i == 6 {
            return Err(self.err.clone());
        }
        if self.family == "funding" && self.i == 3 || self.family == "touch" && self.i == 5 {
            return Ok(ready(Err(self.err.clone())));
        }
        let data = if self.family == "ohlcv" {
            if transport == Transport::Main {
                obj(vec![("rows", self.data.clone())])
            } else {
                obj(vec![("data", obj(vec![("rows", self.data.clone())]))])
            }
        } else {
            self.data.clone()
        };
        Ok(ready(Ok(obj(vec![("data", data)]))))
    }
    fn root(&mut self, k: &str) -> Result<V, V> {
        Ok(field(&self.roots, k))
    }
    fn set_root(&mut self, k: &str, v: V) -> Result<(), V> {
        set(&self.roots, k, v);
        Ok(())
    }
    fn field(&mut self, v: &V, k: &str) -> Result<V, V> {
        if self.capture == 5 && k == "symbol" && txt(&field(v, k)) == "PERP_BTC_USDT" {
            self.trace.push("symbol".into());
            return Err(error("row failure"));
        }
        if self.capture == 2 && k == "price" {
            self.trace.push("price".into());
        }
        if matches!(*v.borrow(), Js::U | Js::Null) {
            Err(error("nullish field"))
        } else {
            Ok(field(v, k))
        }
    }
    fn key_field(&mut self, v: &V, k: Vec<u16>) -> Result<V, V> {
        self.field(v, &String::from_utf16(&k).unwrap())
    }
    fn index(&mut self, v: &V, i: usize) -> Result<V, V> {
        Ok(idx(v, i))
    }
    fn length(&mut self, v: &V) -> Result<usize, V> {
        match &*v.borrow() {
            Js::A(a) => Ok(a.len()),
            _ => Err(error("array length")),
        }
    }
    fn has_index(&mut self, v: &V, i: usize) -> Result<bool, V> {
        Ok(match &*v.borrow() {
            Js::A(a) => a.get(i).is_some_and(Option::is_some),
            _ => false,
        })
    }
    fn iterator(&mut self, mut v: V) -> Result<Iter, V> {
        if self.capture == 3
            && self
                .includes_target
                .as_ref()
                .is_some_and(|t| Rc::ptr_eq(t, &v))
        {
            self.iterator_calls += 1;
            v = arr(vec![strv("ETHUSDT")]);
        }
        if matches!(*v.borrow(), Js::A(_)) {
            Ok(Iter { v, i: 0 })
        } else {
            Err(error("not iterable"))
        }
    }
    fn next(&mut self, it: &mut Iter) -> Result<Option<V>, V> {
        if it.i >= self.length(&it.v)? {
            Ok(None)
        } else {
            let v = idx(&it.v, it.i);
            it.i += 1;
            Ok(Some(v))
        }
    }
    fn close_iterator(&mut self, _i: &mut Iter) -> Result<(), V> {
        if self.capture == 2 || self.capture == 5 {
            self.trace.push("close".into());
        }
        Ok(())
    }
    fn await_element(&mut self, v: V) -> Task<V, V> {
        ready(Ok(v))
    }
    fn is_undefined(&self, v: &V) -> bool {
        matches!(*v.borrow(), Js::U)
    }
    fn truthy(&self, v: &V) -> bool {
        truthy(v)
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(*v.borrow(), Js::U | Js::Null)
    }
    fn is_object(&self, v: &V) -> bool {
        matches!(*v.borrow(), Js::O(_) | Js::A(_) | Js::Null)
    }
    fn is_string(&self, v: &V) -> bool {
        matches!(*v.borrow(), Js::S(_))
    }
    fn is_array(&self, v: &V) -> bool {
        matches!(*v.borrow(), Js::A(_))
    }
    fn strict_true(&self, v: &V) -> bool {
        matches!(*v.borrow(), Js::B(true))
    }
    fn equals(&self, v: &V, s: &str) -> bool {
        matches!(&*v.borrow(),Js::S(v) if *v==u(s))
    }
    fn same(&self, a: &V, b: &V) -> bool {
        if matches!(*a.borrow(),Js::N(n) if n.is_nan())
            || matches!(*b.borrow(),Js::N(n) if n.is_nan())
        {
            false
        } else {
            same(a, b)
        }
    }
    fn same_zero(&self, a: &V, b: &V) -> bool {
        same(a, b)
    }
    fn number_coerce(&mut self, v: &V) -> Result<f64, V> {
        Ok(number(v))
    }
    fn operator_number(&mut self, v: &V) -> Result<f64, V> {
        Ok(number(v))
    }
    fn text(&mut self, v: &V) -> Result<Vec<u16>, V> {
        Ok(match &*v.borrow() {
            Js::S(s) => s.clone(),
            _ => u(&txt(v)),
        })
    }
    fn starts_with(&mut self, v: &V, s: &str) -> Result<bool, V> {
        Ok(txt(v).starts_with(s))
    }
    fn number(&self, n: f64) -> V {
        num(n)
    }
    fn boolean(&self, b: bool) -> V {
        val(Js::B(b))
    }
    fn undefined(&self) -> V {
        val(Js::U)
    }
    fn string(&self, s: Vec<u16>) -> V {
        val(Js::S(s))
    }
    fn object(&self, f: Vec<(&'static str, V)>) -> V {
        obj(f)
    }
    fn object_spread(
        &mut self,
        before: Vec<(&'static str, V)>,
        v: V,
        after: Vec<(&'static str, V)>,
    ) -> Result<V, V> {
        let out = obj(before);
        if let Js::O(fields) = &*v.borrow() {
            for (k, v) in fields {
                set(&out, k, v.clone());
            }
        }
        for (k, v) in after {
            set(&out, k, v);
        }
        Ok(out)
    }
    fn array(&self, v: Vec<Option<V>>) -> V {
        val(Js::A(v))
    }
    fn type_error(&mut self, m: &str) -> V {
        error(m)
    }
    fn error(&mut self, m: &str) -> V {
        error(m)
    }
    fn error_field(&mut self, e: &V, path: &[&str]) -> Result<V, V> {
        let mut v = e.clone();
        for k in path {
            v = field(&v, k);
        }
        Ok(v)
    }
    fn emit(&mut self, c: Binding, e: Option<&V>, m: V) -> Result<(), V> {
        assert_eq!(c, Binding::Emit, "DATA native emitter token");
        self.trace.push("emit".into());
        self.logs
            .push((e.is_some_and(|e| Rc::ptr_eq(e, &self.err)), m));
        if self.capture == 4 {
            return Err(error("emitter failure"));
        }
        Ok(())
    }
    fn get_callable(&mut self) -> Result<bool, V> {
        if self.family == "touch" && self.i == 7 {
            self.trace.push("lookup".into());
            return Err(self.err.clone());
        }
        Ok(!(self.family == "touch" && self.i == 4))
    }
    fn interval(
        &mut self,
        c: Binding,
        delay: V,
        callback: Rc<dyn Fn() -> Result<Task<V, V>, V>>,
    ) -> Result<V, V> {
        assert_eq!(c, Binding::SetInterval, "DATA native interval token");
        self.trace.push("interval".into());
        self.timer_calls.push(delay);
        self.timer_callback = Some(callback);
        Ok(num(7.))
    }
}
fn setup(family: &'static str, i: usize) -> (Rc<RefCell<H>>, V, V) {
    let err = error("top failure");
    set(
        &err,
        "response",
        obj(vec![(
            "data",
            obj(vec![
                ("message", strv("nested failure")),
                ("msg", strv("metadata failure")),
            ]),
        )]),
    );
    let ticker = obj(vec![("symbol", strv("BTCUSDT"))]);
    let orders = arr(vec![
        obj(vec![("symbol", strv("ETHUSDT")), ("status", strv("open"))]),
        obj(vec![("symbol", strv("BTCUSDT")), ("status", strv("open"))]),
        obj(vec![
            ("symbol", strv("IGNORED")),
            ("status", strv("closed")),
        ]),
    ]);
    let roots = obj(vec![
        ("isDisposed", val(Js::B(false))),
        ("_fundingRefreshInFlight", val(Js::B(false))),
        ("_fundingRefreshTimer", val(Js::U)),
        ("FUNDING_REFRESH_INTERVAL_MS", num(60000.)),
        ("tickersBySymbol", obj(vec![("BTCUSDT", ticker.clone())])),
        ("timeframeToSeconds", obj(vec![("1m", num(60.))])),
        ("orders", orders.clone()),
    ]);
    let mode = strv(if family == "positions" && i == 2 || family == "metadata" {
        "hedged"
    } else {
        "oneway"
    });
    let open = if family == "positions" && i == 1 {
        set(&roots, "orders", arr(vec![]));
        arr(vec![])
    } else {
        arr(vec![position("BTCUSDT", "long", &txt(&mode))])
    };
    let metadata = arr(vec![
        meta("ETHUSDT", "long", &txt(&mode)),
        meta("BTCUSDT", "short", &txt(&mode)),
    ]);
    if family == "positions" && i == 4 {
        let m = idx(&metadata, 0);
        set(&m, "id", strv("preexisting"));
        set(&m, "entryPrice", num(99.));
    }
    let mut data = val(Js::U);
    let input;
    let arg;
    match family {
        "timer" => {
            if i == 1 {
                set(&roots, "_fundingRefreshTimer", num(0.));
            }
            if i == 2 {
                set(&roots, "_fundingRefreshTimer", val(Js::Null));
            }
            if i == 3 {
                set(&roots, "isDisposed", val(Js::B(true)));
            }
            input = obj(vec![
                ("disposed", field(&roots, "isDisposed")),
                ("timer", field(&roots, "_fundingRefreshTimer")),
            ]);
            arg = val(Js::U);
        }
        "funding" => {
            data = obj(vec![
                ("success", if i == 2 { num(1.) } else { val(Js::B(true)) }),
                (
                    "rows",
                    if i == 1 {
                        val(Js::Null)
                    } else {
                        funding_rows()
                    },
                ),
            ]);
            if i == 4 {
                set(&roots, "isDisposed", val(Js::B(true)));
            }
            if i == 5 {
                set(&roots, "_fundingRefreshInFlight", val(Js::B(true)));
            }
            input = obj(vec![
                ("data", data.clone()),
                ("disposed", field(&roots, "isDisposed")),
                ("flight", field(&roots, "_fundingRefreshInFlight")),
            ]);
            arg = val(Js::U);
        }
        "touch" => {
            data = obj(vec![
                (
                    "asks",
                    arr(vec![obj(vec![(
                        "price",
                        if i == 2 {
                            num(0.)
                        } else if i == 3 {
                            strv("bad")
                        } else {
                            strv("12.5")
                        },
                    )])]),
                ),
                ("bids", arr(vec![obj(vec![("price", num(9.))])])),
            ]);
            input = obj(vec![
                ("data", data.clone()),
                ("symbol", strv("BTCUSDT")),
                ("side", strv(if i == 1 { "sell" } else { "buy" })),
                ("callable", val(Js::B(i != 4))),
            ]);
            arg = strv("BTCUSDT");
        }
        "ohlcv" => {
            arg = obj(vec![
                ("symbol", strv("BTCUSDT")),
                ("interval", strv("1m")),
                (
                    "to",
                    num(if i == 1 {
                        0.
                    } else if i == 2 {
                        60000.
                    } else {
                        6000000.
                    }),
                ),
                ("from", num(100.)),
                (
                    "limit",
                    num(if i == 3 {
                        1200.
                    } else if i == 4 {
                        0.
                    } else {
                        2.
                    }),
                ),
            ]);
            data = if i == 5 { val(Js::Null) } else { candle_rows() };
            if i == 4 {
                if let Js::A(v) = &mut *data.borrow_mut() {
                    v[0] = None;
                }
            }
            input = obj(vec![("opts", arg.clone()), ("rows", data.clone())]);
        }
        "candle" | "book" => {
            arg = if family == "candle" {
                obj(vec![("symbol", strv("BTCUSDT")), ("interval", strv("1m"))])
            } else {
                strv("BTCUSDT")
            };
            let cb = obj(vec![("callback", val(Js::B(true)))]);
            set(&roots, "arg", arg.clone());
            set(&roots, "callback", cb.clone());
            input = obj(vec![("arg", arg.clone()), ("callback", cb)]);
        }
        "metadata" => {
            arg = if i == 4 {
                arr(vec![strv("BTCUSDT")])
            } else {
                arr(vec![strv("BTCUSDT"), strv("ETHUSDT")])
            };
            if i == 1 {
                if let Js::A(v) = &mut *arg.borrow_mut() {
                    v[0] = None;
                }
            }
            if i == 2 {
                set(&roots, "isDisposed", val(Js::B(true)));
            }
            input = obj(vec![
                ("symbols", arg.clone()),
                ("disposed", field(&roots, "isDisposed")),
            ]);
        }
        "positions" => {
            arg = if i == 3 {
                arr(vec![])
            } else if i == 4 {
                arr(vec![strv("BTCUSDT")])
            } else {
                val(Js::U)
            };
            input = obj(vec![
                ("open", open.clone()),
                ("orders", field(&roots, "orders")),
                ("mode", mode.clone()),
                ("symbols", arg.clone()),
                ("metadata", metadata.clone()),
            ]);
        }
        _ => unreachable!(),
    }
    let metadata_symbols = if family == "metadata" {
        Some(arg.clone())
    } else {
        None
    };
    (
        Rc::new(RefCell::new(H {
            family,
            i,
            trace: vec![],
            requests: vec![],
            logs: vec![],
            updates: vec![],
            timer_calls: vec![],
            timer_callback: None,
            roots,
            data,
            err,
            open,
            metadata,
            mode,
            ticker,
            ret: obj(vec![("unsubscribe", val(Js::B(true)))]),
            args: vec![],
            metadata_symbols,
            metadata_results: vec![],
            capture: 0,
            includes_target: None,
            iterator_calls: 0,
        })),
        input,
        arg,
    )
}
fn native_str(v: &V, s: &str, label: &str) {
    assert!(
        matches!(&*v.borrow(),Js::S(v) if *v==u(s)),
        "DATA native {label}: {v:?}"
    );
}
fn native_num(v: &V, n: f64, label: &str) {
    assert!(
        matches!(*v.borrow(),Js::N(v) if v.to_bits()==n.to_bits()||v.is_nan()&&n.is_nan()),
        "DATA native {label}: {v:?}"
    );
}
fn native_len(v: &V, n: usize, label: &str) {
    assert!(
        matches!(&*v.borrow(),Js::A(v) if v.len()==n),
        "DATA native {label} size {n}: {v:?}"
    );
}
fn native(
    family: &str,
    i: usize,
    h: &H,
    result: &Result<V, V>,
    before: &[String],
    aliases: &[bool],
) {
    let failed = family == "ohlcv" && i == 5
        || matches!(family, "candle" | "book") && i == 1
        || family == "positions" && i == 5
        || family == "touch" && i == 7;
    assert_eq!(
        result.is_err(),
        failed,
        "DATA native {family} outcome iteration {i}"
    );
    if let Err(e) = result {
        if !(family == "ohlcv" && i == 5) {
            assert!(Rc::ptr_eq(e, &h.err), "DATA native {family} original error");
        } else {
            native_str(
                &field(e, "message"),
                "Cannot read properties of null (reading 'map')",
                "map type error",
            );
        }
    }
    let before_expected: Vec<&str> = match family {
        "timer" => {
            if i == 0 {
                vec!["interval"]
            } else {
                vec![]
            }
        }
        "funding" => {
            if i == 4 || i == 5 {
                vec![]
            } else {
                vec!["get:main"]
            }
        }
        "touch" => {
            if i == 4 {
                vec![]
            } else if i == 6 {
                vec!["get:main", "emit"]
            } else if i == 7 {
                vec!["lookup"]
            } else {
                vec!["get:main"]
            }
        }
        "ohlcv" => {
            if i == 1 {
                vec!["get:proxy"]
            } else {
                vec!["get:main"]
            }
        }
        "candle" => vec!["listen:candle"],
        "book" => vec!["listen:book"],
        "metadata" => {
            if i == 5 {
                vec!["mode", "emit"]
            } else {
                vec!["mode"]
            }
        }
        "positions" => vec!["open"],
        _ => unreachable!(),
    };
    assert_eq!(
        before, before_expected,
        "DATA native {family} eager sequence iteration {i}"
    );
    let trace_expected: Vec<&str> = match family {
        "timer" => {
            if i == 0 {
                vec!["interval", "refresh"]
            } else {
                vec![]
            }
        }
        "funding" => match i {
            0 => vec!["get:main", "update"],
            1..=3 => vec!["get:main", "emit"],
            4 | 5 => vec![],
            6 => vec!["get:main"],
            _ => unreachable!(),
        },
        "touch" => match i {
            4 => vec![],
            5 | 6 => vec!["get:main", "emit"],
            7 => vec!["lookup"],
            _ => vec!["get:main"],
        },
        "ohlcv" => {
            if i == 1 {
                vec!["get:proxy"]
            } else {
                vec!["get:main"]
            }
        }
        "candle" => vec!["listen:candle"],
        "book" => vec!["listen:book"],
        "metadata" => match i {
            0 | 4 => vec!["mode", "metadata:BTCUSDT", "metadata:ETHUSDT"],
            1 => vec!["mode", "metadata:ETHUSDT"],
            2 => vec!["mode"],
            3 => vec!["mode", "metadata:BTCUSDT", "emit"],
            5 => vec!["mode", "emit"],
            _ => unreachable!(),
        },
        "positions" => {
            if i == 5 {
                vec!["open"]
            } else if i == 1 || i == 3 {
                vec!["open", "mode"]
            } else {
                vec!["open", "mode", "metadata"]
            }
        }
        _ => unreachable!(),
    };
    assert_eq!(
        h.trace, trace_expected,
        "DATA native {family} sequence iteration {i}"
    );
    let log = family == "funding" && matches!(i, 1..=3)
        || family == "touch" && (i == 5 || i == 6)
        || family == "metadata" && (i == 3 || i == 5);
    assert_eq!(
        h.logs.len(),
        usize::from(log),
        "DATA native {family} log count"
    );
    if log {
        assert_eq!(
            h.logs[0].0,
            family == "metadata",
            "DATA native full error argument"
        );
        native_str(
            &h.logs[0].1,
            if family == "funding" && (i == 1 || i == 2) {
                "WOO X funding rates response has an invalid shape"
            } else if family == "metadata" {
                "metadata failure"
            } else {
                "nested failure"
            },
            "logged message",
        );
    }
    let request_count = match family {
        "funding" => usize::from(i != 4 && i != 5),
        "touch" => usize::from(i != 4 && i != 7),
        "ohlcv" => 1,
        "positions" => usize::from(!matches!(i, 1 | 3 | 5)),
        _ => 0,
    };
    assert_eq!(
        h.requests.len(),
        request_count,
        "DATA native {family} request count"
    );
    if family == "timer" {
        assert_eq!(
            h.timer_calls.len(),
            usize::from(i == 0),
            "DATA native timer count"
        );
        let timer = field(&h.roots, "_fundingRefreshTimer");
        match i {
            0 => {
                native_num(&timer, 7., "registered timer");
                native_num(&h.timer_calls[0], 60000., "interval delay");
            }
            1 => native_num(&timer, 0., "existing zero timer"),
            2 => assert!(
                matches!(*timer.borrow(), Js::Null),
                "DATA native existing null timer"
            ),
            3 => assert!(
                matches!(*timer.borrow(), Js::U),
                "DATA native disposed timer"
            ),
            _ => unreachable!(),
        }
    }
    if family == "funding" {
        assert!(
            matches!(*field(&h.roots,"_fundingRefreshInFlight").borrow(),Js::B(v) if v==(i==5)),
            "DATA native funding finally flag"
        );
        assert_eq!(
            h.updates.len(),
            usize::from(i == 0),
            "DATA native funding update count"
        );
        if i == 0 {
            assert!(h.updates[0].0, "DATA native ticker alias");
            native_num(
                &field(&h.updates[0].1, "fundingRate"),
                0.001,
                "funding rate",
            );
            native_num(
                &field(&h.updates[0].1, "fundingTime"),
                12000.,
                "funding time",
            );
        }
        if request_count == 1 {
            native_str(
                &h.requests[0].1,
                "/v1/public/funding_rates",
                "funding route",
            );
        }
    }
    if family == "touch" && i != 7 {
        let v = result.as_ref().unwrap();
        if i == 0 || i == 1 {
            native_num(v, if i == 0 { 12.5 } else { 9. }, "touch price");
        } else {
            assert!(
                matches!(*v.borrow(), Js::U),
                "DATA native invalid touch undefined"
            );
        }
        if request_count == 1 {
            native_str(
                &h.requests[0].1,
                "/v1/public/orderbook/PERP_BTC_USDT",
                "touch route",
            );
        }
    }
    if family == "ohlcv" {
        native_str(
            &h.requests[0].1,
            if i == 1 {
                "https://api-pub.woox.io/v1/hist/kline"
            } else {
                "/v1/kline"
            },
            "candle route",
        );
        let params = &h.requests[0].2;
        native_str(&field(params, "symbol"), "PERP_BTC_USDT", "candle symbol");
        native_str(&field(params, "type"), "1m", "candle type");
        native_num(
            &field(params, if i == 1 { "size" } else { "limit" }),
            if i == 3 {
                1000.
            } else if i == 4 {
                500.
            } else {
                2.
            },
            "candle limit",
        );
        if i == 1 {
            native_num(&field(params, "start_time"), 100., "history start");
            native_num(&field(params, "end_time"), 0., "history end");
        }
        if i != 5 {
            let v = result.as_ref().unwrap();
            native_len(v, 2, "candles");
            native_num(&field(&idx(v, 0), "timestamp"), 5940., "reversed timestamp");
            native_num(&field(&idx(v, 0), "close"), 3., "reversed close");
            if i == 4 {
                assert!(
                    matches!(&*v.borrow(),Js::A(v) if v[1].is_none()),
                    "DATA native reversed hole"
                );
            } else {
                native_str(&field(&idx(v, 1), "open"), "1", "raw candle open");
                native_num(&field(&idx(v, 1), "timestamp"), 6000., "latest timestamp");
            }
        }
    }
    if family == "candle" || family == "book" {
        assert_eq!(
            h.args,
            vec![(true, true)],
            "DATA native delegated argument aliases"
        );
        if i == 0 {
            assert!(
                Rc::ptr_eq(result.as_ref().unwrap(), &h.ret),
                "DATA native delegated return alias"
            );
            assert_eq!(
                aliases,
                &[true, true, true],
                "DATA native callback alias report"
            );
        }
    }
    if family == "metadata" {
        let v = result.as_ref().unwrap();
        native_len(
            v,
            if i == 2 || i == 3 || i == 5 { 0 } else { 2 },
            "metadata",
        );
        if i == 1 {
            assert!(
                matches!(*idx(v, 0).borrow(), Js::U),
                "DATA native flattened sparse slot"
            );
            native_str(
                &field(&idx(v, 1), "symbol"),
                "ETHUSDT",
                "sparse metadata symbol",
            );
        }
        if i == 0 || i == 4 {
            native_str(
                &field(&idx(v, 0), "symbol"),
                "BTCUSDT",
                "metadata first symbol",
            );
            native_str(
                &field(&idx(v, 1), "symbol"),
                "ETHUSDT",
                "metadata second symbol",
            );
            native_str(
                &field(&idx(v, 1), "positionMode"),
                "hedged",
                "metadata mode",
            );
        }
    }
    if family == "metadata" {
        let expected = if i == 0 || i == 4 {
            2
        } else if i == 1 {
            1
        } else {
            0
        };
        assert_eq!(
            h.metadata_results.len(),
            expected,
            "DATA native metadata retained count"
        );
        for (j, row) in h.metadata_results.iter().enumerate() {
            let out = idx(result.as_ref().unwrap(), if i == 1 { j + 1 } else { j });
            assert!(
                Rc::ptr_eq(row, &out),
                "DATA native metadata row alias iteration {i} result {j}"
            );
        }
    }
    if family == "positions" && i != 5 {
        let v = result.as_ref().unwrap();
        native_len(
            v,
            if i == 1 {
                0
            } else if i == 3 {
                1
            } else if i == 2 {
                3
            } else {
                2
            },
            "positions",
        );
        assert_eq!(
            aliases,
            if i == 1 {
                vec![true, false]
            } else if i == 3 {
                vec![true, true]
            } else {
                vec![false, true]
            },
            "DATA native positions aliases"
        );
        if !matches!(i, 1 | 3) {
            let need = &h.requests[0].2;
            native_len(
                need,
                if i == 2 {
                    2
                } else if i == 4 {
                    0
                } else {
                    1
                },
                "metadata requests",
            );
            if i != 4 {
                native_str(
                    &idx(need, 0),
                    if i == 2 { "BTCUSDT" } else { "ETHUSDT" },
                    "needed symbol",
                );
            }
            let fake = idx(v, 1);
            native_str(&field(&fake, "symbol"), "ETHUSDT", "fake symbol");
            native_str(
                &field(&fake, "id"),
                if i == 4 {
                    "preexisting"
                } else if i == 2 {
                    "ETHUSDT:long"
                } else {
                    "ETHUSDT"
                },
                "fake id",
            );
            native_str(&field(&fake, "extra"), "kept", "spread extension");
            for k in [
                "entryPrice",
                "notional",
                "unrealizedPnl",
                "contracts",
                "liquidationPrice",
            ] {
                native_num(&field(&fake, k), 0., "fake zero");
            }
        }
    }
}
fn corpus() -> &'static Vec<Value> {
    static CORPUS: std::sync::OnceLock<Vec<Value>> = std::sync::OnceLock::new();
    CORPUS.get_or_init(|| {
        let out = std::process::Command::new("node")
            .arg(format!(
                "{}/tests/legacy_data_reads_oracle.cjs",
                env!("CARGO_MANIFEST_DIR")
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "source oracle failed {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(rows.len(), 41, "DATA corpus SIZE");
        let mut n = 0;
        for (family, count) in [
            ("timer", 4),
            ("funding", 7),
            ("touch", 8),
            ("ohlcv", 6),
            ("candle", 2),
            ("book", 2),
            ("metadata", 6),
            ("positions", 6),
        ] {
            for i in 0..count {
                assert_eq!(
                    rows[n]["family"], family,
                    "DATA corpus CONTENT family iteration {n}"
                );
                assert_eq!(rows[n]["i"], i, "DATA corpus CONTENT index iteration {n}");
                n += 1;
            }
        }
        rows
    })
}
fn family(family: &'static str, count: usize) {
    let source = corpus();
    let mut actual = vec![];
    for i in 0..count {
        let (h, input, arg) = setup(family, i);
        let input = enc(&input);
        let result;
        let before;
        let mut aliases = vec![];
        if matches!(family, "timer" | "candle" | "book") {
            result = if family == "timer" {
                start_funding_rates_refresh(h.clone()).map(|()| val(Js::U))
            } else {
                let callback = field(&h.borrow().roots, "callback");
                let mut host = h.borrow_mut();
                if family == "candle" {
                    listen_ohlcv(&mut *host, arg, callback)
                } else {
                    listen_order_book(&mut *host, arg, callback)
                }
            };
            before = h.borrow().trace.clone();
            if family == "timer" {
                let cb = h.borrow().timer_callback.clone();
                if let Some(cb) = cb {
                    run(cb().unwrap()).unwrap();
                }
            }
            if matches!(family, "candle" | "book") {
                aliases.extend([true, true]);
                if i == 0 {
                    aliases.push(true);
                }
            }
        } else {
            let task = match family {
                "funding" => refresh_funding_rates(h.clone()),
                "touch" => fetch_market_ioc_touch_price(
                    h.clone(),
                    arg,
                    strv(if i == 1 { "sell" } else { "buy" }),
                ),
                "ohlcv" => fetch_ohlcv(h.clone(), arg),
                "metadata" => fetch_position_metadata(h.clone(), arg),
                "positions" => {
                    fetch_positions(h.clone(), if i == 3 || i == 4 { Some(arg) } else { None })
                }
                _ => unreachable!(),
            };
            before = h.borrow().trace.clone();
            result = run(task);
        }
        let h = h.borrow();
        if family == "metadata" {
            aliases = h
                .metadata_results
                .iter()
                .map(|row| match &result {
                    Ok(out) => match &*out.borrow() {
                        Js::A(items) => items.iter().flatten().any(|v| Rc::ptr_eq(v, row)),
                        _ => false,
                    },
                    Err(_) => false,
                })
                .collect();
        }
        if family == "positions" {
            if let Ok(v) = &result {
                aliases = [
                    Rc::ptr_eq(v, &h.open),
                    if matches!(&*v.borrow(),Js::A(a) if a.is_empty()) {
                        false
                    } else {
                        Rc::ptr_eq(&idx(v, 0), &idx(&h.open, 0))
                    },
                ]
                .to_vec();
            }
        }
        native(family, i, &h, &result, &before, &aliases);
        let result = match result {
            Ok(v) => json!({"ok":enc(&v)}),
            Err(e) => json!({"error":enc(&field(&e,"message")),"errorAlias":Rc::ptr_eq(&e,&h.err)}),
        };
        let requests = arr(h
            .requests
            .iter()
            .map(|(transport, url, params)| {
                arr(vec![transport.clone(), url.clone(), params.clone()])
            })
            .collect());
        let logs = arr(h
            .logs
            .iter()
            .map(|(full, m)| obj(vec![("full", val(Js::B(*full))), ("message", m.clone())]))
            .collect());
        let updates = arr(h
            .updates
            .iter()
            .map(|(alias, patch)| {
                obj(vec![
                    ("alias", val(Js::B(*alias))),
                    ("patch", patch.clone()),
                ])
            })
            .collect());
        actual.push(json!({"family":family,"i":i,"input":input,"result":result,"before":before,"trace":h.trace,"requests":enc(&requests),"logs":enc(&logs),"updates":enc(&updates),"timerCalls":enc(&arr(h.timer_calls.clone())),"timer":enc(&field(&h.roots,"_fundingRefreshTimer")),"flight":truthy(&field(&h.roots,"_fundingRefreshInFlight")),"disposed":truthy(&field(&h.roots,"isDisposed")),"aliases":aliases}));
    }
    for (i, actual) in actual.iter().enumerate() {
        let expected = source
            .iter()
            .find(|r| r["family"] == family && r["i"] == i)
            .unwrap();
        assert_eq!(actual, expected, "DATA source wire {family} iteration {i}");
    }
}
#[test]
fn timer_native() {
    family("timer", 4)
}
#[test]
fn funding_native() {
    family("funding", 7)
}
#[test]
fn touch_native() {
    family("touch", 8)
}
#[test]
fn ohlcv_native() {
    family("ohlcv", 6)
}
#[test]
fn candle_native() {
    family("candle", 2)
}
#[test]
fn book_native() {
    family("book", 2)
}
#[test]
fn metadata_native() {
    family("metadata", 6)
}
#[test]
fn positions_native() {
    family("positions", 6)
}
#[test]
fn edge_native() {
    let mut wire = vec![];
    let (h, _, arg) = setup("touch", 0);
    h.borrow_mut().capture = 2;
    let result = run(fetch_market_ioc_touch_price(h.clone(), arg, strv("buy"))).unwrap();
    native_num(&result, 12.5, "edge touch price");
    {
        let h = h.borrow();
        assert_eq!(
            h.trace,
            vec!["get:main", "close", "price"],
            "DATA native close before price"
        );
        wire.push(enc(&obj(vec![
            ("family", strv("close")),
            ("price", result),
            ("trace", arr(h.trace.iter().map(|s| strv(s)).collect())),
        ])));
    }
    let (h, _, arg) = setup("positions", 4);
    {
        let mut h = h.borrow_mut();
        h.capture = 3;
        h.includes_target = Some(arg.clone());
        h.metadata = arr(vec![]);
    }
    let result = run(fetch_positions(h.clone(), Some(arg))).unwrap();
    {
        let h = h.borrow();
        native_len(&result, 1, "includes positions");
        native_len(&h.requests[0].2, 0, "includes metadata needed");
        assert_eq!(h.iterator_calls, 0, "DATA native includes ignores iterator");
        assert!(
            Rc::ptr_eq(&idx(&result, 0), &idx(&h.open, 0)),
            "DATA native includes row alias"
        );
        wire.push(enc(&obj(vec![
            ("family", strv("includes")),
            ("needed", h.requests[0].2.clone()),
            ("symbol", field(&idx(&result, 0), "symbol")),
            ("alias", val(Js::B(true))),
            ("iteratorCalls", num(h.iterator_calls as f64)),
        ])));
    }
    let (h, _, _) = setup("funding", 3);
    h.borrow_mut().capture = 4;
    let result = run(refresh_funding_rates(h.clone())).unwrap_err();
    native_str(
        &field(&result, "message"),
        "emitter failure",
        "finally emitter error",
    );
    {
        let h = h.borrow();
        let flag = field(&h.roots, "_fundingRefreshInFlight");
        assert!(
            matches!(*flag.borrow(), Js::B(false)),
            "DATA native finally clears flag after emitter throws"
        );
        wire.push(enc(&obj(vec![
            ("family", strv("finally")),
            ("error", field(&result, "message")),
            ("flight", flag),
            ("trace", arr(h.trace.iter().map(|s| strv(s)).collect())),
        ])));
    }
    let (h, _, _) = setup("funding", 0);
    h.borrow_mut().capture = 5;
    let result = run(refresh_funding_rates(h.clone())).unwrap();
    assert!(
        matches!(*result.borrow(), Js::U),
        "DATA native funding body catch result"
    );
    {
        let h = h.borrow();
        assert_eq!(
            h.trace,
            vec!["get:main", "symbol", "close", "emit"],
            "DATA native funding closes before catch"
        );
        native_str(&h.logs[0].1, "row failure", "funding original row error");
        let flag = field(&h.roots, "_fundingRefreshInFlight");
        assert!(
            matches!(*flag.borrow(), Js::B(false)),
            "DATA native funding body finally flag"
        );
        wire.push(enc(&obj(vec![
            ("family", strv("funding-close")),
            ("message", h.logs[0].1.clone()),
            ("flight", flag),
            ("trace", arr(h.trace.iter().map(|s| strv(s)).collect())),
        ])));
    }
    let (h, _, _) = setup("timer", 0);
    {
        let mut h = h.borrow_mut();
        h.capture = 6;
        h.err = error("timer failure");
    }
    start_funding_rates_refresh(h.clone()).unwrap();
    let cb = h.borrow().timer_callback.clone().unwrap();
    let e = match cb() {
        Err(e) => e,
        Ok(_) => panic!("DATA native timer callback synchronous error"),
    };
    {
        let h = h.borrow();
        assert!(
            Rc::ptr_eq(&e, &h.err),
            "DATA native timer callback error alias"
        );
        native_str(
            &field(&e, "message"),
            "timer failure",
            "timer callback error",
        );
        assert_eq!(
            h.trace,
            vec!["interval", "refresh"],
            "DATA native timer callback sequence"
        );
        wire.push(enc(&obj(vec![
            ("family", strv("timer-call")),
            ("error", field(&e, "message")),
            ("timer", field(&h.roots, "_fundingRefreshTimer")),
            ("trace", arr(h.trace.iter().map(|s| strv(s)).collect())),
        ])));
    }
    let out = std::process::Command::new("node")
        .arg(format!(
            "{}/tests/legacy_data_reads_oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg("--edges")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "edge source failed {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.len(), 5, "DATA edge corpus SIZE");
    for (i, family) in [
        "close",
        "includes",
        "finally",
        "funding-close",
        "timer-call",
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(
            rows[i]["family"],
            enc(&strv(family)),
            "DATA edge corpus CONTENT {i}"
        );
    }
    assert_eq!(wire, rows, "DATA edge source wire");
}
