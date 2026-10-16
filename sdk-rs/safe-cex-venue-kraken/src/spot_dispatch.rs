//! Spot dispatch bodies preserve opaque source values through host-bound operations.
use safe_cex_exchange::runtime::ExchangeError;
use std::rc::Rc;

pub trait SpotDispatchHost {
    type Message: ?Sized;
    type Item: ?Sized;
    type Symbol: ?Sized;
    type Items: Iterator<Item = Result<Rc<Self::Item>, ExchangeError>>;
    fn array_data(&self, message: &Self::Message) -> Result<Option<Self::Items>, ExchangeError>;
    fn symbol(&self, item: &Self::Item) -> Result<Rc<Self::Symbol>, ExchangeError>;
    fn symbol_truthy(&self, symbol: &Self::Symbol) -> bool;
    fn is_snapshot(&self, message: &Self::Message) -> Result<bool, ExchangeError>;
    fn handle_snapshot(
        &self,
        symbol: Rc<Self::Symbol>,
        item: Rc<Self::Item>,
    ) -> Result<(), ExchangeError>;
    fn handle_delta(
        &self,
        symbol: Rc<Self::Symbol>,
        item: Rc<Self::Item>,
    ) -> Result<(), ExchangeError>;
    fn refresh_balance(&self) -> Result<(), ExchangeError>;
}

pub fn handle_book<H: SpotDispatchHost + ?Sized>(
    host: &H,
    message: &H::Message,
) -> Result<(), ExchangeError> {
    let Some(items) = host.array_data(message)? else {
        return Ok(());
    };
    for item in items {
        let item = item?;
        let symbol = host.symbol(&item)?;
        if !host.symbol_truthy(&symbol) {
            continue;
        }
        if host.is_snapshot(message)? {
            host.handle_snapshot(symbol, item)?;
        } else {
            host.handle_delta(symbol, item)?;
        }
    }
    Ok(())
}

pub fn handle_balances<H: SpotDispatchHost + ?Sized>(
    host: &H,
    message: &H::Message,
) -> Result<(), ExchangeError> {
    if host.array_data(message)?.is_none() {
        return Ok(());
    }
    host.refresh_balance()
}
