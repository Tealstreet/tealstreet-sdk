//! REST order-read orchestration; mapOrder and transport remain separate rows.
use crate::constants::ENDPOINTS;
use safe_cex_num::{JsCoercionError, JsJsonValue, js_json_stringify, js_string, js_truthy};
use serde_json::Value;
use std::future::Future;

// ts: ParadexExchange.fetchOrders
pub struct OrderReadEffects<L, S, D, C> {
    pub log: L,
    pub stringify_error: S,
    pub describe_error: D,
    pub coercion_error: C,
}

// ts: ParadexExchange.fetchOrders
pub async fn fetch_orders<E, T, I, F>(
    sdk_init: Option<I>,
    has_jwt_manager: impl FnOnce() -> bool,
    get: impl FnOnce(&'static str) -> Result<F, E>,
    mut map_order: impl FnMut(&Value) -> Result<T, E>,
    effects: &mut OrderReadEffects<
        impl FnMut(String, &'static str) -> Result<(), E>,
        impl FnMut(&E) -> Result<String, E>,
        impl FnMut(&E) -> Result<Option<String>, E>,
        impl FnMut(JsCoercionError) -> E,
    >,
) -> Result<Vec<T>, E>
where
    I: Future<Output = Result<(), E>>,
    F: Future<Output = Result<Option<Value>, E>>,
{
    let result = async {
        if let Some(init) = sdk_init {
            init.await?;
        }
        if !has_jwt_manager() {
            (effects.log)(
                "Cannot fetch orders: JWT manager not initialized".into(),
                "warning",
            )?;
            return Ok(Vec::new());
        }
        (effects.log)("Fetching orders from API...".into(), "debug")?;
        let endpoint = ENDPOINTS.iter().find(|(k, _)| *k == "ORDERS").unwrap().1;
        let data = get(endpoint)?.await?;
        let rows = data.as_ref().and_then(|v| v.get("results"));
        if !js_truthy(data.as_ref()) || !js_truthy(rows) {
            (effects.log)("Orders API returned invalid data structure".into(), "error")?;
            let prepared = data
                .map(JsJsonValue::from)
                .unwrap_or(JsJsonValue::Undefined);
            let text = js_json_stringify(&prepared)
                .map_err(&mut effects.coercion_error)?
                .unwrap_or_else(|| "undefined".into());
            (effects.log)(format!("Data received: {text}"), "debug")?;
            return Ok(Vec::new());
        }
        let rows = rows.unwrap();
        let length = match rows {
            Value::Array(v) => v.len().to_string(),
            Value::String(v) => v.encode_utf16().count().to_string(),
            _ => js_string(rows.get("length")).map_err(&mut effects.coercion_error)?,
        };
        (effects.log)(
            format!("Orders API response: {length} total orders"),
            "debug",
        )?;
        let rows = rows.as_array().ok_or_else(|| {
            (effects.coercion_error)(JsCoercionError {
                name: "TypeError",
                message: "data.results.filter is not a function",
            })
        })?;
        let mut open_orders = Vec::new();
        for row in rows {
            if row.is_null() {
                return Err((effects.coercion_error)(JsCoercionError {
                    name: "TypeError",
                    message: "Cannot read properties of null (reading 'status')",
                }));
            }
            if matches!(
                row.get("status").and_then(Value::as_str),
                Some("NEW" | "OPEN" | "PENDING" | "UNTRIGGERED")
            ) {
                open_orders.push(row);
            }
        }
        (effects.log)(format!("Open orders: {}", open_orders.len()), "debug")?;
        open_orders.into_iter().map(&mut map_order).collect()
    }
    .await;
    match result {
        Ok(orders) => Ok(orders),
        Err(error) => {
            let text = (effects.stringify_error)(&error)?;
            (effects.log)(format!("Error fetching orders: {text}"), "error")?;
            let text = (effects.describe_error)(&error)?.unwrap_or_else(|| "undefined".into());
            (effects.log)(format!("Orders error details: {text}"), "debug")?;
            Err(error)
        }
    }
}
