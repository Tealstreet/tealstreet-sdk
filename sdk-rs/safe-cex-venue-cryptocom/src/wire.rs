//! Wire records from cryptocom.types.ts. Decode JSON through shared numeric ingress.
use safe_cex_num::{js_number_output, numeric_ingress::JsNumberOutput};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct CryptocomInstrument {
    pub symbol: String,
    pub inst_type: String,
    pub display_name: String,
    pub base_ccy: String,
    pub quote_ccy: String,
    pub quote_decimals: f64,
    pub quantity_decimals: f64,
    pub price_tick_size: String,
    pub qty_tick_size: String,
    pub max_leverage: String,
    pub tradable: bool,
    pub expiry_timestamp_ms: f64,
    pub beta_product: bool,
    pub underlying_symbol: String,
    pub contract_size: String,
    pub margin_buy_enabled: bool,
    pub margin_sell_enabled: bool,
    /// Unknown own JSON fields, retained through the shared numeric capture.
    pub additional_fields: serde_json::Value,
    pub(crate) input_keys: Vec<String>,
}

impl JsNumberOutput for CryptocomInstrument {
    fn check_number_output(&self) {
        js_number_output!(self, struct { symbol, inst_type, display_name, base_ccy, quote_ccy, quote_decimals, quantity_decimals, price_tick_size, qty_tick_size, max_leverage, tradable, expiry_timestamp_ms, beta_product, underlying_symbol, contract_size, margin_buy_enabled, margin_sell_enabled, input_keys } passthrough { additional_fields });
    }
}

#[derive(Deserialize)]
#[serde(remote = "CryptocomInstrument")]
struct WireCryptocomInstrument {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "inst_type")]
    inst_type: String,
    #[serde(rename = "display_name")]
    display_name: String,
    #[serde(rename = "base_ccy")]
    base_ccy: String,
    #[serde(rename = "quote_ccy")]
    quote_ccy: String,
    #[serde(rename = "quote_decimals")]
    quote_decimals: f64,
    #[serde(rename = "quantity_decimals")]
    quantity_decimals: f64,
    #[serde(rename = "price_tick_size")]
    price_tick_size: String,
    #[serde(rename = "qty_tick_size")]
    qty_tick_size: String,
    #[serde(rename = "max_leverage")]
    max_leverage: String,
    #[serde(rename = "tradable")]
    tradable: bool,
    #[serde(rename = "expiry_timestamp_ms")]
    expiry_timestamp_ms: f64,
    #[serde(rename = "beta_product")]
    beta_product: bool,
    #[serde(rename = "underlying_symbol")]
    underlying_symbol: String,
    #[serde(rename = "contract_size")]
    contract_size: String,
    #[serde(rename = "margin_buy_enabled")]
    margin_buy_enabled: bool,
    #[serde(rename = "margin_sell_enabled")]
    margin_sell_enabled: bool,
    #[serde(skip)]
    additional_fields: serde_json::Value,
    #[serde(skip)]
    input_keys: Vec<String>,
}

