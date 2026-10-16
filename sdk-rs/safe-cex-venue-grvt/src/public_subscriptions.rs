//! Public websocket subscription methods against the configured send/timer hooks.
use indexmap::IndexMap;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{TableHit, js_number_string, js_table_lookup};
use serde::Serialize;
use std::cell::{Cell, RefCell};

use crate::types::{GrvtJsonRpcVersion, GrvtSubscriptionMethod};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NumberKey(u64);
impl NumberKey {
    pub fn new(value: f64) -> Self {
        Self(if value == 0.0 {
            0
        } else if value.is_nan() {
            f64::NAN.to_bits()
        } else {
            value.to_bits()
        })
    }
    pub fn value(self) -> f64 {
        f64::from_bits(self.0)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SubscribeParams {
    pub stream: String,
    pub selectors: [String; 1],
}
#[derive(Debug, Clone, Serialize)]
pub struct SubscribeRequest {
    pub jsonrpc: GrvtJsonRpcVersion,
    pub id: f64,
    pub method: GrvtSubscriptionMethod,
    pub params: SubscribeParams,
}
#[derive(Debug, Clone, Serialize)]
pub struct UnsubscribeParams {
    pub subscription_id: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct UnsubscribeRequest {
    pub jsonrpc: GrvtJsonRpcVersion,
    pub id: f64,
    pub method: GrvtSubscriptionMethod,
    pub params: UnsubscribeParams,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum RpcRequest {
    Subscribe(SubscribeRequest),
    Unsubscribe(UnsubscribeRequest),
}

/// Live timer insertion slots preserve Set iteration through clear-hook reentrancy.
#[derive(Debug, Default)]
pub struct BatchTimers<T> {
    slots: RefCell<Vec<Option<T>>>,
    iterators: Cell<usize>,
}
struct TimerIteration<'a, T>(&'a BatchTimers<T>);
impl<T> Drop for TimerIteration<'_, T> {
    fn drop(&mut self) {
        self.0.iterators.set(self.0.iterators.get() - 1);
        if self.0.iterators.get() == 0 {
            self.0.slots.borrow_mut().retain(Option::is_some);
        }
    }
}
impl<T: Clone + PartialEq> BatchTimers<T> {
    pub fn insert(&self, timer: T) {
        let mut timers = self.slots.borrow_mut();
        if !timers.iter().flatten().any(|current| current == &timer) {
            timers.push(Some(timer));
        }
    }
    pub fn remove(&self, timer: &T) {
        let mut timers = self.slots.borrow_mut();
        if let Some(current) = timers
            .iter_mut()
            .find(|current| current.as_ref() == Some(timer))
        {
            *current = None;
        }
    }
    pub fn values(&self) -> Vec<T> {
        self.slots.borrow().iter().flatten().cloned().collect()
    }
    fn clear(&self) {
        for slot in self.slots.borrow_mut().iter_mut() {
            *slot = None;
        }
        if self.iterators.get() == 0 {
            self.slots.borrow_mut().clear();
        }
    }
    fn next(&self, index: &mut usize) -> Option<T> {
        loop {
            let timers = self.slots.borrow();
            let timer = timers.get(*index)?;
            *index += 1;
            if let Some(timer) = timer {
                return Some(timer.clone());
            }
        }
    }
}

#[derive(Default)]
pub struct SubscriptionMap {
    entries: IndexMap<NumberKey, usize>,
    slots: Vec<Option<(NumberKey, SubscribeRequest)>>,
    iterators: usize,
}
impl SubscriptionMap {
    pub fn insert(&mut self, key: NumberKey, value: SubscribeRequest) -> Option<SubscribeRequest> {
        if let Some(index) = self.entries.get(&key) {
            return self.slots[*index]
                .replace((key, value))
                .map(|(_, value)| value);
        }
        self.entries.insert(key, self.slots.len());
        self.slots.push(Some((key, value)));
        None
    }
    pub fn shift_remove(&mut self, key: &NumberKey) -> Option<SubscribeRequest> {
        let index = self.entries.shift_remove(key)?;
        let previous = self.slots[index].take().map(|(_, value)| value);
        if self.iterators == 0 {
            self.compact();
        }
        previous
    }
    pub fn contains_key(&self, key: &NumberKey) -> bool {
        self.entries.contains_key(key)
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&NumberKey, &SubscribeRequest)> {
        self.entries
            .iter()
            .map(|(key, index)| (key, &self.slots[*index].as_ref().expect("active slot").1))
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.slots.fill(None);
        if self.iterators == 0 {
            self.slots.clear();
        }
    }
    fn compact(&mut self) {
        self.slots.retain(Option::is_some);
        self.entries.clear();
        for (index, slot) in self.slots.iter().enumerate() {
            self.entries
                .insert(slot.as_ref().expect("compacted slot").0, index);
        }
    }
    pub(super) fn start_replay(&mut self) {
        self.iterators += 1;
    }
    pub(super) fn finish_replay(&mut self) {
        self.iterators -= 1;
        if self.iterators == 0 {
            self.compact();
        }
    }
    pub(super) fn next_replay(&self, index: &mut usize) -> Option<SubscribeRequest> {
        loop {
            let slot = self.slots.get(*index)?;
            *index += 1;
            if let Some((_, value)) = slot {
                return Some(value.clone());
            }
        }
    }
}

/// State shared by mapped methods and the native public websocket owner.
pub struct PublicSubscriptions<T> {
    pub request_id: Cell<f64>,
    pub subscriptions: RefCell<SubscriptionMap>,
    pub ticker_subscriptions: RefCell<IndexMap<String, f64>>,
    pub ticker_batch_timers: BatchTimers<T>,
    pub ticker_batch_generation: Cell<f64>,
}
impl<T: Clone + PartialEq> Default for PublicSubscriptions<T> {
    fn default() -> Self {
        Self {
            request_id: Cell::new(1.0),
            subscriptions: RefCell::new(SubscriptionMap::default()),
            ticker_subscriptions: RefCell::new(IndexMap::new()),
            ticker_batch_timers: BatchTimers {
                slots: RefCell::new(Vec::new()),
                iterators: Cell::new(0),
            },
            ticker_batch_generation: Cell::new(0.0),
        }
    }
}

/// ts: GrvtPublicWebSocket#mapIntervalToGrvt. Unknown intervals retain their original string.
pub fn map_interval_to_grvt(interval: &str) -> TableHit<String> {
    let mapping = [
        ("1m", "CI_1_M"),
        ("3m", "CI_3_M"),
        ("5m", "CI_5_M"),
        ("15m", "CI_15_M"),
        ("30m", "CI_30_M"),
        ("1h", "CI_1_H"),
        ("2h", "CI_2_H"),
        ("4h", "CI_4_H"),
        ("6h", "CI_6_H"),
        ("8h", "CI_8_H"),
        ("12h", "CI_12_H"),
        ("1d", "CI_1_D"),
        ("3d", "CI_3_D"),
        ("5d", "CI_5_D"),
        ("1w", "CI_1_W"),
        ("2w", "CI_2_W"),
        ("3w", "CI_3_W"),
        ("4w", "CI_4_W"),
    ];
    match js_table_lookup(&mapping, interval) {
        TableHit::Mapped(value) => TableHit::Mapped((*value).into()),
        TableHit::Inherited(member) => TableHit::Inherited(member),
        TableHit::Missing => TableHit::Mapped(interval.into()),
    }
}
fn interval_string(interval: TableHit<String>) -> String {
    match interval {
        TableHit::Mapped(value) => value,
        TableHit::Inherited(member) => member.js_string().into(),
        TableHit::Missing => unreachable!("interval mapper always supplies a fallback"),
    }
}
pub(super) fn wire<T: Serialize>(request: &T) -> Result<String, ExchangeError> {
    let value =
        serde_json::to_value(request).map_err(|cause| ExchangeError::new(cause.to_string()))?;
    safe_cex_num::js_json_stringify(&value.into())
        .map_err(|cause| ExchangeError::new(cause.message))
        .map(|wire| wire.expect("RPC request is an object"))
}
impl<T: Clone + PartialEq> PublicSubscriptions<T> {
    fn next_id(&self) -> f64 {
        let id = self.request_id.get();
        self.request_id.set(id + 1.0);
        id
    }
    fn subscribe(
        &self,
        id: f64,
        stream: &str,
        selector: String,
        send: &mut impl FnMut(&RpcRequest, &str) -> Result<(), ExchangeError>,
    ) -> Result<f64, ExchangeError> {
        let request = SubscribeRequest {
            jsonrpc: GrvtJsonRpcVersion::V2,
            id,
            method: GrvtSubscriptionMethod::Subscribe,
            params: SubscribeParams {
                stream: stream.into(),
                selectors: [selector],
            },
        };
        self.subscriptions
            .borrow_mut()
            .insert(NumberKey::new(id), request.clone());
        let request = RpcRequest::Subscribe(request);
        send(&request, &wire(&request)?)?;
        Ok(id)
    }
    fn unsubscribe(
        &self,
        subscription_id: f64,
        send: &mut impl FnMut(&RpcRequest, &str) -> Result<(), ExchangeError>,
    ) -> Result<(), ExchangeError> {
        let request = UnsubscribeRequest {
            jsonrpc: GrvtJsonRpcVersion::V2,
            id: self.next_id(),
            method: GrvtSubscriptionMethod::Unsubscribe,
            params: UnsubscribeParams { subscription_id },
        };
        let request = RpcRequest::Unsubscribe(request);
        send(&request, &wire(&request)?)?;
        self.subscriptions
            .borrow_mut()
            .shift_remove(&NumberKey::new(subscription_id));
        Ok(())
    }

    /// ts: GrvtPublicWebSocket.subscribeToKline.
    pub fn subscribe_to_kline(
        &self,
        instrument_id: &str,
        interval: &str,
        kind: Option<&str>,
        mut send: impl FnMut(&RpcRequest, &str) -> Result<(), ExchangeError>,
    ) -> Result<f64, ExchangeError> {
        let id = self.next_id();
        let interval = interval_string(map_interval_to_grvt(interval));
        let selector = format!("{instrument_id}@{interval}-{}", kind.unwrap_or("TRADE"));
        self.subscribe(id, "v1.candle", selector, &mut send)
    }
    /// ts: GrvtPublicWebSocket.unsubscribeFromKline.
    pub fn unsubscribe_from_kline(
        &self,
        subscription_id: f64,
        mut send: impl FnMut(&RpcRequest, &str) -> Result<(), ExchangeError>,
    ) -> Result<(), ExchangeError> {
        if self
            .subscriptions
            .borrow()
            .contains_key(&NumberKey::new(subscription_id))
        {
            self.unsubscribe(subscription_id, &mut send)?;
        }
        Ok(())
    }
    /// ts: GrvtPublicWebSocket.subscribeToOrderbook.
    pub fn subscribe_to_orderbook(
        &self,
        instrument_id: &str,
        refresh_ms: Option<f64>,
        depth: Option<f64>,
        mut send: impl FnMut(&RpcRequest, &str) -> Result<(), ExchangeError>,
    ) -> Result<f64, ExchangeError> {
        let id = self.next_id();
        let selector = format!(
            "{instrument_id}@{}-{}",
            js_number_string(refresh_ms.unwrap_or(500.0)),
            js_number_string(depth.unwrap_or(50.0))
        );
        self.subscribe(id, "v1.book.s", selector, &mut send)
    }
    /// ts: GrvtPublicWebSocket.unsubscribeFromOrderbook.
    pub fn unsubscribe_from_orderbook(
        &self,
        subscription_id: f64,
        mut send: impl FnMut(&RpcRequest, &str) -> Result<(), ExchangeError>,
    ) -> Result<(), ExchangeError> {
        if self
            .subscriptions
            .borrow()
            .contains_key(&NumberKey::new(subscription_id))
        {
            self.unsubscribe(subscription_id, &mut send)?;
        }
        Ok(())
    }
    /// ts: GrvtPublicWebSocket.subscribeToTicker.
    pub fn subscribe_to_ticker(
        &self,
        instrument_id: &str,
        refresh_ms: Option<f64>,
        stream: Option<&str>,
        mut send: impl FnMut(&RpcRequest, &str) -> Result<(), ExchangeError>,
    ) -> Result<f64, ExchangeError> {
        let id = self.next_id();
        let selector = format!(
            "{instrument_id}@{}",
            js_number_string(refresh_ms.unwrap_or(500.0))
        );
        self.subscribe(id, stream.unwrap_or("v1.ticker.s"), selector, &mut send)
    }
    /// ts: GrvtPublicWebSocket.unsubscribeFromTicker.
    pub fn unsubscribe_from_ticker(
        &self,
        subscription_id: f64,
        mut send: impl FnMut(&RpcRequest, &str) -> Result<(), ExchangeError>,
        mut clear: impl FnMut(&T) -> Result<(), ExchangeError>,
    ) -> Result<(), ExchangeError> {
        if self
            .subscriptions
            .borrow()
            .contains_key(&NumberKey::new(subscription_id))
        {
            self.cancel_ticker_batches(&mut clear)?;
            self.unsubscribe(subscription_id, &mut send)?;
            let instrument = self
                .ticker_subscriptions
                .borrow()
                .iter()
                .find(|(_, id)| **id == subscription_id)
                .map(|(instrument, _)| instrument.clone());
            if let Some(instrument) = instrument {
                self.ticker_subscriptions
                    .borrow_mut()
                    .shift_remove(&instrument);
            }
        }
        Ok(())
    }
    /// ts: GrvtPublicWebSocket#cancelTickerBatches.
    pub fn cancel_ticker_batches(
        &self,
        mut clear: impl FnMut(&T) -> Result<(), ExchangeError>,
    ) -> Result<(), ExchangeError> {
        self.ticker_batch_generation
            .set(self.ticker_batch_generation.get() + 1.0);
        self.ticker_batch_timers
            .iterators
            .set(self.ticker_batch_timers.iterators.get() + 1);
        let _iteration = TimerIteration(&self.ticker_batch_timers);
        let mut index = 0;
        while let Some(timer) = self.ticker_batch_timers.next(&mut index) {
            clear(&timer)?;
        }
        self.ticker_batch_timers.clear();
        Ok(())
    }
    /// ts: GrvtPublicWebSocket#ensureTickerSubscription.
    pub fn ensure_ticker_subscription(
        &self,
        instrument_id: &str,
        is_disposed: impl Fn() -> bool,
        is_connected: impl Fn() -> bool,
        mut send: impl FnMut(&RpcRequest, &str) -> Result<(), ExchangeError>,
    ) -> Result<Option<f64>, ExchangeError> {
        if is_disposed() || !is_connected() {
            return Ok(None);
        }
        if let Some(id) = self
            .ticker_subscriptions
            .borrow()
            .get(instrument_id)
            .copied()
        {
            return Ok(Some(id));
        }
        let id =
            self.subscribe_to_ticker(instrument_id, Some(500.0), Some("v1.ticker.d"), &mut send)?;
        self.ticker_subscriptions
            .borrow_mut()
            .insert(instrument_id.into(), id);
        Ok(Some(id))
    }
}
