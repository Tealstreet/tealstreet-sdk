//! Legacy WOO account, market and ticker read method bodies.
use crate::legacy_metadata::{JsString, Task};
use safe_cex_types::PositionMode;
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    Main,
    Primary,
    Secondary,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    AccountInfo,
    Balance,
    Positions,
    TickersRaw,
    ParseTickers,
    EmitError,
    ObjectError,
    TypeError,
}
/// Host primitives retain JS values/aliases, property/coercion semantics and errors.
/// Object/array construction is pure; prepared calls retain callee and receiver identity.
pub trait Host: 'static {
    type Value: Clone + 'static;
    type Error: 'static;
    type Prepared;
    fn prepare(&mut self, call: Call) -> Result<Self::Prepared, Self::Error>;
    fn invoke(
        &mut self,
        call: Self::Prepared,
    ) -> Result<Task<Self::Value, Self::Error>, Self::Error>;
    fn invoke_parse(
        &mut self,
        call: Self::Prepared,
        rows: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn get(
        &mut self,
        transport: Transport,
        endpoint: &str,
    ) -> Result<Task<Self::Value, Self::Error>, Self::Error>;
    fn field(&mut self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn v(&mut self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn set_field(
        &mut self,
        value: &Self::Value,
        key: &str,
        new: Self::Value,
    ) -> Result<(), Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn undefined(&self) -> Self::Value;
    fn null(&self) -> Self::Value;
    fn number(&self, n: f64) -> Self::Value;
    fn boolean(&self, b: bool) -> Self::Value;
    fn string(&self, s: JsString) -> Self::Value;
    fn object(&self, fields: Vec<(&'static str, Self::Value)>) -> Self::Value;
    fn array(&self, items: Vec<Option<Self::Value>>) -> Self::Value;
    /// Numeric-operator coercion rejects BigInt mixed with the source number literals.
    fn to_number(&mut self, v: &Self::Value) -> Result<f64, Self::Error>;
    fn text(&mut self, v: &Self::Value) -> Result<JsString, Self::Error>;
    fn equals(&self, v: &Self::Value, s: &str) -> bool;
    fn starts_with(&mut self, v: &Self::Value, prefix: &str) -> Result<bool, Self::Error>;
    fn split(&mut self, v: &Self::Value, separator: &str) -> Result<Vec<Self::Value>, Self::Error>;
    fn is_array(&self, v: &Self::Value) -> bool;
    fn length(&mut self, v: &Self::Value, method: &str) -> Result<usize, Self::Error>;
    fn has_index(&mut self, v: &Self::Value, index: usize) -> Result<bool, Self::Error>;
    fn index(&mut self, v: &Self::Value, index: usize) -> Result<Self::Value, Self::Error>;
    fn normalize_symbol(&mut self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn market(&mut self, symbol: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn stored(&mut self, key: &str) -> Result<Self::Value, Self::Error>;
    fn set_position_mode(&mut self, mode: PositionMode) -> Result<(), Self::Error>;
    fn secondary_key(&mut self) -> Result<Self::Value, Self::Error>;
    fn sum_by(&mut self, v: &Self::Value, property: &str) -> Result<Self::Value, Self::Error>;
    fn error(&mut self, call: Self::Prepared, message: JsString) -> Self::Error;
    fn error_field(
        &mut self,
        error: &Self::Error,
        path: &[&str],
    ) -> Result<Self::Value, Self::Error>;
    fn emit(&mut self, call: Self::Prepared, message: Self::Value) -> Result<(), Self::Error>;
}
fn u(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn endpoint(key: &str) -> &'static str {
    crate::constants::ENDPOINTS
        .iter()
        .find(|(k, _)| *k == key)
        .unwrap()
        .1
}
fn call<H: Host>(h: &mut H, kind: Call) -> Result<Task<H::Value, H::Error>, H::Error> {
    let c = h.prepare(kind)?;
    h.invoke(c)
}
fn cents(n: f64) -> f64 {
    safe_cex_num::math_round(n * 100.0) / 100.0
}
fn error_message<H: Host>(
    h: &mut H,
    error: &H::Error,
    paths: &[&[&str]],
    fallback: Option<&str>,
) -> Result<H::Value, H::Error> {
    for (i, path) in paths.iter().enumerate() {
        let value = h.error_field(error, path)?;
        if h.truthy(&value) || i + 1 == paths.len() && fallback.is_none() {
            return Ok(value);
        }
    }
    Ok(fallback.map_or_else(|| h.undefined(), |s| h.string(u(s))))
}
fn emit<H: Host>(h: &mut H, error: &H::Error) -> Result<(), H::Error> {
    let c = h.prepare(Call::EmitError)?;
    let message = error_message(
        h,
        error,
        &[&["response", "data", "message"], &["message"]],
        None,
    )?;
    h.emit(c, message)
}
// ts: WOOXExchange.getAccount
pub fn get_account<H: Host>(host: Rc<RefCell<H>>) -> Task<H::Value, H::Error> {
    let data = call(&mut *host.borrow_mut(), Call::AccountInfo);
    Box::pin(async move {
        let data = data?.await?;
        let mut h = host.borrow_mut();
        let user_id = h.field(&data, "applicationId")?;
        let affiliate = h.field(&data, "referrerID")?;
        let affiliate = if h.truthy(&affiliate) {
            affiliate
        } else {
            h.null()
        };
        Ok(h.object(vec![("userId", user_id), ("affiliateId", affiliate)]))
    })
}
fn balance<H: Host>(h: &mut H, data: H::Value) -> Result<H::Value, H::Error> {
    let mode = h.field(&data, "accountMode")?;
    if !h.equals(&mode, "FUTURES") {
        let constructor = h.prepare(Call::ObjectError)?;
        let mode = h.field(&data, "accountMode")?;
        let mut message = u("WOO X account is in ");
        message.extend(h.text(&mode)?);
        message.extend(u(" mode, only FUTURES mode is supported"));
        return Err(h.error(constructor, message));
    }
    let mode = h.field(&data, "positionMode")?;
    h.set_position_mode(if h.equals(&mode, "ONE_WAY") {
        PositionMode::OneWay
    } else {
        PositionMode::Hedged
    })?;
    let free = h.field(&data, "freeCollateral")?;
    let free = cents(h.to_number(&free)?);
    let total = h.field(&data, "totalAccountValue")?;
    let total = cents(h.to_number(&total)?);
    let collateral = h.field(&data, "totalCollateral")?;
    let free_again = h.field(&data, "freeCollateral")?;
    let used = cents(h.to_number(&collateral)? - h.to_number(&free_again)?);
    let stored = h.stored("balance")?;
    let upnl = h.field(&stored, "upnl")?;
    Ok(h.object(vec![
        ("free", h.number(free)),
        ("total", h.number(total)),
        ("used", h.number(used)),
        ("upnl", upnl),
    ]))
}
// ts: WOOXExchange.fetchBalance
pub fn fetch_balance<H: Host>(host: Rc<RefCell<H>>) -> Task<H::Value, H::Error> {
    let prepared = call(&mut *host.borrow_mut(), Call::AccountInfo);
    let data = match prepared {
        Ok(f) => f,
        Err(e) => {
            let result = (|| {
                let mut h = host.borrow_mut();
                emit(&mut *h, &e)?;
                h.stored("balance")
            })();
            return Box::pin(std::future::ready(result));
        }
    };
    Box::pin(async move {
        let result = data
            .await
            .and_then(|data| balance(&mut *host.borrow_mut(), data));
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                let mut h = host.borrow_mut();
                emit(&mut *h, &e)?;
                h.stored("balance")
            }
        }
    })
}
fn validation_failure<H: Host>(host: Rc<RefCell<H>>, e: H::Error) -> Task<H::Value, H::Error> {
    let result = (|| {
        let mut h = host.borrow_mut();
        emit(&mut *h, &e)?;
        let error = error_message(
            &mut *h,
            &e,
            &[
                &["response", "data", "message"],
                &["response", "data", "msg"],
                &["message"],
            ],
            Some("Invalid API key or secret"),
        )?;
        Ok(h.object(vec![("error", error)]))
    })();
    Box::pin(std::future::ready(result))
}
// ts: WOOXExchange.validateAccount
pub fn validate_account<H: Host>(host: Rc<RefCell<H>>) -> Task<H::Value, H::Error> {
    let prepared = host
        .borrow_mut()
        .get(Transport::Primary, endpoint("ACCOUNT"));
    let first = match prepared {
        Ok(f) => f,
        Err(e) => return validation_failure(host.clone(), e),
    };
    Box::pin(async move {
        let result = async {
            first.await?;
            let secondary = host.borrow_mut().secondary_key()?;
            let enabled = host.borrow().truthy(&secondary);
            if enabled {
                let next = host
                    .borrow_mut()
                    .get(Transport::Secondary, endpoint("ACCOUNT"))?;
                next.await?;
            }
            let next = call(&mut *host.borrow_mut(), Call::Balance)?;
            let fetched = match next.await {
                Ok(v) => v,
                Err(_) => host.borrow().undefined(),
            };
            let h = host.borrow();
            Ok(h.object(vec![("error", h.string(u(""))), ("balance", fetched)]))
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                let mut h = host.borrow_mut();
                emit(&mut *h, &e)?;
                let error = error_message(
                    &mut *h,
                    &e,
                    &[
                        &["response", "data", "message"],
                        &["response", "data", "msg"],
                        &["message"],
                    ],
                    Some("Invalid API key or secret"),
                )?;
                Ok(h.object(vec![("error", error)]))
            }
        }
    })
}
fn markets<H: Host>(h: &mut H, response: H::Value) -> Result<H::Value, H::Error> {
    let data = h.field(&response, "data")?;
    let rows = h.field(&data, "rows")?;
    let length = h.length(&rows, "filter")?;
    let mut selected = vec![];
    for i in 0..length {
        if !h.has_index(&rows, i)? {
            continue;
        }
        let row = h.index(&rows, i)?;
        let symbol = h.field(&row, "symbol")?;
        if h.starts_with(&symbol, "PERP_")? {
            selected.push(row);
        }
    }
    let mut out = vec![];
    for row in selected {
        let symbol = h.field(&row, "symbol")?;
        let parts = h.split(&symbol, "_")?;
        let base = parts.get(1).cloned().unwrap_or_else(|| h.undefined());
        let quote = parts.get(2).cloned().unwrap_or_else(|| h.undefined());
        let imr = h.v(&row, "base_imr")?;
        let imr = if h.truthy(&imr) { imr } else { h.number(0.01) };
        let leverage = (1.0 / h.to_number(&imr)?).floor();
        let mut id = h.text(&base)?;
        id.extend(u("/"));
        id.extend(h.text(&quote)?);
        id.extend(u(":USDT"));
        let mut symbol = h.text(&base)?;
        symbol.extend(h.text(&quote)?);
        let amount = h.v(&row, "base_tick")?;
        let price = h.v(&row, "quote_tick")?;
        let precision = h.object(vec![("amount", amount), ("price", price)]);
        let min = h.v(&row, "base_min")?;
        let max = h.v(&row, "base_max")?;
        let amount = h.object(vec![("min", min), ("max", max)]);
        let lev = h.object(vec![("min", h.number(1.0)), ("max", h.number(leverage))]);
        let limits = h.object(vec![("amount", amount), ("leverage", lev)]);
        out.push(Some(h.object(vec![
            ("id", h.string(id)),
            ("symbol", h.string(symbol)),
            ("base", base),
            ("quote", quote),
            ("active", h.boolean(true)),
            ("precision", precision),
            ("limits", limits),
            ("info", row),
        ])));
    }
    Ok(h.array(out))
}
// ts: WOOXExchange.fetchMarkets
pub fn fetch_markets<H: Host>(host: Rc<RefCell<H>>) -> Task<H::Value, H::Error> {
    let prepared = host.borrow_mut().get(Transport::Main, endpoint("MARKETS"));
    let read = match prepared {
        Ok(f) => f,
        Err(e) => {
            let result = (|| {
                let mut h = host.borrow_mut();
                emit(&mut *h, &e)?;
                h.stored("markets")
            })();
            return Box::pin(std::future::ready(result));
        }
    };
    Box::pin(async move {
        let result = read.await.and_then(|v| markets(&mut *host.borrow_mut(), v));
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                let mut h = host.borrow_mut();
                emit(&mut *h, &e)?;
                h.stored("markets")
            }
        }
    })
}
// ts: WOOXExchange.fetchTickersRaw
pub fn fetch_tickers_raw<H: Host>(host: Rc<RefCell<H>>) -> Task<H::Value, H::Error> {
    let prepared = host.borrow_mut().get(Transport::Main, endpoint("TICKERS"));
    let read = match prepared {
        Ok(f) => f,
        Err(e) => {
            let result = (|| {
                let mut h = host.borrow_mut();
                emit(&mut *h, &e)?;
                Ok(h.null())
            })();
            return Box::pin(std::future::ready(result));
        }
    };
    Box::pin(async move {
        let result = read.await.and_then(|v| {
            let mut h = host.borrow_mut();
            let data = h.field(&v, "data")?;
            h.field(&data, "rows")
        });
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                let mut h = host.borrow_mut();
                emit(&mut *h, &e)?;
                Ok(h.null())
            }
        }
    })
}
fn tickers<H: Host>(h: &mut H, rows: H::Value) -> Result<H::Value, H::Error> {
    if !h.truthy(&rows) {
        return h.stored("tickers");
    }
    if !h.is_array(&rows) {
        let constructor = h.prepare(Call::TypeError)?;
        return Err(h.error(constructor, u("rows.reduce is not a function")));
    }
    let mut out = vec![];
    let mut i = 0;
    while i < h.length(&rows, "length")? {
        let row = h.index(&rows, i)?;
        i += 1;
        let raw = h.field(&row, "symbol")?;
        let symbol = h.normalize_symbol(raw)?;
        let market = h.market(&symbol)?;
        if !h.truthy(&market) {
            continue;
        }
        let open = h.v(&row, "24h_open")?;
        let close = h.v(&row, "24h_close")?;
        let percentage = safe_cex_num::math_round(
            ((h.to_number(&close)? - h.to_number(&open)?) / h.to_number(&open)?) * 100.0 * 100.0,
        ) / 100.0;
        let id = h.field(&market, "id")?;
        let symbol = h.field(&market, "symbol")?;
        let index = h.v(&row, "index_price")?;
        let mark = h.v(&row, "mark_price")?;
        let funding_rate = h.v(&row, "last_funding_rate")?;
        let funding_time = h.v(&row, "next_funding_time")?;
        let volume = h.v(&row, "24h_volume")?;
        let quote_volume = h.v(&row, "24h_amount")?;
        let interest = h.v(&row, "open_interest")?;
        out.push(Some(h.object(vec![
            ("id", id),
            ("symbol", symbol),
            ("bid", close.clone()),
            ("ask", close.clone()),
            ("last", close),
            ("index", index),
            ("mark", mark),
            ("percentage", h.number(percentage)),
            ("fundingRate", funding_rate),
            ("fundingTime", funding_time),
            ("volume", volume),
            ("quoteVolume", quote_volume),
            ("openInterest", interest),
        ])));
    }
    Ok(h.array(out))
}
// ts: WOOXExchange.parseTickers
pub fn parse_tickers<H: Host>(h: &mut H, rows: H::Value) -> Result<H::Value, H::Error> {
    match tickers(h, rows) {
        Ok(v) => Ok(v),
        Err(e) => {
            emit(h, &e)?;
            h.stored("tickers")
        }
    }
}
// ts: WOOXExchange.fetchTickers
pub fn fetch_tickers<H: Host>(host: Rc<RefCell<H>>) -> Task<H::Value, H::Error> {
    let read = call(&mut *host.borrow_mut(), Call::TickersRaw);
    Box::pin(async move {
        let rows = read?.await?;
        let mut h = host.borrow_mut();
        let c = h.prepare(Call::ParseTickers)?;
        h.invoke_parse(c, rows)
    })
}
// ts: WOOXExchange.fetchPositionsAndBalance
pub fn fetch_positions_and_balance<H: Host>(host: Rc<RefCell<H>>) -> Task<H::Value, H::Error> {
    let first = call(&mut *host.borrow_mut(), Call::Balance);
    Box::pin(async move {
        let balance = first?.await?;
        let next = call(&mut *host.borrow_mut(), Call::Positions)?;
        let positions = next.await?;
        let mut h = host.borrow_mut();
        let length = h.length(&positions, "length")?;
        let upnl = if length > 0 {
            let sum = h.sum_by(&positions, "unrealizedPnl")?;
            cents(h.to_number(&sum)?)
        } else {
            0.0
        };
        let upnl = h.number(upnl);
        h.set_field(&balance, "upnl", upnl)?;
        let total = h.field(&balance, "total")?;
        let upnl = h.field(&balance, "upnl")?;
        let total = h.to_number(&total)? - h.to_number(&upnl)?;
        let total = h.number(total);
        h.set_field(&balance, "total", total)?;
        Ok(h.object(vec![("positions", positions), ("balance", balance)]))
    })
}
