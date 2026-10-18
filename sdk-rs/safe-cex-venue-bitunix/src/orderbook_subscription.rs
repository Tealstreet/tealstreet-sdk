//! Futures snapshots preserve the callback's shared book identity and wait timer.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsJsonValue, js_json_stringify, js_string, parse_float};
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Debug)]
pub struct BookLevel {
    pub price: f64,
    pub amount: f64,
    pub total: f64,
}
#[derive(Default, Debug)]
pub struct OrderBook {
    pub bids: Vec<BookLevel>,
    pub asks: Vec<BookLevel>,
}
pub trait OrderBookHost<C> {
    type Parser: Clone + PartialEq;
    type Timer;
    fn precision(&mut self, symbol: &str) -> Result<Option<Value>, ExchangeError>;
    fn allocate_parser(&mut self) -> Self::Parser;
    fn connected(&mut self) -> Result<bool, ExchangeError>;
    fn handlers(&mut self, handler: &str) -> Result<Vec<Self::Parser>, ExchangeError>;
    fn set_handlers(
        &mut self,
        handler: &str,
        parsers: Vec<Self::Parser>,
    ) -> Result<(), ExchangeError>;
    fn topic_count(&mut self, topic: &str) -> Result<f64, ExchangeError>;
    fn set_topic_count(&mut self, topic: &str, count: f64) -> Result<(), ExchangeError>;
    fn send(&mut self, frame: &JsJsonValue) -> Result<(), ExchangeError>;
    fn wait(&mut self, delay: f64) -> Result<Self::Timer, ExchangeError>;
    fn timer_truthy(&mut self, timer: &Self::Timer) -> bool;
    fn clear(&mut self, timer: Self::Timer) -> Result<(), ExchangeError>;
    fn cleanup_stats(&mut self, handler: &str) -> Result<(), ExchangeError>;
    fn call(&mut self, callback: &C, book: Rc<RefCell<OrderBook>>) -> Result<(), ExchangeError>;
}
pub struct BookSubscription<C, P, T> {
    pub callback: C,
    pub parser: P,
    pub handler: String,
    pub topic: String,
    pub frame: JsJsonValue,
    pub book: Rc<RefCell<OrderBook>>,
    pub timeout: Option<T>,
}
pub fn listen_order_book<C, H: OrderBookHost<C>>(
    host: &mut H,
    symbol: &str,
    callback: C,
) -> Result<BookSubscription<C, H::Parser, H::Timer>, ExchangeError> {
    let precision = host.precision(symbol)?;
    let precision = js_string(precision.as_ref()).map_err(ExchangeError::from)?;
    let handler = format!("market_{}_depthop_{precision}", symbol.to_lowercase());
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
        .ok_or_else(|| ExchangeError::new("Missing frame"))?;
    let mut subscription = BookSubscription {
        callback,
        parser: host.allocate_parser(),
        handler,
        topic,
        frame,
        book: Rc::new(RefCell::new(OrderBook::default())),
        timeout: None,
    };
    subscription.subscribe(host)?;
    Ok(subscription)
}
impl<C, P: Clone + PartialEq, T> BookSubscription<C, P, T> {
    pub fn subscribe<H: OrderBookHost<C, Parser = P, Timer = T>>(
        &mut self,
        host: &mut H,
    ) -> Result<(), ExchangeError> {
        if host.connected()? {
            let mut handlers = host.handlers(&self.handler)?;
            handlers.push(self.parser.clone());
            host.set_handlers(&self.handler, handlers)?;
            let count = host.topic_count(&self.topic)?;
            host.set_topic_count(
                &self.topic,
                if count == 0.0 || count.is_nan() {
                    1.0
                } else {
                    count + 1.0
                },
            )?;
            host.send(&self.frame)?;
        } else {
            self.timeout = Some(host.wait(100.0)?);
        }
        Ok(())
    }
    pub fn cancel<H: OrderBookHost<C, Parser = P, Timer = T>>(
        &mut self,
        host: &mut H,
    ) -> Result<(), ExchangeError> {
        let handlers = host
            .handlers(&self.handler)?
            .into_iter()
            .filter(|h| h != &self.parser)
            .collect();
        host.set_handlers(&self.handler, handlers)?;
        self.book.borrow_mut().asks = Vec::new();
        self.book.borrow_mut().bids = Vec::new();
        if self.timeout.as_ref().is_some_and(|t| host.timer_truthy(t)) {
            host.clear(self.timeout.take().unwrap())?;
        }
        let count = host.topic_count(&self.topic)?;
        host.set_topic_count(&self.topic, count - 1.0)?;
        if host.connected()? && host.handlers(&self.handler)?.is_empty() {
            host.cleanup_stats(&self.handler)?;
        }
        Ok(())
    }
    pub fn parse<H: OrderBookHost<C, Parser = P, Timer = T>>(
        &self,
        host: &mut H,
        raw: &Value,
    ) -> Result<(), ExchangeError> {
        let data = raw
            .get("data")
            .ok_or_else(|| ExchangeError::new("Missing book data"))?;
        self.book.borrow_mut().bids = levels(data.get("bids"))?;
        self.book.borrow_mut().asks = levels(data.get("asks"))?;
        {
            let mut book = self.book.borrow_mut();
            let OrderBook { bids, asks } = &mut *book;
            for side in [bids, asks] {
                let mut total = 0.0;
                for level in side {
                    total += level.amount;
                    level.total = total;
                }
            }
        }
        host.call(&self.callback, self.book.clone())
    }
}
fn levels(value: Option<&Value>) -> Result<Vec<BookLevel>, ExchangeError> {
    value
        .and_then(Value::as_array)
        .ok_or_else(|| ExchangeError::new("Book side must be an array"))?
        .iter()
        .map(|row| {
            let row = row
                .as_array()
                .ok_or_else(|| ExchangeError::new("Book level must be an array"))?;
            let price = js_string(row.first()).map_err(ExchangeError::from)?;
            let amount = js_string(row.get(1)).map_err(ExchangeError::from)?;
            Ok(BookLevel {
                price: parse_float(&price),
                amount: parse_float(&amount),
                total: 0.0,
            })
        })
        .collect()
}
