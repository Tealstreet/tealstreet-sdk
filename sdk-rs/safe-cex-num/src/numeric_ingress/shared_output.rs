//! Exhaustive shared model registrations; the trait owner avoids a types -> num cycle.
//! Value/capability-backed graphs are deliberately not implicitly eligible.
use super::JsNumberOutput;

impl JsNumberOutput for safe_cex_types::BothPositionSide {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Both });
    }
}

impl JsNumberOutput for safe_cex_types::Candle {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { timestamp, open, high, low, close, volume, last_side });
    }
}

impl JsNumberOutput for safe_cex_types::ClosedPnlEntryContracts {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Variant0(value), Variant1(value) });
    }
}

impl JsNumberOutput for safe_cex_types::ClosedPnlEntryEntryPrice {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Variant0(value), Variant1(value) });
    }
}

impl JsNumberOutput for safe_cex_types::ClosedPnlEntryExitPrice {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Variant0(value), Variant1(value) });
    }
}

impl JsNumberOutput for safe_cex_types::ClosedPnlEntryFees {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Variant0(value), Variant1(value) });
    }
}

impl JsNumberOutput for safe_cex_types::ClosedPnlEntryLeverage {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Variant0(value), Variant1(value) });
    }
}

impl JsNumberOutput for safe_cex_types::ClosedPnlEntryNotional {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Variant0(value), Variant1(value) });
    }
}

impl JsNumberOutput for safe_cex_types::ClosedPnlEntryUnrealizedPnl {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Variant0(value), Variant1(value) });
    }
}

impl JsNumberOutput for safe_cex_types::CoinBalance {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { base, amount, notional });
    }
}

impl JsNumberOutput for safe_cex_types::DataProvenance {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Venue, Derived });
    }
}

impl JsNumberOutput for safe_cex_types::ExchangeName {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Apexomni, Asterdex, Backpack, Binance, Binanceinverse, Binancespot, Binancepm, Bingx, Bitget, Bitgetinverse, Bitgetusdc, Bitrue, Bitunix, Blofin, Breakoutprop, Bybit, Bybitdemo, Bybitinverse, Bybitspot, Hyrotrader, Coinbase, Coinbasespot, Cryptocom, Grvt, Hyperliquid, Hyperliquidspot, Lighter, LighterRobinhood, Mexc, Okx, Okxdemo, Okxeu, Okxeudemo, Paradex, Phemex, Polymarket, Kodiakfi, Mode, Salsadex, TealstreetDex, Txflow, What, Weex, Woo, Woofi, Kucoin, Kucoinspot, Kraken, Krakenspot });
    }
}

impl JsNumberOutput for safe_cex_types::ExchangeOptionsAuth {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { user_address, signer_address, private_key });
    }
}

impl JsNumberOutput for safe_cex_types::ExchangeOptionsBybitSiteId {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { BraBtl });
    }
}

impl JsNumberOutput for safe_cex_types::ExchangeOptionsExternalPublicDataVariant0 {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { r#type, url, exchange_id, auth_token });
    }
}

impl JsNumberOutput for safe_cex_types::ExchangeOptionsExternalPublicDataVariant0Type {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Websocket });
    }
}

impl JsNumberOutput for safe_cex_types::ExchangeOptionsExternalPublicDataVariant1Type {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { InProcess });
    }
}

impl JsNumberOutput for safe_cex_types::ExchangeOptionsExtra {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { hyrotrader_mode, tick_interval, recv_window, asterdex_api_version, aster_builder_address, aster_builder_fee_rate_maker, aster_builder_fee_rate_taker, lighter_builder_fee_mode, lighter_integrator_account_index, lighter_stop_slippage_pct, kucoin_broker });
    }
}

impl JsNumberOutput for safe_cex_types::ExchangeOptionsExtraAsterdexApiVersion {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { V1, V3 });
    }
}

impl JsNumberOutput for safe_cex_types::ExchangeOptionsExtraKucoinBroker {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { partner, broker_key, broker_name, verify });
    }
}

impl JsNumberOutput for safe_cex_types::ExchangeOptionsExtraLighterBuilderFeeMode {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Dynamic, Static });
    }
}

impl JsNumberOutput for safe_cex_types::ExchangeOptionsHyperliquidBuilder {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { address, fee_tenths_bps });
    }
}

impl JsNumberOutput for safe_cex_types::ExchangeOptionsTuning {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { active_ticker_symbols, calculate_upnl_from_realtime_last_price, data_update_throttle_multiplier, skip_renders_when_window_is_hidden, console_logging });
    }
}

