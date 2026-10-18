//! Recursive order-page phases over the supplied REST boundary.
use safe_cex_exchange::{
    error_reporting::ExchangeErrorInput, exchange::BaseExchange, js_runtime::value::from_json,
    runtime::ExchangeError,
};
use safe_cex_num::{js_number, js_truthy};
use safe_cex_transport::{RestAdapter, RestRequest};
use safe_cex_types::retained_json::RetainedJsonInput;
use serde_json::Value;

fn refusal(message: &str) -> Box<dyn std::error::Error> {
    Box::new(ExchangeError::new(message))
}

// ts: WOOXExchange.fetchOrdersRecursively
pub async fn fetch_orders_recursively(
    base: &BaseExchange,
    adapter: &dyn RestAdapter,
    endpoint: &str,
    initial_page: Option<f64>,
) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let mut page = initial_page.unwrap_or(1.0);
    let mut result = Vec::new();
    loop {
        let params = RetainedJsonInput::Object(vec![
            (
                "status".encode_utf16().collect(),
                RetainedJsonInput::Text("INCOMPLETE".encode_utf16().collect()),
            ),
            (
                "page".encode_utf16().collect(),
                RetainedJsonInput::Float(page),
            ),
        ])
        .into_value()?;
        let response = adapter
            .execute(RestRequest {
                method: "GET".into(),
                url: endpoint.into(),
                params: Some(params),
                ..RestRequest::default()
            })
            .await?;
        if base.is_disposed() {
            return Ok(result);
        }
        let data = match response.body.into_data() {
            RetainedJsonInput::Text(text) => {
                RetainedJsonInput::parse_json(&text).unwrap_or(RetainedJsonInput::Null)
            }
            native => native,
        };
        let data = data.into_value()?;
        let object = data
            .as_object()
            .ok_or_else(|| refusal("order page requires an object response"))?;
        if !js_truthy(object.get("success")) {
            let message = object
                .get("message")
                .filter(|value| js_truthy(Some(value)))
                .or_else(|| {
                    object
                        .get("meta")
                        .and_then(|meta| meta.get("message"))
                        .filter(|value| js_truthy(Some(value)))
                });
            let message = message
                .cloned()
                .unwrap_or_else(|| Value::String("Failed to fetch orders".into()));
            base.emit_and_log_error(ExchangeErrorInput::Raw(from_json(&message)), None, None)?;
            return Ok(result);
        }
        let data = object
            .get("data")
            .filter(|value| js_truthy(Some(value)))
            .unwrap_or(&data);
        let rows = data.get("rows");
        if rows.and_then(Value::as_array).is_some_and(Vec::is_empty) {
            return Ok(result);
        }
        let meta = data
            .get("meta")
            .and_then(Value::as_object)
            .ok_or_else(|| refusal("order page requires object metadata"))?;
        let threshold = js_number(meta.get("records_per_page"))? * page;
        let total = js_number(meta.get("total"))?;
        let more = total > threshold;
        let rows = rows
            .and_then(Value::as_array)
            .ok_or_else(|| refusal("order page requires array rows"))?;
        result.extend(rows.iter().cloned());
        if !more {
            return Ok(result);
        }
        page += 1.0;
    }
}
