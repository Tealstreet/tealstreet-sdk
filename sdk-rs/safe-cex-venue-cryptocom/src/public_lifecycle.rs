//! Public websocket bodies at UTF-16/native state and explicit host boundaries.
use crate::{
    private_lifecycle::UnsubscribeChannels,
    public_subscriptions::{
        ArraySlot, ChannelValue, OrderedStringSet, SubscriptionFrame, SubscriptionParams,
        SubscriptionState, SubscriptionTransport,
    },
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CryptocomMarketDataSubscription {
    pub instrument_name: Vec<u16>,
    pub index_instrument_name: Option<Vec<u16>>,
}
#[derive(Default)]
pub struct IndexMarketMap(RefCell<IndexEntries>);
type IndexEntries = Vec<(Vec<u16>, Rc<OrderedStringSet>)>;
impl IndexMarketMap {
    pub fn get(&self, key: &[u16]) -> Option<Rc<OrderedStringSet>> {
        self.0
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    }
    pub fn set(&self, key: Vec<u16>, value: Rc<OrderedStringSet>) {
        let mut entries = self.0.borrow_mut();
        if let Some((_, v)) = entries.iter_mut().find(|(k, _)| k == &key) {
            *v = value;
        } else {
            entries.push((key, value));
        }
    }
    pub fn entries(&self) -> Vec<(Vec<u16>, Rc<OrderedStringSet>)> {
        self.0.borrow().clone()
    }
    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }
}
pub struct PublicSocketState<W, H> {
    pub channels: SubscriptionState,
    pub index_markets: RefCell<Rc<IndexMarketMap>>,
    pub is_disposed: Cell<bool>,
    pub reconnect_subscription_timer: RefCell<Option<Rc<H>>>,
    pub ws: RefCell<Option<Rc<W>>>,
}
fn subscriptions<W, H>(state: &PublicSocketState<W, H>) -> Rc<OrderedStringSet> {
    state.channels.subscriptions.borrow().clone()
}
fn index_markets<W, H>(state: &PublicSocketState<W, H>) -> Rc<IndexMarketMap> {
    state.index_markets.borrow().clone()
}
pub trait PublicConnectionHost<W> {
    type Error;
    type Urls;
    fn testnet(&self) -> Result<bool, Self::Error>;
    fn urls(&self, environment: &'static str) -> Result<Self::Urls, Self::Error>;
    /// Resolve createWebsocket before reading MARKET on the original urls carrier.
    fn create(&self, urls: Self::Urls) -> Result<Rc<W>, Self::Error>;
    fn add_listener(&self, socket: Option<Rc<W>>, event: &'static str) -> Result<(), Self::Error>;
}
pub fn connect_and_subscribe<W, H, T: PublicConnectionHost<W>>(
    state: &PublicSocketState<W, H>,
    host: &T,
) -> Result<(), T::Error> {
    if !state.is_disposed.get() {
        let environment = if host.testnet()? {
            "TESTNET"
        } else {
            "PRODUCTION"
        };
        let urls = host.urls(environment)?;
        let socket = host.create(urls)?;
        state.ws.replace(Some(socket));
        for event in ["open", "message", "close"] {
            let socket = state.ws.borrow().clone();
            host.add_listener(socket, event)?;
        }
    }
    Ok(())
}
pub trait PublicTimerHost<H> {
    type Error;
    type ResubscribeReturn;
    fn track_last_received(&self) -> Result<(), Self::Error>;
    fn clear_timeout(&self, timer: Option<Rc<H>>) -> Result<(), Self::Error>;
    fn set_timeout<F: Fn() -> Result<(), Self::Error> + 'static>(
        &self,
        callback: F,
        delay: f64,
    ) -> Result<Rc<H>, Self::Error>;
    fn resubscribe(&self) -> Result<Self::ResubscribeReturn, Self::Error>;
}
pub fn run_reconnect_subscribe<W, H, R, E>(
    state: &PublicSocketState<W, H>,
    resubscribe: impl FnOnce() -> Result<R, E>,
) -> Result<(), E> {
    if !state.is_disposed.get() {
        resubscribe()?;
    }
    Ok(())
}
pub fn on_open<W: 'static, H: 'static, T: PublicTimerHost<H> + 'static>(
    state: Rc<PublicSocketState<W, H>>,
    host: Rc<T>,
) -> Result<(), T::Error> {
    if !state.is_disposed.get() {
        host.track_last_received()?;
        let timer = state.reconnect_subscription_timer.borrow().clone();
        host.clear_timeout(timer)?;
        if !subscriptions(&state).is_empty() {
            let callback_state = state.clone();
            let callback_host = host.clone();
            let timer = host.set_timeout(
                move || run_reconnect_subscribe(&callback_state, || callback_host.resubscribe()),
                1000.0,
            )?;
            state.reconnect_subscription_timer.replace(Some(timer));
        }
    }
    Ok(())
}