impl<'de> Deserialize<'de> for CryptocomInstrument {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (input, fields) = crate::wire_record::capture(
            deserializer,
            &[
                "symbol",
                "inst_type",
                "display_name",
                "base_ccy",
                "quote_ccy",
                "quote_decimals",
                "quantity_decimals",
                "price_tick_size",
                "qty_tick_size",
                "max_leverage",
                "tradable",
                "expiry_timestamp_ms",
                "beta_product",
                "underlying_symbol",
                "contract_size",
                "margin_buy_enabled",
                "margin_sell_enabled",
            ],
        )?;
        input
            .decode_with(|deserializer| {
                let mut record = WireCryptocomInstrument::deserialize(
                    safe_cex_types::strict_decode::object(deserializer),
                )?;
                record.additional_fields = fields.additional;
                record.input_keys = fields.keys;
                Ok(record)
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CryptocomTicker {
    pub i: String,
    pub b: String,
    pub k: String,
    pub a: String,
    pub c: String,
    pub h: String,
    pub l: String,
    pub v: String,
    pub vv: String,
    pub oi: String,
    pub t: f64,
    /// Unknown own JSON fields, retained through the shared numeric capture.
    pub additional_fields: serde_json::Value,
    pub(crate) input_keys: Vec<String>,
}

impl JsNumberOutput for CryptocomTicker {
    fn check_number_output(&self) {
        js_number_output!(self, struct { i, b, k, a, c, h, l, v, vv, oi, t, input_keys } passthrough { additional_fields });
    }
}

#[derive(Deserialize)]
#[serde(remote = "CryptocomTicker")]
struct WireCryptocomTicker {
    #[serde(rename = "i")]
    i: String,
    #[serde(rename = "b")]
    b: String,
    #[serde(rename = "k")]
    k: String,
    #[serde(rename = "a")]
    a: String,
    #[serde(rename = "c")]
    c: String,
    #[serde(rename = "h")]
    h: String,
    #[serde(rename = "l")]
    l: String,
    #[serde(rename = "v")]
    v: String,
    #[serde(rename = "vv")]
    vv: String,
    #[serde(rename = "oi")]
    oi: String,
    #[serde(rename = "t")]
    t: f64,
    #[serde(skip)]
    additional_fields: serde_json::Value,
    #[serde(skip)]
    input_keys: Vec<String>,
}

impl<'de> Deserialize<'de> for CryptocomTicker {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (input, fields) = crate::wire_record::capture(
            deserializer,
            &["i", "b", "k", "a", "c", "h", "l", "v", "vv", "oi", "t"],
        )?;
        input
            .decode_with(|deserializer| {
                let mut record = WireCryptocomTicker::deserialize(
                    safe_cex_types::strict_decode::object(deserializer),
                )?;
                record.additional_fields = fields.additional;
                record.input_keys = fields.keys;
                Ok(record)
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CryptocomCandlestick {
    pub t: f64,
    pub o: String,
    pub h: String,
    pub l: String,
    pub c: String,
    pub v: String,
    /// Unknown own JSON fields, retained through the shared numeric capture.
    pub additional_fields: serde_json::Value,
    pub(crate) input_keys: Vec<String>,
}

impl JsNumberOutput for CryptocomCandlestick {
    fn check_number_output(&self) {
        js_number_output!(self, struct { t, o, h, l, c, v, input_keys } passthrough { additional_fields });
    }
}

#[derive(Deserialize)]
#[serde(remote = "CryptocomCandlestick")]
struct WireCryptocomCandlestick {
    #[serde(rename = "t")]
    t: f64,
    #[serde(rename = "o")]
    o: String,
    #[serde(rename = "h")]
    h: String,
    #[serde(rename = "l")]
    l: String,
    #[serde(rename = "c")]
    c: String,
    #[serde(rename = "v")]
    v: String,
    #[serde(skip)]
    additional_fields: serde_json::Value,
    #[serde(skip)]
    input_keys: Vec<String>,
}

impl<'de> Deserialize<'de> for CryptocomCandlestick {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (input, fields) =
            crate::wire_record::capture(deserializer, &["t", "o", "h", "l", "c", "v"])?;
        input
            .decode_with(|deserializer| {
                let mut record = WireCryptocomCandlestick::deserialize(
                    safe_cex_types::strict_decode::object(deserializer),
                )?;
                record.additional_fields = fields.additional;
                record.input_keys = fields.keys;
                Ok(record)
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CryptocomPositionBalance {
    pub instrument_name: String,
    pub quantity: String,
    pub reserved_qty: String,
    pub collateral_amount: String,
    pub collateral_eligible: bool,
    pub haircut: String,
    pub market_value: String,
    pub max_withdrawal_balance: String,
    pub hourly_interest_rate: String,
    /// Unknown own JSON fields, retained through the shared numeric capture.
    pub additional_fields: serde_json::Value,
    pub(crate) input_keys: Vec<String>,
}

impl JsNumberOutput for CryptocomPositionBalance {
    fn check_number_output(&self) {
        js_number_output!(self, struct { instrument_name, quantity, reserved_qty, collateral_amount, collateral_eligible, haircut, market_value, max_withdrawal_balance, hourly_interest_rate, input_keys } passthrough { additional_fields });
    }
}

#[derive(Deserialize)]
#[serde(remote = "CryptocomPositionBalance")]
struct WireCryptocomPositionBalance {
    #[serde(rename = "instrument_name")]
    instrument_name: String,
    #[serde(rename = "quantity")]
    quantity: String,
    #[serde(rename = "reserved_qty")]
    reserved_qty: String,
    #[serde(rename = "collateral_amount")]
    collateral_amount: String,
    #[serde(rename = "collateral_eligible")]
    collateral_eligible: bool,
    #[serde(rename = "haircut")]
    haircut: String,
    #[serde(rename = "market_value")]
    market_value: String,
    #[serde(rename = "max_withdrawal_balance")]
    max_withdrawal_balance: String,
    #[serde(rename = "hourly_interest_rate")]
    hourly_interest_rate: String,
    #[serde(skip)]
    additional_fields: serde_json::Value,
    #[serde(skip)]
    input_keys: Vec<String>,
}

impl<'de> Deserialize<'de> for CryptocomPositionBalance {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (input, fields) = crate::wire_record::capture(
            deserializer,
            &[
                "instrument_name",
                "quantity",
                "reserved_qty",
                "collateral_amount",
                "collateral_eligible",
                "haircut",
                "market_value",
                "max_withdrawal_balance",
                "hourly_interest_rate",
            ],
        )?;
        input
            .decode_with(|deserializer| {
                let mut record = WireCryptocomPositionBalance::deserialize(
                    safe_cex_types::strict_decode::object(deserializer),
                )?;
                record.additional_fields = fields.additional;
                record.input_keys = fields.keys;
                Ok(record)
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CryptocomError {
    pub code: f64,
    pub message: String,
    /// Unknown own JSON fields, retained through the shared numeric capture.
    pub additional_fields: serde_json::Value,
    pub(crate) input_keys: Vec<String>,
}

impl JsNumberOutput for CryptocomError {
    fn check_number_output(&self) {
        js_number_output!(self, struct { code, message, input_keys } passthrough { additional_fields });
    }
}

#[derive(Deserialize)]
#[serde(remote = "CryptocomError")]
struct WireCryptocomError {
    #[serde(rename = "code")]
    code: f64,
    #[serde(rename = "message")]
    message: String,
    #[serde(skip)]
    additional_fields: serde_json::Value,
    #[serde(skip)]
    input_keys: Vec<String>,
}

impl<'de> Deserialize<'de> for CryptocomError {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (input, fields) = crate::wire_record::capture(deserializer, &["code", "message"])?;
        input
            .decode_with(|deserializer| {
                let mut record = WireCryptocomError::deserialize(
                    safe_cex_types::strict_decode::object(deserializer),
                )?;
                record.additional_fields = fields.additional;
                record.input_keys = fields.keys;
                Ok(record)
            })
            .map_err(serde::de::Error::custom)
    }
}

/// Wire spelling of the inline CryptocomTrade side union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CryptocomTradeSide {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}

#[derive(Deserialize)]
#[serde(remote = "CryptocomTradeSide")]
enum WireCryptocomTradeSide {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}

impl<'de> Deserialize<'de> for CryptocomTradeSide {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // A TS string-literal union cannot accept Serde's externally tagged maps.
        let value = String::deserialize(deserializer)?;
        WireCryptocomTradeSide::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(
            &value,
        ))
    }
}

impl JsNumberOutput for CryptocomTradeSide {
    fn check_number_output(&self) {
        js_number_output!(self, enum { Buy, Sell });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CryptocomTrade {
    pub d: f64,
    pub t: f64,
    pub p: f64,
    pub q: f64,
    pub s: CryptocomTradeSide,
    pub i: String,
    pub data_time: f64,
    /// Unknown own JSON fields, retained through the shared numeric capture.
    pub additional_fields: serde_json::Value,
    pub(crate) input_keys: Vec<String>,
}

impl JsNumberOutput for CryptocomTrade {
    fn check_number_output(&self) {
        js_number_output!(self, struct { d, t, p, q, s, i, data_time, input_keys } passthrough { additional_fields });
    }
}

#[derive(Deserialize)]
#[serde(remote = "CryptocomTrade")]
struct WireCryptocomTrade {
    #[serde(rename = "d")]
    d: f64,
    #[serde(rename = "t")]
    t: f64,
    #[serde(rename = "p")]
    p: f64,
    #[serde(rename = "q")]
    q: f64,
    #[serde(rename = "s")]
    s: CryptocomTradeSide,
    #[serde(rename = "i")]
    i: String,
    #[serde(rename = "dataTime")]
    data_time: f64,
    #[serde(skip)]
    additional_fields: serde_json::Value,
    #[serde(skip)]
    input_keys: Vec<String>,
}

impl<'de> Deserialize<'de> for CryptocomTrade {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (input, fields) =
            crate::wire_record::capture(deserializer, &["d", "t", "p", "q", "s", "i", "dataTime"])?;
        input
            .decode_with(|deserializer| {
                let mut record = WireCryptocomTrade::deserialize(
                    safe_cex_types::strict_decode::object(deserializer),
                )?;
                record.additional_fields = fields.additional;
                record.input_keys = fields.keys;
                Ok(record)
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CryptocomAccountIdentity {
    pub uuid: String,
    /// Unknown own JSON fields, retained through the shared numeric capture.
    pub additional_fields: serde_json::Value,
    pub(crate) input_keys: Vec<String>,
}

impl JsNumberOutput for CryptocomAccountIdentity {
    fn check_number_output(&self) {
        js_number_output!(self, struct { uuid, input_keys } passthrough { additional_fields });
    }
}

#[derive(Deserialize)]
#[serde(remote = "CryptocomAccountIdentity")]
struct WireCryptocomAccountIdentity {
    #[serde(rename = "uuid")]
    uuid: String,
    #[serde(skip)]
    additional_fields: serde_json::Value,
    #[serde(skip)]
    input_keys: Vec<String>,
}

impl<'de> Deserialize<'de> for CryptocomAccountIdentity {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (input, fields) = crate::wire_record::capture(deserializer, &["uuid"])?;
        input
            .decode_with(|deserializer| {
                let mut record = WireCryptocomAccountIdentity::deserialize(
                    safe_cex_types::strict_decode::object(deserializer),
                )?;
                record.additional_fields = fields.additional;
                record.input_keys = fields.keys;
                Ok(record)
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CryptocomAccountsResponse {
    pub this_account: CryptocomAccountIdentity,
    /// Unknown own JSON fields, retained through the shared numeric capture.
    pub additional_fields: serde_json::Value,
    pub(crate) input_keys: Vec<String>,
}

impl JsNumberOutput for CryptocomAccountsResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { this_account, input_keys } passthrough { additional_fields });
    }
}

#[derive(Deserialize)]
#[serde(remote = "CryptocomAccountsResponse")]
struct WireCryptocomAccountsResponse {
    #[serde(rename = "this_account")]
    this_account: CryptocomAccountIdentity,
    #[serde(skip)]
    additional_fields: serde_json::Value,
    #[serde(skip)]
    input_keys: Vec<String>,
}

impl<'de> Deserialize<'de> for CryptocomAccountsResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (input, fields) = crate::wire_record::capture(deserializer, &["this_account"])?;
        input
            .decode_with(|deserializer| {
                let mut record = WireCryptocomAccountsResponse::deserialize(
                    safe_cex_types::strict_decode::object(deserializer),
                )?;
                record.additional_fields = fields.additional;
                record.input_keys = fields.keys;
                Ok(record)
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CryptocomBalanceSummary {
    pub total_available_balance: String,
    pub total_margin_balance: String,
    pub total_initial_margin: String,
    pub total_maintenance_margin: String,
    pub total_cash_balance: String,
    pub total_collateral_value: String,
    pub total_session_unrealized_pnl: String,
    pub total_session_realized_pnl: String,
    pub total_position_cost: String,
    pub instrument_name: String,
    pub position_balances: Vec<CryptocomPositionBalance>,
    /// Unknown own JSON fields, retained through the shared numeric capture.
    pub additional_fields: serde_json::Value,
    pub(crate) input_keys: Vec<String>,
}

impl JsNumberOutput for CryptocomBalanceSummary {
    fn check_number_output(&self) {
        js_number_output!(self, struct { total_available_balance, total_margin_balance, total_initial_margin, total_maintenance_margin, total_cash_balance, total_collateral_value, total_session_unrealized_pnl, total_session_realized_pnl, total_position_cost, instrument_name, position_balances, input_keys } passthrough { additional_fields });
    }
}

#[derive(Deserialize)]
#[serde(remote = "CryptocomBalanceSummary")]
struct WireCryptocomBalanceSummary {
    #[serde(rename = "total_available_balance")]
    total_available_balance: String,
    #[serde(rename = "total_margin_balance")]
    total_margin_balance: String,
    #[serde(rename = "total_initial_margin")]
    total_initial_margin: String,
    #[serde(rename = "total_maintenance_margin")]
    total_maintenance_margin: String,
    #[serde(rename = "total_cash_balance")]
    total_cash_balance: String,
    #[serde(rename = "total_collateral_value")]
    total_collateral_value: String,
    #[serde(rename = "total_session_unrealized_pnl")]
    total_session_unrealized_pnl: String,
    #[serde(rename = "total_session_realized_pnl")]
    total_session_realized_pnl: String,
    #[serde(rename = "total_position_cost")]
    total_position_cost: String,
    #[serde(rename = "instrument_name")]
    instrument_name: String,
    #[serde(rename = "position_balances")]
    position_balances: Vec<CryptocomPositionBalance>,
    #[serde(skip)]
    additional_fields: serde_json::Value,
    #[serde(skip)]
    input_keys: Vec<String>,
}

impl<'de> Deserialize<'de> for CryptocomBalanceSummary {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (input, fields) = crate::wire_record::capture(
            deserializer,
            &[
                "total_available_balance",
                "total_margin_balance",
                "total_initial_margin",
                "total_maintenance_margin",
                "total_cash_balance",
                "total_collateral_value",
                "total_session_unrealized_pnl",
                "total_session_realized_pnl",
                "total_position_cost",
                "instrument_name",
                "position_balances",
            ],
        )?;
        input
            .decode_with(|deserializer| {
                let mut record = WireCryptocomBalanceSummary::deserialize(
                    safe_cex_types::strict_decode::object(deserializer),
                )?;
                record.additional_fields = fields.additional;
                record.input_keys = fields.keys;
                Ok(record)
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CryptocomBookData {
    pub bids: Vec<[String; 3]>,
    pub asks: Vec<[String; 3]>,
    pub t: f64,
    /// Unknown own JSON fields, retained through the shared numeric capture.
    pub additional_fields: serde_json::Value,
    pub(crate) input_keys: Vec<String>,
}

impl JsNumberOutput for CryptocomBookData {
    fn check_number_output(&self) {
        js_number_output!(self, struct { bids, asks, t, input_keys } passthrough { additional_fields });
    }
}

#[derive(Deserialize)]
#[serde(remote = "CryptocomBookData")]
struct WireCryptocomBookData {
    #[serde(rename = "bids")]
    bids: Vec<[String; 3]>,
    #[serde(rename = "asks")]
    asks: Vec<[String; 3]>,
    #[serde(rename = "t")]
    t: f64,
    #[serde(skip)]
    additional_fields: serde_json::Value,
    #[serde(skip)]
    input_keys: Vec<String>,
}

impl<'de> Deserialize<'de> for CryptocomBookData {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (input, fields) = crate::wire_record::capture(deserializer, &["bids", "asks", "t"])?;
        input
            .decode_with(|deserializer| {
                let mut record = WireCryptocomBookData::deserialize(
                    safe_cex_types::strict_decode::object(deserializer),
                )?;
                record.additional_fields = fields.additional;
                record.input_keys = fields.keys;
                Ok(record)
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CryptocomBook {
    pub instrument_name: String,
    pub depth: f64,
    pub data: Vec<CryptocomBookData>,
    /// Unknown own JSON fields, retained through the shared numeric capture.
    pub additional_fields: serde_json::Value,
    pub(crate) input_keys: Vec<String>,
}

impl JsNumberOutput for CryptocomBook {
    fn check_number_output(&self) {
        js_number_output!(self, struct { instrument_name, depth, data, input_keys } passthrough { additional_fields });
    }
}

#[derive(Deserialize)]
#[serde(remote = "CryptocomBook")]
struct WireCryptocomBook {
    #[serde(rename = "instrument_name")]
    instrument_name: String,
    #[serde(rename = "depth")]
    depth: f64,
    #[serde(rename = "data")]
    data: Vec<CryptocomBookData>,
    #[serde(skip)]
    additional_fields: serde_json::Value,
    #[serde(skip)]
    input_keys: Vec<String>,
}

impl<'de> Deserialize<'de> for CryptocomBook {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (input, fields) =
            crate::wire_record::capture(deserializer, &["instrument_name", "depth", "data"])?;
        input
            .decode_with(|deserializer| {
                let mut record = WireCryptocomBook::deserialize(
                    safe_cex_types::strict_decode::object(deserializer),
                )?;
                record.additional_fields = fields.additional;
                record.input_keys = fields.keys;
                Ok(record)
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CryptocomJournalType {
    #[serde(rename = "TRADING")]
    Trading,
}

#[derive(Deserialize)]
#[serde(remote = "CryptocomJournalType")]
enum WireCryptocomJournalType {
    #[serde(rename = "TRADING")]
    Trading,
}

impl<'de> Deserialize<'de> for CryptocomJournalType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WireCryptocomJournalType::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(
            &value,
        ))
    }
}

impl JsNumberOutput for CryptocomJournalType {
    fn check_number_output(&self) {
        js_number_output!(self, enum { Trading });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CryptocomLiquidityIndicator {
    #[serde(rename = "MAKER")]
    Maker,
    #[serde(rename = "TAKER")]
    Taker,
}

#[derive(Deserialize)]
#[serde(remote = "CryptocomLiquidityIndicator")]
enum WireCryptocomLiquidityIndicator {
    #[serde(rename = "MAKER")]
    Maker,
    #[serde(rename = "TAKER")]
    Taker,
}

impl<'de> Deserialize<'de> for CryptocomLiquidityIndicator {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WireCryptocomLiquidityIndicator::deserialize(
            serde::de::value::StrDeserializer::<D::Error>::new(&value),
        )
    }
}

impl JsNumberOutput for CryptocomLiquidityIndicator {
    fn check_number_output(&self) {
        js_number_output!(self, enum { Maker, Taker });
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CryptocomTradeFill {
    pub account_id: String,
    pub event_date: f64,
    pub journal_type: CryptocomJournalType,
    pub traded_quantity: f64,
    pub traded_price: f64,
    pub transaction_qty: f64,
    pub transaction_cost: f64,
    pub order_id: String,
    pub trade_id: String,
    pub trade_match_id: String,
    pub client_oid: Option<String>,
    pub taker_side: CryptocomTradeSide,
    pub side: CryptocomTradeSide,
    pub instrument_name: String,
    pub fee: f64,
    pub fee_currency: String,
    pub create_time: f64,
    pub create_time_ns: String,
    pub liquidity_indicator: CryptocomLiquidityIndicator,
    /// Unknown own JSON fields, retained through the shared numeric capture.
    pub additional_fields: serde_json::Value,
    pub(crate) input_keys: Vec<String>,
}

impl JsNumberOutput for CryptocomTradeFill {
    fn check_number_output(&self) {
        js_number_output!(self, struct { account_id, event_date, journal_type, traded_quantity, traded_price, transaction_qty, transaction_cost, order_id, trade_id, trade_match_id, client_oid, taker_side, side, instrument_name, fee, fee_currency, create_time, create_time_ns, liquidity_indicator, input_keys } passthrough { additional_fields });
    }
}

fn deserialize_present_string<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
#[serde(remote = "CryptocomTradeFill")]
struct WireCryptocomTradeFill {
    #[serde(rename = "account_id")]
    account_id: String,
    #[serde(rename = "event_date")]
    event_date: f64,
    #[serde(rename = "journal_type")]
    journal_type: CryptocomJournalType,
    #[serde(rename = "traded_quantity")]
    traded_quantity: f64,
    #[serde(rename = "traded_price")]
    traded_price: f64,
    #[serde(rename = "transaction_qty")]
    transaction_qty: f64,
    #[serde(rename = "transaction_cost")]
    transaction_cost: f64,
    #[serde(rename = "order_id")]
    order_id: String,
    #[serde(rename = "trade_id")]
    trade_id: String,
    #[serde(rename = "trade_match_id")]
    trade_match_id: String,
    #[serde(
        rename = "client_oid",
        default,
        deserialize_with = "deserialize_present_string"
    )]
    client_oid: Option<String>,
    #[serde(rename = "taker_side")]
    taker_side: CryptocomTradeSide,
    #[serde(rename = "side")]
    side: CryptocomTradeSide,
    #[serde(rename = "instrument_name")]
    instrument_name: String,
    #[serde(rename = "fee")]
    fee: f64,
    #[serde(rename = "fee_currency")]
    fee_currency: String,
    #[serde(rename = "create_time")]
    create_time: f64,
    #[serde(rename = "create_time_ns")]
    create_time_ns: String,
    #[serde(rename = "liquidity_indicator")]
    liquidity_indicator: CryptocomLiquidityIndicator,
    #[serde(skip)]
    additional_fields: serde_json::Value,
    #[serde(skip)]
    input_keys: Vec<String>,
}

impl<'de> Deserialize<'de> for CryptocomTradeFill {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (input, fields) = crate::wire_record::capture(
            deserializer,
            &[
                "account_id",
                "event_date",
                "journal_type",
                "traded_quantity",
                "traded_price",
                "transaction_qty",
                "transaction_cost",
                "order_id",
                "trade_id",
                "trade_match_id",
                "client_oid",
                "taker_side",
                "side",
                "instrument_name",
                "fee",
                "fee_currency",
                "create_time",
                "create_time_ns",
                "liquidity_indicator",
            ],
        )?;
        input
            .decode_with(|deserializer| {
                let mut record = WireCryptocomTradeFill::deserialize(
                    safe_cex_types::strict_decode::object(deserializer),
                )?;
                record.additional_fields = fields.additional;
                record.input_keys = fields.keys;
                Ok(record)
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CryptocomOrderType {
    #[serde(rename = "LIMIT")]
    Limit,
    #[serde(rename = "MARKET")]
    Market,
    #[serde(rename = "STOP_LOSS")]
    StopLoss,
    #[serde(rename = "STOP_LIMIT")]
    StopLimit,
    #[serde(rename = "TAKE_PROFIT")]
    TakeProfit,
    #[serde(rename = "TAKE_PROFIT_LIMIT")]
    TakeProfitLimit,
}
#[derive(Deserialize)]
#[serde(remote = "CryptocomOrderType")]
enum WireCryptocomOrderType {
    #[serde(rename = "LIMIT")]
    Limit,
    #[serde(rename = "MARKET")]
    Market,
    #[serde(rename = "STOP_LOSS")]
    StopLoss,
    #[serde(rename = "STOP_LIMIT")]
    StopLimit,
    #[serde(rename = "TAKE_PROFIT")]
    TakeProfit,
    #[serde(rename = "TAKE_PROFIT_LIMIT")]
    TakeProfitLimit,
}
impl<'de> Deserialize<'de> for CryptocomOrderType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WireCryptocomOrderType::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(
            &value,
        ))
    }
}
impl JsNumberOutput for CryptocomOrderType {
    fn check_number_output(&self) {
        js_number_output!(self, enum { Limit, Market, StopLoss, StopLimit, TakeProfit, TakeProfitLimit });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CryptocomTimeInForce {
    #[serde(rename = "GOOD_TILL_CANCEL")]
    GoodTillCancel,
    #[serde(rename = "FILL_OR_KILL")]
    FillOrKill,
    #[serde(rename = "IMMEDIATE_OR_CANCEL")]
    ImmediateOrCancel,
    #[serde(rename = "POST_ONLY")]
    PostOnly,
}
#[derive(Deserialize)]
#[serde(remote = "CryptocomTimeInForce")]
enum WireCryptocomTimeInForce {
    #[serde(rename = "GOOD_TILL_CANCEL")]
    GoodTillCancel,
    #[serde(rename = "FILL_OR_KILL")]
    FillOrKill,
    #[serde(rename = "IMMEDIATE_OR_CANCEL")]
    ImmediateOrCancel,
    #[serde(rename = "POST_ONLY")]
    PostOnly,
}
impl<'de> Deserialize<'de> for CryptocomTimeInForce {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WireCryptocomTimeInForce::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(
            &value,
        ))
    }
}
impl JsNumberOutput for CryptocomTimeInForce {
    fn check_number_output(&self) {
        js_number_output!(self, enum { GoodTillCancel, FillOrKill, ImmediateOrCancel, PostOnly });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CryptocomRefPriceType {
    #[serde(rename = "MARK_PRICE")]
    Mark,
    #[serde(rename = "INDEX_PRICE")]
    Index,
    #[serde(rename = "LAST_PRICE")]
    Last,
}
#[derive(Deserialize)]
#[serde(remote = "CryptocomRefPriceType")]
enum WireCryptocomRefPriceType {
    #[serde(rename = "MARK_PRICE")]
    Mark,
    #[serde(rename = "INDEX_PRICE")]
    Index,
    #[serde(rename = "LAST_PRICE")]
    Last,
}
impl<'de> Deserialize<'de> for CryptocomRefPriceType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WireCryptocomRefPriceType::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(
            &value,
        ))
    }
}
impl JsNumberOutput for CryptocomRefPriceType {
    fn check_number_output(&self) {
        js_number_output!(self, enum { Mark, Index, Last });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CryptocomOrderStatus {
    #[serde(rename = "ACTIVE")]
    Active,
    #[serde(rename = "FILLED")]
    Filled,
    #[serde(rename = "CANCELED")]
    Canceled,
    #[serde(rename = "EXPIRED")]
    Expired,
    #[serde(rename = "PENDING")]
    Pending,
    #[serde(rename = "REJECTED")]
    Rejected,
}
#[derive(Deserialize)]
#[serde(remote = "CryptocomOrderStatus")]
enum WireCryptocomOrderStatus {
    #[serde(rename = "ACTIVE")]
    Active,
    #[serde(rename = "FILLED")]
    Filled,
    #[serde(rename = "CANCELED")]
    Canceled,
    #[serde(rename = "EXPIRED")]
    Expired,
    #[serde(rename = "PENDING")]
    Pending,
    #[serde(rename = "REJECTED")]
    Rejected,
}
impl<'de> Deserialize<'de> for CryptocomOrderStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WireCryptocomOrderStatus::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(
            &value,
        ))
    }
}
impl JsNumberOutput for CryptocomOrderStatus {
    fn check_number_output(&self) {
        js_number_output!(self, enum { Active, Filled, Canceled, Expired, Pending, Rejected });
    }
}

fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq)]
pub struct CryptocomOrder {
    pub account_id: Option<String>,
    pub order_id: String,
    pub client_oid: Option<String>,
    pub order_type: CryptocomOrderType,
    pub time_in_force: CryptocomTimeInForce,
    pub side: CryptocomTradeSide,
    pub exec_inst: Option<Vec<String>>,
    pub quantity: String,
    pub limit_price: Option<String>,
    pub trigger_price: Option<String>,
    pub ref_price: Option<String>,
    pub ref_price_type: Option<CryptocomRefPriceType>,
    pub avg_price: Option<String>,
    pub cumulative_quantity: Option<String>,
    pub cumulative_value: Option<String>,
    pub fee_currency: Option<String>,
    pub fee_instrument_name: Option<String>,
    pub create_time: Option<f64>,
    pub update_time: Option<f64>,
    pub time_created: Option<f64>,
    pub time_updated: Option<f64>,
    pub status: CryptocomOrderStatus,
    pub order_status: Option<CryptocomOrderStatus>,
    pub instrument_name: String,
    pub post_only: Option<bool>,
    /// Unknown own JSON fields, retained through the shared numeric capture.
    pub additional_fields: serde_json::Value,
    pub(crate) input_keys: Vec<String>,
}
#[derive(Deserialize)]
#[serde(remote = "CryptocomOrder")]
struct WireCryptocomOrder {
    #[serde(
        rename = "account_id",
        default,
        deserialize_with = "deserialize_present"
    )]
    account_id: Option<String>,
    #[serde(rename = "order_id")]
    order_id: String,
    #[serde(
        rename = "client_oid",
        default,
        deserialize_with = "deserialize_present"
    )]
    client_oid: Option<String>,
    #[serde(rename = "order_type")]
    order_type: CryptocomOrderType,
    #[serde(rename = "time_in_force")]
    time_in_force: CryptocomTimeInForce,
    #[serde(rename = "side")]
    side: CryptocomTradeSide,
    #[serde(
        rename = "exec_inst",
        default,
        deserialize_with = "deserialize_present"
    )]
    exec_inst: Option<Vec<String>>,
    #[serde(rename = "quantity")]
    quantity: String,
    #[serde(
        rename = "limit_price",
        default,
        deserialize_with = "deserialize_present"
    )]
    limit_price: Option<String>,
    #[serde(
        rename = "trigger_price",
        default,
        deserialize_with = "deserialize_present"
    )]
    trigger_price: Option<String>,
    #[serde(
        rename = "ref_price",
        default,
        deserialize_with = "deserialize_present"
    )]
    ref_price: Option<String>,
    #[serde(
        rename = "ref_price_type",
        default,
        deserialize_with = "deserialize_present"
    )]
    ref_price_type: Option<CryptocomRefPriceType>,
    #[serde(
        rename = "avg_price",
        default,
        deserialize_with = "deserialize_present"
    )]
    avg_price: Option<String>,
    #[serde(
        rename = "cumulative_quantity",
        default,
        deserialize_with = "deserialize_present"
    )]
    cumulative_quantity: Option<String>,
    #[serde(
        rename = "cumulative_value",
        default,
        deserialize_with = "deserialize_present"
    )]
    cumulative_value: Option<String>,
    #[serde(
        rename = "fee_currency",
        default,
        deserialize_with = "deserialize_present"
    )]
    fee_currency: Option<String>,
    #[serde(
        rename = "fee_instrument_name",
        default,
        deserialize_with = "deserialize_present"
    )]
    fee_instrument_name: Option<String>,
    #[serde(
        rename = "create_time",
        default,
        deserialize_with = "deserialize_present"
    )]
    create_time: Option<f64>,
    #[serde(
        rename = "update_time",
        default,
        deserialize_with = "deserialize_present"
    )]
    update_time: Option<f64>,
    #[serde(
        rename = "time_created",
        default,
        deserialize_with = "deserialize_present"
    )]
    time_created: Option<f64>,
    #[serde(
        rename = "time_updated",
        default,
        deserialize_with = "deserialize_present"
    )]
    time_updated: Option<f64>,
    #[serde(rename = "status")]
    status: CryptocomOrderStatus,
    #[serde(
        rename = "order_status",
        default,
        deserialize_with = "deserialize_present"
    )]
    order_status: Option<CryptocomOrderStatus>,
    #[serde(rename = "instrument_name")]
    instrument_name: String,
    #[serde(
        rename = "post_only",
        default,
        deserialize_with = "deserialize_present"
    )]
    post_only: Option<bool>,
    #[serde(skip)]
    additional_fields: serde_json::Value,
    #[serde(skip)]
    input_keys: Vec<String>,
}
impl<'de> Deserialize<'de> for CryptocomOrder {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (input, fields) = crate::wire_record::capture(
            deserializer,
            &[
                "account_id",
                "order_id",
                "client_oid",
                "order_type",
                "time_in_force",
                "side",
                "exec_inst",
                "quantity",
                "limit_price",
                "trigger_price",
                "ref_price",
                "ref_price_type",
                "avg_price",
                "cumulative_quantity",
                "cumulative_value",
                "fee_currency",
                "fee_instrument_name",
                "create_time",
                "update_time",
                "time_created",
                "time_updated",
                "status",
                "order_status",
                "instrument_name",
                "post_only",
            ],
        )?;
        input
            .decode_with(|deserializer| {
                let mut record = WireCryptocomOrder::deserialize(
                    safe_cex_types::strict_decode::object(deserializer),
                )?;
                record.additional_fields = fields.additional;
                record.input_keys = fields.keys;
                Ok(record)
            })
            .map_err(serde::de::Error::custom)
    }
}
impl JsNumberOutput for CryptocomOrder {
    fn check_number_output(&self) {
        js_number_output!(self, struct { account_id, order_id, client_oid, order_type, time_in_force, side, exec_inst, quantity, limit_price, trigger_price, ref_price, ref_price_type, avg_price, cumulative_quantity, cumulative_value, fee_currency, fee_instrument_name, create_time, update_time, time_created, time_updated, status, order_status, instrument_name, post_only, input_keys } passthrough { additional_fields });
    }
}

