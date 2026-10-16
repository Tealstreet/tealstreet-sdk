//! Executable class assembly requiring complete external native member adapters.
use crate::{lifecycle, public_feed};
use std::{future::Future, pin::Pin, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassKind {
    Exchange,
    Public,
    Private,
}
pub enum ClassReturn<V, E> {
    Value(V),
    Future(Pin<Box<dyn Future<Output = Result<V, E>>>>),
}
pub type NativeMember<V, E> = Rc<dyn Fn(V, Vec<V>) -> Result<ClassReturn<V, E>, E>>;
pub type BoundMember<V, E> = Rc<dyn Fn(Vec<V>) -> Result<ClassReturn<V, E>, E>>;
pub struct MemberBinding<V, E> {
    pub target: &'static str,
    pub body: NativeMember<V, E>,
}
pub struct Member {
    pub name: &'static str,
    pub target: &'static str,
    pub prototype: bool,
}
/// Callbacks share the opaque superclass receiver; target labels do not prove body linkage.
/// The host supplies imported primitives and receiver/closure ownership, including cycle handling.
pub trait ClassHost: Sized + 'static {
    type Value: Clone + 'static;
    type Error: 'static;
    fn exchange_runtime(
        &self,
    ) -> lifecycle::WeexExchangeConstructionRuntime<Self, Self::Value, Self::Error>;
    fn public_runtime(
        &self,
    ) -> public_feed::WeexPublicConstructionRuntime<Self, Self::Value, Self::Error>;
    fn super_private(&self, parent: Self::Value) -> Result<Self::Value, Self::Error>;
    fn define(
        &self,
        receiver: &Self::Value,
        name: &'static str,
        value: Self::Value,
    ) -> Result<(), Self::Error>;
    fn undefined(&self) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn number(&self, value: f64) -> Self::Value;
    fn member(
        &self,
        kind: ClassKind,
        name: &'static str,
    ) -> Result<Option<MemberBinding<Self::Value, Self::Error>>, Self::Error>;
    fn binding_error(
        &self,
        kind: ClassKind,
        name: &'static str,
        reason: &'static str,
    ) -> Self::Error;
    fn arrow(
        &self,
        name: &'static str,
        body: BoundMember<Self::Value, Self::Error>,
    ) -> Result<Self::Value, Self::Error>;
    /// Native hosts can retain a weak lexical receiver to avoid an Rc cycle.
    /// The default preserves the opaque-value host contract used by the oracle.
    fn bind_receiver(
        &self,
        receiver: Self::Value,
        body: NativeMember<Self::Value, Self::Error>,
    ) -> BoundMember<Self::Value, Self::Error> {
        Rc::new(move |args| body(receiver.clone(), args))
    }
    /// Install once per host class scope, retaining the existing callable for later instances.
    fn prototype(
        &self,
        kind: ClassKind,
        name: &'static str,
        body: NativeMember<Self::Value, Self::Error>,
    ) -> Result<(), Self::Error>;
}
type Prepared<H> = Vec<(
    &'static str,
    NativeMember<<H as ClassHost>::Value, <H as ClassHost>::Error>,
)>;
fn prepare<H: ClassHost>(h: &H, kind: ClassKind) -> Result<Prepared<H>, H::Error> {
    let mut result = Vec::new();
    for member in members(kind) {
        let binding = h
            .member(kind, member.name)?
            .ok_or_else(|| h.binding_error(kind, member.name, "missing native binding"))?;
        if binding.target != member.target {
            return Err(h.binding_error(kind, member.name, "native target mismatch"));
        }
        result.push((member.name, binding.body));
    }
    for member in members(kind).iter().filter(|m| m.prototype) {
        let body = result
            .iter()
            .find(|(name, _)| *name == member.name)
            .unwrap()
            .1
            .clone();
        h.prototype(kind, member.name, body)?;
    }
    Ok(result)
}
fn bind<H: ClassHost>(
    h: &Rc<H>,
    kind: ClassKind,
    prepared: &Prepared<H>,
    receiver: &H::Value,
    name: &'static str,
) -> Result<H::Value, H::Error> {
    let body = prepared
        .iter()
        .find(|(key, _)| *key == name)
        .ok_or_else(|| h.binding_error(kind, name, "constructor binding absent"))?
        .1
        .clone();
    let bound = h.bind_receiver(receiver.clone(), body);
    let owner = h.clone();
    h.arrow(
        name,
        Rc::new(move |args| {
            let _owner = &owner;
            bound(args)
        }),
    )
}
pub struct WeexExchange<H: ClassHost> {
    pub receiver: H::Value,
    pub host: Rc<H>,
}
impl<H: ClassHost> WeexExchange<H> {
    pub fn new(
        host: Rc<H>,
        name: H::Value,
        options: H::Value,
        store: H::Value,
    ) -> Result<Self, H::Error> {
        let prepared = prepare(host.as_ref(), ClassKind::Exchange)?;
        let receiver = lifecycle::construct_exchange_with_bind(
            host.as_ref(),
            &host.exchange_runtime(),
            name,
            options,
            store,
            |_, receiver, field| bind(&host, ClassKind::Exchange, &prepared, receiver, field),
        )?;
        Ok(Self { receiver, host })
    }
}
pub struct WeexPublicWebsocket<H: ClassHost> {
    pub receiver: H::Value,
    pub host: Rc<H>,
}
impl<H: ClassHost> WeexPublicWebsocket<H> {
    pub fn new(host: Rc<H>, parent: H::Value) -> Result<Self, H::Error> {
        let prepared = prepare(host.as_ref(), ClassKind::Public)?;
        let receiver = public_feed::construct_public_websocket_with_bind(
            host.as_ref(),
            &host.public_runtime(),
            parent,
            |_, receiver, field| bind(&host, ClassKind::Public, &prepared, receiver, field),
        )?;
        Ok(Self { receiver, host })
    }
}
pub struct WeexPrivateWebsocket<H: ClassHost> {
    pub receiver: H::Value,
    pub host: Rc<H>,
}
impl<H: ClassHost> WeexPrivateWebsocket<H> {
    pub fn new(host: Rc<H>, parent: H::Value) -> Result<Self, H::Error> {
        let prepared = prepare(host.as_ref(), ClassKind::Private)?;
        let receiver = host.super_private(parent)?;
        host.define(&receiver, "treatAnyMessageAsPong", host.boolean(true))?;
        host.define(&receiver, "subscriptionId", host.number(1.0))?;
        host.define(
            &receiver,
            "hasLoggedUnsupportedTransport",
            host.boolean(false),
        )?;
        for field in [
            "refreshOrdersTimeout",
            "refreshPositionsTimeout",
            "refreshBalanceTimeout",
        ] {
            host.define(&receiver, field, host.undefined())?;
        }
        for member in members(ClassKind::Private) {
            let value = bind(&host, ClassKind::Private, &prepared, &receiver, member.name)?;
            host.define(&receiver, member.name, value)?;
        }
        Ok(Self { receiver, host })
    }
}
pub fn members(kind: ClassKind) -> &'static [Member] {
    match kind {
        ClassKind::Exchange => &[
            Member {
                name: "validateAccount",
                target: "safe-cex-venue-weex/src/lifecycle.rs::validate_account",
                prototype: false,
            },
            Member {
                name: "getAccount",
                target: "safe-cex-venue-weex/src/lifecycle.rs::get_account",
                prototype: false,
            },
            Member {
                name: "start",
                target: "safe-cex-venue-weex/src/lifecycle.rs::start",
                prototype: false,
            },
            Member {
                name: "tick",
                target: "safe-cex-venue-weex/src/lifecycle.rs::tick",
                prototype: false,
            },
            Member {
                name: "fetchMarkets",
                target: "safe-cex-venue-weex/src/market_reads.rs::fetch_markets",
                prototype: false,
            },
            Member {
                name: "fetchTickers",
                target: "safe-cex-venue-weex/src/market_reads.rs::fetch_tickers",
                prototype: false,
            },
            Member {
                name: "fetchSymbolMaxLeverage",
                target: "safe-cex-venue-weex/src/account_reads.rs::fetch_symbol_max_leverage",
                prototype: false,
            },
            Member {
                name: "fetchLeverageForSymbol",
                target: "safe-cex-venue-weex/src/account_reads.rs::fetch_leverage_for_symbol",
                prototype: false,
            },
            Member {
                name: "updateSymbolMetadata",
                target: "safe-cex-venue-weex/src/account_actions.rs::update_symbol_metadata",
                prototype: false,
            },
            Member {
                name: "fetchBalance",
                target: "safe-cex-venue-weex/src/account_reads.rs::fetch_balance",
                prototype: false,
            },
            Member {
                name: "fetchBalanceAndPositions",
                target: "safe-cex-venue-weex/src/account_reads.rs::fetch_balance_and_positions",
                prototype: false,
            },
            Member {
                name: "fetchPositions",
                target: "safe-cex-venue-weex/src/trading.rs::fetch_positions",
                prototype: false,
            },
            Member {
                name: "fetchOrders",
                target: "safe-cex-venue-weex/src/account_reads.rs::fetch_orders",
                prototype: false,
            },
            Member {
                name: "fetchNormalOrders",
                target: "safe-cex-venue-weex/src/account_reads.rs::fetch_normal_orders",
                prototype: false,
            },
            Member {
                name: "fetchAlgoOrders",
                target: "safe-cex-venue-weex/src/account_reads.rs::fetch_algo_orders",
                prototype: false,
            },
            Member {
                name: "fetchOHLCV",
                target: "safe-cex-venue-weex/src/market_reads.rs::fetch_ohlcv",
                prototype: false,
            },
            Member {
                name: "listenOHLCV",
                target: "safe-cex-venue-weex/src/market_reads.rs::listen_ohlcv",
                prototype: false,
            },
            Member {
                name: "listenOrderBook",
                target: "safe-cex-venue-weex/src/market_reads.rs::listen_order_book",
                prototype: false,
            },
            Member {
                name: "fetchOrderBook",
                target: "safe-cex-venue-weex/src/market_reads.rs::fetch_order_book",
                prototype: false,
            },
            Member {
                name: "fetchPublicTrades",
                target: "safe-cex-venue-weex/src/market_reads.rs::fetch_public_trades",
                prototype: false,
            },
            Member {
                name: "fetchHistoricalFills",
                target: "safe-cex-venue-weex/src/market_reads.rs::fetch_historical_fills",
                prototype: false,
            },
            Member {
                name: "fetchIncomeHistory",
                target: "safe-cex-venue-weex/src/income_reads.rs::fetch_income_history",
                prototype: false,
            },
            Member {
                name: "fetchIncomeHistoryRange",
                target: "safe-cex-venue-weex/src/income_reads.rs::fetch_income_history_range",
                prototype: false,
            },
            Member {
                name: "placeOrder",
                target: "safe-cex-venue-weex/src/trading.rs::place_order",
                prototype: false,
            },
            Member {
                name: "placeOrders",
                target: "safe-cex-venue-weex/src/trading.rs::place_orders",
                prototype: false,
            },
            Member {
                name: "updateOrder",
                target: "safe-cex-venue-weex/src/trading.rs::update_order",
                prototype: false,
            },
            Member {
                name: "cancelOrders",
                target: "safe-cex-venue-weex/src/account_actions.rs::cancel_orders",
                prototype: false,
            },
            Member {
                name: "cancelSymbolOrders",
                target: "safe-cex-venue-weex/src/account_actions.rs::cancel_symbol_orders",
                prototype: false,
            },
            Member {
                name: "cancelAllOrders",
                target: "safe-cex-venue-weex/src/account_actions.rs::cancel_all_orders",
                prototype: false,
            },
            Member {
                name: "setLeverage",
                target: "safe-cex-venue-weex/src/account_actions.rs::set_leverage",
                prototype: false,
            },
            Member {
                name: "changeMarginType",
                target: "safe-cex-venue-weex/src/account_actions.rs::change_margin_type",
                prototype: false,
            },
            Member {
                name: "changePositionMode",
                target: "safe-cex-venue-weex/src/account_actions.rs::change_position_mode",
                prototype: false,
            },
            Member {
                name: "nukePositions",
                target: "safe-cex-venue-weex/src/account_actions.rs::nuke_positions",
                prototype: false,
            },
            Member {
                name: "nuke",
                target: "safe-cex-venue-weex/src/account_actions.rs::nuke",
                prototype: false,
            },
            Member {
                name: "getSymbolConfigFromCache",
                target: "safe-cex-venue-weex/src/symbol_settings.rs::get_symbol_config_from_cache",
                prototype: false,
            },
            Member {
                name: "getOrFetchSymbolConfig",
                target: "safe-cex-venue-weex/src/symbol_settings.rs::get_or_fetch_symbol_config",
                prototype: false,
            },
            Member {
                name: "fetchSymbolConfigs",
                target: "safe-cex-venue-weex/src/account_reads.rs::fetch_symbol_configs",
                prototype: false,
            },
            Member {
                name: "fetchOpenOrderRows",
                target: "safe-cex-venue-weex/src/account_reads.rs::fetch_open_order_rows",
                prototype: false,
            },
            Member {
                name: "fetchUserTradeRows",
                target: "safe-cex-venue-weex/src/market_reads.rs::fetch_user_trade_rows",
                prototype: false,
            },
            Member {
                name: "parseOrderResult",
                target: "safe-cex-venue-weex/src/order_result.rs::parse_order_result",
                prototype: false,
            },
            Member {
                name: "isBatchableRegularOrder",
                target: "safe-cex-venue-weex/src/order_routing.rs::is_batchable_regular_order",
                prototype: false,
            },
            Member {
                name: "placeRegularOrderBatch",
                target: "safe-cex-venue-weex/src/trading.rs::place_regular_order_batch",
                prototype: false,
            },
            Member {
                name: "isAlgoOrder",
                target: "safe-cex-venue-weex/src/order_routing.rs::is_algo_order",
                prototype: false,
            },
            Member {
                name: "inferPositionSide",
                target: "safe-cex-venue-weex/src/order_routing.rs::infer_position_side",
                prototype: false,
            },
            Member {
                name: "formatOrderPayload",
                target: "safe-cex-venue-weex/src/trading.rs::format_order_payload",
                prototype: false,
            },
        ],
        ClassKind::Public => &[
            Member {
                name: "connectAndSubscribe",
                target: "safe-cex-venue-weex/src/public_feed.rs::connect_and_subscribe",
                prototype: false,
            },
            Member {
                name: "onOpen",
                target: "safe-cex-venue-weex/src/ws_control.rs::public_on_open",
                prototype: false,
            },
            Member {
                name: "ping",
                target: "safe-cex-venue-weex/src/ws_control.rs::public_ping",
                prototype: false,
            },
            Member {
                name: "onMessage",
                target: "safe-cex-venue-weex/src/public_feed.rs::on_message",
                prototype: false,
            },
            Member {
                name: "listenOHLCV",
                target: "safe-cex-venue-weex/src/public_feed.rs::listen_ohlcv",
                prototype: false,
            },
            Member {
                name: "listenOrderBook",
                target: "safe-cex-venue-weex/src/public_feed.rs::listen_order_book",
                prototype: false,
            },
            Member {
                name: "dispose",
                target: "safe-cex-venue-weex/src/public_feed.rs::dispose",
                prototype: true,
            },
            Member {
                name: "subscribeTickerTopics",
                target: "safe-cex-venue-weex/src/subscriptions.rs::subscribe_ticker_topics",
                prototype: false,
            },
            Member {
                name: "handleActiveTickerSymbolsChanged",
                target: "safe-cex-venue-weex/src/subscriptions.rs::handle_active_ticker_symbols_changed",
                prototype: false,
            },
            Member {
                name: "getDesiredTickerTopics",
                target: "safe-cex-venue-weex/src/selection.rs::get_desired_ticker_topics",
                prototype: false,
            },
            Member {
                name: "reconcileTickerTopics",
                target: "safe-cex-venue-weex/src/subscriptions.rs::reconcile_ticker_topics",
                prototype: false,
            },
            Member {
                name: "resubscribeTopics",
                target: "safe-cex-venue-weex/src/subscriptions.rs::resubscribe_topics",
                prototype: false,
            },
            Member {
                name: "subscribeTopic",
                target: "safe-cex-venue-weex/src/subscriptions.rs::subscribe_topic",
                prototype: false,
            },
            Member {
                name: "unsubscribeTopic",
                target: "safe-cex-venue-weex/src/subscriptions.rs::unsubscribe_topic",
                prototype: false,
            },
            Member {
                name: "subscribeTopics",
                target: "safe-cex-venue-weex/src/subscriptions.rs::subscribe_topics",
                prototype: false,
            },
            Member {
                name: "sendSubscriptionOperation",
                target: "safe-cex-venue-weex/src/subscriptions.rs::send_subscription_operation",
                prototype: false,
            },
            Member {
                name: "addHandler",
                target: "safe-cex-venue-weex/src/message_handlers.rs::add_handler",
                prototype: false,
            },
            Member {
                name: "removeHandler",
                target: "safe-cex-venue-weex/src/message_handlers.rs::remove_handler",
                prototype: false,
            },
            Member {
                name: "handleRoutedMessage",
                target: "safe-cex-venue-weex/src/message_handlers.rs::handle_routed_message",
                prototype: false,
            },
            Member {
                name: "handleTicker",
                target: "safe-cex-venue-weex/src/public_feed.rs::handle_ticker",
                prototype: false,
            },
            Member {
                name: "handleDepth",
                target: "safe-cex-venue-weex/src/public_feed.rs::handle_depth",
                prototype: false,
            },
        ],
        ClassKind::Private => &[
            Member {
                name: "connectAndSubscribe",
                target: "safe-cex-venue-weex/src/private_feed.rs::connect_and_subscribe",
                prototype: false,
            },
            Member {
                name: "onOpen",
                target: "safe-cex-venue-weex/src/ws_control.rs::private_on_open",
                prototype: false,
            },
            Member {
                name: "ping",
                target: "safe-cex-venue-weex/src/ws_control.rs::private_ping",
                prototype: false,
            },
            Member {
                name: "onMessage",
                target: "safe-cex-venue-weex/src/private_feed.rs::on_message",
                prototype: false,
            },
            Member {
                name: "dispose",
                target: "safe-cex-venue-weex/src/private_feed.rs::dispose",
                prototype: false,
            },
            Member {
                name: "subscribe",
                target: "safe-cex-venue-weex/src/ws_control.rs::private_subscribe",
                prototype: false,
            },
            Member {
                name: "createAuthHeaders",
                target: "safe-cex-venue-weex/src/ws_control.rs::create_auth_headers",
                prototype: false,
            },
            Member {
                name: "logUnsupportedTransport",
                target: "safe-cex-venue-weex/src/ws_control.rs::log_unsupported_transport",
                prototype: false,
            },
            Member {
                name: "handleOrders",
                target: "safe-cex-venue-weex/src/private_feed.rs::handle_orders",
                prototype: false,
            },
            Member {
                name: "handlePositions",
                target: "safe-cex-venue-weex/src/private_feed.rs::handle_positions",
                prototype: false,
            },
            Member {
                name: "handleFills",
                target: "safe-cex-venue-weex/src/private_feed.rs::handle_fills",
                prototype: false,
            },
            Member {
                name: "throttledFetchOrders",
                target: "safe-cex-venue-weex/src/private_feed.rs::throttled_fetch_orders",
                prototype: false,
            },
            Member {
                name: "throttledFetchPositions",
                target: "safe-cex-venue-weex/src/private_feed.rs::throttled_fetch_positions",
                prototype: false,
            },
            Member {
                name: "throttledFetchBalance",
                target: "safe-cex-venue-weex/src/private_feed.rs::throttled_fetch_balance",
                prototype: false,
            },
        ],
    }
}
