//! Candle/book listener bodies share the landed public receiver's topic and handler state.
use crate::{
    public_stream::{BingxPublicStream, MessageCallback},
    read_history::{candle, timeframe_name, venue_symbol},
};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsJsonValue as J, js_json_stringify, js_string, parse_float};
use safe_cex_types::{Candle, OHLCVOptions, OrderBook, OrderBookOrders};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub type Cleanup = Rc<dyn Fn() -> Result<(), ExchangeError>>;
pub type CandleCallback = Rc<dyn Fn(Candle) -> Result<(), ExchangeError>>;
pub type BookCallback = Rc<dyn Fn(Rc<RefCell<OrderBook>>) -> Result<(), ExchangeError>>;
pub trait SubscriptionHost {
    fn is_connected(&self) -> bool;
    fn set_timeout(
        &self,
        delay: f64,
        callback: Rc<dyn Fn() -> Result<(), ExchangeError>>,
    ) -> Result<u64, ExchangeError>;
    fn clear_timeout(&self, id: u64) -> Result<(), ExchangeError>;
    fn cleanup_stats(&self, topic: &str) -> Result<(), ExchangeError>;
}
pub struct BingxSubscriptions {
    pub stream: Rc<BingxPublicStream>,
    pub host: Rc<dyn SubscriptionHost>,
}
fn error(message: impl Into<String>) -> ExchangeError {
    let mut e = ExchangeError::new(message);
    e.name = "TypeError".into();
    e
}
fn levels(value: Option<&Value>, side: &str) -> Result<Vec<OrderBookOrders>, ExchangeError> {
    let rows = value.and_then(Value::as_array).ok_or_else(|| {
        error(match value {
            None => "Cannot read properties of undefined (reading 'map')".into(),
            Some(Value::Null) => "Cannot read properties of null (reading 'map')".into(),
            Some(_) => format!("data.{side}.map is not a function"),
        })
    })?;
    rows.iter()
        .map(|row| {
            if row.is_null() {
                return Err(error(
                    "Cannot destructure property 'price' of 'object null' as it is null.",
                ));
            }
            let float = |key| {
                js_string(row.get(key))
                    .map(|s| parse_float(&s))
                    .map_err(|e| error(e.message))
            };
            Ok(OrderBookOrders {
                price: float("price")?,
                amount: float("volume")?,
                total: 0.0,
            })
        })
        .collect()
}
impl BingxSubscriptions {
    pub fn new(stream: Rc<BingxPublicStream>, host: Rc<dyn SubscriptionHost>) -> Rc<Self> {
        Rc::new(Self { stream, host })
    }
    fn replace_handlers(&self, topic: &str, transform: impl FnOnce(&mut Vec<MessageCallback>)) {
        let mut entries = self.stream.message_handlers.borrow_mut();
        let old = entries.iter().position(|(name, _)| name == topic);
        let mut callbacks = old
            .map(|i| entries[i].1.borrow().clone())
            .unwrap_or_default();
        transform(&mut callbacks);
        let next = Rc::new(RefCell::new(callbacks));
        if let Some(i) = old {
            entries[i].1 = next;
        } else {
            entries.push((topic.into(), next));
        }
    }
    fn increment(&self, topic: &str, register: bool) {
        let mut topics = self.stream.topics.borrow_mut();
        let at = topics.iter().position(|(name, _)| name == topic);
        let old = at.map(|i| topics[i].1).unwrap_or(f64::NAN);
        let next = if register {
            if old == 0.0 || old.is_nan() {
                1.0
            } else {
                old + 1.0
            }
        } else {
            old - 1.0
        };
        if let Some(i) = at {
            topics[i].1 = next
        } else {
            topics.push((topic.into(), next));
        }
    }
    fn send(&self, topic: &str, subscribe: bool) -> Result<(), ExchangeError> {
        let payload = J::Object(vec![
            (
                "reqType".into(),
                J::String(if subscribe { "sub" } else { "unsub" }.into()),
            ),
            ("dataType".into(), J::String(topic.into())),
            ("id".into(), J::String(self.stream.host.nanoid())),
        ]);
        let text = js_json_stringify(&payload)
            .map_err(|e| error(e.message))?
            .ok_or_else(|| error("undefined subscription payload"))?;
        self.stream.host.send_with_log(&text)
    }
    fn wait(
        self: &Rc<Self>,
        topic: String,
        parser: MessageCallback,
        timer: Option<Rc<Cell<Option<u64>>>>,
    ) -> Result<(), ExchangeError> {
        if self.host.is_connected() {
            self.replace_handlers(&topic, |callbacks| callbacks.push(parser));
            self.increment(&topic, true);
            self.send(&topic, true)
        } else {
            let owner = self.clone();
            let remember = timer.clone();
            let id = self.host.set_timeout(
                100.0,
                Rc::new(move || owner.wait(topic.clone(), parser.clone(), timer.clone())),
            )?;
            if let Some(timer) = remember {
                timer.set(Some(id));
            }
            Ok(())
        }
    }
    fn remove(&self, topic: &str, parser: &MessageCallback) -> Result<(), ExchangeError> {
        self.replace_handlers(topic, |callbacks| {
            callbacks.retain(|f| !Rc::ptr_eq(f, parser))
        });
        self.increment(topic, false);
        let empty = self
            .stream
            .message_handlers
            .borrow()
            .iter()
            .find(|(name, _)| name == topic)
            .is_some_and(|(_, callbacks)| callbacks.borrow().is_empty());
        if self.host.is_connected() && empty {
            self.host.cleanup_stats(topic)?;
            self.send(topic, false)?;
        }
        Ok(())
    }
    pub fn listen_ohlcv(
        self: &Rc<Self>,
        opts: &OHLCVOptions,
        callback: CandleCallback,
    ) -> Result<Cleanup, ExchangeError> {
        let interval = timeframe_name(opts.interval.clone());
        let unit = match interval {
            "1m" => "1min",
            "3m" => "3min",
            "5m" => "5min",
            "15m" => "15min",
            "30m" => "30min",
            "1h" => "1hour",
            "2h" => "2hour",
            "4h" => "4hour",
            "6h" => "6hour",
            "12h" => "12hour",
            "1d" => "1day",
            "1w" => "1week",
            _ => unreachable!("closed timeframe"),
        };
        let symbol = venue_symbol(&self.stream.parent, &opts.symbol);
        let topic = format!("market.kline.{symbol}.{unit}.trade.utc+8");
        let parser: MessageCallback = Rc::new(move |rows| {
            let row = rows
                .first()
                .and_then(|v| v.get("data"))
                .and_then(|v| v.get("klineInfosVo"))
                .and_then(|v| v.get(0))
                .ok_or_else(|| error("Cannot read properties of undefined (reading 'time')"))?;
            callback(candle(row)?)
        });
        self.wait(topic.clone(), parser.clone(), None)?;
        let owner = self.clone();
        Ok(Rc::new(move || owner.remove(&topic, &parser)))
    }
    pub fn listen_order_book(
        self: &Rc<Self>,
        symbol: &str,
        callback: BookCallback,
    ) -> Result<Cleanup, ExchangeError> {
        let native = venue_symbol(&self.stream.parent, symbol);
        let topic = format!("market.depth.{native}.step0.level100");
        let book = Rc::new(RefCell::new(OrderBook {
            bids: vec![],
            asks: vec![],
        }));
        let retained = book.clone();
        let timer = Rc::new(Cell::new(None));
        let parser: MessageCallback = Rc::new(move |rows| {
            let data = rows
                .first()
                .and_then(|v| v.get("data"))
                .ok_or_else(|| error("Cannot read properties of undefined (reading 'bids')"))?;
            let bids = levels(data.get("bids"), "bids")?;
            retained.borrow_mut().bids = bids;
            let asks = levels(data.get("asks"), "asks")?;
            retained.borrow_mut().asks = asks;
            safe_cex_store::orderbook::calc_order_book_total(&mut retained.borrow_mut());
            callback(retained.clone())
        });
        self.wait(topic.clone(), parser.clone(), Some(timer.clone()))?;
        let owner = self.clone();
        Ok(Rc::new(move || {
            owner.replace_handlers(&topic, |callbacks| {
                callbacks.retain(|f| !Rc::ptr_eq(f, &parser))
            });
            book.borrow_mut().asks = vec![];
            book.borrow_mut().bids = vec![];
            if let Some(id) = timer.get().filter(|id| *id != 0) {
                owner.host.clear_timeout(id)?;
                timer.set(None);
            }
            owner.increment(&topic, false);
            let empty = owner
                .stream
                .message_handlers
                .borrow()
                .iter()
                .find(|(name, _)| name == &topic)
                .is_some_and(|(_, callbacks)| callbacks.borrow().is_empty());
            if owner.host.is_connected() && empty {
                owner.host.cleanup_stats(&topic)?;
                owner.send(&topic, false)?;
            }
            Ok(())
        }))
    }
}
pub fn exchange_listen_ohlcv<O, C, R>(opts: O, callback: C, listen: impl FnOnce(O, C) -> R) -> R {
    listen(opts, callback)
}
pub fn exchange_listen_order_book<S, C, R>(
    symbol: S,
    callback: C,
    listen: impl FnOnce(S, C) -> R,
) -> R {
    listen(symbol, callback)
}
