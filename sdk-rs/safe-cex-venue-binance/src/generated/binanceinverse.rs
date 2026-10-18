// Generated from the live TypeScript types source by scripts/types.mjs. Do not edit.
pub const RECV_WINDOW: f64 = 20000.0;
pub const BASE_URL: &[(&str, &str)] = &[
    ("livenet", "https://dapi.binance.com"),
    ("testnet", "https://demo-dapi.binance.com"),
];
pub const BASE_WS_URL: &[(&str, &str)] = &[
    ("livenet", "wss://dstream.binance.com/ws"),
    ("testnet", "wss://dstream.binancefuture.com/ws"),
];
pub const ENDPOINTS: &[(&str, &str)] = &[
    ("BALANCE", "/dapi/v1/balance"),
    ("MARKETS", "/dapi/v1/exchangeInfo"),
    ("ACCOUNT", "/dapi/v1/account"),
    ("LEVERAGE_BRACKET", "/dapi/v2/leverageBracket"),
    ("TICKERS_24H", "/dapi/v1/ticker/24hr"),
    ("TICKERS_BOOK", "/dapi/v1/ticker/bookTicker"),
    ("TICKERS_PRICE", "/dapi/v1/premiumIndex"),
    ("HEDGE_MODE", "/dapi/v1/positionSide/dual"),
    ("SET_LEVERAGE", "/dapi/v1/leverage"),
    ("OPEN_ORDERS", "/dapi/v1/openOrders"),
    ("CANCEL_SYMBOL_ORDERS", "/dapi/v1/allOpenOrders"),
    ("ORDER", "/dapi/v1/order"),
    ("BATCH_ORDERS", "/dapi/v1/batchOrders"),
    ("KLINE", "/dapi/v1/klines"),
    ("LISTEN_KEY", "/dapi/v1/listenKey"),
    ("ORDERBOOK", "/dapi/v1/depth"),
    ("ALL_ORDERS", "/dapi/v1/allOrders"),
    ("USER_TRADES", "/dapi/v1/userTrades"),
    ("SET_MARGIN_TYPE", "/dapi/v1/marginType"),
    ("POSITION_RISK", "/dapi/v1/positionRisk"),
    ("TRADES", "/dapi/v1/trades"),
    ("INCOME", "/dapi/v1/income"),
];
pub const PUBLIC_ENDPOINTS: &[&str] = &[
    "/dapi/v1/exchangeInfo",
    "/dapi/v1/ticker/24hr",
    "/dapi/v1/ticker/bookTicker",
    "/dapi/v1/premiumIndex",
    "/dapi/v1/klines",
    "/dapi/v1/depth",
    "/dapi/v1/trades",
];
pub const GRAFANA_ENDPOINTS_EVERY: &[&str] = &[
    "/dapi/v1/order",
    "/dapi/v1/batchOrders",
    "/dapi/v1/leverage",
    "/dapi/v1/marginType",
    "/dapi/v1/allOpenOrders",
    "/dapi/v1/positionSide/dual",
];
pub const GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &[
    "/dapi/v1/balance",
    "/dapi/v1/account",
    "/dapi/v2/leverageBracket",
    "/dapi/v1/openOrders",
    "/dapi/v1/positionRisk",
];
pub const ORDER_TYPE: &[(&str, safe_cex_types::OrderType)] = &[
    ("LIMIT", safe_cex_types::OrderType::Limit),
    ("MARKET", safe_cex_types::OrderType::Market),
    ("STOP_MARKET", safe_cex_types::OrderType::StopLoss),
    ("TAKE_PROFIT_MARKET", safe_cex_types::OrderType::TakeProfit),
    (
        "TRAILING_STOP_MARKET",
        safe_cex_types::OrderType::TrailingStopLoss,
    ),
];
pub const ORDER_SIDE: &[(&str, safe_cex_types::OrderSide)] = &[
    ("BUY", safe_cex_types::OrderSide::Buy),
    ("SELL", safe_cex_types::OrderSide::Sell),
];
pub const POSITION_SIDE: &[(&str, safe_cex_types::PositionSide)] = &[
    ("LONG", safe_cex_types::PositionSide::Long),
    ("SHORT", safe_cex_types::PositionSide::Short),
];
pub const TIME_IN_FORCE: &[(&str, safe_cex_types::OrderTimeInForce)] = &[
    ("GTC", safe_cex_types::OrderTimeInForce::GoodTillCancel),
    ("IOC", safe_cex_types::OrderTimeInForce::ImmediateOrCancel),
    ("FOK", safe_cex_types::OrderTimeInForce::FillOrKill),
    ("GTX", safe_cex_types::OrderTimeInForce::PostOnly),
];
pub const ORDER_TRIGGER_TYPE: &[(&str, safe_cex_types::OrderStopTriggerType)] = &[
    (
        "MARK_PRICE",
        safe_cex_types::OrderStopTriggerType::MarkPrice,
    ),
    (
        "CONTRACT_PRICE",
        safe_cex_types::OrderStopTriggerType::LastPrice,
    ),
];
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BinanceInversePositionRisk {
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "positionAmt")]
    pub position_amt: String,
    #[serde(rename = "entryPrice")]
    pub entry_price: String,
    #[serde(rename = "markPrice")]
    pub mark_price: String,
    #[serde(rename = "unRealizedProfit")]
    pub un_realized_profit: String,
    #[serde(rename = "liquidationPrice")]
    pub liquidation_price: String,
    #[serde(
        rename = "leverage",
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub leverage: Option<String>,
    #[serde(
        rename = "maxQty",
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_qty: Option<String>,
    #[serde(
        rename = "marginType",
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub margin_type: Option<String>,
    #[serde(rename = "isolatedMargin")]
    pub isolated_margin: String,
    #[serde(rename = "positionSide")]
    pub position_side: String,
    #[serde(rename = "updateTime")]
    pub update_time: f64,
}
#[derive(serde::Deserialize)]
#[serde(remote = "BinanceInversePositionRisk")]
struct BinanceInversePositionRiskWire {
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "positionAmt")]
    pub position_amt: String,
    #[serde(rename = "entryPrice")]
    pub entry_price: String,
    #[serde(rename = "markPrice")]
    pub mark_price: String,
    #[serde(rename = "unRealizedProfit")]
    pub un_realized_profit: String,
    #[serde(rename = "liquidationPrice")]
    pub liquidation_price: String,
    #[serde(
        rename = "leverage",
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub leverage: Option<String>,
    #[serde(
        rename = "maxQty",
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_qty: Option<String>,
    #[serde(
        rename = "marginType",
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub margin_type: Option<String>,
    #[serde(rename = "isolatedMargin")]
    pub isolated_margin: String,
    #[serde(rename = "positionSide")]
    pub position_side: String,
    #[serde(rename = "updateTime")]
    pub update_time: f64,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for BinanceInversePositionRisk {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol,position_amt,entry_price,mark_price,un_realized_profit,liquidation_price,leverage,max_qty,margin_type,isolated_margin,position_side,update_time });
    }
}
impl<'de> serde::Deserialize<'de> for BinanceInversePositionRisk {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input =
            <safe_cex_num::numeric_ingress::JsNumberInput as serde::Deserialize>::deserialize(
                deserializer,
            )?;
        input
            .decode_with(|decoder| {
                BinanceInversePositionRiskWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
pub type GetPositionRiskResponse = Vec<BinanceInversePositionRisk>;
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BinanceIncomeRecord {
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "incomeType")]
    pub income_type: String,
    #[serde(rename = "income")]
    pub income: String,
    #[serde(rename = "asset")]
    pub asset: String,
    #[serde(
        rename = "info",
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub info: Option<String>,
    #[serde(rename = "time")]
    pub time: f64,
    #[serde(rename = "tranId")]
    pub tran_id: String,
    #[serde(
        rename = "tradeId",
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub trade_id: Option<String>,
}
#[derive(serde::Deserialize)]
#[serde(remote = "BinanceIncomeRecord")]
struct BinanceIncomeRecordWire {
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "incomeType")]
    pub income_type: String,
    #[serde(rename = "income")]
    pub income: String,
    #[serde(rename = "asset")]
    pub asset: String,
    #[serde(
        rename = "info",
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub info: Option<String>,
    #[serde(rename = "time")]
    pub time: f64,
    #[serde(rename = "tranId")]
    pub tran_id: String,
    #[serde(
        rename = "tradeId",
        default,
        deserialize_with = "crate::deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub trade_id: Option<String>,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for BinanceIncomeRecord {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol,income_type,income,asset,info,time,tran_id,trade_id });
    }
}
impl<'de> serde::Deserialize<'de> for BinanceIncomeRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input =
            <safe_cex_num::numeric_ingress::JsNumberInput as serde::Deserialize>::deserialize(
                deserializer,
            )?;
        input
            .decode_with(|decoder| {
                BinanceIncomeRecordWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
