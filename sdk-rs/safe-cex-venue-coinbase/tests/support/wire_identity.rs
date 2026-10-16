use safe_cex_venue_coinbase::starbase_cutover::*;
use safe_cex_venue_coinbase::types::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::cell::Cell;

pub trait NativeWireIdentity {
    fn assert_native_wire_identity(&self, raw: &Value, path: &str);
}

thread_local! {
    static COMPLETE_IDENTITY_CALLS: Cell<usize> = const { Cell::new(0) };
}

pub fn reset_complete_identity_calls() {
    COMPLETE_IDENTITY_CALLS.with(|calls| calls.set(0));
}

pub fn complete_identity_calls() -> usize {
    COMPLETE_IDENTITY_CALLS.with(Cell::get)
}

// Count inside the passed closure, so skipping its invocation cannot mint a call.
pub fn counted_identity<T>(fields: impl Fn(&T, &Value)) -> impl Fn(&T, &Value) {
    move |value, raw| {
        fields(value, raw);
        COMPLETE_IDENTITY_CALLS.with(|calls| calls.set(calls.get() + 1));
    }
}

pub fn assert_native_identity_and_fields<T: NativeWireIdentity>(
    value: &T,
    raw: &Value,
    path: &str,
    fields: impl FnOnce(&T, &Value),
) {
    fields(value, raw);
    value.assert_native_wire_identity(raw, path);
}

// Content pins captured from the unmodified live TS generators at 1857224642.
// Include labels, payloads, validity and order: cardinality alone permits replacement.
pub fn assert_wire_corpus_content(name: &str, cases: &Value) {
    let expected = match name {
        "BALANCE_RESPONSE" => "05b524cf710a7723cad4df6a7d96075241de2ab4133f97270be8db6bd3d97dd1",
        "BALANCE_RESPONSE_CFM" => {
            "61f7adebd8e328520a98a8e046f8a1b0a1cceea7a7fccfbaab6d76cdfcbbcbc3"
        }
        "CANDLES_RESPONSE" => "dbae931a06d8e3590f175821d066609b2001e1a8e790f3c66b0874a0a604235d",
        "CANDLE_EVENT" => "ca1bb506230e6c17941d182cc383c341e5175ef6b66188d04bdbae3f7203c440",
        "EDIT_ORDER_REQUEST" => "f80293d0080177d21808895bbac4e36882a508edd8d58edf88bb7bc7b1acbeea",
        "EDIT_ORDER_RESPONSE" => "f928332d6f3d1e9af98a2c858e9d56cd58f3aef68b9d9322bc236e9d1999cd8c",
        "INTX_INSTRUMENTS_RESPONSE" => {
            "86cba78c1d83bb6d5d2f38108ab41c93945f428f362eaf2734a87523f81bb28f"
        }
        "LEVEL2_EVENT" => "5dd2109f5267734f6be9cb1320bb29a2f9cd76a3f12415369ed1002141584ab7",
        "LIST_ACCOUNTS_RESPONSE" => {
            "a2b53d0f4640210f08444072e6b77136c816b63062c354065868e0b98cd4d31f"
        }
        "LIST_ORDERS_RESPONSE" => {
            "05f7ccb90e73c591a791d50d79ec3a9dfb6bbb4e66cc6ca4a7a72a5f3b5c7bbc"
        }
        "LIST_PORTFOLIOS_RESPONSE" => {
            "41a06ac081770fe651b1786ecee772953ce439b6c4e20c2e0cab3dde1671bd69"
        }
        "LIST_POSITIONS_CFM_RESPONSE" => {
            "351877d657203e59aaa073e94ad78b9c090b8620d42baf63c781f7e8efc7b7de"
        }
        "LIST_POSITIONS_INTX_RESPONSE" => {
            "93dd970a2a4437318204ce4953e1aa475278e99b64c7601b23c6493f0caaa370"
        }
        "MARKET_RESPONSE" => "8d2ea2928d7343bc20538f87371cef41d4ea5373f2e1f110ab4c97db6719eaf1",
        "ORDER_EVENT" => "a4ffa449fe83f080c74516041f8e07b6b8959af6fab23d528de0c518e1415b34",
        "PLACE_ORDER_REQUEST" => "f7a32ecc89447cf7d193f37fb3696733cb89521b8789851061951b983e7f4c17",
        "PLACE_ORDER_RESPONSE" => {
            "300a3879176709cd489a10720cf060a0e6b75d4348ff4f0bc4b639579009a72e"
        }
        "PORTFOLIO_BREAKDOWN_RESPONSE" => {
            "2b4db9d5b32e48e50016b2537802d5b5c06d8c8cf5a845c8cf9721b209e5c280"
        }
        "STOP_DIRECTION" => "a304fe70493a9e78e68532e40cea936f4bcad1bd5c031c526eb810e1aa6c90ef",
        "TICKER_EVENT" => "bd8ec2451aa6af7dd98cc079dc6c14e2c6ced40bd1759dbaf30b53956e18e642",
        "TRADE_EVENT" => "742b6b766085075ed32ba2201db5e91a449144adf5a00231ef73d5b200c96d4b",
        "CoinbaseStarbaseCutoverBlocker" => {
            "a91743c72f622b4e659133e6f1f7fb0bec8814499b0050471046a0715401c81f"
        }
        "CoinbaseStarbaseCutoverEvidence" => {
            "79099cc530530a5fcc9ff438d53fef649dcd682e4db2069f7a5e15e90b874c57"
        }
        "CoinbaseStarbaseCutoverPlan" => {
            "0e485d37d37e28438d12affe58c43412633637dfcef2cccc9c0e943f044eb023"
        }
        "CoinbaseStarbaseHttpShadowBlocker" => {
            "5387925bca6778aa22f992f58ba5dd6c3f375592ec0ea72c4132ba75a951742a"
        }
        "CoinbaseStarbaseHttpShadowReadiness" => {
            "1367266741cc2291a585eea8a868d251e1fe5c44e0acdc56a9737b2322baa153"
        }
        "CoinbaseStarbaseImplementationStatus" => {
            "3f020994dab8939c3d6a11cfd5e6adfdee41456df96b2f7fc1395a0f4edd6e2e"
        }
        "CoinbaseStarbaseVerificationStatus" => {
            "3fd18be401ee15d9634029f86ecca82637e89598a22c04d8337122feab822714"
        }
        "CoinbaseStarbaseWebsocketBlocker" => {
            "9cd4d6c58b4a70e431a65e068cc2ee18a504190858bcb89bd17219125ac29e19"
        }
        _ => panic!("wire corpus has no independent content pin: {name}"),
    };
    let bytes = serde_json::to_vec(cases).expect("wire corpus serializes");
    let digest = Sha256::digest(bytes);
    let actual = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(actual, expected, "{name}: wire corpus content fingerprint");
}

impl<T: NativeWireIdentity> NativeWireIdentity for Option<T> {
    fn assert_native_wire_identity(&self, raw: &Value, path: &str) {
        match self {
            Some(value) => value.assert_native_wire_identity(raw, path),
            None => assert!(raw.is_null(), "{path} absent optional enum"),
        }
    }
}

