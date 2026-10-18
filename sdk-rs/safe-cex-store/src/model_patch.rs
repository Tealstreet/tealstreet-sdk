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
    pub fn is_type_error(&self) -> bool {
        self.source_type_error
    }

    pub fn invalid_index() -> Self {
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
    Undefined,
    Json(Value),
    Number(f64),
    Native(std::sync::Arc<dyn std::any::Any + Send + Sync>),
}
#[derive(Clone, Copy)]
pub enum PatchValueRef<'a> {
    Undefined,
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
    pub fn set_undefined(&mut self, field: impl Into<String>) {
        self.0.insert(field.into(), PatchValue::Undefined);
    }
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
                    PatchValue::Undefined => PatchValueRef::Undefined,
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
        PatchValueRef::Undefined | PatchValueRef::Number(_) => Err(error(
            field,
            "native numeric patch applied to non-numeric field",
        )),
    }
}
fn retained_info(
    field: &str,
    value: PatchValueRef<'_>,
) -> Result<safe_cex_types::retained_json::RetainedJsonInput, StorePatchError> {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    match value {
        PatchValueRef::Undefined => Ok(Input::Undefined),
        PatchValueRef::Json(value) => Ok(value.clone().into()),
        PatchValueRef::Number(value) => Ok(Input::Float(value)),
        value => decode(field, value),
    }
}
fn model_field<T: serde::de::DeserializeOwned + Clone + 'static>(
    field: &str,
    value: PatchValueRef<'_>,
) -> Result<safe_cex_types::ModelField<T>, StorePatchError> {
    if matches!(value, PatchValueRef::Undefined) {
        Ok(safe_cex_types::ModelField::Undefined)
    } else {
        decode(field, value).map(safe_cex_types::ModelField::Value)
    }
}
fn model_number(
    field: &str,
    value: PatchValueRef<'_>,
) -> Result<safe_cex_types::ModelField<f64>, StorePatchError> {
    if matches!(value, PatchValueRef::Undefined) {
        Ok(safe_cex_types::ModelField::Undefined)
    } else {
        number(field, value).map(safe_cex_types::ModelField::Value)
    }
}
fn model_info(
    field: &str,
    value: PatchValueRef<'_>,
) -> Result<
    safe_cex_types::ModelField<safe_cex_types::retained_json::RetainedJsonInput>,
    StorePatchError,
