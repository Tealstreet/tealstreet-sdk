//! ts: bybit.types.ts
use serde_json::Value;
use std::sync::LazyLock;

pub const RECV_WINDOW: u64 = 20000;
pub const BROKER_ID: &str = "Tealstreet";
pub static BASE_URL: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"livenet": "https://api.bybit.com", "testnet": "https://api-testnet.bybit.com"}),
);
pub static BASE_WS_URL: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"public": {"linear": {"livenet": "wss://stream.bybit.com/v5/public/linear", "testnet": "wss://stream-testnet.bybit.com/v5/public/linear"}, "inverse": {"livenet": "wss://stream.bybit.com/v5/public/inverse", "testnet": "wss://stream-testnet.bybit.com/v5/public/inverse"}}, "private": {"livenet": "wss://stream.bybit.com/v5/private", "testnet": "wss://stream-testnet.bybit.com/v5/private"}}),
);
pub static DEMO_URL: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"livenet": "https://api-demo.bybit.com", "testnet": "https://api-testnet.bybit.com"}),
);
pub static DEMO_WS_URL: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"private": {"livenet": "wss://stream-demo.bybit.com/v5/private", "testnet": "wss://stream-testnet.bybit.com/v5/private"}}),
);
pub static ENDPOINTS: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"ACCOUNT_MARGIN": "/v5/account/info", "ACCOUNT": "/v5/user/query-api", "BALANCE": "/v5/account/wallet-balance", "FEE_RATE": "/v5/account/fee-rate", "UNFILLED_ORDERS": "/v5/order/realtime", "TICKERS": "/v5/market/tickers", "MARKETS": "/v5/market/instruments-info", "OPTION_BASE_COINS": "/v5/market/option-base-coins", "COIN_GREEKS": "/v5/asset/coin-greeks", "CANCEL_ORDER": "/v5/order/cancel", "CANCEL_ORDERS": "/v5/order/cancel-batch", "CANCEL_SYMBOL_ORDERS": "/v5/order/cancel-all", "POSITIONS": "/v5/position/list", "KLINE": "/v5/market/kline", "SET_LEVERAGE": "/v5/position/set-leverage", "SET_TRADING_STOP": "/v5/position/trading-stop", "CREATE_ORDER": "/v5/order/create", "CREATE_BATCH_ORDER": "/v5/order/create-batch", "REPLACE_ORDER": "/v5/order/amend", "SET_POSITION_MODE": "/v5/position/switch-mode", "SET_MARGIN_MODE": "/v5/account/set-margin-mode", "USER_ORDERS": "/v5/order/history", "USER_TRADES": "/v5/execution/list", "USER_CLOSED_PNL": "/v5/position/closed-pnl", "TRANSACTION_LOG": "/v5/account/transaction-log"}),
);
pub static GRAFANA_ENDPOINTS_EVERY: LazyLock<Value> = LazyLock::new(|| {
    serde_json::json!([
        "/v5/order/create",
        "/v5/order/create-batch",
        "/v5/order/amend",
        "/v5/position/switch-mode",
        "/v5/account/set-margin-mode",
        "/v5/position/set-leverage",
        "/v5/order/cancel",
        "/v5/order/cancel-batch",
        "/v5/order/cancel-all",
        "/v5/position/trading-stop"
    ])
});
pub static GRAFANA_ENDPOINTS_PERIODIC: LazyLock<Value> = LazyLock::new(|| {
    serde_json::json!([
        "/v5/position/list",
        "/v5/order/realtime",
        "/v5/account/fee-rate",
        "/v5/user/query-api",
        "/v5/account/info",
        "/v5/account/wallet-balance"
    ])
});
pub static INTERVAL: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"1m": "1", "3m": "3", "5m": "5", "15m": "15", "30m": "30", "1h": "60", "2h": "120", "4h": "240", "6h": "360", "12h": "720", "1d": "D", "1w": "W", "1M": "M"}),
);
pub static ORDER_STATUS: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"Created": "open", "New": "open", "Active": "open", "Untriggered": "open", "PartiallyFilled": "open", "Rejected": "closed", "Filled": "closed", "Deactivated": "closed", "Triggered": "closed", "PendingCancel": "canceled", "Cancelled": "canceled"}),
);
pub static ORDER_TYPE: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"Limit": "limit", "Market": "market", "Stop": "stop_market", "StopLoss": "stop_market", "TakeProfit": "take_profit_market", "PartialTakeProfit": "take_profit_market", "PartialStopLoss": "stop_market", "TrailingStop": "trailing_stop_market"}),
);
pub static POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"CreateByTakeProfit": "TakeProfit", "CreateByPartialTakeProfit": "TakeProfit", "CreateByStopLoss": "StopLoss", "CreateByPartialStopLoss": "StopLoss", "CreateByTrailingStop": "TrailingStop"}),
);
pub static ORDER_SIDE: LazyLock<Value> =
    LazyLock::new(|| serde_json::json!({"Buy": "buy", "Sell": "sell"}));
