//! Stateless helpers from the maintained BlofinExchange source.
use safe_cex_num::{
    js_retained_number as js_number, js_retained_string as js_string,
    numeric_ingress::JsNumberOutput,
};
use safe_cex_types::{
    OrderStopTriggerType, js_coercion::JsCoercionError, retained_json::RetainedJsonInput as Input,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UtilityInput {
    value: Input,
}
impl JsNumberOutput for UtilityInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct {} passthrough { value });
    }
}
impl UtilityInput {
    pub fn json(&self) -> &Input {
        &self.value
    }
    pub(crate) fn from_retained(value: Input) -> Self {
        Self { value }
    }
}
fn scalar(value: Option<&UtilityInput>) -> Option<&Input> {
    value.map(|v| &v.value)
}
fn string_or_empty(value: Option<&Input>) -> Result<String, JsCoercionError> {
    if value.is_some_and(Input::is_truthy) {
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
    let value = scalar(value)?;
    if value.text_equals("last") {
        Some(OrderStopTriggerType::LastPrice)
    } else if value.text_equals("mark") {
        Some(OrderStopTriggerType::MarkPrice)
    } else if value.text_equals("index") {
        Some(OrderStopTriggerType::IndexPrice)
    } else {
        None
    }
}
pub fn to_blofin_trigger_type(value: Option<&UtilityInput>) -> &'static str {
    if scalar(value).is_some_and(|v| v.text_equals("mark_price")) {
        "mark"
    } else if scalar(value).is_some_and(|v| v.text_equals("index_price")) {
        "index"
    } else {
        "last"
    }
}
pub fn is_already_gone_cancel_row(row: Option<&UtilityInput>) -> Result<bool, JsCoercionError> {
    let row = scalar(row);
    if row
        .and_then(|r| r.get("code"))
        .is_some_and(|v| v.text_equals("1000"))
    {
        return Ok(true);
    }
    let message = row.and_then(|r| r.get("msg")).filter(|v| !v.is_nullish());
    let message = match message {
        Some(message) => safe_cex_types::js_coercion::js_retained_string_utf16(message)?,
        None => Vec::new(),
    };
    // The source uses /i without /u. Only ASCII folds can match its ASCII phrase.
    let folded: Vec<u16> = message
        .into_iter()
        .map(|u| if (65..=90).contains(&u) { u + 32 } else { u })
        .collect();
    let needle: Vec<u16> = "filled, triggered, canceled or does not exist"
        .encode_utf16()
        .collect();
    Ok(folded.windows(needle.len()).any(|window| window == needle))
}
pub fn is_algo_order(order_type: Option<&UtilityInput>) -> bool {
    scalar(order_type).is_some_and(|v| {
        ["stop_market", "take_profit_market", "trailing_stop_market"]
            .iter()
            .any(|s| v.text_equals(s))
    })
}
