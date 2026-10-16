//! Legacy WOO order writes, historical orders and open-position readback.
use crate::legacy_metadata::Task;
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binding {
    Put,
    Post,
    Get,
    Emit,
    Log,
    ResolveClientId,
    IsAlgo,
    CancelOrders,
    CancelAlgo,
    Trailing,
    UpdateAlgo,
    PlaceAlgo,
    Stop,
    Positional,
    PositionSide,
    PositionMode,
    Touch,
    Adjust,
    Reverse,
    Normalize,
    InverseType,
    InverseSide,
    InverseTrigger,
    FairCross,
    Cross,
    PriceRange,
    ChoosePrice,
    AssertClientId,
    MapHistorical,
    PositionId,
}
/// Compatible primitives retain values, references, captured calls and original errors.
/// Imported helper/library operations keep their source contracts.
pub trait Host: 'static {
    type Value: Clone + 'static;
    type Error: 'static;
    type Prepared;
    type Iter;
    fn prepare(&mut self, b: Binding) -> Result<Self::Prepared, Self::Error>;
    fn invoke(
        &mut self,
        c: Self::Prepared,
        a: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn call(
        &mut self,
        c: Self::Prepared,
        a: Vec<Self::Value>,
    ) -> Result<Task<Self::Value, Self::Error>, Self::Error>;
    fn root(&mut self, k: &str) -> Result<Self::Value, Self::Error>;
    fn field(&mut self, v: &Self::Value, k: &str) -> Result<Self::Value, Self::Error>;
    fn keyed(&mut self, v: &Self::Value, k: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn has(&mut self, v: &Self::Value, k: &str) -> Result<bool, Self::Error>;
    fn set(&mut self, v: &Self::Value, k: &str, x: Self::Value) -> Result<(), Self::Error>;
    fn truthy(&self, v: &Self::Value) -> bool;
    fn nullish(&self, v: &Self::Value) -> bool;
    fn is_undefined(&self, v: &Self::Value) -> bool;
    fn is_number(&self, v: &Self::Value) -> bool;
    fn is_array(&self, v: &Self::Value) -> bool;
    fn equals(&self, v: &Self::Value, s: &str) -> bool;
    fn same(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn less(&mut self, a: &Self::Value, b: &Self::Value) -> Result<bool, Self::Error>;
    fn number_coerce(&mut self, v: &Self::Value) -> Result<f64, Self::Error>;
    fn text(&mut self, v: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn number(&self, n: f64) -> Self::Value;
    fn string(&self, s: Vec<u16>) -> Self::Value;
    fn boolean(&self, b: bool) -> Self::Value;
    fn undefined(&self) -> Self::Value;
    fn null(&self) -> Self::Value;
    fn object(&self, fields: Vec<(&'static str, Self::Value)>) -> Self::Value;
    fn array(&self, slots: Vec<Option<Self::Value>>) -> Self::Value;
    fn spread_object(
        &mut self,
        v: &Self::Value,
        fields: Vec<(&'static str, Self::Value)>,
    ) -> Result<Self::Value, Self::Error>;
    fn omit_undefined(&mut self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn length(&mut self, v: &Self::Value) -> Result<usize, Self::Error>;
    fn index(&mut self, v: &Self::Value, i: usize) -> Result<Self::Value, Self::Error>;
    fn has_index(&mut self, v: &Self::Value, i: usize) -> Result<bool, Self::Error>;
    fn push(&mut self, v: &Self::Value, x: Self::Value) -> Result<(), Self::Error>;
    fn times(
        &mut self,
        n: f64,
        req: Self::Value,
        key: &'static str,
        qty: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn flatten_one(&mut self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn spread_array(&mut self, v: &Self::Value) -> Result<Vec<Self::Value>, Self::Error>;
    fn iterator(&mut self, v: Self::Value) -> Result<Self::Iter, Self::Error>;
    fn next(&mut self, it: &mut Self::Iter) -> Result<Option<Self::Value>, Self::Error>;
    fn close(&mut self, it: &mut Self::Iter) -> Result<(), Self::Error>;
    fn await_element(&mut self, v: Self::Value) -> Task<Self::Value, Self::Error>;
    fn sort_history(&mut self, rows: Self::Value) -> Result<Self::Value, Self::Error>;
    fn slice(&mut self, v: Self::Value, limit: Self::Value) -> Result<Self::Value, Self::Error>;
    fn v(&mut self, v: &Self::Value, k: &str) -> Result<Self::Value, Self::Error>;
    fn error(&mut self, message: Self::Value, type_error: bool) -> Self::Error;
    fn error_field(&mut self, e: &Self::Error, path: &[&str]) -> Result<Self::Value, Self::Error>;
    fn emit(
        &mut self,
        c: Self::Prepared,
        original: Option<&Self::Error>,
        message: Self::Value,
    ) -> Result<(), Self::Error>;
}
fn s<H: Host>(h: &H, v: &str) -> H::Value {
    h.string(v.encode_utf16().collect())
}
fn array<H: Host>(h: &H, v: Vec<H::Value>) -> H::Value {
    h.array(v.into_iter().map(Some).collect())
}
fn opt<H: Host>(h: &mut H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        h.field(v, k)
    }
}
fn invoke<H: Host>(h: &mut H, b: Binding, a: Vec<H::Value>) -> Result<H::Value, H::Error> {
    let c = h.prepare(b)?;
    h.invoke(c, a)
}
fn call<H: Host>(
    h: &mut H,
    b: Binding,
    a: Vec<H::Value>,
) -> Result<Task<H::Value, H::Error>, H::Error> {
    let c = h.prepare(b)?;
    h.call(c, a)
}
fn endpoint<H: Host>(h: &H, k: &str) -> H::Value {
    s(
        h,
        crate::constants::ENDPOINTS
            .iter()
            .find(|(n, _)| *n == k)
            .expect("legacy endpoint")
            .1,
    )
}
fn path<H: Host>(h: &mut H, k: &str, id: &H::Value) -> Result<H::Value, H::Error> {
    let e = endpoint(h, k);
    let mut t = h.text(&e)?;
    t.push(47);
    t.extend(h.text(id)?);
    Ok(h.string(t))
}
fn message<H: Host>(
    h: &mut H,
    prefix: &str,
    v: &H::Value,
    suffix: &str,
) -> Result<H::Value, H::Error> {
    let mut t = prefix.encode_utf16().collect::<Vec<_>>();
    t.extend(h.text(v)?);
    t.extend(suffix.encode_utf16());
    Ok(h.string(t))
}
fn reject<H: Host>(
    h: &mut H,
    prefix: &str,
    v: &H::Value,
    suffix: &str,
) -> Result<H::Error, H::Error> {
    let m = message(h, prefix, v, suffix)?;
    Ok(h.error(m, false))
}
fn emit<H: Host>(h: &mut H, e: &H::Error, full: bool) -> Result<(), H::Error> {
    let c = h.prepare(Binding::Emit)?;
    let m = h.error_field(e, &["response", "data", "message"])?;
    let m = if h.truthy(&m) {
        m
    } else {
        h.error_field(e, &["message"])?
    };
    h.emit(c, if full { Some(e) } else { None }, m)
}
fn text_value<H: Host>(h: &mut H, v: &H::Value) -> Result<H::Value, H::Error> {
    let t = h.text(v)?;
    Ok(h.string(t))
}
fn market<H: Host>(h: &mut H, opts: &H::Value, ticker: bool) -> Result<H::Value, H::Error> {
    let m = h.root(if ticker {
        "tickersBySymbol"
    } else {
        "marketsBySymbol"
    })?;
    let symbol = h.field(opts, "symbol")?;
    h.keyed(&m, &symbol)
}
fn adjust<H: Host>(h: &mut H, n: H::Value, p: H::Value) -> Result<H::Value, H::Error> {
    invoke(h, Binding::Adjust, vec![n, p])
}
fn inverse<H: Host>(h: &mut H, b: Binding, opts: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    let c = h.prepare(b)?;
    let v = h.field(opts, k)?;
    h.invoke(c, vec![v])
}
fn trigger<H: Host>(h: &mut H, raw: H::Value) -> Result<H::Value, H::Error> {
    let raw = if h.truthy(&raw) {
        raw
    } else {
        s(h, "mark_price")
    };
    invoke(h, Binding::InverseTrigger, vec![raw])
}
fn positive<H: Host>(h: &mut H, v: &H::Value) -> Result<bool, H::Error> {
    Ok(h.is_number(v) && {
        let n = h.number_coerce(v)?;
        n.is_finite() && n > 0.0
    })
}
fn ids<H: Host>(h: &mut H, rows: H::Value) -> Result<H::Value, H::Error> {
    let n = h.length(&rows)?;
    let mut out = vec![];
    for i in 0..n {
        out.push(if h.has_index(&rows, i)? {
            let row = h.index(&rows, i)?;
            let id = h.field(&row, "orderId")?;
            Some(text_value(h, &id)?)
        } else {
            None
        });
    }
    Ok(h.array(out))
}
fn algo_ids<H: Host>(h: &mut H, response: H::Value, optional: bool) -> Result<H::Value, H::Error> {
    let d = h.field(&response, "data")?;
    let d = h.field(&d, "data")?;
    let rows = if optional {
        opt(h, &d, "rows")?
    } else {
        h.field(&d, "rows")?
    };
    if optional && h.nullish(&rows) {
        return Ok(array(h, vec![]));
    }
    ids(h, rows)
}
fn finish<H: Host>(
    host: Rc<RefCell<H>>,
    task: Task<H::Value, H::Error>,
    full: bool,
    fallback: bool,
) -> Task<H::Value, H::Error> {
    Box::pin(async move {
        match task.await {
            Ok(v) => Ok(v),
            Err(e) => {
                let mut h = host.borrow_mut();
                emit(&mut *h, &e, full)?;
                if fallback {
                    Ok(array(&*h, vec![]))
                } else {
                    Err(e)
                }
            }
        }
    })
}
enum Read<H: Host> {
    Pending(Task<H::Value, H::Error>),
    Complete(Result<H::Value, H::Error>),
}
#[derive(Clone, Copy)]
enum Fallback {
    Reject,
    Positions,
}
fn capture<H: Host>(
    h: &mut H,
    read: Result<Task<H::Value, H::Error>, H::Error>,
    full: bool,
    fallback: Fallback,
) -> Read<H> {
    match read {
        Ok(t) => Read::Pending(t),
        Err(e) => Read::Complete((|| {
            emit(h, &e, full)?;
            match fallback {
                Fallback::Reject => Err(e),
                Fallback::Positions => h.root("positions"),
            }
        })()),
    }
}
// ts: WOOXExchange.updateOrder
pub fn update_order<H: Host>(host: Rc<RefCell<H>>, opts: H::Value) -> Task<H::Value, H::Error> {
    let first = (|| {
        let mut h = host.borrow_mut();
        let order = h.field(&opts, "order")?;
        let update = h.field(&opts, "update")?;
        let ty = h.field(&order, "type")?;
        if h.equals(&ty, "trailing_stop_market") {
            let c = h.prepare(Binding::CancelOrders)?;
            let a = array(&*h, vec![order.clone()]);
            return Ok((0, order, update, Read::Pending(h.call(c, vec![a])?)));
        }
        let c = h.prepare(Binding::IsAlgo)?;
        let ty = h.field(&order, "type")?;
        let is = h.invoke(c, vec![ty])?;
        if h.truthy(&is) {
            let a = h.object(vec![("order", order.clone()), ("update", update.clone())]);
            let task = call(&mut *h, Binding::UpdateAlgo, vec![a])?;
            return Ok((1, order, update, Read::Pending(task)));
        }
        let id = h.field(&order, "id")?;
        let payload = h.object(vec![("orderId", id)]);
        if h.has(&update, "price")? {
            let p = h.field(&update, "price")?;
            let p = text_value(&mut *h, &p)?;
            h.set(&payload, "price", p)?;
        }
        let a = if h.has(&update, "amount")? {
            h.field(&update, "amount")?
        } else {
            h.field(&order, "amount")?
        };
        let a = text_value(&mut *h, &a)?;
        h.set(&payload, "quantity", a)?;
        let read = (|| {
            let c = h.prepare(Binding::Put)?;
            let id = h.field(&order, "id")?;
            let url = path(&mut *h, "UPDATE_ORDER", &id)?;
            h.call(c, vec![url, payload])
        })();
        let read = capture(&mut *h, read, false, Fallback::Reject);
        Ok((2, order, update, read))
    })();
    Box::pin(async move {
        let (kind, order, update, read) = first?;
        let task = match read {
            Read::Complete(v) => return v,
            Read::Pending(t) => t,
        };
        if kind == 1 {
            return task.await;
        }
        if kind == 0 {
            task.await?;
            let task = {
                let mut h = host.borrow_mut();
                let c = h.prepare(Binding::Trailing)?;
                let symbol = h.field(&order, "symbol")?;
                let side = h.field(&order, "side")?;
                let ty = s(&*h, "trailing_stop_market");
                let price = if h.has(&update, "price")? {
                    h.field(&update, "price")?
                } else {
                    h.field(&order, "price")?
                };
                let amount = if h.has(&update, "amount")? {
                    h.field(&update, "amount")?
                } else {
                    h.field(&order, "amount")?
                };
                let a = h.object(vec![
                    ("symbol", symbol),
                    ("side", side),
                    ("type", ty),
                    ("price", price),
                    ("amount", amount),
                ]);
                h.call(c, vec![a])?
            };
            return task.await;
        }
        let result = async {
            task.await?;
            let mut h = host.borrow_mut();
            let id = h.field(&order, "id")?;
            Ok(array(&*h, vec![id]))
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                emit(&mut *host.borrow_mut(), &e, false)?;
                Err(e)
            }
        }
    })
}
// ts: WOOXExchange.updateAlgoOrder
pub fn update_algo_order<H: Host>(
    host: Rc<RefCell<H>>,
    opts: H::Value,
) -> Task<H::Value, H::Error> {
    let first = (|| {
        let mut h = host.borrow_mut();
        let order = h.field(&opts, "order")?;
        let update = h.field(&opts, "update")?;
        let payload = h.object(vec![]);
        if h.has(&update, "price")? {
            let a = h.field(&order, "amount")?;
            if h.truthy(&a) {
                let p = h.field(&update, "price")?;
                let p = text_value(&mut *h, &p)?;
                h.set(&payload, "triggerPrice", p)?;
            } else {
                let id = h.field(&order, "id")?;
                let p = h.field(&update, "price")?;
                let p = text_value(&mut *h, &p)?;
                let child = h.object(vec![("algoOrderId", id), ("triggerPrice", p)]);
                let a = array(&*h, vec![child]);
                h.set(&payload, "childOrders", a)?;
            }
        }
        let read = (|| {
            let c = h.prepare(Binding::Put)?;
            let id = h.field(&order, "parentId")?;
            let url = path(&mut *h, "ALGO_ORDER", &id)?;
            h.call(c, vec![url, payload])
        })();
        let read = capture(&mut *h, read, false, Fallback::Reject);
        Ok((order, read))
    })();
    Box::pin(async move {
        let (order, read) = first?;
        let task = match read {
            Read::Complete(v) => return v,
            Read::Pending(t) => t,
        };
        let result = async {
            task.await?;
            let mut h = host.borrow_mut();
            let id = h.field(&order, "id")?;
            Ok(array(&*h, vec![id]))
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                emit(&mut *host.borrow_mut(), &e, false)?;
                Err(e)
            }
        }
    })
}
fn split<H: Host>(
    h: &mut H,
    amount: H::Value,
    max: H::Value,
    precision: H::Value,
    req: H::Value,
    key: &'static str,
) -> Result<H::Value, H::Error> {
    let a = h.number_coerce(&amount)?;
    let m = h.number_coerce(&max)?;
    let lots = if a > m { (a / m).ceil() } else { 1.0 };
    let rest = if h.number_coerce(&amount)? > h.number_coerce(&max)? {
        let remainder = h.number_coerce(&amount)? % h.number_coerce(&max)?;
        let r = h.number(remainder);
        adjust(h, r, precision.clone())?
    } else {
        h.number(0.0)
    };
    let quantity = (h.number_coerce(&amount)? - h.number_coerce(&rest)?) / lots;
    let n = h.number(quantity);
    let qty = adjust(h, n, precision)?;
    let rows = h.times(lots, req.clone(), key, qty)?;
    if h.truthy(&rest) {
        let tail = h.spread_object(&req, vec![(key, rest)])?;
        h.push(&rows, tail)?;
    }
    Ok(rows)
}
fn posts<H: Host>(
    host: Rc<RefCell<H>>,
    payloads: H::Value,
    algo: bool,
) -> Task<H::Value, H::Error> {
    Box::pin(async move {
        let mut out = vec![];
        let mut i = 0;
        loop {
            let item = {
                let mut h = host.borrow_mut();
                if i >= h.length(&payloads)? {
                    break;
                }
                if h.has_index(&payloads, i)? {
                    let p = h.index(&payloads, i)?;
                    Some(h.await_element(p))
                } else {
                    None
                }
            };
            if let Some(p) = item {
                let p = p.await?;
                let task = {
                    let mut h = host.borrow_mut();
                    let c = h.prepare(Binding::Post)?;
                    let url = endpoint(&*h, if algo { "ALGO_ORDER" } else { "PLACE_ORDER" });
                    h.call(c, vec![url, p])?
                };
                let response = task.await?;
                let mut h = host.borrow_mut();
                let id = if algo {
                    algo_ids(&mut *h, response, false)?
                } else {
                    let d = h.field(&response, "data")?;
                    let id = h.field(&d, "order_id")?;
                    text_value(&mut *h, &id)?
                };
                while out.len() <= i {
                    out.push(None);
                }
                out[i] = Some(id);
            }
            i += 1;
        }
        let mut h = host.borrow_mut();
        let rows = h.array(out);
        if algo { h.flatten_one(rows) } else { Ok(rows) }
    })
}
// ts: WOOXExchange.placeAlgoOrder
pub fn place_algo_order<H: Host>(host: Rc<RefCell<H>>, opts: H::Value) -> Task<H::Value, H::Error> {
    let first = (|| {
        let mut h = host.borrow_mut();
        let price = h.field(&opts, "price")?;
        if !h.truthy(&price) {
            let c = h.prepare(Binding::Emit)?;
            let m = s(&*h, "Price is required for algo orders");
            h.emit(c, None, m)?;
            return Ok((None, None, None));
        }
        let ty = h.field(&opts, "type")?;
        if h.equals(&ty, "trailing_stop_market") {
            let t = call(&mut *h, Binding::Trailing, vec![opts.clone()])?;
            return Ok((Some(t), None, None));
        }
        let amount = h.field(&opts, "amount")?;
        if h.truthy(&amount) {
            let t = call(&mut *h, Binding::Stop, vec![opts.clone()])?;
            return Ok((Some(t), None, None));
        }
        let params = h.spread_object(&opts, vec![])?;
        let symbol = h.field(&opts, "symbol")?;
        let side = h.field(&opts, "side")?;
        let side = s(
            &*h,
            if h.equals(&side, "buy") {
                "buy"
            } else {
                "sell"
            },
        );
        h.set(&params, "symbol", symbol)?;
        h.set(&params, "side", side)?;
        let zero = h.number(0.0);
        h.set(&params, "stopLoss", zero.clone())?;
        h.set(&params, "takeProfit", zero)?;
        let rows = h.root("orders")?;
        let n = h.length(&rows)?;
        let mut sibling = None;
        for i in 0..n {
            let row = h.index(&rows, i)?;
            let a = h.field(&row, "symbol")?;
            let b = h.field(&opts, "symbol")?;
            if !h.same(&a, &b) {
                continue;
            }
            let a = h.field(&row, "side")?;
            let b = h.field(&opts, "side")?;
            if !h.same(&a, &b) {
                continue;
            }
            let a = h.field(&row, "type")?;
            let b = h.field(&opts, "type")?;
            let desired = if h.equals(&b, "stop_market") {
                "take_profit_market"
            } else {
                "stop_market"
            };
            if !h.equals(&a, desired) {
                continue;
            }
            let status = h.field(&row, "status")?;
            if h.equals(&status, "open") {
                sibling = Some(row);
                break;
            }
        }
        let ty = h.field(&opts, "type")?;
        if h.equals(&ty, "stop_market") {
            let p = h.field(&opts, "price")?;
            h.set(&params, "stopLoss", p)?;
        }
        let ty = h.field(&opts, "type")?;
        if h.equals(&ty, "take_profit_market") {
            let p = h.field(&opts, "price")?;
            h.set(&params, "takeProfit", p)?;
        }
        let task = if let Some(row) = sibling {
            let ty = h.field(&row, "type")?;
            if h.equals(&ty, "stop_market") {
                let p = h.field(&row, "price")?;
                h.set(&params, "stopLoss", p)?;
            }
            let ty = h.field(&row, "type")?;
            if h.equals(&ty, "take_profit_market") {
                let p = h.field(&row, "price")?;
                h.set(&params, "takeProfit", p)?;
            }
            let c = h.prepare(Binding::CancelAlgo)?;
            let no = h.boolean(false);
            Some(h.call(c, vec![row, no])?)
        } else {
            let task = call(&mut *h, Binding::Positional, vec![params])?;
            return Ok((Some(task), None, None));
        };
        Ok((task, Some(params), Some(())))
    })();
    Box::pin(async move {
        let (task, params, kind) = first?;
        if let Some(task) = task {
            let v = task.await?;
            if kind.is_none() {
                return Ok(v);
            }
        }
        let Some(params) = params else {
            return Ok(array(&*host.borrow(), vec![]));
        };
        let task = call(&mut *host.borrow_mut(), Binding::Positional, vec![params])?;
        task.await
    })
}
struct OrderStart<H: Host> {
    opts: H::Value,
    client: H::Value,
    max: H::Value,
    p_amount: H::Value,
    p_price: H::Value,
    amount: H::Value,
    price: H::Value,
    ty: H::Value,
    order_price: H::Value,
    ticker: H::Value,
    touch: Option<Task<H::Value, H::Error>>,
    market: bool,
    slippage: Option<f64>,
    range: Option<H::Value>,
}
fn regular<H: Host>(
    h: &mut H,
    opts: H::Value,
    client: H::Value,
) -> Result<OrderStart<H>, H::Error> {
    let m = market(h, &opts, false)?;
    if !h.truthy(&m) {
        let sym = h.field(&opts, "symbol")?;
        return Err(reject(h, "Market not found: ", &sym, "")?);
    }
    let limits = h.field(&m, "limits")?;
    let limits = h.field(&limits, "amount")?;
    let max = h.field(&limits, "max")?;
    let precision = h.field(&m, "precision")?;
    let p_price = h.field(&precision, "price")?;
    let precision = h.field(&m, "precision")?;
    let p_amount = h.field(&precision, "amount")?;
    let c = h.prepare(Binding::Adjust)?;
    let a = h.field(&opts, "amount")?;
    let amount = h.invoke(c, vec![a, p_amount.clone()])?;
    let p = h.field(&opts, "price")?;
    let price = if h.truthy(&p) {
        let c = h.prepare(Binding::Adjust)?;
        let p = h.field(&opts, "price")?;
        h.invoke(c, vec![p, p_price.clone()])?
    } else {
        h.null()
    };
    let ty = inverse(h, Binding::InverseType, &opts, "type")?;
    let ot = h.field(&opts, "type")?;
    let order_price = if h.equals(&ot, "limit") {
        price.clone()
    } else {
        h.undefined()
    };
    let ticker = market(h, &opts, true)?;
    let tif = h.field(&opts, "timeInForce")?;
    let post = if h.equals(&tif, "PostOnly") {
        let ty = h.field(&opts, "type")?;
        h.equals(&ty, "limit")
    } else {
        false
    };
    // Turbofish is required, not stylistic: every field of OrderStart<H> is an
    // `H::Value`/`H::Error` associated type, and those do not pin `H`, so the
    // bare literal is uninferrable (E0282). The function's return type already
    // names `OrderStart<H>`.
    let mut d = OrderStart::<H> {
        opts,
        client,
        max,
        p_amount,
        p_price,
        amount,
        price,
        ty,
        order_price,
        ticker,
        touch: None,
        market: false,
        slippage: None,
        range: None,
    };
    if post {
        let limit = if h.nullish(&d.price) {
            h.undefined()
        } else {
            d.price.clone()
        };
        if positive(h, &limit)? {
            let c = h.prepare(Binding::FairCross)?;
            let side = h.field(&d.opts, "side")?;
            let cross = h.invoke(c, vec![side, limit, d.ticker.clone()])?;
            let crosses = opt(h, &cross, "wouldCross")?;
            if h.truthy(&crosses) {
                let side = h.field(&d.opts, "side")?;
                let m = message(
                    h,
                    "WOO post-only ",
                    &side,
                    " order would cross current fair price ",
                )?;
                let mut t = h.text(&m)?;
                let p = h.field(&cross, "fairPrice")?;
                t.extend(h.text(&p)?);
                let m = h.string(t);
                return Err(h.error(m, false));
            }
            let c = h.prepare(Binding::Touch)?;
            let symbol = h.field(&d.opts, "symbol")?;
            let side = h.field(&d.opts, "side")?;
            d.touch = Some(h.call(c, vec![symbol, side])?);
        }
        d.ty = s(h, "POST_ONLY");
    } else {
        let ty = h.field(&d.opts, "type")?;
        if h.equals(&ty, "market") {
            d.market = true;
            let v = h.field(&d.opts, "maxSlippagePct")?;
            let finite = if h.is_number(&v) {
                let v = h.field(&d.opts, "maxSlippagePct")?;
                h.is_number(&v) && h.number_coerce(&v)?.is_finite()
            } else {
                false
            };
            let slip = if finite {
                h.field(&d.opts, "maxSlippagePct")?
            } else {
                h.number(0.9)
            };
            let range = invoke(h, Binding::PriceRange, vec![m])?;
            d.slippage = Some(safe_cex_num::math_min(
                h.number_coerce(&slip)?,
                h.number_coerce(&range)?,
            ));
            d.range = Some(range);
            let c = h.prepare(Binding::Touch)?;
            let symbol = h.field(&d.opts, "symbol")?;
            let side = h.field(&d.opts, "side")?;
            d.touch = Some(h.call(c, vec![symbol, side])?);
        }
    }
    Ok(d)
}
fn request_side<H: Host>(
    h: &mut H,
    d: &OrderStart<H>,
) -> Result<(Vec<(&'static str, H::Value)>, Task<H::Value, H::Error>), H::Error> {
    let c = h.prepare(Binding::Reverse)?;
    let symbol = h.field(&d.opts, "symbol")?;
    let symbol = h.invoke(c, vec![symbol])?;
    let reduce = h.field(&d.opts, "reduceOnly")?;
    let side = inverse(h, Binding::InverseSide, &d.opts, "side")?;
    let fields = vec![
        ("symbol", symbol),
        ("order_type", d.ty.clone()),
        ("order_price", d.order_price.clone()),
        ("reduce_only", reduce),
        ("side", side),
    ];
    let task = call(h, Binding::PositionSide, vec![d.opts.clone()])?;
    Ok((fields, task))
}
// ts: WOOXExchange.placeOrder
pub fn place_order<H: Host>(host: Rc<RefCell<H>>, opts: H::Value) -> Task<H::Value, H::Error> {
    let first = (|| {
        let mut h = host.borrow_mut();
        let client = invoke(&mut *h, Binding::ResolveClientId, vec![opts.clone()])?;
        let c = h.prepare(Binding::IsAlgo)?;
        let ty = h.field(&opts, "type")?;
        let algo = h.invoke(c, vec![ty])?;
        if h.truthy(&algo) {
            let t = call(&mut *h, Binding::PlaceAlgo, vec![opts])?;
            return Ok((Some(t), None, None));
        }
        let d = regular(&mut *h, opts, client)?;
        let side = if d.touch.is_none() {
            Some(request_side(&mut *h, &d)?)
        } else {
            None
        };
        Ok((None, Some(d), side))
    })();
    Box::pin(async move {
        let (delegated, d, side) = first?;
        if let Some(t) = delegated {
            return t.await;
        }
        let mut d = d.expect("regular order");
        let side = if let Some(t) = d.touch.take() {
            let touch = t.await?;
            let mut h = host.borrow_mut();
            if d.market {
                let c = h.prepare(Binding::ChoosePrice)?;
                let side = h.field(&d.opts, "side")?;
                let slip = h.number(d.slippage.expect("market slippage"));
                let cross = h.invoke(
                    c,
                    vec![
                        d.ticker.clone(),
                        side,
                        slip,
                        d.range.take().expect("market range"),
                        touch,
                    ],
                )?;
                let valid = if h.truthy(&cross) {
                    let n = h.number_coerce(&cross)?;
                    n.is_finite() && n > 0.0
                } else {
                    false
                };
                if !valid {
                    let sym = h.field(&d.opts, "symbol")?;
                    return Err(reject(
                        &mut *h,
                        "Ticker ",
                        &sym,
                        " missing valid price for WOO market IOC order",
                    )?);
                }
                d.order_price = adjust(&mut *h, cross, d.p_price.clone())?;
                d.ty = s(&*h, "IOC");
            } else if positive(&mut *h, &touch)? {
                let c = h.prepare(Binding::Cross)?;
                let side = h.field(&d.opts, "side")?;
                let limit = if h.nullish(&d.price) {
                    h.undefined()
                } else {
                    d.price.clone()
                };
                let cross = h.invoke(c, vec![side, limit, touch.clone()])?;
                if h.truthy(&cross) {
                    let side = h.field(&d.opts, "side")?;
                    let mut text = message(
                        &mut *h,
                        "WOO post-only ",
                        &side,
                        " order would cross current ",
                    )?;
                    let mut units = h.text(&text)?;
                    let side = h.field(&d.opts, "side")?;
                    units.extend(
                        if h.equals(&side, "buy") {
                            "ask "
                        } else {
                            "bid "
                        }
                        .encode_utf16(),
                    );
                    units.extend(h.text(&touch)?);
                    text = h.string(units);
                    return Err(h.error(text, false));
                }
            }
            request_side(&mut *h, &d)?
        } else {
            side.expect("eager position-side request")
        };
        let (fields, task) = side;
        let side = task.await?;
        let payloads = {
            let mut h = host.borrow_mut();
            let mut fields = fields;
            fields.push(("position_side", side));
            let client = if h.is_undefined(&d.client) {
                h.undefined()
            } else {
                let n = h.number_coerce(&d.client)?;
                h.number(n)
            };
            fields.push(("client_order_id", client));
            let req = h.object(fields);
            let req = h.omit_undefined(req)?;
            let rows = split(&mut *h, d.amount, d.max, d.p_amount, req, "order_quantity")?;
            let c = h.prepare(Binding::AssertClientId)?;
            let n = h.length(&rows)?;
            let n = h.number(n as f64);
            h.invoke(c, vec![d.client, n])?;
            rows
        };
        let result = async {
            let order_ids = posts(host.clone(), payloads, false).await?;
            let extra = {
                let mut h = host.borrow_mut();
                let sl = h.field(&d.opts, "stopLoss")?;
                let tp = if h.truthy(&sl) {
                    h.undefined()
                } else {
                    h.field(&d.opts, "takeProfit")?
                };
                if h.truthy(&sl) || h.truthy(&tp) {
                    let c = h.prepare(Binding::Positional)?;
                    let opts = h.spread_object(&d.opts, vec![])?;
                    let side = h.field(&d.opts, "side")?;
                    let side = s(
                        &*h,
                        if h.equals(&side, "buy") {
                            "sell"
                        } else {
                            "buy"
                        },
                    );
                    h.set(&opts, "side", side)?;
                    Some(h.call(c, vec![opts])?)
                } else {
                    None
                }
            };
            if let Some(extra) = extra {
                let extra = extra.await?;
                let mut h = host.borrow_mut();
                for id in h.spread_array(&extra)? {
                    h.push(&order_ids, id)?;
                }
            }
            Ok(order_ids)
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                emit(&mut *host.borrow_mut(), &e, true)?;
                Err(e)
            }
        }
    })
}
// ts: WOOXExchange.placeStopMarket
pub fn place_stop_market<H: Host>(
    host: Rc<RefCell<H>>,
    opts: H::Value,
) -> Task<H::Value, H::Error> {
    let first = (|| {
        let mut h = host.borrow_mut();
        let m = market(&mut *h, &opts, false)?;
        let ticker = market(&mut *h, &opts, true)?;
        if !h.truthy(&m) {
            let symbol = h.field(&opts, "symbol")?;
            return Err(reject(&mut *h, "Market ", &symbol, " not found")?);
        }
        if !h.truthy(&ticker) {
            let symbol = h.field(&opts, "symbol")?;
            return Err(reject(&mut *h, "Ticker ", &symbol, " not found")?);
        }
        let limits = h.field(&m, "limits")?;
        let limits = h.field(&limits, "amount")?;
        let max = h.field(&limits, "max")?;
        let p = h.field(&m, "precision")?;
        let p_price = h.field(&p, "price")?;
        let p = h.field(&m, "precision")?;
        let p_amount = h.field(&p, "amount")?;
        let c = h.prepare(Binding::Adjust)?;
        let a = h.field(&opts, "amount")?;
        let amount = h.invoke(c, vec![a, p_amount.clone()])?;
        let price = h.field(&opts, "price")?;
        let price = if h.truthy(&price) {
            let c = h.prepare(Binding::Adjust)?;
            let p = h.field(&opts, "price")?;
            h.invoke(c, vec![p, p_price])?
        } else {
            h.null()
        };
        let ty = h.field(&opts, "type")?;
        let sl = if h.equals(&ty, "stop_market") {
            true
        } else {
            let ty = h.field(&opts, "type")?;
            h.equals(&ty, "trailing_stop_market")
        };
        let raw = h.field(&opts, if sl { "slTriggerType" } else { "tpTriggerType" })?;
        let ty = trigger(&mut *h, raw)?;
        let c = h.prepare(Binding::Reverse)?;
        let sym = h.field(&opts, "symbol")?;
        let sym = h.invoke(c, vec![sym])?;
        let side = inverse(&mut *h, Binding::InverseSide, &opts, "side")?;
        let fields = vec![
            ("symbol", sym),
            ("type", s(&*h, "MARKET")),
            ("algoType", s(&*h, "STOP")),
            ("side", side),
            ("triggerPrice", price),
            ("triggerPriceType", ty),
            ("reduceOnly", h.boolean(true)),
        ];
        let c = h.prepare(Binding::PositionSide)?;
        let yes = h.boolean(true);
        let side_opts = h.spread_object(&opts, vec![("reduceOnly", yes)])?;
        let task = h.call(c, vec![side_opts])?;
        Ok((fields, task, max, p_amount, amount))
    })();
    Box::pin(async move {
        let (mut fields, task, max, p_amount, amount) = first?;
        let side = task.await?;
        let payloads = {
            let mut h = host.borrow_mut();
            fields.push(("positionSide", side));
            let req = h.object(fields);
            let req = h.omit_undefined(req)?;
            split(&mut *h, amount, max, p_amount, req, "quantity")?
        };
        let task = posts(host.clone(), payloads, true);
        finish(host, task, true, false).await
    })
}
// ts: WOOXExchange.placeTrailingStopLossOrder
pub fn place_trailing_stop_loss_order<H: Host>(
    host: Rc<RefCell<H>>,
    opts: H::Value,
) -> Task<H::Value, H::Error> {
    let first = (|| {
        let mut h = host.borrow_mut();
        let m = market(&mut *h, &opts, false)?;
        let ticker = market(&mut *h, &opts, true)?;
        let side = h.field(&opts, "side")?;
        let ps = if h.equals(&side, "buy") {
            "short"
        } else {
            "long"
        };
        let rows = h.root("positions")?;
        let n = h.length(&rows)?;
        let mut position = None;
        for i in 0..n {
            let p = h.index(&rows, i)?;
            let a = h.field(&p, "symbol")?;
            let b = h.field(&opts, "symbol")?;
            if h.same(&a, &b) {
                let side = h.field(&p, "side")?;
                if h.equals(&side, ps) {
                    position = Some(p);
                    break;
                }
            }
        }
        if !h.truthy(&m) {
            let symbol = h.field(&opts, "symbol")?;
            return Err(reject(&mut *h, "Market ", &symbol, " not found")?);
        }
        if !h.truthy(&ticker) {
            let symbol = h.field(&opts, "symbol")?;
            return Err(reject(&mut *h, "Ticker ", &symbol, " not found")?);
        }
        let Some(position) = position else {
            let symbol = h.field(&opts, "symbol")?;
            let suffix = format!(" and side {ps} not found");
            return Err(reject(&mut *h, "Position ", &symbol, &suffix)?);
        };
        let c = h.prepare(Binding::Adjust)?;
        let last = h.field(&ticker, "last")?;
        let price = h.field(&opts, "price")?;
        let max = safe_cex_num::math_max(h.number_coerce(&last)?, h.number_coerce(&price)?);
        let last = h.field(&ticker, "last")?;
        let price = h.field(&opts, "price")?;
        let min = safe_cex_num::math_min(h.number_coerce(&last)?, h.number_coerce(&price)?);
        let diff = h.number(max - min);
        let p = h.field(&m, "precision")?;
        let p = h.field(&p, "price")?;
        let distance = h.invoke(c, vec![diff, p])?;
        let raw = h.field(&opts, "tpTriggerType")?;
        let ty = trigger(&mut *h, raw)?;
        let c = h.prepare(Binding::Reverse)?;
        let sym = h.field(&opts, "symbol")?;
        let sym = h.invoke(c, vec![sym])?;
        let callback = text_value(&mut *h, &distance)?;
        let side = inverse(&mut *h, Binding::InverseSide, &opts, "side")?;
        let contracts = h.field(&position, "contracts")?;
        let contracts = text_value(&mut *h, &contracts)?;
        let fields = vec![
            ("symbol", sym),
            ("type", s(&*h, "MARKET")),
            ("algoType", s(&*h, "TRAILING_STOP")),
            ("callbackValue", callback),
            ("side", side),
            ("reduceOnly", h.boolean(true)),
            ("quantity", contracts),
            ("triggerPriceType", ty),
        ];
        let c = h.prepare(Binding::PositionSide)?;
        let yes = h.boolean(true);
        let side_opts = h.spread_object(&opts, vec![("reduceOnly", yes)])?;
        let task = h.call(c, vec![side_opts])?;
        Ok((fields, task))
    })();
    Box::pin(async move {
        let (mut fields, task) = first?;
        let side = task.await?;
        let task = {
            let mut h = host.borrow_mut();
            fields.push(("positionSide", side));
            let req = h.object(fields);
            let req = h.omit_undefined(req)?;
            (|| {
                let c = h.prepare(Binding::Post)?;
                let url = endpoint(&*h, "ALGO_ORDER");
                h.call(c, vec![url, req])
            })()
        };
        let host2 = host.clone();
        let read = Box::pin(async move {
            let response = task?.await?;
            algo_ids(&mut *host2.borrow_mut(), response, true)
        });
        finish(host, read, false, true).await
    })
}
// ts: WOOXExchange.placePositionalAlgoOrder
pub fn place_positional_algo_order<H: Host>(
    host: Rc<RefCell<H>>,
    opts: H::Value,
) -> Task<H::Value, H::Error> {
    let first = (|| {
        let mut h = host.borrow_mut();
        let m = market(&mut *h, &opts, false)?;
        if !h.truthy(&m) {
            let symbol = h.field(&opts, "symbol")?;
            return Err(reject(&mut *h, "Market ", &symbol, " not found")?);
        }
        let p = h.field(&m, "precision")?;
        let p = h.field(&p, "price")?;
        let c = h.prepare(Binding::PositionSide)?;
        let yes = h.boolean(true);
        let side_opts = h.spread_object(&opts, vec![("reduceOnly", yes)])?;
        let task = h.call(c, vec![side_opts])?;
        Ok((p, task))
    })();
    Box::pin(async move {
        let (p_price, task) = first?;
        let side = task.await?;
        let task = {
            let mut h = host.borrow_mut();
            let other = h.object(vec![
                ("reduceOnly", h.boolean(true)),
                ("positionSide", side),
            ]);
            let other = h.omit_undefined(other)?;
            let req = h.spread_object(&other, vec![])?;
            let c = h.prepare(Binding::Reverse)?;
            let symbol = h.field(&opts, "symbol")?;
            let symbol = h.invoke(c, vec![symbol])?;
            h.set(&req, "symbol", symbol)?;
            let ty = s(&*h, "POSITIONAL_TP_SL");
            h.set(&req, "algoType", ty)?;
            let children = array(&*h, vec![]);
            h.set(&req, "childOrders", children.clone())?;
            for (price_key, trigger_key, ty) in [
                ("stopLoss", "slTriggerType", "STOP_LOSS"),
                ("takeProfit", "tpTriggerType", "TAKE_PROFIT"),
            ] {
                let price = h.field(&opts, price_key)?;
                if h.truthy(&price) {
                    let raw = h.field(&opts, trigger_key)?;
                    let trigger = trigger(&mut *h, raw)?;
                    let child = h.spread_object(&other, vec![])?;
                    let ty = s(&*h, ty);
                    h.set(&child, "algoType", ty)?;
                    let ty = s(&*h, "CLOSE_POSITION");
                    h.set(&child, "type", ty)?;
                    h.set(&child, "triggerPriceType", trigger)?;
                    let side = h.field(&opts, "side")?;
                    let side = s(
                        &*h,
                        if h.equals(&side, "sell") {
                            "SELL"
                        } else {
                            "BUY"
                        },
                    );
                    h.set(&child, "side", side)?;
                    let c = h.prepare(Binding::Adjust)?;
                    let price = h.field(&opts, price_key)?;
                    let price = h.invoke(c, vec![price, p_price.clone()])?;
                    let price = text_value(&mut *h, &price)?;
                    h.set(&child, "triggerPrice", price)?;
                    h.push(&children, child)?;
                }
            }
            (|| {
                let c = h.prepare(Binding::Post)?;
                let url = endpoint(&*h, "ALGO_ORDER");
                h.call(c, vec![url, req])
            })()
        };
        let host2 = host.clone();
        let read = Box::pin(async move {
            let response = task?.await?;
            algo_ids(&mut *host2.borrow_mut(), response, true)
        });
        finish(host, read, false, false).await
    })
}
// ts: WOOXExchange.fetchHistoricalOrders
pub fn fetch_historical_orders<H: Host>(
    host: Rc<RefCell<H>>,
    opts: H::Value,
) -> Task<H::Value, H::Error> {
    let start = (|| {
        let mut h = host.borrow_mut();
        let public = h.root("isPublicDataOnly")?;
        if h.truthy(&public) {
            return Ok(None);
        }
        let symbol = opt(&mut *h, &opts, "symbol")?;
        if h.truthy(&symbol) {
            let markets = h.root("marketsBySymbol")?;
            let symbol = h.field(&opts, "symbol")?;
            let m = h.keyed(&markets, &symbol)?;
            if !h.truthy(&m) {
                return Ok(None);
            }
        }
        let symbol = opt(&mut *h, &opts, "symbol")?;
        let request = if h.truthy(&symbol) {
            let c = h.prepare(Binding::Reverse)?;
            let symbol = h.field(&opts, "symbol")?;
            h.invoke(c, vec![symbol])?
        } else {
            h.undefined()
        };
        Ok(Some(request))
    })();
    let prepared = start.map(|request| {
        request.map(|request| {
            let read = history_request(&mut *host.borrow_mut(), &opts, request.clone(), 1.0);
            (request, read)
        })
    });
    if let Ok(Some((_, Err(e)))) = &prepared {
        let mut h = host.borrow_mut();
        let result = emit(&mut *h, e, false).map(|()| array(&*h, vec![]));
        return Box::pin(std::future::ready(result));
    }
    Box::pin(async move {
        let Some((request, mut read)) = prepared? else {
            return Ok(array(&*host.borrow(), vec![]));
        };
        let result=async{let rows=array(&*host.borrow(),vec![]);let mut seen=vec![];let mut page=1.0;
        loop{let response=read?.await?;let mut h=host.borrow_mut();let data=h.field(&response,"data")?;let disposed=h.root("isDisposed")?;if h.truthy(&disposed){return Ok(array(&*h,vec![]));}let success=opt(&mut *h,&data,"success")?;if h.same(&success,&h.boolean(false)){let c=h.prepare(Binding::Emit)?;let m=opt(&mut *h,&data,"message")?;let m=if h.truthy(&m){m}else{let meta=opt(&mut *h,&data,"meta")?;let m=opt(&mut *h,&meta,"message")?;if h.truthy(&m){m}else{s(&*h,"Failed to fetch order history")}};h.emit(c,None,m)?;break;}
            let r=opt(&mut *h,&data,"rows")?;let r=if h.is_array(&r){h.field(&data,"rows")?}else{array(&*h,vec![])};if h.length(&r)?==0{break;}let mut it=h.iterator(r.clone())?;
            while let Some(row)=h.next(&mut it)? {let mapped=(||{let order=invoke(&mut *h,Binding::MapHistorical,vec![row])?;if !h.truthy(&order){return Ok(());}let id=h.field(&order,"id")?;if seen.iter().any(|v|h.same(v,&id)){return Ok(());}let id=h.field(&order,"id")?;seen.push(id);h.push(&rows,order)?;Ok(())})();if let Err(e)=mapped{match h.close(&mut it){Ok(())|Err(_)=>return Err(e)}}}
            let meta=opt(&mut *h,&data,"meta")?;let total=opt(&mut *h,&meta,"total")?;let total=if h.nullish(&total){h.number(0.0)}else{total};let total=h.number_coerce(&total)?;let meta=opt(&mut *h,&data,"meta")?;let per=opt(&mut *h,&meta,"records_per_page")?;let per=if h.nullish(&per){h.number(100.0)}else{per};let per=h.number_coerce(&per)?;
            if total==0.0||total.is_nan(){if h.length(&r)?<100{break;}}else if total<=per*page{break;}page+=1.0;if page>50.0{let c=h.prepare(Binding::Log)?;let m=s(&*h,"fetchHistoricalOrders stopped at the 50-page cap; older orders were not fetched");let level=s(&*h,"warning");h.invoke(c,vec![m,level])?;break;}read=history_request(&mut *h,&opts,request.clone(),page);
        }
        let mut h=host.borrow_mut();let sorted=h.sort_history(rows)?;let limit=opt(&mut *h,&opts,"limit")?;if h.truthy(&limit){let limit=h.field(&opts,"limit")?;if h.number_coerce(&limit)?>0.0{let limit=h.field(&opts,"limit")?;return h.slice(sorted,limit);}}Ok(sorted)}.await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                emit(&mut *host.borrow_mut(), &e, false)?;
                Ok(array(&*host.borrow(), vec![]))
            }
        }
    })
}
fn history_request<H: Host>(
    h: &mut H,
    opts: &H::Value,
    symbol: H::Value,
    page: f64,
) -> Result<Task<H::Value, H::Error>, H::Error> {
    let c = h.prepare(Binding::Get)?;
    let url = endpoint(h, "ORDERS");
    let from = opt(h, opts, "from")?;
    let to = opt(h, opts, "to")?;
    let params = h.object(vec![
        ("symbol", symbol),
        ("start_t", from),
        ("end_t", to),
        ("size", h.number(100.0)),
        ("page", h.number(page)),
        ("realized_pnl", h.boolean(true)),
    ]);
    let params = h.omit_undefined(params)?;
    let config = h.object(vec![("params", params)]);
    h.call(c, vec![url, config])
}
// ts: WOOXExchange.fetchOpenPositions
pub fn fetch_open_positions<H: Host>(host: Rc<RefCell<H>>) -> Task<H::Value, H::Error> {
    let first = {
        let mut h = host.borrow_mut();
        let c = h.prepare(Binding::Get);
        c.and_then(|c| {
            let url = endpoint(&*h, "POSITIONS");
            h.call(c, vec![url])
        })
    };
    let first = capture(&mut *host.borrow_mut(), first, false, Fallback::Positions);
    Box::pin(async move {
        let task = match first {
            Read::Complete(v) => return v,
            Read::Pending(t) => t,
        };
        let result = async {
            let response = task.await?;
            let (data, task) = {
                let mut h = host.borrow_mut();
                let data = h.field(&response, "data")?;
                let success = opt(&mut *h, &data, "success")?;
                if !h.truthy(&success) {
                    let m = h.field(&data, "message")?;
                    let m = if h.truthy(&m) {
                        m
                    } else {
                        s(&*h, "Failed to fetch positions")
                    };
                    return Err(h.error(m, false));
                }
                let task = call(&mut *h, Binding::PositionMode, vec![])?;
                (data, task)
            };
            let mode = task.await?;
            let mut h = host.borrow_mut();
            let inner = opt(&mut *h, &data, "data")?;
            let positions = opt(&mut *h, &inner, "positions")?;
            let positions = if h.truthy(&positions) {
                positions
            } else {
                array(&*h, vec![])
            };
            if !h.is_array(&positions) {
                let m = s(&*h, "positions.reduce is not a function");
                return Err(h.error(m, true));
            }
            let out = array(&*h, vec![]);
            let mut i = 0;
            while i < h.length(&positions)? {
                let p = h.index(&positions, i)?;
                i += 1;
                let c = h.prepare(Binding::Normalize)?;
                let symbol = h.field(&p, "symbol")?;
                let symbol = h.invoke(c, vec![symbol])?;
                let tickers = h.root("tickersBySymbol")?;
                let ticker = h.keyed(&tickers, &symbol)?;
                if !h.truthy(&ticker) {
                    continue;
                }
                let holding = h.field(&p, "holding")?;
                if !h.truthy(&holding) {
                    continue;
                }
                let entry = h.v(&p, "averageOpenPrice")?;
                let holding = h.v(&p, "holding")?;
                let contracts = h.number_coerce(&holding)?.abs();
                let side = if h.number_coerce(&holding)? > 0.0 {
                    "long"
                } else {
                    "short"
                };
                let mark = h.field(&ticker, "mark")?;
                let max = safe_cex_num::math_max(h.number_coerce(&entry)?, h.number_coerce(&mark)?);
                let mark = h.field(&ticker, "mark")?;
                let min = safe_cex_num::math_min(h.number_coerce(&entry)?, h.number_coerce(&mark)?);
                let loss = if side == "long" {
                    let mark = h.field(&ticker, "mark")?;
                    h.less(&mark, &entry)?
                } else {
                    false
                };
                let loss = if loss {
                    true
                } else if side == "short" {
                    let mark = h.field(&ticker, "mark")?;
                    h.less(&entry, &mark)?
                } else {
                    false
                };
                let abs = (max - min) * contracts;
                let upnl =
                    safe_cex_num::math_round((if loss { -abs } else { abs }) * 100.0) / 100.0;
                let c = h.prepare(Binding::PositionId)?;
                let side = s(&*h, side);
                let id = h.invoke(c, vec![symbol.clone(), side.clone(), mode.clone()])?;
                let mark = h.field(&ticker, "mark")?;
                let notional = contracts * h.number_coerce(&mark)?;
                let leverage = h.field(&p, "leverage")?;
                let liquidation = h.v(&p, "estLiqPrice")?;
                let margin = h.field(&p, "marginMode")?;
                let margin = s(
                    &*h,
                    if h.equals(&margin, "CROSS") {
                        "cross"
                    } else {
                        "isolated"
                    },
                );
                let row = h.object(vec![
                    ("id", id),
                    ("symbol", symbol),
                    ("side", side),
                    ("entryPrice", entry),
                    ("notional", h.number(notional)),
                    ("leverage", leverage),
                    ("unrealizedPnl", h.number(upnl)),
                    ("contracts", h.number(contracts)),
                    ("liquidationPrice", liquidation),
                    ("positionMode", mode.clone()),
                    ("marginType", margin.clone()),
                    ("info", p),
                ]);
                if h.equals(&margin, "isolated") {
                    let c = h.prepare(Binding::Emit)?;
                    let m = s(
                        &*h,
                        "Detected isolated margin position. Only cross margin is supported",
                    );
                    h.emit(c, None, m)?;
                }
                h.push(&out, row)?;
            }
            Ok(out)
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                let mut h = host.borrow_mut();
                emit(&mut *h, &e, false)?;
                h.root("positions")
            }
        }
    })
}