impl CryptocomInstrument {
    /// Current own keys in captured JSON order. Adding/deleting/reinserting public
    /// fields does not reconstruct JS mutation chronology; this is a decoded snapshot.
    pub fn own_keys(&self) -> Vec<String> {
        crate::wire_record::keys(
            &self.input_keys,
            &[
                ("symbol", true),
                ("inst_type", true),
                ("display_name", true),
                ("base_ccy", true),
                ("quote_ccy", true),
                ("quote_decimals", true),
                ("quantity_decimals", true),
                ("price_tick_size", true),
                ("qty_tick_size", true),
                ("max_leverage", true),
                ("tradable", true),
                ("expiry_timestamp_ms", true),
                ("beta_product", true),
                ("underlying_symbol", true),
                ("contract_size", true),
                ("margin_buy_enabled", true),
                ("margin_sell_enabled", true),
            ],
            &self.additional_fields,
        )
    }
}
impl Serialize for CryptocomInstrument {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let keys = self.own_keys();
        let mut output = serializer.serialize_map(Some(keys.len()))?;
        for key in keys {
            match key.as_str() {
                "symbol" => output.serialize_entry(&key, &self.symbol)?,
                "inst_type" => output.serialize_entry(&key, &self.inst_type)?,
                "display_name" => output.serialize_entry(&key, &self.display_name)?,
                "base_ccy" => output.serialize_entry(&key, &self.base_ccy)?,
                "quote_ccy" => output.serialize_entry(&key, &self.quote_ccy)?,
                "quote_decimals" => output.serialize_entry(&key, &self.quote_decimals)?,
                "quantity_decimals" => output.serialize_entry(&key, &self.quantity_decimals)?,
                "price_tick_size" => output.serialize_entry(&key, &self.price_tick_size)?,
                "qty_tick_size" => output.serialize_entry(&key, &self.qty_tick_size)?,
                "max_leverage" => output.serialize_entry(&key, &self.max_leverage)?,
                "tradable" => output.serialize_entry(&key, &self.tradable)?,
                "expiry_timestamp_ms" => output.serialize_entry(&key, &self.expiry_timestamp_ms)?,
                "beta_product" => output.serialize_entry(&key, &self.beta_product)?,
                "underlying_symbol" => output.serialize_entry(&key, &self.underlying_symbol)?,
                "contract_size" => output.serialize_entry(&key, &self.contract_size)?,
                "margin_buy_enabled" => output.serialize_entry(&key, &self.margin_buy_enabled)?,
                "margin_sell_enabled" => output.serialize_entry(&key, &self.margin_sell_enabled)?,
                _ => output.serialize_entry(
                    &key,
                    &crate::wire_record::ExtraValue(&self.additional_fields[&key]),
                )?,
            }
        }
        output.end()
    }
}

