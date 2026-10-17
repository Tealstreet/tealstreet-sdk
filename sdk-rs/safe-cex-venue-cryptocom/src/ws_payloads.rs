//! Typed websocket payload field carriers for cryptocom.types.ts.
//! Strings retain every UTF-16 code unit; numbers retain every JavaScript Number.
//! from_value projects captured native primitives into these static field carriers.
//! It is not a TypeScript runtime validator: interfaces have no runtime body.
//! JSON/property bindings, own-key presence and object/array alias identity remain external.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CryptocomWsValuationChannel {
    Mark,
    Index,
    Funding,
    Estimatedfunding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CryptocomWsSide {
    Buy,
    Sell,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CryptocomWsRefPriceType {
    MarkPrice,
    IndexPrice,
    LastPrice,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CryptocomWsPositionType {
    PerpetualSwap,
    LongPosition,
    ShortPosition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CryptocomWsIsolationType {
    IsolatedMargin,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CryptocomWsTickerUpdate {
    pub instrument_name: Vec<u16>,
    pub subscription: Vec<u16>,
    pub channel: Vec<u16>,
    pub data: Vec<CryptocomWsTickerData>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CryptocomWsTickerData {
    pub i: Vec<u16>,
    pub b: Vec<u16>,
    pub k: Vec<u16>,
    pub a: Vec<u16>,
    pub c: Vec<u16>,
    pub h: Vec<u16>,
    pub l: Vec<u16>,
    pub v: Vec<u16>,
    pub vv: Vec<u16>,
    pub oi: Option<Vec<u16>>,
    pub t: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CryptocomWsValuationUpdate {
    pub instrument_name: Vec<u16>,
    pub subscription: Vec<u16>,
    pub channel: CryptocomWsValuationChannel,
    pub data: Vec<CryptocomWsValuationData>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CryptocomWsValuationData {
    pub v: Vec<u16>,
    pub t: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CryptocomWsTradeUpdate {
    pub instrument_name: Vec<u16>,
    pub subscription: Vec<u16>,
    pub channel: Vec<u16>,
    pub data: Vec<CryptocomWsTradeData>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CryptocomWsTradeData {
    pub d: f64,
    pub t: f64,
    pub p: f64,
    pub q: f64,
    pub s: CryptocomWsSide,
    pub i: Vec<u16>,
    #[allow(non_snake_case)]
    pub dataTime: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CryptocomWsBookUpdate {
    pub instrument_name: Vec<u16>,
    pub subscription: Vec<u16>,
    pub channel: Vec<u16>,
    pub data: Vec<CryptocomWsBookData>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CryptocomWsBookData {
    pub bids: Vec<[Vec<u16>; 3]>,
    pub asks: Vec<[Vec<u16>; 3]>,
    pub t: f64,
    pub tt: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CryptocomWsCandlestickUpdate {
    pub instrument_name: Vec<u16>,
    pub subscription: Vec<u16>,
    pub channel: Vec<u16>,
    pub data: Vec<CryptocomWsCandlestickData>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CryptocomWsCandlestickData {
    pub t: f64,
    pub o: Vec<u16>,
    pub h: Vec<u16>,
    pub l: Vec<u16>,
    pub c: Vec<u16>,
    pub v: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CryptocomWsOrderUpdate {
    pub subscription: Vec<u16>,
    pub channel: Vec<u16>,
    pub data: Vec<CryptocomWsOrderData>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CryptocomWsOrderData {
    pub account_id: Option<Vec<u16>>,
    pub order_id: Vec<u16>,
    pub client_oid: Option<Vec<u16>>,
    pub order_type: Vec<u16>,
    pub time_in_force: Vec<u16>,
    pub side: CryptocomWsSide,
    pub exec_inst: Option<Vec<Vec<u16>>>,
    pub quantity: Vec<u16>,
    pub limit_price: Option<Vec<u16>>,
    pub trigger_price: Option<Vec<u16>>,
    pub ref_price: Option<Vec<u16>>,
    pub ref_price_type: Option<CryptocomWsRefPriceType>,
    pub avg_price: Option<Vec<u16>>,
    pub cumulative_quantity: Option<Vec<u16>>,
    pub cumulative_value: Option<Vec<u16>>,
    pub fee_currency: Option<Vec<u16>>,
    pub fee_instrument_name: Option<Vec<u16>>,
    pub create_time: Option<f64>,
    pub update_time: Option<f64>,
    pub time_created: Option<f64>,
    pub time_updated: Option<f64>,
    pub status: Option<Vec<u16>>,
    pub order_status: Option<Vec<u16>>,
    pub instrument_name: Vec<u16>,
    pub reason: Option<f64>,
    pub update_user_id: Option<Vec<u16>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CryptocomWsPositionUpdate {
    pub subscription: Vec<u16>,
    pub channel: Vec<u16>,
    pub data: Vec<CryptocomWsPositionData>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CryptocomWsPositionData {
    pub account_id: Option<Vec<u16>>,
    pub quantity: Vec<u16>,
    pub cost: Vec<u16>,
    pub open_position_pnl: Vec<u16>,
    pub session_pnl: Option<Vec<u16>>,
    pub instrument_name: Vec<u16>,
    pub open_pos_cost: Option<Vec<u16>>,
    pub update_timestamp_ms: Option<f64>,
    pub r#type: CryptocomWsPositionType,
    pub mark_price: Option<Vec<u16>>,
    pub target_leverage: Option<Vec<u16>>,
    pub leverage: Option<Vec<u16>>,
    pub liquidation_price: Option<Vec<u16>>,
    pub isolated_margin_balance: Option<Vec<u16>>,
    pub isolation_id: Option<Vec<u16>>,
    pub isolation_type: Option<CryptocomWsIsolationType>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CryptocomWsBalanceUpdate {
    pub subscription: Vec<u16>,
    pub channel: Vec<u16>,
    pub data: Vec<CryptocomWsBalanceSummary>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CryptocomWsBalanceSummary {
    pub total_available_balance: Vec<u16>,
    pub total_margin_balance: Vec<u16>,
    pub total_initial_margin: Vec<u16>,
    pub total_maintenance_margin: Vec<u16>,
    pub total_cash_balance: Vec<u16>,
    pub total_collateral_value: Vec<u16>,
    pub total_session_unrealized_pnl: Vec<u16>,
    pub total_session_realized_pnl: Vec<u16>,
    pub total_position_cost: Vec<u16>,
    pub instrument_name: Vec<u16>,
    pub position_balances: Vec<CryptocomWsPositionBalance>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CryptocomWsPositionBalance {
    pub instrument_name: Vec<u16>,
    pub quantity: Vec<u16>,
    pub reserved_qty: Vec<u16>,
    pub collateral_amount: Vec<u16>,
    pub collateral_eligible: bool,
    pub haircut: Vec<u16>,
    pub market_value: Vec<u16>,
    pub max_withdrawal_balance: Vec<u16>,
    pub hourly_interest_rate: Vec<u16>,
}

/// Native primitive ingress. A JSON/property binding must preserve these values first.
#[derive(Clone, Debug, PartialEq)]
pub enum PayloadValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Vec<u16>),
    Array(Vec<PayloadValue>),
    Object(std::collections::BTreeMap<Vec<u16>, PayloadValue>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayloadError(pub &'static str);
fn field<'a>(value: &'a PayloadValue, key: &str) -> Result<&'a PayloadValue, PayloadError> {
    match value {
        PayloadValue::Object(fields) => Ok(fields
            .get(&key.encode_utf16().collect::<Vec<_>>())
            .unwrap_or(&PayloadValue::Undefined)),
        _ => Err(PayloadError("object")),
    }
}
fn string(value: &PayloadValue) -> Result<Vec<u16>, PayloadError> {
    match value {
        PayloadValue::String(value) => Ok(value.clone()),
        _ => Err(PayloadError("string")),
    }
}
fn number(value: &PayloadValue) -> Result<f64, PayloadError> {
    match value {
        PayloadValue::Number(value) => Ok(*value),
        _ => Err(PayloadError("number")),
    }
}
fn boolean(value: &PayloadValue) -> Result<bool, PayloadError> {
    match value {
        PayloadValue::Bool(value) => Ok(*value),
        _ => Err(PayloadError("boolean")),
    }
}
fn array<T>(
    value: &PayloadValue,
    mut decode: impl FnMut(&PayloadValue) -> Result<T, PayloadError>,
) -> Result<Vec<T>, PayloadError> {
    match value {
        PayloadValue::Array(values) => values.iter().map(&mut decode).collect(),
        _ => Err(PayloadError("array")),
    }
}
fn tuple(value: &PayloadValue) -> Result<[Vec<u16>; 3], PayloadError> {
    match value {
        PayloadValue::Array(values) if values.len() == 3 => Ok([
            string(&values[0])?,
            string(&values[1])?,
            string(&values[2])?,
        ]),
        _ => Err(PayloadError("three-string tuple")),
    }
}
fn optional<T>(
    value: &PayloadValue,
    decode: impl FnOnce(&PayloadValue) -> Result<T, PayloadError>,
) -> Result<Option<T>, PayloadError> {
    if matches!(value, PayloadValue::Undefined) {
        Ok(None)
    } else {
        decode(value).map(Some)
    }
}
impl CryptocomWsValuationChannel {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        let value = string(value)?;
        if value == "mark".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Mark);
        }
        if value == "index".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Index);
        }
        if value == "funding".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Funding);
        }
        if value == "estimatedfunding".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Estimatedfunding);
        }
        Err(PayloadError("string literal"))
    }
}
impl CryptocomWsSide {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        let value = string(value)?;
        if value == "BUY".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Buy);
        }
        if value == "SELL".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Sell);
        }
        Err(PayloadError("string literal"))
    }
}
impl CryptocomWsRefPriceType {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        let value = string(value)?;
        if value == "MARK_PRICE".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::MarkPrice);
        }
        if value == "INDEX_PRICE".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::IndexPrice);
        }
        if value == "LAST_PRICE".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::LastPrice);
        }
        Err(PayloadError("string literal"))
    }
}
impl CryptocomWsPositionType {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        let value = string(value)?;
        if value == "PERPETUAL_SWAP".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::PerpetualSwap);
        }
        if value == "LONG_POSITION".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::LongPosition);
        }
        if value == "SHORT_POSITION".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::ShortPosition);
        }
        Err(PayloadError("string literal"))
    }
}
impl CryptocomWsIsolationType {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        let value = string(value)?;
        if value == "ISOLATED_MARGIN".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::IsolatedMargin);
        }
        Err(PayloadError("string literal"))
    }
}
impl CryptocomWsTickerUpdate {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        Ok(Self {
            instrument_name: (string)(field(value, "instrument_name")?)?,
            subscription: (string)(field(value, "subscription")?)?,
            channel: (string)(field(value, "channel")?)?,
            data: array(field(value, "data")?, CryptocomWsTickerData::from_value)?,
        })
    }
}
impl CryptocomWsTickerData {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        Ok(Self {
            i: (string)(field(value, "i")?)?,
            b: (string)(field(value, "b")?)?,
            k: (string)(field(value, "k")?)?,
            a: (string)(field(value, "a")?)?,
            c: (string)(field(value, "c")?)?,
            h: (string)(field(value, "h")?)?,
            l: (string)(field(value, "l")?)?,
            v: (string)(field(value, "v")?)?,
            vv: (string)(field(value, "vv")?)?,
            oi: optional(field(value, "oi")?, string)?,
            t: (number)(field(value, "t")?)?,
        })
    }
}
impl CryptocomWsValuationUpdate {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        Ok(Self {
            instrument_name: (string)(field(value, "instrument_name")?)?,
            subscription: (string)(field(value, "subscription")?)?,
            channel: (CryptocomWsValuationChannel::from_value)(field(value, "channel")?)?,
            data: array(field(value, "data")?, CryptocomWsValuationData::from_value)?,
        })
    }
}
impl CryptocomWsValuationData {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        Ok(Self {
            v: (string)(field(value, "v")?)?,
            t: (number)(field(value, "t")?)?,
        })
    }
}
impl CryptocomWsTradeUpdate {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        Ok(Self {
            instrument_name: (string)(field(value, "instrument_name")?)?,
            subscription: (string)(field(value, "subscription")?)?,
            channel: (string)(field(value, "channel")?)?,
            data: array(field(value, "data")?, CryptocomWsTradeData::from_value)?,
        })
    }
}
impl CryptocomWsTradeData {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        Ok(Self {
            d: (number)(field(value, "d")?)?,
            t: (number)(field(value, "t")?)?,
            p: (number)(field(value, "p")?)?,
            q: (number)(field(value, "q")?)?,
            s: (CryptocomWsSide::from_value)(field(value, "s")?)?,
            i: (string)(field(value, "i")?)?,
            dataTime: (number)(field(value, "dataTime")?)?,
        })
    }
}
impl CryptocomWsBookUpdate {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        Ok(Self {
            instrument_name: (string)(field(value, "instrument_name")?)?,
            subscription: (string)(field(value, "subscription")?)?,
            channel: (string)(field(value, "channel")?)?,
            data: array(field(value, "data")?, CryptocomWsBookData::from_value)?,
        })
    }
}
impl CryptocomWsBookData {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        Ok(Self {
            bids: array(field(value, "bids")?, tuple)?,
            asks: array(field(value, "asks")?, tuple)?,
            t: (number)(field(value, "t")?)?,
            tt: (number)(field(value, "tt")?)?,
        })
    }
}
impl CryptocomWsCandlestickUpdate {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        Ok(Self {
            instrument_name: (string)(field(value, "instrument_name")?)?,
            subscription: (string)(field(value, "subscription")?)?,
            channel: (string)(field(value, "channel")?)?,
            data: array(
                field(value, "data")?,
                CryptocomWsCandlestickData::from_value,
            )?,
        })
    }
}
impl CryptocomWsCandlestickData {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        Ok(Self {
            t: (number)(field(value, "t")?)?,
            o: (string)(field(value, "o")?)?,
            h: (string)(field(value, "h")?)?,
            l: (string)(field(value, "l")?)?,
            c: (string)(field(value, "c")?)?,
            v: (string)(field(value, "v")?)?,
        })
    }
}
impl CryptocomWsOrderUpdate {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        Ok(Self {
            subscription: (string)(field(value, "subscription")?)?,
            channel: (string)(field(value, "channel")?)?,
            data: array(field(value, "data")?, CryptocomWsOrderData::from_value)?,
        })
    }
}
impl CryptocomWsOrderData {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        Ok(Self {
            account_id: optional(field(value, "account_id")?, string)?,
            order_id: (string)(field(value, "order_id")?)?,
            client_oid: optional(field(value, "client_oid")?, string)?,
            order_type: (string)(field(value, "order_type")?)?,
            time_in_force: (string)(field(value, "time_in_force")?)?,
            side: (CryptocomWsSide::from_value)(field(value, "side")?)?,
            exec_inst: optional(field(value, "exec_inst")?, |v| array(v, string))?,
            quantity: (string)(field(value, "quantity")?)?,
            limit_price: optional(field(value, "limit_price")?, string)?,
            trigger_price: optional(field(value, "trigger_price")?, string)?,
            ref_price: optional(field(value, "ref_price")?, string)?,
            ref_price_type: optional(
                field(value, "ref_price_type")?,
                CryptocomWsRefPriceType::from_value,
            )?,
            avg_price: optional(field(value, "avg_price")?, string)?,
            cumulative_quantity: optional(field(value, "cumulative_quantity")?, string)?,
            cumulative_value: optional(field(value, "cumulative_value")?, string)?,
            fee_currency: optional(field(value, "fee_currency")?, string)?,
            fee_instrument_name: optional(field(value, "fee_instrument_name")?, string)?,
            create_time: optional(field(value, "create_time")?, number)?,
            update_time: optional(field(value, "update_time")?, number)?,
            time_created: optional(field(value, "time_created")?, number)?,
            time_updated: optional(field(value, "time_updated")?, number)?,
            status: optional(field(value, "status")?, string)?,
            order_status: optional(field(value, "order_status")?, string)?,
            instrument_name: (string)(field(value, "instrument_name")?)?,
            reason: optional(field(value, "reason")?, number)?,
            update_user_id: optional(field(value, "update_user_id")?, string)?,
        })
    }
}
impl CryptocomWsPositionUpdate {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        Ok(Self {
            subscription: (string)(field(value, "subscription")?)?,
            channel: (string)(field(value, "channel")?)?,
            data: array(field(value, "data")?, CryptocomWsPositionData::from_value)?,
        })
    }
}
impl CryptocomWsPositionData {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        Ok(Self {
            account_id: optional(field(value, "account_id")?, string)?,
            quantity: (string)(field(value, "quantity")?)?,
            cost: (string)(field(value, "cost")?)?,
            open_position_pnl: (string)(field(value, "open_position_pnl")?)?,
            session_pnl: optional(field(value, "session_pnl")?, string)?,
            instrument_name: (string)(field(value, "instrument_name")?)?,
            open_pos_cost: optional(field(value, "open_pos_cost")?, string)?,
            update_timestamp_ms: optional(field(value, "update_timestamp_ms")?, number)?,
            r#type: (CryptocomWsPositionType::from_value)(field(value, "type")?)?,
            mark_price: optional(field(value, "mark_price")?, string)?,
            target_leverage: optional(field(value, "target_leverage")?, string)?,
            leverage: optional(field(value, "leverage")?, string)?,
            liquidation_price: optional(field(value, "liquidation_price")?, string)?,
            isolated_margin_balance: optional(field(value, "isolated_margin_balance")?, string)?,
            isolation_id: optional(field(value, "isolation_id")?, string)?,
            isolation_type: optional(
                field(value, "isolation_type")?,
                CryptocomWsIsolationType::from_value,
            )?,
        })
    }
}
impl CryptocomWsBalanceUpdate {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        Ok(Self {
            subscription: (string)(field(value, "subscription")?)?,
            channel: (string)(field(value, "channel")?)?,
            data: array(field(value, "data")?, CryptocomWsBalanceSummary::from_value)?,
        })
    }
}
impl CryptocomWsBalanceSummary {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        Ok(Self {
            total_available_balance: (string)(field(value, "total_available_balance")?)?,
            total_margin_balance: (string)(field(value, "total_margin_balance")?)?,
            total_initial_margin: (string)(field(value, "total_initial_margin")?)?,
            total_maintenance_margin: (string)(field(value, "total_maintenance_margin")?)?,
            total_cash_balance: (string)(field(value, "total_cash_balance")?)?,
            total_collateral_value: (string)(field(value, "total_collateral_value")?)?,
            total_session_unrealized_pnl: (string)(field(value, "total_session_unrealized_pnl")?)?,
            total_session_realized_pnl: (string)(field(value, "total_session_realized_pnl")?)?,
            total_position_cost: (string)(field(value, "total_position_cost")?)?,
            instrument_name: (string)(field(value, "instrument_name")?)?,
            position_balances: array(
                field(value, "position_balances")?,
                CryptocomWsPositionBalance::from_value,
            )?,
        })
    }
}
impl CryptocomWsPositionBalance {
    pub fn from_value(value: &PayloadValue) -> Result<Self, PayloadError> {
        Ok(Self {
            instrument_name: (string)(field(value, "instrument_name")?)?,
            quantity: (string)(field(value, "quantity")?)?,
            reserved_qty: (string)(field(value, "reserved_qty")?)?,
            collateral_amount: (string)(field(value, "collateral_amount")?)?,
            collateral_eligible: (boolean)(field(value, "collateral_eligible")?)?,
            haircut: (string)(field(value, "haircut")?)?,
            market_value: (string)(field(value, "market_value")?)?,
            max_withdrawal_balance: (string)(field(value, "max_withdrawal_balance")?)?,
            hourly_interest_rate: (string)(field(value, "hourly_interest_rate")?)?,
        })
    }
}

