//! Futures candle subscription over shared strings, JSON and Candle values.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsJsonValue, js_json_stringify, js_number, js_string, parse_float};
use safe_cex_types::Candle;
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};

pub struct OhlcvOptions {
    pub symbol: String,
    pub interval: String,
}
pub trait OhlcvHost<C> {
    type Parser: Clone + PartialEq;
    type Ticker;
    fn allocate_parser(&mut self) -> Self::Parser;
    fn connected(&mut self) -> Result<bool, ExchangeError>;
    fn handlers(&mut self, handler: &str) -> Result<Vec<Self::Parser>, ExchangeError>;
    fn set_handlers(
        &mut self,
        handler: &str,
        handlers: Vec<Self::Parser>,
    ) -> Result<(), ExchangeError>;
    fn topic_count(&mut self, topic: &str) -> Result<f64, ExchangeError>;
    fn set_topic_count(&mut self, topic: &str, count: f64) -> Result<(), ExchangeError>;
    fn send(&mut self, frame: &JsJsonValue) -> Result<(), ExchangeError>;
    fn wait(&mut self, delay: f64) -> Result<(), ExchangeError>;
    fn cleanup_stats(&mut self, handler: &str) -> Result<(), ExchangeError>;
    fn ticker(&mut self, symbol: &str) -> Result<Option<Self::Ticker>, ExchangeError>;
    fn update_last(&mut self, ticker: Self::Ticker, last: f64) -> Result<(), ExchangeError>;
    fn call(&mut self, callback: &C, candle: Candle) -> Result<(), ExchangeError>;
}
pub struct OhlcvSubscription<C, P> {
    pub options: Rc<RefCell<OhlcvOptions>>,
    pub callback: C,
    pub parser: P,
    pub handler: String,
    pub topic: String,
    pub frame: JsJsonValue,
}
fn interval(input: &str) -> &'static str {
    crate::websocket_public::INTERVAL
        .iter()
        .find(|(key, _)| *key == input)
        .map_or("undefined", |(_, value)| *value)
}

fn float(value: Option<&Value>) -> Result<f64, ExchangeError> {
    Ok(parse_float(&js_string(value).map_err(ExchangeError::from)?))
}

pub fn listen_ohlcv<C, H: OhlcvHost<C>>(
    host: &mut H,
    options: Rc<RefCell<OhlcvOptions>>,
    callback: C,
) -> Result<OhlcvSubscription<C, H::Parser>, ExchangeError> {
    let handler = {
        let opts = options.borrow();
        format!(
            "market_{}_klineop_{}",
            opts.symbol.to_lowercase(),
            interval(&opts.interval)
        )
    };
    let frame = JsJsonValue::Object(vec![
        ("event".into(), JsJsonValue::String("sub".into())),
        (
            "params".into(),
            JsJsonValue::Object(vec![(
                "channel".into(),
                JsJsonValue::String(handler.clone()),
            )]),
        ),
    ]);
    let topic = js_json_stringify(&frame)
        .map_err(ExchangeError::from)?
        .ok_or_else(|| ExchangeError::new("Missing subscription frame"))?;
    let subscription = OhlcvSubscription {
        options,
        callback,
        parser: host.allocate_parser(),
        handler,
        topic,
        frame,
    };
    subscription.subscribe(host)?;
    Ok(subscription)
}
impl<C, P: Clone + PartialEq> OhlcvSubscription<C, P> {
    pub fn subscribe<H: OhlcvHost<C, Parser = P>>(
        &self,
        host: &mut H,
    ) -> Result<(), ExchangeError> {
        if host.connected()? {
            let mut handlers = host.handlers(&self.handler)?;
            handlers.push(self.parser.clone());
            host.set_handlers(&self.handler, handlers)?;
            let count = host.topic_count(&self.topic)?;
            let count = if count == 0.0 || count.is_nan() {
                0.0
            } else {
                count
            };
            host.set_topic_count(&self.topic, count + 1.0)?;
            host.send(&self.frame)?;
        } else {
            host.wait(100.0)?;
        }
        Ok(())
    }
    pub fn cancel<H: OhlcvHost<C, Parser = P>>(&self, host: &mut H) -> Result<(), ExchangeError> {
        let handlers = host
            .handlers(&self.handler)?
            .into_iter()
            .filter(|h| h != &self.parser)
            .collect();
        host.set_handlers(&self.handler, handlers)?;
        let count = host.topic_count(&self.topic)?;
        host.set_topic_count(&self.topic, count - 1.0)?;
        if host.connected()? && host.handlers(&self.handler)?.is_empty() {
            host.cleanup_stats(&self.handler)?;
        }
        Ok(())
    }
    pub fn parse<H: OhlcvHost<C, Parser = P>>(
        &self,
        host: &mut H,
        json: &Value,
    ) -> Result<(), ExchangeError> {
        let ticker = host.ticker(&self.options.borrow().symbol)?;
        if let Some(ticker) = ticker {
            host.update_last(ticker, float(field(json, "close")?)?)?;
        }
        let candle = Candle {
            timestamp: js_number(field(json, "time")?).map_err(ExchangeError::from)?,
            open: float(field(json, "open")?)?,
            high: float(field(json, "high")?)?,
            low: float(field(json, "low")?)?,
            close: float(field(json, "close")?)?,
            volume: float(field(json, "volume")?)?,
            last_side: None,
        };
        host.call(&self.callback, candle)
    }
}
fn field<'a>(json: &'a Value, key: &str) -> Result<Option<&'a Value>, ExchangeError> {
    if json.is_null() {
        return Err(ExchangeError::new("Cannot read candle data from null"));
    }
    let data = json
        .get("data")
        .ok_or_else(|| ExchangeError::new("Missing candle data"))?;
    if data.is_null() {
        return Err(ExchangeError::new("Cannot read candle fields from null"));
    }
    Ok(data.get(key))
}
