use safe_cex_num::{JsCoercionError, js_string, js_truthy};
use safe_cex_types::OrderRejectReason;
use serde_json::Value;

// ts: BinancePrivateWebsocket.getOrderRejectReason
pub fn get_order_reject_reason(
    data: &serde_json::Map<String, Value>,
) -> Result<Option<OrderRejectReason>, JsCoercionError> {
    let empty = Value::String(String::new());
    let code = js_string(
        data.get("r")
            .filter(|v| js_truthy(Some(v)))
            .or(Some(&empty)),
    )?
    .to_uppercase();
    Ok(
        if ["POST_ONLY", "GTX", "MAKER"]
            .iter()
            .any(|text| code.contains(text))
        {
            Some(OrderRejectReason::PostOnly)
        } else {
            None
        },
    )
}
