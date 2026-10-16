use safe_cex_venue_woo::{legacy_fill_recovery::*, legacy_metadata::Task};
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
fn pos(contracts: V, side: &str, entry: V) -> V {
    obj(vec![
        ("id", strv("BTCUSDT")),
        ("symbol", strv("BTCUSDT")),
        ("side", strv(side)),
        ("contracts", contracts),
        ("entryPrice", entry),
        ("positionMode", strv("oneway")),
        ("leverage", num(5.)),
        ("marginType", strv("cross")),
        ("unrealizedPnl", num(4.)),
        ("liquidationPrice", num(6.)),
    ])
}
fn fill() -> V {
    obj(vec![
        ("symbol", strv("BTCUSDT")),
        ("side", strv("buy")),
        ("price", num(12.)),
        ("amount", num(1.)),
    ])
}
fn report(status: &str) -> V {
    obj(vec![
        ("status", strv(status)),
        ("algoStatus", strv(status)),
        ("orderId", strv("oid")),
        ("order_id", strv("snake")),
        ("algoOrderId", strv("aid")),
        ("id", strv("fallback")),
        ("side", strv("SELL")),
        ("symbol", strv("PERP_ETH_USDT")),
        ("executedPrice", strv("11")),
        ("executedQuantity", num(3.)),
        ("averageExecutedPrice", strv("14")),
        ("totalExecutedQuantity", num(4.)),
        ("executedTimestamp", num(1700000000123.)),
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
type M = Rc<RefCell<Vec<(V, Option<V>)>>>;
fn empty_map() -> M {
    Rc::new(RefCell::new(vec![]))
}
fn mget(m: &M, k: &V) -> Option<V> {
    m.borrow()
        .iter()
        .rev()
        .find(|(key, v)| v.is_some() && same(key, k))
        .and_then(|(_, v)| v.clone())
}
fn mset(m: &M, k: V, v: V) {
    let mut a = m.borrow_mut();
    if let Some((_, old)) = a
        .iter_mut()
        .rev()
        .find(|(key, v)| v.is_some() && same(key, &k))
    {
        *old = Some(v);
    } else {
        a.push((k, Some(v)));
    }
}
fn mdelete(m: &M, k: &V) {
    for (key, v) in m.borrow_mut().iter_mut() {
        if same(key, k) {
            *v = None;
        }
    }
}
fn mclear(m: &M) {
    for (_, v) in m.borrow_mut().iter_mut() {
        *v = None;
    }
}
fn menc(m: &M) -> V {
    arr(m
        .borrow()
        .iter()
        .filter_map(|(k, v)| v.as_ref().map(|v| arr(vec![k.clone(), v.clone()])))
        .collect())
}
fn mkeys(m: &M) -> Vec<String> {
    m.borrow()
        .iter()
        .filter(|(_, v)| v.is_some())
        .map(|(k, _)| txt(k))
        .collect()
}
fn map_name(m: Map) -> &'static str {
    match m {
        Map::Timers => "timers",
        Map::Flight => "flight",
        Map::Expected => "expected",
    }
}
fn map_index(m: Map) -> usize {
    match m {
        Map::Timers => 0,
        Map::Flight => 1,
        Map::Expected => 2,
    }
}
struct MapIter {
    map: M,
    i: usize,
}
struct Iter {
    v: V,
    i: usize,
}
struct H {
    family: &'static str,
    i: usize,
    roots: V,
    maps: [M; 3],
    trace: Vec<String>,
    timers: Vec<V>,
    cleared: Vec<V>,
    callback: Option<Rc<dyn Fn() -> Result<Task<V, V>, V>>>,
    requests: Vec<V>,
    logs: Vec<V>,
    added: Vec<V>,
    patched: Vec<V>,
    order_added: Vec<V>,
    statuses: Vec<V>,
    fills: Vec<V>,
    map_args: Vec<V>,
    mapped: V,
    response: V,
    err: V,
    old_expected: Option<M>,
    off_alias: bool,
    timer_reads: usize,
    timer_set: bool,
    arg: V,
    current_entry: Option<V>,
}
impl Host for H {
    type Value = V;
    type Error = V;
    type Prepared = Binding;
    type MapIter = MapIter;
    type Iterator = Iter;
    type MapTarget = (Map, M);
    fn prepare(&mut self, b: Binding) -> Result<Binding, V> {
        Ok(b)
    }
    fn invoke(&mut self, c: Binding, args: Vec<V>) -> Result<V, V> {
        match c {
            Binding::Signature => {
                self.trace.push("signature".into());
                if self.family == "sync" && self.i == 4 {
                    self.maps[2] = empty_map();
                    return Ok(strv("sig-replaced"));
                }
                get_position_signature_for_symbol(self, args[0].clone())
            }
            Binding::Schedule => {
                self.trace.push("schedule".into());
                self.requests.push(arr(vec![
                    strv("schedule"),
                    args[0].clone(),
                    args[1].clone(),
                ]));
                Ok(if self.family == "refresh" && self.i == 1 {
                    strv("returned-schedule")
                } else {
                    val(Js::U)
                })
            }
            Binding::Optimistic => {
                self.trace.push("optimistic".into());
                Ok(val(Js::B(self.i != 2)))
            }
            Binding::OffFill => {
                self.trace.push("off".into());
                self.off_alias = self.equals(&args[0], "fill")
                    && Rc::ptr_eq(&args[1], &field(&self.roots, "handleFillForPositionSync"));
                Ok(val(Js::U))
            }
            Binding::BaseDispose => {
                self.trace.push("base-dispose".into());
                set(&self.roots, "isDisposed", val(Js::B(true)));
                Ok(val(Js::U))
            }
            Binding::ClearTimeout => {
                self.trace.push("clear-timeout".into());
                self.cleared.push(args[0].clone());
                if self.family == "dispose" && self.i == 2 {
                    return Err(self.err.clone());
                }
                if self.family == "dispose" && self.i == 3 && number(&args[0]) == 5. {
                    self.map_set(Map::Timers, strv("LTCUSDT"), num(9.))?;
                }
                Ok(val(Js::U))
            }
            Binding::ClearInterval => {
                self.trace.push("clear-interval".into());
                self.cleared.push(args[0].clone());
                Ok(val(Js::U))
            }
            Binding::AddPosition => {
                self.trace.push("add-position".into());
                self.added.push(args[0].clone());
                let positions = field(&self.roots, "positions");
                let mut p = positions.borrow_mut();
                let Js::A(a) = &mut *p else {
                    panic!("positions")
                };
                if let Some(old) = a
                    .iter_mut()
                    .flatten()
                    .find(|p| same(&field(p, "id"), &field(&args[0], "id")))
                {
                    *old = args[0].clone();
                } else {
                    a.push(Some(args[0].clone()));
                }
                Ok(val(Js::U))
            }
            Binding::UpdatePosition => {
                self.trace.push("update-position".into());
                self.patched.push(arr(args.clone()));
                let positions = field(&self.roots, "positions");
                let Js::A(a) = &*positions.borrow() else {
                    panic!("positions")
                };
                let p = a
                    .iter()
                    .flatten()
                    .find(|p| same(&field(p, "id"), &field(&args[0], "id")))
                    .unwrap();
                let Js::O(f) = &*args[1].borrow() else {
                    panic!("patch")
                };
                for (k, v) in f {
                    set(p, k, v.clone());
                }
                Ok(val(Js::U))
            }
            Binding::AddPositions => {
                self.trace.push("add-positions".into());
                self.added.push(args[0].clone());
                set(&self.roots, "positions", args[0].clone());
                Ok(val(Js::U))
            }
            Binding::MapLimit => {
                self.trace.push("map-limit".into());
                self.map_args.push(args[0].clone());
                Ok(if self.i == 5 || self.i == 7 {
                    val(Js::Null)
                } else {
                    self.mapped.clone()
                })
            }
            Binding::MapAlgo => {
                self.trace.push("map-algo".into());
                self.map_args.push(args[0].clone());
                Ok(arr(vec![
                    self.mapped.clone(),
                    obj(vec![("id", strv("second"))]),
                ]))
            }
            Binding::AddOrder => {
                self.trace.push("add-order".into());
                self.order_added.push(args[0].clone());
                Ok(val(Js::U))
            }
            Binding::OrderStatus => {
                self.trace.push("status".into());
                self.statuses.push(arr(args));
                Ok(val(Js::U))
            }
            Binding::Fill => {
                self.trace.push("fill".into());
                self.fills.push(args[0].clone());
                if self.family == "execution" && self.i == 8 {
                    return Err(self.err.clone());
                }
                Ok(val(Js::U))
            }
            Binding::Normalize => Ok(strv(&txt(&args[0]).replace("PERP_", "").replace('_', ""))),
            Binding::PositionId => Ok(if self.equals(&args[2], "oneway") || !truthy(&args[1]) {
                args[0].clone()
            } else {
                strv(&format!("{}:{}", txt(&args[0]), txt(&args[1])))
            }),
            Binding::ParseTimestamp => {
                let v = &args[0];
                if self.nullish(v) || self.equals(v, "") {
                    return Ok(val(Js::U));
                }
                let n = number(v);
                Ok(if n.is_finite() && n > 0.0 {
                    num(if n >= 1e12 {
                        (n / 1000.).floor()
                    } else {
                        n.floor()
                    })
                } else {
                    val(Js::U)
                })
            }
            _ => panic!("unexpected invocation {c:?}"),
        }
    }
    fn call(&mut self, c: Binding, args: Vec<V>) -> Result<Task<V, V>, V> {
        match c {
            Binding::Positions => {
                self.trace.push("positions".into());
                self.requests
                    .push(arr(vec![strv("positions"), args[0].clone()]));
                if self.i == 6 {
                    set(&self.roots, "isDisposed", val(Js::B(true)));
                }
                Ok(ready(if self.i == 4 {
                    Err(self.err.clone())
                } else {
                    Ok(self.response.clone())
                }))
            }
            Binding::Refresh => {
                self.trace.push("refresh".into());
                self.requests
                    .push(arr(vec![strv("refresh"), args[0].clone(), args[1].clone()]));
                Ok(ready(if self.i == 3 {
                    Err(self.err.clone())
                } else {
                    Ok(val(Js::U))
                }))
            }
            _ => panic!("unexpected call {c:?}"),
        }
    }
    fn root(&mut self, k: &str) -> Result<V, V> {
        if self.family == "dispose" && self.i == 4 && k == "_fundingRefreshTimer" && !self.timer_set
        {
            self.timer_reads += 1;
            return Ok(num(if self.timer_reads == 1 { 7. } else { 8. }));
        }
        Ok(field(&self.roots, k))
    }
    fn set_root(&mut self, k: &str, v: V) -> Result<(), V> {
        if k == "_fundingRefreshTimer" {
            self.timer_set = true;
        }
        set(&self.roots, k, v);
        Ok(())
    }
    fn field(&mut self, v: &V, k: &str) -> Result<V, V> {
        if self.nullish(v) {
            Err(error("nullish field"))
        } else {
            Ok(field(v, k))
        }
    }
    fn key_field(&mut self, v: &V, k: Vec<u16>) -> Result<V, V> {
        self.field(v, &String::from_utf16(&k).unwrap())
    }
    fn v(&mut self, v: &V, k: &str) -> Result<V, V> {
        if self.nullish(v) {
            return Ok(val(Js::U));
        }
        let has = |k: &str| matches!(&*v.borrow(),Js::O(f) if f.iter().any(|(key,_)|key==k));
        if has(k) {
            return Ok(field(v, k));
        }
        let mut snake = String::new();
        for c in k.chars() {
            if c.is_ascii_uppercase() {
                snake.push('_');
                snake.push(c.to_ascii_lowercase());
            } else {
                snake.push(c);
            }
        }
        if has(&snake) {
            return Ok(field(v, &snake));
        }
        let mut parts = k.split('_');
        let mut camel = parts.next().unwrap_or("").to_owned();
        for p in parts {
            let mut chars = p.chars();
            if let Some(c) = chars.next() {
                camel.push(c.to_ascii_uppercase());
                camel.extend(chars);
            }
        }
        Ok(field(v, &camel))
    }
    fn length(&mut self, v: &V) -> Result<usize, V> {
        match &*v.borrow() {
            Js::A(a) => Ok(a.len()),
            _ => Err(error("length")),
        }
    }
    fn index(&mut self, v: &V, i: usize) -> Result<V, V> {
        Ok(idx(v, i))
    }
    fn has_index(&mut self, v: &V, i: usize) -> Result<bool, V> {
        Ok(match &*v.borrow() {
            Js::A(a) => a.get(i).is_some_and(Option::is_some),
            _ => false,
        })
    }
    fn is_array(&self, v: &V) -> bool {
        matches!(*v.borrow(), Js::A(_))
    }
    fn truthy(&self, v: &V) -> bool {
        truthy(v)
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(*v.borrow(), Js::Null | Js::U)
    }
    fn is_undefined(&self, v: &V) -> bool {
        matches!(*v.borrow(), Js::U)
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
    fn number(&self, n: f64) -> V {
        num(n)
    }
    fn boolean(&self, b: bool) -> V {
        val(Js::B(b))
    }
    fn string(&self, s: Vec<u16>) -> V {
        val(Js::S(s))
    }
    fn undefined(&self) -> V {
        val(Js::U)
    }
    fn object(&self, f: Vec<(&'static str, V)>) -> V {
        obj(f)
    }
    fn array(&self, v: Vec<V>) -> V {
        arr(v)
    }
    fn text(&mut self, v: &V) -> Result<Vec<u16>, V> {
        Ok(match &*v.borrow() {
            Js::S(s) => s.clone(),
            _ => u(&txt(v)),
        })
    }
    fn number_coerce(&mut self, v: &V) -> Result<f64, V> {
        Ok(number(v))
    }
    fn operator_number(&mut self, v: &V) -> Result<f64, V> {
        Ok(number(v))
    }
    fn add(&mut self, a: V, b: V) -> Result<V, V> {
        Ok(
            if matches!(*a.borrow(), Js::S(_)) || matches!(*b.borrow(), Js::S(_)) {
                strv(&format!("{}{}", txt(&a), txt(&b)))
            } else {
                num(number(&a) + number(&b))
            },
        )
    }
    fn sort_by_id(&mut self, mut v: Vec<V>) -> Result<Vec<V>, V> {
        v.sort_by_key(|p| txt(&field(p, "id")));
        Ok(v)
    }
    fn prepare_map_set(&mut self, m: Map) -> Result<(Map, M), V> {
        Ok((m, self.maps[map_index(m)].clone()))
    }
    fn map_set_prepared(&mut self, (m, target): (Map, M), k: V, v: V) -> Result<(), V> {
        self.trace.push(format!("{}:set", map_name(m)));
        mset(&target, k, v);
        Ok(())
    }
    fn map_get(&mut self, m: Map, k: &V) -> Result<V, V> {
        self.trace.push(format!("{}:get", map_name(m)));
        Ok(mget(&self.maps[map_index(m)], k).unwrap_or_else(|| val(Js::U)))
    }
    fn map_has(&mut self, m: Map, k: &V) -> Result<bool, V> {
        self.trace.push(format!("{}:has", map_name(m)));
        Ok(mget(&self.maps[map_index(m)], k).is_some())
    }
    fn map_set(&mut self, m: Map, k: V, v: V) -> Result<(), V> {
        self.trace.push(format!(
            "{}:{}",
            map_name(m),
            if m == Map::Flight { "add" } else { "set" }
        ));
        mset(&self.maps[map_index(m)], k, v);
        Ok(())
    }
    fn map_delete(&mut self, m: Map, k: &V) -> Result<(), V> {
        self.trace.push(format!("{}:delete", map_name(m)));
        mdelete(&self.maps[map_index(m)], k);
        Ok(())
    }
    fn map_clear(&mut self, m: Map) -> Result<(), V> {
        self.trace.push(format!("{}:clear", map_name(m)));
        mclear(&self.maps[map_index(m)]);
        Ok(())
    }
    fn map_iterator(&mut self, m: Map) -> Result<MapIter, V> {
        self.trace.push(format!("{}:each", map_name(m)));
        Ok(MapIter {
            map: self.maps[map_index(m)].clone(),
            i: 0,
        })
    }
    fn map_next(&mut self, it: &mut MapIter) -> Result<Option<V>, V> {
        loop {
            let next = it.map.borrow().get(it.i).cloned();
            let Some((_, v)) = next else { return Ok(None) };
            it.i += 1;
            if v.is_some() {
                return Ok(v);
            }
        }
    }
    fn iterator(&mut self, v: V) -> Result<Iter, V> {
        Ok(Iter { v, i: 0 })
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
    fn close_iterator(&mut self, _it: &mut Iter) -> Result<(), V> {
        Ok(())
    }
    fn timeout(
        &mut self,
        c: Binding,
        delay: f64,
        callback: Rc<dyn Fn() -> Result<Task<V, V>, V>>,
    ) -> Result<V, V> {
        assert_eq!(c, Binding::SetTimeout, "FILL native timeout callee");
        self.trace.push("timeout".into());
        self.timers.push(num(delay));
        self.callback = Some(callback);
        Ok(num(77.))
    }
    fn error_field(&mut self, e: &V, p: &[&str]) -> Result<V, V> {
        let mut v = e.clone();
        for k in p {
            v = field(&v, k);
        }
        Ok(v)
    }
    fn emit_error(&mut self, c: Binding, m: V) -> Result<(), V> {
        assert_eq!(c, Binding::Emit, "FILL native error callee");
        self.trace.push("emit".into());
        self.logs.push(m);
        Ok(())
    }
    fn order_side(&mut self, v: V) -> Result<V, V> {
        Ok(if self.equals(&v, "BUY") {
            strv("buy")
        } else if self.equals(&v, "SELL") {
            strv("sell")
        } else {
            val(Js::U)
        })
    }
}
fn setup(family: &'static str, i: usize) -> (Rc<RefCell<H>>, V, V, f64) {
    let err = error("top failure");
    set(
        &err,
        "response",
        obj(vec![(
            "data",
            obj(vec![("message", strv("nested failure"))]),
        )]),
    );
    let roots = obj(vec![
        ("positions", arr(vec![pos(num(2.), "long", num(10.))])),
        (
            "handleFillForPositionSync",
            obj(vec![("handler", val(Js::B(true)))]),
        ),
        (
            "tickersBySymbol",
            obj(vec![("BTCUSDT", obj(vec![("mark", num(11.))]))]),
        ),
        ("isDisposed", val(Js::B(false))),
        ("_accountPositionMode", strv("oneway")),
        ("_fundingRefreshTimer", num(7.)),
        ("_fundingRefreshInFlight", val(Js::B(true))),
        ("FILL_POSITION_REFRESH_BASE_DELAY_MS", num(150.)),
        ("MAX_FILL_POSITION_REFRESH_ATTEMPTS", num(4.)),
    ]);
    let maps = [empty_map(), empty_map(), empty_map()];
    let mapped = obj(vec![
        ("id", strv("mapped")),
        ("side", strv("buy")),
        ("symbol", strv("BTCUSDT")),
    ]);
    let response = arr(vec![pos(
        num(if i == 1 || i == 5 { 2. } else { 3. }),
        "long",
        num(10.),
    )]);
    let mut attempt = 0.;
    let arg;
    let input;
    let mut old_expected = None;
    match family {
        "signature" => {
            let a = pos(num(2.), "long", num(10.));
            set(&a, "id", strv("b"));
            let b = pos(num(1.), "short", num(20.));
            set(&b, "id", strv("a"));
            let c = pos(num(9.), "long", num(10.));
            set(&c, "id", strv("x"));
            set(&c, "symbol", strv("ETHUSDT"));
            let positions = if i == 2 {
                arr(vec![])
            } else if i == 1 {
                val(Js::A(vec![None, Some(b), Some(c)]))
            } else {
                arr(vec![a, b, c])
            };
            set(&roots, "positions", positions.clone());
            arg = strv("BTCUSDT");
            input = obj(vec![("positions", positions), ("symbol", arg.clone())]);
        }
        "schedule" => {
            attempt = [0., 2., 1., 3., 0.][i];
            if i == 1 || i == 2 {
                mset(&maps[0], strv("BTCUSDT"), num(if i == 1 { 5. } else { 0. }));
            }
            if i == 4 {
                set(&roots, "isDisposed", val(Js::B(true)));
            }
            arg = strv("BTCUSDT");
            input = obj(vec![
                ("attempt", num(attempt)),
                ("disposed", field(&roots, "isDisposed")),
                (
                    "existing",
                    if i == 1 {
                        num(5.)
                    } else if i == 2 {
                        num(0.)
                    } else {
                        val(Js::U)
                    },
                ),
            ]);
        }
        "refresh" => {
            if i == 5 {
                attempt = 3.;
            }
            let expected = if i == 2 {
                strv("BTCUSDT:long:3:10")
            } else if i == 3 {
                strv("other")
            } else if i == 6 {
                strv("hold")
            } else {
                val(Js::U)
            };
            if i == 2 || i == 3 || i == 6 {
                mset(&maps[2], strv("BTCUSDT"), expected.clone());
            }
            if i == 7 {
                mset(&maps[1], strv("BTCUSDT"), val(Js::U));
            }
            arg = strv("BTCUSDT");
            input = obj(vec![
                ("positions", field(&roots, "positions")),
                ("response", response.clone()),
                ("attempt", num(attempt)),
                ("expected", expected),
                ("flight", val(Js::B(i == 7))),
            ]);
        }
        "optimistic" => {
            arg = fill();
            if matches!(i, 0 | 6 | 7 | 9) {
                set(&roots, "positions", arr(vec![]));
            }
            if matches!(i, 2 | 3 | 4 | 8) {
                set(&arg, "side", strv("sell"));
            }
            if i == 3 {
                set(&arg, "amount", num(2.));
            }
            if i == 4 {
                set(&arg, "amount", num(3.));
            }
            if i == 5 {
                set(&idx(&field(&roots, "positions"), 0), "contracts", strv("2"));
            }
            if i == 6 {
                set(&roots, "_accountPositionMode", strv("hedged"));
            }
            if i == 7 {
                set(&arg, "amount", num(0.));
            }
            if i == 8 {
                let p = idx(&field(&roots, "positions"), 0);
                set(&p, "entryPrice", strv("10"));
                set(&p, "leverage", num(0.));
                set(&p, "marginType", strv(""));
                set(&p, "positionMode", strv(""));
            }
            if i == 9 {
                set(&arg, "side", strv("sell"));
                let ticker = field(&field(&roots, "tickersBySymbol"), "BTCUSDT");
                set(&ticker, "mark", num(0.));
            }
            input = obj(vec![
                ("fill", arg.clone()),
                ("positions", field(&roots, "positions")),
                ("mode", field(&roots, "_accountPositionMode")),
                (
                    "mark",
                    field(&field(&field(&roots, "tickersBySymbol"), "BTCUSDT"), "mark"),
                ),
            ]);
        }
        "sync" => {
            arg = fill();
            if i == 1 {
                if let Js::O(f) = &mut *arg.borrow_mut() {
                    f.retain(|(k, _)| k != "symbol");
                }
            }
            if i == 3 {
                set(&roots, "positions", arr(vec![]));
            }
            if i == 4 {
                old_expected = Some(maps[2].clone());
            }
            input = obj(vec![
                ("fill", arg.clone()),
                ("positions", field(&roots, "positions")),
                ("applied", val(Js::B(i != 2))),
            ]);
        }
        "dispose" => {
            mset(&maps[0], strv("BTCUSDT"), num(5.));
            mset(&maps[0], strv("ETHUSDT"), num(0.));
            mset(&maps[1], strv("BTCUSDT"), val(Js::U));
            mset(&maps[2], strv("BTCUSDT"), strv("saved"));
            if i == 1 {
                set(&roots, "_fundingRefreshTimer", val(Js::U));
            }
            arg = val(Js::U);
            input = obj(vec![
                ("timer", field(&roots, "_fundingRefreshTimer")),
                ("live", val(Js::B(i == 3))),
                ("throw", val(Js::B(i == 2))),
                ("getter", val(Js::B(i == 4))),
            ]);
        }
        "execution" | "algo" => {
            let statuses = if family == "execution" {
                vec![
                    "NEW",
                    "PARTIAL_FILLED",
                    "PARTIALLY_FILLED",
                    "FILLED",
                    "CANCELLED",
                    "FILLED",
                    "FILLED",
                    "FILLED",
                    "FILLED",
                ]
            } else {
                vec![
                    "NEW",
                    "PARTIAL_FILLED",
                    "FILLED",
                    "CANCELLED",
                    "FILLED",
                    "FILLED",
                    "IGNORED",
                    "FILLED",
                ]
            };
            let mut data = report(statuses[i]);
            if family == "execution" && i == 5 {
                for k in ["orderId", "order_id", "id"] {
                    set(&data, k, num(0.));
                }
            }
            if family == "execution" && i == 6 {
                set(&data, "executedTimestamp", num(0.));
            }
            if family == "execution" && i == 7 {
                set(&data, "symbol", strv(""));
                set(&data, "side", strv("UNKNOWN"));
            }
            if family == "algo" && i == 4 {
                set(&data, "algoOrderId", strv("root"));
                set(&data, "orderId", strv("child"));
            }
            if family == "algo" && i == 5 {
                for k in ["orderId", "algoOrderId", "id"] {
                    set(&data, k, strv(""));
                }
            }
            if family == "algo" && i == 6 {
                data = val(Js::Null);
            }
            if family == "algo" && i == 7 {
                data = val(Js::A(vec![Some(data), None]));
            }
            arg = data;
            input = obj(vec![
                ("data", arg.clone()),
                (
                    "mapped",
                    if family == "execution" && (i == 5 || i == 7) {
                        val(Js::Null)
                    } else {
                        mapped.clone()
                    },
                ),
            ]);
        }
        _ => unreachable!(),
    }
    let current_entry = if family == "optimistic" && i == 8 {
        Some(field(&idx(&field(&roots, "positions"), 0), "entryPrice"))
    } else {
        None
    };
    (
        Rc::new(RefCell::new(H {
            family,
            i,
            roots,
            maps,
            trace: vec![],
            timers: vec![],
            cleared: vec![],
            callback: None,
            requests: vec![],
            logs: vec![],
            added: vec![],
            patched: vec![],
            order_added: vec![],
            statuses: vec![],
            fills: vec![],
            map_args: vec![],
            mapped,
            response,
            err,
            old_expected,
            off_alias: false,
            timer_reads: 0,
            timer_set: false,
            arg: arg.clone(),
            current_entry,
        })),
        input,
        arg,
        attempt,
    )
}
fn native_str(v: &V, s: &str, label: &str) {
    assert!(
        matches!(&*v.borrow(),Js::S(v) if *v==u(s)),
        "FILL native {label}: {v:?}"
    );
}
fn native_num(v: &V, n: f64, label: &str) {
    assert!(
        matches!(*v.borrow(),Js::N(v) if v.to_bits()==n.to_bits()||v.is_nan()&&n.is_nan()),
        "FILL native {label}: {v:?}"
    );
}
fn native(family: &str, i: usize, h: &H, result: &Result<V, V>, before: &[String]) {
    let failed = family == "dispose" && i == 2 || family == "execution" && i == 8;
    assert_eq!(
        result.is_err(),
        failed,
        "FILL native {family} outcome iteration {i}"
    );
    if let Err(e) = result {
        assert!(Rc::ptr_eq(e, &h.err), "FILL native original error");
    }
    if family == "signature" {
        native_str(
            result.as_ref().unwrap(),
            if i == 0 {
                "a:short:1:20|b:long:2:10"
            } else if i == 1 {
                "a:short:1:20"
            } else {
                ""
            },
            "signature order and fields",
        );
        assert!(h.trace.is_empty(), "FILL native signature effects");
        if i == 0 {
            native_str(
                &field(&idx(&field(&h.roots, "positions"), 0), "id"),
                "b",
                "source array order",
            );
        }
    }
    if family == "schedule" {
        let mut expected = vec![];
        if i != 4 {
            expected.push("timers:get");
            if i == 1 {
                expected.push("clear-timeout");
            }
            expected.extend(["timeout", "timers:set"]);
        }
        assert_eq!(before, expected, "FILL native schedule eager sequence");
        if i != 4 {
            expected.extend(["timers:delete", "refresh"]);
            if i == 3 {
                expected.push("emit");
            }
        }
        assert_eq!(h.trace, expected, "FILL native schedule callback sequence");
        assert_eq!(
            h.timers.len(),
            usize::from(i != 4),
            "FILL native timeout count"
        );
        if i != 4 {
            native_num(&h.timers[0], [0., 300., 150., 600.][i], "retry delay");
            assert_eq!(h.requests.len(), 1, "FILL native refresh callback count");
            native_str(&idx(&h.requests[0], 0), "refresh", "timer callback binding");
            native_str(&idx(&h.requests[0], 1), "BTCUSDT", "timer symbol");
            native_num(
                &idx(&h.requests[0], 2),
                [0., 2., 1., 3.][i],
                "timer attempt",
            );
        }
        assert_eq!(
            h.cleared.len(),
            usize::from(i == 1),
            "FILL native existing timer truthiness"
        );
        if i == 1 {
            native_num(&h.cleared[0], 5., "existing timer");
        }
        assert!(
            mkeys(&h.maps[0]).is_empty(),
            "FILL native timeout map deletion"
        );
        assert_eq!(
            h.logs.len(),
            usize::from(i == 3),
            "FILL native callback rejection logs"
        );
        if i == 3 {
            native_str(&h.logs[0], "nested failure", "callback rejection message");
        }
    }
    if family == "refresh" {
        assert!(
            matches!(*result.as_ref().unwrap().borrow(), Js::U),
            "FILL native refresh ignores schedule return"
        );
        let mut expected = vec!["flight:has"];
        if i != 7 {
            expected.extend(["flight:add", "signature", "expected:get", "positions"]);
        }
        assert_eq!(before, expected, "FILL native refresh eager sequence");
        if i != 7 {
            match i {
                0 | 2 => expected.extend([
                    "add-positions",
                    "signature",
                    "expected:delete",
                    "flight:delete",
                    "expected:delete",
                ]),
                1 | 3 => {
                    expected.extend(["add-positions", "signature", "flight:delete", "schedule"])
                }
                4 => expected.extend(["emit", "flight:delete", "schedule"]),
                5 => expected.extend([
                    "add-positions",
                    "signature",
                    "flight:delete",
                    "expected:delete",
                ]),
                6 => expected.push("flight:delete"),
                _ => unreachable!(),
            }
        }
        assert_eq!(
            h.trace, expected,
            "FILL native refresh retry and finally sequence"
        );
        assert_eq!(
            mkeys(&h.maps[1]),
            if i == 7 { vec!["BTCUSDT"] } else { vec![] },
            "FILL native in-flight release"
        );
        assert_eq!(
            mkeys(&h.maps[2]).len(),
            usize::from(i == 3 || i == 6),
            "FILL native expected signature retention"
        );
        assert_eq!(
            h.requests.len(),
            if i == 7 {
                0
            } else if matches!(i, 1 | 3 | 4) {
                2
            } else {
                1
            },
            "FILL native refresh request count"
        );
        if matches!(i, 1 | 3 | 4) {
            native_num(&idx(&h.requests[1], 2), 1., "retry attempt");
        }
        assert_eq!(
            h.added.len(),
            usize::from(!matches!(i, 4 | 6 | 7)),
            "FILL native refresh update count"
        );
        if !matches!(i, 4 | 6 | 7) {
            assert!(
                Rc::ptr_eq(&h.added[0], &h.response),
                "FILL native positions response alias"
            );
        }
        assert_eq!(
            h.logs.len(),
            usize::from(i == 4),
            "FILL native refresh error count"
        );
    }
    if family == "optimistic" {
        let accepted = i != 6 && i != 7;
        assert!(
            matches!(*result.as_ref().unwrap().borrow(),Js::B(v) if v==accepted),
            "FILL native optimistic boolean"
        );
        let expected = if i == 3 {
            vec!["update-position"]
        } else if i == 4 {
            vec!["update-position", "add-position"]
        } else if accepted {
            vec!["add-position"]
        } else {
            vec![]
        };
        assert_eq!(h.trace, expected, "FILL native optimistic update sequence");
        assert_eq!(
            h.patched.len(),
            usize::from(i == 3 || i == 4),
            "FILL native reset count"
        );
        if i == 3 || i == 4 {
            native_str(&field(&idx(&h.patched[0], 0), "id"), "BTCUSDT", "reset id");
            let patch = idx(&h.patched[0], 1);
            for k in [
                "entryPrice",
                "notional",
                "unrealizedPnl",
                "contracts",
                "liquidationPrice",
            ] {
                native_num(&field(&patch, k), 0., "reset field");
            }
        }
        assert_eq!(
            h.added.len(),
            usize::from(accepted && i != 3),
            "FILL native optimistic row count"
        );
        if accepted && i != 3 {
            let p = &h.added[0];
            let contracts = if i == 1 {
                3.
            } else if i == 5 {
                21.
            } else {
                1.
            };
            native_num(&field(p, "contracts"), contracts, "optimistic contracts");
            native_str(
                &field(p, "side"),
                if i == 4 || i == 9 { "short" } else { "long" },
                "optimistic side",
            );
            native_str(&field(p, "symbol"), "BTCUSDT", "optimistic symbol");
            native_str(&field(p, "id"), "BTCUSDT", "optimistic id");
            native_num(
                &field(p, "notional"),
                contracts * if i == 9 { 12. } else { 11. },
                "optimistic notional",
            );
            if i == 8 {
                native_str(&field(p, "entryPrice"), "10", "raw reduced entry price");
                assert!(
                    Rc::ptr_eq(&field(p, "entryPrice"), h.current_entry.as_ref().unwrap()),
                    "FILL native retained entry value"
                );
            } else {
                native_num(
                    &field(p, "entryPrice"),
                    if i == 1 {
                        32. / 3.
                    } else if i == 5 {
                        32. / 21.
                    } else if i == 2 {
                        10.
                    } else {
                        12.
                    },
                    "optimistic entry",
                );
            }
            native_num(
                &field(p, "leverage"),
                if matches!(i, 0 | 8 | 9) { 1. } else { 5. },
                "leverage default",
            );
            native_str(&field(p, "positionMode"), "oneway", "position mode");
            native_str(&field(p, "marginType"), "cross", "margin default");
            native_num(
                &field(p, "unrealizedPnl"),
                if matches!(i, 0 | 4 | 9) { 0. } else { 4. },
                "retained pnl",
            );
            native_num(
                &field(p, "liquidationPrice"),
                if matches!(i, 0 | 4 | 9) { 0. } else { 6. },
                "retained liquidation",
            );
        }
    }
    if family == "sync" {
        let expected = match i {
            0 | 4 => vec!["optimistic", "signature", "expected:set", "schedule"],
            1 => vec![],
            2 | 3 => vec!["optimistic", "expected:delete", "schedule"],
            _ => unreachable!(),
        };
        assert_eq!(h.trace, expected, "FILL native fill sync sequence");
        assert_eq!(
            h.requests.len(),
            usize::from(i != 1),
            "FILL native fill sync scheduling"
        );
        if i != 1 {
            native_num(&idx(&h.requests[0], 2), 0., "initial recovery attempt");
        }
        assert_eq!(
            mkeys(&h.maps[2]).len(),
            usize::from(i == 0),
            "FILL native current expected map"
        );
        if i == 0 {
            native_str(
                &mget(&h.maps[2], &strv("BTCUSDT")).unwrap(),
                "BTCUSDT:long:2:10",
                "expected position signature",
            );
        }
        if i == 4 {
            let old = h.old_expected.as_ref().unwrap();
            assert_eq!(mkeys(old).len(), 1, "FILL native captured map count");
            native_str(
                &mget(old, &strv("BTCUSDT")).unwrap(),
                "sig-replaced",
                "captured map setter",
            );
        }
    }
    if family == "dispose" {
        let mut expected = vec!["off", "timers:each", "clear-timeout"];
        if i != 2 {
            if i == 3 {
                expected.push("timers:set");
            }
            expected.push("clear-timeout");
            if i == 3 {
                expected.push("clear-timeout");
            }
            expected.extend(["timers:clear", "flight:clear", "expected:clear"]);
            if i != 1 {
                expected.push("clear-interval");
            }
            expected.push("base-dispose");
        }
        assert_eq!(h.trace, expected, "FILL native disposal sequence");
        assert!(h.off_alias, "FILL native fill listener removal");
        if i == 2 {
            assert_eq!(
                mkeys(&h.maps[0]).len(),
                2,
                "FILL native thrown disposal preserves timers"
            );
            assert!(
                truthy(&field(&h.roots, "_fundingRefreshInFlight")),
                "FILL native failed disposal flag"
            );
        } else {
            for m in &h.maps {
                assert!(mkeys(m).is_empty(), "FILL native disposed maps cleared");
            }
            assert!(
                matches!(*field(&h.roots, "_fundingRefreshTimer").borrow(), Js::U),
                "FILL native funding timer cleared"
            );
            assert!(
                !truthy(&field(&h.roots, "_fundingRefreshInFlight")),
                "FILL native funding flight cleared"
            );
            assert!(
                truthy(&field(&h.roots, "isDisposed")),
                "FILL native base disposed"
            );
        }
        assert_eq!(
            h.cleared.len(),
            if i == 1 {
                2
            } else if i == 2 {
                1
            } else if i == 3 {
                4
            } else {
                3
            },
            "FILL native clear count"
        );
        if i == 4 {
            assert_eq!(h.timer_reads, 2, "FILL native funding timer reread");
            native_num(&h.cleared[2], 8., "second timer value");
        }
        if i == 3 {
            native_num(&h.cleared[2], 9., "live map appended timer");
        }
    }
    if family == "execution" || family == "algo" {
        let expected = if family == "execution" {
            match i {
                0 => vec!["map-limit", "add-order"],
                1 | 2 => vec!["map-limit", "add-order", "fill"],
                3 | 6 | 8 => vec!["map-limit", "status", "fill"],
                4 => vec!["map-limit", "status"],
                5 => vec!["map-limit", "fill"],
                7 => vec!["map-limit", "status"],
                _ => unreachable!(),
            }
        } else {
            match i {
                0 | 1 => vec!["map-algo", "add-order"],
                2 | 4 | 7 => vec!["fill", "status"],
                3 => vec!["status"],
                5 => vec!["fill"],
                6 => vec![],
                _ => unreachable!(),
            }
        };
        assert_eq!(h.trace, expected, "FILL native report sequence");
        let map_count = if family == "execution" {
            1
        } else {
            usize::from(i == 0 || i == 1)
        };
        assert_eq!(h.map_args.len(), map_count, "FILL native mapper count");
        for v in &h.map_args {
            let arg = if family == "algo" && i == 7 {
                idx(&h.arg, 0)
            } else {
                h.arg.clone()
            };
            assert!(Rc::ptr_eq(v, &arg), "FILL native mapper input alias");
        }
        let added = if family == "execution" {
            i <= 2
        } else {
            i <= 1
        };
        assert_eq!(
            h.order_added.len(),
            usize::from(added),
            "FILL native order add count"
        );
        if added {
            assert!(
                Rc::ptr_eq(&h.order_added[0], &h.mapped),
                "FILL native mapped order alias"
            );
        }
        let filled = if family == "execution" {
            matches!(i, 1 | 2 | 3 | 5 | 6 | 8)
        } else {
            matches!(i, 2 | 4 | 5 | 7)
        };
        assert_eq!(
            h.fills.len(),
            usize::from(filled),
            "FILL native fill event count"
        );
        if filled {
            let f = &h.fills[0];
            if family == "execution" {
                native_str(
                    &field(f, "side"),
                    if i == 5 { "sell" } else { "buy" },
                    "fill side",
                );
                native_str(
                    &field(f, "symbol"),
                    if i == 5 { "ETHUSDT" } else { "BTCUSDT" },
                    "fill symbol",
                );
                native_str(&field(f, "price"), "11", "raw fill price");
                native_num(&field(f, "amount"), 3., "fill amount");
                if i == 5 {
                    native_num(&field(f, "id"), 0., "raw zero fill id");
                    assert!(
                        matches!(*field(f, "orderId").borrow(), Js::U),
                        "FILL native empty order id"
                    );
                } else {
                    native_str(&field(f, "id"), "oid", "fill id");
                    native_str(&field(f, "orderId"), "oid", "fill order id");
                }
                if i == 6 {
                    assert!(
                        matches!(&*f.borrow(),Js::O(o) if !o.iter().any(|(k,_)|k=="timestamp")),
                        "FILL native omitted timestamp"
                    );
                } else {
                    native_num(&field(f, "timestamp"), 1700000000., "fill seconds");
                }
            } else {
                native_str(&field(f, "side"), "sell", "algo side");
                native_str(&field(f, "symbol"), "ETHUSDT", "algo symbol");
                native_str(&field(f, "price"), "14", "raw algo price");
                native_num(&field(f, "amount"), 4., "algo amount");
                native_str(
                    &field(f, "id"),
                    if i == 4 {
                        "child"
                    } else if i == 5 {
                        ""
                    } else {
                        "oid"
                    },
                    "algo fill id",
                );
                if i == 5 {
                    assert!(
                        matches!(*field(f, "orderId").borrow(), Js::U),
                        "FILL native empty algo order id"
                    );
                }
                native_num(&field(f, "timestamp"), 1700000000., "algo seconds");
            }
        }
        let status_count = if family == "execution" {
            usize::from(matches!(i, 3 | 4 | 6 | 7 | 8))
        } else {
            usize::from(matches!(i, 2 | 3 | 4 | 7))
        };
        assert_eq!(
            h.statuses.len(),
            status_count,
            "FILL native order status count"
        );
        if status_count == 1 {
            native_str(
                &field(&idx(&h.statuses[0], 0), "id"),
                if family == "execution" {
                    "oid"
                } else if i == 4 {
                    "root"
                } else {
                    "aid"
                },
                "status id precedence",
            );
            native_str(
                &idx(&h.statuses[0], 1),
                if i == 4 && family == "execution" || i == 3 && family == "algo" {
                    "canceled"
                } else {
                    "closed"
                },
                "status value",
            );
        }
    }
}
fn corpus() -> &'static Vec<Value> {
    static CORPUS: std::sync::OnceLock<Vec<Value>> = std::sync::OnceLock::new();
    CORPUS.get_or_init(|| {
        let out = std::process::Command::new("node")
            .arg(format!(
                "{}/tests/legacy_fill_recovery_oracle.cjs",
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
        assert_eq!(rows.len(), 53, "FILL corpus SIZE");
        let mut n = 0;
        for (family, count) in [
            ("signature", 3),
            ("schedule", 5),
            ("refresh", 8),
            ("optimistic", 10),
            ("sync", 5),
            ("dispose", 5),
            ("execution", 9),
            ("algo", 8),
        ] {
            for i in 0..count {
                assert_eq!(
                    rows[n]["family"], family,
                    "FILL corpus CONTENT family iteration {n}"
                );
                assert_eq!(rows[n]["i"], i, "FILL corpus CONTENT index iteration {n}");
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
        let (h, input, arg, attempt) = setup(family, i);
        let input = enc(&input);
        let result;
        let before;
        if family == "refresh" {
            let task = refresh_position_for_symbol_from_fill(h.clone(), arg, attempt);
            before = h.borrow().trace.clone();
            result = run(task);
        } else {
            result = if family == "schedule" {
                schedule_fill_position_refresh(h.clone(), arg, attempt).map(|()| val(Js::U))
            } else {
                let mut h = h.borrow_mut();
                match family {
                    "signature" => get_position_signature_for_symbol(&mut *h, arg),
                    "optimistic" => apply_optimistic_one_way_position_from_fill(&mut *h, arg)
                        .map(|b| val(Js::B(b))),
                    "sync" => handle_fill_for_position_sync(&mut *h, arg).map(|()| val(Js::U)),
                    "dispose" => dispose(&mut *h).map(|()| val(Js::U)),
                    "execution" => handle_execution_report(&mut *h, arg).map(|()| val(Js::U)),
                    "algo" => handle_algo_execution_report(&mut *h, arg).map(|()| val(Js::U)),
                    _ => unreachable!(),
                }
            };
            before = h.borrow().trace.clone();
        }
        if family == "schedule" {
            let cb = h.borrow().callback.clone();
            if let Some(cb) = cb {
                run(cb().unwrap()).unwrap();
            }
        }
        let h = h.borrow();
        native(family, i, &h, &result, &before);
        let result = match result {
            Ok(v) => json!({"ok":enc(&v)}),
            Err(e) => json!({"error":enc(&field(&e,"message")),"errorAlias":Rc::ptr_eq(&e,&h.err)}),
        };
        let order_alias = h
            .order_added
            .iter()
            .map(|v| Rc::ptr_eq(v, &h.mapped))
            .collect::<Vec<_>>();
        let target = if family == "algo" && i == 7 {
            idx(&h.arg, 0)
        } else {
            h.arg.clone()
        };
        let map_alias = h
            .map_args
            .iter()
            .map(|v| Rc::ptr_eq(v, &target))
            .collect::<Vec<_>>();
        actual.push(json!({"family":family,"i":i,"input":input,"result":result,"before":before,"trace":h.trace,"timers":enc(&arr(h.timers.clone())),"cleared":enc(&arr(h.cleared.clone())),"requests":enc(&arr(h.requests.clone())),"logs":enc(&arr(h.logs.clone())),"added":enc(&arr(h.added.clone())),"patched":enc(&arr(h.patched.clone())),"orderAdded":enc(&arr(h.order_added.clone())),"statuses":enc(&arr(h.statuses.clone())),"fills":enc(&arr(h.fills.clone())),"orderAlias":order_alias,"mapAlias":map_alias,"responseAlias":family=="refresh"&&h.added.first().is_some_and(|v|Rc::ptr_eq(v,&h.response)),"positions":enc(&field(&h.roots,"positions")),"expected":enc(&menc(&h.maps[2])),"flight":mkeys(&h.maps[1]),"timerMap":enc(&menc(&h.maps[0])),"fundingTimer":enc(&field(&h.roots,"_fundingRefreshTimer")),"fundingFlight":truthy(&field(&h.roots,"_fundingRefreshInFlight")),"disposed":truthy(&field(&h.roots,"isDisposed")),"oldExpected":enc(&h.old_expected.as_ref().map(menc).unwrap_or_else(||val(Js::U))),"offAlias":h.off_alias,"timerReads":h.timer_reads}));
    }
    for (i, a) in actual.iter().enumerate() {
        let expected = source
            .iter()
            .find(|r| r["family"] == family && r["i"] == i)
            .unwrap();
        assert_eq!(a, expected, "FILL source wire {family} iteration {i}");
    }
}
#[test]
fn signature_native() {
    family("signature", 3)
}
#[test]
fn schedule_native() {
    family("schedule", 5)
}
#[test]
fn refresh_native() {
    family("refresh", 8)
}
#[test]
fn optimistic_native() {
    family("optimistic", 10)
}
#[test]
fn sync_native() {
    family("sync", 5)
}
#[test]
fn dispose_native() {
    family("dispose", 5)
}
#[test]
fn execution_native() {
    family("execution", 9)
}
#[test]
fn algo_native() {
    family("algo", 8)
}