impl CryptocomTicker {
    /// Current own keys in captured JSON order. Adding/deleting/reinserting public
    /// fields does not reconstruct JS mutation chronology; this is a decoded snapshot.
    pub fn own_keys(&self) -> Vec<String> {
        crate::wire_record::keys(
            &self.input_keys,
            &[
                ("i", true),
                ("b", true),
                ("k", true),
                ("a", true),
                ("c", true),
                ("h", true),
                ("l", true),
                ("v", true),
                ("vv", true),
                ("oi", true),
                ("t", true),
            ],
            &self.additional_fields,
        )
    }
}
impl Serialize for CryptocomTicker {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let keys = self.own_keys();
        let mut output = serializer.serialize_map(Some(keys.len()))?;
        for key in keys {
            match key.as_str() {
                "i" => output.serialize_entry(&key, &self.i)?,
                "b" => output.serialize_entry(&key, &self.b)?,
                "k" => output.serialize_entry(&key, &self.k)?,
                "a" => output.serialize_entry(&key, &self.a)?,
                "c" => output.serialize_entry(&key, &self.c)?,
                "h" => output.serialize_entry(&key, &self.h)?,
                "l" => output.serialize_entry(&key, &self.l)?,
                "v" => output.serialize_entry(&key, &self.v)?,
                "vv" => output.serialize_entry(&key, &self.vv)?,
                "oi" => output.serialize_entry(&key, &self.oi)?,
                "t" => output.serialize_entry(&key, &self.t)?,
                _ => output.serialize_entry(
                    &key,
                    &crate::wire_record::ExtraValue(&self.additional_fields[&key]),
                )?,
            }
        }
        output.end()
    }
}

