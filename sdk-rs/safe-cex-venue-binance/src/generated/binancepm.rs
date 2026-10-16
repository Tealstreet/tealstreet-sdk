// Generated from the live TypeScript types source by scripts/types.mjs. Do not edit.
pub const RECV_WINDOW: f64 = 20000.0;
pub const BASE_PAPI_URL: &[(&str, &str)] = &[
    ("livenet", "https://papi.binance.com"),
    ("testnet", "https://demo-fapi.binance.com"),
];
pub const BASE_FAPI_URL: &[(&str, &str)] = &[
    ("livenet", "https://fapi.binance.com"),
    ("testnet", "https://demo-fapi.binance.com"),
];
pub const BASE_WS_URL: &[(&str, &[(&str, &str)])] = &[
    (
        "public",
        &[
            ("livenet", "wss://fstream.binance.com/public/ws"),
            ("testnet", "wss://demo-fstream.binance.com/public/ws"),
        ],
    ),
    (
        "market",
        &[
            ("livenet", "wss://fstream.binance.com/market/ws"),
            ("testnet", "wss://demo-fstream.binance.com/market/ws"),
        ],
    ),
    (
        "private",
        &[
            ("livenet", "wss://fstream.binance.com/pm/ws"),
            ("testnet", "wss://demo-fstream.binance.com/pm/ws"),
        ],
    ),
];
pub const ENDPOINTS: &[(&str, &str)] = &[
    ("BALANCE", "/papi/v1/balance"),
    ("MARKETS", "/fapi/v1/exchangeInfo"),
    ("ACCOUNT", "/papi/v1/account"),
    ("LEVERAGE_BRACKET", "/papi/v1/um/leverageBracket"),
    ("TICKERS_24H", "/fapi/v1/ticker/24hr"),
    ("TICKERS_BOOK", "/fapi/v1/ticker/bookTicker"),
    ("TICKERS_PRICE", "/fapi/v1/premiumIndex"),
    ("HEDGE_MODE", "/papi/v1/um/positionSide/dual"),
    ("SET_LEVERAGE", "/papi/v1/um/leverage"),
    ("OPEN_ORDERS", "/papi/v1/um/openOrders"),
    (
        "OPEN_CONDITIONAL_ORDERS",
        "/papi/v1/um/conditional/openOrders",
    ),
    ("CANCEL_SYMBOL_ORDERS", "/papi/v1/um/allOpenOrders"),
    ("ORDER", "/papi/v1/um/order"),
    ("CONDITIONAL_ORDER", "/papi/v1/um/conditional/order"),
    ("KLINE", "/fapi/v1/klines"),
    ("LISTEN_KEY", "/papi/v1/listenKey"),
    ("ORDERBOOK", "/fapi/v1/depth"),
    ("USER_TRADES", "/papi/v1/um/userTrades"),
    ("POSITION_RISK", "/papi/v1/um/positionRisk"),
];
pub const PUBLIC_ENDPOINTS: &[&str] = &[
    "/fapi/v1/exchangeInfo",
    "/fapi/v1/ticker/24hr",
    "/fapi/v1/ticker/bookTicker",
    "/fapi/v1/premiumIndex",
    "/fapi/v1/klines",
    "/fapi/v1/depth",
];
pub const GRAFANA_ENDPOINTS_EVERY: &[&str] = &[
    "/papi/v1/um/order",
    "/papi/v1/um/leverage",
    "/papi/v1/um/allOpenOrders",
    "/papi/v1/um/positionSide/dual",
];
pub const GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &[
    "/papi/v1/balance",
    "/papi/v1/account",
    "/papi/v1/um/leverageBracket",
    "/papi/v1/um/openOrders",
    "/papi/v1/um/positionRisk",
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
pub struct GetPositionRiskResponseRow {
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
    #[serde(rename = "leverage")]
    pub leverage: String,
    #[serde(rename = "maxNotionalValue")]
    pub max_notional_value: String,
    #[serde(rename = "marginType")]
    pub margin_type: String,
    #[serde(rename = "isolatedMargin")]
    pub isolated_margin: String,
    #[serde(rename = "isAutoAddMargin")]
    pub is_auto_add_margin: String,
    #[serde(rename = "positionSide")]
    pub position_side: String,
    #[serde(rename = "notional")]
    pub notional: String,
    #[serde(rename = "isolatedWallet")]
    pub isolated_wallet: String,
    #[serde(rename = "updateTime")]
    pub update_time: f64,
    #[serde(rename = "isolated")]
    pub isolated: bool,
    #[serde(rename = "adlQuantile")]
    pub adl_quantile: f64,
}
#[derive(serde::Deserialize)]
#[serde(remote = "GetPositionRiskResponseRow")]
struct GetPositionRiskResponseRowWire {
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
    #[serde(rename = "leverage")]
    pub leverage: String,
    #[serde(rename = "maxNotionalValue")]
    pub max_notional_value: String,
    #[serde(rename = "marginType")]
    pub margin_type: String,
    #[serde(rename = "isolatedMargin")]
    pub isolated_margin: String,
    #[serde(rename = "isAutoAddMargin")]
    pub is_auto_add_margin: String,
    #[serde(rename = "positionSide")]
    pub position_side: String,
    #[serde(rename = "notional")]
    pub notional: String,
    #[serde(rename = "isolatedWallet")]
    pub isolated_wallet: String,
    #[serde(rename = "updateTime")]
    pub update_time: f64,
    #[serde(rename = "isolated")]
    pub isolated: bool,
    #[serde(rename = "adlQuantile")]
    pub adl_quantile: f64,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for GetPositionRiskResponseRow {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol,position_amt,entry_price,mark_price,un_realized_profit,liquidation_price,leverage,max_notional_value,margin_type,isolated_margin,is_auto_add_margin,position_side,notional,isolated_wallet,update_time,isolated,adl_quantile });
    }
}
impl<'de> serde::Deserialize<'de> for GetPositionRiskResponseRow {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input =
            <safe_cex_num::numeric_ingress::JsNumberInput as serde::Deserialize>::deserialize(
                deserializer,
            )?;
        input
            .decode_with(|decoder| {
                GetPositionRiskResponseRowWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
pub type GetPositionRiskResponse = Vec<GetPositionRiskResponseRow>;