// Inspect the native field directly; re-encoding can conceal symmetric serde swaps.
impl NativeWireIdentity for String {
    fn assert_native_wire_identity(&self, raw: &Value, path: &str) {
        assert_eq!(
            Some(self.as_str()),
            raw.as_str(),
            "{path} decoded field identity"
        );
    }
}
impl NativeWireIdentity for bool {
    fn assert_native_wire_identity(&self, raw: &Value, path: &str) {
        assert_eq!(Some(*self), raw.as_bool(), "{path} decoded field identity");
    }
}
impl NativeWireIdentity for f64 {
    fn assert_native_wire_identity(&self, raw: &Value, path: &str) {
        assert_eq!(
            Some(self.to_bits()),
            raw.as_f64().map(f64::to_bits),
            "{path} decoded field identity"
        );
    }
}

fn assert_field<T: NativeWireIdentity>(value: &T, raw: &Value, path: &str, key: &str) {
    value.assert_native_wire_identity(&raw[key], &format!("{path}.{key}"));
}

fn assert_child<T: NativeWireIdentity>(value: &T, raw: &Value, path: &str, key: &str) {
    value.assert_native_wire_identity(&raw[key], &format!("{path}.{key}"));
}

fn assert_optional_child<T: NativeWireIdentity>(
    value: &Option<T>,
    raw: &Value,
    path: &str,
    key: &str,
) {
    match value {
        Some(value) => assert_child(value, raw, path, key),
        None => assert!(
            raw.get(key).is_none_or(Value::is_null),
            "{path}.{key} absent in decoded value"
        ),
    }
}

fn assert_children<T: NativeWireIdentity>(values: &[T], raw: &Value, path: &str, key: &str) {
    let children = raw
        .get(key)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{path}.{key} is not an array"));
    assert_eq!(
        values.len(),
        children.len(),
        "{path}.{key} decoded array length"
    );
    for (index, (value, child)) in values.iter().zip(children).enumerate() {
        value.assert_native_wire_identity(child, &format!("{path}.{key}[{index}]"));
    }
}

macro_rules! impl_identity {
    ($type:ty { fields: [$($field:ident => $key:literal),* $(,)?]; children: [$($child:ident : $child_type:ty => $child_key:literal),* $(,)?]; arrays: [$($array:ident : $array_type:ty => $array_key:literal),* $(,)?]; optional: [$($optional:ident : $optional_type:ty => $optional_key:literal),* $(,)?]; enum_fields: [$($enum_field:ident : $enum_type:ty => $enum_key:literal),* $(,)?] }) => {
        impl NativeWireIdentity for $type {
            fn assert_native_wire_identity(&self, raw: &Value, path: &str) {
                $(assert_field(&self.$field, raw, path, $key);)*
                $(assert_child::<$child_type>(&self.$child, raw, path, $child_key);)*
                $(assert_children::<$array_type>(&self.$array, raw, path, $array_key);)*
                $(assert_optional_child::<$optional_type>(&self.$optional, raw, path, $optional_key);)*
                $(assert_child::<$enum_type>(&self.$enum_field, raw, path, $enum_key);)*
            }
        }
    };
    ($type:ty { fields: [$($field:ident => $key:literal),* $(,)?]; children: [$($child:ident : $child_type:ty => $child_key:literal),* $(,)?]; arrays: [$($array:ident : $array_type:ty => $array_key:literal),* $(,)?]; optional: [$($optional:ident : $optional_type:ty => $optional_key:literal),* $(,)?] }) => {
        impl NativeWireIdentity for $type {
            fn assert_native_wire_identity(&self, raw: &Value, path: &str) {
                $(assert_field(&self.$field, raw, path, $key);)*
                $(assert_child::<$child_type>(&self.$child, raw, path, $child_key);)*
                $(assert_children::<$array_type>(&self.$array, raw, path, $array_key);)*
                $(assert_optional_child::<$optional_type>(&self.$optional, raw, path, $optional_key);)*
            }
        }
    };
}

