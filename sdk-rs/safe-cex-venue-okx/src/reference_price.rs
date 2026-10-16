use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickerPriceField {
    Mark,
    Index,
    Last,
}

/// Host reads preserve live getters, reentrancy, and the original read error.
pub trait ReferenceTicker<E> {
    fn price(&self, field: TickerPriceField) -> Result<Option<f64>, E>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceSource {
    Mark,
    Index,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalConfig {
    Absent,
    Present,
    InProcess,
}

// Numeric Ticker fields retain String(number) before parseFloat, including -0.
fn finite_numeric(value: Option<f64>) -> f64 {
    let text = value
        .map(safe_cex_num::js_number_string)
        .unwrap_or_default();
    let parsed = safe_cex_num::parse_float(&text);
    if parsed.is_finite() { parsed } else { 0.0 }
}

fn read_finite<T: ReferenceTicker<E>, E>(
    ticker: Option<&T>,
    field: TickerPriceField,
) -> Result<f64, E> {
    let value = match ticker {
        Some(ticker) => ticker.price(field)?,
        None => None,
    };
    Ok(finite_numeric(value))
}

/// Each host callback evaluates the whole table/key/entry lookup in source order.
pub fn get_ticker<T, E>(
    by_id: impl FnOnce() -> Result<Option<Rc<T>>, E>,
    by_symbol: impl FnOnce() -> Result<Option<Rc<T>>, E>,
) -> Result<Option<Rc<T>>, E> {
    if let Some(ticker) = by_id()? {
        return Ok(Some(ticker));
    }
    by_symbol()
}

pub fn resolve_reference_price<T: ReferenceTicker<E>, E>(
    reference_price: Option<f64>,
    get_ticker: impl FnOnce() -> Result<Option<Rc<T>>, E>,
) -> Result<f64, E> {
    let from_reference = finite_numeric(reference_price);
    if from_reference > 0.0 {
        return Ok(from_reference);
    }
    let ticker = get_ticker()?;
    let from_mark = read_finite(ticker.as_deref(), TickerPriceField::Mark)?;
    if from_mark > 0.0 {
        return Ok(from_mark);
    }
    let from_last = read_finite(ticker.as_deref(), TickerPriceField::Last)?;
    if from_last > 0.0 {
        return Ok(from_last);
    }
    Ok(0.0)
}

pub fn has_external_trigger_reference_price<T: ReferenceTicker<E>, E>(
    config: ExternalConfig,
    source: PriceSource,
    ticker: Option<&T>,
    source_has_price: impl FnOnce(PriceSource) -> Result<Option<bool>, E>,
) -> Result<bool, E> {
    if config == ExternalConfig::InProcess && source_has_price(source)? == Some(true) {
        return Ok(true);
    }
    if config == ExternalConfig::Absent {
        return Ok(false);
    }
    let source_price = read_finite(
        ticker,
        match source {
            PriceSource::Mark => TickerPriceField::Mark,
            PriceSource::Index => TickerPriceField::Index,
        },
    )?;
    let last = read_finite(ticker, TickerPriceField::Last)?;
    Ok(source_price > 0.0 && last > 0.0 && source_price != last)
}

pub fn resolve_trigger_reference_price<T: ReferenceTicker<E>, E>(
    trigger_px_type: Option<&[u16]>,
    fallback_price: Option<f64>,
    is_index_loaded: impl FnOnce() -> Result<bool, E>,
    is_mark_loaded: impl FnOnce() -> Result<bool, E>,
    mut get_ticker: impl FnMut() -> Result<Option<Rc<T>>, E>,
    mut has_external_price: impl FnMut(PriceSource, Option<&T>) -> Result<bool, E>,
) -> Result<f64, E> {
    let ticker = get_ticker()?;
    let fallback = finite_numeric(fallback_price);
    if trigger_px_type.is_some_and(|value| value.iter().copied().eq("index".encode_utf16())) {
        let loaded = is_index_loaded()?;
        if !loaded && !has_external_price(PriceSource::Index, ticker.as_deref())? {
            return Ok(0.0);
        }
        let from_index = read_finite(ticker.as_deref(), TickerPriceField::Index)?;
        return Ok(if from_index > 0.0 { from_index } else { 0.0 });
    }
    if trigger_px_type.is_some_and(|value| value.iter().copied().eq("mark".encode_utf16())) {
        let loaded = is_mark_loaded()?;
        if !loaded && !has_external_price(PriceSource::Mark, ticker.as_deref())? {
            return Ok(0.0);
        }
        let from_mark = read_finite(ticker.as_deref(), TickerPriceField::Mark)?;
        return Ok(if from_mark > 0.0 { from_mark } else { 0.0 });
    }
    let from_last = read_finite(ticker.as_deref(), TickerPriceField::Last)?;
    if from_last > 0.0 {
        return Ok(from_last);
    }
    resolve_reference_price(Some(fallback), get_ticker)
}
