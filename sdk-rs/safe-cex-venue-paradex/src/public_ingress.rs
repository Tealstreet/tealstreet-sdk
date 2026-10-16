//! Public feed method bodies over the original host intrinsics and collections.

// ts: ParadexPublicWebSocket.constructor
pub fn initialize_public_socket<P, M, U, E>(
    parent: &P,
    base: impl FnOnce(&P) -> Result<(), E>,
    urls: impl FnOnce() -> Result<M, E>,
    testnet: impl FnOnce(&P) -> Result<bool, E>,
    lookup: impl FnOnce(M, bool) -> Result<U, E>,
    write: impl FnOnce(U) -> Result<(), E>,
) -> Result<(), E> {
    base(parent)?;
    let map = urls()?;
    write(lookup(map, testnet(parent)?)?)
}

// ts: ParadexPublicWebSocket#handleBBOUpdate; ParadexPublicWebSocket#handleMarketSummaryUpdate
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThrottledFeed {
    Bbo,
    Summary,
}

// ts: ParadexPublicWebSocket#handleBBOUpdate; ParadexPublicWebSocket#handleMarketSummaryUpdate
pub fn install_feed_throttle<T, E>(
    feed: ThrottledFeed,
    factory: impl FnOnce(ThrottledFeed, f64, bool, bool) -> Result<T, E>,
) -> Result<T, E> {
    factory(feed, 100.0, true, true)
}

