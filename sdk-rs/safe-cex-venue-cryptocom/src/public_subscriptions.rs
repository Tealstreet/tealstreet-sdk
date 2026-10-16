//! Typed UTF-16 subscription bodies; socket/class and JSON transport wiring are separate.
use std::{
    cell::{Cell, RefCell},
    marker::PhantomData,
    rc::Rc,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelValue {
    String(Vec<u16>),
    Undefined,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArraySlot {
    Hole,
    Value(ChannelValue),
}

#[derive(Debug, Default)]
pub struct OrderedStringSet(RefCell<Vec<ChannelValue>>);
impl OrderedStringSet {
    pub fn new(values: impl IntoIterator<Item = ChannelValue>) -> Self {
        let set = Self::default();
        for value in values {
            set.insert(value);
        }
        set
    }
    pub fn insert(&self, value: ChannelValue) -> bool {
        let mut values = self.0.borrow_mut();
        if values.contains(&value) {
            false
        } else {
            values.push(value);
            true
        }
    }
    pub fn contains(&self, value: &ChannelValue) -> bool {
        self.0.borrow().iter().any(|item| item == value)
    }
    pub fn values(&self) -> Vec<ChannelValue> {
        self.0.borrow().clone()
    }
    pub fn remove(&self, value: &ChannelValue) -> bool {
        let mut values = self.0.borrow_mut();
        if let Some(index) = values.iter().position(|item| item == value) {
            values.remove(index);
            true
        } else {
            false
        }
    }
    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }
    pub fn is_empty(&self) -> bool {
        self.0.borrow().is_empty()
    }
}

/// Source array operation boundary: preserve length/slice/map lookup, index accessors,
/// inherited slots, custom iterator behavior and original errors in host adapters.
pub trait SubscriptionArray<E> {
    type Slice;
    type Mapped;
    type Iter<'a>: Iterator<Item = Result<ChannelValue, E>>
    where
        Self: 'a;
    fn length(&self) -> Result<f64, E>;
    fn slice(&self, start: f64, end: f64) -> Result<Self::Slice, E>;
    fn iterate(&self) -> Result<Self::Iter<'_>, E>;
    fn map<F: FnMut(ChannelValue) -> ChannelValue>(&self, transform: F) -> Result<Self::Mapped, E>;
}

pub struct SlotIterator<'a, E> {
    slots: &'a RefCell<Vec<ArraySlot>>,
    index: usize,
    error: PhantomData<E>,
}
impl<E> Iterator for SlotIterator<'_, E> {
    type Item = Result<ChannelValue, E>;
    fn next(&mut self) -> Option<Self::Item> {
        let slot = self.slots.borrow().get(self.index).cloned()?;
        self.index += 1;
        Some(Ok(match slot {
            ArraySlot::Hole => ChannelValue::Undefined,
            ArraySlot::Value(value) => value,
        }))
    }
}
impl<E> SubscriptionArray<E> for RefCell<Vec<ArraySlot>> {
    type Slice = Vec<ArraySlot>;
    type Mapped = Vec<ArraySlot>;
    type Iter<'a>
        = SlotIterator<'a, E>
    where
        Self: 'a;
    fn length(&self) -> Result<f64, E> {
        Ok(self.borrow().len() as f64)
    }
    fn slice(&self, start: f64, end: f64) -> Result<Vec<ArraySlot>, E> {
        let slots = self.borrow();
        Ok(slots[(start as usize).min(slots.len())..(end as usize).min(slots.len())].to_vec())
    }
    fn iterate(&self) -> Result<Self::Iter<'_>, E> {
        Ok(SlotIterator {
            slots: self,
            index: 0,
            error: PhantomData,
        })
    }
    fn map<F: FnMut(ChannelValue) -> ChannelValue>(
        &self,
        mut transform: F,
    ) -> Result<Vec<ArraySlot>, E> {
        let length = self.borrow().len();
        let mut result = Vec::with_capacity(length);
        for i in 0..length {
            let slot = self.borrow().get(i).cloned().unwrap_or(ArraySlot::Hole);
            result.push(match slot {
                ArraySlot::Hole => ArraySlot::Hole,
                ArraySlot::Value(value) => ArraySlot::Value(transform(value)),
            });
        }
        Ok(result)
    }
}

pub struct SubscriptionState {
    pub subscriptions: RefCell<Rc<OrderedStringSet>>,
    pub message_id: Cell<f64>,
}

#[derive(Debug, PartialEq)]
pub struct SubscriptionParams<C = Vec<ArraySlot>> {
    pub channels: C,
}
#[derive(Debug, PartialEq)]
pub struct SubscriptionFrame<C = Vec<ArraySlot>> {
    pub id: f64,
    pub method: &'static str,
    pub params: SubscriptionParams<C>,
    pub nonce: f64,
}

/// `prepare` resolves this.send, then JSON.stringify, before evaluating frame fields.
/// `send` stringifies the native frame and invokes that prepared send with its receiver;
/// native return values are ignored, while original errors are returned unchanged.
pub trait SubscriptionTransport<C = Vec<ArraySlot>> {
    type Prepared;
    type Error;
    fn prepare(&self) -> Result<Self::Prepared, Self::Error>;
    fn clock(&self) -> Result<f64, Self::Error>;
    fn send(
        &self,
        prepared: Self::Prepared,
        frame: SubscriptionFrame<C>,
    ) -> Result<(), Self::Error>;
}