impl CryptocomCandlestick {
    /// Current own keys in captured JSON order. Adding/deleting/reinserting public
    /// fields does not reconstruct JS mutation chronology; this is a decoded snapshot.
    pub fn own_keys(&self) -> Vec<String> {
        crate::wire_record::keys(
            &self.input_keys,
            &[
                ("t", true),
                ("o", true),
                ("h", true),
                ("l", true),
                ("c", true),
                ("v", true),
            ],
            &self.additional_fields,
        )
    }
}
impl Serialize for CryptocomCandlestick {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let keys = self.own_keys();
        let mut output = serializer.serialize_map(Some(keys.len()))?;
        for key in keys {
            match key.as_str() {
                "t" => output.serialize_entry(&key, &self.t)?,
                "o" => output.serialize_entry(&key, &self.o)?,
                "h" => output.serialize_entry(&key, &self.h)?,
                "l" => output.serialize_entry(&key, &self.l)?,
                "c" => output.serialize_entry(&key, &self.c)?,
                "v" => output.serialize_entry(&key, &self.v)?,
                _ => output.serialize_entry(
                    &key,
                    &crate::wire_record::ExtraValue(&self.additional_fields[&key]),
                )?,
            }
        }
        output.end()
    }
}

impl CryptocomPositionBalance {
    /// Current own keys in captured JSON order. Adding/deleting/reinserting public
    /// fields does not reconstruct JS mutation chronology; this is a decoded snapshot.
    pub fn own_keys(&self) -> Vec<String> {
        crate::wire_record::keys(
            &self.input_keys,
            &[
                ("instrument_name", true),
                ("quantity", true),
                ("reserved_qty", true),
                ("collateral_amount", true),
                ("collateral_eligible", true),
                ("haircut", true),
                ("market_value", true),
                ("max_withdrawal_balance", true),
                ("hourly_interest_rate", true),
            ],
            &self.additional_fields,
        )
    }
}
impl Serialize for CryptocomPositionBalance {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let keys = self.own_keys();
        let mut output = serializer.serialize_map(Some(keys.len()))?;
        for key in keys {
            match key.as_str() {
                "instrument_name" => output.serialize_entry(&key, &self.instrument_name)?,
                "quantity" => output.serialize_entry(&key, &self.quantity)?,
                "reserved_qty" => output.serialize_entry(&key, &self.reserved_qty)?,
                "collateral_amount" => output.serialize_entry(&key, &self.collateral_amount)?,
                "collateral_eligible" => output.serialize_entry(&key, &self.collateral_eligible)?,
                "haircut" => output.serialize_entry(&key, &self.haircut)?,
                "market_value" => output.serialize_entry(&key, &self.market_value)?,
                "max_withdrawal_balance" => {
                    output.serialize_entry(&key, &self.max_withdrawal_balance)?
                }
                "hourly_interest_rate" => {
                    output.serialize_entry(&key, &self.hourly_interest_rate)?
                }
                _ => output.serialize_entry(
                    &key,
                    &crate::wire_record::ExtraValue(&self.additional_fields[&key]),
                )?,
            }
        }
        output.end()
    }
}

