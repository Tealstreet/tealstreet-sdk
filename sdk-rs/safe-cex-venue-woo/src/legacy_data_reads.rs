//! Legacy WOO funding, candle, touch-price and position read method bodies.
use crate::legacy_metadata::Task;
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    Main,
    Proxy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binding {
    Get(Transport),
    RefreshFunding,
    PositionMode,
    OpenPositions,
    Metadata,
    MetadataSymbol,
    ParseFunding,
    Normalize,
    Reverse,
    CandleStart,
    Now,
    ListenCandle,
    ListenBook,
    Emit,
    UpdateTicker,
    PositionId,
    SetInterval,
    Flatten,
}
/// Primitives preserve JS values/getters, aliases, operator coercions and captured callees.
/// Array access uses standard methods; iterator primitives retain live source iteration.
pub trait Host: 'static {
    type Value: Clone + 'static;
    type Error: 'static;
    type Prepared;
    type Iterator;
    fn prepare(&mut self, binding: Binding) -> Result<Self::Prepared, Self::Error>;
    fn call(
        &mut self,
        call: Self::Prepared,
        args: Vec<Self::Value>,
    ) -> Result<Task<Self::Value, Self::Error>, Self::Error>;
    fn invoke(
        &mut self,
        call: Self::Prepared,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn get(
        &mut self,
        call: Self::Prepared,
        url: Self::Value,
        params: Option<Self::Value>,
    ) -> Result<Task<Self::Value, Self::Error>, Self::Error>;
    fn root(&mut self, key: &str) -> Result<Self::Value, Self::Error>;
    fn set_root(&mut self, key: &str, v: Self::Value) -> Result<(), Self::Error>;
    fn field(&mut self, v: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn key_field(&mut self, v: &Self::Value, key: Vec<u16>) -> Result<Self::Value, Self::Error>;
    fn index(&mut self, v: &Self::Value, i: usize) -> Result<Self::Value, Self::Error>;
    fn length(&mut self, v: &Self::Value) -> Result<usize, Self::Error>;
    fn has_index(&mut self, v: &Self::Value, i: usize) -> Result<bool, Self::Error>;
    fn iterator(&mut self, v: Self::Value) -> Result<Self::Iterator, Self::Error>;
    fn next(&mut self, i: &mut Self::Iterator) -> Result<Option<Self::Value>, Self::Error>;
    fn close_iterator(&mut self, i: &mut Self::Iterator) -> Result<(), Self::Error>;
    fn await_element(&mut self, v: Self::Value) -> Task<Self::Value, Self::Error>;
    fn is_undefined(&self, v: &Self::Value) -> bool;
    fn truthy(&self, v: &Self::Value) -> bool;
    fn nullish(&self, v: &Self::Value) -> bool;
    fn is_object(&self, v: &Self::Value) -> bool;
    fn is_string(&self, v: &Self::Value) -> bool;
    fn is_array(&self, v: &Self::Value) -> bool;
    fn strict_true(&self, v: &Self::Value) -> bool;
    fn equals(&self, v: &Self::Value, s: &str) -> bool;
    fn same(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn same_zero(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn number_coerce(&mut self, v: &Self::Value) -> Result<f64, Self::Error>;
    fn operator_number(&mut self, v: &Self::Value) -> Result<f64, Self::Error>;
    fn text(&mut self, v: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn starts_with(&mut self, v: &Self::Value, s: &str) -> Result<bool, Self::Error>;
    fn number(&self, n: f64) -> Self::Value;
    fn boolean(&self, b: bool) -> Self::Value;
    fn undefined(&self) -> Self::Value;
    fn string(&self, s: Vec<u16>) -> Self::Value;
    fn object(&self, fields: Vec<(&'static str, Self::Value)>) -> Self::Value;
    fn object_spread(
        &mut self,
        before: Vec<(&'static str, Self::Value)>,
        v: Self::Value,
        after: Vec<(&'static str, Self::Value)>,
    ) -> Result<Self::Value, Self::Error>;
    fn array(&self, v: Vec<Option<Self::Value>>) -> Self::Value;
    fn type_error(&mut self, message: &str) -> Self::Error;
    fn error(&mut self, message: &str) -> Self::Error;
    fn error_field(&mut self, e: &Self::Error, path: &[&str]) -> Result<Self::Value, Self::Error>;
    fn emit(
        &mut self,
        call: Self::Prepared,
        e: Option<&Self::Error>,
        message: Self::Value,
    ) -> Result<(), Self::Error>;
    fn get_callable(&mut self) -> Result<bool, Self::Error>;
    fn interval(
        &mut self,
        call: Self::Prepared,
        delay: Self::Value,
        callback: Rc<dyn Fn() -> Result<Task<Self::Value, Self::Error>, Self::Error>>,
    ) -> Result<Self::Value, Self::Error>;
}
fn s<H: Host>(h: &H, v: &str) -> H::Value {
    h.string(v.encode_utf16().collect())
}
fn ep(k: &str) -> &'static str {
    crate::constants::ENDPOINTS
        .iter()
        .find(|(key, _)| *key == k)
        .expect("legacy endpoint")
        .1
}
fn disposed<H: Host>(h: &mut H) -> Result<bool, H::Error> {
    let v = h.root("isDisposed")?;
    Ok(h.truthy(&v))
}
fn invoke<H: Host>(h: &mut H, b: Binding, args: Vec<H::Value>) -> Result<H::Value, H::Error> {
    let c = h.prepare(b)?;
    h.invoke(c, args)
}
fn call<H: Host>(
    h: &mut H,
    b: Binding,
    args: Vec<H::Value>,
) -> Result<Task<H::Value, H::Error>, H::Error> {
    let c = h.prepare(b)?;
    h.call(c, args)
}
fn optional<H: Host>(h: &mut H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        h.field(v, k)
    }
}
fn emit<H: Host>(h: &mut H, e: &H::Error, full: bool, msg: &str) -> Result<(), H::Error> {
    let c = h.prepare(Binding::Emit)?;
    let v = h.error_field(e, &["response", "data", msg])?;
    let v = if h.truthy(&v) {
        v
    } else {
        h.error_field(e, &["message"])?
    };
    h.emit(c, if full { Some(e) } else { None }, v)
}
// ts: WOOXExchange#startFundingRatesRefresh
pub fn start_funding_rates_refresh<H: Host>(host: Rc<RefCell<H>>) -> Result<(), H::Error> {
    let mut h = host.borrow_mut();
    if disposed(&mut *h)? {
        return Ok(());
    }
    let timer = h.root("_fundingRefreshTimer")?;
    if !h.is_undefined(&timer) {
        return Ok(());
    }
    let c = h.prepare(Binding::SetInterval)?;
    let delay = h.root("FUNDING_REFRESH_INTERVAL_MS")?;
    let clone = host.clone();
    let timer = h.interval(
        c,
        delay,
        Rc::new(move || call(&mut *clone.borrow_mut(), Binding::RefreshFunding, vec![])),
    )?;
    h.set_root("_fundingRefreshTimer", timer)
}
// ts: WOOXExchange#refreshFundingRates
pub fn refresh_funding_rates<H: Host>(host: Rc<RefCell<H>>) -> Task<H::Value, H::Error> {
    let guard = (|| {
        let mut h = host.borrow_mut();
        if disposed(&mut *h)? {
            return Ok(false);
        }
        let flight = h.root("_fundingRefreshInFlight")?;
        if h.truthy(&flight) {
            return Ok(false);
        }
        let yes = h.boolean(true);
        h.set_root("_fundingRefreshInFlight", yes)?;
        Ok(true)
    })();
    match guard {
        Ok(false) => return Box::pin(std::future::ready(Ok(host.borrow().undefined()))),
        Err(e) => return Box::pin(std::future::ready(Err(e))),
        Ok(true) => {}
    }
    let first = (|| {
        let mut h = host.borrow_mut();
        let c = h.prepare(Binding::Get(Transport::Main))?;
        let url = s(&*h, ep("FUNDING_RATES"));
        h.get(c, url, None).map(Some)
    })();
    let read = match first {
        Ok(None) => return Box::pin(std::future::ready(Ok(host.borrow().undefined()))),
        Ok(Some(f)) => f,
        Err(e) => {
            let result = funding_finish(&mut *host.borrow_mut(), Err(e));
            return Box::pin(std::future::ready(result));
        }
    };
    Box::pin(async move {
        let result = async {
            let response = read.await?;
            let mut h = host.borrow_mut();
            let data = h.field(&response, "data")?;
            if disposed(&mut *h)? {
                return Ok(h.undefined());
            }
            let valid = if !h.truthy(&data) || !h.is_object(&data) {
                false
            } else {
                let success = h.field(&data, "success")?;
                if !h.strict_true(&success) {
                    false
                } else {
                    let rows = h.field(&data, "rows")?;
                    h.is_array(&rows)
                }
            };
            if !valid {
                return Err(h.error("WOO X funding rates response has an invalid shape"));
            }
            let rows = h.field(&data, "rows")?;
            let mut rows = h.iterator(rows)?;
            while let Some(row) = h.next(&mut rows)? {
                match funding_row(&mut *h, row) {
                    Ok(true) => {
                        h.close_iterator(&mut rows)?;
                        return Ok(h.undefined());
                    }
                    Ok(false) => {}
                    Err(e) => {
                        // JS IteratorClose preserves an existing throw over a close failure.
                        match h.close_iterator(&mut rows) {
                            Ok(()) | Err(_) => return Err(e),
                        }
                    }
                }
            }
            Ok(h.undefined())
        }
        .await;
        funding_finish(&mut *host.borrow_mut(), result)
    })
}
fn funding_row<H: Host>(h: &mut H, row: H::Value) -> Result<bool, H::Error> {
    if disposed(h)? {
        return Ok(true);
    }
    if !h.truthy(&row) || !h.is_object(&row) {
        return Ok(false);
    }
    let symbol = h.field(&row, "symbol")?;
    let rate = h.field(&row, "est_funding_rate")?;
    let time = h.field(&row, "next_funding_time")?;
    if !h.is_string(&symbol) || !h.starts_with(&symbol, "PERP_")? {
        return Ok(false);
    }
    let rate = invoke(h, Binding::ParseFunding, vec![rate])?;
    let time = invoke(h, Binding::ParseFunding, vec![time])?;
    if h.is_undefined(&rate) || h.is_undefined(&time) || h.operator_number(&time)? <= 0.0 {
        return Ok(false);
    }
    let tickers = h.root("tickersBySymbol")?;
    let normalized = invoke(h, Binding::Normalize, vec![symbol.clone()])?;
    let key = h.text(&normalized)?;
    let ticker = h.key_field(&tickers, key)?;
    if !h.truthy(&ticker) {
        return Ok(false);
    }
    let c = h.prepare(Binding::Reverse)?;
    let ts = h.field(&ticker, "symbol")?;
    let reversed = h.invoke(c, vec![ts])?;
    if !h.same(&symbol, &reversed) {
        return Ok(false);
    }
    if disposed(h)? {
        return Ok(true);
    }
    let c = h.prepare(Binding::UpdateTicker)?;
    let patch = h.object(vec![("fundingRate", rate), ("fundingTime", time)]);
    h.invoke(c, vec![ticker, patch])?;
    Ok(false)
}
fn funding_finish<H: Host>(
    h: &mut H,
    result: Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    let result = match result {
        Ok(v) => Ok(v),
        Err(e) => match disposed(h) {
            Ok(false) => emit(h, &e, false, "message").map(|()| h.undefined()),
            Ok(true) => Ok(h.undefined()),
            Err(e) => Err(e),
        },
    };
    let no = h.boolean(false);
    h.set_root("_fundingRefreshInFlight", no)?;
    result
}
// ts: WOOXExchange#fetchMarketIocTouchPrice
pub fn fetch_market_ioc_touch_price<H: Host>(
    host: Rc<RefCell<H>>,
    symbol: H::Value,
    side: H::Value,
) -> Task<H::Value, H::Error> {
    let available = host.borrow_mut().get_callable();
    match available {
        Ok(false) => return Box::pin(std::future::ready(Ok(host.borrow().undefined()))),
        Err(e) => return Box::pin(std::future::ready(Err(e))),
        Ok(true) => {}
    }
    let first = (|| {
        let mut h = host.borrow_mut();
        let c = h.prepare(Binding::Get(Transport::Main))?;
        let reversed = invoke(&mut *h, Binding::Reverse, vec![symbol])?;
        let mut url: Vec<u16> = format!("{}/", ep("ORDERBOOK")).encode_utf16().collect();
        url.extend(h.text(&reversed)?);
        let url = h.string(url);
        h.get(c, url, None).map(Some)
    })();
    let read = match first {
        Ok(None) => return Box::pin(std::future::ready(Ok(host.borrow().undefined()))),
        Ok(Some(v)) => v,
        Err(e) => {
            let emitted = emit(&mut *host.borrow_mut(), &e, false, "message");
            let result = emitted.map(|()| host.borrow().undefined());
            return Box::pin(std::future::ready(result));
        }
    };
    Box::pin(async move {
        let result = async {
            let response = read.await?;
            let mut h = host.borrow_mut();
            let data = h.field(&response, "data")?;
            let key = if h.equals(&side, "buy") {
                "asks"
            } else {
                "bids"
            };
            let levels = optional(&mut *h, &data, key)?;
            let levels = if h.truthy(&levels) {
                levels
            } else {
                h.array(vec![])
            };
            let mut it = h.iterator(levels)?;
            let first = h.next(&mut it)?;
            if first.is_some() {
                h.close_iterator(&mut it)?;
            }
            let first = first.unwrap_or_else(|| h.undefined());
            let price = optional(&mut *h, &first, "price")?;
            let price = h.number_coerce(&price)?;
            Ok(if price.is_finite() && price > 0.0 {
                h.number(price)
            } else {
                h.undefined()
            })
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                let mut h = host.borrow_mut();
                emit(&mut *h, &e, false, "message")?;
                Ok(h.undefined())
            }
        }
    })
}
// ts: WOOXExchange.fetchOHLCV
pub fn fetch_ohlcv<H: Host>(host: Rc<RefCell<H>>, opts: H::Value) -> Task<H::Value, H::Error> {
    let first = (|| {
        let mut h = host.borrow_mut();
        let c = h.prepare(Binding::CandleStart)?;
        let interval = h.field(&opts, "interval")?;
        let now = invoke(&mut *h, Binding::Now, vec![])?;
        let now = h.operator_number(&now)? / 1000.0;
        let now = h.number(now);
        let start = h.invoke(c, vec![interval, now])?;
        let interval = h.field(&opts, "interval")?;
        let seconds = h.root("timeframeToSeconds")?;
        let key = h.text(&interval)?;
        let seconds = h.key_field(&seconds, key)?;
        let seconds = if h.truthy(&seconds) {
            seconds
        } else {
            h.number(f64::INFINITY)
        };
        let to = h.field(&opts, "to")?;
        let to = if h.truthy(&to) { to } else { h.number(0.0) };
        let to = h.operator_number(&to)? / 1000.0;
        let distance = (h.operator_number(&start)? - to).abs() / h.operator_number(&seconds)?;
        let recent = distance < 100.0;
        let c = h.prepare(Binding::Get(if recent {
            Transport::Main
        } else {
            Transport::Proxy
        }))?;
        let c_reverse = h.prepare(Binding::Reverse)?;
        let symbol = h.field(&opts, "symbol")?;
        let symbol = h.invoke(c_reverse, vec![symbol])?;
        let ty = h.field(&opts, "interval")?;
        let mut params = vec![("symbol", symbol), ("type", ty)];
        if !recent {
            params.push(("start_time", h.field(&opts, "from")?));
            params.push(("end_time", h.field(&opts, "to")?));
        }
        let limit = h.field(&opts, "limit")?;
        let limit = if h.truthy(&limit) {
            limit
        } else {
            h.number(500.0)
        };
        let limit = h.operator_number(&limit)?;
        let limit = h.number(if limit.is_nan() {
            f64::NAN
        } else {
            limit.min(1000.0)
        });
        params.push((if recent { "limit" } else { "size" }, limit));
        let params = h.object(params);
        let url = s(&*h, ep(if recent { "KLINE" } else { "KLINE_HISTORY" }));
        let read = h.get(c, url, Some(params))?;
        Ok((recent, read))
    })();
    Box::pin(async move {
        let (recent, read) = first?;
        let response = read.await?;
        let mut h = host.borrow_mut();
        let data = h.field(&response, "data")?;
        let data = if recent {
            data
        } else {
            h.field(&data, "data")?
        };
        let rows = h.field(&data, "rows")?;
        if h.nullish(&rows) {
            let message = if h.is_undefined(&rows) {
                "Cannot read properties of undefined (reading 'map')"
            } else {
                "Cannot read properties of null (reading 'map')"
            };
            return Err(h.type_error(message));
        }
        if !h.is_array(&rows) {
            return Err(h.type_error("rows.map is not a function"));
        }
        let n = h.length(&rows)?;
        let mut candles = vec![];
        for i in 0..n {
            if !h.has_index(&rows, i)? {
                candles.push(None);
                continue;
            }
            let r = h.index(&rows, i)?;
            let timestamp = h.field(&r, "start_timestamp")?;
            let timestamp = h.operator_number(&timestamp)? / 1000.0;
            let mut fields = vec![("timestamp", h.number(timestamp))];
            for k in ["open", "high", "low", "close", "volume"] {
                fields.push((k, h.field(&r, k)?));
            }
            candles.push(Some(h.object(fields)));
        }
        candles.reverse();
        Ok(h.array(candles))
    })
}
// ts: WOOXExchange.listenOHLCV
pub fn listen_ohlcv<H: Host>(
    h: &mut H,
    opts: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    invoke(h, Binding::ListenCandle, vec![opts, callback])
}
// ts: WOOXExchange.listenOrderBook
pub fn listen_order_book<H: Host>(
    h: &mut H,
    symbol: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    invoke(h, Binding::ListenBook, vec![symbol, callback])
}
// ts: WOOXExchange.fetchPositionMetadata
pub fn fetch_position_metadata<H: Host>(
    host: Rc<RefCell<H>>,
    symbols: H::Value,
) -> Task<H::Value, H::Error> {
    let first = call(&mut *host.borrow_mut(), Binding::PositionMode, vec![]);
    let first = match first {
        Ok(f) => f,
        Err(e) => {
            let mut h = host.borrow_mut();
            let result = emit(&mut *h, &e, true, "msg").map(|()| h.array(vec![]));
            return Box::pin(std::future::ready(result));
        }
    };
    Box::pin(async move {
        let result = async {
            let mode = first.await?;
            let mut i = 0;
            let mut metadata: Vec<Option<H::Value>> = vec![];
            while i < host.borrow_mut().length(&symbols)? {
                if !host.borrow_mut().has_index(&symbols, i)? {
                    i += 1;
                    continue;
                }
                let element = {
                    let mut h = host.borrow_mut();
                    let symbol = h.index(&symbols, i)?;
                    h.await_element(symbol)
                };
                let symbol = element.await?;
                let result = {
                    let mut h = host.borrow_mut();
                    if disposed(&mut *h)? {
                        None
                    } else {
                        Some(call(
                            &mut *h,
                            Binding::MetadataSymbol,
                            vec![symbol, mode.clone()],
                        )?)
                    }
                };
                let row = if let Some(result) = result {
                    result.await?
                } else {
                    host.borrow().array(vec![])
                };
                metadata.resize_with(i + 1, || None);
                metadata[i] = Some(row);
                i += 1;
            }
            let mut h = host.borrow_mut();
            let rows = h.array(metadata);
            invoke(&mut *h, Binding::Flatten, vec![rows])
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                let mut h = host.borrow_mut();
                emit(&mut *h, &e, true, "msg")?;
                Ok(h.array(vec![]))
            }
        }
    })
}
fn selected<H: Host>(
    h: &mut H,
    rows: &H::Value,
    mut f: impl FnMut(&mut H, H::Value) -> Result<bool, H::Error>,
) -> Result<Vec<H::Value>, H::Error> {
    let n = h.length(rows)?;
    let mut out = vec![];
    for i in 0..n {
        if h.has_index(rows, i)? {
            let v = h.index(rows, i)?;
            if f(h, v.clone())? {
                out.push(v);
            }
        }
    }
    Ok(out)
}
fn mapped<H: Host>(
    h: &mut H,
    rows: &H::Value,
    mut f: impl FnMut(&mut H, H::Value) -> Result<H::Value, H::Error>,
) -> Result<Vec<Option<H::Value>>, H::Error> {
    let n = h.length(rows)?;
    let mut out = vec![];
    for i in 0..n {
        out.push(if h.has_index(rows, i)? {
            let v = h.index(rows, i)?;
            Some(f(h, v)?)
        } else {
            None
        });
    }
    Ok(out)
}
fn position_id<H: Host>(
    h: &mut H,
    p: &H::Value,
    fallback: Option<&H::Value>,
) -> Result<H::Value, H::Error> {
    let c = h.prepare(Binding::PositionId)?;
    let symbol = h.field(p, "symbol")?;
    let side = h.field(p, "side")?;
    let mode = h.field(p, "positionMode")?;
    let mode = if h.truthy(&mode) {
        mode
    } else {
        fallback.cloned().unwrap_or(mode)
    };
    h.invoke(c, vec![symbol, side, mode])
}
// ts: WOOXExchange.fetchPositions
pub fn fetch_positions<H: Host>(
    host: Rc<RefCell<H>>,
    symbols: Option<H::Value>,
) -> Task<H::Value, H::Error> {
    let read = call(&mut *host.borrow_mut(), Binding::OpenPositions, vec![]);
    Box::pin(async move {
        let open = read?.await?;
        let mode = call(&mut *host.borrow_mut(), Binding::PositionMode, vec![])?;
        let mode = mode.await?;
        let (target, selected_open, needed) = {
            let mut h = host.borrow_mut();
            let explicit = symbols.as_ref().is_some_and(|s| h.truthy(s));
            let target = if explicit {
                symbols.as_ref().unwrap().clone()
            } else {
                let mut values = mapped(&mut *h, &open, |h, p| h.field(&p, "symbol"))?
                    .into_iter()
                    .map(|v| v.unwrap_or_else(|| h.undefined()))
                    .collect::<Vec<_>>();
                let orders = h.root("orders")?;
                let orders = selected(&mut *h, &orders, |h, o| {
                    let status = h.field(&o, "status")?;
                    Ok(h.equals(&status, "open"))
                })?;
                let orders = h.array(orders.into_iter().map(Some).collect());
                values.extend(
                    mapped(&mut *h, &orders, |h, o| h.field(&o, "symbol"))?
                        .into_iter()
                        .map(|v| v.unwrap_or_else(|| h.undefined())),
                );
                let mut unique = vec![];
                for v in values {
                    if !unique.iter().any(|x| h.same_zero(x, &v)) {
                        unique.push(v);
                    }
                }
                h.array(unique.into_iter().map(Some).collect())
            };
            if h.length(&target)? == 0 {
                return Ok(open);
            }
            let filtered = selected(&mut *h, &open, |h, p| {
                let symbol = h.field(&p, "symbol")?;
                let n = h.length(&target)?;
                for i in 0..n {
                    let v = h.index(&target, i)?;
                    if h.same_zero(&v, &symbol) {
                        return Ok(true);
                    }
                }
                Ok(false)
            })?;
            let filtered = h.array(filtered.into_iter().map(Some).collect());
            let needed = selected(&mut *h, &target, |h, symbol| {
                let positions = selected(h, &filtered, |h, p| {
                    let ps = h.field(&p, "symbol")?;
                    Ok(h.same(&ps, &symbol))
                })?;
                if positions.is_empty() {
                    return Ok(true);
                }
                if !h.equals(&mode, "hedged") {
                    return Ok(false);
                }
                let mut long = false;
                for p in &positions {
                    let side = h.field(p, "side")?;
                    if h.equals(&side, "long") {
                        long = true;
                        break;
                    }
                }
                let mut short = false;
                for p in &positions {
                    let side = h.field(p, "side")?;
                    if h.equals(&side, "short") {
                        short = true;
                        break;
                    }
                }
                Ok(!long || !short)
            })?;
            let needed = h.array(needed.into_iter().map(Some).collect());
            (target, filtered, needed)
        };
        let metadata = call(&mut *host.borrow_mut(), Binding::Metadata, vec![needed])?;
        let metadata = metadata.await?;
        let mut h = host.borrow_mut();
        let ids = mapped(&mut *h, &selected_open, |h, p| {
            position_id(h, &p, Some(&mode))
        })?;
        let ids = ids
            .into_iter()
            .map(|v| v.unwrap_or_else(|| h.undefined()))
            .collect::<Vec<_>>();
        let fake = selected(&mut *h, &metadata, |h, p| {
            let id = position_id(h, &p, None)?;
            Ok(!ids.iter().any(|x| h.same_zero(x, &id)))
        })?;
        let fake = h.array(fake.into_iter().map(Some).collect());
        let fake = mapped(&mut *h, &fake, |h, p| {
            let id = position_id(h, &p, None)?;
            let zero = h.number(0.0);
            h.object_spread(
                vec![("id", id)],
                p,
                [
                    "entryPrice",
                    "notional",
                    "unrealizedPnl",
                    "contracts",
                    "liquidationPrice",
                ]
                .into_iter()
                .map(|k| (k, zero.clone()))
                .collect(),
            )
        })?;
        let chosen = if symbols.as_ref().is_some_and(|s| h.truthy(s)) {
            selected_open
        } else {
            open
        };
        let mut out = vec![];
        let mut it = h.iterator(chosen)?;
        while let Some(v) = h.next(&mut it)? {
            out.push(Some(v));
        }
        out.extend(
            fake.into_iter()
                .map(|v| Some(v.unwrap_or_else(|| h.undefined()))),
        );
        let _target = target;
        Ok(h.array(out))
    })
}