impl_identity!(MarketResponseProductsItemFcmTradingSessionDetails {
    fields: [open_time => "open_time", close_time => "close_time", is_session_open => "is_session_open"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(MarketResponseProductsItemFutureProductDetailsPerpetualDetails {
    fields: [funding_rate => "funding_rate", open_interest => "open_interest", funding_time => "funding_time"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(MarketResponseProductsItemFutureProductDetails {
    fields: [venue => "venue", contract_expiry_type => "contract_expiry_type", contract_expiry => "contract_expiry", contract_code => "contract_code", contract_expiry_timezone => "contract_expiry_timezone", contract_root_unit => "contract_root_unit", group_short_description => "group_short_description", contract_size => "contract_size", group_description => "group_description", risk_managed_by => "risk_managed_by", contract_display_name => "contract_display_name"];
    children: [perpetual_details: MarketResponseProductsItemFutureProductDetailsPerpetualDetails => "perpetual_details"];
    arrays: [];
    optional: []
});

impl_identity!(MarketResponseProductsItem {
    fields: [quote_name => "quote_name", base_name => "base_name", cancel_only => "cancel_only", volume_percentage_change_24h => "volume_percentage_change_24h", mid_market_price => "mid_market_price", base_display_symbol => "base_display_symbol", quote_display_symbol => "quote_display_symbol", price_percentage_change_24h => "price_percentage_change_24h", quote_increment => "quote_increment", post_only => "post_only", limit_only => "limit_only", base_max_size => "base_max_size", auction_mode => "auction_mode", price => "price", product_id => "product_id", alias => "alias", quote_currency_id => "quote_currency_id", r#new => "new", trading_disabled => "trading_disabled", alias_to => "alias_to", is_disabled => "is_disabled", base_currency_id => "base_currency_id", price_increment => "price_increment", watched => "watched", quote_min_size => "quote_min_size", product_type => "product_type", volume_24h => "volume_24h", base_increment => "base_increment", base_min_size => "base_min_size", view_only => "view_only", quote_max_size => "quote_max_size", status => "status"];
    children: [fcm_trading_session_details: MarketResponseProductsItemFcmTradingSessionDetails => "fcm_trading_session_details", future_product_details: MarketResponseProductsItemFutureProductDetails => "future_product_details"];
    arrays: [];
    optional: []
});

impl_identity!(MarketResponse {
    fields: [num_products => "num_products"];
    children: [];
    arrays: [products: MarketResponseProductsItem => "products"];
    optional: []
});

impl_identity!(CandlesResponseCandlesItem {
    fields: [start => "start", low => "low", high => "high", open => "open", close => "close", volume => "volume"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(CandlesResponse {
    fields: [];
    children: [];
    arrays: [candles: CandlesResponseCandlesItem => "candles"];
    optional: []
});

impl_identity!(ListAccountsResponseItemAvailableBalance {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListAccountsResponseItemHold {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListAccountsResponseItem {
    fields: [uuid => "uuid", name => "name", currency => "currency", r#default => "default", active => "active", created_at => "created_at", updated_at => "updated_at", deleted_at => "deleted_at", r#type => "type", ready => "ready"];
    children: [available_balance: ListAccountsResponseItemAvailableBalance => "available_balance", hold: ListAccountsResponseItemHold => "hold"];
    arrays: [];
    optional: []
});

impl_identity!(ListPortfoliosResponsePortfoliosItem {
    fields: [name => "name", uuid => "uuid", r#type => "type", deleted => "deleted"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListPortfoliosResponse {
    fields: [];
    children: [];
    arrays: [portfolios: ListPortfoliosResponsePortfoliosItem => "portfolios"];
    optional: []
});

impl_identity!(ListPositionsIntxResponsePositionsItemEntryVwap {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListPositionsIntxResponsePositionsItemVwap {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListPositionsIntxResponsePositionsItemUnrealizedPnl {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListPositionsIntxResponsePositionsItemMarkPrice {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListPositionsIntxResponsePositionsItemLiquidationPrice {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListPositionsIntxResponsePositionsItemImNotional {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListPositionsIntxResponsePositionsItemMmNotional {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListPositionsIntxResponsePositionsItemPositionNotional {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListPositionsIntxResponsePositionsItemPortfolioSummaryPortfolioImNotional {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListPositionsIntxResponsePositionsItemPortfolioSummaryPortfolioMmNotional {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListPositionsIntxResponsePositionsItemPortfolioSummaryUnrealizedPnl {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListPositionsIntxResponsePositionsItemPortfolioSummaryBuyingPower {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListPositionsIntxResponsePositionsItemPortfolioSummaryTotalBalance {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListPositionsIntxResponsePositionsItemPortfolioSummaryMaxWithdrawal {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListPositionsIntxResponsePositionsItemPortfolioSummary {
    fields: [portfolio_uuid => "portfolio_uuid", collateral => "collateral", position_notional => "position_notional", open_position_notional => "open_position_notional", pending_fees => "pending_fees", borrow => "borrow", accrued_interest => "accrued_interest", rolling_debt => "rolling_debt", portfolio_initial_margin => "portfolio_initial_margin", portfolio_maintenance_margin => "portfolio_maintenance_margin", liquidation_percentage => "liquidation_percentage", liquidation_buffer => "liquidation_buffer", margin_type => "margin_type", margin_flags => "margin_flags", liquidation_status => "liquidation_status"];
    children: [portfolio_im_notional: ListPositionsIntxResponsePositionsItemPortfolioSummaryPortfolioImNotional => "portfolio_im_notional", portfolio_mm_notional: ListPositionsIntxResponsePositionsItemPortfolioSummaryPortfolioMmNotional => "portfolio_mm_notional", unrealized_pnl: ListPositionsIntxResponsePositionsItemPortfolioSummaryUnrealizedPnl => "unrealized_pnl", buying_power: ListPositionsIntxResponsePositionsItemPortfolioSummaryBuyingPower => "buying_power", total_balance: ListPositionsIntxResponsePositionsItemPortfolioSummaryTotalBalance => "total_balance", max_withdrawal: ListPositionsIntxResponsePositionsItemPortfolioSummaryMaxWithdrawal => "max_withdrawal"];
    arrays: [];
    optional: []
});

impl_identity!(ListPositionsIntxResponsePositionsItem {
    fields: [product_id => "product_id", product_uuid => "product_uuid", symbol => "symbol", position_side => "position_side", net_size => "net_size", buy_order_size => "buy_order_size", sell_order_size => "sell_order_size", im_contribution => "im_contribution", leverage => "leverage", margin_type => "margin_type", liquidation_buffer => "liquidation_buffer", liquidation_percentage => "liquidation_percentage"];
    children: [entry_vwap: ListPositionsIntxResponsePositionsItemEntryVwap => "entry_vwap", vwap: ListPositionsIntxResponsePositionsItemVwap => "vwap", unrealized_pnl: ListPositionsIntxResponsePositionsItemUnrealizedPnl => "unrealized_pnl", mark_price: ListPositionsIntxResponsePositionsItemMarkPrice => "mark_price", liquidation_price: ListPositionsIntxResponsePositionsItemLiquidationPrice => "liquidation_price", im_notional: ListPositionsIntxResponsePositionsItemImNotional => "im_notional", mm_notional: ListPositionsIntxResponsePositionsItemMmNotional => "mm_notional", position_notional: ListPositionsIntxResponsePositionsItemPositionNotional => "position_notional", portfolio_summary: ListPositionsIntxResponsePositionsItemPortfolioSummary => "portfolio_summary"];
    arrays: [];
    optional: [];
    enum_fields: [position_side: ListPositionsIntxResponsePositionsItemPositionSide => "position_side"]
});

impl_identity!(ListPositionsIntxResponse {
    fields: [];
    children: [];
    arrays: [positions: ListPositionsIntxResponsePositionsItem => "positions"];
    optional: []
});

impl_identity!(ListPositionsCfmResponsePositionsItem {
    fields: [product_id => "product_id", expiration_time => "expiration_time", side => "side", number_of_contracts => "number_of_contracts", current_price => "current_price", avg_entry_price => "avg_entry_price", unrealized_pnl => "unrealized_pnl", daily_realized_pnl => "daily_realized_pnl"];
    children: [];
    arrays: [];
    optional: [];
    enum_fields: [side: ListPositionsCfmResponsePositionsItemSide => "side"]
});

impl_identity!(ListPositionsCfmResponse {
    fields: [];
    children: [];
    arrays: [positions: ListPositionsCfmResponsePositionsItem => "positions"];
    optional: []
});

impl_identity!(CandleEventCandlesItem {
    fields: [start => "start", high => "high", low => "low", open => "open", close => "close", volume => "volume", product_id => "product_id"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(CandleEvent {
    fields: [r#type => "type"];
    children: [];
    arrays: [candles: CandleEventCandlesItem => "candles"];
    optional: []
});

impl_identity!(TradeEventTradesItem {
    fields: [product_id => "product_id", trade_id => "trade_id", price => "price", size => "size", time => "time", side => "side"];
    children: [];
    arrays: [];
    optional: [];
    enum_fields: [side: TradeEventTradesItemSide => "side"]
});

impl_identity!(TradeEvent {
    fields: [r#type => "type"];
    children: [];
    arrays: [trades: TradeEventTradesItem => "trades"];
    optional: []
});

impl_identity!(TickerEventTickersItem {
    fields: [r#type => "type", product_id => "product_id", price => "price", volume_24_h => "volume_24_h", low_24_h => "low_24_h", high_24_h => "high_24_h", low_52_w => "low_52_w", high_52_w => "high_52_w", price_percent_chg_24_h => "price_percent_chg_24_h"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(TickerEvent {
    fields: [r#type => "type"];
    children: [];
    arrays: [tickers: TickerEventTickersItem => "tickers"];
    optional: []
});

impl_identity!(Level2EventUpdatesItem {
    fields: [side => "side", event_time => "event_time", price_level => "price_level", new_quantity => "new_quantity"];
    children: [];
    arrays: [];
    optional: [];
    enum_fields: [side: Level2EventUpdatesItemSide => "side"]
});

impl_identity!(Level2Event {
    fields: [r#type => "type", product_id => "product_id"];
    children: [];
    arrays: [updates: Level2EventUpdatesItem => "updates"];
    optional: []
});

impl_identity!(IntxInstrumentsResponseItemQuote {
    fields: [best_bid_price => "best_bid_price", best_bid_size => "best_bid_size", best_ask_price => "best_ask_price", best_ask_size => "best_ask_size", trade_price => "trade_price", trade_qty => "trade_qty", index_price => "index_price", mark_price => "mark_price", settlement_price => "settlement_price", limit_up => "limit_up", limit_down => "limit_down", predicted_funding => "predicted_funding", timestamp => "timestamp"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(IntxInstrumentsResponseItem {
    fields: [instrument_id => "instrument_id", instrument_uuid => "instrument_uuid", symbol => "symbol", r#type => "type", base_asset_id => "base_asset_id", base_asset_uuid => "base_asset_uuid", base_asset_name => "base_asset_name", quote_asset_id => "quote_asset_id", quote_asset_uuid => "quote_asset_uuid", quote_asset_name => "quote_asset_name", base_increment => "base_increment", quote_increment => "quote_increment", price_band_percent => "price_band_percent", market_order_percent => "market_order_percent", qty_24hr => "qty_24hr", notional_24hr => "notional_24hr", avg_daily_qty => "avg_daily_qty", avg_daily_notional => "avg_daily_notional", previous_day_qty => "previous_day_qty", open_interest => "open_interest", position_limit_qty => "position_limit_qty", position_limit_adq_pct => "position_limit_adq_pct", replacement_cost => "replacement_cost", base_imf => "base_imf", min_notional_value => "min_notional_value", funding_interval => "funding_interval", trading_state => "trading_state"];
    children: [quote: IntxInstrumentsResponseItemQuote => "quote"];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseSummaryPortfolioImNotional {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseSummaryPortfolioMmNotional {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseSummaryUnrealizedPnl {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseSummaryBuyingPower {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseSummaryTotalBalance {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseSummaryMaxWithdrawal {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseSummary {
    fields: [portfolio_uuid => "portfolio_uuid", collateral => "collateral", position_notional => "position_notional", open_position_notional => "open_position_notional", pending_fees => "pending_fees", borrow => "borrow", accrued_interest => "accrued_interest", rolling_debt => "rolling_debt", portfolio_initial_margin => "portfolio_initial_margin", portfolio_maintenance_margin => "portfolio_maintenance_margin", liquidation_percentage => "liquidation_percentage", liquidation_buffer => "liquidation_buffer", margin_type => "margin_type", margin_flags => "margin_flags", liquidation_status => "liquidation_status"];
    children: [portfolio_im_notional: BalanceResponseSummaryPortfolioImNotional => "portfolio_im_notional", portfolio_mm_notional: BalanceResponseSummaryPortfolioMmNotional => "portfolio_mm_notional", unrealized_pnl: BalanceResponseSummaryUnrealizedPnl => "unrealized_pnl", buying_power: BalanceResponseSummaryBuyingPower => "buying_power", total_balance: BalanceResponseSummaryTotalBalance => "total_balance", max_withdrawal: BalanceResponseSummaryMaxWithdrawal => "max_withdrawal"];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponse {
    fields: [];
    children: [summary: BalanceResponseSummary => "summary"];
    arrays: [];
    optional: []
});

impl_identity!(OrderEventOrdersItem {
    fields: [order_id => "order_id", client_order_id => "client_order_id", cumulative_quantity => "cumulative_quantity", leaves_quantity => "leaves_quantity", avg_price => "avg_price", limit_price => "limit_price", stop_price => "stop_price", total_fees => "total_fees", status => "status", product_id => "product_id", creation_time => "creation_time", order_side => "order_side", order_type => "order_type", cancel_reason => "cancel_reason", reject_reason => "reject_reason"];
    children: [];
    arrays: [];
    optional: [];
    enum_fields: [status: OrderEventOrdersItemStatus => "status", order_side: OrderEventOrdersItemOrderSide => "order_side", order_type: OrderEventOrdersItemOrderType => "order_type"]
});

impl_identity!(OrderEvent {
    fields: [r#type => "type"];
    children: [];
    arrays: [orders: OrderEventOrdersItem => "orders"];
    optional: []
});

impl_identity!(ListOrdersResponseOrdersItemOrderConfigurationMarketMarketIoc {
    fields: [base_size => "base_size", quote_size => "quote_size"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListOrdersResponseOrdersItemOrderConfigurationMarketMarketFok {
    fields: [base_size => "base_size", quote_size => "quote_size"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListOrdersResponseOrdersItemOrderConfigurationSorLimitIoc {
    fields: [base_size => "base_size", quote_size => "quote_size", limit_price => "limit_price"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListOrdersResponseOrdersItemOrderConfigurationLimitLimitGtc {
    fields: [base_size => "base_size", quote_size => "quote_size", limit_price => "limit_price", post_only => "post_only"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListOrdersResponseOrdersItemOrderConfigurationLimitLimitGtd {
    fields: [base_size => "base_size", quote_size => "quote_size", limit_price => "limit_price"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListOrdersResponseOrdersItemOrderConfigurationLimitLimitFok {
    fields: [base_size => "base_size", quote_size => "quote_size", limit_price => "limit_price"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListOrdersResponseOrdersItemOrderConfigurationStopLimitStopLimitGtc {
    fields: [base_size => "base_size", limit_price => "limit_price", stop_price => "stop_price"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListOrdersResponseOrdersItemOrderConfigurationStopLimitStopLimitGtd {
    fields: [base_size => "base_size", limit_price => "limit_price", stop_price => "stop_price"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(ListOrdersResponseOrdersItemOrderConfiguration {
    fields: [];
    children: [];
    arrays: [];
    optional: [stop_limit_stop_limit_gtc: ListOrdersResponseOrdersItemOrderConfigurationStopLimitStopLimitGtc => "stop_limit_stop_limit_gtc", stop_limit_stop_limit_gtd: ListOrdersResponseOrdersItemOrderConfigurationStopLimitStopLimitGtd => "stop_limit_stop_limit_gtd", market_market_ioc: ListOrdersResponseOrdersItemOrderConfigurationMarketMarketIoc => "market_market_ioc", market_market_fok: ListOrdersResponseOrdersItemOrderConfigurationMarketMarketFok => "market_market_fok", sor_limit_ioc: ListOrdersResponseOrdersItemOrderConfigurationSorLimitIoc => "sor_limit_ioc", limit_limit_gtc: ListOrdersResponseOrdersItemOrderConfigurationLimitLimitGtc => "limit_limit_gtc", limit_limit_gtd: ListOrdersResponseOrdersItemOrderConfigurationLimitLimitGtd => "limit_limit_gtd", limit_limit_fok: ListOrdersResponseOrdersItemOrderConfigurationLimitLimitFok => "limit_limit_fok"]
});

impl_identity!(ListOrdersResponseOrdersItem {
    fields: [order_id => "order_id", client_order_id => "client_order_id", product_id => "product_id", side => "side", status => "status", order_type => "order_type", created_time => "created_time", filled_size => "filled_size", average_filled_price => "average_filled_price", total_fees => "total_fees"];
    children: [];
    arrays: [];
    optional: [order_configuration: ListOrdersResponseOrdersItemOrderConfiguration => "order_configuration"];
    enum_fields: [side: ListOrdersResponseOrdersItemSide => "side", status: ListOrdersResponseOrdersItemStatus => "status", order_type: ListOrdersResponseOrdersItemOrderType => "order_type"]
});

impl_identity!(ListOrdersResponse {
    fields: [has_next => "has_next", cursor => "cursor"];
    children: [];
    arrays: [orders: ListOrdersResponseOrdersItem => "orders"];
    optional: []
});

impl_identity!(OrderRequestConfigMarketMarketIoc {
    fields: [quote_size => "quote_size", base_size => "base_size"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(OrderRequestConfigSorLimitIoc {
    fields: [base_size => "base_size", limit_price => "limit_price"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(OrderRequestConfigLimitLimitGtc {
    fields: [base_size => "base_size", limit_price => "limit_price", post_only => "post_only"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(OrderRequestConfigLimitLimitGtd {
    fields: [base_size => "base_size", limit_price => "limit_price", end_time => "end_time", post_only => "post_only"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(OrderRequestConfigStopLimitStopLimitGtc {
    fields: [base_size => "base_size", limit_price => "limit_price", stop_price => "stop_price", stop_direction => "stop_direction"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(OrderRequestConfigStopLimitStopLimitGtd {
    fields: [base_size => "base_size", limit_price => "limit_price", stop_price => "stop_price", end_time => "end_time", stop_direction => "stop_direction"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(OrderRequestConfigTriggerBracketGtc {
    fields: [base_size => "base_size", limit_price => "limit_price", stop_trigger_price => "stop_trigger_price"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(OrderRequestConfigTriggerBracketGtd {
    fields: [base_size => "base_size", limit_price => "limit_price", stop_trigger_price => "stop_trigger_price", end_time => "end_time"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(OrderRequestConfig {
    fields: [];
    children: [];
    arrays: [];
    optional: [market_market_ioc: OrderRequestConfigMarketMarketIoc => "market_market_ioc", sor_limit_ioc: OrderRequestConfigSorLimitIoc => "sor_limit_ioc", limit_limit_gtc: OrderRequestConfigLimitLimitGtc => "limit_limit_gtc", limit_limit_gtd: OrderRequestConfigLimitLimitGtd => "limit_limit_gtd", stop_limit_stop_limit_gtc: OrderRequestConfigStopLimitStopLimitGtc => "stop_limit_stop_limit_gtc", stop_limit_stop_limit_gtd: OrderRequestConfigStopLimitStopLimitGtd => "stop_limit_stop_limit_gtd", trigger_bracket_gtc: OrderRequestConfigTriggerBracketGtc => "trigger_bracket_gtc", trigger_bracket_gtd: OrderRequestConfigTriggerBracketGtd => "trigger_bracket_gtd"]
});

impl_identity!(PlaceOrderRequest {
    fields: [client_order_id => "client_order_id", product_id => "product_id", side => "side", self_trade_prevention_id => "self_trade_prevention_id", leverage => "leverage", margin_type => "margin_type", retail_portfolio_id => "retail_portfolio_id"];
    children: [order_configuration: OrderRequestConfig => "order_configuration"];
    arrays: [];
    optional: [];
    enum_fields: [side: PlaceOrderRequestSide => "side", margin_type: Option<MarginType> => "margin_type"]
});

impl_identity!(PlaceOrderResponseSuccessResponse {
    fields: [order_id => "order_id"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(PlaceOrderResponseErrorResponse {
    fields: [error => "error", message => "message", error_details => "error_details", preview_failure_reason => "preview_failure_reason", new_order_failure_reason => "new_order_failure_reason"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(PlaceOrderResponse {
    fields: [success => "success", failure_reason => "failure_reason", order_id => "order_id"];
    children: [];
    arrays: [];
    optional: [success_response: PlaceOrderResponseSuccessResponse => "success_response", error_response: PlaceOrderResponseErrorResponse => "error_response", order_configuration: OrderRequestConfig => "order_configuration"]
});

impl_identity!(EditOrderRequest {
    fields: [order_id => "order_id", price => "price", size => "size"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(EditOrderResponseErrorsItem {
    fields: [edit_failure_reason => "edit_failure_reason", preview_failure_reason => "preview_failure_reason"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(EditOrderResponse {
    fields: [success => "success", errors => "errors"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseCfmBalanceSummaryFuturesBuyingPower {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseCfmBalanceSummaryTotalUsdBalance {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseCfmBalanceSummaryCbiUsdBalance {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseCfmBalanceSummaryCfmUsdBalance {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseCfmBalanceSummaryTotalOpenOrdersHoldAmount {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseCfmBalanceSummaryUnrealizedPnl {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseCfmBalanceSummaryDailyRealizedPnl {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseCfmBalanceSummaryInitialMargin {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseCfmBalanceSummaryAvailableMargin {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseCfmBalanceSummaryLiquidationThreshold {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseCfmBalanceSummaryLiquidationBufferAmount {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseCfmBalanceSummaryIntradayMarginWindowMeasure {
    fields: [margin_window_type => "margin_window_type", margin_level => "margin_level", initial_margin => "initial_margin", maintenance_margin => "maintenance_margin", liquidation_buffer => "liquidation_buffer", total_hold => "total_hold", futures_buying_power => "futures_buying_power"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseCfmBalanceSummaryOvernightMarginWindowMeasure {
    fields: [margin_window_type => "margin_window_type", margin_level => "margin_level", initial_margin => "initial_margin", maintenance_margin => "maintenance_margin", liquidation_buffer => "liquidation_buffer", total_hold => "total_hold", futures_buying_power => "futures_buying_power"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseCfmBalanceSummary {
    fields: [liquidation_buffer_percentage => "liquidation_buffer_percentage"];
    children: [overnight_margin_window_measure: BalanceResponseCfmBalanceSummaryOvernightMarginWindowMeasure => "overnight_margin_window_measure", futures_buying_power: BalanceResponseCfmBalanceSummaryFuturesBuyingPower => "futures_buying_power", total_usd_balance: BalanceResponseCfmBalanceSummaryTotalUsdBalance => "total_usd_balance", cbi_usd_balance: BalanceResponseCfmBalanceSummaryCbiUsdBalance => "cbi_usd_balance", cfm_usd_balance: BalanceResponseCfmBalanceSummaryCfmUsdBalance => "cfm_usd_balance", total_open_orders_hold_amount: BalanceResponseCfmBalanceSummaryTotalOpenOrdersHoldAmount => "total_open_orders_hold_amount", unrealized_pnl: BalanceResponseCfmBalanceSummaryUnrealizedPnl => "unrealized_pnl", daily_realized_pnl: BalanceResponseCfmBalanceSummaryDailyRealizedPnl => "daily_realized_pnl", initial_margin: BalanceResponseCfmBalanceSummaryInitialMargin => "initial_margin", available_margin: BalanceResponseCfmBalanceSummaryAvailableMargin => "available_margin", liquidation_threshold: BalanceResponseCfmBalanceSummaryLiquidationThreshold => "liquidation_threshold", liquidation_buffer_amount: BalanceResponseCfmBalanceSummaryLiquidationBufferAmount => "liquidation_buffer_amount", intraday_margin_window_measure: BalanceResponseCfmBalanceSummaryIntradayMarginWindowMeasure => "intraday_margin_window_measure"];
    arrays: [];
    optional: []
});

impl_identity!(BalanceResponseCfm {
    fields: [];
    children: [balance_summary: BalanceResponseCfmBalanceSummary => "balance_summary"];
    arrays: [];
    optional: []
});

impl_identity!(PortfolioBreakdownResponseBreakdownPortfolio {
    fields: [name => "name", uuid => "uuid", r#type => "type", deleted => "deleted"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalBalance {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalFuturesBalance {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCashEquivalentBalance {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCryptoBalance {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(PortfolioBreakdownResponseBreakdownPortfolioBalancesFuturesUnrealizedPnl {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(PortfolioBreakdownResponseBreakdownPortfolioBalancesPerpUnrealizedPnl {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(PortfolioBreakdownResponseBreakdownPortfolioBalances {
    fields: [];
    children: [total_futures_balance: PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalFuturesBalance => "total_futures_balance", total_cash_equivalent_balance: PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCashEquivalentBalance => "total_cash_equivalent_balance", total_crypto_balance: PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalCryptoBalance => "total_crypto_balance", futures_unrealized_pnl: PortfolioBreakdownResponseBreakdownPortfolioBalancesFuturesUnrealizedPnl => "futures_unrealized_pnl", total_balance: PortfolioBreakdownResponseBreakdownPortfolioBalancesTotalBalance => "total_balance", perp_unrealized_pnl: PortfolioBreakdownResponseBreakdownPortfolioBalancesPerpUnrealizedPnl => "perp_unrealized_pnl"];
    arrays: [];
    optional: []
});

impl_identity!(PortfolioBreakdownResponseBreakdownSpotPositionsItemCostBasis {
    fields: [value => "value", currency => "currency"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(PortfolioBreakdownResponseBreakdownSpotPositionsItem {
    fields: [asset => "asset", account_uuid => "account_uuid", total_balance_fiat => "total_balance_fiat", total_balance_crypto => "total_balance_crypto", available_to_trade_fiat => "available_to_trade_fiat", allocation => "allocation", asset_img_url => "asset_img_url", is_cash => "is_cash", average_entry_price => "average_entry_price", asset_uuid => "asset_uuid", available_to_trade_crypto => "available_to_trade_crypto", unrealized_pnl => "unrealized_pnl", available_to_transfer_fiat => "available_to_transfer_fiat", available_to_transfer_crypto => "available_to_transfer_crypto", asset_color => "asset_color"];
    children: [cost_basis: PortfolioBreakdownResponseBreakdownSpotPositionsItemCostBasis => "cost_basis"];
    arrays: [];
    optional: []
});

impl_identity!(PortfolioBreakdownResponseBreakdownPerpPositionsItem {
    fields: [product_id => "product_id", contract_size => "contract_size", side => "side", amount => "amount", avg_entry_price => "avg_entry_price", current_price => "current_price", unrealized_pnl => "unrealized_pnl", expiry => "expiry", underlying_asset => "underlying_asset", asset_img_url => "asset_img_url", product_name => "product_name", venue => "venue", notional_value => "notional_value", asset_color => "asset_color"];
    children: [];
    arrays: [];
    optional: [];
    enum_fields: [side: PortfolioBreakdownResponseBreakdownPerpPositionsItemSide => "side"]
});

impl_identity!(PortfolioBreakdownResponseBreakdownFuturesPositionsItem {
    fields: [product_id => "product_id", contract_size => "contract_size", side => "side", amount => "amount", avg_entry_price => "avg_entry_price", current_price => "current_price", unrealized_pnl => "unrealized_pnl", expiry => "expiry", underlying_asset => "underlying_asset", asset_img_url => "asset_img_url", product_name => "product_name", venue => "venue", notional_value => "notional_value", asset_color => "asset_color"];
    children: [];
    arrays: [];
    optional: [];
    enum_fields: [side: PortfolioBreakdownResponseBreakdownFuturesPositionsItemSide => "side"]
});

impl_identity!(PortfolioBreakdownResponseBreakdown {
    fields: [];
    children: [portfolio: PortfolioBreakdownResponseBreakdownPortfolio => "portfolio", portfolio_balances: PortfolioBreakdownResponseBreakdownPortfolioBalances => "portfolio_balances"];
    arrays: [spot_positions: PortfolioBreakdownResponseBreakdownSpotPositionsItem => "spot_positions", perp_positions: PortfolioBreakdownResponseBreakdownPerpPositionsItem => "perp_positions", futures_positions: PortfolioBreakdownResponseBreakdownFuturesPositionsItem => "futures_positions"];
    optional: []
});

impl_identity!(PortfolioBreakdownResponse {
    fields: [];
    children: [breakdown: PortfolioBreakdownResponseBreakdown => "breakdown"];
    arrays: [];
    optional: []
});

impl NativeWireIdentity for StopDirection {
    fn assert_native_wire_identity(&self, raw: &Value, path: &str) {
        let expected = match self {
            StopDirection::StopDirectionStopDown => "STOP_DIRECTION_STOP_DOWN",
            StopDirection::StopDirectionStopUp => "STOP_DIRECTION_STOP_UP",
        };
        assert_eq!(raw.as_str(), Some(expected), "{path} decoded enum identity");
    }
}

macro_rules! impl_literal_enum_identity {
    ($type:ty { $($variant:path => $literal:literal),+ $(,)? }) => {
        impl NativeWireIdentity for $type {
            fn assert_native_wire_identity(&self, raw: &Value, path: &str) {
                let actual = match self { $($variant => $literal),+ };
                assert_eq!(raw.as_str(), Some(actual), "{path} decoded native enum variant");
            }
        }
    };
}

impl_literal_enum_identity!(ListPositionsIntxResponsePositionsItemPositionSide {
    ListPositionsIntxResponsePositionsItemPositionSide::PositionSideLong => "POSITION_SIDE_LONG",
    ListPositionsIntxResponsePositionsItemPositionSide::PositionSideShort => "POSITION_SIDE_SHORT",
});
impl_literal_enum_identity!(ListPositionsCfmResponsePositionsItemSide {
    ListPositionsCfmResponsePositionsItemSide::Long => "LONG",
    ListPositionsCfmResponsePositionsItemSide::Short => "SHORT",
});
impl_literal_enum_identity!(TradeEventTradesItemSide {
    TradeEventTradesItemSide::Buy => "BUY",
    TradeEventTradesItemSide::Sell => "SELL",
});
impl_literal_enum_identity!(Level2EventUpdatesItemSide {
    Level2EventUpdatesItemSide::Bid => "bid",
    Level2EventUpdatesItemSide::Offer => "offer",
});
impl_literal_enum_identity!(OrderEventOrdersItemStatus {
    OrderEventOrdersItemStatus::Pending => "PENDING",
    OrderEventOrdersItemStatus::Open => "OPEN",
    OrderEventOrdersItemStatus::Filled => "FILLED",
    OrderEventOrdersItemStatus::Cancelled => "CANCELLED",
    OrderEventOrdersItemStatus::Expired => "EXPIRED",
    OrderEventOrdersItemStatus::Failed => "FAILED",
});
impl_literal_enum_identity!(OrderEventOrdersItemOrderSide {
    OrderEventOrdersItemOrderSide::Buy => "BUY",
    OrderEventOrdersItemOrderSide::Sell => "SELL",
});
impl_literal_enum_identity!(OrderEventOrdersItemOrderType {
    OrderEventOrdersItemOrderType::Limit => "Limit",
    OrderEventOrdersItemOrderType::Market => "Market",
    OrderEventOrdersItemOrderType::StopLimit => "Stop Limit",
});
impl_literal_enum_identity!(ListOrdersResponseOrdersItemSide {
    ListOrdersResponseOrdersItemSide::Buy => "BUY",
    ListOrdersResponseOrdersItemSide::Sell => "SELL",
});
impl_literal_enum_identity!(ListOrdersResponseOrdersItemStatus {
    ListOrdersResponseOrdersItemStatus::Pending => "PENDING",
    ListOrdersResponseOrdersItemStatus::Open => "OPEN",
    ListOrdersResponseOrdersItemStatus::Filled => "FILLED",
    ListOrdersResponseOrdersItemStatus::Cancelled => "CANCELLED",
    ListOrdersResponseOrdersItemStatus::Expired => "EXPIRED",
    ListOrdersResponseOrdersItemStatus::Failed => "FAILED",
    ListOrdersResponseOrdersItemStatus::UnknownOrderStatus => "UNKNOWN_ORDER_STATUS",
    ListOrdersResponseOrdersItemStatus::Queued => "QUEUED",
    ListOrdersResponseOrdersItemStatus::CancelQueued => "CANCEL_QUEUED",
    ListOrdersResponseOrdersItemStatus::EditQueued => "EDIT_QUEUED",
});
impl_literal_enum_identity!(ListOrdersResponseOrdersItemOrderType {
    ListOrdersResponseOrdersItemOrderType::UnknownOrderType => "UNKNOWN_ORDER_TYPE",
    ListOrdersResponseOrdersItemOrderType::Market => "MARKET",
    ListOrdersResponseOrdersItemOrderType::Limit => "LIMIT",
    ListOrdersResponseOrdersItemOrderType::Stop => "STOP",
    ListOrdersResponseOrdersItemOrderType::StopLimit => "STOP_LIMIT",
    ListOrdersResponseOrdersItemOrderType::Bracket => "BRACKET",
    ListOrdersResponseOrdersItemOrderType::Twap => "TWAP",
    ListOrdersResponseOrdersItemOrderType::RollOpen => "ROLL_OPEN",
    ListOrdersResponseOrdersItemOrderType::RollClose => "ROLL_CLOSE",
    ListOrdersResponseOrdersItemOrderType::Liquidation => "LIQUIDATION",
    ListOrdersResponseOrdersItemOrderType::Scaled => "SCALED",
});
impl_literal_enum_identity!(PlaceOrderRequestSide {
    PlaceOrderRequestSide::Buy => "BUY",
    PlaceOrderRequestSide::Sell => "SELL",
});
impl_literal_enum_identity!(MarginType {
    MarginType::Isolated => "ISOLATED",
    MarginType::Cross => "CROSS",
});
impl_literal_enum_identity!(PortfolioBreakdownResponseBreakdownPerpPositionsItemSide {
    PortfolioBreakdownResponseBreakdownPerpPositionsItemSide::FuturesPositionSideLong => "FUTURES_POSITION_SIDE_LONG",
    PortfolioBreakdownResponseBreakdownPerpPositionsItemSide::FuturesPositionSideShort => "FUTURES_POSITION_SIDE_SHORT",
});
impl_literal_enum_identity!(PortfolioBreakdownResponseBreakdownFuturesPositionsItemSide {
    PortfolioBreakdownResponseBreakdownFuturesPositionsItemSide::FuturesPositionSideLong => "FUTURES_POSITION_SIDE_LONG",
    PortfolioBreakdownResponseBreakdownFuturesPositionsItemSide::FuturesPositionSideShort => "FUTURES_POSITION_SIDE_SHORT",
});

// Frozen native variant-to-TypeScript-literal pins; never derive these via Serialize.
impl_literal_enum_identity!(CandleEventType {
    CandleEventType::Snapshot => "snapshot",
    CandleEventType::Update => "update",
});
impl_literal_enum_identity!(TradeEventType {
    TradeEventType::Snapshot => "snapshot",
    TradeEventType::Update => "update",
});
impl_literal_enum_identity!(TickerEventType {
    TickerEventType::Snapshot => "snapshot",
    TickerEventType::Update => "update",
});
impl_literal_enum_identity!(TickerEventTickersItemType {
    TickerEventTickersItemType::Ticker => "ticker",
});
impl_literal_enum_identity!(Level2EventType {
    Level2EventType::Snapshot => "snapshot",
    Level2EventType::Update => "update",
});
impl_literal_enum_identity!(OrderEventType {
    OrderEventType::Snapshot => "snapshot",
    OrderEventType::Update => "update",
});
impl_literal_enum_identity!(CoinbaseStarbaseImplementationStatus {
    CoinbaseStarbaseImplementationStatus::NotImplemented => "not-implemented",
    CoinbaseStarbaseImplementationStatus::Implemented => "implemented",
});
impl_literal_enum_identity!(CoinbaseStarbaseVerificationStatus {
    CoinbaseStarbaseVerificationStatus::Unverified => "unverified",
    CoinbaseStarbaseVerificationStatus::Verified => "verified",
});
impl_literal_enum_identity!(CoinbaseStarbaseHttpShadowBlocker {
    CoinbaseStarbaseHttpShadowBlocker::CredentialedHttpAuthenticationUnverified => "credentialed-http-authentication-unverified",
    CoinbaseStarbaseHttpShadowBlocker::HttpAuthenticationContractNotImplemented => "http-authentication-contract-not-implemented",
    CoinbaseStarbaseHttpShadowBlocker::PrivateHttpResponseContractsNotImplemented => "private-http-response-contracts-not-implemented",
    CoinbaseStarbaseHttpShadowBlocker::ProductOutsideIntxMigration => "product-outside-intx-migration",
    CoinbaseStarbaseHttpShadowBlocker::PublicHttpResponseContractsNotImplemented => "public-http-response-contracts-not-implemented",
});
impl_literal_enum_identity!(CoinbaseStarbaseWebsocketBlocker {
    CoinbaseStarbaseWebsocketBlocker::CredentialedWebsocketSessionLifecycleUnverified => "credentialed-websocket-session-lifecycle-unverified",
    CoinbaseStarbaseWebsocketBlocker::PrivateNotificationEnvelopeUnverified => "private-notification-envelope-unverified",
    CoinbaseStarbaseWebsocketBlocker::WebsocketActivationDisabled => "websocket-activation-disabled",
    CoinbaseStarbaseWebsocketBlocker::WebsocketSessionContractNotImplemented => "websocket-session-contract-not-implemented",
});
impl_literal_enum_identity!(CoinbaseStarbaseCutoverBlocker {
    CoinbaseStarbaseCutoverBlocker::CredentialedHttpAuthenticationUnverified => "credentialed-http-authentication-unverified",
    CoinbaseStarbaseCutoverBlocker::HttpAuthenticationContractNotImplemented => "http-authentication-contract-not-implemented",
    CoinbaseStarbaseCutoverBlocker::PrivateHttpResponseContractsNotImplemented => "private-http-response-contracts-not-implemented",
    CoinbaseStarbaseCutoverBlocker::ProductOutsideIntxMigration => "product-outside-intx-migration",
    CoinbaseStarbaseCutoverBlocker::PublicHttpResponseContractsNotImplemented => "public-http-response-contracts-not-implemented",
    CoinbaseStarbaseCutoverBlocker::CredentialedWebsocketSessionLifecycleUnverified => "credentialed-websocket-session-lifecycle-unverified",
    CoinbaseStarbaseCutoverBlocker::PrivateNotificationEnvelopeUnverified => "private-notification-envelope-unverified",
    CoinbaseStarbaseCutoverBlocker::WebsocketActivationDisabled => "websocket-activation-disabled",
    CoinbaseStarbaseCutoverBlocker::WebsocketSessionContractNotImplemented => "websocket-session-contract-not-implemented",
    CoinbaseStarbaseCutoverBlocker::CredentialedHttpReadParityUnverified => "credentialed-http-read-parity-unverified",
    CoinbaseStarbaseCutoverBlocker::CredentialedWriteCanaryUnverified => "credentialed-write-canary-unverified",
    CoinbaseStarbaseCutoverBlocker::CutoverRunbookUnverified => "cutover-runbook-unverified",
    CoinbaseStarbaseCutoverBlocker::RuntimeActivationDisabled => "runtime-activation-disabled",
    CoinbaseStarbaseCutoverBlocker::RuntimeProductBindingUnavailable => "runtime-product-binding-unavailable",
    CoinbaseStarbaseCutoverBlocker::UnitParityUnverified => "unit-parity-unverified",
});
impl_literal_enum_identity!(CoinbaseStarbaseTestProductClassification {
    CoinbaseStarbaseTestProductClassification::Spot => "spot",
    CoinbaseStarbaseTestProductClassification::Cfm => "cfm",
    CoinbaseStarbaseTestProductClassification::Intx => "intx",
});
impl_literal_enum_identity!(CoinbaseStarbaseHttpShadowReadinessPlanAuthentication {
    CoinbaseStarbaseHttpShadowReadinessPlanAuthentication::StarbaseHttp => "starbase-http",
});
impl_literal_enum_identity!(CoinbaseStarbaseHttpShadowReadinessPlanPrivateReads {
    CoinbaseStarbaseHttpShadowReadinessPlanPrivateReads::StarbaseHttpJsonRpcResults => "starbase-http-json-rpc-results",
});
impl_literal_enum_identity!(CoinbaseStarbaseHttpShadowReadinessPlanPublicReads {
    CoinbaseStarbaseHttpShadowReadinessPlanPublicReads::StarbaseHttpJsonRpcResults => "starbase-http-json-rpc-results",
});
impl_literal_enum_identity!(CoinbaseStarbaseHttpShadowReadinessPlanTransport {
    CoinbaseStarbaseHttpShadowReadinessPlanTransport::Http => "http",
});
impl_literal_enum_identity!(CoinbaseStarbaseHttpShadowReadinessPlanWrites {
    CoinbaseStarbaseHttpShadowReadinessPlanWrites::Disabled => "disabled",
});
impl_literal_enum_identity!(CoinbaseStarbaseCutoverPlanProductBindingRequired {
    CoinbaseStarbaseCutoverPlanProductBindingRequired::TrustedRuntimeIntxPortfolioBinding => "trusted-runtime-intx-portfolio-binding",
});
impl_literal_enum_identity!(CoinbaseStarbaseCutoverPlanProductBindingStatus {
    CoinbaseStarbaseCutoverPlanProductBindingStatus::Unbound => "unbound",
});
impl_literal_enum_identity!(CoinbaseStarbaseCutoverPlanAuthoritativePrivateReads {
    CoinbaseStarbaseCutoverPlanAuthoritativePrivateReads::LegacyCoinbase => "legacy-coinbase",
});
impl_literal_enum_identity!(CoinbaseStarbaseCutoverPlanAuthoritativePublicReads {
    CoinbaseStarbaseCutoverPlanAuthoritativePublicReads::LegacyCoinbase => "legacy-coinbase",
});
impl_literal_enum_identity!(CoinbaseStarbaseCutoverPlanAuthoritativeWrites {
    CoinbaseStarbaseCutoverPlanAuthoritativeWrites::LegacyCoinbase => "legacy-coinbase",
});
impl NativeWireIdentity for CoinbaseStarbaseFalse {
    fn assert_native_wire_identity(&self, raw: &Value, path: &str) {
        assert!(
            matches!(self, CoinbaseStarbaseFalse::False),
            "{path} native false variant"
        );
        assert_eq!(
            raw.as_bool(),
            Some(false),
            "{path} decoded native boolean variant"
        );
    }
}
impl_literal_enum_identity!(CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem {
    CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem::CredentialedHttpAuthenticationUnverified => "credentialed-http-authentication-unverified",
    CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem::HttpAuthenticationContractNotImplemented => "http-authentication-contract-not-implemented",
    CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem::PrivateHttpResponseContractsNotImplemented => "private-http-response-contracts-not-implemented",
    CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem::ProductOutsideIntxMigration => "product-outside-intx-migration",
    CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem::PublicHttpResponseContractsNotImplemented => "public-http-response-contracts-not-implemented",
    CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem::RuntimeProductBindingUnavailable => "runtime-product-binding-unavailable",
});
impl_literal_enum_identity!(CoinbaseStarbaseCutoverPlanWebsocketPrivateNotifications {
    CoinbaseStarbaseCutoverPlanWebsocketPrivateNotifications::Disabled => "disabled",
});
impl_literal_enum_identity!(CoinbaseStarbaseCutoverPlanWebsocketPublicSubscriptions {
    CoinbaseStarbaseCutoverPlanWebsocketPublicSubscriptions::Disabled => "disabled",
});
impl_literal_enum_identity!(CoinbaseStarbaseCutoverPlanWebsocketSession {
    CoinbaseStarbaseCutoverPlanWebsocketSession::Disabled => "disabled",
});

impl_identity!(CoinbaseStarbaseCutoverEvidence {
    fields: [credentialed_http_authentication => "credentialedHttpAuthentication", credentialed_http_read_parity => "credentialedHttpReadParity", credentialed_websocket_session_lifecycle => "credentialedWebsocketSessionLifecycle", credentialed_write_canary => "credentialedWriteCanary", cutover_runbook => "cutoverRunbook", http_authentication_contract => "httpAuthenticationContract", private_http_response_contracts => "privateHttpResponseContracts", private_notification_envelope => "privateNotificationEnvelope", public_http_response_contracts => "publicHttpResponseContracts", unit_parity => "unitParity", websocket_session_contract => "websocketSessionContract"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(CoinbaseStarbaseHttpShadowReadinessPlan {
    fields: [authentication => "authentication", private_reads => "privateReads", public_reads => "publicReads", transport => "transport", writes => "writes"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(CoinbaseStarbaseHttpShadowReadiness {
    fields: [eligible => "eligible", product_classification => "productClassification", blockers => "blockers"];
    children: [];
    arrays: [];
    optional: [plan: CoinbaseStarbaseHttpShadowReadinessPlan => "plan"]
});

impl_identity!(CoinbaseStarbaseCutoverPlanProductBinding {
    fields: [required => "required", status => "status"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(CoinbaseStarbaseCutoverPlanAuthoritative {
    fields: [private_reads => "privateReads", public_reads => "publicReads", writes => "writes"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(CoinbaseStarbaseCutoverPlanHttpShadow {
    fields: [active => "active", eligible => "eligible", runtime_activation_enabled => "runtimeActivationEnabled", blockers => "blockers"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(CoinbaseStarbaseCutoverPlanWebsocket {
    fields: [active => "active", private_notifications => "privateNotifications", public_subscriptions => "publicSubscriptions", runtime_activation_enabled => "runtimeActivationEnabled", session => "session", blockers => "blockers"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(CoinbaseStarbaseCutoverPlanCutover {
    fields: [eligible => "eligible", runtime_activation_enabled => "runtimeActivationEnabled", blockers => "blockers"];
    children: [];
    arrays: [];
    optional: []
});

impl_identity!(CoinbaseStarbaseCutoverPlan {
    fields: [];
    children: [product_binding: CoinbaseStarbaseCutoverPlanProductBinding => "productBinding", authoritative: CoinbaseStarbaseCutoverPlanAuthoritative => "authoritative", http_shadow: CoinbaseStarbaseCutoverPlanHttpShadow => "httpShadow", websocket: CoinbaseStarbaseCutoverPlanWebsocket => "websocket", cutover: CoinbaseStarbaseCutoverPlanCutover => "cutover"];
    arrays: [];
    optional: []
});

impl<T: NativeWireIdentity> NativeWireIdentity for Vec<T> {
    fn assert_native_wire_identity(&self, raw: &Value, path: &str) {
        let children = raw
            .as_array()
            .unwrap_or_else(|| panic!("{path} is not an array"));
        assert_eq!(self.len(), children.len(), "{path} decoded array length");
        for (index, (value, child)) in self.iter().zip(children).enumerate() {
            value.assert_native_wire_identity(child, &format!("{path}[{index}]"));
        }
    }
}
