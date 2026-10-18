//! Spot public ticker event bodies; source symbol names overlap Futures.
use crate::public_ticker_events::{
    Failure, PublicEventArray, TickerField, TickerFields, parse_field, symbol_key,
};

// ts: BinancePublicWebsocket.handleTickerStreamEvents
/// capture snapshots the dictionary handle once, including for an empty array.
/// lookup reads that live dictionary and normalizes all JavaScript-falsy entries to None.
/// get_update captures the current Store method AND receiver before field reads.
/// Decoded JSON data uses stock prototypes; executable hooks, symbol properties,
/// concrete class/Store wiring and getter/lookup behavior beyond these configured
/// callbacks remain pending. Live array length/entries are reread each iteration.
pub fn handle_spot_ticker_events<D, T, U>(
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
        let event = events.event(i)?;
        let key = symbol_key(&event)?;
        if let Some(ticker) = lookup(&tickers_by_symbol, &key)? {
            let update = get_update()?;
            let fields = vec![
                (TickerField::Last, parse_field(&event, "c")?),
                (TickerField::Mark, parse_field(&event, "c")?),
                (TickerField::Index, parse_field(&event, "c")?),
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

// ts: BinancePublicWebsocket.handleBookTickersStreamEvents
/// capture snapshots the dictionary handle once, including for an empty array.
/// lookup reads that live dictionary and normalizes all JavaScript-falsy entries to None.
/// get_update captures the current Store method AND receiver before field reads.
/// Decoded JSON data uses stock prototypes; executable hooks, symbol properties,
/// concrete class/Store wiring and getter/lookup behavior beyond these configured
/// callbacks remain pending. Live array length/entries are reread each iteration.
pub fn handle_spot_book_events<D, T, U>(
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
        let event = events.event(i)?;
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
