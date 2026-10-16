//! Stream handler bodies with opaque values and bound source intrinsics.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::Candle;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lookup {
    Direct,
    GetKey,
}
pub type NumericChanges = Vec<(&'static str, f64)>;

pub trait StreamValues {
    type Value: ?Sized;
    type Items: Iterator<Item = Result<Rc<Self::Value>, ExchangeError>>;
    fn field(
        &self,
        value: &Self::Value,
        key: &'static str,
        lookup: Lookup,
    ) -> Result<Rc<Self::Value>, ExchangeError>;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn parse_float(&self, value: &Self::Value) -> Result<f64, ExchangeError>;
    fn array_data(&self, message: &Self::Value) -> Result<Option<Self::Items>, ExchangeError>;
}
pub trait TickerUpdateHost: StreamValues {
    type Ticker: ?Sized;
    fn ticker_for(&self, symbol: &Self::Value) -> Result<Option<Rc<Self::Ticker>>, ExchangeError>;
    fn update_ticker(
        &self,
        ticker: Rc<Self::Ticker>,
        changes: NumericChanges,
    ) -> Result<(), ExchangeError>;
}
pub trait CandleUpdateHost: StreamValues {
    type Handlers: ?Sized;
    fn strict_number_eq(&self, value: &Self::Value, number: f64) -> bool;
    fn string_units(&self, value: &Self::Value) -> Result<Vec<u16>, ExchangeError>;
    fn handlers(&self, key: &[u16]) -> Result<Rc<Self::Handlers>, ExchangeError>;
    fn handler_length(&self, handlers: &Self::Handlers) -> Result<usize, ExchangeError>;
    fn record_stats(&self, feed: &'static str) -> Result<(), ExchangeError>;
    fn date_ms(&self, value: &Self::Value) -> Result<f64, ExchangeError>;
    fn candle_for_each(
        &self,
        handlers: Rc<Self::Handlers>,
        candle: Rc<RefCell<Candle>>,
    ) -> Result<(), ExchangeError>;
}
fn optional<H: StreamValues + ?Sized>(
    host: &H,
    value: &H::Value,
    key: &'static str,
    lookup: Lookup,
) -> Result<Option<f64>, ExchangeError> {
    let value = host.field(value, key, lookup)?;
    if host.nullish(&value) {
        Ok(None)
    } else {
        host.parse_float(&value).map(Some)
    }
}
fn keyed<H: StreamValues + ?Sized>(
    host: &H,
    value: &H::Value,
    keys: &[&'static str],
) -> Result<Option<f64>, ExchangeError> {
    for key in keys {
        if let Some(number) = optional(host, value, key, Lookup::GetKey)? {
            return Ok(Some(number));
        }
    }
    Ok(None)
}
pub fn futures_handle_ticker<H: TickerUpdateHost + ?Sized>(
    host: &H,
    message: &H::Value,
) -> Result<(), ExchangeError> {
    let first = host.field(message, "product_id", Lookup::GetKey)?;
    let symbol = if host.truthy(&first) {
        first
    } else {
        host.field(message, "symbol", Lookup::GetKey)?
    };
    if !host.truthy(&symbol) {
        return Ok(());
    }
    let Some(ticker) = host.ticker_for(&symbol)? else {
        return Ok(());
    };
    let bid = keyed(host, message, &["bid", "best_bid"])?;
    let ask = keyed(host, message, &["ask", "best_ask"])?;
    let last = keyed(host, message, &["last", "price", "last_trade"])?;
    let mark = keyed(host, message, &["mark", "markPrice"])?;
    let index = keyed(host, message, &["index", "indexPrice"])?;
    let funding_rate = keyed(host, message, &["relative_funding_rate"])?;
    let funding_time = keyed(
        host,
        message,
        &["next_funding_rate_time", "next_funding", "nextFundingTime"],
    )?;
    let open = keyed(host, message, &["open24h", "open_24h"])?;
    let percentage = match (open, last) {
        (Some(open), Some(last)) if open > 0.0 => Some(((last - open) / open) * 100.0),
        _ => None,
    };
    let mut changes = Vec::new();
    for (key, value) in [
        ("bid", bid),
        ("ask", ask),
        ("last", last),
        ("mark", mark),
        ("index", index),
        ("fundingRate", funding_rate),
        ("fundingTime", funding_time),
        ("percentage", percentage),
    ] {
        if let Some(value) = value {
            changes.push((key, value));
        }
    }
    host.update_ticker(ticker, changes)
}
pub fn spot_handle_ticker<H: TickerUpdateHost + ?Sized>(
    host: &H,
    message: &H::Value,
) -> Result<(), ExchangeError> {
    let Some(items) = host.array_data(message)? else {
        return Ok(());
    };
    for item in items {
        let item = item?;
        let symbol = host.field(&item, "symbol", Lookup::Direct)?;
        if !host.truthy(&symbol) {
            continue;
        }
        let Some(ticker) = host.ticker_for(&symbol)? else {
            continue;
        };
        let bid = optional(host, &item, "bid", Lookup::Direct)?;
        let ask = optional(host, &item, "ask", Lookup::Direct)?;
        let last = optional(host, &item, "last", Lookup::Direct)?;
        let volume = optional(host, &item, "volume", Lookup::Direct)?;
        let percentage = optional(host, &item, "change_pct", Lookup::Direct)?;
        let mut changes = Vec::new();
        if let Some(value) = bid {
            changes.push(("bid", value));
        }
        if let Some(value) = ask {
            changes.push(("ask", value));
        }
        if let Some(value) = last {
            changes.extend([("last", value), ("mark", value), ("index", value)]);
        }
        if let Some(value) = volume {
            changes.push(("volume", value));
        }
        if let Some(value) = percentage {
            changes.push(("percentage", value));
        }
        host.update_ticker(ticker, changes)?;
    }
    Ok(())
}
pub fn spot_handle_ohlc<H: CandleUpdateHost + ?Sized>(
    host: &H,
    message: &H::Value,
) -> Result<(), ExchangeError> {
    let Some(items) = host.array_data(message)? else {
        return Ok(());
    };
    for item in items {
        let item = item?;
        let symbol = host.field(&item, "symbol", Lookup::Direct)?;
        let interval = host.field(&item, "interval", Lookup::Direct)?;
        if !host.truthy(&symbol) || !host.truthy(&interval) {
            continue;
        }
        let Some((timeframe, _)) = crate::spot_types::INTERVAL
            .iter()
            .find(|(_, number)| host.strict_number_eq(&interval, *number))
        else {
            continue;
        };
        let mut key = host.string_units(&symbol)?;
        key.extend([58, 58]);
        key.extend(timeframe.encode_utf16());
        let handlers = host.handlers(&key)?;
        if host.handler_length(&handlers)? == 0 {
            continue;
        }
        host.record_stats("ohlc")?;
        let value = host.field(&item, "interval_begin", Lookup::Direct)?;
        let timestamp = host.date_ms(&value)? / 1000.0;
        let number = |key| {
            let value = host.field(&item, key, Lookup::Direct)?;
            host.parse_float(&value)
        };
        let candle = Candle {
            timestamp,
            open: number("open")?,
            high: number("high")?,
            low: number("low")?,
            close: number("close")?,
            volume: number("volume")?,
            last_side: None,
        };
        host.candle_for_each(handlers, Rc::new(RefCell::new(candle)))?;
    }
    Ok(())
}
