//! WOOXExchange members ported in `crate::legacy_order_writes`, over js_runtime values.
use super::order_control::{
    Callee, emit_with, iterate, length, member, module, optional_path, settle, xhr,
};
use crate::binding::{
    rt::{Rt, Shared},
    value::{Js, Thrown, arg, error_object, from_typed, to_json},
};
use crate::legacy_metadata::Task;
use crate::legacy_order_writes::{self as body, Binding, Host};
use safe_cex_exchange::js_runtime::ops;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

// ts: isPositiveFinitePrice
fn positive_price(v: &Js) -> Option<f64> {
    match v {
        Js::Number(n) if n.is_finite() && *n > 0.0 => Some(*n),
        _ => None,
    }
}

fn is_buy(side: &Js) -> bool {
    side.strict_equals(&Js::str("buy"))
}

/// `[ticker?.mark, ticker?.index, ticker?.midPrice, ticker?.last].find(isPositiveFinitePrice)`
fn fair_price(ticker: &Js) -> Result<Option<f64>, Thrown> {
    let mut candidates = vec![];
    for key in ["mark", "index", "midPrice", "last"] {
        candidates.push(optional_path(ticker, &[key])?);
    }
    Ok(candidates.iter().find_map(positive_price))
}

// ts: isPostOnlyCrossingPrice
fn crossing(side: &Js, price: &Js, touch: &Js) -> Result<bool, Thrown> {
    if is_buy(side) {
        ops::greater_equal(price, touch)
    } else {
        ops::less_equal(price, touch)
    }
}

// ts: isPostOnlyCrossingFairPrice
fn fair_cross(_: &Rt, a: Vec<Js>) -> Result<Js, Thrown> {
    let (side, price, ticker) = (arg(&a, 0), arg(&a, 1), arg(&a, 2));
    let Some(fair) = fair_price(&ticker)? else {
        return Ok(Js::Undefined);
    };
    let would = crossing(&side, &price, &Js::Number(fair))?;
    Ok(Js::object(vec![
        ("fairPrice", Js::Number(fair)),
        ("wouldCross", Js::Bool(would)),
    ]))
}

fn post_only_cross(_: &Rt, a: Vec<Js>) -> Result<Js, Thrown> {
    Ok(Js::Bool(crossing(&arg(&a, 0), &arg(&a, 1), &arg(&a, 2))?))
}

// ts: getWooMarketPriceRangePct
fn price_range(_: &Rt, a: Vec<Js>) -> Result<Js, Thrown> {
    let range = optional_path(&arg(&a, 0), &["info", "price_range"])?.to_number()?;
    Ok(Js::Number(if range.is_finite() && range > 0.0 {
        range * 100.0
    } else {
        1.0
    }))
}

// ts: applyMarketIocSlippage
fn slipped(price: f64, buy: bool, slippage: f64) -> f64 {
    let factor = safe_cex_num::math_max(0.0, slippage) / 100.0;
    price * if buy { 1.0 + factor } else { 1.0 - factor }
}

// ts: chooseMarketIocOrderPrice
fn choose_price(_: &Rt, a: Vec<Js>) -> Result<Js, Thrown> {
    let (ticker, side, touch) = (arg(&a, 0), arg(&a, 1), arg(&a, 4));
    let slippage = arg(&a, 2).to_number()?;
    let range = arg(&a, 3).to_number()?;
    let buy = is_buy(&side);
    let quote = if touch.is_nullish() {
        optional_path(&ticker, &[if buy { "ask" } else { "bid" }])?
    } else {
        touch
    };
    let fair = fair_price(&ticker)?;
    if let Some(fair) = fair {
        let fair_crossing = slipped(fair, buy, slippage);
        if let Some(q) = positive_price(&quote) {
            let factor = safe_cex_num::math_max(0.0, range) / 100.0;
            let within = if buy {
                q <= fair * (1.0 + factor)
            } else {
                q >= fair * (1.0 - factor)
            };
            let misses = if buy {
                fair_crossing < q
            } else {
                fair_crossing > q
            };
            if within && misses {
                return Ok(quote);
            }
        }
        return Ok(Js::Number(fair_crossing));
    }
    let fallback = match positive_price(&quote) {
        Some(q) => Some(q),
        None => positive_price(&optional_path(&ticker, &[if buy { "bid" } else { "ask" }])?),
    };
    Ok(fallback.map_or(Js::Undefined, |p| Js::Number(slipped(p, buy, slippage))))
}

