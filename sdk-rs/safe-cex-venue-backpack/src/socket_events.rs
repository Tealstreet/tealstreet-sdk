//! Inbound socket bodies over native values and compatible property/call operations.
use crate::public_control::JsString;
use safe_cex_num::parse_float;
use std::{cell::RefCell, rc::Rc};

pub trait ErrorSink {
    type Error;
    type Emitter;
    fn error_emitter(&mut self) -> Result<Self::Emitter, Self::Error>;
    fn error_text(&mut self, error: &Self::Error) -> Result<JsString, Self::Error>;
    fn emit_error(&mut self, emitter: Self::Emitter, message: JsString) -> Result<(), Self::Error>;
}
pub(crate) fn report<H: ErrorSink>(
    host: &mut H,
    prefix: &str,
    error: &H::Error,
) -> Result<(), H::Error> {
    let emitter = host.error_emitter()?;
    let mut text: JsString = prefix.encode_utf16().collect();
    text.extend(host.error_text(error)?);
    host.emit_error(emitter, text)
}
fn join(prefix: &str, parts: &[&[u16]]) -> JsString {
    let mut text: JsString = prefix.encode_utf16().collect();
    for part in parts {
        text.extend_from_slice(part);
    }
    text
}

pub trait MessageValue: Clone {
    fn truthy(&self) -> bool;
    fn is_null(&self) -> bool;
    fn equals_number(&self, number: f64) -> bool;
    fn equals_string(&self, text: &str) -> bool;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Depth,
    Ticker,
    Trade,
    BookTicker,
    Kline,
    Order,
    Position,
}
pub trait MessageHost<D>: ErrorSink {
    type Value: MessageValue;
    type Dispatcher;
    /// Captures the live handler before evaluating its argument property.
    fn dispatch_method(&mut self, kind: EventKind) -> Result<Self::Dispatcher, Self::Error>;
    fn is_disposed(&mut self) -> Result<bool, Self::Error>;
    fn track_last_received(&mut self) -> Result<(), Self::Error>;
    fn parse(&mut self, data: &D) -> Result<Self::Value, Self::Error>;
    fn property(&mut self, value: &Self::Value, name: &str) -> Result<Self::Value, Self::Error>;
    fn stringify(&mut self, value: &Self::Value) -> Result<JsString, Self::Error>;
    fn starts_with(&mut self, value: &Self::Value, prefix: &str) -> Result<bool, Self::Error>;
    fn pong(&mut self, interval: f64) -> Result<(), Self::Error>;
    fn dispatch(
        &mut self,
        method: Self::Dispatcher,
        data: Self::Value,
        stream: Option<Self::Value>,
    ) -> Result<(), Self::Error>;
}
fn message<D, H: MessageHost<D>>(data: &D, host: &mut H, private: bool) -> Result<(), H::Error> {
    if host.is_disposed()? {
        return Ok(());
    }
    host.track_last_received()?;
    let result = (|| {
        let json = host.parse(data)?;
        if !json.truthy() {
            return Ok(());
        }
        if host.property(&json, "result")?.is_null() && host.property(&json, "id")?.truthy() {
            return Ok(());
        }
        if host.property(&json, "error")?.truthy() {
            let error = host.property(&json, "error")?;
            if host.property(&error, "code")?.equals_number(4002.0) {
                host.pong(10_000.0)?;
                return Ok(());
            }
            let emitter = host.error_emitter()?;
            let error = host.property(&json, "error")?;
            let text = host.stringify(&error)?;
            let prefix = if private {
                "Private WebSocket error: "
            } else {
                "WebSocket error: "
            };
            host.emit_error(emitter, join(prefix, &[&text]))?;
            return Ok(());
        }
        if private {
            if host.property(&json, "stream")?.truthy() && host.property(&json, "data")?.truthy() {
                let stream = host.property(&json, "stream")?;
                if stream.equals_string("account.orderUpdate")
                    || host.starts_with(&stream, "account.orderUpdate.")?
                {
                    let method = host.dispatch_method(EventKind::Order)?;
                    let event = host.property(&json, "data")?;
                    host.dispatch(method, event, None)?;
                } else if stream.equals_string("account.positionUpdate") {
                    let method = host.dispatch_method(EventKind::Position)?;
                    let event = host.property(&json, "data")?;
                    host.dispatch(method, event, None)?;
                }
            }
        } else {
            let candidate = host.property(&json, "data")?;
            let event = if candidate.truthy() {
                candidate
            } else {
                json.clone()
            };
            let stream = host.property(&json, "stream")?;
            if !host.property(&event, "e")?.truthy() {
                return Ok(());
            }
            let kind = host.property(&event, "e")?;
            let selected = [
                ("depth", EventKind::Depth),
                ("ticker", EventKind::Ticker),
                ("trade", EventKind::Trade),
                ("bookTicker", EventKind::BookTicker),
                ("kline", EventKind::Kline),
            ]
            .into_iter()
            .find(|(name, _)| kind.equals_string(name));
            if let Some((_, selected)) = selected {
                let stream = (selected == EventKind::Kline).then_some(stream);
                let method = host.dispatch_method(selected)?;
                host.dispatch(method, event, stream)?;
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(()),
        Err(error) => report(
            host,
            if private {
                "Error parsing private WebSocket message: "
            } else {
                "Error parsing WebSocket message: "
            },
            &error,
        ),
    }
}
// ts: BackpackPublicWebSocket.onMessage
pub fn on_public_message<D, H: MessageHost<D>>(data: &D, host: &mut H) -> Result<(), H::Error> {
    message(data, host, false)
}
// ts: BackpackPrivateWebSocket.onMessage
pub fn on_private_message<D, H: MessageHost<D>>(data: &D, host: &mut H) -> Result<(), H::Error> {
    message(data, host, true)
}

pub trait HandlerHost<D>: ErrorSink {
    type List;
    type Handler;
    type Throttle;
    type ThrottleFactory;
    /// Captures the live factory before rereading the depth symbol.
    fn throttle_factory(&mut self) -> Result<Self::ThrottleFactory, Self::Error>;
    fn symbol(&mut self, data: &D) -> Result<JsString, Self::Error>;
    fn interval(&mut self, data: &D) -> Result<JsString, Self::Error>;
    fn handlers(&mut self, topic: &[u16]) -> Result<Option<Self::List>, Self::Error>;
    fn length(&mut self, list: &Self::List) -> Result<usize, Self::Error>;
    fn handler(
        &mut self,
        list: &Self::List,
        index: usize,
    ) -> Result<Option<Self::Handler>, Self::Error>;
    fn record_stats(&mut self, topic: &[u16]) -> Result<(), Self::Error>;
    fn invoke_handler(&mut self, handler: Self::Handler, data: &D) -> Result<(), Self::Error>;
    fn is_null(&mut self, data: &D, field: &str) -> Result<bool, Self::Error>;
    fn depth_throttle(
        &mut self,
        factory: Self::ThrottleFactory,
        symbol: &[u16],
    ) -> Result<Self::Throttle, Self::Error>;
    fn invoke_throttle(&mut self, throttle: Self::Throttle, data: &D) -> Result<(), Self::Error>;
}
// ts: BackpackPublicWebSocket#handleDepthUpdate
pub fn handle_depth_update<D, H: HandlerHost<D>>(data: &D, host: &mut H) -> Result<(), H::Error> {
    let symbol = host.symbol(data)?;
    let topic = join("depth.", &[&symbol]);
    if let Some(handlers) = host.handlers(&topic)? {
        if host.length(&handlers)? > 0 {
            host.record_stats(&topic)?;
            let length = host.length(&handlers)?;
            for index in 0..length {
                if let Some(handler) = host.handler(&handlers, index)? {
                    if let Err(error) = host.invoke_handler(handler, data) {
                        report(host, "Error handling depth update: ", &error)?;
                    }
                }
            }
        }
    }
    let factory = host.throttle_factory()?;
    let symbol = host.symbol(data)?;
    let throttle = host.depth_throttle(factory, &symbol)?;
    host.invoke_throttle(throttle, data)
}
// ts: BackpackPublicWebSocket#handleKlineUpdate
pub fn handle_kline_update<D, H: HandlerHost<D>>(
    data: &D,
    stream: Option<&[u16]>,
    host: &mut H,
) -> Result<(), H::Error> {
    let result = (|| {
        let topic = if let Some(stream) = stream.filter(|s| !s.is_empty()) {
            stream.to_vec()
        } else {
            let interval = host.interval(data)?;
            let symbol = host.symbol(data)?;
            join("kline.", &[&interval, &[46], &symbol])
        };
        if let Some(handlers) = host.handlers(&topic)? {
            if host.length(&handlers)? > 0 {
                if host.is_null(data, "o")? || host.is_null(data, "c")? {
                    return Ok(());
                }
                host.record_stats(&topic)?;
                let length = host.length(&handlers)?;
                for index in 0..length {
                    if let Some(handler) = host.handler(&handlers, index)? {
                        host.invoke_handler(handler, data)?;
                    }
                }
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(()),
        Err(error) => report(host, "Error handling kline update: ", &error),
    }
}

#[derive(Debug, Default, Clone)]
pub struct TickerPatch {
    pub last: Option<f64>,
    pub percentage: Option<f64>,
    pub volume: Option<f64>,
    pub quote_volume: Option<f64>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
}
impl TickerPatch {
    pub fn is_empty(&self) -> bool {
        self.last.is_none()
            && self.percentage.is_none()
            && self.volume.is_none()
            && self.quote_volume.is_none()
            && self.bid.is_none()
            && self.ask.is_none()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Bids,
    Asks,
}
pub trait TickerHost<D>: ErrorSink {
    type Ticker;
    fn ticker(&mut self, data: &D) -> Result<Option<Self::Ticker>, Self::Error>;
    /// Returns the field's JavaScript ToString value, propagating coercion failures.
    fn field_text(&mut self, data: &D, field: &str) -> Result<JsString, Self::Error>;
    fn levels_len(&mut self, data: &D, side: Side) -> Result<usize, Self::Error>;
    fn first_price(&mut self, data: &D, side: Side) -> Result<JsString, Self::Error>;
    fn update_ticker(
        &mut self,
        ticker: Self::Ticker,
        update: TickerPatch,
    ) -> Result<(), Self::Error>;
}
fn number(text: &[u16]) -> f64 {
    parse_float(&String::from_utf16_lossy(text))
}
fn valid(value: f64) -> Option<f64> {
    (!value.is_nan()).then_some(value)
}
fn guarded<D, H: TickerHost<D>>(
    host: &mut H,
    prefix: &str,
    body: impl FnOnce(&mut H) -> Result<(), H::Error>,
) -> Result<(), H::Error> {
    match body(host) {
        Ok(()) => Ok(()),
        Err(error) => report(host, prefix, &error),
    }
}
// ts: BackpackPublicWebSocket#updateTickerFromDepth
pub fn update_ticker_from_depth<D, H: TickerHost<D>>(
    data: &D,
    host: &mut H,
) -> Result<(), H::Error> {
    guarded(host, "Error updating ticker from depth: ", |host| {
        if let Some(ticker) = host.ticker(data)? {
            if host.levels_len(data, Side::Bids)? > 0 || host.levels_len(data, Side::Asks)? > 0 {
                let bid = if host.levels_len(data, Side::Bids)? > 0 {
                    number(&host.first_price(data, Side::Bids)?)
                } else {
                    f64::NAN
                };
                let ask = if host.levels_len(data, Side::Asks)? > 0 {
                    number(&host.first_price(data, Side::Asks)?)
                } else {
                    f64::NAN
                };
                let patch = TickerPatch {
                    bid: valid(bid),
                    ask: valid(ask),
                    ..Default::default()
                };
                if !patch.is_empty() {
                    host.update_ticker(ticker, patch)?;
                }
            }
        }
        Ok(())
    })
}
fn ticker_update<D, H: TickerHost<D>>(data: &D, host: &mut H) -> Result<(), H::Error> {
    guarded(host, "Error handling ticker update: ", |host| {
        let Some(ticker) = host.ticker(data)? else {
            return Ok(());
        };
        let last = number(&host.field_text(data, "c")?);
        let percentage = number(&host.field_text(data, "P")?);
        let volume = number(&host.field_text(data, "v")?);
        let quote_volume = number(&host.field_text(data, "q")?);
        let patch = TickerPatch {
            last: valid(last),
            percentage: valid(percentage).map(|p| p * 100.0),
            volume: valid(volume),
            quote_volume: valid(quote_volume),
            ..Default::default()
        };
        if !patch.is_empty() {
            host.update_ticker(ticker, patch)?;
        }
        Ok(())
    })
}
pub type TickerCallback<D, E> = Rc<dyn Fn(&D) -> Result<(), E>>;
// ts: BackpackPublicWebSocket#handleTickerUpdate
pub fn create_ticker_handler<D: 'static, H: TickerHost<D> + 'static, R>(
    host: Rc<RefCell<H>>,
    throttle: impl FnOnce(TickerCallback<D, H::Error>, f64, bool, bool) -> Result<R, H::Error>,
) -> Result<R, H::Error>
where
    H::Error: 'static,
{
    let callback: TickerCallback<D, H::Error> =
        Rc::new(move |data| ticker_update(data, &mut *host.borrow_mut()));
    throttle(callback, 1000.0, true, true)
}
// ts: BackpackPublicWebSocket#handleTradeUpdate
pub fn handle_trade_update<D, H: TickerHost<D>>(data: &D, host: &mut H) -> Result<(), H::Error> {
    guarded(host, "Error handling trade update: ", |host| {
        let Some(ticker) = host.ticker(data)? else {
            return Ok(());
        };
        let last = number(&host.field_text(data, "p")?);
        if !last.is_nan() {
            host.update_ticker(
                ticker,
                TickerPatch {
                    last: Some(last),
                    ..Default::default()
                },
            )?;
        }
        Ok(())
    })
}
// ts: BackpackPublicWebSocket#handleBookTickerUpdate
pub fn handle_book_ticker_update<D, H: TickerHost<D>>(
    data: &D,
    host: &mut H,
) -> Result<(), H::Error> {
    guarded(host, "Error handling book ticker update: ", |host| {
        let Some(ticker) = host.ticker(data)? else {
            return Ok(());
        };
        let bid = number(&host.field_text(data, "b")?);
        let ask = number(&host.field_text(data, "a")?);
        let patch = TickerPatch {
            bid: valid(bid),
            ask: valid(ask),
            ..Default::default()
        };
        if !patch.is_empty() {
            host.update_ticker(ticker, patch)?;
        }
        Ok(())
    })
}
