//! GRVT public websocket lifecycle using bound socket, emitter and timer operations.
use crate::public_subscriptions::{PublicSubscriptions, RpcRequest, SubscriptionMap, wire};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::Market;
use std::{cell::RefCell, rc::Rc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketEvent {
    Open,
    Message,
    Close,
}

/// ts: GrvtPublicWebSocket.connectAndSubscribe. Listener registration reads the live socket slot.
pub fn connect_and_subscribe<S, H>(
    is_disposed: impl Fn() -> bool,
    is_testnet: impl Fn() -> bool,
    socket: &RefCell<Option<Rc<S>>>,
    handler: impl Fn(SocketEvent) -> Rc<H>,
    mut create: impl FnMut(&str) -> Result<Rc<S>, ExchangeError>,
    mut listen: impl FnMut(&S, SocketEvent, &H) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    if !is_disposed() {
        let env = if is_testnet() {
            "TESTNET"
        } else {
            "PRODUCTION"
        };
        let url = crate::protocol::WSS_URLS
            .iter()
            .find(|(name, _)| *name == env)
            .expect("closed environment")
            .1;
        *socket.borrow_mut() = Some(create(url)?);
        for event in [SocketEvent::Open, SocketEvent::Message, SocketEvent::Close] {
            let current = socket.borrow().clone().ok_or_else(|| {
                let mut error = ExchangeError::new(
                    "Cannot read properties of undefined (reading 'addEventListener')",
                );
                error.name = "TypeError".into();
                error
            })?;
            let callback = handler(event);
            listen(&current, event, &callback)?;
        }
    }
    Ok(())
}

/// ts: GrvtPublicWebSocket.onOpen. Operations execute in source order after the disposed guard.
pub fn on_open<H>(
    handler: impl Fn() -> Rc<H>,
    is_disposed: impl Fn() -> bool,
    mut track: impl FnMut() -> Result<(), ExchangeError>,
    mut replay: impl FnMut() -> Result<(), ExchangeError>,
    mut subscribe_all: impl FnMut() -> Result<(), ExchangeError>,
    mut off: impl FnMut(&str, &H) -> Result<(), ExchangeError>,
    mut on: impl FnMut(&str, &H) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    if !is_disposed() {
        track()?;
        replay()?;
        subscribe_all()?;
        off("markets:changed", &handler())?;
        on("markets:changed", &handler())?;
    }
    Ok(())
}

pub trait MarketIdentity {
    fn id(&self) -> &str;
    fn symbol(&self) -> &str;
}
impl MarketIdentity for Market {
    fn id(&self) -> &str {
        &self.id
    }
    fn symbol(&self) -> &str {
        &self.symbol
    }
}
#[derive(Debug, Clone)]
pub struct AddedMarket {
    pub id: String,
    pub symbol: String,
}
impl MarketIdentity for AddedMarket {
    fn id(&self) -> &str {
        &self.id
    }
    fn symbol(&self) -> &str {
        &self.symbol
    }
}
pub type AddedMarkets<M = Market> = RefCell<Vec<Rc<RefCell<M>>>>;

/// ts: GrvtPublicWebSocket#onMarketsChanged. Error reporting reads the retained live row.
pub fn on_markets_changed<T: Clone + PartialEq, M: MarketIdentity>(
    state: &PublicSubscriptions<T>,
    added: &AddedMarkets<M>,
    has_socket: impl Fn() -> bool,
    is_disposed: impl Fn() -> bool,
    is_connected: impl Fn() -> bool,
    mut send: impl FnMut(&RpcRequest, &str) -> Result<(), ExchangeError>,
    mut emit: impl FnMut(&str) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    if added.borrow().is_empty() || !has_socket() || is_disposed() {
        return Ok(());
    }
    if !is_connected() {
        return Ok(());
    }
    let mut index = 0;
    loop {
        let Some(market) = added.borrow().get(index).cloned() else {
            break;
        };
        index += 1;
        let instrument = market.borrow().id().to_owned();
        if let Err(cause) =
            state.ensure_ticker_subscription(&instrument, &is_disposed, &is_connected, &mut send)
        {
            let symbol = market.borrow().symbol().to_owned();
            emit(&format!(
                "Error subscribing to ticker for {symbol}: {}",
                cause.js_error_string()
            ))?;
        }
    }
    Ok(())
}
struct ReplayIteration<'a>(&'a RefCell<SubscriptionMap>);
impl Drop for ReplayIteration<'_> {
    fn drop(&mut self) {
        self.0.borrow_mut().finish_replay();
    }
}

