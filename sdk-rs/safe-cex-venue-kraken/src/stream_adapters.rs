//! Exchange stream delegators; the host supplies the pending concrete socket factory.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{Candle, OHLCVOptions, OrderBook};
use std::{cell::RefCell, rc::Rc};

pub type BookCallback = Rc<dyn Fn(&OrderBook) -> Result<(), ExchangeError>>;
pub type CandleCallback = Rc<dyn Fn(&Candle) -> Result<(), ExchangeError>>;
pub type Unsubscribe = Rc<dyn Fn() -> Result<(), ExchangeError>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicVenue {
    Futures,
    Spot,
}

pub trait PublicStreams {
    fn connect_and_subscribe(&self) -> Result<(), ExchangeError>;
    fn listen_order_book(
        &self,
        symbol: &str,
        callback: BookCallback,
    ) -> Result<Unsubscribe, ExchangeError>;
    fn listen_ohlcv(
        &self,
        options: &OHLCVOptions,
        callback: CandleCallback,
    ) -> Result<Unsubscribe, ExchangeError>;
}

fn current<S>(slot: &RefCell<Option<Rc<S>>>, member: &str) -> Result<Rc<S>, ExchangeError> {
    slot.borrow().clone().ok_or_else(|| {
        let mut error = ExchangeError::new(format!(
            "Cannot read properties of undefined (reading '{member}')"
        ));
        error.name = "TypeError".into();
        error
    })
}

fn ensure_public<S: PublicStreams, P: ?Sized>(
    slot: &RefCell<Option<Rc<S>>>,
    parent: &P,
    venue: PublicVenue,
    create: impl FnOnce(PublicVenue, &P) -> Result<Rc<S>, ExchangeError>,
) -> Result<(), ExchangeError> {
    let missing = slot.borrow().is_none();
    if missing {
        let socket = create(venue, parent)?;
        *slot.borrow_mut() = Some(socket);
        current(slot, "connectAndSubscribe")?.connect_and_subscribe()?;
    }
    Ok(())
}

pub fn futures_listen_order_book<S: PublicStreams, P: ?Sized>(
    parent: &P,
    slot: &RefCell<Option<Rc<S>>>,
    symbol: &str,
    callback: BookCallback,
    create: impl FnOnce(PublicVenue, &P) -> Result<Rc<S>, ExchangeError>,
) -> Result<Unsubscribe, ExchangeError> {
    ensure_public(slot, parent, PublicVenue::Futures, create)?;
    current(slot, "listenOrderBook")?.listen_order_book(symbol, callback)
}

pub fn spot_listen_order_book<S: PublicStreams, P: ?Sized>(
    parent: &P,
    slot: &RefCell<Option<Rc<S>>>,
    symbol: &str,
    callback: BookCallback,
    create: impl FnOnce(PublicVenue, &P) -> Result<Rc<S>, ExchangeError>,
) -> Result<Unsubscribe, ExchangeError> {
    ensure_public(slot, parent, PublicVenue::Spot, create)?;
    current(slot, "listenOrderBook")?.listen_order_book(symbol, callback)
}

pub fn spot_listen_ohlcv<S: PublicStreams, P: ?Sized>(
    parent: &P,
    slot: &RefCell<Option<Rc<S>>>,
    options: &OHLCVOptions,
    callback: CandleCallback,
    create: impl FnOnce(PublicVenue, &P) -> Result<Rc<S>, ExchangeError>,
) -> Result<Unsubscribe, ExchangeError> {
    ensure_public(slot, parent, PublicVenue::Spot, create)?;
    current(slot, "listenOHLCV")?.listen_ohlcv(options, callback)
}

pub fn futures_listen_ohlcv<O: ?Sized, C: ?Sized>(_options: &O, _callback: &C) -> Unsubscribe {
    Rc::new(|| Ok(()))
}

pub fn futures_handle_ohlc<T: ?Sized>(_message: &T) {}
