//! REST order-read orchestration; mapOrder and transport remain separate rows.
use crate::constants::ENDPOINTS;
use safe_cex_num::{JsCoercionError, js_retained_json_stringify, js_retained_string};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
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
    mut map_order: impl FnMut(&Input) -> Result<T, E>,
    effects: &mut OrderReadEffects<
        impl FnMut(String, &'static str) -> Result<(), E>,
        impl FnMut(&E) -> Result<String, E>,
        impl FnMut(&E) -> Result<Option<String>, E>,
        impl FnMut(JsCoercionError) -> E,
    >,
) -> Result<Vec<T>, E>
where
    I: Future<Output = Result<(), E>>,
    F: Future<Output = Result<Option<Input>, E>>,
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
        if !data.as_ref().is_some_and(Input::is_truthy) || !rows.is_some_and(Input::is_truthy) {
            (effects.log)("Orders API returned invalid data structure".into(), "error")?;
            let text = js_retained_json_stringify(data.as_ref().unwrap_or(&Input::Undefined))
                .map_err(&mut effects.coercion_error)?
                .unwrap_or_else(|| "undefined".into());
            (effects.log)(format!("Data received: {text}"), "debug")?;
            return Ok(Vec::new());
        }
        let rows = rows.unwrap();
        let length = match rows {
            Input::Array(v) => v.len().to_string(),
            Input::Text(v) => v.len().to_string(),
            _ => js_retained_string(rows.get("length")).map_err(&mut effects.coercion_error)?,
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
            if row.is_nullish() {
                return Err((effects.coercion_error)(JsCoercionError {
                    name: "TypeError",
                    message: if row.is_null() {
                        "Cannot read properties of null (reading 'status')"
                    } else {
                        "Cannot read properties of undefined (reading 'status')"
                    },
                }));
            }
            if row.get("status").is_some_and(|value| {
                ["NEW", "OPEN", "PENDING", "UNTRIGGERED"]
                    .iter()
                    .any(|status| value.text_equals(status))
            }) {
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