pub static POSITION_SIDE: LazyLock<Value> =
    LazyLock::new(|| serde_json::json!({"Buy": "long", "Sell": "short"}));
pub static ORDER_TIME_IN_FORCE: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"GTC": "GoodTillCancel", "IOC": "ImmediateOrCancel", "FOK": "FillOrKill", "PostOnly": "PostOnly", "RPI": "PostOnly"}),
);
pub static ORDER_TIME_IN_FORCE_OUTBOUND: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"GoodTillCancel": "GTC", "ImmediateOrCancel": "IOC", "FillOrKill": "FOK", "PostOnly": "PostOnly"}),
);
pub static ORDER_TRIGGER_TYPE: LazyLock<Value> = LazyLock::new(
    || serde_json::json!({"IndexPrice": "index_price", "MarkPrice": "mark_price", "LastPrice": "last_price"}),
);
pub const ORDER_HISTORY_MAX_WINDOW_MS: u64 = 604799999;

use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BybitTransactionType {
    Trade,
    Settlement,
    Delivery,
    Transfer,
    Airdrop,
    Liquidation,
    Adl,
    BlockTrade,
    Bonus,
    FeeRefund,
    Interest,
    CurrencyBuy,
    CurrencySell,
    AutoRepayment,
    AutoBorrow,
    ManualRepayment,
    SpotRepayCollateral,
    CustodyNetwork,
    CustodySettle,
    Custody,
    CustodyAuto,
    CustodyManual,
    PremarketTransferIn,
    PremarketTransferOut,
    PremarketDelivery,
    PremarketRollback,
    NoneC2cTransfer,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BybitTransactionAccountType {
    Unified,
    Contract,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BybitTransactionCategory {
    Linear,
    Inverse,
    Option,
    Spot,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BybitApiKeyPermissions {
    pub contract_trade: Vec<String>,
    pub spot: Vec<String>,
    pub wallet: Vec<String>,
    pub options: Vec<String>,
    pub derivatives: Vec<String>,
    pub copy_trading: Vec<String>,
    pub block_trade: Vec<String>,
    pub exchange: Vec<String>,
    pub n_f_t: Vec<String>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BybitApiKeyInfo {
    pub id: String,
    pub note: String,
    pub api_key: String,
    pub read_only: f64,
    pub secret: String,
    pub permissions: BybitApiKeyPermissions,
    pub ips: Vec<String>,
    pub r#type: f64,
    pub deadline_day: f64,
    pub expired_at: String,
    pub created_at: String,
    pub unified: f64,
    pub uta: f64,
    pub user_i_d: f64,
    pub inviter_i_d: f64,
    pub vip_level: String,
    pub mkt_maker_level: String,
    pub affiliate_i_d: f64,
    pub rsa_public_key: String,
    pub is_master: bool,
    pub parent_uid: String,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BybitTransactionRecord {
    pub symbol: String,
    pub category: String,
    pub side: String,
    pub transaction_time: String,
    pub r#type: BybitTransactionType,
    pub qty: String,
    pub size: String,
    pub currency: String,
    pub trade_price: String,
    pub funding: String,
    pub fee: String,
    pub cash_flow: String,
    pub change: String,
    pub cash_balance: String,
    pub fee_rate: String,
    pub bonus_change: String,
    pub trade_id: String,
    pub order_id: String,
    pub order_link_id: String,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BybitTransactionHistoryOptions {
    pub account_type: Option<BybitTransactionAccountType>,
    pub category: Option<BybitTransactionCategory>,
    pub currency: Option<String>,
    pub base_coin: Option<String>,
    pub r#type: Option<BybitTransactionType>,
    pub start_time: Option<f64>,
    pub end_time: Option<f64>,
    pub limit: Option<f64>,
    pub cursor: Option<String>,
}
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

// ts: normalizeConfiguredOrigin
pub fn normalize_configured_origin(origin: Option<&str>) -> String {
    safe_cex_num::js_trim(origin.unwrap_or(""))
        .trim_end_matches('/')
        .to_owned()
}
// ts: restOriginToWsUrl
pub fn rest_origin_to_ws_url(origin: &str, path: &str) -> String {
    if origin.is_empty() {
        return String::new();
    }
    if origin
        .get(..4)
        .is_some_and(|s| s.eq_ignore_ascii_case("http"))
    {
        format!("ws{}{path}", &origin[4..])
    } else {
        format!("{origin}{path}")
    }
}
pub static HYROTRADER_SIMULATOR_URL: LazyLock<Value> = LazyLock::new(|| {
    let origin =
        normalize_configured_origin(option_env!("NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL"));
    serde_json::json!({"livenet": origin, "testnet": origin})
});
pub static HYROTRADER_SIMULATOR_WS_URL: LazyLock<Value> = LazyLock::new(|| {
    let origin =
        normalize_configured_origin(option_env!("NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL"));
    let public = normalize_configured_origin(option_env!(
        "NEXT_PUBLIC_HYROTRADER_SIMULATOR_PUBLIC_WS_URL"
    ));
    let private = normalize_configured_origin(option_env!(
        "NEXT_PUBLIC_HYROTRADER_SIMULATOR_PRIVATE_WS_URL"
    ));
    let public = if public.is_empty() {
        rest_origin_to_ws_url(&origin, "/v5/public/linear")
    } else {
        public
    };
    let private = if private.is_empty() {
        rest_origin_to_ws_url(&origin, "/v5/private")
    } else {
        private
    };
    serde_json::json!({"public":{"linear":{"livenet":public,"testnet":public},"inverse":{"livenet":"","testnet":""}},"private":{"livenet":private,"testnet":private}})
});

pub static HYROTRADER_PROD_URL: LazyLock<Value> = LazyLock::new(|| {
    let origin = normalize_configured_origin(option_env!("NEXT_PUBLIC_HYROTRADER_PROD_REST_URL"));
    serde_json::json!({"livenet": origin, "testnet": origin})
});
pub static HYROTRADER_PROD_WS_URL: LazyLock<Value> = LazyLock::new(|| {
    let origin = normalize_configured_origin(option_env!("NEXT_PUBLIC_HYROTRADER_PROD_REST_URL"));
    let public =
        normalize_configured_origin(option_env!("NEXT_PUBLIC_HYROTRADER_PROD_PUBLIC_WS_URL"));
    let private =
        normalize_configured_origin(option_env!("NEXT_PUBLIC_HYROTRADER_PROD_PRIVATE_WS_URL"));
    let public = if public.is_empty() {
        rest_origin_to_ws_url(&origin, "/v5/public/linear")
    } else {
        public
    };
    let private = if private.is_empty() {
        rest_origin_to_ws_url(&origin, "/v5/private")
    } else {
        private
    };
    serde_json::json!({"public":{"linear":{"livenet":public,"testnet":public},"inverse":{"livenet":"","testnet":""}},"private":{"livenet":private,"testnet":private}})
});

// Private derived decoder supplies canonical serde spelling metadata. Only the
// string-required public Deserialize implementation can construct the public type.
#[derive(Serialize, Deserialize)]
enum BybitTransactionTypeWire {
    #[serde(rename = "TRADE")]
    Trade,
    #[serde(rename = "SETTLEMENT")]
    Settlement,
    #[serde(rename = "DELIVERY")]
    Delivery,
    #[serde(rename = "TRANSFER")]
    Transfer,
    #[serde(rename = "AIRDROP")]
    Airdrop,
    #[serde(rename = "LIQUIDATION")]
    Liquidation,
    #[serde(rename = "ADL")]
    Adl,
    #[serde(rename = "BLOCK_TRADE")]
    BlockTrade,
    #[serde(rename = "BONUS")]
    Bonus,
    #[serde(rename = "FEE_REFUND")]
    FeeRefund,
    #[serde(rename = "INTEREST")]
    Interest,
    #[serde(rename = "CURRENCY_BUY")]
    CurrencyBuy,
    #[serde(rename = "CURRENCY_SELL")]
    CurrencySell,
    #[serde(rename = "AUTO_REPAYMENT")]
    AutoRepayment,
    #[serde(rename = "AUTO_BORROW")]
    AutoBorrow,
    #[serde(rename = "MANUAL_REPAYMENT")]
    ManualRepayment,
    #[serde(rename = "SPOT_REPAY_COLLATERAL")]
    SpotRepayCollateral,
    #[serde(rename = "CUSTODY_NETWORK")]
    CustodyNetwork,
    #[serde(rename = "CUSTODY_SETTLE")]
    CustodySettle,
    #[serde(rename = "CUSTODY")]
    Custody,
    #[serde(rename = "CUSTODY_AUTO")]
    CustodyAuto,
    #[serde(rename = "CUSTODY_MANUAL")]
    CustodyManual,
    #[serde(rename = "PREMARKET_TRANSFER_IN")]
    PremarketTransferIn,
    #[serde(rename = "PREMARKET_TRANSFER_OUT")]
    PremarketTransferOut,
    #[serde(rename = "PREMARKET_DELIVERY")]
    PremarketDelivery,
    #[serde(rename = "PREMARKET_ROLLBACK")]
    PremarketRollback,
    #[serde(rename = "NONE_C2C_TRANSFER")]
    NoneC2cTransfer,
}
impl<'de> Deserialize<'de> for BybitTransactionType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = <String as Deserialize>::deserialize(d)?;
        let wire = BybitTransactionTypeWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))?;
        Ok(match wire {
            BybitTransactionTypeWire::Trade => Self::Trade,
            BybitTransactionTypeWire::Settlement => Self::Settlement,
            BybitTransactionTypeWire::Delivery => Self::Delivery,
            BybitTransactionTypeWire::Transfer => Self::Transfer,
            BybitTransactionTypeWire::Airdrop => Self::Airdrop,
            BybitTransactionTypeWire::Liquidation => Self::Liquidation,
            BybitTransactionTypeWire::Adl => Self::Adl,
            BybitTransactionTypeWire::BlockTrade => Self::BlockTrade,
            BybitTransactionTypeWire::Bonus => Self::Bonus,
            BybitTransactionTypeWire::FeeRefund => Self::FeeRefund,
            BybitTransactionTypeWire::Interest => Self::Interest,
            BybitTransactionTypeWire::CurrencyBuy => Self::CurrencyBuy,
            BybitTransactionTypeWire::CurrencySell => Self::CurrencySell,
            BybitTransactionTypeWire::AutoRepayment => Self::AutoRepayment,
            BybitTransactionTypeWire::AutoBorrow => Self::AutoBorrow,
            BybitTransactionTypeWire::ManualRepayment => Self::ManualRepayment,
            BybitTransactionTypeWire::SpotRepayCollateral => Self::SpotRepayCollateral,
            BybitTransactionTypeWire::CustodyNetwork => Self::CustodyNetwork,
            BybitTransactionTypeWire::CustodySettle => Self::CustodySettle,
            BybitTransactionTypeWire::Custody => Self::Custody,
            BybitTransactionTypeWire::CustodyAuto => Self::CustodyAuto,
            BybitTransactionTypeWire::CustodyManual => Self::CustodyManual,
            BybitTransactionTypeWire::PremarketTransferIn => Self::PremarketTransferIn,
            BybitTransactionTypeWire::PremarketTransferOut => Self::PremarketTransferOut,
            BybitTransactionTypeWire::PremarketDelivery => Self::PremarketDelivery,
            BybitTransactionTypeWire::PremarketRollback => Self::PremarketRollback,
            BybitTransactionTypeWire::NoneC2cTransfer => Self::NoneC2cTransfer,
        })
    }
}
#[cfg(test)]
#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn bybit_transaction_type_accepted_set() {
    let _exhaustive = |value: BybitTransactionType| match value {
        BybitTransactionType::Trade
        | BybitTransactionType::Settlement
        | BybitTransactionType::Delivery
        | BybitTransactionType::Transfer
        | BybitTransactionType::Airdrop
        | BybitTransactionType::Liquidation
        | BybitTransactionType::Adl
        | BybitTransactionType::BlockTrade
        | BybitTransactionType::Bonus
        | BybitTransactionType::FeeRefund
        | BybitTransactionType::Interest
        | BybitTransactionType::CurrencyBuy
        | BybitTransactionType::CurrencySell
        | BybitTransactionType::AutoRepayment
        | BybitTransactionType::AutoBorrow
        | BybitTransactionType::ManualRepayment
        | BybitTransactionType::SpotRepayCollateral
        | BybitTransactionType::CustodyNetwork
        | BybitTransactionType::CustodySettle
        | BybitTransactionType::Custody
        | BybitTransactionType::CustodyAuto
        | BybitTransactionType::CustodyManual
        | BybitTransactionType::PremarketTransferIn
        | BybitTransactionType::PremarketTransferOut
        | BybitTransactionType::PremarketDelivery
        | BybitTransactionType::PremarketRollback
        | BybitTransactionType::NoneC2cTransfer => (),
    };
    crate::union_audit::check::<BybitTransactionType, BybitTransactionTypeWire>(
        "BybitTransactionType",
    );
}

// Private derived decoder supplies canonical serde spelling metadata. Only the
// string-required public Deserialize implementation can construct the public type.
#[derive(Serialize, Deserialize)]
enum BybitTransactionAccountTypeWire {
    #[serde(rename = "UNIFIED")]
    Unified,
    #[serde(rename = "CONTRACT")]
    Contract,
}
impl<'de> Deserialize<'de> for BybitTransactionAccountType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = <String as Deserialize>::deserialize(d)?;
        let wire =
            BybitTransactionAccountTypeWire::deserialize(serde::de::value::StringDeserializer::<
                D::Error,
            >::new(value))?;
        Ok(match wire {
            BybitTransactionAccountTypeWire::Unified => Self::Unified,
            BybitTransactionAccountTypeWire::Contract => Self::Contract,
        })
    }
}
#[cfg(test)]
#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn bybit_transaction_account_type_accepted_set() {
    let _exhaustive = |value: BybitTransactionAccountType| match value {
        BybitTransactionAccountType::Unified | BybitTransactionAccountType::Contract => (),
    };
    crate::union_audit::check::<BybitTransactionAccountType, BybitTransactionAccountTypeWire>(
        "BybitTransactionAccountType",
    );
}

