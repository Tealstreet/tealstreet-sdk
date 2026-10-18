//! Startup, polling and order-book recovery bodies over compatible native hosts.
use crate::{
    constants,
    orderbook_state::StateRef,
    public_control::{JsString, SubscriptionState, Subscriptions},
};
use safe_cex_num::js_number_string;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}
fn units(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn join(parts: &[&[u16]]) -> JsString {
    parts.iter().flat_map(|p| p.iter().copied()).collect()
}
pub trait Logger {
    type Error;
    type LogTarget;
    fn log_target(&mut self) -> Result<Self::LogTarget, Self::Error>;
    fn log(
        &mut self,
        target: Self::LogTarget,
        message: JsString,
        severity: Option<Severity>,
    ) -> Result<(), Self::Error>;
    fn error_text(&mut self, error: &Self::Error) -> Result<JsString, Self::Error>;
}

pub type NativeMap<V> = Rc<RefCell<Vec<(JsString, V)>>>;
pub struct PublicSocketState<B, U, H, F, T, R> {
    pub ticker_handler: Option<R>,
    pub base: B,
    pub subscriptions: SubscriptionState,
    pub topics: NativeMap<f64>,
    pub order_book_state: NativeMap<StateRef<U, H>>,
    pub snapshot_fetchers: NativeMap<F>,
    pub depth_throttles: NativeMap<T>,
}
// ts: BackpackPublicWebSocket.constructor
pub fn construct_public<B, U, H, F, T, R, E>(
    super_constructor: impl FnOnce() -> Result<B, E>,
    initialize_ticker: impl FnOnce(
        &PublicSocketState<B, U, H, F, T, R>,
        f64,
        bool,
        bool,
    ) -> Result<R, E>,
) -> Result<PublicSocketState<B, U, H, F, T, R>, E> {
    let base = super_constructor()?;
    let mut socket = PublicSocketState {
        ticker_handler: None,
        base,
        subscriptions: Rc::new(RefCell::new(Subscriptions::default())),
        topics: Rc::new(RefCell::new(Vec::new())),
        order_book_state: Rc::new(RefCell::new(Vec::new())),
        snapshot_fetchers: Rc::new(RefCell::new(Vec::new())),
        depth_throttles: Rc::new(RefCell::new(Vec::new())),
    };
    socket.ticker_handler = Some(initialize_ticker(&socket, 1000.0, true, true)?);
    Ok(socket)
}
#[derive(Debug)]
pub struct ExchangeFields {
    pub account_leverage_limit: Cell<f64>,
    pub grafana_every: &'static [&'static str],
    pub grafana_periodic: &'static [&'static str],
    pub client_id_label: &'static str,
    pub client_id_requirement: &'static str,
}
impl ExchangeFields {
    pub fn valid_client_id(&self, id: &[u16]) -> bool {
        if id.is_empty()
            || id.len() > 10
            || !(49..=57).contains(&id[0])
            || !id.iter().all(|c| (48..=57).contains(c))
        {
            return false;
        }
        id.iter().fold(0u64, |n, c| n * 10 + u64::from(*c - 48)) <= 0xffff_ffff
    }
}
pub trait ConstructorHost {
    type Error;
    type Base;
    type Xhr;
    type Socket;
    fn super_constructor(&mut self) -> Result<Self::Base, Self::Error>;
    fn initialize_fields(
        &mut self,
        base: &Self::Base,
        fields: ExchangeFields,
    ) -> Result<(), Self::Error>;
    /// Composes the supplied createAPI factory and rateLimit middleware with the original opts.
    fn create_xhr(&mut self, base: &Self::Base, max_rps: f64) -> Result<Self::Xhr, Self::Error>;
    fn assign_xhr(&mut self, base: &Self::Base, xhr: Self::Xhr) -> Result<(), Self::Error>;
    fn create_public_socket(&mut self, base: &Self::Base) -> Result<Self::Socket, Self::Error>;
    fn assign_public_socket(
        &mut self,
        base: &Self::Base,
        socket: Self::Socket,
    ) -> Result<(), Self::Error>;
    fn create_private_socket(&mut self, base: &Self::Base) -> Result<Self::Socket, Self::Error>;
    fn assign_private_socket(
        &mut self,
        base: &Self::Base,
        socket: Self::Socket,
    ) -> Result<(), Self::Error>;
}
// ts: BackpackExchange.constructor
pub fn construct_exchange<H: ConstructorHost>(host: &mut H) -> Result<H::Base, H::Error> {
    let base = host.super_constructor()?;
    host.initialize_fields(
        &base,
        ExchangeFields {
            account_leverage_limit: Cell::new(1.0),
            grafana_every: constants::GRAFANA_ENDPOINTS_EVERY,
            grafana_periodic: constants::GRAFANA_ENDPOINTS_PERIODIC,
            client_id_label: "Backpack",
            client_id_requirement: "be a canonical decimal uint32 between 1 and 4294967295",
        },
    )?;
    let xhr = host.create_xhr(&base, 10.0)?;
    host.assign_xhr(&base, xhr)?;
    let public = host.create_public_socket(&base)?;
    host.assign_public_socket(&base, public)?;
    let private = host.create_private_socket(&base)?;
    host.assign_private_socket(&base, private)?;
    Ok(base)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadKind {
    Markets,
    Tickers,
    Tick,
    Account,
    Positions,
    Balance,
    Orders,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    Ticker,
    Trades,
    BookTicker,
}
pub type Callback<E> = Box<dyn FnOnce() -> Result<(), E>>;
pub trait RuntimeHost: Logger {
    type Value: Clone;
    type FetchFuture: Future<Output = Result<Self::Value, Self::Error>>;
    type UpdateTarget;
    type Market;
    type Subscriber;
    fn fetch(&mut self, kind: ReadKind) -> Result<Self::FetchFuture, Self::Error>;
    fn disposed(&mut self) -> Result<bool, Self::Error>;
    fn public_only(&mut self) -> Result<bool, Self::Error>;
    fn update_target(&mut self) -> Result<Self::UpdateTarget, Self::Error>;
    fn loaded(&mut self) -> Result<Vec<(JsString, bool)>, Self::Error>;
    fn update(
        &mut self,
        target: Self::UpdateTarget,
        kind: Option<ReadKind>,
        value: Option<Self::Value>,
        loaded: Vec<(JsString, bool)>,
    ) -> Result<(), Self::Error>;
    fn length(&mut self, value: &Self::Value) -> Result<usize, Self::Error>;
    fn connect_public(&mut self) -> Result<(), Self::Error>;
    fn connect_private(&mut self) -> Result<(), Self::Error>;
    fn subscribe_all(&mut self, markets: Self::Value) -> Result<(), Self::Error>;
    fn positions_upnl(&mut self, positions: &Self::Value) -> Result<f64, Self::Error>;
    fn set_balance_upnl(&mut self, balance: &Self::Value, upnl: f64) -> Result<(), Self::Error>;
    fn error_message(&mut self, error: &Self::Error) -> Result<JsString, Self::Error>;
    fn schedule_loop(
        &mut self,
        callback: Callback<Self::Error>,
        delay: f64,
    ) -> Result<(), Self::Error>;
    fn repeat_tick(&mut self) -> Result<(), Self::Error>;
    fn timeout(&mut self, callback: Callback<Self::Error>, delay: f64) -> Result<(), Self::Error>;
    fn market_at(
        &mut self,
        markets: &Self::Value,
        index: usize,
    ) -> Result<Self::Market, Self::Error>;
    fn market_id(&mut self, market: &Self::Market) -> Result<JsString, Self::Error>;
    fn subscriber(&mut self, kind: StreamKind) -> Result<Self::Subscriber, Self::Error>;
    fn subscribe(&mut self, subscriber: Self::Subscriber, id: JsString) -> Result<(), Self::Error>;
}
fn update_loaded<H: RuntimeHost>(
    h: &mut H,
    kind: Option<ReadKind>,
    value: Option<H::Value>,
    keys: &[&str],
) -> Result<(), H::Error> {
    let target = h.update_target()?;
    let mut loaded = h.loaded()?;
    for key in keys {
        let key = units(key);
        if let Some(entry) = loaded.iter_mut().find(|(name, _)| *name == key) {
            entry.1 = true;
        } else {
            loaded.push((key, true));
        }
    }
    h.update(target, kind, value, loaded)
}
fn log_plain<H: Logger>(h: &mut H, message: &str) -> Result<(), H::Error> {
    let target = h.log_target()?;
    h.log(target, units(message), None)
}
fn fetch<H: RuntimeHost>(
    host: &Rc<RefCell<H>>,
    kind: ReadKind,
) -> Result<H::FetchFuture, H::Error> {
    host.borrow_mut().fetch(kind)
}
// ts: BackpackExchange.start
pub fn start<H: RuntimeHost>(host: Rc<RefCell<H>>) -> impl Future<Output = Result<(), H::Error>> {
    let markets = host.borrow_mut().fetch(ReadKind::Markets);
    async move {
        let markets = markets?.await?;
        {
            let mut h = host.borrow_mut();
            if h.disposed()? {
                return Ok(());
            }
            update_loaded(
                &mut *h,
                Some(ReadKind::Markets),
                Some(markets.clone()),
                &["markets"],
            )?;
        }
        let tickers = fetch(&host, ReadKind::Tickers)?.await?;
        {
            let mut h = host.borrow_mut();
            if h.disposed()? {
                return Ok(());
            }
            let target = h.log_target()?;
            let count = h.length(&tickers)?.min(h.length(&markets)?);
            h.log(
                target,
                units(&format!("Loaded {count} Backpack markets")),
                None,
            )?;
            update_loaded(
                &mut *h,
                Some(ReadKind::Tickers),
                Some(tickers),
                &["tickers"],
            )?;
            h.connect_public()?;
            h.subscribe_all(markets)?;
            if h.public_only()? {
                update_loaded(&mut *h, None, None, &["balance", "positions", "orders"])?;
                return Ok(());
            }
        }
        fetch(&host, ReadKind::Tick)?.await?;
        if host.borrow_mut().disposed()? {
            return Ok(());
        }
        let orders = fetch(&host, ReadKind::Orders)?.await?;
        let mut h = host.borrow_mut();
        if h.disposed()? {
            return Ok(());
        }
        log_plain(&mut *h, "Loaded Backpack orders")?;
        update_loaded(&mut *h, Some(ReadKind::Orders), Some(orders), &["orders"])?;
        log_plain(&mut *h, "Ready to trade on Backpack")?;
        h.connect_private()
    }
}
// ts: BackpackExchange#subscribeToAllMarkets
pub fn subscribe_to_all_markets<H: RuntimeHost + 'static>(
    markets: H::Value,
    host: Rc<RefCell<H>>,
) -> Result<(), H::Error> {
    let captured = host.clone();
    host.borrow_mut().timeout(
        Box::new(move || subscribe_markets_now(markets, captured)),
        2000.0,
    )
}
// The delayed callback body is shared with the native weak-owner timer binding.
pub(crate) fn subscribe_markets_now<H: RuntimeHost + 'static>(
    markets: H::Value,
    captured: Rc<RefCell<H>>,
) -> Result<(), H::Error> {
    if captured.borrow_mut().disposed()? {
        return Ok(());
    }
    let mut index = 0;
    while index < captured.borrow_mut().length(&markets)? {
        let market = captured.borrow_mut().market_at(&markets, index)?;
        let result = (|| {
            for kind in [
                StreamKind::Ticker,
                StreamKind::Trades,
                StreamKind::BookTicker,
            ] {
                let mut h = captured.borrow_mut();
                let target = h.subscriber(kind)?;
                let id = h.market_id(&market)?;
                h.subscribe(target, id)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            let mut h = captured.borrow_mut();
            let logger = h.log_target()?;
            let id = h.market_id(&market)?;
            let error = h.error_text(&error)?;
            h.log(
                logger,
                join(&[&units("Failed to subscribe to "), &id, &units(": "), &error]),
                Some(Severity::Warning),
            )?;
        }
        index += 1;
    }
    Ok(())
}
// ts: BackpackExchange.tick
pub fn tick<H: RuntimeHost + 'static>(
    host: Rc<RefCell<H>>,
) -> impl Future<Output = Result<(), H::Error>> {
    let begin = (|| {
        let mut h = host.borrow_mut();
        if h.disposed()? || h.public_only()? {
            Ok(None)
        } else {
            Ok(Some(h.fetch(ReadKind::Account)))
        }
    })();
    let begin = match begin {
        Err(error) => Err(error),
        Ok(None) => Ok(None),
        Ok(Some(Ok(future))) => Ok(Some(future)),
        Ok(Some(Err(error))) => {
            let logged = {
                let mut h = host.borrow_mut();
                log_tick_error(&mut *h, &error)
            };
            let result = logged.and_then(|()| finish_tick(host.clone()));
            match result {
                Ok(()) => Ok(None),
                Err(error) => Err(error),
            }
        }
    };
    async move {
        let Some(account) = begin? else {
            return Ok(());
        };
        {
            let result = async {
                account.await?;
                if host.borrow_mut().disposed()? {
                    return Ok(false);
                }
                let positions = fetch(&host, ReadKind::Positions)?.await?;
                {
                    let mut h = host.borrow_mut();
                    if h.disposed()? {
                        return Ok(false);
                    }
                    update_loaded(
                        &mut *h,
                        Some(ReadKind::Positions),
                        Some(positions.clone()),
                        &["positions"],
                    )?;
                }
                let balance = fetch(&host, ReadKind::Balance)?.await?;
                {
                    let mut h = host.borrow_mut();
                    if h.disposed()? {
                        return Ok(false);
                    }
                    let upnl = if h.length(&positions)? > 0 {
                        h.positions_upnl(&positions)?
                    } else {
                        0.0
                    };
                    h.set_balance_upnl(&balance, upnl)?;
                    update_loaded(
                        &mut *h,
                        Some(ReadKind::Balance),
                        Some(balance),
                        &["balance"],
                    )?;
                }
                let orders = fetch(&host, ReadKind::Orders)?.await?;
                let mut h = host.borrow_mut();
                if h.disposed()? {
                    return Ok(false);
                }
                update_loaded(&mut *h, Some(ReadKind::Orders), Some(orders), &["orders"])?;
                Ok(true)
            }
            .await;
            match result {
                Ok(false) => return Ok(()),
                Ok(true) => (),
                Err(error) => log_tick_error(&mut *host.borrow_mut(), &error)?,
            }
        }
        finish_tick(host)
    }
}
fn finish_tick<H: RuntimeHost + 'static>(host: Rc<RefCell<H>>) -> Result<(), H::Error> {
    if !host.borrow_mut().disposed()? {
        let captured = host.clone();
        host.borrow_mut().schedule_loop(
            Box::new(move || captured.borrow_mut().repeat_tick()),
            10000.0,
        )?;
    }
    Ok(())
}

