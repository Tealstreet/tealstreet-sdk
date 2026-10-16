use safe_cex_venue_woo::{legacy_metadata::Task, legacy_order_writes::*};
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
fn idx(v: &V, i: usize) -> V {
    match &*v.borrow() {
        Js::A(a) => a
            .get(i)
            .and_then(Clone::clone)
            .unwrap_or_else(|| val(Js::U)),
        _ => val(Js::U),
    }
}
fn len(v: &V) -> usize {
    match &*v.borrow() {
        Js::A(a) => a.len(),
        _ => 0,
    }
}
fn opt() -> V {
    obj(vec![
        ("symbol", strv("BTCUSDT")),
        ("type", strv("limit")),
        ("side", strv("buy")),
        ("price", num(12.)),
        ("amount", num(2.)),
    ])
}
fn order() -> V {
    obj(vec![
        ("id", strv("oid")),
        ("parentId", strv("parent")),
        ("symbol", strv("BTCUSDT")),
        ("type", strv("limit")),
        ("side", strv("buy")),
        ("price", num(10.)),
        ("amount", num(2.)),
    ])
}
fn hist(id: &str, time: f64) -> V {
    obj(vec![
        ("order_id", strv(id)),
        ("symbol", strv("PERP_BTC_USDT")),
        ("side", strv("BUY")),
        ("type", strv("LIMIT")),
        ("status", strv("FILLED")),
        ("created_time", strv(&safe_cex_num::js_number_string(time))),
        ("quantity", strv("2")),
        ("executed", strv("2")),
        ("price", strv("10")),
        ("realized_pnl", strv("0")),
        ("total_fee", strv("0")),
        ("position_side", strv("LONG")),
    ])
}
fn history_page(rows: Vec<V>, total: f64, per: f64) -> V {
    obj(vec![
        ("rows", arr(rows)),
        (
            "meta",
            obj(vec![("total", num(total)), ("records_per_page", num(per))]),
        ),
    ])
}
struct H {
    roots: V,
    family: &'static str,
    i: usize,
    trace: Vec<String>,
    requests: Vec<(String, Vec<V>)>,
    logs: Vec<V>,
    err: V,
    delegated: V,
    client: V,
    fail: bool,
    missing: bool,
    post: usize,
    get: usize,
    pages: Vec<V>,
    open_rows: V,
    position_row: V,
    input: V,
    sibling: V,
    saved: V,
}
impl H {
    fn record(&mut self, name: &str, a: Vec<V>) {
        self.trace.push(name.into());
        self.requests.push((name.into(), a));
    }
}
impl Host for H {
    type Value = V;
    type Error = V;
    type Prepared = Binding;
    type Iter = (V, usize);
    fn prepare(&mut self, b: Binding) -> Result<Binding, V> {
        if (self.family == "update" && self.i == 5 && b == Binding::Put)
            || (self.family == "history" && self.i == 7 && b == Binding::Get)
        {
            Err(self.err.clone())
        } else {
            Ok(b)
        }
    }
    fn invoke(&mut self, b: Binding, a: Vec<V>) -> Result<V, V> {
        Ok(match b {
            Binding::ResolveClientId => {
                self.record("client", a);
                self.client.clone()
            }
            Binding::IsAlgo => {
                self.record("is-algo", a.clone());
                val(Js::B(matches!(
                    txt(&a[0]).as_str(),
                    "stop_market" | "take_profit_market" | "trailing_stop_market"
                )))
            }
            Binding::AssertClientId => {
                self.record("assert-client", a);
                val(Js::U)
            }
            Binding::Adjust => num(safe_cex_num::adjust(number(&a[0]), txt(&a[1]))),
            Binding::Reverse => strv(&if txt(&a[0]).starts_with("PERP_") {
                txt(&a[0])
            } else {
                format!("PERP_{}", txt(&a[0]).replace("USDT", "_USDT"))
            }),
            Binding::Normalize => strv(&txt(&a[0]).trim_start_matches("PERP_").replace('_', "")),
            Binding::InverseSide => match txt(&a[0]).as_str() {
                "buy" => strv("BUY"),
                "sell" => strv("SELL"),
                _ => val(Js::U),
            },
            Binding::InverseTrigger => match txt(&a[0]).as_str() {
                "mark_price" => strv("MARK_PRICE"),
                "last_price" => strv("MARKET_PRICE"),
                _ => val(Js::U),
            },
            Binding::InverseType => match txt(&a[0]).as_str() {
                "limit" => strv("LIMIT"),
                "market" => strv("BID"),
                "stop_market" => strv("STOP_LOSS"),
                _ => val(Js::U),
            },
            Binding::FairCross => {
                let fair = field(&a[2], "mark");
                let p = number(&a[1]);
                obj(vec![
                    ("fairPrice", fair.clone()),
                    (
                        "wouldCross",
                        val(Js::B(if txt(&a[0]) == "buy" {
                            p >= number(&fair)
                        } else {
                            p <= number(&fair)
                        })),
                    ),
                ])
            }
            Binding::Cross => val(Js::B(if txt(&a[0]) == "buy" {
                number(&a[1]) >= number(&a[2])
            } else {
                number(&a[1]) <= number(&a[2])
            })),
            Binding::PriceRange => {
                let n = number(&field(&field(&a[0], "info"), "price_range"));
                num(if n.is_finite() && n > 0. {
                    n * 100.
                } else {
                    1.
                })
            }
            Binding::ChoosePrice => num(if txt(&a[1]) == "buy" { 12.108 } else { 11.892 }),
            Binding::MapHistorical => {
                let r = &a[0];
                let id = field(r, "order_id");
                let created = number(&field(r, "created_time")) * 1000.;
                obj(vec![
                    ("id", id),
                    ("symbol", strv("BTCUSDT")),
                    ("side", strv("buy")),
                    ("type", strv("limit")),
                    ("status", strv("closed")),
                    ("price", num(10.)),
                    ("averagePrice", val(Js::U)),
                    ("amount", num(2.)),
                    ("filled", num(2.)),
                    ("reduceOnly", val(Js::B(false))),
                    ("positionSide", strv("long")),
                    ("createdTime", num(created)),
                    ("updatedTime", val(Js::U)),
                    ("realizedPnl", num(0.)),
                    ("fees", num(0.)),
                    ("feeAsset", val(Js::U)),
                    ("rebate", val(Js::U)),
                    ("rebateAsset", val(Js::U)),
                ])
            }
            Binding::PositionId => strv(&if txt(&a[2]) == "oneway" {
                txt(&a[0])
            } else {
                format!("{}:{}", txt(&a[0]), txt(&a[1]))
            }),
            Binding::Log => {
                self.trace.push("log".into());
                self.logs.push(obj(vec![
                    ("level", a[1].clone()),
                    ("message", a[0].clone()),
                ]));
                val(Js::U)
            }
            _ => panic!("WRITES native unsupported synchronous binding {b:?}"),
        })
    }
    fn call(&mut self, b: Binding, a: Vec<V>) -> Result<Task<V, V>, V> {
        let name = match b {
            Binding::Put => "put",
            Binding::Post => "post",
            Binding::Get => "get",
            Binding::CancelOrders => "cancel",
            Binding::CancelAlgo => "cancel-algo",
            Binding::Trailing => "trailing",
            Binding::UpdateAlgo => "update-algo",
            Binding::PlaceAlgo => "algo",
            Binding::Stop => "stop",
            Binding::Positional => "positional",
            Binding::PositionSide => "position-side",
            Binding::PositionMode => "position-mode",
            Binding::Touch => "touch",
            _ => panic!("WRITES native unsupported async binding {b:?}"),
        };
        self.record(name, a.clone());
        let r = match b {
            Binding::Put => {
                if self.fail {
                    Err(self.err.clone())
                } else {
                    Ok(obj(vec![("data", obj(vec![]))]))
                }
            }
            Binding::Post => {
                if self.fail {
                    Err(self.err.clone())
                } else {
                    self.post += 1;
                    Ok(if txt(&a[0]) == "/v1/order" {
                        obj(vec![(
                            "data",
                            obj(vec![("order_id", num(self.post as f64))]),
                        )])
                    } else {
                        let d = if self.missing {
                            obj(vec![])
                        } else {
                            obj(vec![(
                                "rows",
                                arr(vec![obj(vec![("orderId", num(self.post as f64))])]),
                            )])
                        };
                        obj(vec![("data", obj(vec![("data", d)]))])
                    })
                }
            }
            Binding::Get => {
                if self.fail {
                    Err(self.err.clone())
                } else {
                    Ok(if txt(&a[0]) == "/v3/positions" {
                        obj(vec![(
                            "data",
                            obj(vec![
                                (
                                    "success",
                                    val(Js::B(!(self.family == "open" && self.i == 4))),
                                ),
                                ("data", obj(vec![("positions", self.open_rows.clone())])),
                            ]),
                        )])
                    } else {
                        let p = self.pages[self.get.min(self.pages.len() - 1)].clone();
                        self.get += 1;
                        obj(vec![("data", p)])
                    })
                }
            }
            Binding::PositionSide => Ok(strv("BOTH")),
            Binding::PositionMode => Ok(strv(if self.family == "open" && self.i == 1 {
                "hedged"
            } else {
                "oneway"
            })),
            Binding::Touch => Ok(num(12.01)),
            _ => Ok(self.delegated.clone()),
        };
        Ok(ready(r))
    }
    fn root(&mut self, k: &str) -> Result<V, V> {
        Ok(field(&self.roots, k))
    }
    fn field(&mut self, v: &V, k: &str) -> Result<V, V> {
        if matches!(*v.borrow(), Js::U | Js::Null) {
            return Err(error(&format!(
                "Cannot read properties of {} (reading '{k}')",
                txt(v)
            )));
        }
        Ok(field(v, k))
    }
    fn keyed(&mut self, v: &V, k: &V) -> Result<V, V> {
        self.field(v, &txt(k))
    }
    fn has(&mut self, v: &V, k: &str) -> Result<bool, V> {
        Ok(matches!(&*v.borrow(),Js::O(o) if o.iter().any(|(key,_)|key==k)))
    }
    fn set(&mut self, v: &V, k: &str, x: V) -> Result<(), V> {
        set(v, k, x);
        Ok(())
    }
    fn truthy(&self, v: &V) -> bool {
        truthy(v)
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(*v.borrow(), Js::U | Js::Null)
    }
    fn is_undefined(&self, v: &V) -> bool {
        matches!(*v.borrow(), Js::U)
    }
    fn is_number(&self, v: &V) -> bool {
        matches!(*v.borrow(), Js::N(_))
    }
    fn is_array(&self, v: &V) -> bool {
        matches!(*v.borrow(), Js::A(_))
    }
    fn equals(&self, v: &V, s: &str) -> bool {
        matches!(&*v.borrow(),Js::S(v) if v==&u(s))
    }
    fn same(&self, a: &V, b: &V) -> bool {
        same(a, b)
    }
    fn number_coerce(&mut self, v: &V) -> Result<f64, V> {
        Ok(number(v))
    }
    fn text(&mut self, v: &V) -> Result<Vec<u16>, V> {
        Ok(match &*v.borrow() {
            Js::S(s) => s.clone(),
            _ => u(&txt(v)),
        })
    }
    fn less(&mut self, a: &V, b: &V) -> Result<bool, V> {
        Ok(match (&*a.borrow(), &*b.borrow()) {
            (Js::S(a), Js::S(b)) => a < b,
            _ => number(a) < number(b),
        })
    }
    fn number(&self, n: f64) -> V {
        num(n)
    }
    fn string(&self, s: Vec<u16>) -> V {
        val(Js::S(s))
    }
    fn boolean(&self, b: bool) -> V {
        val(Js::B(b))
    }
    fn undefined(&self) -> V {
        val(Js::U)
    }
    fn null(&self) -> V {
        val(Js::Null)
    }
    fn object(&self, f: Vec<(&'static str, V)>) -> V {
        obj(f)
    }
    fn array(&self, a: Vec<Option<V>>) -> V {
        val(Js::A(a))
    }
    fn spread_object(&mut self, v: &V, f: Vec<(&'static str, V)>) -> Result<V, V> {
        let out = match &*v.borrow() {
            Js::O(o) => val(Js::O(o.clone())),
            _ => obj(vec![]),
        };
        for (k, v) in f {
            set(&out, k, v);
        }
        Ok(out)
    }
    fn omit_undefined(&mut self, v: V) -> Result<V, V> {
        let Js::O(o) = &*v.borrow() else {
            panic!("object")
        };
        Ok(val(Js::O(
            o.iter()
                .filter(|(_, v)| !matches!(*v.borrow(), Js::U))
                .cloned()
                .collect(),
        )))
    }
    fn length(&mut self, v: &V) -> Result<usize, V> {
        Ok(len(v))
    }
    fn index(&mut self, v: &V, i: usize) -> Result<V, V> {
        Ok(idx(v, i))
    }
    fn has_index(&mut self, v: &V, i: usize) -> Result<bool, V> {
        Ok(matches!(&*v.borrow(),Js::A(a) if a.get(i).is_some_and(Option::is_some)))
    }
    fn push(&mut self, v: &V, x: V) -> Result<(), V> {
        let Js::A(a) = &mut *v.borrow_mut() else {
            panic!("array")
        };
        a.push(Some(x));
        Ok(())
    }
    fn times(&mut self, n: f64, req: V, key: &'static str, qty: V) -> Result<V, V> {
        let mut out = vec![];
        for _ in 0..n as usize {
            out.push(self.spread_object(&req, vec![(key, qty.clone())])?);
        }
        Ok(arr(out))
    }
    fn flatten_one(&mut self, v: V) -> Result<V, V> {
        let mut out = vec![];
        for i in 0..len(&v) {
            let a = idx(&v, i);
            for j in 0..len(&a) {
                out.push(idx(&a, j));
            }
        }
        Ok(arr(out))
    }
    fn spread_array(&mut self, v: &V) -> Result<Vec<V>, V> {
        Ok((0..len(v)).map(|i| idx(v, i)).collect())
    }
    fn iterator(&mut self, v: V) -> Result<Self::Iter, V> {
        Ok((v, 0))
    }
    fn next(&mut self, it: &mut Self::Iter) -> Result<Option<V>, V> {
        if it.1 >= len(&it.0) {
            Ok(None)
        } else {
            let v = idx(&it.0, it.1);
            it.1 += 1;
            Ok(Some(v))
        }
    }
    fn close(&mut self, _: &mut Self::Iter) -> Result<(), V> {
        Ok(())
    }
    fn await_element(&mut self, v: V) -> Task<V, V> {
        ready(Ok(v))
    }
    fn sort_history(&mut self, v: V) -> Result<V, V> {
        {
            let mut value = v.borrow_mut();
            let Js::A(a) = &mut *value else {
                panic!("array")
            };
            a.sort_by(|a, b| {
                number(&field(b.as_ref().unwrap(), "createdTime"))
                    .partial_cmp(&number(&field(a.as_ref().unwrap(), "createdTime")))
                    .unwrap()
            });
        }
        Ok(v)
    }
    fn slice(&mut self, v: V, limit: V) -> Result<V, V> {
        Ok(arr((0..len(&v).min(number(&limit) as usize))
            .map(|i| idx(&v, i))
            .collect()))
    }
    fn v(&mut self, v: &V, k: &str) -> Result<V, V> {
        Ok(field(v, k))
    }
    fn error(&mut self, m: V, _: bool) -> V {
        obj(vec![("message", m)])
    }
    fn error_field(&mut self, e: &V, p: &[&str]) -> Result<V, V> {
        let mut v = e.clone();
        for k in p {
            v = field(&v, k);
        }
        Ok(v)
    }
    fn emit(&mut self, _: Binding, e: Option<&V>, m: V) -> Result<(), V> {
        self.trace.push("emit".into());
        self.logs.push(obj(vec![
            (
                "full",
                val(Js::B(e.is_some_and(|e| Rc::ptr_eq(e, &self.err)))),
            ),
            ("message", m),
        ]));
        Ok(())
    }
}
fn setup(family: &'static str, i: usize) -> (Rc<RefCell<H>>, V) {
    let mut input = opt();
    let market = obj(vec![
        ("limits", obj(vec![("amount", obj(vec![("max", num(3.))]))])),
        (
            "precision",
            obj(vec![("price", num(0.1)), ("amount", num(0.1))]),
        ),
        ("info", obj(vec![("price_range", num(0.01))])),
    ]);
    let ticker = obj(vec![
        ("mark", num(12.)),
        ("last", num(10.)),
        ("ask", num(12.01)),
        ("bid", num(11.99)),
    ]);
    let position = obj(vec![
        ("symbol", strv("BTCUSDT")),
        ("side", strv("short")),
        ("contracts", strv("3")),
    ]);
    let saved = arr(vec![obj(vec![("saved", val(Js::B(true)))])]);
    let position_row = obj(vec![
        ("symbol", strv("PERP_BTC_USDT")),
        ("holding", num(2.)),
        ("averageOpenPrice", strv("10")),
        ("estLiqPrice", strv("3")),
        ("leverage", num(5.)),
        ("marginMode", strv("CROSS")),
    ]);
    let markets = obj(vec![("BTCUSDT", market)]);
    let positions = arr(vec![position]);
    let orders = arr(vec![]);
    let roots = obj(vec![
        ("marketsBySymbol", markets.clone()),
        ("tickersBySymbol", obj(vec![("BTCUSDT", ticker)])),
        ("positions", positions.clone()),
        ("orders", orders.clone()),
        ("isPublicDataOnly", val(Js::B(false))),
        ("isDisposed", val(Js::B(false))),
    ]);
    let mut client = val(Js::U);
    let mut fail = false;
    let mut missing = false;
    let mut pages = vec![history_page(vec![hist("a", 1.), hist("b", 2.)], 2., 100.)];
    let mut open_rows = arr(vec![position_row.clone()]);
    let mut sibling = val(Js::U);
    match family {
        "update" => {
            let o = order();
            let mut update = obj(vec![("price", num(13.))]);
            if i == 1 {
                update = obj(vec![("amount", num(0.))]);
            }
            if i == 2 {
                set(&o, "type", strv("trailing_stop_market"));
            }
            if i == 3 {
                set(&o, "type", strv("stop_market"));
            }
            if i == 4 {
                fail = true;
            }
            input = obj(vec![("order", o), ("update", update)]);
        }
        "update-algo" => {
            let o = order();
            let mut update = obj(vec![("price", num(0.))]);
            if i == 0 {
                set(&o, "amount", num(0.));
            }
            if i == 2 {
                update = obj(vec![("amount", num(9.))]);
            }
            if i == 3 {
                fail = true;
            }
            input = obj(vec![("order", o), ("update", update)]);
        }
        "place" => match i {
            0 => set(&input, "amount", num(8.)),
            1 => client = strv("23"),
            2 => set(&input, "type", strv("stop_market")),
            3 => set(&input, "type", strv("market")),
            4 => {
                set(&input, "timeInForce", strv("PostOnly"));
                set(&input, "price", num(20.));
            }
            5 => {
                set(&input, "stopLoss", num(9.));
                set(&input, "takeProfit", num(15.));
            }
            6 => fail = true,
            _ => panic!("case"),
        },
        "algo" => {
            set(&input, "type", strv("stop_market"));
            if i == 0 {
                set(&input, "price", num(0.));
            }
            if i == 1 {
                set(&input, "type", strv("trailing_stop_market"));
            }
            if i == 3 || i == 4 {
                set(&input, "amount", num(0.));
            }
            if i == 3 {
                sibling = obj(vec![
                    ("id", strv("sibling")),
                    ("symbol", strv("BTCUSDT")),
                    ("side", strv("buy")),
                    ("type", strv("take_profit_market")),
                    ("status", strv("open")),
                    ("price", num(15.)),
                ]);
                set(&roots, "orders", arr(vec![sibling.clone()]));
            }
        }
        "stop" => {
            set(&input, "type", strv("stop_market"));
            if i == 0 {
                set(&input, "amount", num(4.));
            }
            if i == 1 {
                set(&input, "slTriggerType", strv("last_price"));
                set(&input, "tpTriggerType", strv("index_price"));
            }
            if i == 2 {
                fail = true;
            }
            if i == 3 {
                set(&roots, "marketsBySymbol", obj(vec![]));
            }
        }
        "trailing" => {
            set(&input, "type", strv("trailing_stop_market"));
            if i == 0 {
                set(&input, "tpTriggerType", strv("last_price"));
            }
            if i == 1 {
                fail = true;
            }
            if i == 2 {
                missing = true;
            }
            if i == 3 {
                set(&roots, "positions", arr(vec![]));
            }
        }
        "positional" => {
            set(&input, "stopLoss", num(9.));
            set(&input, "takeProfit", num(15.));
            set(&input, "side", strv("sell"));
            set(&input, "slTriggerType", strv("last_price"));
            if i == 1 {
                set(&input, "stopLoss", num(0.));
                set(&input, "takeProfit", num(0.));
            }
            if i == 2 {
                fail = true;
            }
            if i == 3 {
                missing = true;
            }
        }
        "history" => {
            input = obj(vec![
                ("symbol", strv("BTCUSDT")),
                ("from", num(1000000000000.)),
                ("to", num(2000000000000.)),
            ]);
            if i == 1 {
                pages = vec![
                    history_page(vec![hist("a", 1.)], 3., 1.),
                    history_page(vec![hist("b", 2.), hist("a", 10.)], 3., 2.),
                ];
                set(&input, "limit", num(1.));
            }
            if i == 2 {
                set(&roots, "isPublicDataOnly", val(Js::B(true)));
            }
            if i == 3 {
                set(&input, "symbol", strv("UNKNOWN"));
            }
            if i == 4 {
                pages = vec![obj(vec![
                    ("success", val(Js::B(false))),
                    ("meta", obj(vec![("message", strv("history failed"))])),
                ])];
            }
            if i == 5 {
                fail = true;
            }
            if i == 6 {
                pages = vec![history_page(vec![hist("a", 1.)], 10000., 1.)];
            }
        }
        "open" => {
            input = val(Js::U);
            if i == 1 {
                set(&position_row, "holding", num(-3.));
            }
            if i == 2 {
                set(&position_row, "marginMode", strv("ISOLATED"));
            }
            if i == 3 {
                fail = true;
            }
            if i == 5 {
                open_rows = obj(vec![("bad", val(Js::B(true)))]);
            }
            if i == 6 {
                set(
                    &field(&field(&roots, "tickersBySymbol"), "BTCUSDT"),
                    "mark",
                    strv("9"),
                );
            }
            set(&roots, "positions", saved.clone());
        }
        _ => panic!("case"),
    }
    let err = error("top failure");
    set(
        &err,
        "response",
        obj(vec![(
            "data",
            obj(vec![("message", strv("nested failure"))]),
        )]),
    );
    let h = H {
        roots,
        family,
        i,
        trace: vec![],
        requests: vec![],
        logs: vec![],
        err,
        delegated: arr(vec![strv("delegated")]),
        client,
        fail,
        missing,
        post: 0,
        get: 0,
        pages,
        open_rows,
        position_row,
        input: input.clone(),
        sibling,
        saved,
    };
    (Rc::new(RefCell::new(h)), input)
}
fn native_str(v: &V, s: &str, label: &str) {
    assert!(
        matches!(&*v.borrow(),Js::S(v) if v==&u(s)),
        "WRITES native {label}: {v:?}"
    );
}
fn native_num(v: &V, n: f64, label: &str) {
    assert!(
        matches!(&*v.borrow(),Js::N(v) if v.to_bits()==n.to_bits()),
        "WRITES native {label}: {v:?}"
    );
}
fn native_assert(h: &H, result: &Result<V, V>, before: &[String]) {
    let f = h.family;
    let i = h.i;
    let error_expected = matches!(
        (f, i),
        ("update", 4 | 5)
            | ("update-algo", 3)
            | ("place", 4 | 6)
            | ("stop", 2 | 3)
            | ("trailing", 3)
            | ("positional", 2)
    );
    assert_eq!(
        result.is_err(),
        error_expected,
        "WRITES native {f} outcome iteration {i}"
    );
    if let Err(e) = result {
        if !matches!((f, i), ("place", 4) | ("stop", 3) | ("trailing", 3)) {
            assert!(Rc::ptr_eq(e, &h.err), "WRITES native original write error");
        } else {
            native_str(
                &field(e, "message"),
                match (f, i) {
                    ("place", 4) => "WOO post-only buy order would cross current fair price 12",
                    ("stop", 3) => "Market BTCUSDT not found",
                    _ => "Position BTCUSDT and side short not found",
                },
                "preflight message",
            );
        }
    }
    let req = |name: &str| {
        h.requests
            .iter()
            .filter(|(n, _)| n == name)
            .collect::<Vec<_>>()
    };
    match f {
        "update" => {
            let expected = match i {
                2 => vec!["cancel", "trailing"],
                3 => vec!["is-algo", "update-algo"],
                4 => vec!["is-algo", "put", "emit"],
                5 => vec!["is-algo", "emit"],
                _ => vec!["is-algo", "put"],
            };
            assert_eq!(h.trace, expected, "WRITES native update sequence");
            if i == 0 || i == 1 || i == 4 {
                let r = req("put");
                assert_eq!(r.len(), 1, "WRITES native edit request count");
                native_str(&r[0].1[0], "/v3/order/oid", "edit route");
                let p = &r[0].1[1];
                native_str(
                    &field(p, "quantity"),
                    if i == 1 { "0" } else { "2" },
                    "required edit quantity",
                );
                native_str(&field(p, "orderId"), "oid", "edit id");
                if i != 1 {
                    native_str(&field(p, "price"), "13", "edit price");
                }
            }
            if i == 2 {
                let c = req("cancel");
                assert!(
                    Rc::ptr_eq(&idx(&c[0].1[0], 0), &field(&h.input, "order")),
                    "WRITES native canceled original order"
                );
                let r = req("trailing");
                native_str(
                    &field(&r[0].1[0], "type"),
                    "trailing_stop_market",
                    "trailing replacement type",
                );
                native_num(
                    &field(&r[0].1[0], "price"),
                    13.,
                    "trailing replacement price",
                );
            }
            if i == 2 || i == 3 {
                assert!(
                    Rc::ptr_eq(result.as_ref().unwrap(), &h.delegated),
                    "WRITES native delegated result"
                );
            }
            if i == 0 || i == 1 {
                native_str(&idx(result.as_ref().unwrap(), 0), "oid", "edited return id");
            }
            if i == 5 {
                assert_eq!(
                    before,
                    ["is-algo", "emit"],
                    "WRITES native eager edit catch"
                );
            }
        }
        "update-algo" => {
            let expected = if i == 3 {
                vec!["put", "emit"]
            } else {
                vec!["put"]
            };
            assert_eq!(h.trace, expected, "WRITES native algo edit sequence");
            let r = req("put");
            native_str(
                &r[0].1[0],
                "/v3/algo/order/parent",
                "algo edit parent route",
            );
            let p = &r[0].1[1];
            if i == 0 {
                native_str(
                    &field(&idx(&field(p, "childOrders"), 0), "algoOrderId"),
                    "oid",
                    "algo edit child id",
                );
                native_str(
                    &field(&idx(&field(p, "childOrders"), 0), "triggerPrice"),
                    "0",
                    "algo edit zero price",
                );
            }
            if i == 1 || i == 3 {
                native_str(&field(p, "triggerPrice"), "0", "algo edit zero price");
            }
            if i == 2 {
                assert!(
                    matches!(&*p.borrow(),Js::O(a) if a.is_empty()),
                    "WRITES native amount-only empty edit"
                );
            }
        }
        "place" => {
            let expected = match i {
                2 => vec!["client", "is-algo", "algo"],
                3 => vec![
                    "client",
                    "is-algo",
                    "touch",
                    "position-side",
                    "assert-client",
                    "post",
                ],
                4 => vec!["client", "is-algo"],
                5 => vec![
                    "client",
                    "is-algo",
                    "position-side",
                    "assert-client",
                    "post",
                    "positional",
                ],
                6 => vec![
                    "client",
                    "is-algo",
                    "position-side",
                    "assert-client",
                    "post",
                    "emit",
                ],
                0 => vec![
                    "client",
                    "is-algo",
                    "position-side",
                    "assert-client",
                    "post",
                    "post",
                    "post",
                    "post",
                ],
                _ => vec![
                    "client",
                    "is-algo",
                    "position-side",
                    "assert-client",
                    "post",
                ],
            };
            assert_eq!(
                h.trace, expected,
                "WRITES native regular placement sequence"
            );
            if i == 2 {
                assert!(
                    Rc::ptr_eq(result.as_ref().unwrap(), &h.delegated),
                    "WRITES native delegated placement result"
                );
                assert!(
                    Rc::ptr_eq(&req("algo")[0].1[0], &h.input),
                    "WRITES native algo options identity"
                );
            }
            if i != 2 && i != 4 {
                let r = req("post");
                assert_eq!(
                    r.len(),
                    if i == 0 { 4 } else { 1 },
                    "WRITES native split lot count"
                );
                for row in r {
                    native_str(&row.1[0], "/v1/order", "native place endpoint");
                    let p = &row.1[1];
                    native_num(&field(p, "order_quantity"), 2., "native lot quantity");
                    native_str(
                        &field(p, "order_type"),
                        if i == 3 { "IOC" } else { "LIMIT" },
                        "native order type",
                    );
                    native_num(
                        &field(p, "order_price"),
                        if i == 3 { 12.1 } else { 12. },
                        "native IOC price",
                    );
                    native_str(&field(p, "position_side"), "BOTH", "native position side");
                    if i == 1 {
                        native_num(&field(p, "client_order_id"), 23., "native client number");
                    } else {
                        assert!(
                            matches!(*field(p, "client_order_id").borrow(), Js::U),
                            "WRITES native omitted client id"
                        );
                    }
                }
            }
            if i == 5 {
                let r = req("positional");
                native_str(&field(&r[0].1[0], "side"), "sell", "attached opposite side");
                assert_eq!(
                    len(result.as_ref().unwrap()),
                    2,
                    "WRITES native appended algo id count"
                );
                native_str(
                    &idx(result.as_ref().unwrap(), 1),
                    "delegated",
                    "appended algo id",
                );
            }
            if i == 6 {
                assert!(
                    truthy(&field(&h.logs[0], "full")),
                    "WRITES native regular full original log"
                );
            }
        }
        "algo" => {
            let expected = match i {
                0 => vec!["emit"],
                1 => vec!["trailing"],
                2 => vec!["stop"],
                3 => vec!["cancel-algo", "positional"],
                _ => vec!["positional"],
            };
            assert_eq!(
                h.trace, expected,
                "WRITES native algo routing and merge sequence"
            );
            if i == 0 {
                assert_eq!(
                    len(result.as_ref().unwrap()),
                    0,
                    "WRITES native missing-price empty result"
                );
                native_str(
                    &field(&h.logs[0], "message"),
                    "Price is required for algo orders",
                    "algo missing price log",
                );
            } else {
                assert!(
                    Rc::ptr_eq(result.as_ref().unwrap(), &h.delegated),
                    "WRITES native algo original result"
                );
            }
            if i == 3 {
                let c = req("cancel-algo");
                assert!(
                    Rc::ptr_eq(&c[0].1[0], &h.sibling),
                    "WRITES native sibling identity"
                );
                assert!(
                    matches!(*c[0].1[1].borrow(), Js::B(false)),
                    "WRITES native no sibling replacement"
                );
            }
            if i == 3 || i == 4 {
                let r = req("positional");
                native_num(&field(&r[0].1[0], "stopLoss"), 12., "merged stop loss");
                native_num(
                    &field(&r[0].1[0], "takeProfit"),
                    if i == 3 { 15. } else { 0. },
                    "merged take profit",
                );
            }
            if i == 4 {
                assert_eq!(
                    before,
                    ["positional"],
                    "WRITES native eager no-sibling delegation"
                );
            }
        }
        "stop" => {
            let expected = match i {
                0 => vec!["position-side", "post", "post", "post"],
                2 => vec!["position-side", "post", "emit"],
                3 => vec![],
                _ => vec!["position-side", "post"],
            };
            assert_eq!(h.trace, expected, "WRITES native stop placement sequence");
            if i != 3 {
                let ps = req("position-side");
                assert!(
                    !Rc::ptr_eq(&ps[0].1[0], &h.input),
                    "WRITES native cloned stop side options"
                );
                assert!(
                    truthy(&field(&ps[0].1[0], "reduceOnly")),
                    "WRITES native forced stop reduction"
                );
                let r = req("post");
                assert_eq!(
                    r.len(),
                    if i == 0 { 3 } else { 1 },
                    "WRITES native stop split count"
                );
                for (j, row) in r.iter().enumerate() {
                    let p = &row.1[1];
                    native_str(&row.1[0], "/v3/algo/order", "stop route");
                    native_str(&field(p, "algoType"), "STOP", "stop algo type");
                    native_num(
                        &field(p, "quantity"),
                        if i == 0 {
                            if j < 2 { 1.5 } else { 1. }
                        } else {
                            2.
                        },
                        "stop lot quantity",
                    );
                    native_str(
                        &field(p, "triggerPriceType"),
                        if i == 1 { "MARKET_PRICE" } else { "MARK_PRICE" },
                        "stop trigger selection",
                    );
                }
            }
            if i == 2 {
                assert!(
                    truthy(&field(&h.logs[0], "full")),
                    "WRITES native stop full error log"
                );
            }
        }
        "trailing" => {
            let expected = match i {
                1 => vec!["position-side", "post", "emit"],
                3 => vec![],
                _ => vec!["position-side", "post"],
            };
            assert_eq!(h.trace, expected, "WRITES native trailing sequence");
            if i != 3 {
                let r = req("post");
                let p = &r[0].1[1];
                native_str(&field(p, "callbackValue"), "2", "trailing distance string");
                native_str(
                    &field(p, "quantity"),
                    "3",
                    "trailing position contracts string",
                );
                native_str(
                    &field(p, "triggerPriceType"),
                    if i == 0 { "MARKET_PRICE" } else { "MARK_PRICE" },
                    "trailing tp trigger selector",
                );
            }
            if i == 1 || i == 2 {
                assert_eq!(
                    len(result.as_ref().unwrap()),
                    0,
                    "WRITES native trailing empty fallback"
                );
            }
        }
        "positional" => {
            let expected = if i == 2 {
                vec!["position-side", "post", "emit"]
            } else {
                vec!["position-side", "post"]
            };
            assert_eq!(h.trace, expected, "WRITES native positional sequence");
            let r = req("post");
            let p = &r[0].1[1];
            native_str(
                &field(p, "algoType"),
                "POSITIONAL_TP_SL",
                "positional algo type",
            );
            let children = field(p, "childOrders");
            assert_eq!(
                len(&children),
                if i == 1 { 0 } else { 2 },
                "WRITES native positional child count"
            );
            if i != 1 {
                let sl = idx(&children, 0);
                let tp = idx(&children, 1);
                native_str(
                    &field(&sl, "algoType"),
                    "STOP_LOSS",
                    "first positional child",
                );
                native_str(
                    &field(&tp, "algoType"),
                    "TAKE_PROFIT",
                    "second positional child",
                );
                native_str(
                    &field(&sl, "triggerPriceType"),
                    "MARKET_PRICE",
                    "sl trigger",
                );
                native_str(&field(&tp, "triggerPriceType"), "MARK_PRICE", "tp trigger");
                native_str(&field(&sl, "triggerPrice"), "9", "sl price string");
                native_str(&field(&tp, "triggerPrice"), "15", "tp price string");
                native_str(&field(&sl, "side"), "SELL", "positional closing side");
            }
            if i == 3 {
                assert_eq!(
                    len(result.as_ref().unwrap()),
                    0,
                    "WRITES native missing rows optional fallback"
                );
            }
        }
        "history" => {
            let count = match i {
                1 => 2,
                2 | 3 | 7 => 0,
                6 => 50,
                _ => 1,
            };
            let r = req("get");
            assert_eq!(
                r.len(),
                count,
                "WRITES native history independent request count"
            );
            for (j, row) in r.iter().enumerate() {
                native_str(&row.1[0], "/v1/orders", "history route");
                let p = field(&row.1[1], "params");
                native_num(&field(&p, "page"), (j + 1) as f64, "advancing history page");
                native_num(
                    &field(&p, "start_t"),
                    1000000000000.,
                    "history milliseconds start",
                );
                native_num(
                    &field(&p, "end_t"),
                    2000000000000.,
                    "history milliseconds end",
                );
                assert!(
                    matches!(*field(&p, "realized_pnl").borrow(), Js::B(true)),
                    "WRITES native requested PnL fields"
                );
                native_num(&field(&p, "size"), 100., "history independent page size");
            }
            let v = result.as_ref().unwrap();
            assert_eq!(
                len(v),
                match i {
                    0 => 2,
                    1 | 6 => 1,
                    _ => 0,
                },
                "WRITES native dedupe sorted history count"
            );
            if i == 0 || i == 1 {
                native_str(&field(&idx(v, 0), "id"), "b", "history newest id");
                native_num(
                    &field(&idx(v, 0), "createdTime"),
                    2000.,
                    "history newest native time",
                );
                native_num(
                    &field(&idx(v, 0), "realizedPnl"),
                    0.,
                    "history native zero PnL",
                );
            }
            if i == 4 {
                native_str(
                    &field(&h.logs[0], "message"),
                    "history failed",
                    "history service failure",
                );
            }
            if i == 7 {
                assert_eq!(before, ["emit"], "WRITES native eager history catch");
            }
            if i == 6 {
                assert_eq!(h.logs.len(), 1, "WRITES native cap warning count");
                native_str(
                    &field(&h.logs[0], "level"),
                    "warning",
                    "history cap severity",
                );
            }
        }
        "open" => {
            let expected = if i == 3 || i == 4 {
                vec!["get", "emit"]
            } else if i == 2 || i == 5 {
                vec!["get", "position-mode", "emit"]
            } else {
                vec!["get", "position-mode"]
            };
            assert_eq!(
                h.trace, expected,
                "WRITES native open-position read sequence"
            );
            let v = result.as_ref().unwrap();
            if (3..=5).contains(&i) {
                assert!(
                    Rc::ptr_eq(v, &h.saved),
                    "WRITES native position fallback reference"
                );
            } else {
                assert_eq!(len(v), 1, "WRITES native mapped open count");
                let p = idx(v, 0);
                native_str(&field(&p, "entryPrice"), "10", "raw native position entry");
                native_num(
                    &field(&p, "contracts"),
                    if i == 1 { 3. } else { 2. },
                    "native position contracts",
                );
                native_num(
                    &field(&p, "unrealizedPnl"),
                    if i == 1 {
                        -6.
                    } else if i == 6 {
                        2.
                    } else {
                        4.
                    },
                    "native position PnL",
                );
                native_num(
                    &field(&p, "notional"),
                    if i == 1 {
                        36.
                    } else if i == 6 {
                        18.
                    } else {
                        24.
                    },
                    "native position notional",
                );
                native_str(
                    &field(&p, "id"),
                    if i == 1 { "BTCUSDT:short" } else { "BTCUSDT" },
                    "native position id",
                );
                assert!(
                    Rc::ptr_eq(&field(&p, "info"), &h.position_row),
                    "WRITES native original position info"
                );
            }
        }
        _ => panic!("family"),
    }
}
fn corpus() -> &'static Vec<Value> {
    static C: std::sync::OnceLock<Vec<Value>> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        let out = std::process::Command::new("node")
            .arg(format!(
                "{}/tests/legacy_order_writes_oracle.cjs",
                env!("CARGO_MANIFEST_DIR")
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "source failed {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(rows.len(), 49, "WRITES corpus SIZE");
        let mut n = 0;
        for (f, c) in [
            ("update", 6),
            ("update-algo", 4),
            ("place", 7),
            ("algo", 5),
            ("stop", 4),
            ("trailing", 4),
            ("positional", 4),
            ("history", 8),
            ("open", 7),
        ] {
            for i in 0..c {
                assert_eq!(
                    rows[n]["family"], f,
                    "WRITES corpus CONTENT family iteration {n}"
                );
                assert_eq!(rows[n]["i"], i, "WRITES corpus CONTENT index iteration {n}");
                n += 1;
            }
        }
        rows
    })
}
fn family(f: &'static str, count: usize) {
    let mut actual = vec![];
    for i in 0..count {
        let (h, input) = setup(f, i);
        let encoded_input = enc(&input);
        let task = match f {
            "update" => update_order(h.clone(), input),
            "update-algo" => update_algo_order(h.clone(), input),
            "place" => place_order(h.clone(), input),
            "algo" => place_algo_order(h.clone(), input),
            "stop" => place_stop_market(h.clone(), input),
            "trailing" => place_trailing_stop_loss_order(h.clone(), input),
            "positional" => place_positional_algo_order(h.clone(), input),
            "history" => fetch_historical_orders(h.clone(), input),
            "open" => fetch_open_positions(h.clone()),
            _ => panic!("family"),
        };
        let before = h.borrow().trace.clone();
        let result = run(task);
        let h = h.borrow();
        native_assert(&h, &result, &before);
        let error_alias = result.as_ref().err().is_some_and(|e| Rc::ptr_eq(e, &h.err));
        let result_alias = result
            .as_ref()
            .is_ok_and(|v| Rc::ptr_eq(v, &h.saved) || Rc::ptr_eq(v, &h.delegated));
        let info_alias = if f == "open" {
            result
                .as_ref()
                .map(|v| {
                    if Rc::ptr_eq(v, &h.saved) {
                        vec![]
                    } else {
                        (0..len(v))
                            .map(|i| Rc::ptr_eq(&field(&idx(v, i), "info"), &h.position_row))
                            .collect()
                    }
                })
                .unwrap_or_default()
        } else {
            vec![]
        };
        let input_alias = h
            .requests
            .iter()
            .filter(|(n, _)| {
                matches!(
                    n.as_str(),
                    "client" | "algo" | "trailing" | "stop" | "position-side"
                )
            })
            .map(|(n, a)| json!([n, Rc::ptr_eq(&a[0], &h.input)]))
            .collect::<Vec<_>>();
        let sibling_alias = h
            .requests
            .iter()
            .filter(|(n, _)| n == "cancel-algo")
            .map(|(_, a)| Rc::ptr_eq(&a[0], &h.sibling))
            .collect::<Vec<_>>();
        let encoded_result = match result {
            Ok(v) => json!({"ok":enc(&v)}),
            Err(e) => json!({"error":enc(&field(&e,"message"))}),
        };
        let requests = h
            .requests
            .iter()
            .map(|(n, a)| json!([enc(&strv(n)), a.iter().map(enc).collect::<Vec<_>>()]))
            .collect::<Vec<_>>();
        actual.push(json!({"family":f,"i":i,"input":encoded_input,"result":encoded_result,"before":before,"trace":h.trace,"requests":requests,"logs":h.logs.iter().map(enc).collect::<Vec<_>>(),"errorAlias":error_alias,"resultAlias":result_alias,"inputAlias":input_alias,"siblingAlias":sibling_alias,"infoAlias":info_alias}));
    }
    let corpus = corpus();
    for (i, row) in actual.iter().enumerate() {
        let source = corpus
            .iter()
            .find(|r| r["family"] == f && r["i"] == i)
            .unwrap();
        assert_eq!(row, source, "WRITES source {f} iteration {i}");
    }
}
#[test]
fn update_native() {
    family("update", 6)
}
#[test]
fn update_algo_native() {
    family("update-algo", 4)
}
#[test]
fn place_native() {
    family("place", 7)
}
#[test]
fn algo_native() {
    family("algo", 5)
}
#[test]
fn stop_native() {
    family("stop", 4)
}
#[test]
fn trailing_native() {
    family("trailing", 4)
}
#[test]
fn positional_native() {
    family("positional", 4)
}
#[test]
fn history_native() {
    family("history", 8)
}
#[test]
fn open_native() {
    family("open", 7)
}