/// Host iterators own resolution/next/IteratorClose effects; Drop closes a live iterator
/// on body error without replacing that error, while next errors/exhaustion retain source rules.
pub trait MarketSubscriptions<E> {
    type Item;
    type Iter<'a>: Iterator<Item = Result<Self::Item, E>>
    where
        Self: 'a;
    fn iterate(&self) -> Result<Self::Iter<'_>, E>;
    fn instrument_name(&self, item: &Self::Item) -> Result<Vec<u16>, E>;
    fn index_instrument_name(&self, item: &Self::Item) -> Result<Option<Vec<u16>>, E>;
}
pub trait MarketSubscriptionHost {
    type Error;
    type Registered;
    fn log_warning(&self, message: Vec<u16>) -> Result<(), Self::Error>;
    /// Resolve registration first, then pass the original locally-created channel array; return ignored.
    fn register(&self, channels: Vec<ArraySlot>) -> Result<Self::Registered, Self::Error>;
}
pub struct SkippedChannels {
    pub ticker: f64,
    pub valuation: f64,
    pub invalid_index: f64,
}
fn channel(prefix: &str, instrument: &[u16]) -> ChannelValue {
    ChannelValue::String(
        prefix
            .encode_utf16()
            .chain(instrument.iter().copied())
            .collect(),
    )
}
pub fn queue_channel<W, H>(
    state: &PublicSocketState<W, H>,
    channels: &mut Vec<ChannelValue>,
    skipped: &mut SkippedChannels,
    channel: ChannelValue,
    optional: bool,
) -> bool {
    if subscriptions(state).contains(&channel) || channels.contains(&channel) {
        return true;
    }
    if subscriptions(state).values().len() as f64 + channels.len() as f64 >= 350.0 {
        if optional {
            skipped.valuation += 1.0;
        } else {
            skipped.ticker += 1.0;
        }
        return false;
    }
    channels.push(channel);
    true
}
pub fn subscribe_to_market_data<
    W,
    H,
    M: MarketSubscriptions<T::Error>,
    T: MarketSubscriptionHost,
>(
    state: &PublicSocketState<W, H>,
    markets: &M,
    host: &T,
) -> Result<(), T::Error> {
    let mut channels = Vec::new();
    let mut skipped = SkippedChannels {
        ticker: 0.0,
        valuation: 0.0,
        invalid_index: 0.0,
    };
    for market in markets.iterate()? {
        let market = market?;
        let instrument = markets.instrument_name(&market)?;
        queue_channel(
            state,
            &mut channels,
            &mut skipped,
            channel("ticker.", &instrument),
            false,
        );
    }
    for market in markets.iterate()? {
        let market = market?;
        let instrument = markets.instrument_name(&market)?;
        let index = markets.index_instrument_name(&market)?;
        queue_channel(
            state,
            &mut channels,
            &mut skipped,
            channel("mark.", &instrument),
            true,
        );
        if let Some(index) = index
            .as_ref()
            .filter(|index| index.ends_with(&"-INDEX".encode_utf16().collect::<Vec<_>>()))
        {
            if queue_channel(
                state,
                &mut channels,
                &mut skipped,
                channel("index.", index),
                true,
            ) {
                let ids = index_markets(state)
                    .get(index)
                    .unwrap_or_else(|| Rc::new(OrderedStringSet::default()));
                ids.insert(ChannelValue::String(instrument.clone()));
                index_markets(state).set(index.clone(), ids);
            }
        } else if index.as_ref().is_some_and(|index| !index.is_empty()) {
            skipped.invalid_index += 1.0;
        }
        queue_channel(
            state,
            &mut channels,
            &mut skipped,
            channel("funding.", &instrument),
            true,
        );
    }
    if skipped.ticker > 0.0 || skipped.valuation > 0.0 || skipped.invalid_index > 0.0 {
        let summary = format!(
            "Crypto.com public WS cap 350/400; skipped {} ticker / {} valuation / {} invalid index channels",
            safe_cex_num::js_number_string(skipped.ticker),
            safe_cex_num::js_number_string(skipped.valuation),
            safe_cex_num::js_number_string(skipped.invalid_index)
        );
        host.log_warning(summary.encode_utf16().collect())?;
    }
    host.register(channels.into_iter().map(ArraySlot::Value).collect())?;
    Ok(())
}