impl JsNumberOutput for safe_cex_types::Execution {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { id, timestamp, symbol, order_id, price, qty, side, commission, commission_asset, position_side, realized_pnl, maker, venue_reported_revenue, venue_reported_revenue_asset });
    }
}

impl JsNumberOutput for safe_cex_types::ExecutionPositionSide {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Side(value), Both(value) });
    }
}

impl JsNumberOutput for safe_cex_types::ExecutionsOptions {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { symbol, order_id, from, to, limit });
    }
}

impl JsNumberOutput for safe_cex_types::IncomeCategory {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { RealizedPnl, Funding, Commission, Rebate, Transfer, Liquidation, Other });
    }
}

impl JsNumberOutput for safe_cex_types::MarginType {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Isolated, Cross });
    }
}

impl JsNumberOutput for safe_cex_types::MarketLimits {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { amount, leverage, price, cost });
    }
}

impl JsNumberOutput for safe_cex_types::MarketLimitsAmount {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { min, max });
    }
}

impl JsNumberOutput for safe_cex_types::MarketLimitsLeverage {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { min, max });
    }
}

impl JsNumberOutput for safe_cex_types::MarketLimitsPrice {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { min, max });
    }
}

impl JsNumberOutput for safe_cex_types::OptionGreeks {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { delta, gamma, vega, theta, units });
    }
}

impl JsNumberOutput for safe_cex_types::OptionRight {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Call, Put });
    }
}

impl JsNumberOutput for safe_cex_types::OptionTerms {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { underlying, expiry, strike, right, style, settlement, premium_asset, settle_asset });
    }
}

impl JsNumberOutput for safe_cex_types::OptionTermsSettlement {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Cash });
    }
}

impl JsNumberOutput for safe_cex_types::OptionTermsStyle {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { European });
    }
}

impl JsNumberOutput for safe_cex_types::PositionOption {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { greeks });
    }
}

impl JsNumberOutput for safe_cex_types::MarketPrecision {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { amount, price, base, quote });
    }
}

impl JsNumberOutput for safe_cex_types::OrderBook {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { bids, asks });
    }
}

impl JsNumberOutput for safe_cex_types::OrderBookOrders {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { price, amount, total });
    }
}

impl JsNumberOutput for safe_cex_types::OrderExtra {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { remaining_native, filled_in_placement_units, remaining_notional, conditional_order_price, reject_reason, is_from_chaser, chaser_id, chaser_attempts, chaser_max_attempts });
    }
}

impl JsNumberOutput for safe_cex_types::OrderRejectReason {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { PostOnly, InsufficientBalance, ReduceOnly });
    }
}

impl JsNumberOutput for safe_cex_types::OrderSide {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Buy, Sell });
    }
}

impl JsNumberOutput for safe_cex_types::OrderStatus {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Open, Closed, Canceled, Creating, Updating, Canceling });
    }
}

impl JsNumberOutput for safe_cex_types::OrderStopTriggerType {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { MarkPrice, LastPrice, IndexPrice });
    }
}

impl JsNumberOutput for safe_cex_types::OrderTimeInForce {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { GoodTillCancel, ImmediateOrCancel, FillOrKill, PostOnly });
    }
}

impl JsNumberOutput for safe_cex_types::OrderType {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Market, Limit, Conditional, StopLoss, TakeProfit, TrailingStopLoss });
    }
}

impl JsNumberOutput for safe_cex_types::PlaceOrderOpts {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { symbol, r#type, side, amount, price, trigger_price, stop_loss, take_profit, reduce_only, time_in_force, trigger_type, tp_trigger_type, sl_trigger_type, client_order_id, max_slippage_pct, close, implied_volatility });
    }
}

impl JsNumberOutput for safe_cex_types::PositionExtra {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { contracts_native, unrealized_pnl_native });
    }
}

impl JsNumberOutput for safe_cex_types::PositionMode {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Hedged, OneWay });
    }
}

impl JsNumberOutput for safe_cex_types::PositionSide {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Long, Short });
    }
}

impl JsNumberOutput for safe_cex_types::store_model::TaskOrigin {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Web, Cli, Webhook, Automation });
    }
}

impl JsNumberOutput for safe_cex_types::store_model::TwapStatus {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Idle, Running, Paused, Completed });
    }
}

impl JsNumberOutput for safe_cex_types::store_model::SliceAction {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Market, Next });
    }
}

// Uninhabited marker: this admits no runtime capability or skipped field.
impl JsNumberOutput for safe_cex_types::RemoteSignerCapability {
    fn check_number_output(&self) {
        match *self {}
    }
}