impl CryptocomError {
    /// Current own keys in captured JSON order. Adding/deleting/reinserting public
    /// fields does not reconstruct JS mutation chronology; this is a decoded snapshot.
    pub fn own_keys(&self) -> Vec<String> {
        crate::wire_record::keys(
            &self.input_keys,
            &[("code", true), ("message", true)],
            &self.additional_fields,
        )
    }
}
impl Serialize for CryptocomError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let keys = self.own_keys();
        let mut output = serializer.serialize_map(Some(keys.len()))?;
        for key in keys {
            match key.as_str() {
                "code" => output.serialize_entry(&key, &self.code)?,
                "message" => output.serialize_entry(&key, &self.message)?,
                _ => output.serialize_entry(
                    &key,
                    &crate::wire_record::ExtraValue(&self.additional_fields[&key]),
                )?,
            }
        }
        output.end()
    }
}

impl CryptocomTrade {
    /// Current own keys in captured JSON order. Adding/deleting/reinserting public
    /// fields does not reconstruct JS mutation chronology; this is a decoded snapshot.
    pub fn own_keys(&self) -> Vec<String> {
        crate::wire_record::keys(
            &self.input_keys,
            &[
                ("d", true),
                ("t", true),
                ("p", true),
                ("q", true),
                ("s", true),
                ("i", true),
                ("dataTime", true),
            ],
            &self.additional_fields,
        )
    }
}
impl Serialize for CryptocomTrade {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let keys = self.own_keys();
        let mut output = serializer.serialize_map(Some(keys.len()))?;
        for key in keys {
            match key.as_str() {
                "d" => output.serialize_entry(&key, &self.d)?,
                "t" => output.serialize_entry(&key, &self.t)?,
                "p" => output.serialize_entry(&key, &self.p)?,
                "q" => output.serialize_entry(&key, &self.q)?,
                "s" => output.serialize_entry(&key, &self.s)?,
                "i" => output.serialize_entry(&key, &self.i)?,
                "dataTime" => output.serialize_entry(&key, &self.data_time)?,
                _ => output.serialize_entry(
                    &key,
                    &crate::wire_record::ExtraValue(&self.additional_fields[&key]),
                )?,
            }
        }
        output.end()
    }
}

impl CryptocomAccountIdentity {
    /// Current own keys in captured JSON order. Adding/deleting/reinserting public
    /// fields does not reconstruct JS mutation chronology; this is a decoded snapshot.
    pub fn own_keys(&self) -> Vec<String> {
        crate::wire_record::keys(&self.input_keys, &[("uuid", true)], &self.additional_fields)
    }
}
impl Serialize for CryptocomAccountIdentity {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let keys = self.own_keys();
        let mut output = serializer.serialize_map(Some(keys.len()))?;
        for key in keys {
            match key.as_str() {
                "uuid" => output.serialize_entry(&key, &self.uuid)?,
                _ => output.serialize_entry(
                    &key,
                    &crate::wire_record::ExtraValue(&self.additional_fields[&key]),
                )?,
            }
        }
        output.end()
    }
}

impl CryptocomAccountsResponse {
    /// Current own keys in captured JSON order. Adding/deleting/reinserting public
    /// fields does not reconstruct JS mutation chronology; this is a decoded snapshot.
    pub fn own_keys(&self) -> Vec<String> {
        crate::wire_record::keys(
            &self.input_keys,
            &[("this_account", true)],
            &self.additional_fields,
        )
    }
}
impl Serialize for CryptocomAccountsResponse {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let keys = self.own_keys();
        let mut output = serializer.serialize_map(Some(keys.len()))?;
        for key in keys {
            match key.as_str() {
                "this_account" => output.serialize_entry(&key, &self.this_account)?,
                _ => output.serialize_entry(
                    &key,
                    &crate::wire_record::ExtraValue(&self.additional_fields[&key]),
                )?,
            }
        }
        output.end()
    }
}

impl CryptocomBalanceSummary {
    /// Current own keys in captured JSON order. Adding/deleting/reinserting public
    /// fields does not reconstruct JS mutation chronology; this is a decoded snapshot.
    pub fn own_keys(&self) -> Vec<String> {
        crate::wire_record::keys(
            &self.input_keys,
            &[
                ("total_available_balance", true),
                ("total_margin_balance", true),
                ("total_initial_margin", true),
                ("total_maintenance_margin", true),
                ("total_cash_balance", true),
                ("total_collateral_value", true),
                ("total_session_unrealized_pnl", true),
                ("total_session_realized_pnl", true),
                ("total_position_cost", true),
                ("instrument_name", true),
                ("position_balances", true),
            ],
            &self.additional_fields,
        )
    }
}
impl Serialize for CryptocomBalanceSummary {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let keys = self.own_keys();
        let mut output = serializer.serialize_map(Some(keys.len()))?;
        for key in keys {
            match key.as_str() {
                "total_available_balance" => {
                    output.serialize_entry(&key, &self.total_available_balance)?
                }
                "total_margin_balance" => {
                    output.serialize_entry(&key, &self.total_margin_balance)?
                }
                "total_initial_margin" => {
                    output.serialize_entry(&key, &self.total_initial_margin)?
                }
                "total_maintenance_margin" => {
                    output.serialize_entry(&key, &self.total_maintenance_margin)?
                }
                "total_cash_balance" => output.serialize_entry(&key, &self.total_cash_balance)?,
                "total_collateral_value" => {
                    output.serialize_entry(&key, &self.total_collateral_value)?
                }
                "total_session_unrealized_pnl" => {
                    output.serialize_entry(&key, &self.total_session_unrealized_pnl)?
                }
                "total_session_realized_pnl" => {
                    output.serialize_entry(&key, &self.total_session_realized_pnl)?
                }
                "total_position_cost" => output.serialize_entry(&key, &self.total_position_cost)?,
                "instrument_name" => output.serialize_entry(&key, &self.instrument_name)?,
                "position_balances" => output.serialize_entry(&key, &self.position_balances)?,
                _ => output.serialize_entry(
                    &key,
                    &crate::wire_record::ExtraValue(&self.additional_fields[&key]),
                )?,
            }
        }
        output.end()
    }
}

impl CryptocomBookData {
    /// Current own keys in captured JSON order. Adding/deleting/reinserting public
    /// fields does not reconstruct JS mutation chronology; this is a decoded snapshot.
    pub fn own_keys(&self) -> Vec<String> {
        crate::wire_record::keys(
            &self.input_keys,
            &[("bids", true), ("asks", true), ("t", true)],
            &self.additional_fields,
        )
    }
}
impl Serialize for CryptocomBookData {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let keys = self.own_keys();
        let mut output = serializer.serialize_map(Some(keys.len()))?;
        for key in keys {
            match key.as_str() {
                "bids" => output.serialize_entry(&key, &self.bids)?,
                "asks" => output.serialize_entry(&key, &self.asks)?,
                "t" => output.serialize_entry(&key, &self.t)?,
                _ => output.serialize_entry(
                    &key,
                    &crate::wire_record::ExtraValue(&self.additional_fields[&key]),
                )?,
            }
        }
        output.end()
    }
}

impl CryptocomBook {
    /// Current own keys in captured JSON order. Adding/deleting/reinserting public
    /// fields does not reconstruct JS mutation chronology; this is a decoded snapshot.
    pub fn own_keys(&self) -> Vec<String> {
        crate::wire_record::keys(
            &self.input_keys,
            &[("instrument_name", true), ("depth", true), ("data", true)],
            &self.additional_fields,
        )
    }
}
impl Serialize for CryptocomBook {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let keys = self.own_keys();
        let mut output = serializer.serialize_map(Some(keys.len()))?;
        for key in keys {
            match key.as_str() {
                "instrument_name" => output.serialize_entry(&key, &self.instrument_name)?,
                "depth" => output.serialize_entry(&key, &self.depth)?,
                "data" => output.serialize_entry(&key, &self.data)?,
                _ => output.serialize_entry(
                    &key,
                    &crate::wire_record::ExtraValue(&self.additional_fields[&key]),
                )?,
            }
        }
        output.end()
    }
}