#[cfg(test)]
mod accepted_ws_field {
    use serde_json::Value;
    #[test]
    fn field_native_contract() {
        let cases:Vec<Value>=serde_json::from_str(r###"[{"input":{"value":{"k":"o","v":{"found":{"k":"n","v":"8000000000000000"},"other":{"k":"b","v":false}}},"key":"found"},"output":{"ok":true,"value":{"kind":"number","bits":"8000000000000000"},"reference":true}},{"input":{"value":{"k":"o","v":{"found":{"k":"n","v":"4031800000000000"}}},"key":"found"},"output":{"ok":true,"value":{"kind":"number","bits":"4031800000000000"},"reference":true}},{"input":{"value":{"k":"o","v":{"found":{"k":"n","v":"7ff0000000000000"}}},"key":"found"},"output":{"ok":true,"value":{"kind":"number","bits":"7ff0000000000000"},"reference":true}},{"input":{"value":{"k":"o","v":{"found":{"k":"n","v":"7ff8000000000000"}}},"key":"found"},"output":{"ok":true,"value":{"kind":"number","bits":"7ff8000000000000"},"reference":true}},{"input":{"value":{"k":"o","v":{"found":{"k":"s","v":[55296,97,56320]}}},"key":"found"},"output":{"ok":true,"value":{"kind":"string","units":[55296,97,56320]},"reference":true}},{"input":{"value":{"k":"o","v":{"found":{"k":"b","v":false}}},"key":"found"},"output":{"ok":true,"value":{"kind":"boolean","value":false},"reference":true}},{"input":{"value":{"k":"o","v":{"found":{"k":"null"}}},"key":"found"},"output":{"ok":true,"value":{"kind":"null"},"reference":true}},{"input":{"value":{"k":"o","v":{"found":{"k":"u"}}},"key":"found"},"output":{"ok":true,"value":{"kind":"undefined"},"reference":true}},{"input":{"value":{"k":"o","v":{"found":{"k":"a","v":[{"k":"s","v":[65]},{"k":"n","v":"8000000000000000"}]}}},"key":"found"},"output":{"ok":true,"value":{"kind":"array","items":[{"kind":"string","units":[65]},{"kind":"number","bits":"8000000000000000"}]},"reference":true}},{"input":{"value":{"k":"o","v":{"found":{"k":"o","v":{"inner":{"k":"s","v":[88]}}}}},"key":"found"},"output":{"ok":true,"value":{"kind":"object","fields":[{"key_units":[105,110,110,101,114],"value":{"kind":"string","units":[88]}}]},"reference":true}},{"input":{"value":{"k":"o","v":{"found":{"k":"s","v":[65]}}},"key":"missing"},"output":{"ok":true,"value":{"kind":"undefined"},"reference":false}},{"input":{"value":{"k":"o","v":{}},"key":"constructor"},"output":{"ok":true,"value":{"kind":"undefined"},"reference":false}},{"input":{"value":{"k":"o","v":{"":{"k":"s","v":[]}}},"key":""},"output":{"ok":true,"value":{"kind":"string","units":[]},"reference":true}},{"input":{"value":{"k":"o","v":{"\u03a9\ud83d\ude80":{"k":"n","v":"fff0000000000000"}}},"key":"\u03a9\ud83d\ude80"},"output":{"ok":true,"value":{"kind":"number","bits":"fff0000000000000"},"reference":true}},{"input":{"value":{"k":"u"},"key":"found"},"output":{"ok":false}},{"input":{"value":{"k":"null"},"key":"found"},"output":{"ok":false}},{"input":{"value":{"k":"b","v":true},"key":"found"},"output":{"ok":false}},{"input":{"value":{"k":"n","v":"8000000000000000"},"key":"found"},"output":{"ok":false}},{"input":{"value":{"k":"s","v":[65]},"key":"found"},"output":{"ok":false}},{"input":{"value":{"k":"a","v":[{"k":"s","v":[65]}]},"key":"found"},"output":{"ok":false}}]"###).unwrap();
        assert_eq!(cases.len(), 20, "ws field independent fixture count");
        let mut executed = 0usize;
        for case in cases {
            let value = &case["input"];
            let observed = {
                fn payload(v: &serde_json::Value) -> crate::ws_payloads::PayloadValue {
                    use crate::ws_payloads::PayloadValue as P;
                    match v["k"].as_str().unwrap() {
                        "u" => P::Undefined,
                        "null" => P::Null,
                        "b" => P::Bool(v["v"].as_bool().unwrap()),
                        "n" => P::Number(f64::from_bits(
                            u64::from_str_radix(v["v"].as_str().unwrap(), 16).unwrap(),
                        )),
                        "s" => P::String(
                            v["v"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
                                .collect(),
                        ),
                        "a" => P::Array(v["v"].as_array().unwrap().iter().map(payload).collect()),
                        "o" => P::Object(
                            v["v"]
                                .as_object()
                                .unwrap()
                                .iter()
                                .map(|(k, v)| (k.encode_utf16().collect(), payload(v)))
                                .collect(),
                        ),
                        _ => panic!("unclassified captured primitive fixture"),
                    }
                }
                fn native_payload(v: &crate::ws_payloads::PayloadValue) -> serde_json::Value {
                    use crate::ws_payloads::PayloadValue as P;
                    match v {
                        P::Undefined => serde_json::json!({"kind":"undefined"}),
                        P::Null => serde_json::json!({"kind":"null"}),
                        P::Bool(value) => serde_json::json!({"kind":"boolean","value":value}),
                        P::Number(value) => {
                            serde_json::json!({"kind":"number","bits":format!("{:016x}",value.to_bits())})
                        }
                        P::String(units) => serde_json::json!({"kind":"string","units":units}),
                        P::Array(items) => {
                            serde_json::json!({"kind":"array","items":items.iter().map(native_payload).collect::<Vec<_>>()})
                        }
                        P::Object(fields) => {
                            serde_json::json!({"kind":"object","fields":fields.iter().map(|(key,value)|serde_json::json!({"key_units":key,"value":native_payload(value)})).collect::<Vec<_>>()})
                        }
                    }
                }
                let input = payload(&value["value"]);
                let key = value["key"].as_str().unwrap();
                match super::field(&input, key) {
                    Ok(value) => {
                        let reference = match &input {
                            crate::ws_payloads::PayloadValue::Object(fields) => fields
                                .get(&key.encode_utf16().collect::<Vec<_>>())
                                .is_some_and(|expected| std::ptr::eq(value, expected)),
                            _ => false,
                        };
                        serde_json::json!({"ok":true,"value":native_payload(value),"reference":reference})
                    }
                    Err(_) => serde_json::json!({"ok":false}),
                }
            };
            assert_eq!(
                observed, case["output"],
                "native websocket field guard/borrow input {value}"
            );
            executed += 1;
        }
        assert_eq!(executed, 20, "ws field executed fixture count");
    }
}
