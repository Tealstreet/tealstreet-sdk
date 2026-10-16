//! Function-for-function wire vocabulary from breakoutprop.types.ts.
use safe_cex_num::{js_number_output, numeric_ingress::JsNumberOutput};
use serde::{Deserialize, Serialize};

pub const RECV_WINDOW: f64 = 20_000.0;
pub const BASE_URL: &str = "https://breakout.prosp.devexperts.com/dxsca-web";
pub const ENDPOINTS: &[(&str, &str)] = &[
    ("LOGIN", "/login"),
    ("USER", "/users/{username}"),
    ("PORTFOLIO", "accounts/{accountCode}/portfolio"),
    ("INSTRUMENTS", "instruments/query"),
    (
        "INSTRUMENT_DETAILS",
        "accounts/{accountCode}/instruments/query",
    ),
    ("MARKET_DATA", "marketdata"),
    ("CANCEL_ORDER", "accounts/{accountCode}/orders/{orderCode}"),
    ("PLACE_ORDER", "accounts/{accountCode}/orders"),
];
pub const GRAFANA_ENDPOINTS_EVERY: &[&str] = &[
    "accounts/{accountCode}/orders",
    "accounts/{accountCode}/orders/{orderCode}",
];
pub const GRAFANA_ENDPOINTS_PERIODIC: &[&str] = &["accounts/{accountCode}/portfolio"];
// Use safe_cex_num::js_table_lookup at open-string lookup sites to retain prototypes.
pub const INTERVAL: &[(&str, &str)] = &[
    ("1m", "m"),
    ("5m", "5m"),
    ("15m", "15m"),
    ("30m", "30m"),
    ("1h", "h"),
    ("2h", "2h"),
    ("4h", "4h"),
    ("1d", "d"),
    ("1w", "w"),
];
pub const ORDER_TYPE: &[(&str, &str)] = &[
    ("LIMIT", "limit"),
    ("MARKET", "market"),
    ("STOP", "stop_market"),
];
pub const ORDER_SIDE: &[(&str, &str)] = &[("BUY", "buy"), ("SELL", "sell")];
pub const ORDER_STATUS: &[(&str, &str)] = &[("WORKING", "open")];
pub const POSITION_SIDE: &[(&str, &str)] = &[("BUY", "long"), ("SELL", "short")];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    #[serde(rename = "sessionToken")]
    pub session_token: String,
    #[serde(rename = "timeout")]
    pub timeout: String,
}
impl JsNumberOutput for LoginResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { session_token, timeout });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAccount {
    #[serde(rename = "owner")]
    pub owner: String,
    #[serde(rename = "account")]
    pub account: String,
    #[serde(rename = "version")]
    pub version: f64,
    #[serde(rename = "baseCurrency")]
    pub base_currency: String,
    #[serde(rename = "registrationTime")]
    pub registration_time: String,
    #[serde(rename = "accountStatus")]
    pub account_status: String,
    #[serde(rename = "pricingStream")]
    pub pricing_stream: String,
    #[serde(rename = "positionBased")]
    pub position_based: bool,
}
impl JsNumberOutput for UserAccount {
    fn check_number_output(&self) {
        js_number_output!(self, struct { owner, account, version, base_currency, registration_time, account_status, pricing_stream, position_based });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDetail {
    #[serde(rename = "login")]
    pub login: String,
    #[serde(rename = "domain")]
    pub domain: String,
    #[serde(rename = "version")]
    pub version: f64,
    #[serde(rename = "fullName")]
    pub full_name: String,
    #[serde(rename = "accounts")]
    pub accounts: Vec<UserAccount>,
    #[serde(rename = "username")]
    pub username: String,
}
impl JsNumberOutput for UserDetail {
    fn check_number_output(&self) {
        js_number_output!(self, struct { login, domain, version, full_name, accounts, username });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserResponse {
    #[serde(rename = "userDetails")]
    pub user_details: Vec<UserDetail>,
}
impl JsNumberOutput for UserResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { user_details });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioBalance {
    #[serde(rename = "account")]
    pub account: String,
    #[serde(rename = "version")]
    pub version: f64,
    #[serde(rename = "value")]
    pub value: f64,
    #[serde(rename = "currency")]
    pub currency: String,
}
impl JsNumberOutput for PortfolioBalance {
    fn check_number_output(&self) {
        js_number_output!(self, struct { account, version, value, currency });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioPosition {
    #[serde(rename = "account")]
    pub account: String,
    #[serde(rename = "version")]
    pub version: f64,
    #[serde(rename = "positionCode")]
    pub position_code: String,
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "quantity")]
    pub quantity: f64,
    #[serde(rename = "side")]
    pub side: String,
    #[serde(rename = "quantityNotional")]
    pub quantity_notional: f64,
    #[serde(rename = "openTime")]
    pub open_time: String,
    #[serde(rename = "openPrice")]
    pub open_price: f64,
    #[serde(rename = "lastUpdateTime")]
    pub last_update_time: String,
    #[serde(rename = "marginRate")]
    pub margin_rate: f64,
}
impl JsNumberOutput for PortfolioPosition {
    fn check_number_output(&self) {
        js_number_output!(self, struct { account, version, position_code, symbol, quantity, side, quantity_notional, open_time, open_price, last_update_time, margin_rate });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderLeg {
    #[serde(rename = "instrument")]
    pub instrument: String,
    #[serde(rename = "positionEffect")]
    pub position_effect: String,
    #[serde(rename = "positionCode")]
    pub position_code: String,
    #[serde(rename = "side")]
    pub side: String,
    #[serde(rename = "price")]
    pub price: f64,
    #[serde(rename = "legRatio")]
    pub leg_ratio: f64,
    #[serde(rename = "quantity")]
    pub quantity: f64,
    #[serde(rename = "filledQuantity")]
    pub filled_quantity: f64,
    #[serde(rename = "remainingQuantity")]
    pub remaining_quantity: f64,
    #[serde(rename = "averagePrice")]
    pub average_price: f64,
}
impl JsNumberOutput for OrderLeg {
    fn check_number_output(&self) {
        js_number_output!(self, struct { instrument, position_effect, position_code, side, price, leg_ratio, quantity, filled_quantity, remaining_quantity, average_price });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderExecution {
    #[serde(rename = "account")]
    pub account: String,
    #[serde(rename = "executionCode")]
    pub execution_code: String,
    #[serde(rename = "orderCode")]
    pub order_code: String,
    #[serde(rename = "updateOrderId")]
    pub update_order_id: f64,
    #[serde(rename = "version")]
    pub version: f64,
    #[serde(rename = "clientOrderId")]
    pub client_order_id: String,
    #[serde(rename = "actionCode")]
    pub action_code: String,
    #[serde(rename = "status")]
    pub status: String,
    #[serde(rename = "finalStatus")]
    pub final_status: bool,
    #[serde(rename = "filledQuantity")]
    pub filled_quantity: f64,
    #[serde(rename = "lastQuantity")]
    pub last_quantity: f64,
    #[serde(rename = "filledQuantityNotional")]
    pub filled_quantity_notional: f64,
    #[serde(rename = "lastQuantityNotional")]
    pub last_quantity_notional: f64,
    #[serde(rename = "transactionTime")]
    pub transaction_time: String,
    #[serde(rename = "marginRate")]
    pub margin_rate: f64,
}
impl JsNumberOutput for OrderExecution {
    fn check_number_output(&self) {
        js_number_output!(self, struct { account, execution_code, order_code, update_order_id, version, client_order_id, action_code, status, final_status, filled_quantity, last_quantity, filled_quantity_notional, last_quantity_notional, transaction_time, margin_rate });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioOrder {
    #[serde(rename = "account")]
    pub account: String,
    #[serde(rename = "version")]
    pub version: f64,
    #[serde(rename = "orderId")]
    pub order_id: f64,
    #[serde(rename = "orderCode")]
    pub order_code: String,
    #[serde(rename = "clientOrderId")]
    pub client_order_id: String,
    #[serde(rename = "actionCode")]
    pub action_code: String,
    #[serde(rename = "legCount")]
    pub leg_count: f64,
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(rename = "instrument")]
    pub instrument: String,
    #[serde(rename = "status")]
    pub status: String,
    #[serde(rename = "finalStatus")]
    pub final_status: bool,
    #[serde(rename = "legs")]
    pub legs: Vec<OrderLeg>,
    #[serde(rename = "side")]
    pub side: String,
    #[serde(rename = "tif")]
    pub tif: String,
    #[serde(rename = "marginRate")]
    pub margin_rate: f64,
    #[serde(rename = "issueTime")]
    pub issue_time: String,
    #[serde(rename = "transactionTime")]
    pub transaction_time: String,
    #[serde(rename = "executions")]
    pub executions: Vec<OrderExecution>,
    #[serde(rename = "cashTransactions")]
    #[serde(deserialize_with = "decode_cash_transactions")]
    pub cash_transactions: serde_json::Value,
}
impl JsNumberOutput for PortfolioOrder {
    fn check_number_output(&self) {
        js_number_output!(self, struct { account, version, order_id, order_code, client_order_id, action_code, leg_count, r#type, instrument, status, final_status, legs, side, tif, margin_rate, issue_time, transaction_time, executions } passthrough { cash_transactions });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Portfolio {
    #[serde(rename = "account")]
    pub account: String,
    #[serde(rename = "version")]
    pub version: f64,
    #[serde(rename = "balances")]
    pub balances: Vec<PortfolioBalance>,
    #[serde(rename = "positions")]
    pub positions: Vec<PortfolioPosition>,
    #[serde(rename = "orders")]
    pub orders: Vec<PortfolioOrder>,
}
impl JsNumberOutput for Portfolio {
    fn check_number_output(&self) {
        js_number_output!(self, struct { account, version, balances, positions, orders });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioResponse {
    #[serde(rename = "portfolios")]
    pub portfolios: Vec<Portfolio>,
}
impl JsNumberOutput for PortfolioResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { portfolios });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instrument {
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "version")]
    pub version: f64,
    #[serde(rename = "description")]
    pub description: String,
    #[serde(rename = "priceIncrement")]
    pub price_increment: f64,
    #[serde(rename = "pipSize")]
    pub pip_size: f64,
    #[serde(rename = "currency")]
    pub currency: String,
    #[serde(rename = "lotSize")]
    pub lot_size: f64,
    #[serde(rename = "multiplier")]
    pub multiplier: f64,
    #[serde(rename = "firstCurrency")]
    pub first_currency: String,
}
impl JsNumberOutput for Instrument {
    fn check_number_output(&self) {
        js_number_output!(self, struct { r#type, symbol, version, description, price_increment, pip_size, currency, lot_size, multiplier, first_currency });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentsResponse {
    #[serde(rename = "instruments")]
    pub instruments: Vec<Instrument>,
}
impl JsNumberOutput for InstrumentsResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { instruments });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaxOrderSize {
    #[serde(rename = "value")]
    pub value: f64,
    #[serde(rename = "limitType")]
    pub limit_type: String,
}
impl JsNumberOutput for MaxOrderSize {
    fn check_number_output(&self) {
        js_number_output!(self, struct { value, limit_type });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarginTier {
    #[serde(rename = "endVolume")]
    pub end_volume: String,
    #[serde(rename = "value")]
    pub value: f64,
}
impl JsNumberOutput for MarginTier {
    fn check_number_output(&self) {
        js_number_output!(self, struct { end_volume, value });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarginRate {
    #[serde(rename = "rateType")]
    pub rate_type: String,
    #[serde(rename = "tiers")]
    pub tiers: Vec<MarginTier>,
}
impl JsNumberOutput for MarginRate {
    fn check_number_output(&self) {
        js_number_output!(self, struct { rate_type, tiers });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentDetails {
    #[serde(rename = "account")]
    pub account: String,
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "tradingStatus")]
    pub trading_status: String,
    #[serde(rename = "minOrderSize")]
    pub min_order_size: f64,
    #[serde(rename = "maxOrderSize")]
    pub max_order_size: MaxOrderSize,
    #[serde(rename = "minOrderSizeIncrement")]
    pub min_order_size_increment: f64,
    #[serde(rename = "marginRate")]
    pub margin_rate: MarginRate,
}
impl JsNumberOutput for InstrumentDetails {
    fn check_number_output(&self) {
        js_number_output!(self, struct { account, r#type, symbol, trading_status, min_order_size, max_order_size, min_order_size_increment, margin_rate });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentDetailsResponse {
    #[serde(rename = "instrumentDetails")]
    pub instrument_details: Vec<InstrumentDetails>,
}
impl JsNumberOutput for InstrumentDetailsResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { instrument_details });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteEvent {
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(rename = "ask")]
    pub ask: f64,
    #[serde(rename = "bid")]
    pub bid: f64,
    #[serde(rename = "time")]
    pub time: String,
}
impl JsNumberOutput for QuoteEvent {
    fn check_number_output(&self) {
        js_number_output!(self, struct { symbol, r#type, ask, bid, time });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickerResponse {
    #[serde(rename = "events")]
    pub events: Vec<QuoteEvent>,
}
impl JsNumberOutput for TickerResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { events });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandleEvent {
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(rename = "candleType")]
    pub candle_type: String,
    #[serde(rename = "open")]
    pub open: f64,
    #[serde(rename = "close")]
    pub close: f64,
    #[serde(rename = "high")]
    pub high: f64,
    #[serde(rename = "low")]
    pub low: f64,
    #[serde(rename = "volume")]
    pub volume: f64,
    #[serde(rename = "time")]
    pub time: String,
}
impl JsNumberOutput for CandleEvent {
    fn check_number_output(&self) {
        js_number_output!(self, struct { symbol, r#type, candle_type, open, close, high, low, volume, time });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandleResponse {
    #[serde(rename = "events")]
    pub events: Vec<CandleEvent>,
}
impl JsNumberOutput for CandleResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { events });
    }
}

fn decode_cash_transactions<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<serde_json::Value, D::Error> {
    let value: serde_json::Value =
        safe_cex_num::numeric_ingress::js_deserialize_passthrough(decoder)?;
    match value {
        serde_json::Value::Array(items) => Ok(serde_json::Value::Array(items)),
        _ => Err(serde::de::Error::custom(
            "cashTransactions must be an array",
        )),
    }
}
