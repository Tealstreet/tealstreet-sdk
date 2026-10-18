use crate::{
    data_post::{WorkerDataPayload, WorkerDataPost},
    error_post::WorkerErrorPost,
};
use crate::{
    handler::BaseHandler,
    order_book_post::{OrderBookPost, OrderBookUpdateError, OrderBookWorkerPost},
    payload::{StoreWorkerOp, StoreWorkerPayload},
};
use indexmap::{IndexMap, IndexSet};
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::value::{Js, from_json, from_typed},
    runtime::{ExchangeError, ExchangeRuntime, ExchangeTask, ListenerCleanup},
};
use safe_cex_types::OrderBook;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
};

// Lodash throttle: leading + trailing, with maxWait equal to wait.
struct StreamThrottle<T> {
    runtime: Rc<dyn ExchangeRuntime>,
    callback: Box<dyn Fn(T)>,
    wait: f64,
    last_call: Cell<Option<f64>>,
    last_invoke: Cell<f64>,
    args: RefCell<Option<T>>,
    timer: Cell<Option<u64>>,
    next: Cell<u64>,
    tasks: RefCell<HashMap<u64, Box<dyn ExchangeTask>>>,
}
struct StreamTimerGuard<T> {
    throttle: Weak<StreamThrottle<T>>,
    id: u64,
    settled: Rc<Cell<bool>>,
}
impl<T> Drop for StreamTimerGuard<T> {
    fn drop(&mut self) {
        if self.settled.replace(true) {
            return;
        }
        if let Some(throttle) = self.throttle.upgrade() {
            if throttle.timer.get() == Some(self.id) {
                // Dropped timers must release their own pending slot for the next call.
                throttle.timer.set(None);
                throttle.last_call.set(None);
                // Keep arguments until invoke/next call: spawn may discard before the leading call.
            }
            let task = throttle.tasks.borrow_mut().remove(&self.id);
            drop(task);
        }
    }
}
impl<T: 'static> StreamThrottle<T> {
    fn new(
        runtime: Rc<dyn ExchangeRuntime>,
        wait: f64,
        callback: impl Fn(T) + 'static,
    ) -> Rc<Self> {
        Rc::new(Self {
            runtime,
            callback: Box::new(callback),
            wait: if wait.is_nan() { 0.0 } else { wait },
            last_call: Cell::new(None),
            last_invoke: Cell::new(0.0),
            args: RefCell::new(None),
            timer: Cell::new(None),
            next: Cell::new(0),
            tasks: RefCell::new(HashMap::new()),
        })
    }
    fn should_invoke(&self, now: f64) -> bool {
        self.last_call.get().is_none_or(|last| {
            now - last >= self.wait || now - last < 0.0 || now - self.last_invoke.get() >= self.wait
        })
    }
    fn invoke(&self, now: f64) {
        let args = self.args.borrow_mut().take();
        self.last_invoke.set(now);
        if let Some(args) = args {
            (self.callback)(args);
        }
    }
    fn trailing(&self, now: f64) {
        self.timer.set(None);
        if self.args.borrow().is_some() {
            self.invoke(now);
        }
    }
    fn arm(self: &Rc<Self>, delay: f64) {
        let id = self.next.get();
        self.next.set(id.wrapping_add(1));
        self.timer.set(Some(id));
        let weak = Rc::downgrade(self);
        let settled = Rc::new(Cell::new(false));
        let guard = StreamTimerGuard {
            throttle: weak.clone(),
            id,
            settled: settled.clone(),
        };
        // Browser setTimeout converts its delay to a signed 32-bit integer, then clamps negatives.
        let millis = if delay.is_finite() {
            let n = delay.trunc().rem_euclid(4_294_967_296.0);
            if n >= 2_147_483_648.0 { 0 } else { n as u64 }
        } else {
            0
        };
        let sleep = self.runtime.sleep(millis);
        let task = self.runtime.spawn_local(Box::pin(async move {
            let guard = guard;
            if sleep.await.is_err() {
                return;
            }
            if let Some(throttle) = weak.upgrade() {
                guard.settled.set(true);
                let handle = throttle.tasks.borrow_mut().remove(&id);
                drop(handle);
                let now = throttle.runtime.now_ms();
                if throttle.should_invoke(now) {
                    throttle.trailing(now);
                } else {
                    let remaining = safe_cex_num::math_min(
                        throttle.wait - (now - throttle.last_call.get().unwrap_or(now)),
                        throttle.wait - (now - throttle.last_invoke.get()),
                    );
                    throttle.arm(remaining);
                }
            }
        }));
        if !settled.get() {
            self.tasks.borrow_mut().insert(id, task);
        }
    }
    fn cancel_timer(&self) {
        if let Some(id) = self.timer.take() {
            let task = self.tasks.borrow_mut().remove(&id);
            if let Some(task) = task {
                task.cancel();
            }
        }
    }
    fn call(self: &Rc<Self>, args: T) {
        let now = self.runtime.now_ms();
        let invoke = self.should_invoke(now);
        *self.args.borrow_mut() = Some(args);
        self.last_call.set(Some(now));
        if invoke {
            if self.timer.get().is_none() {
                self.last_invoke.set(now);
                self.arm(self.wait);
                self.invoke(now);
                return;
            }
            self.cancel_timer();
            self.arm(self.wait);
            self.invoke(now);
            return;
        }
        if self.timer.get().is_none() {
            self.arm(self.wait);
        }
    }
    fn flush(&self) {
        // Lodash flush deliberately leaves the already-scheduled timeout alive.
        if self.timer.get().is_some() {
            self.trailing(self.runtime.now_ms());
        }
    }
    fn cancel(&self) {
        self.cancel_timer();
        self.last_invoke.set(0.0);
        self.last_call.set(None);
        self.args.borrow_mut().take();
    }
}
impl<T> Drop for StreamThrottle<T> {
    fn drop(&mut self) {
        for (_, task) in self.tasks.get_mut().drain() {
            task.cancel();
        }
    }
}
struct SymbolSub {
    unsub: RefCell<Option<ListenerCleanup>>,
    throttle: Rc<StreamThrottle<OrderBook>>,
    sub_ids: RefCell<IndexSet<String>>,
}

