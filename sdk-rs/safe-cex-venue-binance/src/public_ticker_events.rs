//! Binance-family public event bodies over decoded live event arrays.
use crate::private_event_value::EventOps;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::get_key::{GetKeyUnknownValue, v_unknown};
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};
pub type Failure = Rc<ExchangeError>;
/// None is an undefined event; an Rc cell retains decoded JSON event identity.
/// Own undefined-valued properties and non-JSON mutation/coercion hooks remain
/// outside this decoded data boundary.
pub type PublicEvent = Option<Rc<RefCell<Value>>>;
pub type PublicEvents = Rc<RefCell<Vec<PublicEvent>>>;
/// Common lossless frame access for standalone JSON adapters and composed owners.
#[derive(Clone)]
pub enum PublicPayload {
    Json(PublicEvent),
    Native(crate::private_event_value::EventValue),
}
pub trait PublicEventArray {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn event(&self, index: usize) -> PublicPayload;
}
impl PublicEventArray for PublicEvents {
    fn len(&self) -> usize {
        self.borrow().len()
    }
    fn event(&self, index: usize) -> PublicPayload {
        PublicPayload::Json(self.borrow().get(index).cloned().unwrap_or(None))
    }
}
impl PublicEventArray for crate::private_event_value::EventArray {
    fn len(&self) -> usize {
        self.borrow().len()
    }
    fn event(&self, index: usize) -> PublicPayload {
        PublicPayload::Native(
            self.borrow()
                .get(index)
                .cloned()
                .flatten()
                .unwrap_or(crate::private_event_value::EventValue::Undefined),
        )
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
fn type_error(message: impl Into<String>) -> Failure {
    let mut e = ExchangeError::new(message);
    e.name = "TypeError".into();
    Rc::new(e)
}
fn json_symbol_key(event: &PublicEvent) -> Result<String, Failure> {
    let Some(event) = event else {
        return Err(type_error(
            "Cannot read properties of undefined (reading 's')",
        ));
    };
    let data = event.borrow();
    if data.is_null() {
        return Err(type_error("Cannot read properties of null (reading 's')"));
    }
    let symbol = data.as_object().and_then(|fields| fields.get("s"));
    safe_cex_num::js_string(symbol).map_err(|e| type_error(e.message))
}
fn json_parse_field(event: &PublicEvent, key: &str) -> Result<f64, Failure> {
    let data = event.as_ref().map(|event| event.borrow());
    let value = v_unknown(data.as_deref(), key).map_err(|e| type_error(e.message))?;
    let text = match value {
        GetKeyUnknownValue::Undefined => "undefined".into(),
        GetKeyUnknownValue::Json(value) => {
            safe_cex_num::js_string(Some(value)).map_err(|e| type_error(e.message))?
        }
        GetKeyUnknownValue::Length(value) => value.to_string(),
        GetKeyUnknownValue::ObjectInherited(member) => member.js_string().into(),
        GetKeyUnknownValue::ArrayInherited(member) => member.js_string(),
        GetKeyUnknownValue::ArrayPrototype => String::new(),
    };
    Ok(safe_cex_num::parse_float(&text))
}

pub(crate) fn symbol_key(event: &PublicPayload) -> Result<String, Failure> {
    match event {
        PublicPayload::Json(v) => json_symbol_key(v),
        PublicPayload::Native(v) => v.read("s")?.text(),
    }
}
pub(crate) fn parse_field(event: &PublicPayload, key: &str) -> Result<f64, Failure> {
    match event {
        PublicPayload::Json(v) => json_parse_field(v, key),
        PublicPayload::Native(v) => crate::private_rest::keyed(v, key)?.float(),
    }
}

// ts: BinanceMarketWebsocket.handleTickerStreamEvents
/// capture snapshots the dictionary handle once, including for an empty array.
/// lookup reads that live dictionary and normalizes all JavaScript-falsy entries to None.
/// get_update captures the current Store method AND receiver before field reads.
/// Decoded JSON data uses stock prototypes; executable hooks, symbol properties,
/// concrete class/Store wiring and getter/lookup behavior beyond these configured
/// callbacks remain pending. Live array length/entries are reread each iteration.
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
/// capture snapshots the dictionary handle once, including for an empty array.
/// lookup reads that live dictionary and normalizes all JavaScript-falsy entries to None.
/// get_update captures the current Store method AND receiver before field reads.
/// Decoded JSON data uses stock prototypes; executable hooks, symbol properties,
/// concrete class/Store wiring and getter/lookup behavior beyond these configured
/// callbacks remain pending. Live array length/entries are reread each iteration.
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
/// capture snapshots the dictionary handle once, including for an empty array.
/// lookup reads that live dictionary and normalizes all JavaScript-falsy entries to None.
/// get_update captures the current Store method AND receiver before field reads.
/// Decoded JSON data uses stock prototypes; executable hooks, symbol properties,
/// concrete class/Store wiring and getter/lookup behavior beyond these configured
/// callbacks remain pending. Live array length/entries are reread each iteration.
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
/// capture snapshots the dictionary handle once, including for an empty array.
/// lookup reads that live dictionary and normalizes all JavaScript-falsy entries to None.
/// get_update captures the current Store method AND receiver before field reads.
/// Decoded JSON data uses stock prototypes; executable hooks, symbol properties,
/// concrete class/Store wiring and getter/lookup behavior beyond these configured
/// callbacks remain pending. Live array length/entries are reread each iteration.
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
/// capture snapshots the dictionary handle once, including for an empty array.
/// lookup reads that live dictionary and normalizes all JavaScript-falsy entries to None.
/// get_update captures the current Store method AND receiver before field reads.
/// Decoded JSON data uses stock prototypes; executable hooks, symbol properties,
/// concrete class/Store wiring and getter/lookup behavior beyond these configured
/// callbacks remain pending. Live array length/entries are reread each iteration.
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
/// capture snapshots the dictionary handle once, including for an empty array.
/// lookup reads that live dictionary and normalizes all JavaScript-falsy entries to None.
/// get_update captures the current Store method AND receiver before field reads.
/// Decoded JSON data uses stock prototypes; executable hooks, symbol properties,
/// concrete class/Store wiring and getter/lookup behavior beyond these configured
/// callbacks remain pending. Live array length/entries are reread each iteration.
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
