//! Public subscription lifetime methods; full socket construction and message
//! dispatch remain separate rows. Timers/tasks belong to the supplied exchange.
use crate::ws_helpers::PublicKlineCandle;
use safe_cex_exchange::{OwnedOperation, exchange::BaseExchange, runtime::ExchangeError};
use safe_cex_num::js_number_string;
use safe_cex_types::OrderBook;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub trait PublicSubscriptionIo {
    fn is_disposed(&self) -> bool;
    fn is_connected(&self) -> bool;
    fn send(&self, message: &str) -> Result<(), ExchangeError>;
    fn ping(&self) -> Result<(), ExchangeError>;
    fn enable_reconnecting(&self) -> bool;
    fn emit_ws_reconnected(&self) -> Result<(), ExchangeError>;
    fn cleanup_message_handler_stats(&self, stream: &str) -> Result<(), ExchangeError>;
    fn dispose(&self);
}
pub type BookCallback = Rc<dyn Fn(OrderBook) -> Result<(), ExchangeError>>;
pub type CandleCallback = Rc<dyn Fn(PublicKlineCandle) -> Result<(), ExchangeError>>;
pub type BookCleanup = Rc<dyn Fn() -> Result<(), ExchangeError>>;
struct BookSubscription {
    callbacks: RefCell<Vec<(f64, BookCallback)>>,
    retry_timer: RefCell<Option<OwnedOperation>>,
}
struct KlineWait {
    stream: String,
    timer: RefCell<Option<OwnedOperation>>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct PublicSubscriptionState {
    pub topics: Vec<(String, Vec<String>)>,
    pub books: Vec<(String, Vec<f64>, bool)>,
    pub klines: Vec<String>,
    pub pending: Vec<String>,
    pub ready: bool,
    pub subscription_id: f64,
    pub listener_id: f64,
}
pub struct PublicSubscriptions {
    owner: Rc<BaseExchange>,
    io: Rc<dyn PublicSubscriptionIo>,
    topics: RefCell<Vec<(String, Vec<String>)>>,
    books: RefCell<Vec<(String, Rc<BookSubscription>)>>,
    klines: RefCell<Vec<(String, CandleCallback)>>,
    pending: RefCell<Vec<String>>,
    waits: RefCell<Vec<Rc<KlineWait>>>,
    open_tasks: RefCell<Vec<OwnedOperation>>,
    ready: Cell<bool>,
    subscription_id: Cell<f64>,
    listener_id: Cell<f64>,
}
impl PublicSubscriptions {
    pub fn new(owner: Rc<BaseExchange>, io: Rc<dyn PublicSubscriptionIo>) -> Rc<Self> {
        Rc::new(Self {
            owner,
            io,
            topics: RefCell::new(Vec::new()),
            books: RefCell::new(Vec::new()),
            klines: RefCell::new(Vec::new()),
            pending: RefCell::new(Vec::new()),
            waits: RefCell::new(Vec::new()),
            open_tasks: RefCell::new(Vec::new()),
            ready: Cell::new(false),
            subscription_id: Cell::new(1.0),
            listener_id: Cell::new(1.0),
        })
    }
    pub fn state(&self) -> PublicSubscriptionState {
        PublicSubscriptionState {
            topics: self.topics.borrow().clone(),
            books: self
                .books
                .borrow()
                .iter()
                .map(|(stream, s)| {
                    (
                        stream.clone(),
                        s.callbacks.borrow().iter().map(|(id, _)| *id).collect(),
                        s.retry_timer.borrow().is_some(),
                    )
                })
                .collect(),
            klines: self
                .klines
                .borrow()
                .iter()
                .map(|(stream, _)| stream.clone())
                .collect(),
            pending: self.pending.borrow().clone(),
            ready: self.ready.get(),
            subscription_id: self.subscription_id.get(),
            listener_id: self.listener_id.get(),
        }
    }
    /// The source constructor wraps BaseWebSocket.onClose before listeners are installed.
    pub fn before_close(&self) {
        self.ready.set(false);
        self.pending.borrow_mut().clear();
    }
    pub fn begin_connect(&self) {
        self.ready.set(false);
    }
    fn set_topic(&self, key: String, values: Vec<String>) {
        // Source {} uses the inherited __proto__ setter, not an own property.
        // Arbitrary replacement prototypes remain outside the native boundary.
        if key == "__proto__" {
            return;
        }
        let mut topics = self.topics.borrow_mut();
        if let Some((_, old)) = topics.iter_mut().find(|(k, _)| *k == key) {
            *old = values;
        } else {
            topics.push((key, values));
        }
    }
    fn send_subscription(&self, method: &str, streams: &[String]) -> Result<(), ExchangeError> {
        let id = self.subscription_id.get();
        self.subscription_id.set(id + 1.0);
        let params =
            serde_json::to_string(streams).map_err(|e| ExchangeError::new(e.to_string()))?;
        self.io.send(&format!(
            "{{\"method\":\"{method}\",\"params\":{params},\"id\":{}}}",
            js_number_string(id)
        ))
    }
    fn subscribe(&self, streams: &[String]) -> Result<(), ExchangeError> {
        if !self.io.is_connected() {
            return Ok(());
        }
        self.send_subscription("SUBSCRIBE", streams)?;
        for stream in streams {
            if stream.contains("@depth") || stream.contains("@kline") {
                self.set_topic(stream.clone(), vec![stream.clone()]);
            } else {
                let mut parts = stream.split('@');
                let symbol = parts.next().unwrap_or("");
                let kind = parts.next().unwrap_or("undefined");
                let values = self
                    .topics
                    .borrow()
                    .iter()
                    .find(|(key, _)| key == kind)
                    .map(|(_, v)| v.clone());
                if values.is_none()
                    && !matches!(
                        safe_cex_num::js_table_lookup::<&str, ()>(&[], kind),
                        safe_cex_num::TableHit::Missing
                    )
                {
                    continue;
                }
                let mut values = values.unwrap_or_default();
                values.push(symbol.to_owned());
                self.set_topic(kind.to_owned(), values);
            }
        }
        Ok(())
    }
    fn unsubscribe(&self, streams: &[String]) -> Result<(), ExchangeError> {
        if self.io.is_connected() {
            self.send_subscription("UNSUBSCRIBE", streams)?;
        }
        Ok(())
    }
    // ts: AsterDEXPublicWebsocket.onOpen
    pub fn on_open(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if self.io.is_disposed() {
            return Ok(());
        }
        self.io.ping()?;
        // JS async IIFE executes the first subscribe synchronously, then yields
        // at its resolved Promise before emitting/replaying. Keep that ordering.
        let first = self.subscribe(&["!ticker@arr".into(), "!markPrice@arr@1s".into()]);
        let controller = self.clone();
        let task = self.owner.start_owned_operation(async move {
            let result = (|| {
                first?;
                if controller.io.enable_reconnecting() {
                    controller.io.emit_ws_reconnected()?;
                }
                controller.clear_order_book_subscription_retries();
                let streams: Vec<_> = controller
                    .topics
                    .borrow()
                    .iter()
                    .filter(|(key, _)| !key.starts_with('!') && key.contains('@'))
                    .map(|(key, _)| key.clone())
                    .collect();
                if !streams.is_empty() {
                    let _ = controller.subscribe(&streams);
                }
                Ok::<(), ExchangeError>(())
            })();
            // Source catches all IIFE failures and always marks initialized.
            let _ = result;
            controller.ready.set(true);
        });
        self.open_tasks.borrow_mut().push(task);
        Ok(())
    }
    // ts: AsterDEXPublicWebsocket.listenOrderBook
    pub fn listen_order_book(
        self: &Rc<Self>,
        symbol: &str,
        callback: BookCallback,
    ) -> Result<BookCleanup, ExchangeError> {
        let stream = format!("{}@depth20@100ms", symbol.to_lowercase());
        let id = self.listener_id.get();
        self.listener_id.set(id + 1.0);
        let existing = self
            .books
            .borrow()
            .iter()
            .find(|(key, _)| *key == stream)
            .map(|(_, s)| s.clone());
        if let Some(subscription) = existing {
            let mut callbacks = subscription.callbacks.borrow_mut();
            if let Some((_, old)) = callbacks.iter_mut().find(|(key, _)| *key == id) {
                *old = callback;
            } else {
                callbacks.push((id, callback));
            }
        } else {
            let subscription = Rc::new(BookSubscription {
                callbacks: RefCell::new(vec![(id, callback)]),
                retry_timer: RefCell::new(None),
            });
            self.books
                .borrow_mut()
                .push((stream.clone(), subscription.clone()));
            self.set_topic(stream.clone(), vec![stream.clone()]);
            self.subscribe_order_book(&stream, subscription, 0)?;
        }
        let controller = self.clone();
        Ok(Rc::new(move || {
            controller.unsubscribe_order_book_listener(&stream, id)
        }))
    }
    // ts: AsterDEXPublicWebsocket.subscribeOrderBook
    fn subscribe_order_book(
        self: &Rc<Self>,
        stream: &str,
        subscription: Rc<BookSubscription>,
        retry_count: u32,
    ) -> Result<(), ExchangeError> {
        let current = self
            .books
            .borrow()
            .iter()
            .find(|(key, _)| key == stream)
            .map(|(_, s)| s.clone());
        if !current
            .as_ref()
            .is_some_and(|s| Rc::ptr_eq(s, &subscription))
            || subscription.callbacks.borrow().is_empty()
        {
            return Ok(());
        }
        if !self.io.is_disposed()
            && self.io.is_connected()
            && self
                .send_subscription("SUBSCRIBE", &[stream.to_owned()])
                .is_ok()
        {
            return Ok(());
        }
        if retry_count < 50 {
            let controller = self.clone();
            let key = stream.to_owned();
            let next = subscription.clone();
            let timer = self.owner.start_timeout(100, move || {
                controller.subscribe_order_book(&key, next.clone(), retry_count + 1)
            })?;
            *subscription.retry_timer.borrow_mut() = Some(timer);
        }
        Ok(())
    }
    // ts: AsterDEXPublicWebsocket.unsubscribeOrderBookListener
    pub fn unsubscribe_order_book_listener(
        &self,
        stream: &str,
        id: f64,
    ) -> Result<(), ExchangeError> {
        let subscription = self
            .books
            .borrow()
            .iter()
            .find(|(key, _)| key == stream)
            .map(|(_, s)| s.clone());
        let Some(subscription) = subscription else {
            return Ok(());
        };
        let empty = {
            let mut callbacks = subscription.callbacks.borrow_mut();
            let Some(index) = callbacks.iter().position(|(key, _)| *key == id) else {
                return Ok(());
            };
            callbacks.remove(index);
            callbacks.is_empty()
        };
        if !empty {
            return Ok(());
        }
        let timer = subscription.retry_timer.borrow_mut().take();
        if let Some(timer) = timer {
            timer.cancel();
        }
        self.books.borrow_mut().retain(|(key, _)| key != stream);
        self.topics.borrow_mut().retain(|(key, _)| key != stream);
        self.io.cleanup_message_handler_stats(stream)?;
        if !self.io.is_disposed() {
            if let Err(error) = self.unsubscribe(&[stream.to_owned()]) {
                self.owner.runtime.report_exception(
                    &error,
                    &serde_json::json!({"operation":"AsterDEX book unsubscribe"}).into(),
                );
            }
        }
        Ok(())
    }
    // ts: AsterDEXPublicWebsocket.clearOrderBookSubscriptionRetries
    pub fn clear_order_book_subscription_retries(&self) {
        let subscriptions: Vec<_> = self.books.borrow().iter().map(|(_, s)| s.clone()).collect();
        for subscription in subscriptions {
            let timer = subscription.retry_timer.borrow_mut().take();
            if let Some(timer) = timer {
                timer.cancel();
            }
        }
    }
    fn do_kline_subscribe(&self, stream: &str) -> Result<(), ExchangeError> {
        if !self.io.is_disposed() {
            self.set_topic(stream.to_owned(), vec![stream.to_owned()]);
            self.send_subscription("SUBSCRIBE", &[stream.to_owned()])?;
        }
        Ok(())
    }
    fn wait_for_kline(
        self: &Rc<Self>,
        wait: Rc<KlineWait>,
        retry_count: f64,
    ) -> Result<(), ExchangeError> {
        if self.io.is_connected() {
            if self.ready.get() {
                self.do_kline_subscribe(&wait.stream)?;
            } else {
                self.pending.borrow_mut().push(wait.stream.clone());
            }
            let timer = wait.timer.borrow_mut().take();
            if let Some(timer) = timer {
                timer.cancel();
            }
            self.waits
                .borrow_mut()
                .retain(|current| !Rc::ptr_eq(current, &wait));
        } else {
            // Node setTimeout truncates this positive fractional delay. Preserve
            // the source's uncancelled unsubscribe wait until native disposal.
            let delay = (100.0 * 1.5_f64.powf((retry_count / 10.0).floor())).min(2000.0) as u64;
            let controller = self.clone();
            let next = wait.clone();
            let timer = self.owner.start_timeout(delay, move || {
                controller.wait_for_kline(next.clone(), retry_count + 1.0)
            })?;
            *wait.timer.borrow_mut() = Some(timer);
        }
        Ok(())
    }
    // ts: AsterDEXPublicWebsocket.subscribeToKline
    pub fn subscribe_to_kline(
        self: &Rc<Self>,
        stream: &str,
        callback: CandleCallback,
    ) -> Result<(), ExchangeError> {
        let mut callbacks = self.klines.borrow_mut();
        if let Some((_, old)) = callbacks.iter_mut().find(|(key, _)| key == stream) {
            *old = callback;
        } else {
            callbacks.push((stream.to_owned(), callback));
        }
        drop(callbacks);
        let wait = Rc::new(KlineWait {
            stream: stream.to_owned(),
            timer: RefCell::new(None),
        });
        self.waits.borrow_mut().push(wait.clone());
        self.wait_for_kline(wait, 0.0)
    }
    // ts: AsterDEXPublicWebsocket.unsubscribeFromKline
    pub fn unsubscribe_from_kline(&self, stream: &str) -> Result<(), ExchangeError> {
        self.klines.borrow_mut().retain(|(key, _)| key != stream);
        self.topics.borrow_mut().retain(|(key, _)| key != stream);
        self.io.cleanup_message_handler_stats(stream)?;
        let _ = self.unsubscribe(&[stream.to_owned()]);
        Ok(())
    }
    /// Adapter used by the existing decoded depth handler; clone per listener.
    pub fn deliver_book(&self, stream: &str, book: &OrderBook) -> Result<(), ExchangeError> {
        let subscription = self
            .books
            .borrow()
            .iter()
            .find(|(key, _)| key == stream)
            .map(|(_, s)| s.clone());
        if let Some(subscription) = subscription {
            let callbacks = subscription.callbacks.borrow().clone();
            for (id, callback) in callbacks {
                if subscription
                    .callbacks
                    .borrow()
                    .iter()
                    .any(|(key, _)| *key == id)
                {
                    callback(book.clone())?;
                }
            }
        }
        Ok(())
    }
    pub fn deliver_kline(
        &self,
        stream: &str,
        candle: PublicKlineCandle,
    ) -> Result<(), ExchangeError> {
        let callback = self
            .klines
            .borrow()
            .iter()
            .find(|(key, _)| key == stream)
            .map(|(_, cb)| cb.clone());
        if let Some(callback) = callback {
            callback(candle)?;
        }
        Ok(())
    }
    // ts: AsterDEXPublicWebsocket.dispose
    pub fn dispose(&self) {
        self.klines.borrow_mut().clear();
        self.clear_order_book_subscription_retries();
        self.books.borrow_mut().clear();
        self.topics.borrow_mut().clear();
        self.pending.borrow_mut().clear();
        self.ready.set(false);
        // CHOSEN: close this controller's waits/open work as well as the socket;
        // source kline closure timers can leak and its IIFE can finish afterward.
        let waits = self.waits.borrow_mut().drain(..).collect::<Vec<_>>();
        for wait in waits {
            let timer = wait.timer.borrow_mut().take();
            if let Some(timer) = timer {
                timer.cancel();
            }
        }
        let tasks = self.open_tasks.borrow_mut().drain(..).collect::<Vec<_>>();
        for task in tasks {
            task.cancel();
        }
        self.io.dispose();
    }
}