impl CryptocomTradeFill {
    /// Current own keys in captured JSON order. Adding/deleting/reinserting public
    /// fields does not reconstruct JS mutation chronology; this is a decoded snapshot.
    pub fn own_keys(&self) -> Vec<String> {
        crate::wire_record::keys(
            &self.input_keys,
            &[
                ("account_id", true),
                ("event_date", true),
                ("journal_type", true),
                ("traded_quantity", true),
                ("traded_price", true),
                ("transaction_qty", true),
                ("transaction_cost", true),
                ("order_id", true),
                ("trade_id", true),
                ("trade_match_id", true),
                ("client_oid", self.client_oid.is_some()),
                ("taker_side", true),
                ("side", true),
                ("instrument_name", true),
                ("fee", true),
                ("fee_currency", true),
                ("create_time", true),
                ("create_time_ns", true),
                ("liquidity_indicator", true),
            ],
            &self.additional_fields,
        )
    }
}
impl Serialize for CryptocomTradeFill {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let keys = self.own_keys();
        let mut output = serializer.serialize_map(Some(keys.len()))?;
        for key in keys {
            match key.as_str() {
                "account_id" => output.serialize_entry(&key, &self.account_id)?,
                "event_date" => output.serialize_entry(&key, &self.event_date)?,
                "journal_type" => output.serialize_entry(&key, &self.journal_type)?,
                "traded_quantity" => output.serialize_entry(&key, &self.traded_quantity)?,
                "traded_price" => output.serialize_entry(&key, &self.traded_price)?,
                "transaction_qty" => output.serialize_entry(&key, &self.transaction_qty)?,
                "transaction_cost" => output.serialize_entry(&key, &self.transaction_cost)?,
                "order_id" => output.serialize_entry(&key, &self.order_id)?,
                "trade_id" => output.serialize_entry(&key, &self.trade_id)?,
                "trade_match_id" => output.serialize_entry(&key, &self.trade_match_id)?,
                "client_oid" => output.serialize_entry(&key, &self.client_oid)?,
                "taker_side" => output.serialize_entry(&key, &self.taker_side)?,
                "side" => output.serialize_entry(&key, &self.side)?,
                "instrument_name" => output.serialize_entry(&key, &self.instrument_name)?,
                "fee" => output.serialize_entry(&key, &self.fee)?,
                "fee_currency" => output.serialize_entry(&key, &self.fee_currency)?,
                "create_time" => output.serialize_entry(&key, &self.create_time)?,
                "create_time_ns" => output.serialize_entry(&key, &self.create_time_ns)?,
                "liquidity_indicator" => output.serialize_entry(&key, &self.liquidity_indicator)?,
                _ => output.serialize_entry(
                    &key,
                    &crate::wire_record::ExtraValue(&self.additional_fields[&key]),
                )?,
            }
        }
        output.end()
    }
}

impl CryptocomOrder {
    /// Current own keys in captured JSON order. Adding/deleting/reinserting public
    /// fields does not reconstruct JS mutation chronology; this is a decoded snapshot.
    pub fn own_keys(&self) -> Vec<String> {
        crate::wire_record::keys(
            &self.input_keys,
            &[
                ("account_id", self.account_id.is_some()),
                ("order_id", true),
                ("client_oid", self.client_oid.is_some()),
                ("order_type", true),
                ("time_in_force", true),
                ("side", true),
                ("exec_inst", self.exec_inst.is_some()),
                ("quantity", true),
                ("limit_price", self.limit_price.is_some()),
                ("trigger_price", self.trigger_price.is_some()),
                ("ref_price", self.ref_price.is_some()),
                ("ref_price_type", self.ref_price_type.is_some()),
                ("avg_price", self.avg_price.is_some()),
                ("cumulative_quantity", self.cumulative_quantity.is_some()),
                ("cumulative_value", self.cumulative_value.is_some()),
                ("fee_currency", self.fee_currency.is_some()),
                ("fee_instrument_name", self.fee_instrument_name.is_some()),
                ("create_time", self.create_time.is_some()),
                ("update_time", self.update_time.is_some()),
                ("time_created", self.time_created.is_some()),
                ("time_updated", self.time_updated.is_some()),
                ("status", true),
                ("order_status", self.order_status.is_some()),
                ("instrument_name", true),
                ("post_only", self.post_only.is_some()),
            ],
            &self.additional_fields,
        )
    }
}
impl Serialize for CryptocomOrder {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let keys = self.own_keys();
        let mut output = serializer.serialize_map(Some(keys.len()))?;
        for key in keys {
            match key.as_str() {
                "account_id" => output.serialize_entry(&key, &self.account_id)?,
                "order_id" => output.serialize_entry(&key, &self.order_id)?,
                "client_oid" => output.serialize_entry(&key, &self.client_oid)?,
                "order_type" => output.serialize_entry(&key, &self.order_type)?,
                "time_in_force" => output.serialize_entry(&key, &self.time_in_force)?,
                "side" => output.serialize_entry(&key, &self.side)?,
                "exec_inst" => output.serialize_entry(&key, &self.exec_inst)?,
                "quantity" => output.serialize_entry(&key, &self.quantity)?,
                "limit_price" => output.serialize_entry(&key, &self.limit_price)?,
                "trigger_price" => output.serialize_entry(&key, &self.trigger_price)?,
                "ref_price" => output.serialize_entry(&key, &self.ref_price)?,
                "ref_price_type" => output.serialize_entry(&key, &self.ref_price_type)?,
                "avg_price" => output.serialize_entry(&key, &self.avg_price)?,
                "cumulative_quantity" => output.serialize_entry(&key, &self.cumulative_quantity)?,
                "cumulative_value" => output.serialize_entry(&key, &self.cumulative_value)?,
                "fee_currency" => output.serialize_entry(&key, &self.fee_currency)?,
                "fee_instrument_name" => output.serialize_entry(&key, &self.fee_instrument_name)?,
                "create_time" => output.serialize_entry(&key, &self.create_time)?,
                "update_time" => output.serialize_entry(&key, &self.update_time)?,
                "time_created" => output.serialize_entry(&key, &self.time_created)?,
                "time_updated" => output.serialize_entry(&key, &self.time_updated)?,
                "status" => output.serialize_entry(&key, &self.status)?,
                "order_status" => output.serialize_entry(&key, &self.order_status)?,
                "instrument_name" => output.serialize_entry(&key, &self.instrument_name)?,
                "post_only" => output.serialize_entry(&key, &self.post_only)?,
                _ => output.serialize_entry(
                    &key,
                    &crate::wire_record::ExtraValue(&self.additional_fields[&key]),
                )?,
            }
        }
        output.end()
    }
}

