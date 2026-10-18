//! Complete REST read and order projection bodies. Runtime property/builtin/transport
//! bindings are explicit native dependencies; exchange construction stays separate.
use std::{future::Future, rc::Rc};

pub trait ReadHost: 'static {
    type Value: Clone + 'static;
    type Error: 'static;
    type Prepared;
    type PostFuture: Future<Output = Result<Self::Value, Self::Error>>;
    type ModeFuture: Future<Output = Result<Self::Value, Self::Error>>;
    fn root(&self) -> Self::Value;
    fn undefined(&self) -> Self::Value;
    fn number(&self, value: f64) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn string(&self, value: &'static str) -> Self::Value;
    fn object(&self, fields: Vec<(&'static str, Self::Value)>) -> Self::Value;
    fn array(&self, values: Vec<Self::Value>) -> Self::Value;
    fn get(&self, value: &Self::Value, key: &'static str) -> Result<Self::Value, Self::Error>;
    fn index(&self, value: &Self::Value, index: usize) -> Result<Self::Value, Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn equal_string(&self, value: &Self::Value, text: &'static str) -> bool;
    fn zero_number(&self, value: &Self::Value) -> bool;
    fn greater_zero(&self, value: Self::Value) -> Result<bool, Self::Error>;
    fn divide(&self, value: Self::Value, denominator: f64) -> Result<f64, Self::Error>;
    fn prepare(
        &self,
        receiver: &Self::Value,
        key: &'static str,
    ) -> Result<Self::Prepared, Self::Error>;
    fn call(
        &self,
        prepared: Self::Prepared,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn endpoint(&self, key: &'static str) -> Result<Self::Value, Self::Error>;
    fn post(
        &self,
        prepared: Self::Prepared,
        endpoint: Self::Value,
        params: Self::Value,
    ) -> Result<Self::PostFuture, Self::Error>;
    /// Promise.all owns both eagerly created original promises, including rejection handling.
    fn prepare_all(&self) -> Result<Self::Prepared, Self::Error>;
    fn all(
        &self,
        prepared: Self::Prepared,
        futures: Vec<Self::PostFuture>,
    ) -> Result<Self::PostFuture, Self::Error>;
    fn position_mode(&self, prepared: Self::Prepared) -> Result<Self::ModeFuture, Self::Error>;
    fn imported(&self, module: &'static str, key: &'static str)
    -> Result<Self::Value, Self::Error>;
    /// Standard builtin bindings; parsing/coercion and their errors retain original values.
    fn prepare_parse_float(&self) -> Result<Self::Prepared, Self::Error>;
    fn call_parse_float(
        &self,
        prepared: Self::Prepared,
        value: Self::Value,
    ) -> Result<f64, Self::Error>;
    fn parse_float(&self, value: Self::Value) -> Result<f64, Self::Error> {
        let prepared = self.prepare_parse_float()?;
        self.call_parse_float(prepared, value)
    }
    fn prepare_pow(&self) -> Result<Self::Prepared, Self::Error>;
    fn pow10_negative(
        &self,
        prepared: Self::Prepared,
        value: Self::Value,
    ) -> Result<f64, Self::Error>;
    fn abs(&self, value: Self::Value) -> Result<f64, Self::Error>;
    fn max_zero(&self, value: f64) -> Result<f64, Self::Error>;
    fn now(&self) -> Result<Self::Value, Self::Error>;
    fn template_string(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn property_key(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    /// Native two-entry destructuring: bind each yielded entry before the next step;
    /// close after bindings, and on binding error close while retaining that original error.
    fn iterator_pair<F: FnMut(Self::Value) -> Result<Self::Value, Self::Error>>(
        &self,
        value: Self::Value,
        bind: F,
    ) -> Result<(Self::Value, Self::Value), Self::Error>;
    fn ticker_id(
        &self,
        instrument: Self::Value,
        time: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn interval(&self, interval: Self::Value) -> Result<Self::Value, Self::Error>;
    fn create_position_id(
        &self,
        symbol: Self::Value,
        side: Self::Value,
        mode: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    /// Resolve and invoke native Array methods; no store/object borrow may span a callback.
    fn map<F: Fn(Self::Value) -> Result<Self::Value, Self::Error> + 'static>(
        &self,
        array: Self::Value,
        callback: F,
    ) -> Result<Self::Value, Self::Error>;
    fn filter<F: Fn(Self::Value) -> Result<Self::Value, Self::Error> + 'static>(
        &self,
        array: Self::Value,
        callback: F,
    ) -> Result<Self::Value, Self::Error>;
    /// Spread/for-of are native iterator dependencies, with incremental callbacks.
    fn spread(&self, array: Self::Value) -> Result<Vec<Self::Value>, Self::Error>;
    fn each<F: FnMut(Self::Value) -> Result<(), Self::Error>>(
        &self,
        iterable: Self::Value,
        callback: F,
    ) -> Result<(), Self::Error>;
    fn prepare_set(&self) -> Result<Self::Prepared, Self::Error>;
    fn set(&self, prepared: Self::Prepared, array: Self::Value)
    -> Result<Self::Value, Self::Error>;
    fn uniq_by(&self, array: Self::Value, key: &'static str) -> Result<Self::Value, Self::Error>;
    fn assign(
        &self,
        object: &Self::Value,
        key: Self::Value,
        value: Self::Value,
    ) -> Result<(), Self::Error>;
    fn error_value(&self, error: &Self::Error) -> Self::Value;
    fn error_template(
        &self,
        prefix: &'static str,
        error: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn severity_error(&self) -> Result<Self::Value, Self::Error>;
}
enum Begun<F, V, E> {
    Waiting(F),
    Done(Result<V, E>),
}
fn caught_begin<H: ReadHost>(
    h: &H,
    result: Result<H::PostFuture, H::Error>,
    prefix: &'static str,
    swallow: bool,
) -> Begun<H::PostFuture, H::Value, H::Error> {
    match result {
        Ok(f) => Begun::Waiting(f),
        Err(e) => Begun::Done(catch(h, e, prefix, swallow)),
    }
}
fn empty<H: ReadHost>(h: &H) -> H::Value {
    h.object(vec![])
}
fn fallback_number(value: f64, fallback: f64) -> f64 {
    if value == 0.0 || value.is_nan() {
        fallback
    } else {
        value
    }
}
fn parsed<H: ReadHost>(h: &H, value: &H::Value, key: &'static str) -> Result<f64, H::Error> {
    let prepared = h.prepare_parse_float()?;
    h.call_parse_float(prepared, h.get(value, key)?)
}
fn or_empty<H: ReadHost>(h: &H, value: H::Value) -> H::Value {
    if h.truthy(&value) {
        value
    } else {
        h.array(vec![])
    }
}
fn nullish_field<H: ReadHost>(
    h: &H,
    value: &H::Value,
    keys: &[&'static str],
    fallback: H::Value,
) -> Result<H::Value, H::Error> {
    for key in keys {
        let v = h.get(value, key)?;
        if !h.nullish(&v) {
            return Ok(v);
        }
    }
    Ok(fallback)
}
fn begin<H: ReadHost>(
    h: &H,
    endpoint: &'static str,
    params: H::Value,
) -> Result<H::PostFuture, H::Error> {
    let xhr = h.get(&h.root(), "xhr")?;
    let prepared = h.prepare(&xhr, "post")?;
    let endpoint = h.endpoint(endpoint)?;
    h.post(prepared, endpoint, params)
}
fn catch<H: ReadHost>(
    h: &H,
    error: H::Error,
    prefix: &'static str,
    swallow: bool,
) -> Result<H::Value, H::Error> {
    let log = h.prepare(&h.root(), "log")?;
    let message = h.error_template(prefix, h.error_value(&error))?;
    let severity = h.severity_error()?;
    h.call(log, vec![message, severity])?;
    if swallow {
        Ok(h.array(vec![]))
    } else {
        Err(error)
    }
}
fn data<H: ReadHost>(h: &H, response: H::Value) -> Result<H::Value, H::Error> {
    h.get(&response, "data")
}

pub fn fetch_markets<H: ReadHost>(h: Rc<H>) -> impl Future<Output = Result<H::Value, H::Error>> {
    let begun = begin(&*h, "INSTRUMENTS", empty(&*h));
    let begun = caught_begin(&*h, begun, "Error fetching markets: ", false);
    async move {
        let begun = match begun {
            Begun::Waiting(f) => f,
            Begun::Done(result) => return result,
        };
        let result = async {
            let response = begun.await?;
            let payload = data(&*h, response)?;
            let rows = h.get(&payload, "data")?;
            let rows = h.filter(rows, {
                let callback_h = h.clone();
                move |m| {
                    let h = callback_h.as_ref();
                    let kind = h.get(&m, "inst_type")?;
                    if h.equal_string(&kind, "PERPETUAL_SWAP") {
                        h.get(&m, "tradable")
                    } else {
                        Ok(h.boolean(false))
                    }
                }
            })?;
            h.map(rows, {
                let callback_h = h.clone();
                move |m| {
                    let h = callback_h.as_ref();
                    let qty_tick = parsed(h, &m, "qty_tick_size")?;
                    let price_tick = parsed(h, &m, "price_tick_size")?;
                    let id = h.get(&m, "symbol")?;
                    let symbol = h.get(&m, "symbol")?;
                    let base = h.get(&m, "base_ccy")?;
                    let quote = h.get(&m, "quote_ccy")?;
                    let contract_size = fallback_number(parsed(h, &m, "contract_size")?, 1.0);
                    let underlying = h.get(&m, "underlying_symbol")?;
                    let amount = if qty_tick != 0.0 && !qty_tick.is_nan() {
                        qty_tick
                    } else {
                        {
                            let pow = h.prepare_pow()?;
                            h.pow10_negative(pow, h.get(&m, "quantity_decimals")?)?
                        }
                    };
                    let price = if price_tick != 0.0 && !price_tick.is_nan() {
                        price_tick
                    } else {
                        {
                            let pow = h.prepare_pow()?;
                            h.pow10_negative(pow, h.get(&m, "quote_decimals")?)?
                        }
                    };
                    let base_precision = h.get(&m, "quantity_decimals")?;
                    let quote_precision = h.get(&m, "quote_decimals")?;
                    let leverage = parsed(h, &m, "max_leverage")?;
                    let bounds = || h.object(vec![("min", h.number(0.0)), ("max", h.number(0.0))]);
                    Ok(h.object(vec![
                        ("id", id),
                        ("symbol", symbol),
                        ("base", base),
                        ("quote", quote),
                        ("active", h.boolean(true)),
                        ("type", h.string("swap")),
                        ("spot", h.boolean(false)),
                        ("swap", h.boolean(true)),
                        ("future", h.boolean(false)),
                        ("contract", h.boolean(true)),
                        ("linear", h.boolean(true)),
                        ("inverse", h.boolean(false)),
                        ("contractSize", h.number(contract_size)),
                        ("info", h.object(vec![("underlyingSymbol", underlying)])),
                        (
                            "precision",
                            h.object(vec![
                                ("amount", h.number(amount)),
                                ("price", h.number(price)),
                                ("base", base_precision),
                                ("quote", quote_precision),
                            ]),
                        ),
                        (
                            "limits",
                            h.object(vec![
                                (
                                    "amount",
                                    h.object(vec![
                                        ("min", h.number(fallback_number(qty_tick, 0.0))),
                                        ("max", h.number(0.0)),
                                    ]),
                                ),
                                ("price", bounds()),
                                ("cost", bounds()),
                                (
                                    "leverage",
                                    h.object(vec![
                                        ("min", h.number(1.0)),
                                        ("max", h.number(leverage)),
                                    ]),
                                ),
                            ]),
                        ),
                    ]))
                }
            })
        }
        .await;
        match result {
            Ok(value) => Ok(value),
            Err(error) => catch(&*h, error, "Error fetching markets: ", false),
        }
    }
}
pub fn fetch_tickers<H: ReadHost>(h: Rc<H>) -> impl Future<Output = Result<H::Value, H::Error>> {
    let begun = begin(&*h, "TICKERS", empty(&*h));
    let begun = caught_begin(&*h, begun, "Error fetching tickers: ", false);
    async move {
        let begun = match begun {
            Begun::Waiting(f) => f,
            Begun::Done(result) => return result,
        };
        let result = async {
            let payload = data(&*h, begun.await?)?;
            let rows = h.get(&payload, "data")?;
            let rows = h.filter(rows, {
                let callback_h = h.clone();
                move |t| {
                    let h = callback_h.as_ref();
                    let instrument = h.get(&t, "i")?;
                    let includes = h.prepare(&instrument, "includes")?;
                    let included = h.call(includes, vec![h.string("PERP")])?;
                    Ok(included)
                }
            })?;
            h.map(rows, {
                let callback_h = h.clone();
                move |t| {
                    let h = callback_h.as_ref();
                    let instrument = h.get(&t, "i")?;
                    let instrument = h.template_string(instrument)?;
                    let time = h.now()?;
                    let id = h.ticker_id(instrument, time)?;
                    let symbol = h.get(&t, "i")?;
                    let bid = parsed(h, &t, "b")?;
                    let ask = parsed(h, &t, "k")?;
                    let last = parsed(h, &t, "a")?;
                    let percentage = parsed(h, &t, "c")? * 100.0;
                    let volume = parsed(h, &t, "v")?;
                    let quote_volume = parsed(h, &t, "vv")?;
                    let oi = fallback_number(parsed(h, &t, "oi")?, 0.0);
                    Ok(h.object(vec![
                        ("id", id),
                        ("symbol", symbol),
                        ("bid", h.number(bid)),
                        ("ask", h.number(ask)),
                        ("last", h.number(last)),
                        ("mark", h.number(0.0)),
                        ("index", h.number(0.0)),
                        ("percentage", h.number(percentage)),
                        ("fundingRate", h.number(0.0)),
                        ("fundingTime", h.number(0.0)),
                        ("volume", h.number(volume)),
                        ("quoteVolume", h.number(quote_volume)),
                        ("openInterest", h.number(oi)),
                    ]))
                }
            })
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => catch(&*h, e, "Error fetching tickers: ", false),
        }
    }
}
pub fn fetch_ohlcv<H: ReadHost>(
    h: Rc<H>,
    opts: H::Value,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    let begun = (|| {
        let symbol = h.get(&opts, "symbol")?;
        let interval = h.interval(h.get(&opts, "interval")?)?;
        let params = h.object(vec![("instrument_name", symbol), ("timeframe", interval)]);
        if h.truthy(&h.get(&opts, "from")?) {
            h.assign(&params, h.string("start_ts"), h.get(&opts, "from")?)?;
        }
        if h.truthy(&h.get(&opts, "to")?) {
            h.assign(&params, h.string("end_ts"), h.get(&opts, "to")?)?;
        }
        begin(&*h, "CANDLESTICK", params)
    })();
    let begun = caught_begin(&*h, begun, "Error fetching OHLCV: ", false);
    async move {
        let begun = match begun {
            Begun::Waiting(f) => f,
            Begun::Done(result) => return result,
        };
        let result = async {
            let payload = data(&*h, begun.await?)?;
            let rows = h.get(&payload, "data")?;
            h.map(rows, {
                let callback_h = h.clone();
                move |k| {
                    let h = callback_h.as_ref();
                    let timestamp = h.get(&k, "t")?;
                    let open = parsed(h, &k, "o")?;
                    let high = parsed(h, &k, "h")?;
                    let low = parsed(h, &k, "l")?;
                    let close = parsed(h, &k, "c")?;
                    let volume = parsed(h, &k, "v")?;
                    Ok(h.object(vec![
                        ("timestamp", timestamp),
                        ("open", h.number(open)),
                        ("high", h.number(high)),
                        ("low", h.number(low)),
                        ("close", h.number(close)),
                        ("volume", h.number(volume)),
                    ]))
                }
            })
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => catch(&*h, e, "Error fetching OHLCV: ", false),
        }
    }
}
fn book_rows<H: ReadHost>(h: Rc<H>, rows: H::Value) -> Result<H::Value, H::Error> {
    h.map(rows, {
        let callback_h = h.clone();
        move |row| {
            let h = callback_h.as_ref();
            // Destructuring consumes only two iterator entries, not all trailing entries.
            let (price, quantity) = h.iterator_pair(row, Ok)?;
            let parsed_price = h.parse_float(price.clone())?;
            let amount = h.parse_float(quantity)?;
            Ok(h.object(vec![
                ("id", price),
                ("price", h.number(parsed_price)),
                ("amount", h.number(amount)),
                ("total", h.number(0.0)),
            ]))
        }
    })
}
pub fn fetch_order_book<H: ReadHost>(
    h: Rc<H>,
    symbol: H::Value,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    // Resolve xhr.post and endpoint before constructing the source request object.
    let begun = (|| {
        let xhr = h.get(&h.root(), "xhr")?;
        let prepared = h.prepare(&xhr, "post")?;
        let endpoint = h.endpoint("BOOK")?;
        h.post(
            prepared,
            endpoint,
            h.object(vec![("instrument_name", symbol), ("depth", h.number(50.0))]),
        )
    })();
    let begun = caught_begin(&*h, begun, "Error fetching order book: ", false);
    async move {
        let begun = match begun {
            Begun::Waiting(f) => f,
            Begun::Done(result) => return result,
        };
        let result = async {
            let payload = data(&*h, begun.await?)?;
            let rows = h.get(&payload, "data")?;
            let first = if h.nullish(&rows) {
                h.undefined()
            } else {
                h.index(&rows, 0)?
            };
            if !h.truthy(&first) {
                return Ok(h.object(vec![("bids", h.array(vec![])), ("asks", h.array(vec![]))]));
            }
            let bids = book_rows(h.clone(), h.get(&first, "bids")?)?;
            let asks = book_rows(h.clone(), h.get(&first, "asks")?)?;
            Ok(h.object(vec![("bids", bids), ("asks", asks)]))
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => catch(&*h, e, "Error fetching order book: ", false),
        }
    }
}
pub fn fetch_orders<H: ReadHost>(h: Rc<H>) -> impl Future<Output = Result<H::Value, H::Error>> {
    let begun = (|| {
        let prepared_all = h.prepare_all()?;
        let regular = begin(&*h, "OPEN_ORDERS", empty(&*h))?;
        let advanced = begin(&*h, "ADVANCED_OPEN_ORDERS", empty(&*h))?;
        h.all(prepared_all, vec![regular, advanced])
    })();
    let begun = caught_begin(&*h, begun, "Error fetching orders: ", true);
    async move {
        let begun = match begun {
            Begun::Waiting(f) => f,
            Begun::Done(result) => return result,
        };
        let result = async {
            let responses = begun.await?;
            let (regular, advanced) = h.iterator_pair(responses, |response| data(&*h, response))?;
            let mut rows = h.spread(or_empty(&*h, h.get(&regular, "data")?))?;
            rows.extend(h.spread(or_empty(&*h, h.get(&advanced, "data")?))?);
            h.map(h.array(rows), {
                let callback_h = h.clone();
                move |o| {
                    let h = callback_h.as_ref();
                    let method = h.prepare(&h.root(), "mapOrder")?;
                    h.call(method, vec![o])
                }
            })
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => catch(&*h, e, "Error fetching orders: ", true),
        }
    }
}
pub fn map_order<H: ReadHost>(h: &H, o: H::Value) -> Result<H::Value, H::Error> {
    let quantity = fallback_number(parsed(h, &o, "quantity")?, 0.0);
    let filled = if h.truthy(&h.get(&o, "cumulative_quantity")?) {
        fallback_number(parsed(h, &o, "cumulative_quantity")?, 0.0)
    } else {
        0.0
    };
    let parse_price = h.prepare_parse_float()?;
    let price = fallback_number(
        h.call_parse_float(
            parse_price,
            nullish_field(
                h,
                &o,
                &["ref_price", "trigger_price", "limit_price"],
                h.string("0"),
            )?,
        )?,
        0.0,
    );
    let status = nullish_field(h, &o, &["status", "order_status"], h.string("ACTIVE"))?;
    let updated = nullish_field(h, &o, &["update_time", "time_updated"], h.undefined())?;
    let created = nullish_field(h, &o, &["create_time", "time_created"], h.undefined())?;
    let id = h.get(&o, "order_id")?;
    let symbol = h.get(&o, "instrument_name")?;
    let kind_method = h.prepare(&h.root(), "mapCryptocomOrderType")?;
    let kind = h.call(kind_method, vec![h.get(&o, "order_type")?])?;
    let side = if h.equal_string(&h.get(&o, "side")?, "BUY") {
        h.imported("OrderSide", "Buy")?
    } else {
        h.imported("OrderSide", "Sell")?
    };
    let remaining = h.max_zero(quantity - filled)?;
    let status_method = h.prepare(&h.root(), "mapOrderStatus")?;
    let status = h.call(status_method, vec![status])?;
    let last_update = if h.truthy(&updated) {
        updated
    } else if h.truthy(&created) {
        created
    } else {
        h.now()?
    };
    let exec = h.get(&o, "exec_inst")?;
    let reduce_only = if h.nullish(&exec) {
        h.boolean(false)
    } else {
        let includes = h.prepare(&exec, "includes")?;
        let value = h.call(includes, vec![h.string("REDUCE_ONLY")])?;
        if h.nullish(&value) {
            h.boolean(false)
        } else {
            value
        }
    };
    let tif_method = h.prepare(&h.root(), "mapCryptocomTimeInForce")?;
    let tif = h.call(tif_method, vec![h.get(&o, "time_in_force")?])?;
    Ok(h.object(vec![
        ("id", id),
        ("symbol", symbol),
        ("type", kind),
        ("side", side),
        ("price", h.number(price)),
        ("amount", h.number(quantity)),
        ("filled", h.number(filled)),
        ("remaining", h.number(remaining)),
        ("status", status),
        ("lastUpdate", last_update),
        ("reduceOnly", reduce_only),
        ("timeInForce", tif),
    ]))
}
fn zero_position<H: ReadHost>(
    h: &H,
    market: &H::Value,
    side: &'static str,
    mode: H::Value,
) -> Result<H::Value, H::Error> {
    let id = h.create_position_id(
        h.get(market, "symbol")?,
        h.imported("PositionSide", side)?,
        mode.clone(),
    )?;
    let symbol = h.get(market, "symbol")?;
    let side = h.imported("PositionSide", side)?;
    let now = h.now()?;
    let margin = h.imported("MarginType", "Cross")?;
    Ok(h.object(vec![
        ("id", id),
        ("symbol", symbol),
        ("side", side.clone()),
        ("entryPrice", h.number(0.0)),
        ("notional", h.number(0.0)),
        ("leverage", h.number(1.0)),
        ("unrealizedPnl", h.number(0.0)),
        ("contracts", h.number(0.0)),
        ("liquidationPrice", h.number(0.0)),
        ("lastUpdate", now),
        ("positionMode", mode),
        ("marginType", margin),
    ]))
}
pub fn fetch_positions<H: ReadHost>(h: Rc<H>) -> impl Future<Output = Result<H::Value, H::Error>> {
    let begun = begin(&*h, "POSITIONS", empty(&*h));
    let begun = caught_begin(&*h, begun, "Error fetching positions: ", true);
    async move {
        let begun = match begun {
            Begun::Waiting(f) => f,
            Begun::Done(result) => return result,
        };
        let result = async {
            let payload = data(&*h, begun.await?)?;
            let mode_method = h.prepare(&h.root(), "getPositionMode")?;
            let mode = h.position_mode(mode_method)?.await?;
            let rows = or_empty(&*h, h.get(&payload, "data")?);
            let rows = h.map(rows, {
                let callback_h = h.clone();
                move |p| {
                    let h = callback_h.as_ref();
                    let quantity = parsed(h, &p, "quantity")?;
                    let cost = parsed(h, &p, "cost")?;
                    Ok(h.object(vec![
                        ("p", p),
                        ("quantity", h.number(quantity)),
                        ("cost", h.number(cost)),
                    ]))
                }
            })?;
            let rows = h.filter(rows, {
                let callback_h = h.clone();
                move |row| {
                    let h = callback_h.as_ref();
                    Ok(h.boolean(!h.zero_number(&h.get(&row, "quantity")?)))
                }
            })?;
            let actual = h.map(rows, {
                let callback_h = h.clone();
                let mode = mode.clone();
                move |row| {
                    let h = callback_h.as_ref();
                    let p = h.get(&row, "p")?;
                    let quantity = h.get(&row, "quantity")?;
                    let cost = h.get(&row, "cost")?;
                    let side = if h.greater_zero(quantity.clone())? {
                        h.imported("PositionSide", "Long")?
                    } else {
                        h.imported("PositionSide", "Short")?
                    };
                    let contracts = h.abs(quantity)?;
                    let entry = fallback_number(h.divide(cost.clone(), contracts)?, 0.0);
                    let id = h.create_position_id(
                        h.get(&p, "instrument_name")?,
                        side.clone(),
                        mode.clone(),
                    )?;
                    let symbol = h.get(&p, "instrument_name")?;
                    let notional = h.abs(cost)?;
                    let upnl = fallback_number(parsed(h, &p, "open_position_pnl")?, 0.0);
                    let margin = if h.equal_string(&h.get(&p, "isolation_type")?, "ISOLATED_MARGIN")
                    {
                        h.imported("MarginType", "Isolated")?
                    } else {
                        h.imported("MarginType", "Cross")?
                    };
                    let isolation = h.get(&p, "isolation_id")?;
                    let updated = h.get(&p, "update_timestamp_ms")?;
                    let updated = if h.truthy(&updated) {
                        updated
                    } else {
                        h.now()?
                    };
                    Ok(h.object(vec![
                        ("id", id),
                        ("symbol", symbol),
                        ("side", side.clone()),
                        ("entryPrice", h.number(entry)),
                        ("notional", h.number(notional)),
                        ("leverage", h.number(1.0)),
                        ("unrealizedPnl", h.number(upnl)),
                        ("contracts", h.number(contracts)),
                        ("liquidationPrice", h.number(0.0)),
                        ("marginType", margin),
                        ("positionMode", mode.clone()),
                        ("info", h.object(vec![("isolationId", isolation)])),
                        ("lastUpdate", updated),
                    ]))
                }
            })?;
            let prepared_set = h.prepare_set()?;
            let symbols = h.map(actual.clone(), {
                let h = h.clone();
                move |p| h.get(&p, "symbol")
            })?;
            let existing = h.set(prepared_set, symbols)?;
            let fake = h.array(vec![]);
            let store = h.get(&h.root(), "store")?;
            let markets = h.get(&store, "markets")?;
            h.each(markets, |m| {
                if h.truthy(&h.get(&m, "active")?) && !h.truthy(&h.get(&m, "isSpot")?) {
                    let has = h.prepare(&existing, "has")?;
                    let present = h.call(has, vec![h.get(&m, "symbol")?])?;
                    if !h.truthy(&present) {
                        let push = h.prepare(&fake, "push")?;
                        let long = zero_position(&*h, &m, "Long", mode.clone())?;
                        let short = zero_position(&*h, &m, "Short", mode.clone())?;
                        h.call(push, vec![long, short])?;
                    }
                }
                Ok(())
            })?;
            let mut all = h.spread(actual)?;
            all.extend(h.spread(fake)?);
            h.uniq_by(h.array(all), "id")
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => catch(&*h, e, "Error fetching positions: ", true),
        }
    }
}
pub fn fetch_balance<H: ReadHost>(h: Rc<H>) -> impl Future<Output = Result<H::Value, H::Error>> {
    let begun = begin(&*h, "BALANCE", empty(&*h));
    let begun = caught_begin(&*h, begun, "Error fetching balance: ", false);
    async move {
        let begun = match begun {
            Begun::Waiting(f) => f,
            Begun::Done(result) => return result,
        };
        let result = async {
            let payload = data(&*h, begun.await?)?;
            let rows = h.get(&payload, "data")?;
            let summary = if h.nullish(&rows) {
                h.undefined()
            } else {
                h.index(&rows, 0)?
            };
            let balances = empty(&*h);
            let positions = if h.nullish(&summary) {
                h.undefined()
            } else {
                h.get(&summary, "position_balances")?
            };
            h.each(or_empty(&*h, positions), |pb| {
                let quantity = parsed(&*h, &pb, "quantity")?;
                let free = parsed(&*h, &pb, "max_withdrawal_balance")?;
                let key = h.get(&pb, "instrument_name")?;
                let locked = h.max_zero(quantity - free)?;
                let key = h.property_key(key)?;
                h.assign(
                    &balances,
                    key,
                    h.object(vec![
                        ("available", h.number(free)),
                        ("locked", h.number(locked)),
                    ]),
                )
            })?;
            if !h.truthy(&summary) {
                return Ok(h.object(vec![
                    ("total", h.number(0.0)),
                    ("free", h.number(0.0)),
                    ("used", h.number(0.0)),
                    ("upnl", h.number(0.0)),
                    ("currencies", balances),
                ]));
            }
            let total = fallback_number(parsed(&*h, &summary, "total_margin_balance")?, 0.0);
            let free = fallback_number(parsed(&*h, &summary, "total_available_balance")?, 0.0);
            let used = fallback_number(parsed(&*h, &summary, "total_initial_margin")?, 0.0);
            let upnl = fallback_number(parsed(&*h, &summary, "total_session_unrealized_pnl")?, 0.0);
            Ok(h.object(vec![
                ("total", h.number(total)),
                ("free", h.number(free)),
                ("used", h.number(used)),
                ("upnl", h.number(upnl)),
                ("currencies", balances),
            ]))
        }
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => catch(&*h, e, "Error fetching balance: ", false),
        }
    }
}

#[cfg(test)]
mod accepted_control_flow {
    use super::*;
    use crate::native_bindings::BoundHost;
    use safe_cex_exchange::js_runtime::value::{Js, Thrown};
    use std::cell::RefCell;

    #[test]
    fn parsed_native_contract() {
        let cases = [
            (None, -0.0),
            (None, f64::INFINITY),
            (None, f64::NAN),
            (Some("global"), 17.0),
            (Some("field"), 17.0),
            (Some("call"), 17.0),
            (Some("numeric"), 17.0),
        ];
        let mut executed = 0;
        for (failure, output) in cases {
            let events = Rc::new(RefCell::new(Vec::new()));
            let globals = Js::object(vec![]);
            let owner = Js::object(vec![]);
            let input = Js::object(vec![]);
            let error = Js::object(vec![]);
            let (expected, t, e) = (input.clone(), events.clone(), error.clone());
            let parser = Js::function("supplied parser", move |this, args| {
                t.borrow_mut().push("call");
                assert!(matches!(this, Js::Undefined), "parsed native receiver");
                assert_eq!(args.len(), 1, "parsed native argument count");
                assert!(
                    args[0].same_reference(&expected),
                    "parsed native input identity"
                );
                if failure == Some("call") {
                    Err(Thrown::new(e.clone()))
                } else {
                    Ok(Js::Number(output))
                }
            });
            let (t, e) = (events.clone(), error.clone());
            globals
                .define_accessor(
                    "parseFloat",
                    Some(Rc::new(move |_| {
                        t.borrow_mut().push("global");
                        if failure == Some("global") {
                            Err(Thrown::new(e.clone()))
                        } else {
                            Ok(parser.clone())
                        }
                    })),
                    None,
                )
                .expect("fresh extensible accessor target");
            let (expected, t, e) = (owner.clone(), events.clone(), error.clone());
            owner
                .define_accessor(
                    "quantity",
                    Some(Rc::new(move |this| {
                        t.borrow_mut().push("field");
                        assert!(
                            this.same_reference(&expected),
                            "parsed native field receiver"
                        );
                        if failure == Some("field") {
                            Err(Thrown::new(e.clone()))
                        } else {
                            Ok(input.clone())
                        }
                    })),
                    None,
                )
                .expect("fresh extensible accessor target");
            let mut ops = crate::accepted_test_support::binding(globals);
            let (t, e) = (events.clone(), error.clone());
            Rc::get_mut(&mut ops.value).unwrap().numeric = Rc::new(move |value| {
                t.borrow_mut().push("numeric");
                if failure == Some("numeric") {
                    return Err(Thrown::new(e.clone()));
                }
                match value {
                    Js::Number(number) => Ok(*number),
                    _ => panic!("parsed native numeric carrier"),
                }
            });
            let host = BoundHost {
                ops: Rc::new(ops),
                receiver: Js::object(vec![]),
            };
            match (parsed(&host, &owner, "quantity"), failure) {
                (Ok(actual), None) => assert!(
                    actual.is_nan() && output.is_nan() || actual.to_bits() == output.to_bits(),
                    "parsed native result bits"
                ),
                (Err(actual), Some(_)) => assert!(
                    actual.value.same_reference(&error),
                    "parsed native original error"
                ),
                _ => panic!("parsed native completion"),
            }
            let expected = match failure {
                Some("global") => vec!["global"],
                Some("field") => vec!["global", "field"],
                Some("call") => vec!["global", "field", "call"],
                _ => vec!["global", "field", "call", "numeric"],
            };
            assert_eq!(*events.borrow(), expected, "parsed native evaluation order");
            executed += 1;
        }
        assert_eq!(executed, 7, "parsed native executed cases");
    }
}
