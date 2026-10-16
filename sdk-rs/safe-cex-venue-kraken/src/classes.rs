//! Externally assembled class factories with explicit complete member bindings.
use crate::{
    construction::{self, ConstructorHost, Venue},
    rest_account::{Handle, ReadHost, ReadResult},
    trading::units,
};
use std::{future::Future, pin::Pin, rc::Rc};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    FuturesExchange,
    SpotExchange,
    FuturesPublic,
    SpotPublic,
    FuturesPrivate,
    SpotPrivate,
}
pub type BodyFuture<H> = Pin<Box<dyn Future<Output = ReadResult<H, Handle<H>>>>>;
pub enum Return<H: ReadHost> {
    Value(Handle<H>),
    Future(BodyFuture<H>),
}
pub type NativeMember<H> = Rc<dyn Fn(Handle<H>, Vec<Handle<H>>) -> ReadResult<H, Return<H>>>;
pub type BoundMember<H> = Rc<dyn Fn(Vec<Handle<H>>) -> ReadResult<H, Return<H>>>;
#[derive(Clone, Copy)]
pub enum Initializer {
    Undefined,
    Boolean(bool),
    Map,
    Object,
    Imported(&'static str),
    Arrow(&'static str),
    Prototype(&'static str),
}
pub struct Definition {
    pub name: &'static str,
    pub initializer: Initializer,
}
pub trait ClassHost: ReadHost + Sized + 'static {
    fn super_socket(&self, kind: Kind, parent: Handle<Self>) -> ReadResult<Self, Handle<Self>>;
    fn define(
        &self,
        receiver: &Self::Value,
        name: &[u16],
        value: Handle<Self>,
    ) -> ReadResult<Self, ()>;
    fn undefined_value(&self) -> Handle<Self>;
    fn fresh_map(&self) -> ReadResult<Self, Handle<Self>>;
    fn imported(&self, name: &'static str) -> ReadResult<Self, Handle<Self>>;
    fn member(
        &self,
        kind: Kind,
        name: &'static str,
        target: &'static str,
    ) -> ReadResult<Self, NativeMember<Self>>;
    fn arrow(&self, body: BoundMember<Self>) -> ReadResult<Self, Handle<Self>>;
    fn prototype(
        &self,
        kind: Kind,
        name: &'static str,
        body: NativeMember<Self>,
    ) -> ReadResult<Self, ()>;
}
pub fn initialize<H: ClassHost>(h: &H, kind: Kind, receiver: Handle<H>) -> ReadResult<H, ()> {
    for definition in definitions(kind) {
        let value = match definition.initializer {
            Initializer::Undefined => h.undefined_value(),
            Initializer::Boolean(value) => h.boolean(value),
            Initializer::Map => h.fresh_map()?,
            Initializer::Object => h.object(vec![])?,
            Initializer::Imported(name) => h.imported(name)?,
            Initializer::Arrow(target) => {
                let body = h.member(kind, definition.name, target)?;
                let captured = receiver.clone();
                h.arrow(Rc::new(move |args| body(captured.clone(), args)))?
            }
            Initializer::Prototype(_) => continue,
        };
        h.define(&receiver, &units(definition.name), value)?;
    }
    Ok(())
}
fn prepare<H: ClassHost>(h: &H, kind: Kind) -> ReadResult<H, ()> {
    for definition in definitions(kind) {
        if let Initializer::Prototype(target) = definition.initializer {
            let body = h.member(kind, definition.name, target)?;
            h.prototype(kind, definition.name, body)?;
        }
    }
    Ok(())
}
fn socket<H: ClassHost>(h: &H, kind: Kind, parent: Handle<H>) -> ReadResult<H, Handle<H>> {
    prepare(h, kind)?;
    let receiver = h.super_socket(kind, parent)?;
    initialize(h, kind, receiver.clone())?;
    Ok(receiver)
}
pub fn futures_public<H: ClassHost>(h: &H, parent: Handle<H>) -> ReadResult<H, Handle<H>> {
    socket(h, Kind::FuturesPublic, parent)
}
pub fn spot_public<H: ClassHost>(h: &H, parent: Handle<H>) -> ReadResult<H, Handle<H>> {
    socket(h, Kind::SpotPublic, parent)
}
pub fn futures_private<H: ClassHost>(h: &H, parent: Handle<H>) -> ReadResult<H, Handle<H>> {
    socket(h, Kind::FuturesPrivate, parent)
}
pub fn spot_private<H: ClassHost>(h: &H, parent: Handle<H>) -> ReadResult<H, Handle<H>> {
    socket(h, Kind::SpotPrivate, parent)
}
pub fn futures_exchange<H: ClassHost + ConstructorHost>(
    h: &H,
    name: Handle<H>,
    options: Handle<H>,
    store: Handle<H>,
) -> ReadResult<H, Handle<H>> {
    prepare(h, Kind::FuturesExchange)?;
    construction::construct_with(h, Venue::Futures, name, options, store, |receiver| {
        initialize(h, Kind::FuturesExchange, receiver)
    })
}
pub fn spot_exchange<H: ClassHost + ConstructorHost>(
    h: &H,
    name: Handle<H>,
    options: Handle<H>,
    store: Handle<H>,
) -> ReadResult<H, Handle<H>> {
    prepare(h, Kind::SpotExchange)?;
    construction::construct_with(h, Venue::Spot, name, options, store, |receiver| {
        initialize(h, Kind::SpotExchange, receiver)
    })
}

pub fn definitions(kind: Kind) -> &'static [Definition] {
    match kind {
        Kind::FuturesExchange => &[
            Definition {
                name: "fetchHistoricalPositions",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/reads.rs::fetch_historical_positions",
                ),
            },
            Definition {
                name: "xhr",
                initializer: Initializer::Undefined,
            },
            Definition {
                name: "unlimitedXHR",
                initializer: Initializer::Undefined,
            },
            Definition {
                name: "historyXhr",
                initializer: Initializer::Undefined,
            },
            Definition {
                name: "leverageSettings",
                initializer: Initializer::Map,
            },
            Definition {
                name: "marginTypePreferences",
                initializer: Initializer::Map,
            },
            Definition {
                name: "signWsAuth",
                initializer: Initializer::Arrow("safe-cex-crypto/src/lib.rs::sign_kraken_ws_auth"),
            },
            Definition {
                name: "dispose",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/lifecycle.rs::futures_dispose",
                ),
            },
            Definition {
                name: "start",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/startup.rs::start_futures",
                ),
            },
            Definition {
                name: "fetchMarkets",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/rest_account.rs::futures_markets",
                ),
            },
            Definition {
                name: "fetchTickers",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/tickers.rs::fetch_tickers",
                ),
            },
            Definition {
                name: "fetchOrders",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/orders.rs::fetch_orders",
                ),
            },
            Definition {
                name: "placeOrder",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/trading.rs::futures_place",
                ),
            },
            Definition {
                name: "updateOrder",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/trading.rs::futures_update",
                ),
            },
            Definition {
                name: "cancelOrders",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/trading.rs::futures_cancel",
                ),
            },
            Definition {
                name: "listenOrderBook",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/stream_adapters.rs::futures_listen_order_book",
                ),
            },
            Definition {
                name: "fetchPositions",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/rest_account.rs::futures_positions",
                ),
            },
            Definition {
                name: "_fetchAccountLogPage",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/reads.rs::fetch_account_log_page",
                ),
            },
            Definition {
                name: "fetchIncomeHistoryRange",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/private_accounts.rs::income_range",
                ),
            },
            Definition {
                name: "fetchIncomeHistory",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/private_accounts.rs::income_history",
                ),
            },
            Definition {
                name: "fetchHistoricalFills",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/rest_account.rs::futures_fills",
                ),
            },
            Definition {
                name: "fetchOHLCV",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/futures_candles.rs::fetch_ohlcv",
                ),
            },
            Definition {
                name: "ohlcvPollingTimeouts",
                initializer: Initializer::Map,
            },
            Definition {
                name: "ohlcvPollingActive",
                initializer: Initializer::Map,
            },
            Definition {
                name: "lastCandleCache",
                initializer: Initializer::Map,
            },
            Definition {
                name: "listenOHLCV",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/ohlcv_polling.rs::listen_ohlcv",
                ),
            },
            Definition {
                name: "fetchBalance",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/rest_account.rs::futures_balance",
                ),
            },
            Definition {
                name: "fetchLeverageSettings",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/reads.rs::fetch_leverage_settings",
                ),
            },
            Definition {
                name: "setLeverage",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/trading.rs::futures_set_leverage",
                ),
            },
            Definition {
                name: "changeMarginType",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/trading.rs::futures_margin",
                ),
            },
            Definition {
                name: "validateAccount",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/validation.rs::validate_futures_account",
                ),
            },
            Definition {
                name: "tick",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/periodic_sync.rs::futures_tick",
                ),
            },
        ],
        Kind::SpotExchange => &[
            Definition {
                name: "xhr",
                initializer: Initializer::Undefined,
            },
            Definition {
                name: "unlimitedXHR",
                initializer: Initializer::Undefined,
            },
            Definition {
                name: "isSpot",
                initializer: Initializer::Boolean(true),
            },
            Definition {
                name: "grafanaEndpointsEvery",
                initializer: Initializer::Imported("GRAFANA_ENDPOINTS_EVERY"),
            },
            Definition {
                name: "grafanaEndpointsPeriodic",
                initializer: Initializer::Imported("GRAFANA_ENDPOINTS_PERIODIC"),
            },
            Definition {
                name: "start",
                initializer: Initializer::Arrow("safe-cex-venue-kraken/src/startup.rs::start_spot"),
            },
            Definition {
                name: "fetchMarkets",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/rest_account.rs::spot_markets",
                ),
            },
            Definition {
                name: "fetchTickers",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/rest_account.rs::spot_tickers",
                ),
            },
            Definition {
                name: "fetchOrders",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/rest_account.rs::spot_orders",
                ),
            },
            Definition {
                name: "fetchAndSetOrders",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/startup.rs::fetch_and_set_orders",
                ),
            },
            Definition {
                name: "placeOrder",
                initializer: Initializer::Arrow("safe-cex-venue-kraken/src/trading.rs::spot_place"),
            },
            Definition {
                name: "updateOrder",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/trading.rs::spot_update",
                ),
            },
            Definition {
                name: "cancelOrders",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/spot_cancel.rs::cancel_orders",
                ),
            },
            Definition {
                name: "listenOrderBook",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/stream_adapters.rs::spot_listen_order_book",
                ),
            },
            Definition {
                name: "fetchOHLCV",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/spot_reads.rs::fetch_ohlcv",
                ),
            },
            Definition {
                name: "listenOHLCV",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/stream_adapters.rs::spot_listen_ohlcv",
                ),
            },
            Definition {
                name: "fetchBalance",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/spot_balance.rs::fetch_balance",
                ),
            },
            Definition {
                name: "refreshBalance",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/rest_account.rs::spot_refresh_balance",
                ),
            },
            Definition {
                name: "fetchPositions",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/reads.rs::fetch_spot_positions",
                ),
            },
            Definition {
                name: "fetchWsToken",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/reads.rs::fetch_ws_token",
                ),
            },
            Definition {
                name: "validateAccount",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/validation.rs::validate_spot_account",
                ),
            },
            Definition {
                name: "tick",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/periodic_sync.rs::spot_tick",
                ),
            },
        ],
        Kind::FuturesPublic => &[
            Definition {
                name: "ohlcvHandlers",
                initializer: Initializer::Object,
            },
            Definition {
                name: "orderBookHandlers",
                initializer: Initializer::Object,
            },
            Definition {
                name: "orderBookCache",
                initializer: Initializer::Object,
            },
            Definition {
                name: "getTickerProductIds",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/ws_helpers.rs::get_ticker_product_ids",
                ),
            },
            Definition {
                name: "connectAndSubscribe",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/lifecycle.rs::futures_public_connect",
                ),
            },
            Definition {
                name: "onOpen",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/public_subscriptions.rs::futures_on_open",
                ),
            },
            Definition {
                name: "onMessage",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/message_dispatch.rs::futures_public",
                ),
            },
            Definition {
                name: "handleTicker",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/stream_updates.rs::futures_handle_ticker",
                ),
            },
            Definition {
                name: "listenOHLCV",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/stream_adapters.rs::futures_listen_ohlcv",
                ),
            },
            Definition {
                name: "handleOHLC",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/stream_adapters.rs::futures_handle_ohlc",
                ),
            },
            Definition {
                name: "clearFeedStatsIfIdle",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/feed_stats.rs::futures_clear_feed_stats_if_idle",
                ),
            },
            Definition {
                name: "listenOrderBook",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/book_listeners.rs::futures_listen_book",
                ),
            },
            Definition {
                name: "handleBookSnapshot",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/orderbooks.rs::futures_snapshot",
                ),
            },
            Definition {
                name: "handleBookDelta",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/orderbooks.rs::futures_delta",
                ),
            },
            Definition {
                name: "emitOrderBook",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/orderbooks.rs::futures_emit",
                ),
            },
            Definition {
                name: "mapTimeframe",
                initializer: Initializer::Prototype(
                    "safe-cex-venue-kraken/src/ws_helpers.rs::map_timeframe",
                ),
            },
        ],
        Kind::SpotPublic => &[
            Definition {
                name: "ohlcvHandlers",
                initializer: Initializer::Object,
            },
            Definition {
                name: "orderBookHandlers",
                initializer: Initializer::Object,
            },
            Definition {
                name: "orderBookCache",
                initializer: Initializer::Object,
            },
            Definition {
                name: "connectAndSubscribe",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/lifecycle.rs::spot_public_connect",
                ),
            },
            Definition {
                name: "onOpen",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/public_subscriptions.rs::spot_on_open",
                ),
            },
            Definition {
                name: "onMessage",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/message_dispatch.rs::spot_public",
                ),
            },
            Definition {
                name: "handleTicker",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/stream_updates.rs::spot_handle_ticker",
                ),
            },
            Definition {
                name: "handleBook",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/spot_dispatch.rs::handle_book",
                ),
            },
            Definition {
                name: "handleBookSnapshot",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/orderbooks.rs::spot_snapshot",
                ),
            },
            Definition {
                name: "handleBookDelta",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/orderbooks.rs::spot_delta",
                ),
            },
            Definition {
                name: "emitOrderBook",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/orderbooks.rs::spot_emit",
                ),
            },
            Definition {
                name: "listenOHLCV",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/book_listeners.rs::spot_listen_ohlcv",
                ),
            },
            Definition {
                name: "handleOHLC",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/stream_updates.rs::spot_handle_ohlc",
                ),
            },
            Definition {
                name: "clearFeedStatsIfIdle",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/feed_stats.rs::spot_clear_feed_stats_if_idle",
                ),
            },
            Definition {
                name: "listenOrderBook",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/book_listeners.rs::spot_listen_book",
                ),
            },
        ],
        Kind::FuturesPrivate => &[
            Definition {
                name: "authenticated",
                initializer: Initializer::Boolean(false),
            },
            Definition {
                name: "waitingChallenge",
                initializer: Initializer::Boolean(false),
            },
            Definition {
                name: "pendingChallenge",
                initializer: Initializer::Undefined,
            },
            Definition {
                name: "originalChallenge",
                initializer: Initializer::Undefined,
            },
            Definition {
                name: "signedChallenge",
                initializer: Initializer::Undefined,
            },
            Definition {
                name: "ordersThrottleTimeout",
                initializer: Initializer::Undefined,
            },
            Definition {
                name: "connectAndSubscribe",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/lifecycle.rs::futures_private_connect",
                ),
            },
            Definition {
                name: "onOpen",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/private_auth.rs::futures_on_open",
                ),
            },
            Definition {
                name: "authenticate",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/private_auth.rs::authenticate",
                ),
            },
            Definition {
                name: "onMessage",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/message_dispatch.rs::futures_private",
                ),
            },
            Definition {
                name: "performAuth",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/private_control.rs::perform_auth",
                ),
            },
            Definition {
                name: "subscribePrivateFeeds",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/private_control.rs::subscribe_private_feeds",
                ),
            },
            Definition {
                name: "handleFills",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/private_accounts.rs::futures_fills",
                ),
            },
            Definition {
                name: "handleOpenOrders",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/private_accounts.rs::futures_open_orders",
                ),
            },
            Definition {
                name: "handleOpenPositions",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/private_accounts.rs::futures_open_positions",
                ),
            },
            Definition {
                name: "handleBalances",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/private_accounts.rs::futures_balances",
                ),
            },
            Definition {
                name: "throttledFetchOrders",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/private_control.rs::futures_throttled_fetch_orders",
                ),
            },
            Definition {
                name: "fetchThrottledOrders",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/private_control.rs::futures_fetch_throttled_orders",
                ),
            },
        ],
        Kind::SpotPrivate => &[
            Definition {
                name: "authenticated",
                initializer: Initializer::Boolean(false),
            },
            Definition {
                name: "wsToken",
                initializer: Initializer::Undefined,
            },
            Definition {
                name: "ordersThrottleTimeout",
                initializer: Initializer::Undefined,
            },
            Definition {
                name: "connectAndSubscribe",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/lifecycle.rs::spot_private_connect",
                ),
            },
            Definition {
                name: "setToken",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/private_auth.rs::set_token",
                ),
            },
            Definition {
                name: "onOpen",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/private_auth.rs::spot_on_open",
                ),
            },
            Definition {
                name: "onMessage",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/message_dispatch.rs::spot_private",
                ),
            },
            Definition {
                name: "handleExecutions",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/private_accounts.rs::spot_executions",
                ),
            },
            Definition {
                name: "parseOrder",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/private_accounts.rs::spot_parse_order",
                ),
            },
            Definition {
                name: "handleBalances",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/spot_dispatch.rs::handle_balances",
                ),
            },
            Definition {
                name: "throttledFetchOrders",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-kraken/src/private_control.rs::spot_throttled_fetch_orders",
                ),
            },
        ],
    }
}