// Private derived decoder supplies canonical serde spelling metadata. Only the
// string-required public Deserialize implementation can construct the public type.
#[derive(Serialize, Deserialize)]
enum BybitTransactionCategoryWire {
    #[serde(rename = "linear")]
    Linear,
    #[serde(rename = "inverse")]
    Inverse,
    #[serde(rename = "option")]
    Option,
    #[serde(rename = "spot")]
    Spot,
}
impl<'de> Deserialize<'de> for BybitTransactionCategory {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = <String as Deserialize>::deserialize(d)?;
        let wire =
            BybitTransactionCategoryWire::deserialize(serde::de::value::StringDeserializer::<
                D::Error,
            >::new(value))?;
        Ok(match wire {
            BybitTransactionCategoryWire::Linear => Self::Linear,
            BybitTransactionCategoryWire::Inverse => Self::Inverse,
            BybitTransactionCategoryWire::Option => Self::Option,
            BybitTransactionCategoryWire::Spot => Self::Spot,
        })
    }
}
#[cfg(test)]
#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn bybit_transaction_category_accepted_set() {
    let _exhaustive = |value: BybitTransactionCategory| match value {
        BybitTransactionCategory::Linear
        | BybitTransactionCategory::Inverse
        | BybitTransactionCategory::Option
        | BybitTransactionCategory::Spot => (),
    };
    crate::union_audit::check::<BybitTransactionCategory, BybitTransactionCategoryWire>(
        "BybitTransactionCategory",
    );
}

