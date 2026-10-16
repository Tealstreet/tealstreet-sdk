//! Authentication and wire payload records from polymarket.types.ts.
//! Decode numeric JSON through safe_cex_num::numeric_ingress, before buffering.
use safe_cex_num::{js_number_output, numeric_ingress::JsNumberOutput};
use serde::{Deserialize, Deserializer, Serialize};

// Optional means absent, not present null, in these source declarations.
fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PolymarketCredentials {
    #[serde(rename = "apiKey")]
    pub api_key: String,
    pub secret: String,
    pub passphrase: String,
}

impl<'de> Deserialize<'de> for PolymarketCredentials {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(rename = "apiKey")]
            api_key: String,
            secret: String,
            passphrase: String,
        }
        let fields = Fields::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            api_key: fields.api_key,
            secret: fields.secret,
            passphrase: fields.passphrase,
        })
    }
}

impl JsNumberOutput for PolymarketCredentials {
    fn check_number_output(&self) {
        js_number_output!(self, struct { api_key, secret, passphrase })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PolymarketDeriveKeyRequest {
    pub address: String,
    pub nonce: f64,
    pub signature: String,
    pub timestamp: f64,
}

impl<'de> Deserialize<'de> for PolymarketDeriveKeyRequest {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            address: String,
            nonce: f64,
            signature: String,
            timestamp: f64,
        }
        let fields = Fields::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            address: fields.address,
            nonce: fields.nonce,
            signature: fields.signature,
            timestamp: fields.timestamp,
        })
    }
}

impl JsNumberOutput for PolymarketDeriveKeyRequest {
    fn check_number_output(&self) {
        js_number_output!(self, struct { address, nonce, signature, timestamp })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PolymarketDeriveKeyResponse {
    #[serde(rename = "apiKey")]
    pub api_key: String,
    pub secret: String,
    pub passphrase: String,
}

impl<'de> Deserialize<'de> for PolymarketDeriveKeyResponse {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(rename = "apiKey")]
            api_key: String,
            secret: String,
            passphrase: String,
        }
        let fields = Fields::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            api_key: fields.api_key,
            secret: fields.secret,
            passphrase: fields.passphrase,
        })
    }
}

impl JsNumberOutput for PolymarketDeriveKeyResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { api_key, secret, passphrase })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PolymarketL2Headers {
    #[serde(rename = "POLY_ADDRESS")]
    pub poly_address: String,
    #[serde(rename = "POLY_SIGNATURE")]
    pub poly_signature: String,
    #[serde(rename = "POLY_TIMESTAMP")]
    pub poly_timestamp: String,
    #[serde(rename = "POLY_API_KEY")]
    pub poly_api_key: String,
    #[serde(rename = "POLY_PASSPHRASE")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poly_passphrase: Option<String>,
}

impl<'de> Deserialize<'de> for PolymarketL2Headers {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(rename = "POLY_ADDRESS")]
            poly_address: String,
            #[serde(rename = "POLY_SIGNATURE")]
            poly_signature: String,
            #[serde(rename = "POLY_TIMESTAMP")]
            poly_timestamp: String,
            #[serde(rename = "POLY_API_KEY")]
            poly_api_key: String,
            #[serde(rename = "POLY_PASSPHRASE")]
            #[serde(default, deserialize_with = "present")]
            poly_passphrase: Option<String>,
        }
        let fields = Fields::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            poly_address: fields.poly_address,
            poly_signature: fields.poly_signature,
            poly_timestamp: fields.poly_timestamp,
            poly_api_key: fields.poly_api_key,
            poly_passphrase: fields.poly_passphrase,
        })
    }
}

impl JsNumberOutput for PolymarketL2Headers {
    fn check_number_output(&self) {
        js_number_output!(self, struct { poly_address, poly_signature, poly_timestamp, poly_api_key, poly_passphrase })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PolymarketBalanceAllowance {
    pub balance: String,
    pub allowance: String,
}

impl<'de> Deserialize<'de> for PolymarketBalanceAllowance {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            balance: String,
            allowance: String,
        }
        let fields = Fields::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            balance: fields.balance,
            allowance: fields.allowance,
        })
    }
}

impl JsNumberOutput for PolymarketBalanceAllowance {
    fn check_number_output(&self) {
        js_number_output!(self, struct { balance, allowance })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PolymarketOrderBookLevel {
    pub price: String,
    pub size: String,
}

impl<'de> Deserialize<'de> for PolymarketOrderBookLevel {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            price: String,
            size: String,
        }
        let fields = Fields::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            price: fields.price,
            size: fields.size,
        })
    }
}

impl JsNumberOutput for PolymarketOrderBookLevel {
    fn check_number_output(&self) {
        js_number_output!(self, struct { price, size })
    }
}

