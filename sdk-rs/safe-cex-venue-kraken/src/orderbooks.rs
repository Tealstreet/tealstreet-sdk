//! Order-book method bodies with bound current maps, sort/iteration and callbacks.
use crate::stream_updates::{Lookup, StreamValues};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::OrderBookOrders;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Bids,
    Asks,
}
pub type Level = Rc<RefCell<OrderBookOrders>>;
pub type Levels = Rc<RefCell<Vec<Level>>>;
pub struct Book {
    pub bids: Levels,
    pub asks: Levels,
}

pub trait BookHost: StreamValues {
    type Handlers: ?Sized;
    type Cache: ?Sized;
    type Map: ?Sized;
    fn handlers(&self, symbol: &Self::Value) -> Result<Rc<Self::Handlers>, ExchangeError>;
    fn handler_length(&self, handlers: &Self::Handlers) -> Result<usize, ExchangeError>;
    fn stats(&self, feed: &'static str) -> Result<(), ExchangeError>;
    fn iterable_or_empty(&self, value: Rc<Self::Value>) -> Result<Self::Items, ExchangeError>;
    fn reset_cache(&self, symbol: &Self::Value) -> Result<(), ExchangeError>;
    fn cache(&self, symbol: &Self::Value) -> Result<Option<Rc<Self::Cache>>, ExchangeError>;
    fn required_side(
        &self,
        symbol: &Self::Value,
        side: Side,
    ) -> Result<Rc<Self::Map>, ExchangeError>;
    fn set(&self, map: Rc<Self::Map>, price: f64, quantity: f64) -> Result<(), ExchangeError>;
    fn delete(&self, map: Rc<Self::Map>, price: f64) -> Result<(), ExchangeError>;
    fn is_buy(&self, value: &Self::Value) -> bool;
    fn contract_size_or_one(&self, symbol: &Self::Value) -> Result<Rc<Self::Value>, ExchangeError>;
    fn sorted_entries(
        &self,
        cache: &Self::Cache,
        side: Side,
    ) -> Result<Vec<(f64, f64)>, ExchangeError>;
    fn multiply(&self, amount: f64, contract_size: &Self::Value) -> Result<f64, ExchangeError>;
    fn emit(&self, symbol: Rc<Self::Value>) -> Result<(), ExchangeError>;
    fn for_each(
        &self,
        handlers: Rc<Self::Handlers>,
        build: impl FnMut() -> Rc<RefCell<Book>>,
    ) -> Result<(), ExchangeError>;
}
fn number<H: BookHost + ?Sized>(
    host: &H,
    value: &H::Value,
    key: &'static str,
    lookup: Lookup,
) -> Result<f64, ExchangeError> {
    host.parse_float(&*host.field(value, key, lookup)?)
}
fn captured_levels<H: BookHost + ?Sized>(
    host: &H,
    symbol: &H::Value,
    raw: Rc<H::Value>,
    side: Side,
    snapshot: bool,
) -> Result<(), ExchangeError> {
    for level in host.iterable_or_empty(raw)? {
        let level = level?;
        let price = number(host, &level, "price", Lookup::Direct)?;
        let quantity = number(host, &level, "qty", Lookup::Direct)?;
        if snapshot && quantity <= 0.0 {
            continue;
        }
        if snapshot && quantity.is_nan() {
            continue;
        }
        let cache = host.required_side(symbol, side)?;
        if !snapshot && quantity == 0.0 {
            host.delete(cache, price)?;
        } else {
            host.set(cache, price, quantity)?;
        }
    }
    Ok(())
}
pub fn futures_snapshot<H: BookHost + ?Sized>(
    host: &H,
    message: &H::Value,
) -> Result<(), ExchangeError> {
    let symbol = host.field(message, "product_id", Lookup::GetKey)?;
    if !host.truthy(&symbol) {
        return Ok(());
    }
    let handlers = host.handlers(&symbol)?;
    if host.handler_length(&handlers)? == 0 {
        return Ok(());
    }
    host.stats("book")?;
    let bids = host.field(message, "bids", Lookup::GetKey)?;
    let asks = host.field(message, "asks", Lookup::GetKey)?;
    host.reset_cache(&symbol)?;
    captured_levels(host, &symbol, bids, Side::Bids, true)?;
    captured_levels(host, &symbol, asks, Side::Asks, true)?;
    host.emit(symbol)
}
pub fn spot_snapshot<H: BookHost + ?Sized>(
    host: &H,
    symbol: Rc<H::Value>,
    item: &H::Value,
) -> Result<(), ExchangeError> {
    let handlers = host.handlers(&symbol)?;
    if host.handler_length(&handlers)? == 0 {
        return Ok(());
    }
    host.reset_cache(&symbol)?;
    let bids = host.field(item, "bids", Lookup::Direct)?;
    let asks = host.field(item, "asks", Lookup::Direct)?;
    captured_levels(host, &symbol, bids, Side::Bids, true)?;
    captured_levels(host, &symbol, asks, Side::Asks, true)?;
    host.emit(symbol)
}
pub fn futures_delta<H: BookHost + ?Sized>(
    host: &H,
    message: &H::Value,
) -> Result<(), ExchangeError> {
    let symbol = host.field(message, "product_id", Lookup::GetKey)?;
    if !host.truthy(&symbol) {
        return Ok(());
    }
    let handlers = host.handlers(&symbol)?;
    if host.handler_length(&handlers)? == 0 {
        return Ok(());
    }
    host.stats("book")?;
    if host.cache(&symbol)?.is_none() {
        return Ok(());
    }
    let side = host.field(message, "side", Lookup::GetKey)?;
    let price = number(host, message, "price", Lookup::GetKey)?;
    let quantity = number(host, message, "qty", Lookup::GetKey)?;
    let side = if host.is_buy(&side) {
        Side::Bids
    } else {
        Side::Asks
    };
    let cache = host.required_side(&symbol, side)?;
    if quantity == 0.0 {
        host.delete(cache, price)?;
    } else {
        host.set(cache, price, quantity)?;
    }
    host.emit(symbol)
}
pub fn spot_delta<H: BookHost + ?Sized>(
    host: &H,
    symbol: Rc<H::Value>,
    item: &H::Value,
) -> Result<(), ExchangeError> {
    let handlers = host.handlers(&symbol)?;
    if host.handler_length(&handlers)? == 0 {
        return Ok(());
    }
    host.stats("book")?;
    if host.cache(&symbol)?.is_none() {
        return Ok(());
    }
    let bids = host.field(item, "bids", Lookup::Direct)?;
    let asks = host.field(item, "asks", Lookup::Direct)?;
    captured_levels(host, &symbol, bids, Side::Bids, false)?;
    captured_levels(host, &symbol, asks, Side::Asks, false)?;
    host.emit(symbol)
}
fn emitted_levels<H: BookHost + ?Sized>(
    host: &H,
    cache: &H::Cache,
    side: Side,
    contract: Option<&H::Value>,
) -> Result<Levels, ExchangeError> {
    let entries = host.sorted_entries(cache, side)?;
    let mut levels = Vec::new();
    for (price, amount) in entries.into_iter().take(100) {
        let amount = if let Some(contract) = contract {
            host.multiply(amount, contract)?
        } else {
            amount
        };
        levels.push(Rc::new(RefCell::new(OrderBookOrders {
            price,
            amount,
            total: 0.0,
        })));
    }
    Ok(Rc::new(RefCell::new(levels)))
}
fn emit_book<H: BookHost + ?Sized>(
    host: &H,
    symbol: &H::Value,
    futures: bool,
) -> Result<(), ExchangeError> {
    let handlers = host.handlers(symbol)?;
    if host.handler_length(&handlers)? == 0 {
        return Ok(());
    }
    let Some(cache) = host.cache(symbol)? else {
        return Ok(());
    };
    let contract = if futures {
        Some(host.contract_size_or_one(symbol)?)
    } else {
        None
    };
    let bids = emitted_levels(host, &cache, Side::Bids, contract.as_deref())?;
    let asks = emitted_levels(host, &cache, Side::Asks, contract.as_deref())?;
    host.for_each(handlers, || {
        Rc::new(RefCell::new(Book {
            bids: bids.clone(),
            asks: asks.clone(),
        }))
    })
}
pub fn futures_emit<H: BookHost + ?Sized>(
    host: &H,
    symbol: &H::Value,
) -> Result<(), ExchangeError> {
    emit_book(host, symbol, true)
}
pub fn spot_emit<H: BookHost + ?Sized>(host: &H, symbol: &H::Value) -> Result<(), ExchangeError> {
    emit_book(host, symbol, false)
}