fn subscriptions(state: &SubscriptionState) -> Rc<OrderedStringSet> {
    state.subscriptions.borrow().clone()
}

pub fn send_subscription_frames<
    E,
    A: SubscriptionArray<E>,
    T: SubscriptionTransport<A::Slice, Error = E>,
>(
    state: &SubscriptionState,
    channels: &A,
    transport: &T,
) -> Result<(), T::Error> {
    let mut i = 0.0;
    while i < channels.length()? {
        let prepared = transport.prepare()?;
        let id = state.message_id.get();
        state.message_id.set(id + 1.0);
        // The source reads/slices the live array after resolving the callees and id++.
        let channels = channels.slice(i, i + 100.0)?;
        let frame = SubscriptionFrame {
            id,
            method: "subscribe",
            params: SubscriptionParams { channels },
            nonce: transport.clock()?,
        };
        transport.send(prepared, frame)?;
        i += 100.0;
    }
    Ok(())
}

pub fn register_and_subscribe<R, P, E, A: SubscriptionArray<E>>(
    state: &SubscriptionState,
    channels: &A,
    resolve_frames: impl FnOnce() -> Result<P, E>,
    send_frames: impl FnOnce(P, Vec<ArraySlot>) -> Result<R, E>,
) -> Result<(), E> {
    let mut new_channels = Vec::new();
    for channel in channels.iterate()? {
        let channel = channel?;
        if subscriptions(state).contains(&channel) {
            continue;
        }
        subscriptions(state).insert(channel.clone());
        new_channels.push(ArraySlot::Value(channel));
    }
    let prepared = resolve_frames()?;
    send_frames(prepared, new_channels)?;
    Ok(())
}

pub fn resubscribe<R, P, E>(
    state: &SubscriptionState,
    resolve_frames: impl FnOnce() -> Result<P, E>,
    send_frames: impl FnOnce(P, Vec<ArraySlot>) -> Result<R, E>,
) -> Result<(), E> {
    if !subscriptions(state).is_empty() {
        let prepared = resolve_frames()?;
        let channels = subscriptions(state)
            .values()
            .into_iter()
            .map(ArraySlot::Value)
            .collect();
        send_frames(prepared, channels)?;
    }
    Ok(())
}

fn subscribe_channel<T: SubscriptionTransport>(
    state: &SubscriptionState,
    channel: Vec<u16>,
    transport: &T,
) -> Result<(), T::Error> {
    let channel = ChannelValue::String(channel);
    if !subscriptions(state).contains(&channel) {
        subscriptions(state).insert(channel.clone());
        let prepared = transport.prepare()?;
        let id = state.message_id.get();
        state.message_id.set(id + 1.0);
        let frame = SubscriptionFrame {
            id,
            method: "subscribe",
            params: SubscriptionParams {
                channels: vec![ArraySlot::Value(channel)],
            },
            nonce: transport.clock()?,
        };
        transport.send(prepared, frame)?;
    }
    Ok(())
}

pub fn subscribe_to_order_book<T: SubscriptionTransport>(
    state: &SubscriptionState,
    symbol: &[u16],
    depth: Option<f64>,
    transport: &T,
) -> Result<(), T::Error> {
    let channel = "book."
        .encode_utf16()
        .chain(symbol.iter().copied())
        .chain(".".encode_utf16())
        .chain(safe_cex_num::js_number_string(depth.unwrap_or(50.0)).encode_utf16())
        .collect();
    subscribe_channel(state, channel, transport)
}
pub fn subscribe_to_ticker<T: SubscriptionTransport>(
    state: &SubscriptionState,
    symbol: &[u16],
    transport: &T,
) -> Result<(), T::Error> {
    let channel = "ticker."
        .encode_utf16()
        .chain(symbol.iter().copied())
        .collect();
    subscribe_channel(state, channel, transport)
}
pub fn subscribe_to_tickers<R, P, E, A: SubscriptionArray<E>>(
    symbols: &A,
    resolve_register: impl FnOnce() -> Result<P, E>,
    register: impl FnOnce(P, A::Mapped) -> Result<R, E>,
) -> Result<(), E> {
    // Source resolves the method before evaluating symbols.map(...); map retains holes.
    let prepared = resolve_register()?;
    let channels = symbols.map(|value| {
        let symbol = match value {
            ChannelValue::String(value) => value,
            ChannelValue::Undefined => "undefined".encode_utf16().collect(),
        };
        ChannelValue::String("ticker.".encode_utf16().chain(symbol).collect())
    })?;
    register(prepared, channels)?;
    Ok(())
}
pub fn subscribe_to_trades<T: SubscriptionTransport>(
    state: &SubscriptionState,
    symbol: &[u16],
    transport: &T,
) -> Result<(), T::Error> {
    let channel = "trade."
        .encode_utf16()
        .chain(symbol.iter().copied())
        .collect();
    subscribe_channel(state, channel, transport)
}
pub fn subscribe_to_candlestick<T: SubscriptionTransport>(
    state: &SubscriptionState,
    symbol: &[u16],
    interval: &[u16],
    transport: &T,
) -> Result<(), T::Error> {
    let channel = "candlestick."
        .encode_utf16()
        .chain(interval.iter().copied())
        .chain(".".encode_utf16())
        .chain(symbol.iter().copied())
        .collect();
    subscribe_channel(state, channel, transport)
}