// Uninhabited marker: this admits no runtime capability or skipped field.
impl JsNumberOutput for safe_cex_types::PayloadSignerCapability {
    fn check_number_output(&self) {
        match *self {}
    }
}

// Uninhabited marker: this admits no runtime capability or skipped field.
impl JsNumberOutput for safe_cex_types::TransportCapability {
    fn check_number_output(&self) {
        match *self {}
    }
}

// Uninhabited marker: this admits no runtime capability or skipped field.
impl JsNumberOutput for safe_cex_types::PublicDataSourceCapability {
    fn check_number_output(&self) {
        match *self {}
    }
}

impl JsNumberOutput for safe_cex_types::ExchangeOptionsExternalPublicDataVariant1 {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { r#type, source });
    }
}
impl JsNumberOutput for safe_cex_types::ExchangeOptionsExternalPublicData {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Variant0(value), Variant1(value) });
    }
}

impl JsNumberOutput for safe_cex_types::ExchangeOptionsPolymarket {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { api_key, api_secret, api_passphrase, signer_type, funder_address });
    }
}

impl JsNumberOutput for safe_cex_types::ExchangeOptions {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { key, secret, passphrase, secondary_key, secondary_secret, secondary_passphrase, secondary_proxy_url, auth, paradex_address, subkey_address, grvt_signer_address, grvt_trading_account_id, stark_private_key, vault, application_id, api_key_index, hyperliquid_builder, testnet, enable_spot_markets, enable_option_markets, enable_inverse_markets, bybit_site_id, public_data_only, private_only, tealstreet_proxy, tealstreet_tunnel, tealstreet_ws_route, override_proxy_url, proxy_region, omni_key_seed, subaccount, orderly_exchange_id, tuning, extra, polymarket, remote_signer, transport, payload_signer, hosted_mode, external_public_data });
    }
}

impl JsNumberOutput for safe_cex_types::Market {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { id, symbol, pretty_symbol, base, quote, active, precision, limits, option, contract_size, is_expiring, is_inverse, is_spot, is_perpetual, is_read_only, taker_fee, maker_fee, runtime } passthrough { info });
    }
}

impl JsNumberOutput for safe_cex_types::Order {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { id, parent_id, status, symbol, r#type, side, price, amount, filled, remaining, reduce_only, close, client_order_id, time_in_force, last_update, trigger_type, tp_trigger_type, sl_trigger_type, take_profit, stop_loss, extra } passthrough { info });
    }
}

impl JsNumberOutput for safe_cex_types::Position {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { id, symbol, side, entry_price, notional, leverage, margin_type, position_mode, unrealized_pnl, realized_pnl, contracts, liquidation_price, margin_size, extra, last_update, option } passthrough { info });
    }
}

impl JsNumberOutput for safe_cex_types::Ticker {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { id, symbol, bid, ask, last, mark, index, mid_price, percentage, open_interest, funding_rate, funding_time, volume, quote_volume, timestamp } passthrough { info });
    }
}

impl JsNumberOutput for safe_cex_types::CurrencyBalance {
    fn check_number_output(&self) {
        match self {
            Self::Declared(balance) => balance.check_number_output(),
            Self::AvailableLocked { available, locked } => {
                available.check_number_output();
                locked.check_number_output();
            }
        }
    }
}

impl JsNumberOutput for safe_cex_types::Balance {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { used, free, total, upnl, currencies, coins } passthrough { info });
    }
}

impl<T: JsNumberOutput> JsNumberOutput for safe_cex_types::ModelField<T> {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Absent, Undefined, Value(value) });
    }
}

impl JsNumberOutput for safe_cex_types::PreparedOrderOpts {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { symbol, r#type, side, amount, price, trigger_price, stop_loss, take_profit, reduce_only, time_in_force, trigger_type, tp_trigger_type, sl_trigger_type, client_order_id, max_slippage_pct, close, implied_volatility });
    }
}

impl JsNumberOutput for safe_cex_types::ExchangeOptionsExtraHyrotraderMode {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Demo, Production });
    }
}

impl JsNumberOutput for safe_cex_types::NumericValue {
    fn check_number_output(&self) {
        crate::js_number_output!(self, enum { Number(value), Text(value) });
    }
}

impl JsNumberOutput for safe_cex_types::MarketRuntime {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { kind, spot, swap, future, contract, linear, inverse, fees });
    }
}

impl JsNumberOutput for safe_cex_types::MarketFees {
    fn check_number_output(&self) {
        crate::js_number_output!(self, struct { maker, taker });
    }
}