impl Serialize for BybitTransactionType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = match self {
            Self::Trade => BybitTransactionTypeWire::Trade,
            Self::Settlement => BybitTransactionTypeWire::Settlement,
            Self::Delivery => BybitTransactionTypeWire::Delivery,
            Self::Transfer => BybitTransactionTypeWire::Transfer,
            Self::Airdrop => BybitTransactionTypeWire::Airdrop,
            Self::Liquidation => BybitTransactionTypeWire::Liquidation,
            Self::Adl => BybitTransactionTypeWire::Adl,
            Self::BlockTrade => BybitTransactionTypeWire::BlockTrade,
            Self::Bonus => BybitTransactionTypeWire::Bonus,
            Self::FeeRefund => BybitTransactionTypeWire::FeeRefund,
            Self::Interest => BybitTransactionTypeWire::Interest,
            Self::CurrencyBuy => BybitTransactionTypeWire::CurrencyBuy,
            Self::CurrencySell => BybitTransactionTypeWire::CurrencySell,
            Self::AutoRepayment => BybitTransactionTypeWire::AutoRepayment,
            Self::AutoBorrow => BybitTransactionTypeWire::AutoBorrow,
            Self::ManualRepayment => BybitTransactionTypeWire::ManualRepayment,
            Self::SpotRepayCollateral => BybitTransactionTypeWire::SpotRepayCollateral,
            Self::CustodyNetwork => BybitTransactionTypeWire::CustodyNetwork,
            Self::CustodySettle => BybitTransactionTypeWire::CustodySettle,
            Self::Custody => BybitTransactionTypeWire::Custody,
            Self::CustodyAuto => BybitTransactionTypeWire::CustodyAuto,
            Self::CustodyManual => BybitTransactionTypeWire::CustodyManual,
            Self::PremarketTransferIn => BybitTransactionTypeWire::PremarketTransferIn,
            Self::PremarketTransferOut => BybitTransactionTypeWire::PremarketTransferOut,
            Self::PremarketDelivery => BybitTransactionTypeWire::PremarketDelivery,
            Self::PremarketRollback => BybitTransactionTypeWire::PremarketRollback,
            Self::NoneC2cTransfer => BybitTransactionTypeWire::NoneC2cTransfer,
        };
        wire.serialize(serializer)
    }
}