// ts: ParadexPublicWebSocket#handleBBOUpdate; ParadexPublicWebSocket#handleMarketSummaryUpdate; ParadexPublicWebSocket#handleTradesUpdate
#[derive(Debug, PartialEq)]
pub struct FeedTicker {
    pub symbol: Vec<u16>,
    pub fields: Vec<(&'static str, f64)>,
}

// ts: ParadexPublicWebSocket#updateOHLCVFromTrade
pub struct FeedCandle<V> {
    pub spread: Option<V>,
    pub timestamp: Option<V>,
    pub open: Option<V>,
    pub high: V,
    pub low: V,
    pub close: f64,
    pub volume: V,
}

// ts: ParadexPublicWebSocket.onMessage; ParadexPublicWebSocket#retrySubscribeChannel; ParadexPublicWebSocket#handleBBOUpdate; ParadexPublicWebSocket#handleMarketSummaryUpdate; ParadexPublicWebSocket#handleTradesUpdate; ParadexPublicWebSocket#updateOHLCVFromTrade; ParadexPublicWebSocket#maybeRecoverOhlcvTradeStream
pub trait PublicIngressHost {
    type Value: Clone;
    type Error;
    type Log;
    fn get(&self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn optional_get(&self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn is_object(&self, value: &Self::Value) -> bool;
    fn is_array(&self, value: &Self::Value) -> bool;
    fn is_undefined(&self, value: &Self::Value) -> bool;
    fn is_null(&self, value: &Self::Value) -> bool;
    fn text_value(&self, value: Vec<u16>) -> Self::Value;
    fn literal(&self, text: &str) -> Self::Value;
    fn number(&self, value: f64) -> Self::Value;
    fn equal(&self, left: &Self::Value, right: &Self::Value) -> bool;
    fn string(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn template(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn parse_float(&self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn to_number(&self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn coerce_number(&self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn add(&self, left: &Self::Value, right: f64) -> Result<Self::Value, Self::Error>;
    fn stringify(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn split_optional(
        &self,
        value: &Self::Value,
        separator: char,
    ) -> Result<Self::Value, Self::Error>;
    fn object_keys_empty(&self, value: &Self::Value) -> Result<bool, Self::Error>;
    fn array(&self, values: Vec<Self::Value>) -> Self::Value;
    fn empty_object(&self) -> Self::Value;
    fn each(
        &self,
        value: &Self::Value,
        callback: &mut dyn FnMut(Self::Value) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error>;
    fn iterate(
        &self,
        value: &Self::Value,
        callback: &mut dyn FnMut(Self::Value) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error>;
    fn select_log(&self) -> Result<Self::Log, Self::Error>;
    fn log(
        &self,
        logger: Self::Log,
        text: Vec<u16>,
        severity: &'static str,
    ) -> Result<(), Self::Error>;
    fn caught(&self, prefix: &str, error: Self::Error) -> Result<(), Self::Error>;
    fn update_ticker(&self, ticker: FeedTicker) -> Result<(), Self::Error>;
    fn now(&self) -> Result<f64, Self::Error>;
    fn update_candle_from_trade(
        &self,
        symbol: Vec<u16>,
        price: f64,
        size: f64,
        timestamp: f64,
    ) -> Result<(), Self::Error>;
    fn disposed(&self) -> Result<bool, Self::Error>;
    fn track(&self) -> Result<(), Self::Error>;
    fn parse_event(&self, event: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn handlers(&self, topic: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn stats(&self, topic: Self::Value) -> Result<(), Self::Error>;
    fn call_handler(&self, handler: Self::Value, data: Self::Value) -> Result<(), Self::Error>;
    fn pending(&self, id: Self::Value) -> Result<Self::Value, Self::Error>;
    fn delete_pending(&self, id: Self::Value) -> Result<(), Self::Error>;
    fn retry(
        &self,
        channel: Self::Value,
        attempt: Self::Value,
        reason: Self::Value,
    ) -> Result<(), Self::Error>;
    fn visit_candles(
        &self,
        callback: &mut dyn FnMut(Vec<u16>, Self::Value) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error>;
    fn interval(&self, key: &[u16]) -> Result<Option<f64>, Self::Error>;
    fn write_trade_time(&self, key: &[u16], time: f64) -> Result<(), Self::Error>;
    fn candle(&self, key: &[u16]) -> Result<Self::Value, Self::Error>;
    fn spread(&self, candle: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn make_candle(&self, candle: FeedCandle<Self::Value>) -> Result<Self::Value, Self::Error>;
    fn write_candle(&self, key: &[u16], candle: Self::Value) -> Result<(), Self::Error>;
}

fn text(parts: &[&[u16]]) -> Vec<u16> {
    parts.iter().flat_map(|p| p.iter().copied()).collect()
}
fn u(value: &str) -> Vec<u16> {
    value.encode_utf16().collect()
}
fn fallback<H: PublicIngressHost>(h: &H, v: H::Value, default: &str) -> H::Value {
    if h.truthy(&v) { v } else { h.literal(default) }
}
fn first<H: PublicIngressHost>(
    h: &H,
    v: &H::Value,
    keys: &[&str],
    default: &str,
) -> Result<H::Value, H::Error> {
    for key in keys {
        let value = h.get(v, key)?;
        if h.truthy(&value) {
            return Ok(value);
        }
    }
    Ok(h.literal(default))
}
fn records<H: PublicIngressHost>(h: &H, data: &H::Value) -> Result<H::Value, H::Error> {
    if h.is_array(&h.optional_get(data, "data")?) {
        h.get(data, "data")
    } else {
        let raw = h.optional_get(data, "data")?;
        Ok(h.array(vec![if h.truthy(&raw) { raw } else { data.clone() }]))
    }
}
fn positive(fields: &mut Vec<(&'static str, f64)>, key: &'static str, value: f64) {
    if value.is_finite() && value > 0.0 {
        fields.push((key, value));
    }
}

// ts: ParadexPublicWebSocket#handleBBOUpdate
pub fn handle_bbo_update<H: PublicIngressHost>(h: &H, data: &H::Value) -> Result<(), H::Error> {
    let result = (|| {
        let raw = h.optional_get(data, "data")?;
        if !h.truthy(&raw) {
            return Ok(());
        }
        let channel = h.string(&fallback(h, h.optional_get(data, "channel")?, ""))?;
        let channel_symbol = channel.split(|c| *c == 46).nth(1).unwrap_or(&[]);
        let market = h.get(&raw, "market")?;
        let symbol = h.string(&if h.truthy(&market) {
            market
        } else {
            h.text_value(channel_symbol.to_vec())
        })?;
        if symbol.is_empty() {
            return Ok(());
        }
        let bid = h.parse_float(&first(h, &raw, &["bid_price"], "0")?)?;
        let ask = h.parse_float(&first(h, &raw, &["ask_price"], "0")?)?;
        let index = h.parse_float(&first(h, &raw, &["index_price"], "0")?)?;
        let mut fields = Vec::new();
        positive(&mut fields, "bid", bid);
        positive(&mut fields, "ask", ask);
        positive(&mut fields, "index", index);
        h.update_ticker(FeedTicker { symbol, fields })
    })();
    match result {
        Ok(()) => Ok(()),
        Err(e) => h.caught("Error handling BBO update: ", e),
    }
}

// ts: ParadexPublicWebSocket#handleMarketSummaryUpdate
pub fn handle_summary_update<H: PublicIngressHost>(h: &H, data: &H::Value) -> Result<(), H::Error> {
    let result = (|| {
        let raw = records(h, data)?;
        h.iterate(&raw, &mut |summary| {
            if !h.truthy(&summary) || !h.is_object(&summary) {
                return Ok(());
            }
            let symbol = h.string(&first(h, &summary, &["symbol", "market"], "")?)?;
            if symbol.is_empty() {
                return Ok(());
            }
            let specs: &[(&str, &[&str])] = &[
                ("bid", &["bid"]),
                ("ask", &["ask"]),
                ("last", &["last_traded_price", "last_price", "last"]),
                ("mark", &["mark_price"]),
                (
                    "index",
                    &["oracle_price", "index_price", "underlying_price"],
                ),
                ("percentage", &["price_change_rate_24h"]),
                ("volume", &["volume_24h", "volume"]),
                ("openInterest", &["open_interest"]),
                ("fundingRate", &["funding_rate"]),
            ];
            let mut parsed = Vec::new();
            for (_, keys) in specs {
                parsed.push(h.parse_float(&first(h, &summary, keys, "0")?)?);
            }
            let mut fields = Vec::new();
            for ((key, _), value) in specs.iter().zip(parsed) {
                if ["bid", "ask", "last", "mark", "index"].contains(key) {
                    positive(&mut fields, key, value);
                } else if value.is_finite() {
                    fields.push((
                        key,
                        if *key == "percentage" {
                            value * 100.0
                        } else {
                            value
                        },
                    ));
                }
            }
            h.update_ticker(FeedTicker { symbol, fields })
        })
    })();
    match result {
        Ok(()) => Ok(()),
        Err(e) => h.caught("Error handling markets_summary update: ", e),
    }
}

// ts: ParadexPublicWebSocket#handleTradesUpdate
pub fn handle_trades_update<H: PublicIngressHost>(h: &H, data: &H::Value) -> Result<(), H::Error> {
    let result = (|| {
        let raw = records(h, data)?;
        let channel = h.string(&fallback(h, h.optional_get(data, "channel")?, ""))?;
        let channel_symbol = channel.split(|c| *c == 46).nth(1).unwrap_or(&[]);
        if h.equal(&h.get(&raw, "length")?, &h.number(0.0)) {
            let logger = h.select_log()?;
            return h.log(
                logger,
                text(&[
                    &u("Trades data is undefined. Full data: "),
                    &h.stringify(data)?,
                ]),
                "warning",
            );
        }
        h.iterate(&raw, &mut |trade| {
            let trade = if h.truthy(&trade) && h.is_object(&trade) {
                trade
            } else {
                h.empty_object()
            };
            if h.object_keys_empty(&trade)? {
                return Ok(());
            }
            let market = h.get(&trade, "market")?;
            let symbol = h.string(&if h.truthy(&market) {
                market
            } else {
                h.text_value(channel_symbol.to_vec())
            })?;
            if symbol.is_empty() {
                let logger = h.select_log()?;
                return h.log(
                    logger,
                    text(&[
                        &u("Trade missing market symbol. Data: "),
                        &h.stringify(&trade)?,
                    ]),
                    "warning",
                );
            }
            let price = h.parse_float(&first(h, &trade, &["price"], "0")?)?;
            let size = h.parse_float(&first(h, &trade, &["size"], "0")?)?;
            let created = h.get(&trade, "created_at")?;
            let created = if h.is_undefined(&created) || h.is_null(&created) {
                h.get(&trade, "timestamp")?
            } else {
                created
            };
            let created = h.to_number(&created)?;
            let timestamp = if created.is_finite() && created > 0.0 {
                created
            } else {
                h.now()?
            };
            if price.is_finite() && price > 0.0 {
                h.update_ticker(FeedTicker {
                    symbol: symbol.clone(),
                    fields: vec![("last", price), ("mark", price)],
                })?;
                h.update_candle_from_trade(
                    symbol,
                    price,
                    if size.is_finite() { size } else { 0.0 },
                    timestamp,
                )
            } else {
                let logger = h.select_log()?;
                h.log(
                    logger,
                    text(&[
                        &u("Trade price is 0 or invalid: "),
                        &h.template(&h.get(&trade, "price")?)?,
                    ]),
                    "warning",
                )
            }
        })
    })();
    match result {
        Ok(()) => Ok(()),
        Err(e) => h.caught("Error handling trades update: ", e),
    }
}

// ts: ParadexPublicWebSocket.onMessage
pub fn on_public_message<H: PublicIngressHost>(h: &H, event: &H::Value) -> Result<(), H::Error> {
    if h.disposed()? {
        return Ok(());
    }
    h.track()?;
    let data = h.parse_event(event)?;
    if !h.truthy(&data) {
        return Ok(());
    }
    if h.equal(&h.get(&data, "jsonrpc")?, &h.literal("2.0")) {
        if h.equal(&h.get(&data, "method")?, &h.literal("subscription"))
            && h.truthy(&h.get(&data, "params")?)
        {
            let channel = h.get(&h.get(&data, "params")?, "channel")?;
            let parts = h.split_optional(&channel, '.')?;
            let topic = h.get(&parts, "0")?;
            let symbol = h.get(&parts, "1")?;
            let handlers = h.handlers(&topic)?;
            if h.truthy(&handlers) {
                if h.truthy(&symbol) && h.equal(&topic, &h.literal("order_book")) {
                    let value = text(&[&h.template(&topic)?, &u("."), &h.template(&symbol)?]);
                    h.stats(h.text_value(value))?;
                } else {
                    h.stats(topic.clone())?;
                }
                h.each(&handlers, &mut |handler| {
                    h.call_handler(handler, h.get(&data, "params")?)
                })?;
            } else if h.truthy(&topic) {
                let logger = h.select_log()?;
                h.log(
                    logger,
                    text(&[
                        &u("No handler for channel type: "),
                        &h.template(&topic)?,
                        &u(" (channel: "),
                        &h.template(&channel)?,
                        &u(")"),
                    ]),
                    "warning",
                )?;
            }
        } else if !h.is_undefined(&h.get(&data, "id")?) {
            let pending = h.pending(h.get(&data, "id")?)?;
            if h.truthy(&pending) {
                h.delete_pending(h.get(&data, "id")?)?;
                if h.truthy(&h.get(&data, "error")?) {
                    let channel = h.get(&pending, "channel")?;
                    let attempt = h.get(&pending, "attempt")?;
                    let reason = h.optional_get(&h.get(&data, "error")?, "message")?;
                    h.retry(channel, attempt, reason)?;
                }
            }
            if h.truthy(&h.get(&data, "error")?) {
                let logger = h.select_log()?;
                h.log(
                    logger,
                    text(&[
                        &u("WebSocket error for request "),
                        &h.template(&h.get(&data, "id")?)?,
                        &u(": "),
                        &h.template(&h.get(&h.get(&data, "error")?, "message")?)?,
                    ]),
                    "error",
                )?;
            }
        }
    } else {
        let topic = h.get(&data, "type")?;
        let topic = if h.truthy(&topic) {
            topic
        } else {
            h.get(&data, "channel")?
        };
        let handlers = h.handlers(&topic)?;
        if h.truthy(&handlers) {
            h.stats(topic)?;
            h.each(&handlers, &mut |handler| {
                h.call_handler(handler, data.clone())
            })?;
        }
    }
    Ok(())
}

fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_positive() || b.is_sign_positive() {
            0.0
        } else {
            -0.0
        }
    } else if a > b {
        a
    } else {
        b
    }
}
fn js_min(a: f64, b: f64) -> f64 {
    -js_max(-a, -b)
}

// ts: ParadexPublicWebSocket#updateOHLCVFromTrade
pub fn update_ohlcv_from_trade<H: PublicIngressHost>(
    h: &H,
    symbol: &[u16],
    price: f64,
    size: f64,
    timestamp: f64,
) -> Result<(), H::Error> {
    h.visit_candles(&mut |key, callbacks| {
        if key.split(|c| *c == 58).next().unwrap_or(&[]) != symbol {
            return Ok(());
        }
        let Some(interval) = h.interval(&key)?.filter(|v| *v != 0.0 && !v.is_nan()) else {
            return Ok(());
        };
        h.write_trade_time(&key, h.now()?)?;
        let previous = h.candle(&key)?;
        let stamp = (timestamp / interval).floor() * interval;
        let candle = if !h.truthy(&previous)
            || !h.equal(&h.get(&previous, "timestamp")?, &h.number(stamp))
        {
            FeedCandle {
                spread: None,
                timestamp: Some(h.number(stamp)),
                open: Some(h.number(price)),
                high: h.number(price),
                low: h.number(price),
                close: price,
                volume: h.number(size),
            }
        } else {
            let spread = h.spread(&previous)?;
            let high = js_max(h.coerce_number(&h.get(&previous, "high")?)?, price);
            let low = js_min(h.coerce_number(&h.get(&previous, "low")?)?, price);
            let volume = h.add(&h.get(&previous, "volume")?, size)?;
            FeedCandle {
                timestamp: None,
                open: None,
                spread: Some(spread),
                high: h.number(high),
                low: h.number(low),
                close: price,
                volume,
            }
        };
        let current = h.make_candle(candle)?;
        let previous = h.candle(&key)?;
        let mut changed = !h.truthy(&previous);
        for field in ["timestamp", "close", "high", "low", "volume"] {
            if changed {
                break;
            }
            changed = !h.equal(&h.get(&previous, field)?, &h.get(&current, field)?);
        }
        if changed {
            h.write_candle(&key, current.clone())?;
            h.each(&callbacks, &mut |callback| {
                h.call_handler(callback, current.clone())
            })?;
        }
        Ok(())
    })
}

// ts: ParadexPublicWebSocket#maybeRecoverOhlcvTradeStream
pub fn maybe_recover_ohlcv_trade_stream<E>(
    now: impl FnOnce() -> Result<f64, E>,
    trade_time: impl FnOnce() -> Result<Option<f64>, E>,
    recovery_time: impl FnOnce() -> Result<Option<f64>, E>,
    write_recovery: impl FnOnce(f64) -> Result<(), E>,
    warning_arguments: impl FnOnce() -> Result<(Vec<u16>, Vec<u16>), E>,
    warn: impl FnOnce(Vec<u16>) -> Result<(), E>,
    subscribe: impl FnOnce(bool) -> Result<(), E>,
) -> Result<(), E> {
    let now = now()?;
    let last = trade_time()?
        .filter(|n| *n != 0.0 && !n.is_nan())
        .unwrap_or(0.0);
    if last > 0.0 && now - last <= 20_000.0 {
        return Ok(());
    }
    let recovery = recovery_time()?
        .filter(|n| *n != 0.0 && !n.is_nan())
        .unwrap_or(0.0);
    if now - recovery < 30_000.0 {
        return Ok(());
    }
    write_recovery(now)?;
    let (symbol, interval) = warning_arguments()?;
    warn(text(&[
        &u("No Paradex trade updates for "),
        &symbol,
        &u(" "),
        &interval,
        &u("; re-subscribing trade channel"),
    ]))?;
    subscribe(true)
}

// ts: ParadexPublicWebSocket#retrySubscribeChannel
pub fn retry_subscribe_channel<T, E>(
    channel: &[u16],
    attempt: f64,
    reason: Option<&[u16]>,
    active: impl FnOnce() -> Result<bool, E>,
    remove_sent: impl FnOnce(&[u16]) -> Result<(), E>,
    warn: impl FnOnce(Vec<u16>) -> Result<(), E>,
    schedule: impl FnOnce(f64, f64) -> Result<T, E>,
    add_timeout: impl FnOnce(T) -> Result<(), E>,
) -> Result<(), E> {
    if attempt >= 2.0 || !active()? {
        return Ok(());
    }
    if channel.starts_with(&u("trades.")) {
        if let Some(symbol) = channel.split(|c| *c == 46).nth(1).filter(|s| !s.is_empty()) {
            remove_sent(symbol)?;
        }
    }
    let suffix = reason
        .filter(|r| !r.is_empty())
        .map(|r| text(&[&u(": "), r]))
        .unwrap_or_default();
    warn(text(&[
        &u("Retrying Paradex subscription "),
        channel,
        &u(" after error"),
        &suffix,
    ]))?;
    let timer = schedule(1_000.0 * (attempt + 1.0), attempt + 1.0)?;
    add_timeout(timer)
}

// ts: ParadexPublicWebSocket#retrySubscribeChannel
pub fn execute_subscription_retry<E>(
    delete_timeout: impl FnOnce() -> Result<(), E>,
    disposed: impl FnOnce() -> Result<bool, E>,
    active: impl FnOnce() -> Result<bool, E>,
    send: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    delete_timeout()?;
    if disposed()? || !active()? {
        return Ok(());
    }
    send()
}
