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

#[derive(Debug, Clone)]
pub struct LoginResponse {
    pub session_token: String,
    pub timeout: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "LoginResponse")]
struct LoginResponseWire {
    #[serde(rename = "sessionToken")]
    pub session_token: String,
    #[serde(rename = "timeout")]
    pub timeout: String,
}

impl Serialize for LoginResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        LoginResponseWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for LoginResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        LoginResponseWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for LoginResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { session_token, timeout });
    }
}

#[derive(Debug, Clone)]
pub struct UserAccount {
    pub owner: String,
    pub account: String,
    pub version: f64,
    pub base_currency: String,
    pub registration_time: String,
    pub account_status: String,
    pub pricing_stream: String,
    pub position_based: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "UserAccount")]
struct UserAccountWire {
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

impl Serialize for UserAccount {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        UserAccountWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for UserAccount {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        UserAccountWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for UserAccount {
    fn check_number_output(&self) {
        js_number_output!(self, struct { owner, account, version, base_currency, registration_time, account_status, pricing_stream, position_based });
    }
}

#[derive(Debug, Clone)]
pub struct UserDetail {
    pub login: String,
    pub domain: String,
    pub version: f64,
    pub full_name: String,
    pub accounts: Vec<UserAccount>,
    pub username: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "UserDetail")]
struct UserDetailWire {
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

impl Serialize for UserDetail {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        UserDetailWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for UserDetail {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        UserDetailWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for UserDetail {
    fn check_number_output(&self) {
        js_number_output!(self, struct { login, domain, version, full_name, accounts, username });
    }
}

#[derive(Debug, Clone)]
pub struct UserResponse {
    pub user_details: Vec<UserDetail>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "UserResponse")]
struct UserResponseWire {
    #[serde(rename = "userDetails")]
    pub user_details: Vec<UserDetail>,
}

impl Serialize for UserResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        UserResponseWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for UserResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        UserResponseWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for UserResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { user_details });
    }
}

#[derive(Debug, Clone)]
pub struct PortfolioBalance {
    pub account: String,
    pub version: f64,
    pub value: f64,
    pub currency: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "PortfolioBalance")]
struct PortfolioBalanceWire {
    #[serde(rename = "account")]
    pub account: String,
    #[serde(rename = "version")]
    pub version: f64,
    #[serde(rename = "value")]
    pub value: f64,
    #[serde(rename = "currency")]
    pub currency: String,
}

