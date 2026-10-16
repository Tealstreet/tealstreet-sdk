//! Static own-property protocol tables; table lookup behavior belongs to consumers.
use safe_cex_types::{OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce, OrderType};

pub const RECV_WINDOW: u64 = 20_000;
pub const BROKER_ID: &str = "6587";
pub const BASE_URL: [(&str, &str); 2] = [
    ("livenet", "https://omni.apex.exchange/api"),
    ("testnet", "https://testnet.omni.apex.exchange/api"),
];
pub const BASE_WS_URL: [(&[&str], &str); 4] = [
    (
        &["public", "linear", "livenet"],
        "wss://quote.omni.apex.exchange/realtime_public?v=2&timestamp=1661415017232",
    ),
    (
        &["public", "linear", "testnet"],
        "wss://qa-quote.omni.apex.exchange/realtime_public?v=2&timestamp=1661415017232",
    ),
    (
        &["private", "livenet"],
        "wss://quote.omni.apex.exchange/realtime_private?v=2&timestamp=1661415017232",
    ),
    (
        &["private", "testnet"],
        "wss://qa-quote.omni.apex.exchange/realtime_private?v=2&timestamp=1661415017233",
    ),
];
pub const ENDPOINTS: [(&str, &str); 12] = [
    ("USER", "/v3/user"),
    ("SYMBOLS", "/v3/symbols"),
    ("KLINE", "/v3/klines"),
    ("ACCOUNT", "/v3/account"),
    ("BALANCE", "/v3/account-balance"),
    ("FILLS", "/v3/fills"),
    ("HISTORY_ORDERS", "/v3/history-orders"),
    ("OPEN_ORDERS", "/v3/open-orders"),
    ("CANCEL_ORDERS", "/v3/delete-open-orders"),
    ("CANCEL_ORDER", "/v3/delete-order"),
    ("SET_LEVERAGE", "/v3/set-initial-margin-rate"),
    ("CREATE_ORDER", "/v3/order"),
];
pub const GRAFANA_ENDPOINTS_EVERY: [&str; 5] = [
    "/v3/order",
    "/v3/set-initial-margin-rate",
    "/v3/delete-order",
    "/v3/delete-open-orders",
    "/v3/user",
];
pub const GRAFANA_ENDPOINTS_PERIODIC: [&str; 2] = ["/v3/account", "/v3/account-balance"];
pub const INTERVAL: [(&str, &str); 12] = [
    ("1m", "1"),
    ("3m", "3"),
    ("5m", "5"),
    ("15m", "15"),
    ("30m", "30"),
    ("1h", "60"),
    ("2h", "120"),
    ("4h", "240"),
    ("6h", "360"),
    ("12h", "720"),
    ("1d", "D"),
    ("1w", "W"),
];
pub const ORDER_STATUS: [(&str, OrderStatus); 6] = [
    ("PENDING", OrderStatus::Open),
    ("OPEN", OrderStatus::Open),
    ("FILLED", OrderStatus::Closed),
    ("CANCELED", OrderStatus::Canceled),
    ("EXPIRED", OrderStatus::Closed),
    ("UNTRIGGERED", OrderStatus::Open),
];
pub const ORDER_TYPE: [(&str, OrderType); 6] = [
    ("LIMIT", OrderType::Limit),
    ("MARKET", OrderType::Market),
    ("STOP_MARKET", OrderType::StopLoss),
    ("STOP_LIMIT", OrderType::StopLoss),
    ("TAKE_PROFIT_MARKET", OrderType::TakeProfit),
    ("TAKE_PROFIT_LIMIT", OrderType::TakeProfit),
];
pub const ORDER_SIDE: [(&str, OrderSide); 2] = [("BUY", OrderSide::Buy), ("SELL", OrderSide::Sell)];
pub const ORDER_TIME_IN_FORCE: [(&str, OrderTimeInForce); 4] = [
    ("GOOD_TIL_CANCEL", OrderTimeInForce::GoodTillCancel),
    ("IMMEDIATE_OR_CANCEL", OrderTimeInForce::ImmediateOrCancel),
    ("FILL_OR_KILL", OrderTimeInForce::FillOrKill),
    ("POST_ONLY", OrderTimeInForce::PostOnly),
];
pub const ORDER_TRIGGER_TYPE: [(&str, OrderStopTriggerType); 2] = [
    ("INDEX", OrderStopTriggerType::IndexPrice),
    ("MARKET", OrderStopTriggerType::LastPrice),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum OrderCancelReason {
    #[serde(rename = "UNKNOWN_ORDER_CANCEL_REASON")]
    UnknownOrderCancelReason,
    #[serde(rename = "EXPIRED")]
    Expired,
    #[serde(rename = "USER_CANCELED")]
    UserCanceled,
    #[serde(rename = "COULD_NOT_FILL")]
    CouldNotFill,
    #[serde(rename = "REDUCE_ONLY_CANCELED")]
    ReduceOnlyCanceled,
    #[serde(rename = "LIQUIDATE_CANCELED")]
    LiquidateCanceled,
    #[serde(rename = "INTERNAL_FAILED")]
    InternalFailed,
}
impl<'de> serde::Deserialize<'de> for OrderCancelReason {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(decoder)?;
        match value.as_str() {
            "UNKNOWN_ORDER_CANCEL_REASON" => Ok(Self::UnknownOrderCancelReason),
            "EXPIRED" => Ok(Self::Expired),
            "USER_CANCELED" => Ok(Self::UserCanceled),
            "COULD_NOT_FILL" => Ok(Self::CouldNotFill),
            "REDUCE_ONLY_CANCELED" => Ok(Self::ReduceOnlyCanceled),
            "LIQUIDATE_CANCELED" => Ok(Self::LiquidateCanceled),
            "INTERNAL_FAILED" => Ok(Self::InternalFailed),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &[
                    "UNKNOWN_ORDER_CANCEL_REASON",
                    "EXPIRED",
                    "USER_CANCELED",
                    "COULD_NOT_FILL",
                    "REDUCE_ONLY_CANCELED",
                    "LIQUIDATE_CANCELED",
                    "INTERNAL_FAILED",
                ],
            )),
        }
    }
}
