//! Class field initialization and constructor bodies at original import boundaries.
use crate::collection_reads::{Callback, ReadHost};
use std::rc::Rc;
pub trait CompositionHost: ReadHost {
    fn super_exchange(&self, args: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
    fn super_public(&self, args: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
    fn define(&self, r: Self::Value, key: &str, v: Self::Value) -> Result<(), Self::Error>;
    fn new_set(&self) -> Result<Self::Value, Self::Error>;
    fn new_map(&self) -> Result<Self::Value, Self::Error>;
    fn rule(&self, label: &str, pattern: &str) -> Result<Self::Value, Self::Error>;
    fn grafana(&self, which: &str) -> Result<Self::Value, Self::Error>;
    fn binding(&self, name: &'static str) -> Result<Self::Value, Self::Error>;
    fn construct_socket(
        &self,
        which: &'static str,
        r: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    // Exact ABI provisioning is pure; each function retains its name and lexical receiver.
    fn member(&self, name: &'static str, target: &'static str, r: Self::Value) -> Self::Value;
    fn named_function(&self, name: &'static str, f: Callback<Self>) -> Self::Value;
}
fn get<H: CompositionHost>(h: &H, r: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    h.get(r, k)
}
fn method<H: CompositionHost>(
    h: &H,
    r: H::Value,
    k: &str,
) -> Result<(H::Value, H::Value), H::Error> {
    Ok((get(h, &r, k)?, r))
}
fn invoke<H: CompositionHost>(
    h: &H,
    p: (H::Value, H::Value),
    a: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    h.call(p.0, p.1, a)
}
pub const EXCHANGE_MEMBERS: [(&str, &str); 46] = [
    (
        "isSuccessCode",
        "safe-cex-venue-bitrue/src/read_helpers.rs::is_success_code",
    ),
    (
        "parseMaxLeverageFromErrorMessage",
        "safe-cex-venue-bitrue/src/read_helpers.rs::parse_max_leverage_from_error_message",
    ),
    (
        "setMarketMaxLeverage",
        "safe-cex-venue-bitrue/src/read_helpers.rs::set_market_max_leverage",
    ),
    (
        "parseMaxLeverageFromBracketResponse",
        "safe-cex-venue-bitrue/src/read_helpers.rs::parse_max_leverage_from_bracket_response",
    ),
    (
        "fetchAndSyncSymbolMaxLeverage",
        "safe-cex-venue-bitrue/src/leverage_reads.rs::fetch_and_sync_symbol_max_leverage",
    ),
    (
        "normalizeTimestampMs",
        "safe-cex-venue-bitrue/src/read_helpers.rs::normalize_timestamp_ms",
    ),
    (
        "normalizeTradeSymbol",
        "safe-cex-venue-bitrue/src/read_helpers.rs::normalize_trade_symbol",
    ),
    (
        "pickTradeOrderId",
        "safe-cex-venue-bitrue/src/read_helpers.rs::pick_trade_order_id",
    ),
    (
        "resolveTradeQty",
        "safe-cex-venue-bitrue/src/read_helpers.rs::resolve_trade_qty",
    ),
    (
        "fetchBitrueTradeRows",
        "safe-cex-venue-bitrue/src/collection_reads.rs::trade_rows",
    ),
    (
        "dispose",
        "safe-cex-venue-bitrue/src/runtime_lifecycle.rs::dispose",
    ),
    (
        "handleFillForFastRefresh",
        "safe-cex-venue-bitrue/src/refresh_entrypoints.rs::handle_fill_for_fast_refresh",
    ),
    (
        "queueFastPrivateRefresh",
        "safe-cex-venue-bitrue/src/runtime_lifecycle.rs::queue_refresh",
    ),
    (
        "runFastPrivateRefresh",
        "safe-cex-venue-bitrue/src/runtime_lifecycle.rs::run_refresh",
    ),
    (
        "validateAccount",
        "safe-cex-venue-bitrue/src/runtime_lifecycle.rs::validate",
    ),
    (
        "start",
        "safe-cex-venue-bitrue/src/runtime_lifecycle.rs::start",
    ),
    (
        "updateOrder",
        "safe-cex-venue-bitrue/src/trading_rest.rs::update",
    ),
    (
        "tick",
        "safe-cex-venue-bitrue/src/runtime_lifecycle.rs::tick",
    ),
    (
        "fetchBalanceAndPositionsRaw",
        "safe-cex-venue-bitrue/src/account_reads.rs::fetch_balance_and_positions_raw",
    ),
    (
        "fetchBalance",
        "safe-cex-venue-bitrue/src/account_reads.rs::fetch_balance",
    ),
    (
        "fetchOpenPositions",
        "safe-cex-venue-bitrue/src/collection_reads.rs::open_positions",
    ),
    (
        "getPositionAccounts",
        "safe-cex-venue-bitrue/src/account_helpers.rs::get_position_accounts",
    ),
    (
        "getAccountPositionSnapshots",
        "safe-cex-venue-bitrue/src/account_helpers.rs::get_account_position_snapshots",
    ),
    (
        "fetchPositionMetadata",
        "safe-cex-venue-bitrue/src/position_metadata.rs::fetch_position_metadata",
    ),
    (
        "fetchPositions",
        "safe-cex-venue-bitrue/src/collection_reads.rs::positions",
    ),
    (
        "fetchBalanceAndPositions",
        "safe-cex-venue-bitrue/src/collection_reads.rs::balance_positions",
    ),
    (
        "parsePositions",
        "safe-cex-venue-bitrue/src/runtime_lifecycle.rs::parse_positions",
    ),
    (
        "parseBalance",
        "safe-cex-venue-bitrue/src/balance.rs::parse_balance",
    ),
    (
        "fetchOrders",
        "safe-cex-venue-bitrue/src/collection_reads.rs::orders",
    ),
    (
        "fetchAndEmitOrders",
        "safe-cex-venue-bitrue/src/collection_reads.rs::emit_orders",
    ),
    (
        "fetchOrdersForSymbol",
        "safe-cex-venue-bitrue/src/collection_reads.rs::symbol_orders",
    ),
    (
        "fetchHistoricalTradesInternal",
        "safe-cex-venue-bitrue/src/collection_reads.rs::history",
    ),
    (
        "fetchHistoricalFills",
        "safe-cex-venue-bitrue/src/collection_reads.rs::fills",
    ),
    (
        "fetchMarkets",
        "safe-cex-venue-bitrue/src/trading_rest.rs::markets",
    ),
    (
        "fetchOHLCV",
        "safe-cex-venue-bitrue/src/trading_rest.rs::candles",
    ),
    (
        "formatCreateOrder",
        "safe-cex-venue-bitrue/src/trading_rest.rs::format_order",
    ),
    (
        "placeOrder",
        "safe-cex-venue-bitrue/src/trading_rest.rs::place",
    ),
    (
        "placeOrders",
        "safe-cex-venue-bitrue/src/trading_rest.rs::places",
    ),
    (
        "placeOrderBatch",
        "safe-cex-venue-bitrue/src/trading_rest.rs::batch",
    ),
    (
        "fetchSymbolMaxLeverage",
        "safe-cex-venue-bitrue/src/leverage_reads.rs::fetch_symbol_max_leverage",
    ),
    (
        "updateSymbolMetadata",
        "safe-cex-venue-bitrue/src/leverage_reads.rs::update_symbol_metadata",
    ),
    (
        "setLeverage",
        "safe-cex-venue-bitrue/src/trading_rest.rs::leverage",
    ),
    (
        "changeMarginType",
        "safe-cex-venue-bitrue/src/refresh_entrypoints.rs::change_margin_type",
    ),
    (
        "cancelOrders",
        "safe-cex-venue-bitrue/src/trading_rest.rs::cancel",
    ),
    (
        "listenOHLCV",
        "safe-cex-venue-bitrue/src/runtime_lifecycle.rs::listen_candles",
    ),
    (
        "listenOrderBook",
        "safe-cex-venue-bitrue/src/runtime_lifecycle.rs::listen_book",
    ),
];
pub const PUBLIC_MEMBERS: [(&str, &str); 16] = [
    (
        "resolveMarketSymbol",
        "safe-cex-venue-bitrue/src/ws_sync.rs::resolve_market_symbol",
    ),
    (
        "convertToWsSymbol",
        "safe-cex-venue-bitrue/src/ws_symbol_helpers.rs::convert_to_ws_symbol",
    ),
    (
        "ensureTickerSubscription",
        "safe-cex-venue-bitrue/src/public_bodies.rs::ensure_ticker_subscription",
    ),
    (
        "connectAndSubscribe",
        "safe-cex-venue-bitrue/src/public_bodies.rs::connect_and_subscribe",
    ),
    (
        "onOpen",
        "safe-cex-venue-bitrue/src/public_bodies.rs::on_open",
    ),
    (
        "onMarketsChanged",
        "safe-cex-venue-bitrue/src/public_bodies.rs::on_markets_changed",
    ),
    (
        "handlePingEvent",
        "safe-cex-venue-bitrue/src/ws_sync.rs::handle_public_ping",
    ),
    ("ping", "safe-cex-venue-bitrue/src/ws_public.rs::ping"),
    (
        "subscribe",
        "safe-cex-venue-bitrue/src/public_bodies.rs::subscribe",
    ),
    (
        "formatKlineTopic",
        "safe-cex-venue-bitrue/src/ws_symbol_helpers.rs::format_kline_topic",
    ),
    (
        "mapKline",
        "safe-cex-venue-bitrue/src/public_bodies.rs::map_kline",
    ),
    (
        "listenOHLCV",
        "safe-cex-venue-bitrue/src/public_streams.rs::listen_candles",
    ),
    (
        "onMessage",
        "safe-cex-venue-bitrue/src/public_streams.rs::message",
    ),
    (
        "handleTickersEvent",
        "safe-cex-venue-bitrue/src/public_bodies.rs::handle_tickers_event",
    ),
    (
        "makeRequest",
        "safe-cex-venue-bitrue/src/public_bodies.rs::make_request",
    ),
    (
        "listenOrderBook",
        "safe-cex-venue-bitrue/src/public_streams.rs::listen_book",
    ),
];
pub fn initialize_exchange<H: CompositionHost>(h: &H, r: H::Value) -> Result<(), H::Error> {
    h.define(
        r.clone(),
        "clientOrderIdRule",
        h.rule("Bitrue", "^.{1,31}$")?,
    )?;
    for key in ["xhr", "unlimitedXHR", "publicWebsocket", "privateWebsocket"] {
        h.define(r.clone(), key, h.undefined())?;
    }
    h.define(r.clone(), "prioritySymbols", h.new_set()?)?;
    h.define(r.clone(), "inProgressOrderFetchId", h.number(0.0))?;
    h.define(
        r.clone(),
        "grafanaEndpointsPeriodic",
        h.grafana("periodic")?,
    )?;
    h.define(r.clone(), "grafanaEndpointsEvery", h.grafana("every")?)?;
    h.define(r.clone(), "recentWsRequests", h.object(vec![]))?;
    h.define(r.clone(), "fastPrivateRefreshTimeoutId", h.undefined())?;
    h.define(r.clone(), "pendingOrderRefreshSymbols", h.new_set()?)?;
    h.define(r.clone(), "isFastPrivateRefreshRunning", h.boolean(false))?;
    h.define(r.clone(), "positionMetadataBySymbol", h.new_map()?)?;
    for (name, target) in EXCHANGE_MEMBERS {
        h.define(r.clone(), name, h.member(name, target, r.clone()))?;
    }
    Ok(())
}
pub fn constructor_body<H: CompositionHost>(
    h: &H,
    r: H::Value,
    opts: H::Value,
) -> Result<(), H::Error> {
    let p = method(h, r.clone(), "createXhrWithSecondaryKey")?;
    let factory = h.binding("createAPI")?;
    let host = h.clone();
    let limiter = h.named_function(
        "rateLimit",
        Rc::new(move |a| {
            let f = host.binding("rateLimit")?;
            host.call(
                f,
                host.undefined(),
                vec![
                    a.first().cloned().unwrap_or_else(|| host.undefined()),
                    host.object(vec![("maxRPS", host.number(5.0))]),
                ],
            )
        }),
    );
    let xhr = invoke(
        h,
        p,
        vec![
            opts.clone(),
            factory,
            h.object(vec![("rateLimit", limiter)]),
        ],
    )?;
    h.set(&r, "xhr", xhr)?;
    let api = h.binding("createAPI")?;
    let unlimited = h.call(api, h.undefined(), vec![opts, r.clone()])?;
    h.set(&r, "unlimitedXHR", unlimited)?;
    let public = h.construct_socket("public", r.clone())?;
    h.set(&r, "publicWebsocket", public)?;
    let private = h.construct_socket("private", r.clone())?;
    h.set(&r, "privateWebsocket", private)?;
    let p = method(h, get(h, &r, "emitter")?, "on")?;
    invoke(
        h,
        p,
        vec![h.text("fill"), get(h, &r, "handleFillForFastRefresh")?],
    )?;
    Ok(())
}
pub fn construct_exchange<H: CompositionHost>(
    h: &H,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let opts = args.get(1).cloned().unwrap_or_else(|| h.undefined());
    let forwarded = (0..3)
        .map(|i| args.get(i).cloned().unwrap_or_else(|| h.undefined()))
        .collect();
    let r = h.super_exchange(forwarded)?;
    initialize_exchange(h, r.clone())?;
    constructor_body(h, r.clone(), opts)?;
    Ok(r)
}
pub fn construct_public<H: CompositionHost>(
    h: &H,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let r = h.super_public(args)?;
    h.define(r.clone(), "topics", h.object(vec![]))?;
    h.define(r.clone(), "messageHandlers", h.object(vec![]))?;
    h.define(r.clone(), "tickerSymbols", h.new_set()?)?;
    h.define(r.clone(), "resolveRequests", h.object(vec![]))?;
    for (name, target) in PUBLIC_MEMBERS {
        h.define(r.clone(), name, h.member(name, target, r.clone()))?;
    }
    Ok(r)
}
