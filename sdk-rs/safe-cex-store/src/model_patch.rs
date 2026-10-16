//! In-memory partial model updates; numeric values never cross a JSON serializer.
use indexmap::IndexMap;
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorePatchError {
    field: String,
    message: String,
    source_type_error: bool,
}
impl StorePatchError {
    pub(crate) fn source_type_error(field: &str, message: String) -> Self {
        Self {
            field: field.into(),
            message,
            source_type_error: true,
        }
    }
    pub fn is_type_error(&self) -> bool {
        self.source_type_error
    }

    pub(crate) fn invalid_index() -> Self {
        error("$index", "ticker delta index must address a row")
    }
    pub fn field(&self) -> &str {
        &self.field
    }
    pub fn message(&self) -> &str {
        &self.message
    }
}
impl std::fmt::Display for StorePatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.source_type_error {
            return f.write_str(&self.message);
        }
        write!(
            f,
            "invalid store patch field {}: {}",
            self.field, self.message
        )
    }
}
impl std::error::Error for StorePatchError {}

#[derive(Clone)]
pub enum PatchValue {
    Json(Value),
    Number(f64),
    Native(std::sync::Arc<dyn std::any::Any + Send + Sync>),
}
#[derive(Clone, Copy)]
pub enum PatchValueRef<'a> {
    Json(&'a Value),
    Number(f64),
    Native(&'a (dyn std::any::Any + Send + Sync)),
}
/// A JSON patch and a numeric patch share field iteration, without collapsing NaN to null.
mod sealed {
    pub trait Sealed {}
    impl Sealed for super::ModelPatch {}
    impl Sealed for serde_json::Map<String, serde_json::Value> {}
}
// Sealed sources guarantee stable, side-effect-free field iteration.
pub trait PatchSource: sealed::Sealed {
    fn visit(
        &self,
        visitor: &mut dyn FnMut(&str, PatchValueRef<'_>) -> Result<(), StorePatchError>,
    ) -> Result<(), StorePatchError>;
}
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModelPatch(pub IndexMap<String, PatchValue>);
impl ModelPatch {
    pub fn set_native<T: std::any::Any + Send + Sync>(
        &mut self,
        field: impl Into<String>,
        value: T,
    ) {
        self.0
            .insert(field.into(), PatchValue::Native(std::sync::Arc::new(value)));
    }
    pub fn set_number(&mut self, field: impl Into<String>, value: f64) {
        self.0.insert(field.into(), PatchValue::Number(value));
    }
    pub fn set_json(&mut self, field: impl Into<String>, value: Value) {
        self.0.insert(field.into(), PatchValue::Json(value));
    }
}
impl From<Map<String, Value>> for ModelPatch {
    fn from(value: Map<String, Value>) -> Self {
        Self(
            value
                .into_iter()
                .map(|(key, value)| (key, PatchValue::Json(value)))
                .collect(),
        )
    }
}
impl PatchSource for ModelPatch {
    fn visit(
        &self,
        visitor: &mut dyn FnMut(&str, PatchValueRef<'_>) -> Result<(), StorePatchError>,
    ) -> Result<(), StorePatchError> {
        for (key, value) in &self.0 {
            visitor(
                key,
                match value {
                    PatchValue::Json(value) => PatchValueRef::Json(value),
                    PatchValue::Number(value) => PatchValueRef::Number(*value),
                    PatchValue::Native(value) => PatchValueRef::Native(value.as_ref()),
                },
            )?;
        }
        Ok(())
    }
}
impl PatchSource for Map<String, Value> {
    fn visit(
        &self,
        visitor: &mut dyn FnMut(&str, PatchValueRef<'_>) -> Result<(), StorePatchError>,
    ) -> Result<(), StorePatchError> {
        for (key, value) in self {
            visitor(key, PatchValueRef::Json(value))?;
        }
        Ok(())
    }
}
fn error(field: &str, message: impl Into<String>) -> StorePatchError {
    StorePatchError {
        field: field.into(),
        message: message.into(),
        source_type_error: false,
    }
}
fn number(field: &str, value: PatchValueRef<'_>) -> Result<f64, StorePatchError> {
    match value {
        PatchValueRef::Number(value) => Ok(value),
        PatchValueRef::Native(value) => value
            .downcast_ref::<f64>()
            .copied()
            .ok_or_else(|| error(field, "native value has wrong numeric type")),
        PatchValueRef::Json(Value::Number(value)) => {
            Ok(safe_cex_num::number_from_string(&value.to_string()))
        }
        _ => Err(error(field, "expected number")),
    }
}
fn decode<T: serde::de::DeserializeOwned + Clone + 'static>(
    field: &str,
    value: PatchValueRef<'_>,
) -> Result<T, StorePatchError> {
    match value {
        PatchValueRef::Json(value) => {
            serde_json::from_value(value.clone()).map_err(|e| error(field, e.to_string()))
        }
        PatchValueRef::Native(value) => value
            .downcast_ref::<T>()
            .cloned()
            .ok_or_else(|| error(field, "native value has wrong field type")),
        PatchValueRef::Number(_) => Err(error(
            field,
            "native numeric patch applied to non-numeric field",
        )),
    }
}
fn passthrough_value(field: &str, value: PatchValueRef<'_>) -> Result<Value, StorePatchError> {
    match value {
        PatchValueRef::Json(value) => Ok(value.clone()),
        value => decode(field, value),
    }
}
pub(crate) trait PatchModel {
    fn patch_field(&mut self, field: &str, value: PatchValueRef<'_>)
    -> Result<(), StorePatchError>;
}
pub(crate) fn apply<T: PatchModel + Clone>(
    target: &mut T,
    changes: &dyn PatchSource,
) -> Result<(), StorePatchError> {
    let mut candidate = target.clone();
    changes.visit(&mut |field, value| candidate.patch_field(field, value))?;
    *target = candidate;
    Ok(())
}

impl PatchModel for safe_cex_types::Order {
    fn patch_field(
        &mut self,
        field: &str,
        value: PatchValueRef<'_>,
    ) -> Result<(), StorePatchError> {
        match field {
            "id" => self.id = decode(field, value)?,
            "parentId" => self.parent_id = Some(decode(field, value)?),
            "status" => self.status = decode(field, value)?,
            "symbol" => self.symbol = decode(field, value)?,
            "type" => self.r#type = decode(field, value)?,
            "side" => self.side = decode(field, value)?,
            "price" => self.price = number(field, value)?,
            "amount" => self.amount = number(field, value)?,
            "filled" => self.filled = number(field, value)?,
            "remaining" => self.remaining = number(field, value)?,
            "reduceOnly" => self.reduce_only = decode(field, value)?,
            "close" => self.close = Some(decode(field, value)?),
            "info" => self.info = Some(passthrough_value(field, value)?),
            "timeInForce" => self.time_in_force = Some(decode(field, value)?),
            "lastUpdate" => self.last_update = number(field, value)?,
            "triggerType" => self.trigger_type = Some(decode(field, value)?),
            "tpTriggerType" => self.tp_trigger_type = Some(decode(field, value)?),
            "slTriggerType" => self.sl_trigger_type = Some(decode(field, value)?),
            "takeProfit" => self.take_profit = Some(number(field, value)?),
            "stopLoss" => self.stop_loss = Some(number(field, value)?),
            "extra" => self.extra = Some(decode(field, value)?),
            _ => {}
        }
        Ok(())
    }
}

impl PatchModel for safe_cex_types::Position {
    fn patch_field(
        &mut self,
        field: &str,
        value: PatchValueRef<'_>,
    ) -> Result<(), StorePatchError> {
        match field {
            "id" => self.id = decode(field, value)?,
            "symbol" => self.symbol = decode(field, value)?,
            "side" => self.side = decode(field, value)?,
            "entryPrice" => self.entry_price = number(field, value)?,
            "notional" => self.notional = number(field, value)?,
            "leverage" => self.leverage = number(field, value)?,
            "marginType" => self.margin_type = Some(decode(field, value)?),
            "positionMode" => self.position_mode = Some(decode(field, value)?),
            "unrealizedPnl" => self.unrealized_pnl = number(field, value)?,
            "realizedPnl" => self.realized_pnl = Some(number(field, value)?),
            "contracts" => self.contracts = number(field, value)?,
            "liquidationPrice" => self.liquidation_price = number(field, value)?,
            "marginSize" => self.margin_size = Some(number(field, value)?),
            "info" => self.info = Some(passthrough_value(field, value)?),
            "extra" => self.extra = Some(decode(field, value)?),
            "lastUpdate" => self.last_update = Some(number(field, value)?),
            _ => {}
        }
        Ok(())
    }
}

pub(crate) fn merge_position(
    target: &mut safe_cex_types::Position,
    incoming: safe_cex_types::Position,
) {
    let old = std::mem::replace(target, incoming);
    target.margin_type = target.margin_type.take().or(old.margin_type);
    target.position_mode = target.position_mode.take().or(old.position_mode);
    target.realized_pnl = target.realized_pnl.take().or(old.realized_pnl);
    target.margin_size = target.margin_size.take().or(old.margin_size);
    target.info = target.info.take().or(old.info);
    target.extra = target.extra.take().or(old.extra);
    target.last_update = target.last_update.take().or(old.last_update);
}

impl PatchModel for safe_cex_types::Market {
    fn patch_field(
        &mut self,
        field: &str,
        value: PatchValueRef<'_>,
    ) -> Result<(), StorePatchError> {
        match field {
            "id" => self.id = decode(field, value)?,
            "symbol" => self.symbol = decode(field, value)?,
            "prettySymbol" => self.pretty_symbol = Some(decode(field, value)?),
            "base" => self.base = decode(field, value)?,
            "quote" => self.quote = decode(field, value)?,
            "active" => self.active = decode(field, value)?,
            "precision" => self.precision = decode(field, value)?,
            "limits" => self.limits = decode(field, value)?,
            "contractSize" => self.contract_size = Some(number(field, value)?),
            "isExpiring" => self.is_expiring = Some(decode(field, value)?),
            "isInverse" => self.is_inverse = Some(decode(field, value)?),
            "isSpot" => self.is_spot = Some(decode(field, value)?),
            "isPerpetual" => self.is_perpetual = Some(decode(field, value)?),
            "isReadOnly" => self.is_read_only = Some(decode(field, value)?),
            "info" => self.info = Some(passthrough_value(field, value)?),
            "takerFee" => self.taker_fee = Some(number(field, value)?),
            "makerFee" => self.maker_fee = Some(number(field, value)?),
            _ => {}
        }
        Ok(())
    }
}

pub(crate) fn merge_market(target: &mut safe_cex_types::Market, incoming: safe_cex_types::Market) {
    let old = std::mem::replace(target, incoming);
    target.pretty_symbol = target.pretty_symbol.take().or(old.pretty_symbol);
    target.contract_size = target.contract_size.take().or(old.contract_size);
    target.is_expiring = target.is_expiring.take().or(old.is_expiring);
    target.is_inverse = target.is_inverse.take().or(old.is_inverse);
    target.is_spot = target.is_spot.take().or(old.is_spot);
    target.is_read_only = target.is_read_only.take().or(old.is_read_only);
    target.info = target.info.take().or(old.info);
    target.taker_fee = target.taker_fee.take().or(old.taker_fee);
    target.maker_fee = target.maker_fee.take().or(old.maker_fee);
}

impl PatchModel for safe_cex_types::Ticker {
    fn patch_field(
        &mut self,
        field: &str,
        value: PatchValueRef<'_>,
    ) -> Result<(), StorePatchError> {
        match field {
            "id" => self.id = decode(field, value)?,
            "symbol" => self.symbol = decode(field, value)?,
            "bid" => self.bid = number(field, value)?,
            "ask" => self.ask = number(field, value)?,
            "last" => self.last = number(field, value)?,
            "mark" => self.mark = number(field, value)?,
            "index" => self.index = number(field, value)?,
            "midPrice" => self.mid_price = Some(number(field, value)?),
            "percentage" => self.percentage = number(field, value)?,
            "openInterest" => self.open_interest = number(field, value)?,
            "fundingRate" => self.funding_rate = number(field, value)?,
            "fundingTime" => self.funding_time = number(field, value)?,
            "volume" => self.volume = number(field, value)?,
            "quoteVolume" => self.quote_volume = number(field, value)?,
            "timestamp" => self.timestamp = Some(number(field, value)?),
            "info" => self.info = Some(passthrough_value(field, value)?),
            _ => {}
        }
        Ok(())
    }
}

impl<'de> serde::Deserialize<'de> for ModelPatch {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Map::<String, Value>::deserialize(safe_cex_types::strict_decode::object(deserializer))
            .map(Self::from)
    }
}

impl std::fmt::Debug for PatchValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(value) => f.debug_tuple("Json").field(value).finish(),
            Self::Number(value) => f.debug_tuple("Number").field(value).finish(),
            Self::Native(value) => f.debug_tuple("Native").field(&(**value).type_id()).finish(),
        }
    }
}
impl PartialEq for PatchValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Json(a), Self::Json(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::Native(a), Self::Native(b)) => std::sync::Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}
