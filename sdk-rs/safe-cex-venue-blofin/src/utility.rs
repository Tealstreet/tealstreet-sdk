//! Stateless helpers from the maintained BlofinExchange source.
use safe_cex_num::{js_number, js_truthy, numeric_ingress::JsNumberOutput};
use safe_cex_types::{
    OrderStopTriggerType,
    js_coercion::{JsCoercionError, js_string},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UtilityInput {
    #[serde(deserialize_with = "safe_cex_num::numeric_ingress::js_deserialize_passthrough")]
    value: Value,
}
impl JsNumberOutput for UtilityInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct {} passthrough { value });
    }
}
impl UtilityInput {
    pub fn json(&self) -> &Value {
        &self.value
    }
    pub(crate) fn from_retained(value: Value) -> Self {
        Self { value }
    }
}
fn scalar(value: Option<&UtilityInput>) -> Option<&Value> {
    value.map(|v| &v.value)
}
fn string_or_empty(value: Option<&Value>) -> Result<String, JsCoercionError> {
    if js_truthy(value) {
        js_string(value)
    } else {
        Ok(String::new())
    }
}
pub fn get_normalized_inst_type(payload: &UtilityInput) -> Result<String, JsCoercionError> {
    if payload.value.is_null() {
        return Err(JsCoercionError {
            name: "TypeError",
            message: "Cannot read properties of null (reading 'instType')",
        });
    }
    Ok(string_or_empty(payload.value.get("instType"))?.to_uppercase())
}
pub fn is_blofin_linear_swap_market(payload: &UtilityInput) -> Result<bool, JsCoercionError> {
    Ok(get_normalized_inst_type(payload)? == "SWAP"
        && string_or_empty(payload.value.get("contractType"))?.to_lowercase() == "linear")
}
pub fn is_known_non_swap_instrument(payload: &UtilityInput) -> Result<bool, JsCoercionError> {
    let inst_type = get_normalized_inst_type(payload)?;
    Ok(!inst_type.is_empty() && inst_type != "SWAP")
}
pub fn parse_positive_price(value: Option<&UtilityInput>) -> Result<Option<f64>, JsCoercionError> {
    let parsed = js_number(scalar(value))?;
    Ok((parsed.is_finite() && parsed > 0.0).then_some(parsed))
}
pub fn map_blofin_trigger_type(value: Option<&UtilityInput>) -> Option<OrderStopTriggerType> {
    match scalar(value).and_then(Value::as_str) {
        Some("last") => Some(OrderStopTriggerType::LastPrice),
        Some("mark") => Some(OrderStopTriggerType::MarkPrice),
        Some("index") => Some(OrderStopTriggerType::IndexPrice),
        _ => None,
    }
}
pub fn to_blofin_trigger_type(value: Option<&UtilityInput>) -> &'static str {
    match scalar(value).and_then(Value::as_str) {
        Some("mark_price") => "mark",
        Some("index_price") => "index",
        _ => "last",
    }
}
pub fn is_already_gone_cancel_row(row: Option<&UtilityInput>) -> Result<bool, JsCoercionError> {
    let row = scalar(row);
    if row.and_then(|r| r.get("code")).and_then(Value::as_str) == Some("1000") {
        return Ok(true);
    }
    let message = row.and_then(|r| r.get("msg")).filter(|v| !v.is_null());
    let message = match message {
        Some(message) => js_string(Some(message))?,
        None => String::new(),
    };
    // The source uses /i without /u. Only ASCII folds can match its ASCII phrase.
    Ok(message
        .to_ascii_lowercase()
        .contains("filled, triggered, canceled or does not exist"))
}
pub fn is_algo_order(order_type: Option<&UtilityInput>) -> bool {
    matches!(
        scalar(order_type).and_then(Value::as_str),
        Some("stop_market" | "take_profit_market" | "trailing_stop_market")
    )
}