/// Native filter/map and constructor iteration preserve original callbacks/carriers
/// and source iterator closing effects; JavaScript builtin binding is a host dependency.
pub trait MarketCatalog<E> {
    type Item;
    type Filtered;
    type Mapped;
    type Iter: Iterator<Item = Result<ChannelValue, E>>;
    fn filter<F: FnMut(&Self::Item) -> Result<bool, E>>(
        &self,
        predicate: F,
    ) -> Result<Self::Filtered, E>;
    fn underlying_matches(&self, item: &Self::Item, index: &[u16]) -> Result<bool, E>;
    fn map<F: FnMut(&Self::Item) -> Result<ChannelValue, E>>(
        &self,
        filtered: Self::Filtered,
        transform: F,
    ) -> Result<Self::Mapped, E>;
    fn market_id(&self, item: &Self::Item) -> Result<ChannelValue, E>;
    fn iterate(&self, mapped: Self::Mapped) -> Result<Self::Iter, E>;
}
pub fn get_market_ids_by_index_instrument<W, H, E, T: MarketCatalog<E>>(
    state: &PublicSocketState<W, H>,
    index: &[u16],
    catalog: &T,
) -> Result<Rc<OrderedStringSet>, E> {
    if let Some(cached) = index_markets(state).get(index) {
        return Ok(cached);
    }
    let filtered = catalog.filter(|market| catalog.underlying_matches(market, index))?;
    let mapped = catalog.map(filtered, |market| catalog.market_id(market))?;
    let ids = Rc::new(OrderedStringSet::default());
    for id in catalog.iterate(mapped)? {
        ids.insert(id?);
    }
    if !ids.is_empty() {
        index_markets(state).set(index.to_vec(), ids.clone());
    }
    Ok(ids)
}

