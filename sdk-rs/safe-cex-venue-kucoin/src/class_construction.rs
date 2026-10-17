use crate::public_streams::StreamOps;
use std::rc::Rc;

pub struct ClassOps<V, E> {
    pub value: Rc<StreamOps<V, E>>,
    pub null: Rc<dyn Fn() -> V>,
    pub boolean: Rc<dyn Fn(bool) -> V>,
    pub regexp: Rc<dyn Fn(&str) -> Result<V, E>>,
    pub define: Rc<dyn Fn(&V, &str, V) -> Result<(), E>>,
    pub member: Rc<dyn Fn(&V, &str) -> Result<V, E>>,
    pub superclass: Rc<dyn Fn(&V) -> Result<V, E>>,
    pub super_construct: Rc<dyn Fn(V, V, Vec<V>) -> Result<V, E>>,
    pub construct: Rc<dyn Fn(V, Vec<V>) -> Result<V, E>>,
    pub function: Rc<dyn Fn(&str, usize, Rc<dyn Fn(Vec<V>) -> Result<V, E>>) -> V>,
    pub native_member: Rc<dyn Fn(&V, &'static str, &'static str, &'static str) -> Result<V, E>>,
    pub prototype_member: Rc<
        dyn Fn(&V, &'static str, &'static str, &'static str, usize, &'static str) -> Result<(), E>,
    >,
    pub derive:
        Rc<dyn Fn(&str, V, usize, Rc<dyn Fn(V, V, Vec<V>) -> Result<V, E>>) -> Result<V, E>>,
}
fn call<V, E>(
    h: &ClassOps<V, E>,
    path: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let callee = (h.value.capture)(path)?;
    (h.value.invoke)(callee, args()?)
}
fn assign_call<V, E>(
    h: &ClassOps<V, E>,
    receiver: &V,
    key: &str,
    path: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<(), E> {
    let value = call(h, path, args)?;
    (h.value.set)(receiver, key, value)
}
fn callback<V: Clone + 'static, E: 'static>(h: Rc<ClassOps<V, E>>, rps: f64) -> V {
    let ops = h.clone();
    (h.function)(
        "rateLimit",
        1,
        Rc::new(move |args| {
            call(&ops, "rateLimit", || {
                Ok(vec![
                    args.first()
                        .cloned()
                        .unwrap_or_else(|| (ops.value.undefined)()),
                    (ops.value.object)(vec![("maxRPS", (ops.value.number)(rps))]),
                ])
            })
        }),
    )
}
pub fn futures_constructor<V: Clone + 'static, E: 'static>(
    h: Rc<ClassOps<V, E>>,
    active: V,
    target: V,
    args: Vec<V>,
) -> Result<V, E> {
    let arg = |i| {
        args.get(i)
            .cloned()
            .unwrap_or_else(|| (h.value.undefined)())
    };
    let opts = arg(1);
    let superclass = (h.superclass)(&active)?;
    let receiver = (h.super_construct)(superclass, target, vec![arg(0), opts.clone(), arg(2)])?;
    initialize_futures_exchange(&h, &receiver)?;
    let callee = (h.member)(&receiver, "createXhrWithSecondaryKey")?;
    let xhr = (h.value.invoke)(
        callee,
        vec![
            opts.clone(),
            (h.value.capture)("createAPI")?,
            (h.value.object)(vec![("rateLimit", callback(h.clone(), 3.))]),
        ],
    )?;
    (h.value.set)(&receiver, "xhr", xhr)?;
    assign_call(&h, &receiver, "unlimitedXHR", "createAPI", || {
        Ok(vec![opts, receiver.clone()])
    })?;
    let ctor = (h.value.capture)("KucoinPublicWebsocket")?;
    let socket = (h.construct)(
        ctor,
        vec![
            receiver.clone(),
            (h.value.object)(vec![("role", (h.value.literal)("detail"))]),
        ],
    )?;
    (h.value.set)(&receiver, "publicWebsocket", socket)?;
    let ctor = (h.value.capture)("KucoinPrivateWebsocket")?;
    let socket = (h.construct)(ctor, vec![receiver.clone()])?;
    (h.value.set)(&receiver, "privateWebsocket", socket)?;
    Ok(receiver)
}
pub fn spot_constructor<V: Clone + 'static, E: 'static>(
    h: Rc<ClassOps<V, E>>,
    active: V,
    target: V,
    args: Vec<V>,
) -> Result<V, E> {
    let arg = |i| {
        args.get(i)
            .cloned()
            .unwrap_or_else(|| (h.value.undefined)())
    };
    let opts = arg(1);
    let superclass = (h.superclass)(&active)?;
    let receiver = (h.super_construct)(superclass, target, vec![arg(0), opts.clone(), arg(2)])?;
    initialize_spot_exchange(&h, &receiver)?;
    let callee = (h.member)(&receiver, "createXhrWithSecondaryKey")?;
    let xhr = (h.value.invoke)(
        callee,
        vec![
            opts.clone(),
            (h.value.capture)("createKucoinSpotAPI")?,
            (h.value.object)(vec![("rateLimit", callback(h.clone(), 5.))]),
        ],
    )?;
    (h.value.set)(&receiver, "xhr", xhr)?;
    assign_call(&h, &receiver, "unlimitedXHR", "createKucoinSpotAPI", || {
        Ok(vec![opts, receiver.clone()])
    })?;
    let ctor = (h.value.capture)("KucoinSpotPublicWebsocket")?;
    let socket = (h.construct)(ctor, vec![receiver.clone()])?;
    (h.value.set)(&receiver, "publicWebsocket", socket)?;
    let ctor = (h.value.capture)("KucoinSpotPrivateWebsocket")?;
    let socket = (h.construct)(ctor, vec![receiver.clone()])?;
    (h.value.set)(&receiver, "privateWebsocket", socket)?;
    Ok(receiver)
}
fn implicit_socket<V: Clone, E>(
    h: &ClassOps<V, E>,
    active: V,
    target: V,
    args: Vec<V>,
    initialize: impl FnOnce(&V) -> Result<(), E>,
) -> Result<V, E> {
    let superclass = (h.superclass)(&active)?;
    let receiver = (h.super_construct)(superclass, target, args)?;
    initialize(&receiver)?;
    Ok(receiver)
}

fn arrows<V, E>(
    h: &ClassOps<V, E>,
    receiver: &V,
    class: &'static str,
    members: &[(&'static str, &'static str)],
) -> Result<(), E> {
    for (key, root) in members {
        let member = (h.native_member)(receiver, class, key, root)?;
        (h.define)(receiver, key, member)?;
    }
    Ok(())
}
fn initialize_futures_exchange<V: Clone, E>(h: &ClassOps<V, E>, receiver: &V) -> Result<(), E> {
    (h.define)(
        receiver,
        "clientOrderIdRule",
        call(h, "patternClientOrderIdRule", || {
            Ok(vec![
                (h.value.literal)("KuCoin"),
                (h.regexp)("^[A-Za-z0-9_-]{1,40}$")?,
            ])
        })?,
    )?;
    (h.define)(receiver, "xhr", (h.value.undefined)())?;
    (h.define)(receiver, "unlimitedXHR", (h.value.undefined)())?;
    (h.define)(receiver, "publicWebsocket", (h.value.undefined)())?;
    (h.define)(receiver, "snapshotWebsockets", (h.value.array)(vec![]))?;
    (h.define)(receiver, "privateWebsocket", (h.value.undefined)())?;
    (h.define)(
        receiver,
        "grafanaEndpointsEvery",
        (h.value.capture)("GRAFANA_ENDPOINTS_EVERY")?,
    )?;
    (h.define)(
        receiver,
        "grafanaEndpointsPeriodic",
        (h.value.capture)("GRAFANA_ENDPOINTS_PERIODIC")?,
    )?;
    arrows(
        h,
        receiver,
        "KucoinExchange",
        &[
            (
                "toKucoinSymbol",
                "safe-cex-venue-kucoin/src/sizing.rs::to_kucoin_symbol",
            ),
            (
                "toInternalSymbol",
                "safe-cex-venue-kucoin/src/sizing.rs::to_internal_symbol",
            ),
            (
                "toApiSymbol",
                "safe-cex-venue-kucoin/src/sizing.rs::to_api_symbol",
            ),
            (
                "contractSizeFor",
                "safe-cex-venue-kucoin/src/sizing.rs::contract_size_for",
            ),
            (
                "nativeAmountForCancelReplace",
                "safe-cex-venue-kucoin/src/sizing.rs::native_amount_for_cancel_replace",
            ),
            (
                "parseMarginType",
                "safe-cex-venue-kucoin/src/order_helpers.rs::parse_margin_type",
            ),
            (
                "parsePositionMode",
                "safe-cex-venue-kucoin/src/order_helpers.rs::parse_position_mode",
            ),
            (
                "parseRowPositionMode",
                "safe-cex-venue-kucoin/src/order_helpers.rs::parse_row_position_mode",
            ),
            (
                "getAccount",
                "safe-cex-venue-kucoin/src/futures_native_accounts.rs::get_account",
            ),
            (
                "validateAccount",
                "safe-cex-venue-kucoin/src/futures_native_accounts.rs::validate_account",
            ),
            (
                "dispose",
                "safe-cex-venue-kucoin/src/socket_boundaries.rs::dispose_futures",
            ),
            (
                "start",
                "safe-cex-venue-kucoin/src/bootstrap_reads.rs::start",
            ),
            ("tick", "safe-cex-venue-kucoin/src/bootstrap_reads.rs::tick"),
            (
                "fetchBalance",
                "safe-cex-venue-kucoin/src/rest_reads.rs::fetch_balance",
            ),
            (
                "fetchPositionMetadata",
                "safe-cex-venue-kucoin/src/bootstrap_reads.rs::fetch_position_metadata",
            ),
            (
                "fetchPositions",
                "safe-cex-venue-kucoin/src/bootstrap_reads.rs::fetch_positions",
            ),
            (
                "fetchOpenPositions",
                "safe-cex-venue-kucoin/src/bootstrap_reads.rs::fetch_open_positions",
            ),
            (
                "fetchMarkets",
                "safe-cex-venue-kucoin/src/bootstrap_reads.rs::fetch_markets",
            ),
            (
                "fetchTickers",
                "safe-cex-venue-kucoin/src/bootstrap_reads.rs::fetch_tickers",
            ),
            (
                "fetchOrders",
                "safe-cex-venue-kucoin/src/futures_native_order_reads.rs::fetch_orders",
            ),
            (
                "reconcileFetchedOrderRemainders",
                "safe-cex-venue-kucoin/src/sizing.rs::reconcile_fetched_order_remainders",
            ),
            (
                "fetchNormalOrders",
                "safe-cex-venue-kucoin/src/futures_native_order_reads.rs::fetch_normal_orders",
            ),
            (
                "fetchAlgoOrders",
                "safe-cex-venue-kucoin/src/futures_native_order_reads.rs::fetch_algo_orders",
            ),
            (
                "fetchOHLCV",
                "safe-cex-venue-kucoin/src/bootstrap_reads.rs::fetch_ohlcv",
            ),
            (
                "listenOHLCV",
                "safe-cex-venue-kucoin/src/socket_boundaries.rs::listen_ohlcv",
            ),
            (
                "listenOrderBook",
                "safe-cex-venue-kucoin/src/socket_boundaries.rs::listen_order_book",
            ),
            (
                "cancelAllOrders",
                "safe-cex-venue-kucoin/src/order_control.rs::cancel_all_orders",
            ),
            (
                "cancelOrders",
                "safe-cex-venue-kucoin/src/order_control.rs::cancel_orders",
            ),
            (
                "cancelNormalOrders",
                "safe-cex-venue-kucoin/src/order_control.rs::cancel_normal_orders",
            ),
            (
                "cancelSymbolOrders",
                "safe-cex-venue-kucoin/src/order_control.rs::cancel_symbol_orders",
            ),
            (
                "placeOrder",
                "safe-cex-venue-kucoin/src/order_lifecycle.rs::place_order",
            ),
            (
                "formatCreateOrder",
                "safe-cex-venue-kucoin/src/formatters.rs::format_create_order",
            ),
            (
                "formatAlgoOrder",
                "safe-cex-venue-kucoin/src/formatters.rs::format_algo_order",
            ),
            (
                "getOrderPlanType",
                "safe-cex-venue-kucoin/src/order_helpers.rs::get_order_plan_type",
            ),
            (
                "placeOrderBatch",
                "safe-cex-venue-kucoin/src/order_lifecycle.rs::place_order_batch",
            ),
            (
                "placeAlgoOrders",
                "safe-cex-venue-kucoin/src/order_lifecycle.rs::place_algo_orders",
            ),
            (
                "updateVirtualTpSlOrder",
                "safe-cex-venue-kucoin/src/order_lifecycle.rs::update_virtual_tp_sl_order",
            ),
            (
                "updateOrder",
                "safe-cex-venue-kucoin/src/order_control.rs::update_order",
            ),
            (
                "updateAlgoOrder",
                "safe-cex-venue-kucoin/src/order_control.rs::update_algo_order",
            ),
            (
                "setLeverage",
                "safe-cex-venue-kucoin/src/order_control.rs::set_leverage",
            ),
            (
                "changeMarginType",
                "safe-cex-venue-kucoin/src/order_control.rs::change_margin_type",
            ),
            (
                "changePositionMode",
                "safe-cex-venue-kucoin/src/order_control.rs::change_position_mode",
            ),
            (
                "mapOrder",
                "safe-cex-venue-kucoin/src/mappers.rs::map_order",
            ),
            (
                "getOrderSideV2",
                "safe-cex-venue-kucoin/src/order_helpers.rs::get_order_side_v2",
            ),
            (
                "getOrderSide",
                "safe-cex-venue-kucoin/src/order_helpers.rs::get_order_side",
            ),
            (
                "getAlgoOrderSide",
                "safe-cex-venue-kucoin/src/order_helpers.rs::get_algo_order_side",
            ),
            (
                "getPositionForOrder",
                "safe-cex-venue-kucoin/src/order_helpers.rs::get_position_for_order",
            ),
            (
                "getKucoinMarginMode",
                "safe-cex-venue-kucoin/src/order_helpers.rs::get_kucoin_margin_mode",
            ),
            (
                "getKucoinPositionSide",
                "safe-cex-venue-kucoin/src/order_helpers.rs::get_kucoin_position_side",
            ),
            (
                "mapStopPriceType",
                "safe-cex-venue-kucoin/src/order_helpers.rs::map_stop_price_type",
            ),
            (
                "getKucoinStopDirection",
                "safe-cex-venue-kucoin/src/order_helpers.rs::get_kucoin_stop_direction",
            ),
            (
                "isVirtualTPSLOrder",
                "safe-cex-venue-kucoin/src/order_helpers.rs::is_virtual_tpsl_order",
            ),
            (
                "isAlgoOrder",
                "safe-cex-venue-kucoin/src/order_helpers.rs::is_algo_order",
            ),
            (
                "getOrderPlanTypeAndAmount",
                "safe-cex-venue-kucoin/src/sizing.rs::get_order_plan_type_and_amount",
            ),
        ],
    )?;
    Ok(())
}
fn initialize_spot_exchange<V: Clone, E>(h: &ClassOps<V, E>, receiver: &V) -> Result<(), E> {
    (h.define)(
        receiver,
        "clientOrderIdRule",
        call(h, "patternClientOrderIdRule", || {
            Ok(vec![
                (h.value.literal)("KuCoin Spot"),
                (h.regexp)("^[A-Za-z0-9_-]{1,40}$")?,
            ])
        })?,
    )?;
    (h.define)(receiver, "xhr", (h.value.undefined)())?;
    (h.define)(receiver, "unlimitedXHR", (h.value.undefined)())?;
    (h.define)(receiver, "isSpot", (h.boolean)(true))?;
    (h.define)(receiver, "publicWebsocket", (h.value.undefined)())?;
    (h.define)(receiver, "privateWebsocket", (h.value.undefined)())?;
    (h.define)(
        receiver,
        "grafanaEndpointsEvery",
        (h.value.capture)("KUCOIN_SPOT_GRAFANA_ENDPOINTS_EVERY")?,
    )?;
    (h.define)(
        receiver,
        "grafanaEndpointsPeriodic",
        (h.value.capture)("KUCOIN_SPOT_GRAFANA_ENDPOINTS_PERIODIC")?,
    )?;
    arrows(
        h,
        receiver,
        "KucoinSpotExchange",
        &[
            (
                "dispose",
                "safe-cex-venue-kucoin/src/socket_boundaries.rs::dispose_spot",
            ),
            (
                "getAccount",
                "safe-cex-venue-kucoin/src/spot_native_accounts.rs::get_account",
            ),
            (
                "validateAccount",
                "safe-cex-venue-kucoin/src/spot_native_accounts.rs::validate_account",
            ),
            ("start", "safe-cex-venue-kucoin/src/spot_bodies.rs::start"),
            (
                "fetchBalance",
                "safe-cex-venue-kucoin/src/spot_bodies.rs::fetch_balance",
            ),
            (
                "getUsdPrice",
                "safe-cex-venue-kucoin/src/sizing.rs::get_usd_price",
            ),
            (
                "fetchPositions",
                "safe-cex-venue-kucoin/src/spot_helpers.rs::fetch_positions",
            ),
            (
                "fetchSymbolMaxLeverage",
                "safe-cex-venue-kucoin/src/spot_helpers.rs::fetch_symbol_max_leverage",
            ),
            (
                "setLeverage",
                "safe-cex-venue-kucoin/src/spot_helpers.rs::set_leverage",
            ),
            (
                "changeMarginType",
                "safe-cex-venue-kucoin/src/spot_helpers.rs::change_margin_type",
            ),
            (
                "changePositionMode",
                "safe-cex-venue-kucoin/src/spot_helpers.rs::change_position_mode",
            ),
            ("tick", "safe-cex-venue-kucoin/src/spot_bodies.rs::tick"),
            (
                "fetchOrders",
                "safe-cex-venue-kucoin/src/spot_bodies.rs::fetch_orders",
            ),
            (
                "fetchActiveOrderSymbols",
                "safe-cex-venue-kucoin/src/spot_native_order_reads.rs::fetch_active_order_symbols",
            ),
            (
                "mapOrder",
                "safe-cex-venue-kucoin/src/mappers.rs::map_spot_order",
            ),
            (
                "placeOrder",
                "safe-cex-venue-kucoin/src/spot_bodies.rs::place_order",
            ),
            (
                "updateOrder",
                "safe-cex-venue-kucoin/src/spot_bodies.rs::update_order",
            ),
            (
                "cancelOrders",
                "safe-cex-venue-kucoin/src/spot_native_order_reads.rs::cancel_orders",
            ),
            (
                "cancelSymbolOrders",
                "safe-cex-venue-kucoin/src/spot_native_order_reads.rs::cancel_symbol_orders",
            ),
            (
                "cancelAllOrders",
                "safe-cex-venue-kucoin/src/spot_native_order_reads.rs::cancel_all_orders",
            ),
            (
                "fetchHistoricalFills",
                "safe-cex-venue-kucoin/src/spot_bodies.rs::fetch_historical_fills",
            ),
            (
                "fetchMarkets",
                "safe-cex-venue-kucoin/src/spot_native_order_reads.rs::fetch_markets",
            ),
            (
                "fetchTickers",
                "safe-cex-venue-kucoin/src/spot_native_order_reads.rs::fetch_tickers",
            ),
            (
                "fetchOHLCV",
                "safe-cex-venue-kucoin/src/spot_bodies.rs::fetch_ohlcv",
            ),
            (
                "listenOHLCV",
                "safe-cex-venue-kucoin/src/socket_boundaries.rs::listen_ohlcv_spot",
            ),
            (
                "listenOrderBook",
                "safe-cex-venue-kucoin/src/socket_boundaries.rs::listen_order_book_spot",
            ),
        ],
    )?;
    Ok(())
}
fn initialize_futures_private<V: Clone, E>(h: &ClassOps<V, E>, receiver: &V) -> Result<(), E> {
    (h.define)(receiver, "token", (h.null)())?;
    (h.define)(receiver, "socketEndpoint", (h.null)())?;
    (h.define)(receiver, "subscribed", (h.boolean)(false))?;
    (h.define)(receiver, "snapshotRefreshTimer", (h.value.undefined)())?;
    arrows(
        h,
        receiver,
        "KucoinPrivateWebsocket",
        &[
            (
                "connectAndSubscribe",
                "safe-cex-venue-kucoin/src/socket_setup.rs::connect_futures_private",
            ),
            (
                "onOpen",
                "safe-cex-venue-kucoin/src/private_socket.rs::on_open_futures_private",
            ),
            (
                "ping",
                "safe-cex-venue-kucoin/src/ws_ping.rs::ping_futures_private",
            ),
            (
                "onMessage",
                "safe-cex-venue-kucoin/src/message_routing.rs::futures_private_message",
            ),
            (
                "refreshSnapshotSoon",
                "safe-cex-venue-kucoin/src/private_refresh.rs::refresh_snapshot_soon",
            ),
            (
                "reconnect",
                "safe-cex-venue-kucoin/src/socket_setup.rs::reconnect_futures_private",
            ),
            (
                "dispose",
                "safe-cex-venue-kucoin/src/private_dispose.rs::dispose_futures_private",
            ),
            (
                "handleOrderTopic",
                "safe-cex-venue-kucoin/src/order_lifecycle.rs::handle_futures_order_topic",
            ),
            (
                "handleAlgoOrdersTopic",
                "safe-cex-venue-kucoin/src/message_routing.rs::algo_order_message",
            ),
            (
                "subscribe",
                "safe-cex-venue-kucoin/src/private_socket.rs::subscribe_futures_private",
            ),
        ],
    )?;
    Ok(())
}
fn initialize_futures_public<V: Clone, E>(h: &ClassOps<V, E>, receiver: &V) -> Result<(), E> {
    (h.define)(receiver, "messageHandlers", (h.value.object)(vec![]))?;
    (h.define)(receiver, "token", (h.null)())?;
    (h.define)(receiver, "socketEndpoint", (h.null)())?;
    (h.define)(receiver, "role", (h.value.undefined)())?;
    (h.define)(receiver, "shardIndex", (h.value.undefined)())?;
    (h.define)(receiver, "shardCount", (h.value.undefined)())?;
    arrows(
        h,
        receiver,
        "KucoinPublicWebsocket",
        &[
            (
                "connectAndSubscribe",
                "safe-cex-venue-kucoin/src/socket_setup.rs::connect_futures_public",
            ),
            (
                "reconnect",
                "safe-cex-venue-kucoin/src/socket_setup.rs::reconnect_futures_public",
            ),
            (
                "onOpen",
                "safe-cex-venue-kucoin/src/public_lifecycle.rs::on_open_futures",
            ),
            (
                "onMarketsChanged",
                "safe-cex-venue-kucoin/src/public_lifecycle.rs::on_markets_changed",
            ),
            (
                "ping",
                "safe-cex-venue-kucoin/src/ws_ping.rs::ping_futures_public",
            ),
            (
                "subscribe",
                "safe-cex-venue-kucoin/src/public_subscriptions.rs::subscribe_public",
            ),
            (
                "snapshotShardSymbols",
                "safe-cex-venue-kucoin/src/ws_helpers.rs::snapshot_shard_symbols",
            ),
            (
                "subscribeActiveTicker",
                "safe-cex-venue-kucoin/src/public_subscriptions.rs::subscribe_active_ticker",
            ),
            (
                "unsubscribeActiveTicker",
                "safe-cex-venue-kucoin/src/public_subscriptions.rs::unsubscribe_active_ticker",
            ),
            (
                "sendChannelOp",
                "safe-cex-venue-kucoin/src/ws_helpers.rs::send_channel_op",
            ),
            (
                "onMessage",
                "safe-cex-venue-kucoin/src/message_routing.rs::futures_public_message",
            ),
            (
                "tickerForApiSymbol",
                "safe-cex-venue-kucoin/src/public_lifecycle.rs::ticker_for_api_symbol",
            ),
            (
                "handleTickerV2",
                "safe-cex-venue-kucoin/src/public_streams.rs::handle_ticker_v2",
            ),
            (
                "handleSnapshot",
                "safe-cex-venue-kucoin/src/public_streams.rs::handle_snapshot",
            ),
            (
                "handleInstrument",
                "safe-cex-venue-kucoin/src/public_streams.rs::handle_instrument",
            ),
            (
                "listenOHLCV",
                "safe-cex-venue-kucoin/src/public_streams.rs::listen_ohlcv",
            ),
            (
                "listenOrderBook",
                "safe-cex-venue-kucoin/src/public_streams.rs::listen_order_book",
            ),
        ],
    )?;
    Ok(())
}
fn initialize_spot_private<V: Clone, E>(h: &ClassOps<V, E>, receiver: &V) -> Result<(), E> {
    (h.define)(receiver, "token", (h.null)())?;
    (h.define)(receiver, "socketEndpoint", (h.null)())?;
    (h.define)(receiver, "pingInterval", (h.value.number)(5_000.))?;
    (h.define)(receiver, "orderRefreshTimeout", (h.value.undefined)())?;
    (h.define)(receiver, "balanceRefreshTimeout", (h.value.undefined)())?;
    arrows(
        h,
        receiver,
        "KucoinSpotPrivateWebsocket",
        &[
            (
                "connectAndSubscribe",
                "safe-cex-venue-kucoin/src/socket_setup.rs::connect_spot_private",
            ),
            (
                "onOpen",
                "safe-cex-venue-kucoin/src/private_socket.rs::on_open_spot_private",
            ),
            (
                "ping",
                "safe-cex-venue-kucoin/src/ws_ping.rs::ping_spot_private",
            ),
            (
                "onMessage",
                "safe-cex-venue-kucoin/src/message_routing.rs::spot_private_message",
            ),
            (
                "handleOrderTopic",
                "safe-cex-venue-kucoin/src/order_lifecycle.rs::handle_spot_order_topic",
            ),
            (
                "subscribe",
                "safe-cex-venue-kucoin/src/private_socket.rs::subscribe_spot_private",
            ),
            (
                "refreshOrdersSoon",
                "safe-cex-venue-kucoin/src/private_refresh.rs::refresh_orders_soon",
            ),
            (
                "refreshBalanceSoon",
                "safe-cex-venue-kucoin/src/private_refresh.rs::refresh_balance_soon",
            ),
        ],
    )?;
    Ok(())
}
fn initialize_spot_public<V: Clone, E>(h: &ClassOps<V, E>, receiver: &V) -> Result<(), E> {
    (h.define)(receiver, "token", (h.null)())?;
    (h.define)(receiver, "socketEndpoint", (h.null)())?;
    (h.define)(receiver, "pingInterval", (h.value.number)(5_000.))?;
    arrows(
        h,
        receiver,
        "KucoinSpotPublicWebsocket",
        &[
            (
                "connectAndSubscribe",
                "safe-cex-venue-kucoin/src/socket_setup.rs::connect_spot_public",
            ),
            (
                "onOpen",
                "safe-cex-venue-kucoin/src/public_lifecycle.rs::on_open_spot",
            ),
            (
                "ping",
                "safe-cex-venue-kucoin/src/ws_ping.rs::ping_spot_public",
            ),
            (
                "subscribe",
                "safe-cex-venue-kucoin/src/socket_setup.rs::subscribe_spot_public",
            ),
            (
                "subscribeTopic",
                "safe-cex-venue-kucoin/src/spot_topics.rs::subscribe_spot_topic",
            ),
            (
                "unsubscribeTopic",
                "safe-cex-venue-kucoin/src/spot_topics.rs::unsubscribe_spot_topic",
            ),
            (
                "onMessage",
                "safe-cex-venue-kucoin/src/message_routing.rs::spot_public_message",
            ),
            (
                "handleTickerSnapshot",
                "safe-cex-venue-kucoin/src/public_streams.rs::handle_spot_ticker_snapshot",
            ),
            (
                "listenOHLCV",
                "safe-cex-venue-kucoin/src/public_streams.rs::listen_spot_ohlcv",
            ),
            (
                "listenOrderBook",
                "safe-cex-venue-kucoin/src/public_streams.rs::listen_spot_order_book",
            ),
            (
                "getCandleHandlerTopic",
                "safe-cex-venue-kucoin/src/ws_helpers.rs::get_candle_handler_topic",
            ),
        ],
    )?;
    Ok(())
}
pub fn define_futures_exchange<V: Clone + 'static, E: 'static>(
    h: Rc<ClassOps<V, E>>,
) -> Result<V, E> {
    let base = (h.value.capture)("BaseExchange")?;
    let ops = h.clone();
    let class = (h.derive)(
        "KucoinExchange",
        base,
        3,
        Rc::new(move |active, target, args| futures_constructor(ops.clone(), active, target, args)),
    )?;
    (h.prototype_member)(
        &class,
        "KucoinExchange",
        "apiMarginCoin",
        "get",
        0,
        "safe-cex-venue-kucoin/src/sizing.rs::api_margin_coin",
    )?;
    (h.prototype_member)(
        &class,
        "KucoinExchange",
        "getOrFetchPositionMode",
        "method",
        1,
        "safe-cex-venue-kucoin/src/position_metadata.rs::get_or_fetch_position_mode",
    )?;
    (h.prototype_member)(
        &class,
        "KucoinExchange",
        "getOrFetchMarginType",
        "method",
        1,
        "safe-cex-venue-kucoin/src/position_metadata.rs::get_or_fetch_margin_type",
    )?;
    Ok(class)
}

pub fn define_spot_exchange<V: Clone + 'static, E: 'static>(h: Rc<ClassOps<V, E>>) -> Result<V, E> {
    let base = (h.value.capture)("BaseExchange")?;
    let ops = h.clone();
    let class = (h.derive)(
        "KucoinSpotExchange",
        base,
        3,
        Rc::new(move |active, target, args| spot_constructor(ops.clone(), active, target, args)),
    )?;
    Ok(class)
}

pub fn define_futures_private<V: Clone + 'static, E: 'static>(
    h: Rc<ClassOps<V, E>>,
) -> Result<V, E> {
    let base = (h.value.capture)("BaseWebSocket")?;
    let ops = h.clone();
    let class = (h.derive)(
        "KucoinPrivateWebsocket",
        base,
        0,
        Rc::new(move |active, target, args| {
            implicit_socket(&ops, active, target, args, |receiver| {
                initialize_futures_private(&ops, receiver)
            })
        }),
    )?;
    Ok(class)
}

pub fn define_futures_public<V: Clone + 'static, E: 'static>(
    h: Rc<ClassOps<V, E>>,
) -> Result<V, E> {
    let base = (h.value.capture)("BaseWebSocket")?;
    let ops = h.clone();
    let class = (h.derive)(
        "KucoinPublicWebsocket",
        base,
        2,
        Rc::new(move |active, target, args| {
            let receiver = std::cell::RefCell::new(None::<V>);
            let parent = args
                .first()
                .cloned()
                .unwrap_or_else(|| (ops.value.undefined)());
            let opts = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| (ops.value.undefined)());
            crate::socket_setup::public_constructor(
                parent,
                |parent| {
                    *receiver.borrow_mut() = Some((ops.super_construct)(
                        (ops.superclass)(&active)?,
                        target,
                        vec![parent],
                    )?);
                    Ok(())
                },
                || initialize_futures_public(&ops, receiver.borrow().as_ref().unwrap()),
                |key| {
                    if (ops.value.nullish)(&opts) {
                        return Ok(None);
                    }
                    let value = (ops.value.get)(&opts, key)?;
                    Ok(if (ops.value.nullish)(&value) {
                        None
                    } else {
                        Some(value)
                    })
                },
                |key| match key {
                    "detail" => (ops.value.literal)("detail"),
                    "0" => (ops.value.number)(0.),
                    "1" => (ops.value.number)(1.),
                    _ => unreachable!(),
                },
                |key, value| (ops.value.set)(receiver.borrow().as_ref().unwrap(), key, value),
            )?;
            Ok(receiver.into_inner().unwrap())
        }),
    )?;
    Ok(class)
}

pub fn define_spot_private<V: Clone + 'static, E: 'static>(h: Rc<ClassOps<V, E>>) -> Result<V, E> {
    let base = (h.value.capture)("BaseWebSocket")?;
    let ops = h.clone();
    let class = (h.derive)(
        "KucoinSpotPrivateWebsocket",
        base,
        0,
        Rc::new(move |active, target, args| {
            implicit_socket(&ops, active, target, args, |receiver| {
                initialize_spot_private(&ops, receiver)
            })
        }),
    )?;
    (h.prototype_member)(
        &class,
        "KucoinSpotPrivateWebsocket",
        "dispose",
        "method",
        0,
        "safe-cex-venue-kucoin/src/private_dispose.rs::dispose_spot_private",
    )?;
    Ok(class)
}

pub fn define_spot_public<V: Clone + 'static, E: 'static>(h: Rc<ClassOps<V, E>>) -> Result<V, E> {
    let base = (h.value.capture)("BaseWebSocket")?;
    let ops = h.clone();
    let class = (h.derive)(
        "KucoinSpotPublicWebsocket",
        base,
        0,
        Rc::new(move |active, target, args| {
            implicit_socket(&ops, active, target, args, |receiver| {
                initialize_spot_public(&ops, receiver)
            })
        }),
    )?;
    Ok(class)
}