impl Serialize for PortfolioBalance {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        PortfolioBalanceWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for PortfolioBalance {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        PortfolioBalanceWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for PortfolioBalance {
    fn check_number_output(&self) {
        js_number_output!(self, struct { account, version, value, currency });
    }
}

#[derive(Debug, Clone)]
pub struct PortfolioPosition {
    pub account: String,
    pub version: f64,
    pub position_code: String,
    pub symbol: String,
    pub quantity: f64,
    pub side: String,
    pub quantity_notional: f64,
    pub open_time: String,
    pub open_price: f64,
    pub last_update_time: String,
    pub margin_rate: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "PortfolioPosition")]
struct PortfolioPositionWire {
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

impl Serialize for PortfolioPosition {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        PortfolioPositionWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for PortfolioPosition {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        PortfolioPositionWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for PortfolioPosition {
    fn check_number_output(&self) {
        js_number_output!(self, struct { account, version, position_code, symbol, quantity, side, quantity_notional, open_time, open_price, last_update_time, margin_rate });
    }
}

#[derive(Debug, Clone)]
pub struct OrderLeg {
    pub instrument: String,
    pub position_effect: String,
    pub position_code: String,
    pub side: String,
    pub price: f64,
    pub leg_ratio: f64,
    pub quantity: f64,
    pub filled_quantity: f64,
    pub remaining_quantity: f64,
    pub average_price: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderLeg")]
struct OrderLegWire {
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

impl Serialize for OrderLeg {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderLegWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for OrderLeg {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        OrderLegWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for OrderLeg {
    fn check_number_output(&self) {
        js_number_output!(self, struct { instrument, position_effect, position_code, side, price, leg_ratio, quantity, filled_quantity, remaining_quantity, average_price });
    }
}

#[derive(Debug, Clone)]
pub struct OrderExecution {
    pub account: String,
    pub execution_code: String,
    pub order_code: String,
    pub update_order_id: f64,
    pub version: f64,
    pub client_order_id: String,
    pub action_code: String,
    pub status: String,
    pub final_status: bool,
    pub filled_quantity: f64,
    pub last_quantity: f64,
    pub filled_quantity_notional: f64,
    pub last_quantity_notional: f64,
    pub transaction_time: String,
    pub margin_rate: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "OrderExecution")]
struct OrderExecutionWire {
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

impl Serialize for OrderExecution {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OrderExecutionWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for OrderExecution {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        OrderExecutionWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for OrderExecution {
    fn check_number_output(&self) {
        js_number_output!(self, struct { account, execution_code, order_code, update_order_id, version, client_order_id, action_code, status, final_status, filled_quantity, last_quantity, filled_quantity_notional, last_quantity_notional, transaction_time, margin_rate });
    }
}

#[derive(Debug, Clone)]
pub struct PortfolioOrder {
    pub account: String,
    pub version: f64,
    pub order_id: f64,
    pub order_code: String,
    pub client_order_id: String,
    pub action_code: String,
    pub leg_count: f64,
    pub r#type: String,
    pub instrument: String,
    pub status: String,
    pub final_status: bool,
    pub legs: Vec<OrderLeg>,
    pub side: String,
    pub tif: String,
    pub margin_rate: f64,
    pub issue_time: String,
    pub transaction_time: String,
    pub executions: Vec<OrderExecution>,
    pub cash_transactions: serde_json::Value,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "PortfolioOrder")]
struct PortfolioOrderWire {
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

impl Serialize for PortfolioOrder {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        PortfolioOrderWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for PortfolioOrder {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        PortfolioOrderWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for PortfolioOrder {
    fn check_number_output(&self) {
        js_number_output!(self, struct { account, version, order_id, order_code, client_order_id, action_code, leg_count, r#type, instrument, status, final_status, legs, side, tif, margin_rate, issue_time, transaction_time, executions } passthrough { cash_transactions });
    }
}

#[derive(Debug, Clone)]
pub struct Portfolio {
    pub account: String,
    pub version: f64,
    pub balances: Vec<PortfolioBalance>,
    pub positions: Vec<PortfolioPosition>,
    pub orders: Vec<PortfolioOrder>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "Portfolio")]
struct PortfolioWire {
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

impl Serialize for Portfolio {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        PortfolioWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for Portfolio {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        PortfolioWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for Portfolio {
    fn check_number_output(&self) {
        js_number_output!(self, struct { account, version, balances, positions, orders });
    }
}

#[derive(Debug, Clone)]
pub struct PortfolioResponse {
    pub portfolios: Vec<Portfolio>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "PortfolioResponse")]
struct PortfolioResponseWire {
    #[serde(rename = "portfolios")]
    pub portfolios: Vec<Portfolio>,
}

impl Serialize for PortfolioResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        PortfolioResponseWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for PortfolioResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        PortfolioResponseWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for PortfolioResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { portfolios });
    }
}

#[derive(Debug, Clone)]
pub struct Instrument {
    pub r#type: String,
    pub symbol: String,
    pub version: f64,
    pub description: String,
    pub price_increment: f64,
    pub pip_size: f64,
    pub currency: String,
    pub lot_size: f64,
    pub multiplier: f64,
    pub first_currency: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "Instrument")]
struct InstrumentWire {
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

impl Serialize for Instrument {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        InstrumentWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for Instrument {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        InstrumentWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for Instrument {
    fn check_number_output(&self) {
        js_number_output!(self, struct { r#type, symbol, version, description, price_increment, pip_size, currency, lot_size, multiplier, first_currency });
    }
}

#[derive(Debug, Clone)]
pub struct InstrumentsResponse {
    pub instruments: Vec<Instrument>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "InstrumentsResponse")]
struct InstrumentsResponseWire {
    #[serde(rename = "instruments")]
    pub instruments: Vec<Instrument>,
}

impl Serialize for InstrumentsResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        InstrumentsResponseWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for InstrumentsResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        InstrumentsResponseWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for InstrumentsResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { instruments });
    }
}

#[derive(Debug, Clone)]
pub struct MaxOrderSize {
    pub value: f64,
    pub limit_type: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "MaxOrderSize")]
struct MaxOrderSizeWire {
    #[serde(rename = "value")]
    pub value: f64,
    #[serde(rename = "limitType")]
    pub limit_type: String,
}

impl Serialize for MaxOrderSize {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        MaxOrderSizeWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for MaxOrderSize {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        MaxOrderSizeWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for MaxOrderSize {
    fn check_number_output(&self) {
        js_number_output!(self, struct { value, limit_type });
    }
}

#[derive(Debug, Clone)]
pub struct MarginTier {
    pub end_volume: String,
    pub value: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "MarginTier")]
struct MarginTierWire {
    #[serde(rename = "endVolume")]
    pub end_volume: String,
    #[serde(rename = "value")]
    pub value: f64,
}

impl Serialize for MarginTier {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        MarginTierWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for MarginTier {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        MarginTierWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for MarginTier {
    fn check_number_output(&self) {
        js_number_output!(self, struct { end_volume, value });
    }
}

#[derive(Debug, Clone)]
pub struct MarginRate {
    pub rate_type: String,
    pub tiers: Vec<MarginTier>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "MarginRate")]
struct MarginRateWire {
    #[serde(rename = "rateType")]
    pub rate_type: String,
    #[serde(rename = "tiers")]
    pub tiers: Vec<MarginTier>,
}

impl Serialize for MarginRate {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        MarginRateWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for MarginRate {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        MarginRateWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for MarginRate {
    fn check_number_output(&self) {
        js_number_output!(self, struct { rate_type, tiers });
    }
}

#[derive(Debug, Clone)]
pub struct InstrumentDetails {
    pub account: String,
    pub r#type: String,
    pub symbol: String,
    pub trading_status: String,
    pub min_order_size: f64,
    pub max_order_size: MaxOrderSize,
    pub min_order_size_increment: f64,
    pub margin_rate: MarginRate,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "InstrumentDetails")]
struct InstrumentDetailsWire {
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

impl Serialize for InstrumentDetails {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        InstrumentDetailsWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for InstrumentDetails {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        InstrumentDetailsWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for InstrumentDetails {
    fn check_number_output(&self) {
        js_number_output!(self, struct { account, r#type, symbol, trading_status, min_order_size, max_order_size, min_order_size_increment, margin_rate });
    }
}

#[derive(Debug, Clone)]
pub struct InstrumentDetailsResponse {
    pub instrument_details: Vec<InstrumentDetails>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "InstrumentDetailsResponse")]
struct InstrumentDetailsResponseWire {
    #[serde(rename = "instrumentDetails")]
    pub instrument_details: Vec<InstrumentDetails>,
}

impl Serialize for InstrumentDetailsResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        InstrumentDetailsResponseWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for InstrumentDetailsResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        InstrumentDetailsResponseWire::deserialize(safe_cex_types::strict_decode::object(
            deserializer,
        ))
    }
}

impl JsNumberOutput for InstrumentDetailsResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { instrument_details });
    }
}

#[derive(Debug, Clone)]
pub struct QuoteEvent {
    pub symbol: String,
    pub r#type: String,
    pub ask: f64,
    pub bid: f64,
    pub time: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "QuoteEvent")]
struct QuoteEventWire {
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

impl Serialize for QuoteEvent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        QuoteEventWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for QuoteEvent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        QuoteEventWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for QuoteEvent {
    fn check_number_output(&self) {
        js_number_output!(self, struct { symbol, r#type, ask, bid, time });
    }
}

#[derive(Debug, Clone)]
pub struct TickerResponse {
    pub events: Vec<QuoteEvent>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "TickerResponse")]
struct TickerResponseWire {
    #[serde(rename = "events")]
    pub events: Vec<QuoteEvent>,
}

impl Serialize for TickerResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        TickerResponseWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for TickerResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        TickerResponseWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for TickerResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { events });
    }
}

#[derive(Debug, Clone)]
pub struct CandleEvent {
    pub symbol: String,
    pub r#type: String,
    pub candle_type: String,
    pub open: f64,
    pub close: f64,
    pub high: f64,
    pub low: f64,
    pub volume: f64,
    pub time: String,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "CandleEvent")]
struct CandleEventWire {
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

impl Serialize for CandleEvent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        CandleEventWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for CandleEvent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        CandleEventWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

impl JsNumberOutput for CandleEvent {
    fn check_number_output(&self) {
        js_number_output!(self, struct { symbol, r#type, candle_type, open, close, high, low, volume, time });
    }
}

#[derive(Debug, Clone)]
pub struct CandleResponse {
    pub events: Vec<CandleEvent>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "CandleResponse")]
struct CandleResponseWire {
    #[serde(rename = "events")]
    pub events: Vec<CandleEvent>,
}

impl Serialize for CandleResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        CandleResponseWire::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for CandleResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        CandleResponseWire::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
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
