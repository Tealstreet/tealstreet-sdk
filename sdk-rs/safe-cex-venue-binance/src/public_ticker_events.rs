//! Binance-family public event bodies over decoded live event arrays.
use crate::private_event_value::{EventOps, EventValue};
use safe_cex_exchange::runtime::ExchangeError;
use std::rc::Rc;
pub type Failure = Rc<ExchangeError>;
pub trait PublicEventArray {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn event(&self, index: usize) -> EventValue;
}
impl PublicEventArray for crate::private_event_value::EventArray {
    fn len(&self) -> usize {
        self.borrow().len()
    }
    fn event(&self, index: usize) -> EventValue {
        self.borrow()
            .get(index)
            .cloned()
            .flatten()
            .unwrap_or(EventValue::Undefined)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickerField {
    Last,
    Mark,
    Index,
    Percentage,
    Volume,
    QuoteVolume,
    FundingRate,
    FundingTime,
    Bid,
    Ask,
}
impl TickerField {
    pub fn name(self) -> &'static str {
        match self {
            Self::Last => "last",
            Self::Mark => "mark",
            Self::Index => "index",
            Self::Percentage => "percentage",
            Self::Volume => "volume",
            Self::QuoteVolume => "quoteVolume",
            Self::FundingRate => "fundingRate",
            Self::FundingTime => "fundingTime",
            Self::Bid => "bid",
            Self::Ask => "ask",
        }
    }
}
/// Ordered source object fields; None/omission is represented by no entry.
/// Native f64 retains NaN/infinity/signed zero until the configured Store boundary.
pub type TickerFields = Vec<(TickerField, f64)>;
pub(crate) fn symbol_key(event: &EventValue) -> Result<String, Failure> {
    event.read("s")?.text()
}
pub(crate) fn parse_field(event: &EventValue, key: &str) -> Result<f64, Failure> {
    crate::private_rest::keyed(event, key)?.float()
}

// ts: BinanceMarketWebsocket.handleTickerStreamEvents
/// Capture the dictionary once and retain live lookups.
/// Capture the Store method and receiver before reading fields.
/// Reread array length and entries after each callback.
pub fn handle_futures_ticker_events<D, T, U>(
    events: &impl PublicEventArray,
    capture: impl FnOnce() -> Result<D, Failure>,
    mut lookup: impl FnMut(&D, &str) -> Result<Option<T>, Failure>,
    mut get_update: impl FnMut() -> Result<U, Failure>,
) -> Result<(), Failure>
where
    U: FnOnce(T, TickerFields) -> Result<(), Failure>,
{
    let tickers_by_symbol = capture()?;
    let mut i = 0;
    while i < events.len() {
        let event = events.event(i);
        let key = symbol_key(&event)?;
        if let Some(ticker) = lookup(&tickers_by_symbol, &key)? {
            let update = get_update()?;
            let fields = vec![
                (TickerField::Last, parse_field(&event, "c")?),
                (TickerField::Percentage, parse_field(&event, "P")?),
                (TickerField::Volume, parse_field(&event, "v")?),
                (TickerField::QuoteVolume, parse_field(&event, "q")?),
            ];
            update(ticker, fields)?;
        }
        i += 1;
    }
    Ok(())
}

// ts: BinanceMarketWebsocket.handleMarkPriceStreamEvents
/// Capture the dictionary once and retain live lookups.
/// Capture the Store method and receiver before reading fields.
/// Reread array length and entries after each callback.
pub fn handle_futures_mark_events<D, T, U>(
    events: &impl PublicEventArray,
    capture: impl FnOnce() -> Result<D, Failure>,
    mut lookup: impl FnMut(&D, &str) -> Result<Option<T>, Failure>,
    mut get_update: impl FnMut() -> Result<U, Failure>,
) -> Result<(), Failure>
where
    U: FnOnce(T, TickerFields) -> Result<(), Failure>,
{
    let tickers_by_symbol = capture()?;
    let mut i = 0;
    while i < events.len() {
        let event = events.event(i);
        let key = symbol_key(&event)?;
        if let Some(ticker) = lookup(&tickers_by_symbol, &key)? {
            let update = get_update()?;
            let fields = vec![
                (TickerField::Mark, parse_field(&event, "p")?),
                (TickerField::Index, parse_field(&event, "i")?),
                (TickerField::FundingRate, parse_field(&event, "r")?),
                (TickerField::FundingTime, parse_field(&event, "T")?),
            ];
            update(ticker, fields)?;
        }
        i += 1;
    }
    Ok(())
}

// ts: BinancePublicWebsocket.handleBookTickersStreamEvents
/// Capture the dictionary once and retain live lookups.
/// Capture the Store method and receiver before reading fields.
/// Reread array length and entries after each callback.
pub fn handle_futures_book_events<D, T, U>(
    events: &impl PublicEventArray,
    capture: impl FnOnce() -> Result<D, Failure>,
    mut lookup: impl FnMut(&D, &str) -> Result<Option<T>, Failure>,
    mut get_update: impl FnMut() -> Result<U, Failure>,
) -> Result<(), Failure>
where
    U: FnOnce(T, TickerFields) -> Result<(), Failure>,
{
    let tickers_by_symbol = capture()?;
    let mut i = 0;
    while i < events.len() {
        let event = events.event(i);
        let key = symbol_key(&event)?;
        if let Some(ticker) = lookup(&tickers_by_symbol, &key)? {
            let update = get_update()?;
            let fields = vec![
                (TickerField::Bid, parse_field(&event, "b")?),
                (TickerField::Ask, parse_field(&event, "a")?),
            ];
            update(ticker, fields)?;
        }
        i += 1;
    }
    Ok(())
}

// ts: BinancePMMarketWebsocket.handleTickerStreamEvents
/// Capture the dictionary once and retain live lookups.
/// Capture the Store method and receiver before reading fields.
/// Reread array length and entries after each callback.
pub fn handle_pm_ticker_events<D, T, U>(
    events: &impl PublicEventArray,
    capture: impl FnOnce() -> Result<D, Failure>,
    mut lookup: impl FnMut(&D, &str) -> Result<Option<T>, Failure>,
    mut get_update: impl FnMut() -> Result<U, Failure>,
) -> Result<(), Failure>
where
    U: FnOnce(T, TickerFields) -> Result<(), Failure>,
{
    let tickers_by_symbol = capture()?;
    let mut i = 0;
    while i < events.len() {
        let event = events.event(i);
        let key = symbol_key(&event)?;
        if let Some(ticker) = lookup(&tickers_by_symbol, &key)? {
            let update = get_update()?;
            let fields = vec![
                (TickerField::Last, parse_field(&event, "c")?),
                (TickerField::Percentage, parse_field(&event, "P")?),
                (TickerField::Volume, parse_field(&event, "v")?),
                (TickerField::QuoteVolume, parse_field(&event, "q")?),
            ];
            update(ticker, fields)?;
        }
        i += 1;
    }
    Ok(())
}

// ts: BinancePMMarketWebsocket.handleMarkPriceStreamEvents
/// Capture the dictionary once and retain live lookups.
/// Capture the Store method and receiver before reading fields.
/// Reread array length and entries after each callback.
pub fn handle_pm_mark_events<D, T, U>(
    events: &impl PublicEventArray,
    capture: impl FnOnce() -> Result<D, Failure>,
    mut lookup: impl FnMut(&D, &str) -> Result<Option<T>, Failure>,
    mut get_update: impl FnMut() -> Result<U, Failure>,
) -> Result<(), Failure>
where
    U: FnOnce(T, TickerFields) -> Result<(), Failure>,
{
    let tickers_by_symbol = capture()?;
    let mut i = 0;
    while i < events.len() {
        let event = events.event(i);
        let key = symbol_key(&event)?;
        if let Some(ticker) = lookup(&tickers_by_symbol, &key)? {
            let update = get_update()?;
            let fields = vec![
                (TickerField::Mark, parse_field(&event, "p")?),
                (TickerField::Index, parse_field(&event, "i")?),
                (TickerField::FundingRate, parse_field(&event, "r")?),
                (TickerField::FundingTime, parse_field(&event, "T")?),
            ];
            update(ticker, fields)?;
        }
        i += 1;
    }
    Ok(())
}

// ts: BinancePMPublicWebsocket.handleBookTickersStreamEvents
/// Capture the dictionary once and retain live lookups.
/// Capture the Store method and receiver before reading fields.
/// Reread array length and entries after each callback.
pub fn handle_pm_book_events<D, T, U>(
    events: &impl PublicEventArray,
    capture: impl FnOnce() -> Result<D, Failure>,
    mut lookup: impl FnMut(&D, &str) -> Result<Option<T>, Failure>,
    mut get_update: impl FnMut() -> Result<U, Failure>,
) -> Result<(), Failure>
where
    U: FnOnce(T, TickerFields) -> Result<(), Failure>,
{
    let tickers_by_symbol = capture()?;
    let mut i = 0;
    while i < events.len() {
        let event = events.event(i);
        let key = symbol_key(&event)?;
        if let Some(ticker) = lookup(&tickers_by_symbol, &key)? {
            let update = get_update()?;
            let fields = vec![
                (TickerField::Bid, parse_field(&event, "b")?),
                (TickerField::Ask, parse_field(&event, "a")?),
            ];
            update(ticker, fields)?;
        }
        i += 1;
    }
    Ok(())
}