fn log_tick_error<H: RuntimeHost>(h: &mut H, error: &H::Error) -> Result<(), H::Error> {
    let target = h.log_target()?;
    let message = h.error_message(error)?;
    h.log(
        target,
        join(&[&units("Error in tick: "), &message]),
        Some(Severity::Error),
    )
}

pub trait RecoveryHost<U, H>: Logger {
    type Fetcher;
    type TerminalEmitter;
    fn state(&mut self, id: &[u16]) -> Result<Option<StateRef<U, H>>, Self::Error>;
    fn fetcher(&mut self, id: &[u16]) -> Result<Option<Self::Fetcher>, Self::Error>;
    fn disposed(&mut self) -> Result<bool, Self::Error>;
    fn terminate(
        &mut self,
        id: JsString,
        state: StateRef<U, H>,
        reason: JsString,
    ) -> Result<(), Self::Error>;
    fn microtask(&mut self, callback: Callback<Self::Error>) -> Result<(), Self::Error>;
    fn fetch_snapshot(&mut self, fetcher: Self::Fetcher) -> Result<(), Self::Error>;
    fn terminal_emitter(&mut self) -> Result<Self::TerminalEmitter, Self::Error>;
    fn emit_terminal(
        &mut self,
        target: Self::TerminalEmitter,
        message: JsString,
    ) -> Result<(), Self::Error>;
    fn terminal_handler(&mut self, handler: Rc<H>, reason: &[u16]) -> Result<(), Self::Error>;
}
// ts: BackpackPublicWebSocket#recoverOrderBook
pub fn recover_order_book<U: 'static, H: 'static, R: RecoveryHost<U, H> + 'static>(
    market_id: JsString,
    reason: JsString,
    host: Rc<RefCell<R>>,
) -> Result<(), R::Error> {
    let state = host.borrow_mut().state(&market_id)?;
    let fetcher = host.borrow_mut().fetcher(&market_id)?;
    let (Some(state), Some(fetcher)) = (state, fetcher) else {
        return Ok(());
    };
    {
        let s = state.borrow();
        if s.is_terminal || s.is_snapshot_fetch_in_flight || s.is_recovery_scheduled {
            return Ok(());
        }
    }
    if state.borrow().recovery_attempts >= 3.0 {
        return host.borrow_mut().terminate(market_id,state,units("recovery exhausted after 3 consecutive resnapshots; orderbook disabled until reconnect"));
    }
    {
        let mut s = state.borrow_mut();
        s.recovery_attempts += 1.0;
        s.is_snapshot_loaded = false;
        s.last_update_id = 0.0;
        s.is_recovery_scheduled = true;
    }
    {
        let mut h = host.borrow_mut();
        let target = h.log_target()?;
        let attempts = units(&js_number_string(state.borrow().recovery_attempts));
        h.log(
            target,
            join(&[
                &units("Backpack orderbook sequence gap for "),
                &market_id,
                &units(" ("),
                &reason,
                &units("); resnapshot "),
                &attempts,
                &units("/3"),
            ]),
            Some(Severity::Warning),
        )?;
    }
    let captured = host.clone();
    host.borrow_mut().microtask(Box::new(move || {
        let mut h = captured.borrow_mut();
        if h.disposed()? {
            return Ok(());
        }
        let current = h.state(&market_id)?;
        if !current
            .as_ref()
            .is_some_and(|current| Rc::ptr_eq(current, &state))
        {
            return Ok(());
        }
        state.borrow_mut().is_recovery_scheduled = false;
        h.fetch_snapshot(fetcher)
    }))
}
// ts: BackpackPublicWebSocket#terminateOrderBook
pub fn terminate_order_book<U, H, R: RecoveryHost<U, H>>(
    market_id: &[u16],
    state: &StateRef<U, H>,
    reason: &[u16],
    host: &mut R,
) -> Result<(), R::Error> {
    {
        let mut s = state.borrow_mut();
        if s.is_terminal {
            return Ok(());
        }
        s.is_terminal = true;
        s.is_snapshot_loaded = false;
        s.updates = Vec::new();
    }
    if !state.borrow().terminal_error_emitted {
        state.borrow_mut().terminal_error_emitted = true;
        let target = host.terminal_emitter()?;
        host.emit_terminal(
            target,
            join(&[
                &units("Backpack orderbook disabled for "),
                market_id,
                &units(": "),
                reason,
            ]),
        )?;
    }
    let handlers = state.borrow().terminal_handlers.clone();
    let mut index = 0;
    loop {
        let length = handlers.borrow().slots().len();
        if index >= length {
            break;
        }
        let handler = handlers.borrow().slots()[index].clone();
        if let Some(handler) = handler {
            if let Err(error) = host.terminal_handler(handler, reason) {
                let target = host.log_target()?;
                let error = host.error_text(&error)?;
                host.log(
                    target,
                    join(&[
                        &units("Error clearing Backpack orderbook for "),
                        market_id,
                        &units(": "),
                        &error,
                    ]),
                    Some(Severity::Warning),
                )?;
            }
        }
        index += 1;
    }
    Ok(())
}