pub enum ValuationValue {
    String(Vec<u16>),
    Other,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ValuationPatch {
    Mark(f64),
    Index(f64),
    FundingRate(f64),
}
pub fn valuation_number(text: &[u16]) -> Option<f64> {
    // A lone surrogate cannot belong to numeric grammar, so reject it rather than project it.
    let text = String::from_utf16(text).ok()?;
    if safe_cex_num::js_trim(&text).is_empty() {
        return None;
    }
    let value = safe_cex_num::number_from_string(&text);
    value.is_finite().then_some(value)
}
/// Store/property/callee operations retain original receivers/errors. ID iterators
/// remain live and implement source IteratorClose effects on body error in their adapter.
pub trait ValuationHost {
    type Error;
    type Prepared;
    type Ids;
    type Iter<'a>: Iterator<Item = Result<ChannelValue, Self::Error>>
    where
        Self: 'a;
    type Market;
    type Ticker;
    fn raw_value(&self) -> Result<ValuationValue, Self::Error>;
    fn channel(&self) -> Result<Vec<u16>, Self::Error>;
    fn prepare_index_ids(&self) -> Result<Self::Prepared, Self::Error>;
    fn instrument_name(&self) -> Result<Vec<u16>, Self::Error>;
    fn index_ids(
        &self,
        prepared: Self::Prepared,
        index: Vec<u16>,
    ) -> Result<Self::Ids, Self::Error>;
    fn iterate_ids(&self, ids: Self::Ids) -> Result<Self::Iter<'_>, Self::Error>;
    fn market(&self, id: ChannelValue) -> Result<Option<Self::Market>, Self::Error>;
    /// Source store/table resolution followed by update.instrument_name lookup.
    fn market_from_update(&self) -> Result<Option<Self::Market>, Self::Error>;
    fn ticker(&self, market: Self::Market) -> Result<Option<Self::Ticker>, Self::Error>;
    fn update(&self, ticker: Self::Ticker, patch: ValuationPatch) -> Result<(), Self::Error>;
}
fn is_channel(actual: Vec<u16>, expected: &str) -> bool {
    actual == expected.encode_utf16().collect::<Vec<_>>()
}
pub fn handle_valuation_update<T: ValuationHost>(host: &T) -> Result<(), T::Error> {
    let ValuationValue::String(raw) = host.raw_value()? else {
        return Ok(());
    };
    let Some(value) = valuation_number(&raw) else {
        return Ok(());
    };
    if (is_channel(host.channel()?, "mark") || is_channel(host.channel()?, "index")) && value <= 0.0
    {
        return Ok(());
    }
    if is_channel(host.channel()?, "index") {
        let prepared = host.prepare_index_ids()?;
        let index = host.instrument_name()?;
        let ids = host.index_ids(prepared, index)?;
        for id in host.iterate_ids(ids)? {
            let Some(market) = host.market(id?)? else {
                continue;
            };
            if let Some(ticker) = host.ticker(market)? {
                host.update(ticker, ValuationPatch::Index(value))?;
            }
        }
        return Ok(());
    }
    let Some(market) = host.market_from_update()? else {
        return Ok(());
    };
    let Some(ticker) = host.ticker(market)? else {
        return Ok(());
    };
    if is_channel(host.channel()?, "mark") {
        host.update(ticker, ValuationPatch::Mark(value))?;
    } else if is_channel(host.channel()?, "funding") {
        host.update(ticker, ValuationPatch::FundingRate(value))?;
    }
    Ok(())
}

pub trait CandlestickHost<U> {
    type Error;
    type Callbacks;
    type Callback;
    type Return;
    fn subscription(&self, update: &U) -> Result<Vec<u16>, Self::Error>;
    fn callbacks(&self, handler: Vec<u16>) -> Result<Self::Callbacks, Self::Error>;
    fn for_each<F: FnMut(Self::Callback) -> Result<Self::Return, Self::Error>>(
        &self,
        callbacks: Self::Callbacks,
        callback: F,
    ) -> Result<(), Self::Error>;
    fn call(&self, callback: Self::Callback, update: &U) -> Result<Self::Return, Self::Error>;
}
pub fn handle_candlestick_update<U, T: CandlestickHost<U>>(
    update: &U,
    host: &T,
) -> Result<(), T::Error> {
    let handler = host.subscription(update)?;
    let callbacks = host.callbacks(handler)?;
    host.for_each(callbacks, |callback| host.call(callback, update))?;
    Ok(())
}
pub fn unsubscribe<
    W,
    H,
    E,
    A: UnsubscribeChannels<E>,
    T: SubscriptionTransport<A::Native, Error = E>,
>(
    state: &PublicSocketState<W, H>,
    channels: &A,
    transport: &T,
) -> Result<(), E> {
    if channels.length()? > 0.0 {
        let prepared = transport.prepare()?;
        let id = state.channels.message_id.get();
        state.channels.message_id.set(id + 1.0);
        let frame = SubscriptionFrame {
            id,
            method: "unsubscribe",
            params: SubscriptionParams {
                channels: channels.native(),
            },
            nonce: transport.clock()?,
        };
        transport.send(prepared, frame)?;
        channels.for_each(|channel| Ok(subscriptions(state).remove(&channel)))?;
    }
    Ok(())
}
pub trait PublicDisposeHost<W, H> {
    type Error;
    fn clear_timeout(&self, timer: Option<Rc<H>>) -> Result<(), Self::Error>;
    fn close(&self, socket: Option<Rc<W>>) -> Result<(), Self::Error>;
}
pub fn dispose<W, H, T: PublicDisposeHost<W, H>>(
    state: &PublicSocketState<W, H>,
    host: &T,
) -> Result<(), T::Error> {
    let timer = state.reconnect_subscription_timer.borrow().clone();
    host.clear_timeout(timer)?;
    subscriptions(state).clear();
    index_markets(state).clear();
    if state.ws.borrow().is_some() {
        let socket = state.ws.borrow().clone();
        host.close(socket)?;
    }
    Ok(())
}

#[cfg(test)]
mod accepted_helpers {
    use super::*;
    #[test]
    fn channel_native_contract() {
        let cases = [
            (
                "ticker.",
                vec![65, 66],
                vec![116, 105, 99, 107, 101, 114, 46, 65, 66],
            ),
            ("", vec![], vec![]),
            (
                "Ω.",
                vec![0xd800, 65, 0xdc00],
                vec![937, 46, 0xd800, 65, 0xdc00],
            ),
            ("🙂", vec![0], vec![0xd83d, 0xde42, 0]),
        ];
        assert_eq!(cases.len(), 4, "public channel independent cases");
        let mut executed = 0;
        for (prefix, instrument, expected) in cases {
            match channel(prefix, &instrument) {
                ChannelValue::String(actual) => {
                    assert_eq!(actual, expected, "public channel native UTF16")
                }
                _ => panic!("public channel native variant"),
            }
            executed += 1;
        }
        assert_eq!(executed, 4, "public channel executed cases");
    }
    #[test]
    fn is_channel_native_contract() {
        let cases:Vec<serde_json::Value>=serde_json::from_str(r###"[{"input":{"value":[],"literal":""},"output":true},{"input":{"value":[109,97,114,107,101,116],"literal":"market"},"output":true},{"input":{"value":[77,97,114,107,101,116],"literal":"market"},"output":false},{"input":{"value":[109,97,114,107,101,116,88],"literal":"market"},"output":false},{"input":{"value":[109,97,114,107,101],"literal":"market"},"output":false},{"input":{"value":[],"literal":"market"},"output":false},{"input":{"value":[109,97,114,107,101,116],"literal":""},"output":false},{"input":{"value":[109,97,114,107,101,116,0],"literal":"market"},"output":false},{"input":{"value":[0],"literal":"\u0000"},"output":true},{"input":{"value":[97,0,98],"literal":"a\u0000b"},"output":true},{"input":{"value":[937],"literal":"\u03a9"},"output":true},{"input":{"value":[55357,56898],"literal":"\ud83d\ude42"},"output":true},{"input":{"value":[233],"literal":"\u00e9"},"output":true},{"input":{"value":[101,769],"literal":"\u00e9"},"output":false},{"input":{"value":[233],"literal":"e\u0301"},"output":false},{"input":{"value":[55296],"literal":""},"output":false},{"input":{"value":[56320],"literal":""},"output":false},{"input":{"value":[109,97,114,107,101,116,55296],"literal":"market"},"output":false},{"input":{"value":[56320,109,97,114,107,101,116],"literal":"market"},"output":false},{"input":{"value":[55357,56898,120],"literal":"\ud83d\ude42"},"output":false},{"input":{"value":[120,55357,56898],"literal":"\ud83d\ude42"},"output":false},{"input":{"value":[937],"literal":"\u03c9"},"output":false}]"###).unwrap();
        assert_eq!(cases.len(), 22, "public is_channel independent cases");
        let mut executed = 0;
        for case in cases {
            let value: Vec<u16> = serde_json::from_value(case["input"]["value"].clone()).unwrap();
            let literal = case["input"]["literal"].as_str().unwrap();
            assert_eq!(
                is_channel(value, literal),
                case["output"].as_bool().unwrap(),
                "public is_channel native comparison input {}",
                case["input"]
            );
            executed += 1;
        }
        assert_eq!(executed, 22, "public is_channel executed cases");
    }
}
