// Generated from the live TypeScript types source by scripts/types.mjs. Do not edit.
pub const RECV_WINDOW: f64 = 20000.0;
pub const BASE_URL: &[(&str, &str)] = &[
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
            ("livenet", "wss://fstream.binance.com/private/ws"),
            ("testnet", "wss://demo-fstream.binance.com/private/ws"),
        ],
    ),
];
pub const ENDPOINTS: &[(&str, &str)] = &[
    ("BALANCE", "/fapi/v3/balance"),
    ("MARKETS", "/fapi/v1/exchangeInfo"),
    ("ACCOUNT", "/fapi/v3/account"),
    ("SYMBOL_CONFIG", "/fapi/v1/symbolConfig"),
    ("LEVERAGE_BRACKET", "/fapi/v1/leverageBracket"),
    ("TICKERS_24H", "/fapi/v1/ticker/24hr"),
    ("TICKERS_BOOK", "/fapi/v1/ticker/bookTicker"),
    ("TICKERS_PRICE", "/fapi/v1/premiumIndex"),
    ("HEDGE_MODE", "/fapi/v1/positionSide/dual"),
    ("SET_LEVERAGE", "/fapi/v1/leverage"),
    ("OPEN_ORDERS", "/fapi/v1/openOrders"),
    ("CANCEL_SYMBOL_ORDERS", "/fapi/v1/allOpenOrders"),
    ("ORDER", "/fapi/v1/order"),
    ("BATCH_ORDERS", "/fapi/v1/batchOrders"),
    ("KLINE", "/fapi/v1/klines"),
    ("LISTEN_KEY", "/fapi/v1/listenKey"),
    ("ORDERBOOK", "/fapi/v1/depth"),
    ("ALL_ORDERS", "/fapi/v1/allOrders"),
    ("USER_TRADES", "/fapi/v1/userTrades"),
    ("SET_MARGIN_TYPE", "/fapi/v1/marginType"),
    ("POSITION_RISK", "/fapi/v3/positionRisk"),
    ("TRADES", "/fapi/v1/trades"),
    ("ALGO_ORDER", "/fapi/v1/algoOrder"),
    ("OPEN_ALGO_ORDERS", "/fapi/v1/openAlgoOrders"),
    ("CANCEL_SYMBOL_ALGO_ORDERS", "/fapi/v1/algoOpenOrders"),
    ("INCOME", "/fapi/v1/income"),
];
pub const PUBLIC_ENDPOINTS: &[&str] = &[
    "/fapi/v1/exchangeInfo",
    "/fapi/v1/ticker/24hr",
    "/fapi/v1/ticker/bookTicker",
    "/fapi/v1/premiumIndex",
    "/fapi/v1/klines",
    "/fapi/v1/depth",
    "/fapi/v1/trades",
];
pub const GRAFANA_ENDPOINTS_EVERY: &[&str] = &[
    "/fapi/v1/order",
    "/fapi/v1/batchOrders",
    "/fapi/v1/leverage",
    "/fapi/v1/marginType",
    "/fapi/v1/allOpenOrders",
    "/fapi/v1/algoOpenOrders",
    "/fapi/v1/positionSide/dual",
];
pub const GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &[
    "/fapi/v3/balance",
    "/fapi/v3/account",
    "/fapi/v1/symbolConfig",
    "/fapi/v1/leverageBracket",
    "/fapi/v1/openOrders",
    "/fapi/v3/positionRisk",
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum BinanceIncomeType {
    #[serde(rename = "TRANSFER")]
    TRANSFER,
    #[serde(rename = "WELCOME_BONUS")]
    WelcomeBonus,
    #[serde(rename = "REALIZED_PNL")]
    RealizedPnl,
    #[serde(rename = "FUNDING_FEE")]
    FundingFee,
    #[serde(rename = "COMMISSION")]
    COMMISSION,
    #[serde(rename = "INSURANCE_CLEAR")]
    InsuranceClear,
    #[serde(rename = "REFERRAL_KICKBACK")]
    ReferralKickback,
    #[serde(rename = "COMMISSION_REBATE")]
    CommissionRebate,
    #[serde(rename = "API_REBATE")]
    ApiRebate,
    #[serde(rename = "CONTEST_REWARD")]
    ContestReward,
    #[serde(rename = "CROSS_COLLATERAL_TRANSFER")]
    CrossCollateralTransfer,
    #[serde(rename = "OPTIONS_PREMIUM_FEE")]
    OptionsPremiumFee,
    #[serde(rename = "OPTIONS_SETTLE_PROFIT")]
    OptionsSettleProfit,
    #[serde(rename = "INTERNAL_TRANSFER")]
    InternalTransfer,
    #[serde(rename = "AUTO_EXCHANGE")]
    AutoExchange,
    #[serde(rename = "DELIVERED_SETTELMENT")]
    DeliveredSettelment,
    #[serde(rename = "COIN_SWAP_DEPOSIT")]
    CoinSwapDeposit,
    #[serde(rename = "COIN_SWAP_WITHDRAW")]
    CoinSwapWithdraw,
    #[serde(rename = "POSITION_LIMIT_INCREASE_FEE")]
    PositionLimitIncreaseFee,
}
#[derive(serde::Deserialize)]
#[serde(remote = "BinanceIncomeType")]
// Remote variants must retain the public source-derived identifiers.
#[allow(clippy::upper_case_acronyms)]
enum BinanceIncomeTypeWire {
    #[serde(rename = "TRANSFER")]
    TRANSFER,
    #[serde(rename = "WELCOME_BONUS")]
    WelcomeBonus,
    #[serde(rename = "REALIZED_PNL")]
    RealizedPnl,
    #[serde(rename = "FUNDING_FEE")]
    FundingFee,
    #[serde(rename = "COMMISSION")]
    COMMISSION,
    #[serde(rename = "INSURANCE_CLEAR")]
    InsuranceClear,
    #[serde(rename = "REFERRAL_KICKBACK")]
    ReferralKickback,
    #[serde(rename = "COMMISSION_REBATE")]
    CommissionRebate,
    #[serde(rename = "API_REBATE")]
    ApiRebate,
    #[serde(rename = "CONTEST_REWARD")]
    ContestReward,
    #[serde(rename = "CROSS_COLLATERAL_TRANSFER")]
    CrossCollateralTransfer,
    #[serde(rename = "OPTIONS_PREMIUM_FEE")]
    OptionsPremiumFee,
    #[serde(rename = "OPTIONS_SETTLE_PROFIT")]
    OptionsSettleProfit,
    #[serde(rename = "INTERNAL_TRANSFER")]
    InternalTransfer,
    #[serde(rename = "AUTO_EXCHANGE")]
    AutoExchange,
    #[serde(rename = "DELIVERED_SETTELMENT")]
    DeliveredSettelment,
    #[serde(rename = "COIN_SWAP_DEPOSIT")]
    CoinSwapDeposit,
    #[serde(rename = "COIN_SWAP_WITHDRAW")]
    CoinSwapWithdraw,
    #[serde(rename = "POSITION_LIMIT_INCREASE_FEE")]
    PositionLimitIncreaseFee,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for BinanceIncomeType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { TRANSFER,WelcomeBonus,RealizedPnl,FundingFee,COMMISSION,InsuranceClear,ReferralKickback,CommissionRebate,ApiRebate,ContestReward,CrossCollateralTransfer,OptionsPremiumFee,OptionsSettleProfit,InternalTransfer,AutoExchange,DeliveredSettelment,CoinSwapDeposit,CoinSwapWithdraw,PositionLimitIncreaseFee });
    }
}
impl<'de> serde::Deserialize<'de> for BinanceIncomeType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(deserializer)?;
        BinanceIncomeTypeWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
            text,
        ))
    }
}