/// `mapWooHistoricalOrder(row, resolveSymbol)` with the method-local resolver
/// `this.store.marketsBySymbol[normalizeSymbol(marketId)]?.symbol`.
fn map_historical(rt: &Rt, a: Vec<Js>) -> Result<Js, Thrown> {
    let row = arg(&a, 0);
    if row.is_nullish() {
        return Err(Thrown::type_error(&format!(
            "Cannot read properties of {} (reading 'order_id')",
            row.to_display()
        )));
    }
    let row = match to_json(&row)? {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    let normalize = rt.global("normalizeSymbol")?;
    let resolve = |market_id: &str| -> Result<Option<String>, String> {
        let read = || -> Result<Option<String>, Thrown> {
            let markets = rt.path("store.marketsBySymbol")?;
            let key = normalize.call(Js::Undefined, vec![Js::str(market_id)])?;
            let symbol = optional_path(&markets.get_key(&key.to_js_string()?)?, &["symbol"])?;
            Ok(symbol.truthy().then(|| symbol.to_display()))
        };
        read().map_err(|e| e.message())
    };
    match crate::order_history::map_woo_historical_order(&row, resolve)
        .map_err(|e| Thrown::type_error(&e))?
    {
        Some(order) => from_typed(&order),
        None => Ok(Js::Null),
    }
}

/// Stable merge sort with a fallible JS comparator (NaN compares as +0).
fn merge_sort(
    items: Vec<Js>,
    cmp: &dyn Fn(&Js, &Js) -> Result<f64, Thrown>,
) -> Result<Vec<Js>, Thrown> {
    if items.len() <= 1 {
        return Ok(items);
    }
    let mut left = items;
    let right = left.split_off(left.len() / 2);
    let (left, right) = (merge_sort(left, cmp)?, merge_sort(right, cmp)?);
    let mut out = Vec::with_capacity(left.len() + right.len());
    let (mut l, mut r) = (left.into_iter().peekable(), right.into_iter().peekable());
    while let (Some(a), Some(b)) = (l.peek(), r.peek()) {
        if cmp(a, b)? > 0.0 {
            out.extend(r.next());
        } else {
            out.extend(l.next());
        }
    }
    out.extend(l);
    out.extend(r);
    Ok(out)
}

pub(crate) struct Frame {
    pub(crate) rt: Rt,
}

impl Frame {
    fn this(&self, name: &str, label: &'static str) -> Result<Callee, Thrown> {
        member(self.rt.this.clone(), name, label)
    }
    fn inverse(&self, table: &str) -> Result<Callee, Thrown> {
        let table = self.rt.global(table)?;
        Ok(Callee::Table(
            self.rt
                .global("inverseObj")?
                .call(Js::Undefined, vec![table])?,
        ))
    }
}

impl Host for Frame {
    type Value = Js;
    type Error = Thrown;
    type Prepared = Callee;
    type Iter = (Js, usize);
    fn prepare(&mut self, b: Binding) -> Result<Callee, Thrown> {
        let rt = &self.rt;
        match b {
            Binding::Put => xhr(rt, "put", "this.xhr.put"),
            Binding::Post => xhr(rt, "post", "this.xhr.post"),
            Binding::Get => xhr(rt, "get", "this.xhr.get"),
            Binding::Emit => self.this("emitAndLogError", "this.emitAndLogError"),
            Binding::Log => self.this("log", "this.log"),
            Binding::ResolveClientId => self.this(
                "resolveCallerClientOrderId",
                "this.resolveCallerClientOrderId",
            ),
            Binding::IsAlgo => self.this("isAlgoOrder", "this.isAlgoOrder"),
            Binding::CancelOrders => self.this("cancelOrders", "this.cancelOrders"),
            Binding::CancelAlgo => self.this("cancelAlgoOrder", "this.cancelAlgoOrder"),
            Binding::Trailing => self.this(
                "placeTrailingStopLossOrder",
                "this.placeTrailingStopLossOrder",
            ),
            Binding::UpdateAlgo => self.this("updateAlgoOrder", "this.updateAlgoOrder"),
            Binding::PlaceAlgo => self.this("placeAlgoOrder", "this.placeAlgoOrder"),
            Binding::Stop => self.this("placeStopMarket", "this.placeStopMarket"),
            Binding::Positional => {
                self.this("placePositionalAlgoOrder", "this.placePositionalAlgoOrder")
            }
            Binding::PositionSide => {
                self.this("getPositionSideForOrder", "this.getPositionSideForOrder")
            }
            Binding::PositionMode => self.this("getPositionMode", "this.getPositionMode"),
            Binding::Touch => {
                self.this("fetchMarketIocTouchPrice", "this.fetchMarketIocTouchPrice")
            }
            Binding::AssertClientId => self.this(
                "assertSingleNativeClientOrderId",
                "this.assertSingleNativeClientOrderId",
            ),
            Binding::Adjust => module(rt, "adjust", "adjust"),
            Binding::Reverse => module(rt, "reverseSymbol", "reverseSymbol"),
            Binding::Normalize => module(rt, "normalizeSymbol", "normalizeSymbol"),
            Binding::PositionId => module(rt, "createPositionId", "createPositionId"),
            Binding::InverseType => self.inverse("ORDER_TYPE"),
            Binding::InverseSide => self.inverse("ORDER_SIDE"),
            Binding::InverseTrigger => self.inverse("ORDER_TRIGGER_TYPE"),
            Binding::FairCross => Ok(Callee::Native(fair_cross)),
            Binding::Cross => Ok(Callee::Native(post_only_cross)),
            Binding::PriceRange => Ok(Callee::Native(price_range)),
            Binding::ChoosePrice => Ok(Callee::Native(choose_price)),
            Binding::MapHistorical => Ok(Callee::Native(map_historical)),
        }
    }
    fn invoke(&mut self, c: Callee, a: Vec<Js>) -> Result<Js, Thrown> {
        c.call(&self.rt, a)
    }
    fn call(&mut self, c: Callee, a: Vec<Js>) -> Result<Task<Js, Thrown>, Thrown> {
        Ok(settle(&self.rt, c.call(&self.rt, a)?))
    }
    /// `this.isPublicDataOnly` / `this.isDisposed`; every other root is `this.store.<k>`.
    fn root(&mut self, k: &str) -> Result<Js, Thrown> {
        match k {
            "isPublicDataOnly" | "isDisposed" => self.rt.this.get(k),
            _ => self.rt.this.get("store")?.get(k),
        }
    }
    fn field(&mut self, v: &Js, k: &str) -> Result<Js, Thrown> {
        v.get(k)
    }
    fn keyed(&mut self, v: &Js, k: &Js) -> Result<Js, Thrown> {
        v.get_key(&k.to_js_string()?)
    }
    /// `k in v`
    fn has(&mut self, v: &Js, k: &str) -> Result<bool, Thrown> {
        fn has_in(v: &Js, k: &str) -> bool {
            match v {
                Js::Object(o) => {
                    v.has_own(k) || o.proto.borrow().as_ref().is_some_and(|p| has_in(p, k))
                }
                Js::Array(_) => k == "length" || v.has_own(k),
                _ => v.has_own(k),
            }
        }
        if !v.is_object() {
            return Err(Thrown::type_error(&format!(
                "Cannot use 'in' operator to search for '{k}' in {}",
                v.to_display()
            )));
        }
        Ok(has_in(v, k))
    }
    fn set(&mut self, v: &Js, k: &str, x: Js) -> Result<(), Thrown> {
        v.set(k, x)
    }
    fn truthy(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn nullish(&self, v: &Js) -> bool {
        v.is_nullish()
    }
    fn is_undefined(&self, v: &Js) -> bool {
        v.is_undefined()
    }
    fn is_number(&self, v: &Js) -> bool {
        matches!(v, Js::Number(_))
    }
    fn is_array(&self, v: &Js) -> bool {
        v.is_array()
    }
    fn equals(&self, v: &Js, s: &str) -> bool {
        v.strict_equals(&Js::str(s))
    }
    fn same(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn less(&mut self, a: &Js, b: &Js) -> Result<bool, Thrown> {
        ops::less_than(a, b)
    }
    fn number_coerce(&mut self, v: &Js) -> Result<f64, Thrown> {
        v.to_number()
    }
    fn text(&mut self, v: &Js) -> Result<Vec<u16>, Thrown> {
        Ok(v.to_js_string()?.to_vec())
    }
    fn number(&self, n: f64) -> Js {
        Js::Number(n)
    }
    fn string(&self, s: Vec<u16>) -> Js {
        Js::utf16(&s)
    }
    fn boolean(&self, b: bool) -> Js {
        Js::Bool(b)
    }
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn null(&self) -> Js {
        Js::Null
    }
    fn object(&self, fields: Vec<(&'static str, Js)>) -> Js {
        Js::object(fields)
    }
    fn array(&self, slots: Vec<Option<Js>>) -> Js {
        Js::holey_array(slots)
    }
    fn spread_object(&mut self, v: &Js, fields: Vec<(&'static str, Js)>) -> Result<Js, Thrown> {
        let out = v.spread_object()?;
        for (k, x) in fields {
            out.define(k, x)?;
        }
        Ok(out)
    }
    fn omit_undefined(&mut self, v: Js) -> Result<Js, Thrown> {
        self.rt
            .global("omitUndefined")?
            .call(Js::Undefined, vec![v])
    }
    fn length(&mut self, v: &Js) -> Result<usize, Thrown> {
        length(v)
    }
    fn index(&mut self, v: &Js, i: usize) -> Result<Js, Thrown> {
        v.get(&i.to_string())
    }
    fn has_index(&mut self, v: &Js, i: usize) -> Result<bool, Thrown> {
        Ok(v.has_own(&i.to_string()))
    }
    fn push(&mut self, v: &Js, x: Js) -> Result<(), Thrown> {
        v.push(x)
    }
    /// lodash `times(n, () => ({ ...req, [key]: qty }))`
    fn times(&mut self, n: f64, req: Js, key: &'static str, qty: Js) -> Result<Js, Thrown> {
        let n = if n.is_nan() {
            0.0
        } else if n.is_infinite() {
            f64::MAX.copysign(n)
        } else {
            n.trunc()
        };
        let mut out = vec![];
        if (1.0..=MAX_SAFE_INTEGER).contains(&n) {
            for _ in 0..n as u64 {
                out.push(self.spread_object(&req, vec![(key, qty.clone())])?);
            }
        }
        Ok(Js::array(out))
    }
    /// `Array.prototype.flat()`: one level, holes dropped.
    fn flatten_one(&mut self, v: Js) -> Result<Js, Thrown> {
        let mut out = vec![];
        for item in v.array_items()?.unwrap_or_default().into_iter().flatten() {
            match item.array_items()? {
                Some(inner) => out.extend(inner.into_iter().flatten()),
                None => out.push(item),
            }
        }
        Ok(Js::array(out))
    }
    fn spread_array(&mut self, v: &Js) -> Result<Vec<Js>, Thrown> {
        iterate(v)
    }
    fn iterator(&mut self, v: Js) -> Result<(Js, usize), Thrown> {
        if !v.is_array() {
            return Err(Thrown::type_error(&format!(
                "{} is not iterable",
                v.to_display()
            )));
        }
        Ok((v, 0))
    }
    fn next(&mut self, it: &mut (Js, usize)) -> Result<Option<Js>, Thrown> {
        if it.1 >= length(&it.0)? {
            return Ok(None);
        }
        let value = it.0.get(&it.1.to_string())?;
        it.1 += 1;
        Ok(Some(value))
    }
    fn close(&mut self, _: &mut (Js, usize)) -> Result<(), Thrown> {
        Ok(())
    }
    fn await_element(&mut self, v: Js) -> Task<Js, Thrown> {
        settle(&self.rt, v)
    }
    /// `orders.sort((a, b) => b.createdTime - a.createdTime)`, in place.
    fn sort_history(&mut self, rows: Js) -> Result<Js, Thrown> {
        let Js::Array(cells) = &rows else {
            return Err(Thrown::type_error("orders.sort is not a function"));
        };
        let items = rows.array_items()?.expect("array variant");
        let holes = items.iter().filter(|v| v.is_none()).count();
        let (undefined, values): (Vec<Js>, Vec<Js>) =
            items.into_iter().flatten().partition(Js::is_undefined);
        let cmp = |a: &Js, b: &Js| -> Result<f64, Thrown> {
            let d = ops::subtract(&b.get("createdTime")?, &a.get("createdTime")?)?.to_number()?;
            Ok(if d.is_nan() { 0.0 } else { d })
        };
        let sorted = merge_sort(values, &cmp)?;
        let mut out: Vec<Option<Js>> = sorted.into_iter().chain(undefined).map(Some).collect();
        out.extend(std::iter::repeat_n(None, holes));
        let _=cells;
        for (index,value) in out.into_iter().enumerate(){let key=index.to_string();if let Some(value)=value{rows.set(&key,value)?;}else{rows.delete(&key.encode_utf16().collect::<Vec<_>>())?;}}
        Ok(rows)
    }
    /// `sorted.slice(0, limit)`
    fn slice(&mut self, v: Js, limit: Js) -> Result<Js, Thrown> {
        let items = v.array_items()?.unwrap_or_default();
        let len = items.len() as f64;
        let end = limit.to_number()?;
        let end = if end.is_nan() { 0.0 } else { end.trunc() };
        let end = if end < 0.0 {
            (len + end).max(0.0)
        } else {
            end.min(len)
        };
        Ok(Js::holey_array(items[..end as usize].to_vec()))
    }
    fn v(&mut self, v: &Js, k: &str) -> Result<Js, Thrown> {
        self.rt
            .global("v")?
            .call(Js::Undefined, vec![v.clone(), Js::str(k)])
    }
    fn error(&mut self, message: Js, type_error: bool) -> Thrown {
        let name = if type_error { "TypeError" } else { "Error" };
        Thrown::new(error_object(name, &message.to_display()))
    }
    fn error_field(&mut self, e: &Thrown, path: &[&str]) -> Result<Js, Thrown> {
        optional_path(&e.value, path)
    }
    fn emit(&mut self, c: Callee, original: Option<&Thrown>, message: Js) -> Result<(), Thrown> {
        emit_with(&self.rt, &c, original, message)
    }
}

type Frames = Rc<RefCell<Frame>>;

fn async_member(rt: &Rt, name: &str, run: impl Fn(Frames, Js) -> Task<Js, Thrown> + 'static) -> Js {
    let rt = rt.clone();
    Js::function(name, move |_, args| {
        let frame = Rc::new(RefCell::new(Frame { rt: rt.clone() }));
        Ok(rt.spawn(run(frame, arg(&args, 0))))
    })
}

pub(crate) fn bind(rt: &Rt, name: &str) -> Option<Result<Js, Thrown>> {
    let member = match name {
        "updateOrder" => async_member(rt, name, body::update_order),
        "updateAlgoOrder" => async_member(rt, name, body::update_algo_order),
        "placeAlgoOrder" => async_member(rt, name, body::place_algo_order),
        "placeOrder" => async_member(rt, name, body::place_order),
        "placeStopMarket" => async_member(rt, name, body::place_stop_market),
        "placeTrailingStopLossOrder" => {
            async_member(rt, name, body::place_trailing_stop_loss_order)
        }
        "placePositionalAlgoOrder" => async_member(rt, name, body::place_positional_algo_order),
        "fetchHistoricalOrders" => async_member(rt, name, body::fetch_historical_orders),
        "fetchOpenPositions" => async_member(rt, name, |h, _| body::fetch_open_positions(h)),
        _ => return None,
    };
    Some(Ok(member))
}

pub(crate) fn register(_g: &Js, _weak: Weak<Shared>) {}