// A present optional timestamp must be a Number, not null.
fn present_number<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    f64::deserialize(d).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PolymarketPrice {
    pub token_id: String,
    pub price: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<f64>,
}
impl<'de> Deserialize<'de> for PolymarketPrice {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            token_id: String,
            price: String,
            #[serde(default, deserialize_with = "present_number")]
            timestamp: Option<f64>,
        }
        let fields = Fields::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            token_id: fields.token_id,
            price: fields.price,
            timestamp: fields.timestamp,
        })
    }
}
impl JsNumberOutput for PolymarketPrice {
    fn check_number_output(&self) {
        js_number_output!(self, struct { token_id, price, timestamp })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PolymarketOrderBook {
    pub asset_id: String,
    pub bids: Vec<PolymarketOrderBookLevel>,
    pub asks: Vec<PolymarketOrderBookLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}
impl<'de> Deserialize<'de> for PolymarketOrderBook {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            asset_id: String,
            bids: Vec<PolymarketOrderBookLevel>,
            asks: Vec<PolymarketOrderBookLevel>,
            #[serde(default, deserialize_with = "present_number")]
            timestamp: Option<f64>,
            #[serde(default, deserialize_with = "present")]
            hash: Option<String>,
        }
        let fields = Fields::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            asset_id: fields.asset_id,
            bids: fields.bids,
            asks: fields.asks,
            timestamp: fields.timestamp,
            hash: fields.hash,
        })
    }
}
impl JsNumberOutput for PolymarketOrderBook {
    fn check_number_output(&self) {
        js_number_output!(self, struct { asset_id, bids, asks, timestamp, hash })
    }
}

fn present_strings<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<String>>, D::Error> {
    Vec::<String>::deserialize(d).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PolymarketOrderResponse {
    pub success: bool,
    #[serde(rename = "errorMsg", skip_serializing_if = "Option::is_none")]
    pub error_msg: Option<String>,
    #[serde(rename = "orderID", skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    #[serde(rename = "transactionsHashes", skip_serializing_if = "Option::is_none")]
    pub transactions_hashes: Option<Vec<String>>,
    #[serde(rename = "status", skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(rename = "takingAmount", skip_serializing_if = "Option::is_none")]
    pub taking_amount: Option<String>,
    #[serde(rename = "makingAmount", skip_serializing_if = "Option::is_none")]
    pub making_amount: Option<String>,
}
impl<'de> Deserialize<'de> for PolymarketOrderResponse {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            success: bool,
            #[serde(rename = "errorMsg", default, deserialize_with = "present")]
            error_msg: Option<String>,
            #[serde(rename = "orderID", default, deserialize_with = "present")]
            order_id: Option<String>,
            #[serde(
                rename = "transactionsHashes",
                default,
                deserialize_with = "present_strings"
            )]
            transactions_hashes: Option<Vec<String>>,
            #[serde(rename = "status", default, deserialize_with = "present")]
            status: Option<String>,
            #[serde(rename = "takingAmount", default, deserialize_with = "present")]
            taking_amount: Option<String>,
            #[serde(rename = "makingAmount", default, deserialize_with = "present")]
            making_amount: Option<String>,
        }
        let fields = Fields::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            success: fields.success,
            error_msg: fields.error_msg,
            order_id: fields.order_id,
            transactions_hashes: fields.transactions_hashes,
            status: fields.status,
            taking_amount: fields.taking_amount,
            making_amount: fields.making_amount,
        })
    }
}
impl JsNumberOutput for PolymarketOrderResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { success, error_msg, order_id, transactions_hashes, status, taking_amount, making_amount })
    }
}

/// CLOB wire metadata; probability, payoff and market mapping belong to prediction-core.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PolymarketClobMarket {
    pub condition_id: String,
    pub token_id: String,
    pub outcome: String,
    pub minimum_order_size: String,
    pub minimum_tick_size: String,
    pub active: bool,
    pub closed: bool,
}
impl<'de> Deserialize<'de> for PolymarketClobMarket {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            condition_id: String,
            token_id: String,
            outcome: String,
            minimum_order_size: String,
            minimum_tick_size: String,
            active: bool,
            closed: bool,
        }
        let fields = Fields::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            condition_id: fields.condition_id,
            token_id: fields.token_id,
            outcome: fields.outcome,
            minimum_order_size: fields.minimum_order_size,
            minimum_tick_size: fields.minimum_tick_size,
            active: fields.active,
            closed: fields.closed,
        })
    }
}
impl JsNumberOutput for PolymarketClobMarket {
    fn check_number_output(&self) {
        js_number_output!(self, struct { condition_id, token_id, outcome, minimum_order_size, minimum_tick_size, active, closed })
    }
}