// ts-bug: BaseHandler.dispose leaves active subscriptions attached.
pub struct OrderBookSubscriptionsHandler {
    pub base: Rc<BaseHandler>,
    exchange: Rc<dyn Exchange>,
    post: OrderBookPost,
    tick_delay: f64,
    subs: RefCell<IndexMap<String, Rc<SymbolSub>>>,
}
impl OrderBookSubscriptionsHandler {
    pub fn new(
        exchange: Rc<dyn Exchange>,
        post: OrderBookPost,
        orderbook_tick_delay: f64,
        error_post: WorkerErrorPost,
    ) -> Rc<Self> {
        Rc::new_cyclic(|weak: &Weak<Self>| {
            let replay = weak.clone();
            Self {
                base: BaseHandler::new(
                    exchange.clone(),
                    "OrderBookSubscriptionsHandler".into(),
                    error_post,
                    Rc::new(move |payload| {
                        if let Some(handler) = replay.upgrade() {
                            if let Err(error) = handler.handle(payload) {
                                handler
                                    .exchange
                                    .base()
                                    .runtime
                                    .report_exception(&error, &Value::Null.into());
                            }
                        }
                    }),
                ),
                exchange,
                post,
                tick_delay: orderbook_tick_delay,
                subs: RefCell::default(),
            }
        })
    }
    pub fn send_order_book_update(
        &self,
        data: &safe_cex_types::object_map::JsObjectMap<safe_cex_num::JsJsonValue>,
        book: Rc<RefCell<OrderBook>>,
    ) -> Result<(), OrderBookUpdateError> {
        // postMessage receives the original book before the ticker lookup. A host
        // callback may mutate that book synchronously; re-read it after the post.
        (self.post.book)(OrderBookWorkerPost::new(data, book.clone()))?;
        let prices = {
            let book = book.borrow();
            book.asks
                .first()
                .zip(book.bids.first())
                .map(|(ask, bid)| (ask.price, bid.price))
        };
        if let Some((ask, bid)) = prices {
            let store = &self.exchange.base().store;
            let ticker = store
                .tickers()?
                .iter()
                .find(|t| {
                    Some(t.symbol.as_str())
                        == data.get("symbol").and_then(|value| match value {
                            safe_cex_num::JsJsonValue::String(symbol) => Some(symbol.as_str()),
                            _ => None,
                        })
                })
                .cloned();
            if let Some(ticker) = ticker
                && (ticker.ask != ask || ticker.bid != bid)
            {
                let mut patch = safe_cex_store::model_patch::ModelPatch::default();
                patch.set_number("ask", ask);
                patch.set_number("bid", bid);
                store.update_ticker(&ticker.id, &patch)?;
            }
        }
        Ok(())
    }
    pub fn handle(self: &Rc<Self>, payload: StoreWorkerPayload) -> Result<(), ExchangeError> {
        match payload.op {
            StoreWorkerOp::SubscribeOrderBook => {
                if self.base.buffer_until_markets_loaded(payload.clone())? {
                    return Ok(());
                }
                let Some(data) = payload.data.filter(Value::is_object) else {
                    return Ok(());
                };
                let (Some(symbol), Some(sub_id)) =
                    (data["symbol"].as_str(), data["subId"].as_str())
                else {
                    return Ok(());
                };
                if !self
                    .exchange
                    .base()
                    .store
                    .markets()?
                    .iter()
                    .any(|m| m.symbol == symbol)
                {
                    self.exchange.base().runtime.console("warn",&format!("[OrderBookSubscriptionsHandler] Symbol \"{symbol}\" not found on {}, skipping subscription",serde_json::to_value(&self.exchange.base().name).unwrap().as_str().unwrap()));
                    return Ok(());
                }
                if let Some(sub) = self.subs.borrow().get(symbol) {
                    sub.sub_ids.borrow_mut().insert(sub_id.into());
                    return Ok(());
                }
                let symbol = symbol.to_owned();
                let sub_id = sub_id.to_owned();
                let multiplier = self
                    .exchange
                    .base()
                    .options()
                    .tuning
                    .as_ref()
                    .and_then(|t| t.data_update_throttle_multiplier)
                    .filter(|v| *v != 0.0 && !v.is_nan())
                    .unwrap_or(1.0);
                let data: safe_cex_types::object_map::JsObjectMap<safe_cex_num::JsJsonValue> = data
                    .as_object()
                    .expect("validated subscription data")
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone().into()))
                    .collect();
                let weak = Rc::downgrade(self);
                let throttle = StreamThrottle::new(
                    self.exchange.base().runtime.clone(),
                    self.tick_delay * multiplier,
                    move |book| {
                        if let Some(handler) = weak.upgrade() {
                            if let Err(error) =
                                handler.send_order_book_update(&data, Rc::new(RefCell::new(book)))
                            {
                                // Shared Exchange listeners still return unit; this parent
                                // adapter reports rather than synchronously returning a throw.
                                handler.exchange.base().runtime.report_exception(
                                    &error.into_exchange_error(),
                                    &json!({"method":"sendOrderBookUpdate"}).into(),
                                );
                            }
                        }
                    },
                );
                let send = throttle.clone();
                let unsub = self
                    .exchange
                    .listen_order_book(symbol.clone(), Rc::new(move |book| send.call(book)))?;
                self.subs.borrow_mut().insert(
                    symbol,
                    Rc::new(SymbolSub {
                        unsub: RefCell::new(Some(unsub)),
                        throttle,
                        sub_ids: RefCell::new(IndexSet::from([sub_id])),
                    }),
                );
            }
            StoreWorkerOp::UnsubscribeOrderBook => {
                let Some(id) = payload.data.as_ref().and_then(|d| d["subId"].as_str()) else {
                    return Ok(());
                };
                let found = self
                    .subs
                    .borrow()
                    .iter()
                    .find(|(_, sub)| sub.sub_ids.borrow().contains(id))
                    .map(|(key, sub)| (key.clone(), sub.clone()));
                if let Some((key, sub)) = found {
                    sub.sub_ids.borrow_mut().shift_remove(id);
                    if sub.sub_ids.borrow().is_empty() {
                        let result = sub
                            .unsub
                            .borrow_mut()
                            .take()
                            .map_or(Ok(()), |unsub| unsub());
                        if result.is_ok() {
                            sub.throttle.cancel();
                        }
                        self.subs.borrow_mut().shift_remove(&key);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

struct SymbolIntervalSub {
    unsub: RefCell<Option<ListenerCleanup>>,
    throttle: RefCell<Option<Rc<StreamThrottle<safe_cex_types::Candle>>>>,
    sub_ids: RefCell<IndexSet<String>>,
    last_candle_timestamp: Cell<f64>,
    payloads: RefCell<IndexMap<String, (u64, Value)>>,
    next_payload: Cell<u64>,
}
// ts-bug: BaseHandler.dispose leaves active subscriptions attached.
pub struct OHLCVSubscriptionsHandler {
    pub base: Rc<BaseHandler>,
    exchange: Rc<dyn Exchange>,
    post: WorkerDataPost,
    subs: RefCell<IndexMap<String, Rc<SymbolIntervalSub>>>,
}
impl OHLCVSubscriptionsHandler {
    pub fn new(
        exchange: Rc<dyn Exchange>,
        post: WorkerDataPost,
        error_post: WorkerErrorPost,
    ) -> Rc<Self> {
        Rc::new_cyclic(|weak: &Weak<Self>| {
            let replay = weak.clone();
            Self {
                base: BaseHandler::new(
                    exchange.clone(),
                    "OHLCVSubscriptionsHandler".into(),
                    error_post,
                    Rc::new(move |payload| {
                        if let Some(handler) = replay.upgrade() {
                            if let Err(error) = handler.handle(payload) {
                                handler
                                    .exchange
                                    .base()
                                    .runtime
                                    .report_exception(&error, &Value::Null.into());
                            }
                        }
                    }),
                ),
                exchange,
                post,
                subs: RefCell::default(),
            }
        })
    }
    fn send_candle_update(
        post: &WorkerDataPost,
        sub: &SymbolIntervalSub,
        candle: safe_cex_types::Candle,
    ) -> Result<(), ExchangeError> {
        let candle = from_typed(&candle).map_err(|e| e.to_exchange_error())?;
        let mut cursor = 0;
        loop {
            // JS Map iteration observes deletions and newly appended entries during post.
            let next = sub
                .payloads
                .borrow()
                .iter()
                .find(|(_, (ordinal, _))| *ordinal >= cursor)
                .map(|(id, (ordinal, data))| (id.clone(), *ordinal, data.clone()));
            let Some((id, ordinal, data)) = next else {
                break;
            };
            cursor = ordinal + 1;
            let data = from_json(&data);
            data.define("subId", Js::str(&id))
                .map_err(|e| e.to_exchange_error())?;
            data.define("candle", candle.clone())
                .map_err(|e| e.to_exchange_error())?;
            post.send(WorkerDataPayload {
                op: StoreWorkerOp::ListenOHLCV,
                data,
            })?;
        }
        Ok(())
    }
    fn get_key(symbol: &str, interval: &str) -> String {
        format!("{symbol}:{interval}")
    }
    pub fn handle(self: &Rc<Self>, payload: StoreWorkerPayload) -> Result<(), ExchangeError> {
        match payload.op {
            StoreWorkerOp::SubscribeOHLCV => {
                if self.base.buffer_until_markets_loaded(payload.clone())? {
                    return Ok(());
                }
                let Some(data) = payload.data.filter(Value::is_object) else {
                    return Ok(());
                };
                let (Some(symbol), Some(interval), Some(id)) = (
                    data["symbol"].as_str(),
                    data["interval"].as_str(),
                    data["subId"].as_str(),
                ) else {
                    return Ok(());
                };
                if !self
                    .exchange
                    .base()
                    .store
                    .markets()?
                    .iter()
                    .any(|m| m.symbol == symbol)
                {
                    self.exchange.base().runtime.console("warn",&format!("[OHLCVSubscriptionsHandler] Symbol \"{symbol}\" not found on {}, skipping subscription",serde_json::to_value(&self.exchange.base().name).unwrap().as_str().unwrap()));
                    return Ok(());
                }
                let key = Self::get_key(symbol, interval);
                let id = id.to_owned();
                if let Some(sub) = self.subs.borrow().get(&key) {
                    sub.sub_ids.borrow_mut().insert(id.clone());
                    let mut payloads = sub.payloads.borrow_mut();
                    if let Some((_, existing)) = payloads.get_mut(&id) {
                        *existing = data;
                    } else {
                        let ordinal = sub.next_payload.get();
                        sub.next_payload.set(ordinal + 1);
                        payloads.insert(id, (ordinal, data));
                    }
                    return Ok(());
                }
                let Ok(options) = serde_json::from_value(data.clone()) else {
                    return Ok(());
                };
                let sub = Rc::new(SymbolIntervalSub {
                    unsub: RefCell::new(None),
                    throttle: RefCell::new(None),
                    sub_ids: RefCell::new(IndexSet::from([id.clone()])),
                    last_candle_timestamp: Cell::new(0.0),
                    payloads: RefCell::new(IndexMap::from([(id, (0, data))])),
                    next_payload: Cell::new(1),
                });
                let weak = Rc::downgrade(&sub);
                let post = self.post.clone();
                let multiplier = self
                    .exchange
                    .base()
                    .options()
                    .tuning
                    .as_ref()
                    .and_then(|t| t.data_update_throttle_multiplier)
                    .filter(|v| *v != 0.0 && !v.is_nan())
                    .unwrap_or(1.0);
                let runtime = self.exchange.base().runtime.clone();
                let throttle =
                    StreamThrottle::new(runtime.clone(), 60.0 * multiplier, move |candle| {
                        if let Some(sub) = weak.upgrade()
                            && let Err(error) = Self::send_candle_update(&post, &sub, candle)
                        {
                            runtime.report_exception(
                                &error,
                                &json!({"operation":"worker candle post"}).into(),
                            );
                        }
                    });
                *sub.throttle.borrow_mut() = Some(throttle.clone());
                let listener_sub = sub.clone();
                let unsub = self.exchange.listen_ohlcv(
                    options,
                    Rc::new(move |candle| {
                        if candle.timestamp != listener_sub.last_candle_timestamp.get() {
                            throttle.flush();
                            listener_sub.last_candle_timestamp.set(candle.timestamp);
                        }
                        throttle.call(candle);
                    }),
                )?;
                *sub.unsub.borrow_mut() = Some(unsub);
                self.subs.borrow_mut().insert(key, sub);
            }
            StoreWorkerOp::UnsubscribeOHLCV => {
                let Some(id) = payload.data.as_ref().and_then(|d| d["subId"].as_str()) else {
                    return Ok(());
                };
                let found = self
                    .subs
                    .borrow()
                    .iter()
                    .find(|(_, sub)| sub.sub_ids.borrow().contains(id))
                    .map(|(key, sub)| (key.clone(), sub.clone()));
                if let Some((key, sub)) = found {
                    sub.sub_ids.borrow_mut().shift_remove(id);
                    sub.payloads.borrow_mut().shift_remove(id);
                    if sub.sub_ids.borrow().is_empty() {
                        let result = sub
                            .unsub
                            .borrow_mut()
                            .take()
                            .map_or(Ok(()), |unsub| unsub());
                        if result.is_ok() {
                            if let Some(throttle) = sub.throttle.borrow().as_ref() {
                                throttle.cancel();
                            }
                        }
                        self.subs.borrow_mut().shift_remove(&key);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod test_runtime;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn immediately_discarded_timeout_preserves_synchronous_leading_updates() {
        let runtime = Rc::new(test_runtime::Runtime::default());
        runtime.discard_spawn.set(true);
        let calls = Rc::new(RefCell::new(Vec::new()));
        let sink = calls.clone();
        let throttle =
            StreamThrottle::new(runtime.clone(), 100.0, move |v| sink.borrow_mut().push(v));
        throttle.call(1);
        throttle.call(2);
        throttle.flush();
        assert_eq!(*calls.borrow(), vec![1, 2]);
        runtime.discard_spawn.set(false);
        throttle.call(3);
        throttle.call(4);
        runtime.advance(100);
        assert_eq!(*calls.borrow(), vec![1, 2, 3, 4]);
    }
    #[test]
    fn discarded_stream_timers_release_the_slot_and_drop_does_not_retain_the_owner() {
        let runtime = Rc::new(test_runtime::Runtime::default());
        let calls = Rc::new(RefCell::new(Vec::new()));
        let log = calls.clone();
        let throttle =
            StreamThrottle::new(runtime.clone(), 100.0, move |v| log.borrow_mut().push(v));
        throttle.call(1);
        throttle.call(2);
        let discarded = std::mem::take(&mut *runtime.tasks.borrow_mut());
        drop(discarded);
        throttle.call(3);
        assert_eq!(*calls.borrow(), vec![1, 3]);
        runtime.advance(10);
        throttle.call(4);
        runtime.advance(20);
        let discarded = std::mem::take(&mut *runtime.tasks.borrow_mut());
        drop(discarded);
        throttle.call(5);
        assert_eq!(*calls.borrow(), vec![1, 3, 5]);
        throttle.call(6);
        let weak = Rc::downgrade(&throttle);
        drop(throttle);
        assert!(weak.upgrade().is_none());
        runtime.advance(200);
        assert_eq!(*calls.borrow(), vec![1, 3, 5]);
    }
}