#[cfg(test)]
mod accepted_present_helpers {
    use serde_json::Value;
    #[test]
    fn present_string_native_contract() {
        let cases:Vec<Value>=serde_json::from_str(r###"[{"input":"\"\"","output":{"ok":true,"present":true,"value":"","units":[]}},{"input":"\"client-A\"","output":{"ok":true,"present":true,"value":"client-A","units":[99,108,105,101,110,116,45,65]}},{"input":"\" \\tclient B\\n\"","output":{"ok":true,"present":true,"value":" \tclient B\n","units":[32,9,99,108,105,101,110,116,32,66,10]}},{"input":"\"\\u03a9\\ud83d\\ude80\"","output":{"ok":true,"present":true,"value":"\u03a9\ud83d\ude80","units":[937,55357,56960]}},{"input":"null","output":{"ok":false}},{"input":"true","output":{"ok":false}},{"input":"0","output":{"ok":false}},{"input":"[]","output":{"ok":false}},{"input":"{}","output":{"ok":false}},{"input":"[\"s\"]","output":{"ok":false}},{"input":"{\"s\":\"x\"}","output":{"ok":false}}]"###).unwrap();
        for case in cases {
            let value = &case["input"];
            let observed = {
                let raw = value.as_str().unwrap();
                let mut d = serde_json::Deserializer::from_str(raw);
                match super::deserialize_present_string(&mut d) {
                    Ok(Some(value)) => {
                        serde_json::json!({"ok":true,"present":true,"units":value.encode_utf16().collect::<Vec<_>>(),"value":value})
                    }
                    Ok(None) => {
                        serde_json::json!({"ok":true,"present":false,"units":[],"value":null})
                    }
                    Err(_) => serde_json::json!({"ok":false}),
                }
            };
            assert_eq!(
                observed, case["output"],
                "native present helper string input {value}"
            );
        }
    }
    #[test]
    fn present_generic_native_contract() {
        let cases:Vec<Value>=serde_json::from_str(r###"[{"input":{"kind":"string","raw":"\"\""},"output":{"ok":true,"present":true,"value":"","kind":"string"}},{"input":{"kind":"string","raw":"\"order-A\""},"output":{"ok":true,"present":true,"value":"order-A","kind":"string"}},{"input":{"kind":"string","raw":"\"\\u03a9\\ud83d\\ude80\""},"output":{"ok":true,"present":true,"value":"\u03a9\ud83d\ude80","kind":"string"}},{"input":{"kind":"string","raw":"null"},"output":{"ok":false}},{"input":{"kind":"string","raw":"[]"},"output":{"ok":false}},{"input":{"kind":"string","raw":"true"},"output":{"ok":false}},{"input":{"kind":"number","raw":"-0.0"},"output":{"ok":true,"present":true,"value":{"bits":"8000000000000000"},"kind":"number"}},{"input":{"kind":"number","raw":"17.5"},"output":{"ok":true,"present":true,"value":{"bits":"4031800000000000"},"kind":"number"}},{"input":{"kind":"number","raw":"null"},"output":{"ok":false}},{"input":{"kind":"number","raw":"\"17.5\""},"output":{"ok":false}},{"input":{"kind":"number","raw":"[]"},"output":{"ok":false}},{"input":{"kind":"boolean","raw":"true"},"output":{"ok":true,"present":true,"value":true,"kind":"boolean"}},{"input":{"kind":"boolean","raw":"false"},"output":{"ok":true,"present":true,"value":false,"kind":"boolean"}},{"input":{"kind":"boolean","raw":"null"},"output":{"ok":false}},{"input":{"kind":"boolean","raw":"0"},"output":{"ok":false}},{"input":{"kind":"boolean","raw":"\"false\""},"output":{"ok":false}},{"input":{"kind":"strings","raw":"[]"},"output":{"ok":true,"present":true,"value":{"len":0,"items":[]},"kind":"strings"}},{"input":{"kind":"strings","raw":"[\"exec-A\"]"},"output":{"ok":true,"present":true,"value":{"len":1,"items":["exec-A"]},"kind":"strings"}},{"input":{"kind":"strings","raw":"[\"exec-A\", \"exec-B\"]"},"output":{"ok":true,"present":true,"value":{"len":2,"items":["exec-A","exec-B"]},"kind":"strings"}},{"input":{"kind":"strings","raw":"null"},"output":{"ok":false}},{"input":{"kind":"strings","raw":"\"exec-A\""},"output":{"ok":false}},{"input":{"kind":"strings","raw":"[\"exec-A\",0]"},"output":{"ok":false}},{"input":{"kind":"reference","raw":"\"MARK_PRICE\""},"output":{"ok":true,"present":true,"value":{"variant":"Mark","ordinal":0},"kind":"reference"}},{"input":{"kind":"reference","raw":"\"INDEX_PRICE\""},"output":{"ok":true,"present":true,"value":{"variant":"Index","ordinal":1},"kind":"reference"}},{"input":{"kind":"reference","raw":"\"LAST_PRICE\""},"output":{"ok":true,"present":true,"value":{"variant":"Last","ordinal":2},"kind":"reference"}},{"input":{"kind":"reference","raw":"null"},"output":{"ok":false}},{"input":{"kind":"reference","raw":"{\"MARK_PRICE\":null}"},"output":{"ok":false}},{"input":{"kind":"reference","raw":"\"unknown\""},"output":{"ok":false}},{"input":{"kind":"status","raw":"\"ACTIVE\""},"output":{"ok":true,"present":true,"value":{"variant":"Active","ordinal":0},"kind":"status"}},{"input":{"kind":"status","raw":"\"FILLED\""},"output":{"ok":true,"present":true,"value":{"variant":"Filled","ordinal":1},"kind":"status"}},{"input":{"kind":"status","raw":"\"CANCELED\""},"output":{"ok":true,"present":true,"value":{"variant":"Canceled","ordinal":2},"kind":"status"}},{"input":{"kind":"status","raw":"\"EXPIRED\""},"output":{"ok":true,"present":true,"value":{"variant":"Expired","ordinal":3},"kind":"status"}},{"input":{"kind":"status","raw":"\"PENDING\""},"output":{"ok":true,"present":true,"value":{"variant":"Pending","ordinal":4},"kind":"status"}},{"input":{"kind":"status","raw":"\"REJECTED\""},"output":{"ok":true,"present":true,"value":{"variant":"Rejected","ordinal":5},"kind":"status"}},{"input":{"kind":"status","raw":"null"},"output":{"ok":false}},{"input":{"kind":"status","raw":"{\"ACTIVE\":null}"},"output":{"ok":false}},{"input":{"kind":"status","raw":"\"unknown\""},"output":{"ok":false}},{"input":{"kind":"optional-string","raw":"null"},"output":{"ok":true,"present":true,"value":null,"kind":"optional-string"}},{"input":{"kind":"optional-string","raw":"\"inner\""},"output":{"ok":true,"present":true,"value":"inner","kind":"optional-string"}}]"###).unwrap();
        for case in cases {
            let value = &case["input"];
            let observed = {
                let kind = value["kind"].as_str().unwrap();
                let raw = value["raw"].as_str().unwrap();
                let mut d = serde_json::Deserializer::from_str(raw);
                match kind {
                    "string" => {
                        let result = super::deserialize_present::<_, String>(&mut d);
                        match result {
                            Ok(Some(value)) => {
                                let native = serde_json::json!(value);
                                serde_json::json!({"ok":true,"present":true,"kind":kind,"value":native})
                            }
                            Ok(None) => {
                                serde_json::json!({"ok":true,"present":false,"kind":kind,"value":null})
                            }
                            Err(_) => serde_json::json!({"ok":false}),
                        }
                    }
                    "number" => {
                        let result = super::deserialize_present::<_, f64>(&mut d);
                        match result {
                            Ok(Some(value)) => {
                                let native =
                                    serde_json::json!({"bits":format!("{:016x}",value.to_bits())});
                                serde_json::json!({"ok":true,"present":true,"kind":kind,"value":native})
                            }
                            Ok(None) => {
                                serde_json::json!({"ok":true,"present":false,"kind":kind,"value":null})
                            }
                            Err(_) => serde_json::json!({"ok":false}),
                        }
                    }
                    "boolean" => {
                        let result = super::deserialize_present::<_, bool>(&mut d);
                        match result {
                            Ok(Some(value)) => {
                                let native = serde_json::json!(value);
                                serde_json::json!({"ok":true,"present":true,"kind":kind,"value":native})
                            }
                            Ok(None) => {
                                serde_json::json!({"ok":true,"present":false,"kind":kind,"value":null})
                            }
                            Err(_) => serde_json::json!({"ok":false}),
                        }
                    }
                    "strings" => {
                        let result = super::deserialize_present::<_, Vec<String>>(&mut d);
                        match result {
                            Ok(Some(value)) => {
                                let native = serde_json::json!({"len":value.len(),"items":value});
                                serde_json::json!({"ok":true,"present":true,"kind":kind,"value":native})
                            }
                            Ok(None) => {
                                serde_json::json!({"ok":true,"present":false,"kind":kind,"value":null})
                            }
                            Err(_) => serde_json::json!({"ok":false}),
                        }
                    }
                    "reference" => {
                        let result = super::deserialize_present::<
                            _,
                            crate::wire::CryptocomRefPriceType,
                        >(&mut d);
                        match result {
                            Ok(Some(value)) => {
                                let native = match value {
                                    crate::wire::CryptocomRefPriceType::Mark => {
                                        serde_json::json!({"variant":"Mark","ordinal":0u32})
                                    }
                                    crate::wire::CryptocomRefPriceType::Index => {
                                        serde_json::json!({"variant":"Index","ordinal":1u32})
                                    }
                                    crate::wire::CryptocomRefPriceType::Last => {
                                        serde_json::json!({"variant":"Last","ordinal":2u32})
                                    }
                                };
                                serde_json::json!({"ok":true,"present":true,"kind":kind,"value":native})
                            }
                            Ok(None) => {
                                serde_json::json!({"ok":true,"present":false,"kind":kind,"value":null})
                            }
                            Err(_) => serde_json::json!({"ok":false}),
                        }
                    }
                    "status" => {
                        let result = super::deserialize_present::<
                            _,
                            crate::wire::CryptocomOrderStatus,
                        >(&mut d);
                        match result {
                            Ok(Some(value)) => {
                                let native = match value {
                                    crate::wire::CryptocomOrderStatus::Active => {
                                        serde_json::json!({"variant":"Active","ordinal":0u32})
                                    }
                                    crate::wire::CryptocomOrderStatus::Filled => {
                                        serde_json::json!({"variant":"Filled","ordinal":1u32})
                                    }
                                    crate::wire::CryptocomOrderStatus::Canceled => {
                                        serde_json::json!({"variant":"Canceled","ordinal":2u32})
                                    }
                                    crate::wire::CryptocomOrderStatus::Expired => {
                                        serde_json::json!({"variant":"Expired","ordinal":3u32})
                                    }
                                    crate::wire::CryptocomOrderStatus::Pending => {
                                        serde_json::json!({"variant":"Pending","ordinal":4u32})
                                    }
                                    crate::wire::CryptocomOrderStatus::Rejected => {
                                        serde_json::json!({"variant":"Rejected","ordinal":5u32})
                                    }
                                };
                                serde_json::json!({"ok":true,"present":true,"kind":kind,"value":native})
                            }
                            Ok(None) => {
                                serde_json::json!({"ok":true,"present":false,"kind":kind,"value":null})
                            }
                            Err(_) => serde_json::json!({"ok":false}),
                        }
                    }
                    "optional-string" => {
                        let result = super::deserialize_present::<_, Option<String>>(&mut d);
                        match result {
                            Ok(Some(value)) => {
                                let native = serde_json::json!(value);
                                serde_json::json!({"ok":true,"present":true,"kind":kind,"value":native})
                            }
                            Ok(None) => {
                                serde_json::json!({"ok":true,"present":false,"kind":kind,"value":null})
                            }
                            Err(_) => serde_json::json!({"ok":false}),
                        }
                    }
                    _ => panic!("unclassified native fixture instantiation"),
                }
            };
            assert_eq!(
                observed, case["output"],
                "native present helper generic input {value}"
            );
        }
    }
}