impl Serialize for BybitTransactionAccountType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = match self {
            Self::Unified => BybitTransactionAccountTypeWire::Unified,
            Self::Contract => BybitTransactionAccountTypeWire::Contract,
        };
        wire.serialize(serializer)
    }
}

impl Serialize for BybitTransactionCategory {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = match self {
            Self::Linear => BybitTransactionCategoryWire::Linear,
            Self::Inverse => BybitTransactionCategoryWire::Inverse,
            Self::Option => BybitTransactionCategoryWire::Option,
            Self::Spot => BybitTransactionCategoryWire::Spot,
        };
        wire.serialize(serializer)
    }
}

// Private remote companion: the public path requires a map before serde can
// select its otherwise-permissive positional record visitor.
#[derive(Serialize, Deserialize)]
#[serde(remote = "BybitApiKeyPermissions")]
struct BybitApiKeyPermissionsWire {
    #[serde(rename = "ContractTrade")]
    contract_trade: Vec<String>,
    #[serde(rename = "Spot")]
    spot: Vec<String>,
    #[serde(rename = "Wallet")]
    wallet: Vec<String>,
    #[serde(rename = "Options")]
    options: Vec<String>,
    #[serde(rename = "Derivatives")]
    derivatives: Vec<String>,
    #[serde(rename = "CopyTrading")]
    copy_trading: Vec<String>,
    #[serde(rename = "BlockTrade")]
    block_trade: Vec<String>,
    #[serde(rename = "Exchange")]
    exchange: Vec<String>,
    #[serde(rename = "NFT")]
    n_f_t: Vec<String>,
}
impl<'de> Deserialize<'de> for BybitApiKeyPermissions {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|decoder| {
                BybitApiKeyPermissionsWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl Serialize for BybitApiKeyPermissions {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BybitApiKeyPermissionsWire::serialize(self, serializer)
    }
}

// Private remote companion: the public path requires a map before serde can
// select its otherwise-permissive positional record visitor.
#[derive(Serialize, Deserialize)]
#[serde(remote = "BybitApiKeyInfo")]
struct BybitApiKeyInfoWire {
    #[serde(rename = "id")]
    id: String,
    #[serde(rename = "note")]
    note: String,
    #[serde(rename = "apiKey")]
    api_key: String,
    #[serde(rename = "readOnly")]
    read_only: f64,
    #[serde(rename = "secret")]
    secret: String,
    #[serde(rename = "permissions")]
    permissions: BybitApiKeyPermissions,
    #[serde(rename = "ips")]
    ips: Vec<String>,
    #[serde(rename = "type")]
    r#type: f64,
    #[serde(rename = "deadlineDay")]
    deadline_day: f64,
    #[serde(rename = "expiredAt")]
    expired_at: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "unified")]
    unified: f64,
    #[serde(rename = "uta")]
    uta: f64,
    #[serde(rename = "userID")]
    user_i_d: f64,
    #[serde(rename = "inviterID")]
    inviter_i_d: f64,
    #[serde(rename = "vipLevel")]
    vip_level: String,
    #[serde(rename = "mktMakerLevel")]
    mkt_maker_level: String,
    #[serde(rename = "affiliateID")]
    affiliate_i_d: f64,
    #[serde(rename = "rsaPublicKey")]
    rsa_public_key: String,
    #[serde(rename = "isMaster")]
    is_master: bool,
    #[serde(rename = "parentUid")]
    parent_uid: String,
}
impl<'de> Deserialize<'de> for BybitApiKeyInfo {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|decoder| {
                BybitApiKeyInfoWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl Serialize for BybitApiKeyInfo {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BybitApiKeyInfoWire::serialize(self, serializer)
    }
}

// Private remote companion: the public path requires a map before serde can
// select its otherwise-permissive positional record visitor.
#[derive(Serialize, Deserialize)]
#[serde(remote = "BybitTransactionRecord")]
struct BybitTransactionRecordWire {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "category")]
    category: String,
    #[serde(rename = "side")]
    side: String,
    #[serde(rename = "transactionTime")]
    transaction_time: String,
    #[serde(rename = "type")]
    r#type: BybitTransactionType,
    #[serde(rename = "qty")]
    qty: String,
    #[serde(rename = "size")]
    size: String,
    #[serde(rename = "currency")]
    currency: String,
    #[serde(rename = "tradePrice")]
    trade_price: String,
    #[serde(rename = "funding")]
    funding: String,
    #[serde(rename = "fee")]
    fee: String,
    #[serde(rename = "cashFlow")]
    cash_flow: String,
    #[serde(rename = "change")]
    change: String,
    #[serde(rename = "cashBalance")]
    cash_balance: String,
    #[serde(rename = "feeRate")]
    fee_rate: String,
    #[serde(rename = "bonusChange")]
    bonus_change: String,
    #[serde(rename = "tradeId")]
    trade_id: String,
    #[serde(rename = "orderId")]
    order_id: String,
    #[serde(rename = "orderLinkId")]
    order_link_id: String,
}
impl<'de> Deserialize<'de> for BybitTransactionRecord {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let fields = serde_json::Map::<String, serde_json::Value>::deserialize(d)?;
        BybitTransactionRecordWire::deserialize(serde_json::Value::Object(fields))
            .map_err(serde::de::Error::custom)
    }
}
impl Serialize for BybitTransactionRecord {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BybitTransactionRecordWire::serialize(self, serializer)
    }
}