> {
    if matches!(value, PatchValueRef::Undefined) {
        Ok(safe_cex_types::ModelField::Undefined)
    } else {
        retained_info(field, value).map(safe_cex_types::ModelField::Value)
    }
}
pub trait PatchModel {
    fn patch_field(&mut self, field: &str, value: PatchValueRef<'_>)
    -> Result<(), StorePatchError>;
}
pub fn apply<T: PatchModel + Clone>(
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
        if matches!(value, PatchValueRef::Undefined)
            && !matches!(
                field,
                "parentId"
                    | "status"
                    | "type"
                    | "side"
                    | "price"
                    | "reduceOnly"
                    | "close"
                    | "info"
                    | "clientOrderId"
                    | "timeInForce"
                    | "triggerType"
                    | "tpTriggerType"
                    | "slTriggerType"
                    | "takeProfit"
                    | "stopLoss"
                    | "extra"
            )
        {
            return Ok(());
        }
        match field {
            "id" => self.id = decode(field, value)?,
            "parentId" => self.parent_id = model_field(field, value)?,
            "status" => self.status = model_field(field, value)?,
            "symbol" => self.symbol = decode(field, value)?,
            "type" => self.r#type = model_field(field, value)?,
            "side" => self.side = model_field(field, value)?,
            "price" => self.price = model_number(field, value)?,
            "amount" => self.amount = number(field, value)?,
            "filled" => self.filled = number(field, value)?,
            "remaining" => self.remaining = number(field, value)?,
            "reduceOnly" => self.reduce_only = model_field(field, value)?,
            "close" => self.close = model_field(field, value)?,
            "info" => self.info = model_info(field, value)?,
            "clientOrderId" => self.client_order_id = model_field(field, value)?,
            "timeInForce" => self.time_in_force = model_field(field, value)?,
            "lastUpdate" => self.last_update = number(field, value)?,
            "triggerType" => self.trigger_type = model_field(field, value)?,
            "tpTriggerType" => self.tp_trigger_type = model_field(field, value)?,
            "slTriggerType" => self.sl_trigger_type = model_field(field, value)?,
            "takeProfit" => self.take_profit = model_number(field, value)?,
            "stopLoss" => self.stop_loss = model_number(field, value)?,
            "extra" => self.extra = model_field(field, value)?,
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
        if matches!(value, PatchValueRef::Undefined)
            && !matches!(
                field,
                "side"
                    | "marginType"
                    | "positionMode"
                    | "realizedPnl"
                    | "marginSize"
                    | "info"
                    | "extra"
                    | "lastUpdate"
                    | "option"
            )
        {
            return Ok(());
        }
        match field {
            "id" => self.id = decode(field, value)?,
            "symbol" => self.symbol = decode(field, value)?,
            "side" => self.side = model_field(field, value)?,
            "entryPrice" => self.entry_price = number(field, value)?,
            "notional" => self.notional = number(field, value)?,
            "leverage" => self.leverage = number(field, value)?,
            "marginType" => self.margin_type = model_field(field, value)?,
            "positionMode" => self.position_mode = model_field(field, value)?,
            "unrealizedPnl" => self.unrealized_pnl = number(field, value)?,
            "realizedPnl" => self.realized_pnl = model_number(field, value)?,
            "contracts" => self.contracts = number(field, value)?,
            "liquidationPrice" => self.liquidation_price = number(field, value)?,
            "marginSize" => self.margin_size = model_number(field, value)?,
            "info" => self.info = model_info(field, value)?,
            "extra" => self.extra = model_field(field, value)?,
            "lastUpdate" => self.last_update = model_number(field, value)?,
            "option" => self.option = model_field(field, value)?,
            _ => {}
        }
        Ok(())
    }
}

impl PatchModel for safe_cex_types::Market {
    fn patch_field(
        &mut self,
        field: &str,
        value: PatchValueRef<'_>,
    ) -> Result<(), StorePatchError> {
        if matches!(value, PatchValueRef::Undefined) && field != "info" {
            return Ok(());
        }
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
            "info" => self.info = Some(retained_info(field, value)?),
            "takerFee" => self.taker_fee = Some(number(field, value)?),
            "makerFee" => self.maker_fee = Some(number(field, value)?),
            "option" => self.option = Some(decode(field, value)?),
            _ => {}
        }
        Ok(())
    }
}

impl PatchModel for safe_cex_types::Ticker {
    fn patch_field(
        &mut self,
        field: &str,
        value: PatchValueRef<'_>,
    ) -> Result<(), StorePatchError> {
        if matches!(value, PatchValueRef::Undefined)
            && !matches!(
                field,
                "info"
                    | "bid"
                    | "ask"
                    | "last"
                    | "mark"
                    | "index"
                    | "percentage"
                    | "openInterest"
                    | "fundingRate"
                    | "fundingTime"
                    | "volume"
                    | "quoteVolume"
            )
        {
            return Ok(());
        }
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
            "info" => self.info = Some(retained_info(field, value)?),
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
            Self::Undefined => f.write_str("Undefined"),
            Self::Json(value) => f.debug_tuple("Json").field(value).finish(),
            Self::Number(value) => f.debug_tuple("Number").field(value).finish(),
            Self::Native(value) => f.debug_tuple("Native").field(&(**value).type_id()).finish(),
        }
    }
}
impl PartialEq for PatchValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Undefined, Self::Undefined) => true,
            (Self::Json(a), Self::Json(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::Native(a), Self::Native(b)) => std::sync::Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}