/// ts: GrvtPublicWebSocket#resubscribe. Iteration observes live insert/delete/reinsert operations.
pub fn resubscribe<T: Clone + PartialEq>(
    state: &PublicSubscriptions<T>,
    mut send: impl FnMut(&RpcRequest, &str) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    state.subscriptions.borrow_mut().start_replay();
    let _iteration = ReplayIteration(&state.subscriptions);
    let mut index = 0;
    loop {
        let request = state.subscriptions.borrow().next_replay(&mut index);
        let Some(request) = request else {
            break;
        };
        let request = RpcRequest::Subscribe(request);
        send(&request, &wire(&request)?)?;
    }
    Ok(())
}

pub type TimerCallback = Box<dyn FnMut() -> Result<(), ExchangeError>>;
pub trait TickerBatchHost<T> {
    fn is_disposed(&self) -> bool;
    fn is_connected(&self) -> bool;
    fn markets(&self) -> Result<Rc<AddedMarkets>, ExchangeError>;
    fn clear(&self, timer: &T) -> Result<(), ExchangeError>;
    fn schedule(&self, delay_ms: f64, callback: TimerCallback) -> Result<T, ExchangeError>;
    fn send(&self, request: &RpcRequest, wire: &str) -> Result<(), ExchangeError>;
    fn emit(&self, message: &str) -> Result<(), ExchangeError>;
}

/// ts: GrvtPublicWebSocket.subscribeToAllTickers. Batch slices retain shared Market references.
pub fn subscribe_to_all_tickers<T: Clone + PartialEq + 'static>(
    state: Rc<PublicSubscriptions<T>>,
    host: Rc<dyn TickerBatchHost<T>>,
) -> Result<(), ExchangeError> {
    if host.is_disposed() || !host.is_connected() {
        return Ok(());
    }
    state.cancel_ticker_batches(|timer| host.clear(timer))?;
    let generation = state.ticker_batch_generation.get();
    let markets = host.markets()?;
    if markets.borrow().is_empty() {
        return Ok(());
    }
    let batch_size = 20;
    let mut index = 0;
    while index < markets.borrow().len() {
        let batch = {
            let current = markets.borrow();
            current[index..usize::min(index + batch_size, current.len())].to_vec()
        };
        let callback_state = state.clone();
        let callback_host = host.clone();
        let handle = Rc::new(RefCell::new(None));
        let callback_handle = handle.clone();
        let callback = Box::new(move || {
            let timer = callback_handle.borrow().clone().ok_or_else(|| {
                let mut error = ExchangeError::new("Cannot access 'timeout' before initialization");
                error.name = "ReferenceError".into();
                error
            })?;
            callback_state.ticker_batch_timers.remove(&timer);
            if callback_host.is_disposed()
                || generation != callback_state.ticker_batch_generation.get()
                || !callback_host.is_connected()
            {
                return Ok(());
            }
            for market in &batch {
                let instrument = market.borrow().id.clone();
                if let Err(cause) = callback_state.ensure_ticker_subscription(
                    &instrument,
                    || callback_host.is_disposed(),
                    || callback_host.is_connected(),
                    |request, wire| callback_host.send(request, wire),
                ) {
                    let symbol = market.borrow().symbol.clone();
                    callback_host.emit(&format!(
                        "Error subscribing to ticker for {}: {}",
                        symbol,
                        cause.js_error_string()
                    ))?;
                }
            }
            Ok(())
        });
        let timer = host.schedule((index / batch_size) as f64 * 100.0, callback)?;
        *handle.borrow_mut() = Some(timer.clone());
        state.ticker_batch_timers.insert(timer);
        index += batch_size;
    }
    Ok(())
}

/// ts: GrvtPublicWebSocket.dispose. Cancellation and emitter removal precede base disposal.
pub fn dispose<T: Clone + PartialEq, H>(
    handler: impl Fn() -> Rc<H>,
    state: &PublicSubscriptions<T>,
    mut clear: impl FnMut(&T) -> Result<(), ExchangeError>,
    mut off: impl FnMut(&str, &H) -> Result<(), ExchangeError>,
    mut base: impl FnMut() -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    state.cancel_ticker_batches(&mut clear)?;
    state.subscriptions.borrow_mut().clear();
    state.ticker_subscriptions.borrow_mut().clear();
    off("markets:changed", &handler())?;
    base()?;
    Ok(())
}
