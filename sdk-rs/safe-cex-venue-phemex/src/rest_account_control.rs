//! REST settings control over opaque symbols, markets, contexts and errors.
use safe_cex_num::{math_max, math_min};
use std::future::Future;
pub struct LeverageContext<V> {
    pub margin_type: V,
    pub position_mode: V,
    pub long_leverage: f64,
    pub short_leverage: f64,
}
pub trait ContextHost {
    type Symbol;
    type Row: Clone;
    type Rows;
    type Reply;
    type Call;
    type Value;
    type Error;
    fn ends_usdc(&mut self, symbol: &Self::Symbol) -> Result<bool, Self::Error>;
    fn format_out(&mut self, symbol: &Self::Symbol) -> Result<Self::Value, Self::Error>;
    fn get_target(&mut self) -> Result<Self::Call, Self::Error>;
    fn get(
        &mut self,
        call: Self::Call,
        endpoint: &'static str,
        currency: &'static str,
        symbol: Self::Value,
    ) -> impl Future<Output = Result<Self::Reply, Self::Error>>;
    fn raw_rows(&mut self, reply: Self::Reply) -> Result<Self::Rows, Self::Error>;
    fn stored_rows(&mut self) -> Result<Self::Rows, Self::Error>;
    fn length(&mut self, rows: &Self::Rows) -> Result<usize, Self::Error>;
    fn row(&mut self, rows: &Self::Rows, index: usize) -> Result<Option<Self::Row>, Self::Error>;
    fn matches(
        &mut self,
        row: &Self::Row,
        symbol: &Self::Symbol,
        formatted: bool,
    ) -> Result<bool, Self::Error>;
    fn side_is(
        &mut self,
        row: &Self::Row,
        field: &'static str,
        side: &'static str,
    ) -> Result<bool, Self::Error>;
    fn stored_margin(&mut self, row: &Self::Row) -> Result<Option<Self::Value>, Self::Error>;
    fn stored_mode(&mut self, row: &Self::Row) -> Result<Option<Self::Value>, Self::Error>;
    fn stored_leverage(&mut self, row: &Self::Row) -> Result<f64, Self::Error>;
    fn raw_leverage(&mut self, row: &Self::Row) -> Result<f64, Self::Error>;
    fn cross_margin(&mut self, row: &Self::Row) -> Result<bool, Self::Error>;
    fn raw_hedged(&mut self, row: &Self::Row) -> Result<bool, Self::Error>;
    fn value(&mut self, value: &'static str) -> Result<Self::Value, Self::Error>;
    fn report(&mut self, error: &Self::Error) -> Result<(), Self::Error>;
}
fn endpoint(key: &str) -> &'static str {
    crate::constants::ENDPOINTS
        .iter()
        .find(|(name, _)| *name == key)
        .unwrap()
        .1
}
fn filter_rows<H: ContextHost>(
    host: &mut H,
    rows: H::Rows,
    symbol: &H::Symbol,
    formatted: bool,
) -> Result<Vec<H::Row>, H::Error> {
    let length = host.length(&rows)?;
    let mut filtered = Vec::new();
    for index in 0..length {
        if let Some(row) = host.row(&rows, index)? {
            if host.matches(&row, symbol, formatted)? {
                filtered.push(row);
            }
        }
    }
    Ok(filtered)
}
fn find_side<H: ContextHost>(
    host: &mut H,
    rows: &[H::Row],
    field: &'static str,
    side: &'static str,
) -> Result<H::Row, H::Error> {
    for row in rows {
        if host.side_is(row, field, side)? {
            return Ok(row.clone());
        }
    }
    Ok(rows[0].clone())
}
fn stored_context<H: ContextHost>(
    host: &mut H,
    symbol: &H::Symbol,
) -> Result<Option<LeverageContext<H::Value>>, H::Error> {
    let rows = host.stored_rows()?;
    let rows = filter_rows(host, rows, symbol, false)?;
    let margin = if let Some(row) = rows.first() {
        host.stored_margin(row)?
    } else {
        None
    };
    let mode = if let Some(row) = rows.first() {
        host.stored_mode(row)?
    } else {
        None
    };
    let (Some(margin_type), Some(position_mode)) = (margin, mode) else {
        return Ok(None);
    };
    let long = find_side(host, &rows, "side", "long")?;
    let short = find_side(host, &rows, "side", "short")?;
    Ok(Some(LeverageContext {
        margin_type,
        position_mode,
        long_leverage: host.stored_leverage(&long)?.abs(),
        short_leverage: host.stored_leverage(&short)?.abs(),
    }))
}
pub async fn resolve_leverage_context<H: ContextHost>(
    host: &mut H,
    symbol: H::Symbol,
) -> Result<Option<LeverageContext<H::Value>>, H::Error> {
    let result = async {
        let call = host.get_target()?;
        let currency = if host.ends_usdc(&symbol)? {
            "USDC"
        } else {
            "USDT"
        };
        let formatted = host.format_out(&symbol)?;
        let reply = host
            .get(call, endpoint("POSITIONS"), currency, formatted)
            .await?;
        let rows = host.raw_rows(reply)?;
        let rows = filter_rows(host, rows, &symbol, true)?;
        if rows.is_empty() {
            return stored_context(host, &symbol);
        }
        let long = find_side(host, &rows, "posSide", "Long")?;
        let short = find_side(host, &rows, "posSide", "Short")?;
        let raw = host.raw_leverage(&long)?;
        let cross = raw < 0.0 || host.cross_margin(&long)?;
        let margin_type = host.value(if cross { "cross" } else { "isolated" })?;
        let hedged = host.raw_hedged(&long)?;
        let position_mode = host.value(if hedged { "hedged" } else { "oneway" })?;
        let long = host.raw_leverage(&long)?.abs();
        let short = host.raw_leverage(&short)?.abs();
        Ok(Some(LeverageContext {
            margin_type,
            position_mode,
            long_leverage: if long == 0.0 || long.is_nan() {
                0.0
            } else {
                long
            },
            short_leverage: if short == 0.0 || short.is_nan() {
                0.0
            } else {
                short
            },
        }))
    }
    .await;
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            host.report(&error)?;
            stored_context(host, &symbol)
        }
    }
}
pub trait SettingsHost {
    type Symbol;
    type Value;
    type Market;
    type Context;
    type Reply;
    type Positions;
    type Call;
    type Error;
    fn symbol_truthy(&mut self, symbol: &Self::Symbol) -> Result<bool, Self::Error>;
    fn symbol_text(&mut self, symbol: &Self::Symbol) -> Result<Vec<u16>, Self::Error>;
    fn message(&mut self, text: Vec<u16>) -> Result<Self::Value, Self::Error>;
    fn emit(&mut self, message: Self::Value) -> Result<(), Self::Error>;
    fn error(&mut self, message: Self::Value) -> Result<Self::Error, Self::Error>;
    fn format(&mut self, symbol: &Self::Symbol) -> Result<Self::Value, Self::Error>;
    fn market(&mut self, symbol: &Self::Symbol) -> Result<Option<Self::Market>, Self::Error>;
    fn context(
        &mut self,
        symbol: &Self::Symbol,
    ) -> impl Future<Output = Result<Option<Self::Context>, Self::Error>>;
    fn long(&mut self, context: &Self::Context) -> Result<f64, Self::Error>;
    fn short(&mut self, context: &Self::Context) -> Result<f64, Self::Error>;
    fn cross(&mut self, context: &Self::Context) -> Result<bool, Self::Error>;
    fn hedged(&mut self, context: &Self::Context) -> Result<bool, Self::Error>;
    fn min(&mut self, market: &Self::Market) -> Result<f64, Self::Error>;
    fn max(&mut self, market: &Self::Market) -> Result<f64, Self::Error>;
    fn put_target(&mut self) -> Result<Self::Call, Self::Error>;
    fn put(
        &mut self,
        call: Self::Call,
        endpoint: &'static str,
        symbol: Self::Value,
        mode: Option<&'static str>,
        leverages: Vec<(&'static str, f64)>,
    ) -> impl Future<Output = Result<Self::Reply, Self::Error>>;
    fn zero_code(&mut self, reply: &Self::Reply, optional: bool) -> Result<bool, Self::Error>;
    fn reply_message(
        &mut self,
        reply: &Self::Reply,
        optional: bool,
    ) -> Result<Self::Value, Self::Error>;
    fn fetch_positions(
        &mut self,
        symbol: &Self::Symbol,
    ) -> impl Future<Output = Result<Self::Positions, Self::Error>>;
    fn replace_symbol_positions(
        &mut self,
        symbol: &Self::Symbol,
        positions: Self::Positions,
    ) -> Result<(), Self::Error>;
    fn add_positions(&mut self, positions: Self::Positions) -> Result<(), Self::Error>;
    fn record_mode(
        &mut self,
        symbol: &Self::Symbol,
        context: &Self::Context,
    ) -> Result<(), Self::Error>;
    fn record_leverage(
        &mut self,
        symbol: &Self::Symbol,
        context: &Self::Context,
        side: Option<&Self::Value>,
        applied: f64,
        signed: f64,
    ) -> Result<(), Self::Error>;
    fn side_truthy(&mut self, side: Option<&Self::Value>) -> Result<bool, Self::Error>;
    fn side_long(&mut self, side: Option<&Self::Value>) -> Result<bool, Self::Error>;
    fn helper(
        &mut self,
        symbol: Self::Symbol,
        leverage: f64,
        side: Option<Self::Value>,
        allow: Option<bool>,
    ) -> impl Future<Output = Result<(), Self::Error>>;
    fn report(&mut self, error: &Self::Error) -> Result<(), Self::Error>;
}
fn text(prefix: &str, symbol: &[u16], suffix: &str) -> Vec<u16> {
    prefix
        .encode_utf16()
        .chain(symbol.iter().copied())
        .chain(suffix.encode_utf16())
        .collect()
}
fn fail<H: SettingsHost>(host: &mut H, text: Vec<u16>) -> Result<H::Error, H::Error> {
    let message = host.message(text.clone())?;
    host.emit(message)?;
    let message = host.message(text)?;
    host.error(message)
}
pub async fn change_position_mode<H: SettingsHost>(
    host: &mut H,
    hedged: bool,
    symbol: H::Symbol,
) -> Result<(), H::Error> {
    if !host.symbol_truthy(&symbol)? {
        return Err(fail(host, "Symbol is required".encode_utf16().collect())?);
    }
    let result = async {
        let call = host.put_target()?;
        let formatted = host.format(&symbol)?;
        let reply = host
            .put(
                call,
                endpoint("SET_POSITION_MODE"),
                formatted,
                Some(if hedged { "Hedged" } else { "OneWay" }),
                Vec::new(),
            )
            .await?;
        if !host.zero_code(&reply, false)? {
            let msg = host.reply_message(&reply, false)?;
            host.emit(msg)?;
            let msg = host.reply_message(&reply, false)?;
            return Err(host.error(msg)?);
        }
        let positions = host.fetch_positions(&symbol).await?;
        host.replace_symbol_positions(&symbol, positions)?;
        if let Some(context) = host.context(&symbol).await? {
            host.record_mode(&symbol, &context)?;
        }
        Ok(())
    }
    .await;
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            host.report(&error)?;
            Err(error)
        }
    }
}
pub async fn change_margin_type<H: SettingsHost>(
    host: &mut H,
    isolated: bool,
    symbol: H::Symbol,
) -> Result<(), H::Error> {
    if !host.symbol_truthy(&symbol)? {
        return Err(fail(host, "Symbol is required".encode_utf16().collect())?);
    }
    let Some(context) = host.context(&symbol).await? else {
        let symbol = host.symbol_text(&symbol)?;
        return Err(fail(
            host,
            text(
                "Could not read current leverage settings for ",
                &symbol,
                " — margin type unchanged",
            ),
        )?);
    };
    let leverage = host.long(&context)?;
    host.helper(
        symbol,
        if isolated { leverage } else { -leverage },
        None,
        Some(true),
    )
    .await
}
pub async fn set_leverage<H: SettingsHost>(
    host: &mut H,
    symbol: H::Symbol,
    leverage: f64,
    side: Option<H::Value>,
) -> Result<(), H::Error> {
    host.helper(symbol, leverage, side, None).await
}
fn prep<H: SettingsHost>(
    host: &mut H,
    market: &H::Market,
    context: &H::Context,
    leverage: f64,
    input: f64,
    allow: bool,
) -> Result<f64, H::Error> {
    let min = host.min(market)?;
    let max = host.max(market)?;
    let safe = math_min(math_max(leverage.abs(), min), max);
    let negative = if allow {
        input < 0.0
    } else {
        host.cross(context)?
    };
    Ok(if negative { -safe } else { safe })
}
pub async fn set_leverage_helper<H: SettingsHost>(
    host: &mut H,
    symbol: H::Symbol,
    input: f64,
    side: Option<H::Value>,
    allow: bool,
) -> Result<(), H::Error> {
    let market = host.market(&symbol)?;
    let context = host.context(&symbol).await?;
    let Some(market) = market else {
        let sym = host.symbol_text(&symbol)?;
        return Err(fail(host, text("Market ", &sym, " not found"))?);
    };
    let Some(context) = context else {
        let sym = host.symbol_text(&symbol)?;
        return Err(fail(
            host,
            text(
                "Could not read current leverage settings for ",
                &sym,
                " — leverage unchanged",
            ),
        )?);
    };
    let formatted = host.format(&symbol)?;
    let mut values = Vec::new();
    if host.hedged(&context)? {
        if host.side_truthy(side.as_ref())? {
            if host.side_long(side.as_ref())? {
                values.push((
                    "longLeverageRr",
                    prep(host, &market, &context, input, input, allow)?,
                ));
                let short = host.short(&context)?;
                let short = if short == 0.0 || short.is_nan() {
                    input
                } else {
                    short
                };
                values.push((
                    "shortLeverageRr",
                    prep(host, &market, &context, short, input, allow)?,
                ));
            } else {
                values.push((
                    "shortLeverageRr",
                    prep(host, &market, &context, input, input, allow)?,
                ));
                let long = host.long(&context)?;
                let long = if long == 0.0 || long.is_nan() {
                    input
                } else {
                    long
                };
                values.push((
                    "longLeverageRr",
                    prep(host, &market, &context, long, input, allow)?,
                ));
            }
        } else {
            for key in ["longLeverageRr", "shortLeverageRr"] {
                values.push((key, prep(host, &market, &context, input, input, allow)?));
            }
        }
    } else {
        values.push((
            "leverageRr",
            prep(host, &market, &context, input, input, allow)?,
        ));
    }
    let call = host.put_target()?;
    let reply = host
        .put(call, endpoint("SET_LEVERAGE"), formatted, None, values)
        .await?;
    if !host.zero_code(&reply, true)? {
        let msg = host.reply_message(&reply, true)?;
        host.emit(msg)?;
        let msg = host.reply_message(&reply, true)?;
        return Err(host.error(msg)?);
    }
    let positions = host.fetch_positions(&symbol).await?;
    host.add_positions(positions)?;
    let signed = prep(host, &market, &context, input, input, allow)?;
    host.record_leverage(&symbol, &context, side.as_ref(), signed.abs(), signed)?;
    Ok(())
}

#[cfg(test)]
mod accepted_native_contract {
    use super::*;

    #[test]
    fn endpoint_lookup_preserves_selected_wire_paths() {
        assert_eq!(endpoint("ACCOUNT"), "/g-accounts/accountPositions");
        assert_eq!(endpoint("SET_LEVERAGE"), "/g-positions/leverage");
        assert_eq!(
            endpoint("SET_POSITION_MODE"),
            "/g-positions/switch-pos-mode-sync"
        );
        assert!(std::panic::catch_unwind(|| endpoint("not-an-endpoint")).is_err());
    }
}

#[cfg(test)]
mod accepted_outbound_contract {
    #[test]
    fn text_keeps_raw_symbol_units_between_utf16_affixes() {
        assert_eq!(
            super::text("A🚀", &[0xd800, 0, 0xdfff], "é"),
            [65, 0xd83d, 0xde80, 0xd800, 0, 0xdfff, 233]
        );
        assert_eq!(super::text("", &[], ""), Vec::<u16>::new());
    }
}
