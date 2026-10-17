//! Static account/API field carriers projected from captured native member values.
//! This is not a runtime TypeScript interface validator or a JSON/property binding.
//! Primitive strings keep UTF16, numbers keep f64, unknown/record referents stay native.
use crate::ws_payloads::{
    CryptocomWsIsolationType, CryptocomWsPositionType, CryptocomWsRefPriceType, CryptocomWsSide,
    PayloadValue,
};

#[derive(Clone, Debug)]
pub struct NativeRecord<V> {
    pub original: V,
}
/// Captured primitive/member projection dependencies. `record` retains the original
/// string-keyed native object; `array` must not hold a borrow guard across callbacks.
/// Property/getter/prototype/own-key bindings are outside this static carrier slice.
pub trait AccountEnvelopeHost {
    type Value: Clone;
    type Error;
    fn field(&self, value: &Self::Value, key: &'static str) -> Result<Self::Value, Self::Error>;
    fn string(&self, value: Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn number(&self, value: Self::Value) -> Result<f64, Self::Error>;
    fn boolean(&self, value: Self::Value) -> Result<bool, Self::Error>;
    fn undefined(&self, value: &Self::Value) -> bool;
    fn record(&self, value: Self::Value) -> Result<NativeRecord<Self::Value>, Self::Error>;
    fn array<T>(
        &self,
        value: Self::Value,
        decode: impl FnMut(Self::Value) -> Result<T, Self::Error>,
    ) -> Result<Vec<T>, Self::Error>;
    fn invalid_literal(&self, name: &'static str) -> Self::Error;
}
fn optional<H: AccountEnvelopeHost, T>(
    h: &H,
    value: H::Value,
    decode: impl FnOnce(H::Value) -> Result<T, H::Error>,
) -> Result<Option<T>, H::Error> {
    if h.undefined(&value) {
        Ok(None)
    } else {
        decode(value).map(Some)
    }
}
fn strings<H: AccountEnvelopeHost>(h: &H, value: H::Value) -> Result<Vec<Vec<u16>>, H::Error> {
    h.array(value, |v| h.string(v))
}
fn equal(value: &[u16], text: &str) -> bool {
    value.iter().copied().eq(text.encode_utf16())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CryptocomRequestType {
    Limit,
    Market,
    StopLoss,
    StopLimit,
    TakeProfit,
    TakeProfitLimit,
}
impl CryptocomRequestType {
    pub fn from_units(value: &[u16]) -> Option<Self> {
        if equal(value, "LIMIT") {
            return Some(Self::Limit);
        }
        if equal(value, "MARKET") {
            return Some(Self::Market);
        }
        if equal(value, "STOP_LOSS") {
            return Some(Self::StopLoss);
        }
        if equal(value, "STOP_LIMIT") {
            return Some(Self::StopLimit);
        }
        if equal(value, "TAKE_PROFIT") {
            return Some(Self::TakeProfit);
        }
        if equal(value, "TAKE_PROFIT_LIMIT") {
            return Some(Self::TakeProfitLimit);
        }
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CryptocomRequestTimeInForce {
    GoodTillCancel,
    FillOrKill,
    ImmediateOrCancel,
    PostOnly,
}
impl CryptocomRequestTimeInForce {
    pub fn from_units(value: &[u16]) -> Option<Self> {
        if equal(value, "GOOD_TILL_CANCEL") {
            return Some(Self::GoodTillCancel);
        }
        if equal(value, "FILL_OR_KILL") {
            return Some(Self::FillOrKill);
        }
        if equal(value, "IMMEDIATE_OR_CANCEL") {
            return Some(Self::ImmediateOrCancel);
        }
        if equal(value, "POST_ONLY") {
            return Some(Self::PostOnly);
        }
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CryptocomSubscribeMethod {
    Subscribe,
}
impl CryptocomSubscribeMethod {
    pub fn from_units(value: &[u16]) -> Option<Self> {
        if equal(value, "subscribe") {
            return Some(Self::Subscribe);
        }
        None
    }
}

#[derive(Clone, Debug)]
pub struct CryptocomOrderRequest {
    pub instrument_name: Vec<u16>,
    pub side: CryptocomWsSide,
    pub r#type: CryptocomRequestType,
    pub quantity: Option<Vec<u16>>,
    pub notional: Option<Vec<u16>>,
    pub price: Option<Vec<u16>>,
    pub ref_price: Option<Vec<u16>>,
    pub ref_price_type: Option<CryptocomWsRefPriceType>,
    pub time_in_force: Option<CryptocomRequestTimeInForce>,
    pub exec_inst: Option<Vec<Vec<u16>>>,
    pub client_oid: Option<Vec<u16>>,
    pub post_only: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct CryptocomPosition {
    pub account_id: Option<Vec<u16>>,
    pub quantity: Vec<u16>,
    pub cost: Vec<u16>,
    pub open_position_pnl: Vec<u16>,
    pub session_pnl: Option<Vec<u16>>,
    pub instrument_name: Vec<u16>,
    pub open_pos_cost: Option<Vec<u16>>,
    pub update_timestamp_ms: Option<f64>,
    pub r#type: CryptocomWsPositionType,
    pub position_id: Option<Vec<u16>>,
    pub leverage: Option<Vec<u16>>,
    pub liquidation_price: Option<Vec<u16>>,
    pub mark_price: Option<Vec<u16>>,
    pub isolation_id: Option<Vec<u16>>,
    pub isolation_type: Option<CryptocomWsIsolationType>,
}

#[derive(Clone, Debug)]
pub struct CryptocomAccountSummary {
    pub accounts: Vec<CryptocomAccount>,
}

#[derive(Clone, Debug)]
pub struct CryptocomAccount {
    pub balance: f64,
    pub available_balance: f64,
    pub margin_balance: f64,
    pub order_margin: f64,
    pub position_margin: f64,
    pub currency: Vec<u16>,
    pub is_liquidating: bool,
    pub position_limit: f64,
    pub used_position_limit: f64,
    pub position_limit_usd: f64,
    pub margin_score: Vec<u16>,
    pub has_risk: bool,
    pub account_id: Option<Vec<u16>>,
}

#[derive(Clone, Debug)]
pub struct CryptocomWsRequest<V> {
    pub id: f64,
    pub method: Vec<u16>,
    pub params: Option<NativeRecord<V>>,
    pub api_key: Option<Vec<u16>>,
    pub sig: Option<Vec<u16>>,
    pub nonce: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct CryptocomWsResponse<V> {
    pub id: Option<f64>,
    pub method: Option<Vec<u16>>,
    pub code: f64,
    pub message: Option<Vec<u16>>,
    pub result: Option<V>,
}

#[derive(Clone, Debug)]
pub struct CryptocomWsSubscription {
    pub id: f64,
    pub method: CryptocomSubscribeMethod,
    pub params: CryptocomSubscriptionParams,
    pub nonce: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct CryptocomSubscriptionParams {
    pub channels: Vec<Vec<u16>>,
}

#[derive(Clone, Debug)]
pub struct CryptocomApiResponse<T> {
    pub id: Option<f64>,
    pub method: Vec<u16>,
    pub code: f64,
    pub result: Option<T>,
    pub message: Option<Vec<u16>>,
}

impl CryptocomOrderRequest {
    pub fn from_native<H: AccountEnvelopeHost>(h: &H, value: &H::Value) -> Result<Self, H::Error> {
        Ok(Self {
            instrument_name: h.string(h.field(value, "instrument_name")?)?,
            side: CryptocomWsSide::from_value(&PayloadValue::String(
                h.string(h.field(value, "side")?)?,
            ))
            .map_err(|_| h.invalid_literal("CryptocomWsSide"))?,
            r#type: CryptocomRequestType::from_units(&h.string(h.field(value, "type")?)?)
                .ok_or_else(|| h.invalid_literal("CryptocomRequestType"))?,
            quantity: optional(h, h.field(value, "quantity")?, |v| h.string(v))?,
            notional: optional(h, h.field(value, "notional")?, |v| h.string(v))?,
            price: optional(h, h.field(value, "price")?, |v| h.string(v))?,
            ref_price: optional(h, h.field(value, "ref_price")?, |v| h.string(v))?,
            ref_price_type: optional(h, h.field(value, "ref_price_type")?, |v| {
                CryptocomWsRefPriceType::from_value(&PayloadValue::String(h.string(v)?))
                    .map_err(|_| h.invalid_literal("CryptocomWsRefPriceType"))
            })?,
            time_in_force: optional(h, h.field(value, "time_in_force")?, |v| {
                CryptocomRequestTimeInForce::from_units(&h.string(v)?)
                    .ok_or_else(|| h.invalid_literal("CryptocomRequestTimeInForce"))
            })?,
            exec_inst: optional(h, h.field(value, "exec_inst")?, |v| strings(h, v))?,
            client_oid: optional(h, h.field(value, "client_oid")?, |v| h.string(v))?,
            post_only: optional(h, h.field(value, "post_only")?, |v| h.boolean(v))?,
        })
    }
}

impl CryptocomPosition {
    pub fn from_native<H: AccountEnvelopeHost>(h: &H, value: &H::Value) -> Result<Self, H::Error> {
        Ok(Self {
            account_id: optional(h, h.field(value, "account_id")?, |v| h.string(v))?,
            quantity: h.string(h.field(value, "quantity")?)?,
            cost: h.string(h.field(value, "cost")?)?,
            open_position_pnl: h.string(h.field(value, "open_position_pnl")?)?,
            session_pnl: optional(h, h.field(value, "session_pnl")?, |v| h.string(v))?,
            instrument_name: h.string(h.field(value, "instrument_name")?)?,
            open_pos_cost: optional(h, h.field(value, "open_pos_cost")?, |v| h.string(v))?,
            update_timestamp_ms: optional(h, h.field(value, "update_timestamp_ms")?, |v| {
                h.number(v)
            })?,
            r#type: CryptocomWsPositionType::from_value(&PayloadValue::String(
                h.string(h.field(value, "type")?)?,
            ))
            .map_err(|_| h.invalid_literal("CryptocomWsPositionType"))?,
            position_id: optional(h, h.field(value, "position_id")?, |v| h.string(v))?,
            leverage: optional(h, h.field(value, "leverage")?, |v| h.string(v))?,
            liquidation_price: optional(h, h.field(value, "liquidation_price")?, |v| h.string(v))?,
            mark_price: optional(h, h.field(value, "mark_price")?, |v| h.string(v))?,
            isolation_id: optional(h, h.field(value, "isolation_id")?, |v| h.string(v))?,
            isolation_type: optional(h, h.field(value, "isolation_type")?, |v| {
                CryptocomWsIsolationType::from_value(&PayloadValue::String(h.string(v)?))
                    .map_err(|_| h.invalid_literal("CryptocomWsIsolationType"))
            })?,
        })
    }
}

impl CryptocomAccountSummary {
    pub fn from_native<H: AccountEnvelopeHost>(h: &H, value: &H::Value) -> Result<Self, H::Error> {
        Ok(Self {
            accounts: h.array(h.field(value, "accounts")?, |v| {
                CryptocomAccount::from_native(h, &v)
            })?,
        })
    }
}

impl CryptocomAccount {
    pub fn from_native<H: AccountEnvelopeHost>(h: &H, value: &H::Value) -> Result<Self, H::Error> {
        Ok(Self {
            balance: h.number(h.field(value, "balance")?)?,
            available_balance: h.number(h.field(value, "available_balance")?)?,
            margin_balance: h.number(h.field(value, "margin_balance")?)?,
            order_margin: h.number(h.field(value, "order_margin")?)?,
            position_margin: h.number(h.field(value, "position_margin")?)?,
            currency: h.string(h.field(value, "currency")?)?,
            is_liquidating: h.boolean(h.field(value, "is_liquidating")?)?,
            position_limit: h.number(h.field(value, "position_limit")?)?,
            used_position_limit: h.number(h.field(value, "used_position_limit")?)?,
            position_limit_usd: h.number(h.field(value, "position_limit_usd")?)?,
            margin_score: h.string(h.field(value, "margin_score")?)?,
            has_risk: h.boolean(h.field(value, "has_risk")?)?,
            account_id: optional(h, h.field(value, "account_id")?, |v| h.string(v))?,
        })
    }
}

impl<V> CryptocomWsRequest<V> {
    pub fn from_native<H: AccountEnvelopeHost<Value = V>>(
        h: &H,
        value: &H::Value,
    ) -> Result<Self, H::Error> {
        Ok(Self {
            id: h.number(h.field(value, "id")?)?,
            method: h.string(h.field(value, "method")?)?,
            params: optional(h, h.field(value, "params")?, |v| h.record(v))?,
            api_key: optional(h, h.field(value, "api_key")?, |v| h.string(v))?,
            sig: optional(h, h.field(value, "sig")?, |v| h.string(v))?,
            nonce: optional(h, h.field(value, "nonce")?, |v| h.number(v))?,
        })
    }
}

impl<V> CryptocomWsResponse<V> {
    pub fn from_native<H: AccountEnvelopeHost<Value = V>>(
        h: &H,
        value: &H::Value,
    ) -> Result<Self, H::Error> {
        Ok(Self {
            id: optional(h, h.field(value, "id")?, |v| h.number(v))?,
            method: optional(h, h.field(value, "method")?, |v| h.string(v))?,
            code: h.number(h.field(value, "code")?)?,
            message: optional(h, h.field(value, "message")?, |v| h.string(v))?,
            result: optional(h, h.field(value, "result")?, |v| Ok::<_, H::Error>(v))?,
        })
    }
}

impl CryptocomWsSubscription {
    pub fn from_native<H: AccountEnvelopeHost>(h: &H, value: &H::Value) -> Result<Self, H::Error> {
        Ok(Self {
            id: h.number(h.field(value, "id")?)?,
            method: CryptocomSubscribeMethod::from_units(&h.string(h.field(value, "method")?)?)
                .ok_or_else(|| h.invalid_literal("CryptocomSubscribeMethod"))?,
            params: CryptocomSubscriptionParams::from_native(h, &h.field(value, "params")?)?,
            nonce: optional(h, h.field(value, "nonce")?, |v| h.number(v))?,
        })
    }
}

impl CryptocomSubscriptionParams {
    pub fn from_native<H: AccountEnvelopeHost>(h: &H, value: &H::Value) -> Result<Self, H::Error> {
        Ok(Self {
            channels: strings(h, h.field(value, "channels")?)?,
        })
    }
}

impl<T> CryptocomApiResponse<T> {
    pub fn from_native<H: AccountEnvelopeHost>(
        h: &H,
        value: &H::Value,
        decode_result: impl FnOnce(H::Value) -> Result<T, H::Error>,
    ) -> Result<Self, H::Error> {
        Ok(Self {
            id: optional(h, h.field(value, "id")?, |v| h.number(v))?,
            method: h.string(h.field(value, "method")?)?,
            code: h.number(h.field(value, "code")?)?,
            result: optional(h, h.field(value, "result")?, decode_result)?,
            message: optional(h, h.field(value, "message")?, |v| h.string(v))?,
        })
    }
}

#[cfg(test)]
mod accepted_control_flow {
    #[test]
    fn equal_native_contract() {
        let cases:Vec<serde_json::Value>=serde_json::from_str(r###"[{"input":{"value":[],"literal":""},"output":true},{"input":{"value":[109,97,114,107,101,116],"literal":"market"},"output":true},{"input":{"value":[77,97,114,107,101,116],"literal":"market"},"output":false},{"input":{"value":[109,97,114,107,101,116,88],"literal":"market"},"output":false},{"input":{"value":[109,97,114,107,101],"literal":"market"},"output":false},{"input":{"value":[],"literal":"market"},"output":false},{"input":{"value":[109,97,114,107,101,116],"literal":""},"output":false},{"input":{"value":[109,97,114,107,101,116,0],"literal":"market"},"output":false},{"input":{"value":[0],"literal":"\u0000"},"output":true},{"input":{"value":[97,0,98],"literal":"a\u0000b"},"output":true},{"input":{"value":[937],"literal":"\u03a9"},"output":true},{"input":{"value":[55357,56898],"literal":"\ud83d\ude42"},"output":true},{"input":{"value":[233],"literal":"\u00e9"},"output":true},{"input":{"value":[101,769],"literal":"\u00e9"},"output":false},{"input":{"value":[233],"literal":"e\u0301"},"output":false},{"input":{"value":[55296],"literal":""},"output":false},{"input":{"value":[56320],"literal":""},"output":false},{"input":{"value":[109,97,114,107,101,116,55296],"literal":"market"},"output":false},{"input":{"value":[56320,109,97,114,107,101,116],"literal":"market"},"output":false},{"input":{"value":[55357,56898,120],"literal":"\ud83d\ude42"},"output":false},{"input":{"value":[120,55357,56898],"literal":"\ud83d\ude42"},"output":false},{"input":{"value":[937],"literal":"\u03c9"},"output":false}]"###).unwrap();
        assert_eq!(cases.len(), 22, "envelope equal independent cases");
        let mut executed = 0;
        for case in cases {
            let value: Vec<u16> = serde_json::from_value(case["input"]["value"].clone()).unwrap();
            let literal = case["input"]["literal"].as_str().unwrap();
            assert_eq!(
                super::equal(&value, literal),
                case["output"].as_bool().unwrap(),
                "envelope equal native comparison input {}",
                case["input"]
            );
            executed += 1;
        }
        assert_eq!(executed, 22, "envelope equal executed cases");
    }
}