// Private remote companion: the public path requires a map before serde can
// select its otherwise-permissive positional record visitor.
#[derive(Serialize, Deserialize)]
#[serde(remote = "BybitTransactionHistoryOptions")]
struct BybitTransactionHistoryOptionsWire {
    #[serde(
        rename = "accountType",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    account_type: Option<BybitTransactionAccountType>,
    #[serde(
        rename = "category",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    category: Option<BybitTransactionCategory>,
    #[serde(
        rename = "currency",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    currency: Option<String>,
    #[serde(
        rename = "baseCoin",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    base_coin: Option<String>,
    #[serde(
        rename = "type",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    r#type: Option<BybitTransactionType>,
    #[serde(
        rename = "startTime",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    start_time: Option<f64>,
    #[serde(
        rename = "endTime",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    end_time: Option<f64>,
    #[serde(
        rename = "limit",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    limit: Option<f64>,
    #[serde(
        rename = "cursor",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    cursor: Option<String>,
}
impl<'de> Deserialize<'de> for BybitTransactionHistoryOptions {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|decoder| {
                BybitTransactionHistoryOptionsWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl Serialize for BybitTransactionHistoryOptions {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BybitTransactionHistoryOptionsWire::serialize(self, serializer)
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for BybitApiKeyInfo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { id, note, api_key, read_only, secret, permissions, ips, r#type, deadline_day, expired_at, created_at, unified, uta, user_i_d, inviter_i_d, vip_level, mkt_maker_level, affiliate_i_d, rsa_public_key, is_master, parent_uid });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for BybitTransactionHistoryOptions {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { account_type, category, currency, base_coin, r#type, start_time, end_time, limit, cursor });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for BybitApiKeyPermissions {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { contract_trade, spot, wallet, options, derivatives, copy_trading, block_trade, exchange, n_f_t });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for BybitTransactionType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Trade, Settlement, Delivery, Transfer, Airdrop, Liquidation, Adl, BlockTrade, Bonus, FeeRefund, Interest, CurrencyBuy, CurrencySell, AutoRepayment, AutoBorrow, ManualRepayment, SpotRepayCollateral, CustodyNetwork, CustodySettle, Custody, CustodyAuto, CustodyManual, PremarketTransferIn, PremarketTransferOut, PremarketDelivery, PremarketRollback, NoneC2cTransfer });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for BybitTransactionAccountType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Unified, Contract });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for BybitTransactionCategory {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Linear, Inverse, Option, Spot });
    }
}
