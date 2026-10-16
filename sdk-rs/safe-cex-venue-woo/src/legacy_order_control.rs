//! Legacy WOO order read wrappers, cancellation and hedge-side selection.
use crate::legacy_metadata::Task;
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    LimitOrders,
    AlgoOrders,
    PositionMode,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Delete,
    MapLimit,
    MapAlgo,
    IsAlgo,
    Place,
    Emit,
    ParseInt,
    ReverseSymbol,
    UpdateCanceled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Options {
    None,
    Params,
    Data,
}
/// Primitives retain JS values, references, getters, coercions and original errors.
/// Array methods use the source's captured-length or live-length iteration as specified.
pub trait Host: 'static {
    type Value: Clone + 'static;
    type Error: 'static;
    type Prepared;
    fn prepare(&mut self, op: Operation) -> Result<Self::Prepared, Self::Error>;
    fn await_element(&mut self, v: Self::Value) -> Task<Self::Value, Self::Error>;
    fn call(&mut self, call: Call) -> Result<Task<Self::Value, Self::Error>, Self::Error>;
    fn recursive_orders(
        &mut self,
        endpoint: &str,
    ) -> Result<Task<Self::Value, Self::Error>, Self::Error>;
    fn delete(
        &mut self,
        call: Self::Prepared,
        endpoint: Self::Value,
        options: Options,
        body: Self::Value,
    ) -> Result<Task<(), Self::Error>, Self::Error>;
    fn field(&mut self, v: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn index(&mut self, v: &Self::Value, i: usize) -> Result<Self::Value, Self::Error>;
    fn length(&mut self, v: &Self::Value) -> Result<usize, Self::Error>;
    fn has_index(&mut self, v: &Self::Value, i: usize) -> Result<bool, Self::Error>;
    fn truthy(&self, v: &Self::Value) -> bool;
    fn is_null(&self, v: &Self::Value) -> bool;
    fn nullish(&self, v: &Self::Value) -> bool;
    fn omit_undefined(&self, v: Self::Value) -> Self::Value;
    fn is_array(&self, v: &Self::Value) -> bool;
    fn same(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn same_zero(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn equals(&self, v: &Self::Value, s: &str) -> bool;
    fn array(&self, values: Vec<Self::Value>) -> Self::Value;
    fn object(&self, fields: Vec<(&'static str, Self::Value)>) -> Self::Value;
    fn undefined(&self) -> Self::Value;
    fn string(&self, s: &str) -> Self::Value;
    fn utf16(&self, s: Vec<u16>) -> Self::Value;
    fn stringify(&mut self, v: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn spread(&mut self, v: &Self::Value) -> Result<Vec<Self::Value>, Self::Error>;
    fn parse_int(
        &mut self,
        call: Self::Prepared,
        v: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn reverse_symbol(
        &mut self,
        call: Self::Prepared,
        v: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn map_limit(
        &mut self,
        call: Self::Prepared,
        v: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn map_algo(
        &mut self,
        call: Self::Prepared,
        v: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn is_algo(&mut self, call: Self::Prepared, v: Self::Value) -> Result<bool, Self::Error>;
    fn stored_orders(&mut self) -> Result<Self::Value, Self::Error>;
    fn place_positional(
        &mut self,
        call: Self::Prepared,
        opts: Self::Value,
    ) -> Result<Task<(), Self::Error>, Self::Error>;
    fn update_canceled(&mut self, call: Self::Prepared, id: Self::Value)
    -> Result<(), Self::Error>;
    fn type_error(&mut self, message: &str) -> Self::Error;
    fn error_field(&mut self, e: &Self::Error, path: &[&str]) -> Result<Self::Value, Self::Error>;
    fn includes(&mut self, v: &Self::Value, text: &str) -> Result<bool, Self::Error>;
    fn emit(
        &mut self,
        call: Self::Prepared,
        original: Option<&Self::Error>,
        message: Self::Value,
    ) -> Result<(), Self::Error>;
}
fn endpoint(k: &str) -> &'static str {
    crate::constants::ENDPOINTS
        .iter()
        .find(|(key, _)| *key == k)
        .expect("legacy endpoint")
        .1
}
fn emit<H: Host>(h: &mut H, e: &H::Error, full: bool) -> Result<(), H::Error> {
    let call = h.prepare(Operation::Emit)?;
    let m = h.error_field(e, &["response", "data", "message"])?;
    let m = if h.truthy(&m) {
        m
    } else {
        h.error_field(e, &["message"])?
    };
    h.emit(call, if full { Some(e) } else { None }, m)
}
// ts: WOOXExchange.fetchOrders
pub fn fetch_orders<H: Host>(host: Rc<RefCell<H>>) -> Task<H::Value, H::Error> {
    let first = host.borrow_mut().call(Call::LimitOrders);
    Box::pin(async move {
        let limit = first?.await?;
        let algo = host.borrow_mut().call(Call::AlgoOrders)?;
        let algo = algo.await?;
        let mut h = host.borrow_mut();
        let mut rows = h.spread(&limit)?;
        rows.extend(h.spread(&algo)?);
        Ok(h.array(rows))
    })
}
fn mapped_orders<H: Host>(host: Rc<RefCell<H>>, algo: bool) -> Task<H::Value, H::Error> {
    let read =
        host.borrow_mut()
            .recursive_orders(endpoint(if algo { "ALGO_ORDERS" } else { "ORDERS" }));
    let read = match read {
        Ok(f) => f,
        Err(e) => {
            let mut h = host.borrow_mut();
            let r = emit(&mut *h, &e, false).map(|()| h.array(vec![]));
            return Box::pin(std::future::ready(r));
        }
    };
    Box::pin(async move {
        let result = async {
            let rows = read.await?;
            let mut h = host.borrow_mut();
            if !h.is_array(&rows) {
                return Err(h.type_error("rows.reduce is not a function"));
            }
            let mut out = vec![];
            let mut i = 0;
            while i < h.length(&rows)? {
                let map = h.prepare(if algo {
                    Operation::MapAlgo
                } else {
                    Operation::MapLimit
                })?;
                let row = h.index(&rows, i)?;
                if algo {
                    let children = h.map_algo(map, row)?;
                    let mut j = 0;
                    while j < h.length(&children)? {
                        out.push(h.index(&children, j)?);
                        j += 1;
                    }
                } else {
                    let order = h.map_limit(map, row)?;
                    if !h.is_null(&order) {
                        out.push(order);
                    }
                }
                i += 1;
            }
            Ok(h.array(out))
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                let mut h = host.borrow_mut();
                emit(&mut *h, &e, false)?;
                Ok(h.array(vec![]))
            }
        }
    })
}
// ts: WOOXExchange.fetchLimitOrders
pub fn fetch_limit_orders<H: Host>(host: Rc<RefCell<H>>) -> Task<H::Value, H::Error> {
    mapped_orders(host, false)
}
// ts: WOOXExchange.fetchAlgoOrders
pub fn fetch_algo_orders<H: Host>(host: Rc<RefCell<H>>) -> Task<H::Value, H::Error> {
    mapped_orders(host, true)
}
// ts: WOOXExchange.cancelOrders
pub fn cancel_orders<H: Host>(host: Rc<RefCell<H>>, orders: H::Value) -> Task<(), H::Error> {
    let filtered = (|| {
        let mut h = host.borrow_mut();
        let n = h.length(&orders)?;
        let mut seen: Vec<H::Value> = vec![];
        let mut out = vec![];
        for i in 0..n {
            if !h.has_index(&orders, i)? {
                continue;
            }
            let o = h.index(&orders, i)?;
            let parent = h.field(&o, "parentId")?;
            if h.truthy(&parent) {
                let parent = h.field(&o, "parentId")?;
                if seen.iter().any(|p| h.same_zero(p, &parent)) {
                    continue;
                }
            }
            let parent = h.field(&o, "parentId")?;
            if !seen.iter().any(|p| h.same_zero(p, &parent)) {
                seen.push(parent);
            }
            out.push(o);
        }
        Ok(out)
    })();
    let orders = match filtered {
        Ok(o) => o,
        Err(e) => return Box::pin(std::future::ready(Err(e))),
    };
    let mut orders = orders.into_iter();
    let first = orders.next().map(|o| host.borrow_mut().await_element(o));
    Box::pin(async move {
        let result = async {
            if let Some(element) = first {
                let o = element.await?;
                let task = cancel_one(&mut *host.borrow_mut(), o)?;
                task.await?;
            }
            for o in orders {
                let element = host.borrow_mut().await_element(o);
                let o = element.await?;
                let task = cancel_one(&mut *host.borrow_mut(), o)?;
                task.await?;
            }
            Ok(())
        }
        .await;
        match result {
            Ok(()) => Ok(()),
            Err(e) => {
                emit(&mut *host.borrow_mut(), &e, true)?;
                Err(e)
            }
        }
    })
}
fn cancel_one<H: Host>(h: &mut H, o: H::Value) -> Result<Task<(), H::Error>, H::Error> {
    let call = h.prepare(Operation::IsAlgo)?;
    let ty = h.field(&o, "type")?;
    if h.is_algo(call, ty)? {
        let call = h.prepare(Operation::Delete)?;
        let parent = h.field(&o, "parentId")?;
        let id = h.stringify(&parent)?;
        let mut url: Vec<u16> = format!("{}/", endpoint("ALGO_ORDER"))
            .encode_utf16()
            .collect();
        url.extend(id);
        let url = h.utf16(url);
        let u = h.undefined();
        h.delete(call, url, Options::None, u)
    } else {
        let call = h.prepare(Operation::Delete)?;
        let parse = h.prepare(Operation::ParseInt)?;
        let id = h.field(&o, "id")?;
        let id = h.parse_int(parse, &id)?;
        let reverse = h.prepare(Operation::ReverseSymbol)?;
        let symbol = h.field(&o, "symbol")?;
        let symbol = h.reverse_symbol(reverse, symbol)?;
        let body = h.object(vec![("order_id", id), ("symbol", symbol)]);
        let body = h.omit_undefined(body);
        let url = h.string(endpoint("CANCEL_ORDER"));
        h.delete(call, url, Options::Data, body)
    }
}
// ts: WOOXExchange.cancelAlgoOrder
pub fn cancel_algo_order<H: Host>(
    host: Rc<RefCell<H>>,
    order: H::Value,
    recreate: bool,
) -> Task<(), H::Error> {
    let prepared = (|| {
        let mut h = host.borrow_mut();
        let rows = h.stored_orders()?;
        let n = h.length(&rows)?;
        let mut sibling = None;
        for i in 0..n {
            let o = h.index(&rows, i)?;
            let parent = h.field(&o, "parentId")?;
            let target = h.field(&order, "parentId")?;
            if h.same(&parent, &target) {
                let id = h.field(&o, "id")?;
                let target = h.field(&order, "id")?;
                if !h.same(&id, &target) {
                    sibling = Some(o);
                    break;
                }
            }
        }
        let parent = h.field(&order, "parentId")?;
        let id = if h.truthy(&parent) {
            parent
        } else {
            h.field(&order, "id")?
        };
        let call = h.prepare(Operation::Delete)?;
        let id = h.stringify(&id)?;
        let mut url: Vec<u16> = format!("{}/", endpoint("ALGO_ORDER"))
            .encode_utf16()
            .collect();
        url.extend(id);
        let url = h.utf16(url);
        let u = h.undefined();
        let task = h.delete(call, url, Options::None, u)?;
        Ok((sibling, task))
    })();
    let prepared = match prepared {
        Ok(v) => v,
        Err(e) => {
            let r = cancel_algo_catch(&mut *host.borrow_mut(), &order, e);
            return Box::pin(std::future::ready(r));
        }
    };
    Box::pin(async move {
        let result = async {
            let (sibling, task) = prepared;
            task.await?;
            if let Some(s) = sibling.filter(|_| recreate) {
                let task = {
                    let mut h = host.borrow_mut();
                    let ty = h.field(&s, "type")?;
                    let stop = h.equals(&ty, "stop_market");
                    let ty = h.field(&s, "type")?;
                    let trigger_stop = h.equals(&ty, "stop_market");
                    let call = h.prepare(Operation::Place)?;
                    let symbol = h.field(&s, "symbol")?;
                    let side = h.field(&s, "side")?;
                    let price = h.field(&s, "price")?;
                    let trigger = h.field(
                        &s,
                        if trigger_stop {
                            "slTriggerType"
                        } else {
                            "tpTriggerType"
                        },
                    )?;
                    let opts = h.object(vec![
                        ("symbol", symbol),
                        ("side", side),
                        (if stop { "stopLoss" } else { "takeProfit" }, price),
                        (
                            if trigger_stop {
                                "slTriggerType"
                            } else {
                                "tpTriggerType"
                            },
                            trigger,
                        ),
                    ]);
                    h.place_positional(call, opts)?
                };
                task.await?;
            }
            Ok(())
        }
        .await;
        match result {
            Ok(()) => Ok(()),
            Err(e) => cancel_algo_catch(&mut *host.borrow_mut(), &order, e),
        }
    })
}
fn cancel_algo_catch<H: Host>(h: &mut H, order: &H::Value, e: H::Error) -> Result<(), H::Error> {
    let message = h.error_field(&e, &["response", "data", "message"])?;
    if !h.nullish(&message) && h.includes(&message, "already canceled")? {
        let call = h.prepare(Operation::UpdateCanceled)?;
        let id = h.field(order, "id")?;
        h.update_canceled(call, id)
    } else {
        emit(h, &e, false)?;
        Err(e)
    }
}
// ts: WOOXExchange.getPositionSideForOrder
pub fn get_position_side_for_order<H: Host>(
    host: Rc<RefCell<H>>,
    opts: H::Value,
) -> Task<H::Value, H::Error> {
    let read = host.borrow_mut().call(Call::PositionMode);
    Box::pin(async move {
        let mode = read?.await?;
        let mut h = host.borrow_mut();
        let reduce = h.field(&opts, "reduceOnly")?;
        if h.equals(&mode, "hedged") {
            let side = h.field(&opts, "side")?;
            let buy = h.equals(&side, "buy");
            Ok(h.string(if buy ^ h.truthy(&reduce) {
                "LONG"
            } else {
                "SHORT"
            }))
        } else {
            Ok(h.undefined())
        }
    })
}
fn cancel_scope<H: Host>(host: Rc<RefCell<H>>, symbol: Option<H::Value>) -> Task<(), H::Error> {
    let read = (|| {
        let mut h = host.borrow_mut();
        let (key, options, body) = if let Some(symbol) = symbol {
            let reverse = h.prepare(Operation::ReverseSymbol)?;
            let symbol = h.reverse_symbol(reverse, symbol)?;
            (
                "CANCEL_SYMBOL_ORDERS",
                Options::Params,
                h.object(vec![("symbol", symbol)]),
            )
        } else {
            ("CANCEL_ORDERS", Options::None, h.undefined())
        };
        let call = h.prepare(Operation::Delete)?;
        let url = h.string(endpoint(key));
        h.delete(call, url, options, body)
    })();
    let read = match read {
        Ok(f) => f,
        Err(e) => {
            let r = emit(&mut *host.borrow_mut(), &e, false);
            return Box::pin(std::future::ready(r));
        }
    };
    Box::pin(async move {
        match read.await {
            Ok(()) => Ok(()),
            Err(e) => emit(&mut *host.borrow_mut(), &e, false),
        }
    })
}
// ts: WOOXExchange.cancelAllOrders
pub fn cancel_all_orders<H: Host>(host: Rc<RefCell<H>>) -> Task<(), H::Error> {
    cancel_scope(host, None)
}
// ts: WOOXExchange.cancelSymbolOrders
pub fn cancel_symbol_orders<H: Host>(host: Rc<RefCell<H>>, symbol: H::Value) -> Task<(), H::Error> {
    cancel_scope(host, Some(symbol))
}
