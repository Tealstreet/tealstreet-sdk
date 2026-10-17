//! Sequential OKX REST operations; adapters retain the original rejection response.
use safe_cex_exchange::{
    client_order_id::{ClientOrderLookup, order_carries_client_order_id},
    runtime::ExchangeError,
};
use safe_cex_num::{
    JsJsonValue, TableHit, js_json_stringify, js_string, js_table_lookup, js_truthy,
};
use safe_cex_transport::{RestAdapter, RestFailure, RestRequest, RestResponse};
use safe_cex_types::{Market, Order, OrderStatus};
use serde_json::{Value, json};

fn endpoint(key: &str) -> String {
    crate::types::ENDPOINTS
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, path)| (*path).to_owned())
        .unwrap_or_default()
}

fn response_data(response: &RestResponse) -> Value {
    response.body.json_value().unwrap_or(Value::Null)
}

fn coercion(error: safe_cex_num::JsCoercionError) -> ExchangeError {
    let mut result = ExchangeError::new(error.message);
    result.name = error.name.into();
    result
}

fn error_message(value: Option<&Value>) -> Result<String, ExchangeError> {
    // new Error(undefined) has an empty message, unlike String(undefined).
    value.map_or_else(
        || Ok(String::new()),
        |value| js_string(Some(value)).map_err(coercion),
    )
}

fn placement_message(error: &RestFailure) -> Result<String, ExchangeError> {
    let body = error.response.as_deref().map(response_data);
    let first = body
        .as_ref()
        .and_then(|body| body.get("data"))
        .and_then(|data| data.get(0));
    if let Some(message) = first
        .and_then(|row| row.get("sMsg"))
        .filter(|value| js_truthy(Some(value)))
    {
        let mut message = js_string(Some(message)).map_err(coercion)?;
        if let Some(code) = first
            .and_then(|row| row.get("sCode"))
            .filter(|value| js_truthy(Some(value)))
        {
            message.push_str(&format!(
                " (code {})",
                js_string(Some(code)).map_err(coercion)?
            ));
        }
        return Ok(message);
    }
    match body
        .as_ref()
        .and_then(|body| body.get("msg"))
        .filter(|value| js_truthy(Some(value)))
    {
        Some(message) => error_message(Some(message)),
        None => Ok(error.message.clone()),
    }
}

async fn place_native(
    adapter: &dyn RestAdapter,
    payloads: &[JsJsonValue],
    algo: bool,
    emit: &dyn Fn(&str) -> Result<(), ExchangeError>,
) -> Result<Vec<String>, ExchangeError> {
    let mut ids = Vec::new();
    let mut refused_id = false;
    for batch in payloads.chunks(if algo { 1 } else { 20 }) {
        let payload = if algo {
            batch[0].clone()
        } else {
            JsJsonValue::Array(batch.to_vec())
        };
        let body = js_json_stringify(&payload)
            .map_err(coercion)?
            .map(String::into_bytes);
        let request = RestRequest {
            method: "post".into(),
            url: endpoint(if algo {
                "PLACE_ALGO_ORDER"
            } else {
                "PLACE_ORDERS"
            }),
            body: body.map(safe_cex_transport::RestBody::Encoded),
            ..Default::default()
        };
        let result = match adapter
            .execute(request.clone())
            .await
            .and_then(|response| crate::api::check_response(response, request))
        {
            Err(error) => Err(ExchangeError::new(placement_message(&error)?)),
            Ok(response) => {
                let body = response_data(&response);
                let rows = body
                    .get("data")
                    .and_then(Value::as_array)
                    .ok_or_else(|| ExchangeError::new("OKX order response data must be an array"));
                rows.and_then(|rows| {
                    let mut batch_ids = Vec::new();
                    for row in rows {
                        if row.is_null() {
                            return Err(ExchangeError::new(format!(
                                "Cannot read properties of null (reading '{}')",
                                if algo { "algoId" } else { "ordId" }
                            )));
                        }
                        let id = row.get(if algo { "algoId" } else { "ordId" });
                        if js_truthy(id) {
                            // ts-bug: placeOrderBatch/placeAlgoOrderBatch can leak non-string IDs from any.
                            // The chosen string-valued core contract rejects them with a diagnostic.
                            if let Some(id) = id.and_then(Value::as_str) {
                                batch_ids.push(id.to_owned());
                            } else {
                                // Retain the refusal until every source row and request finishes.
                                // A later source error must win over this typed-output choice.
                                refused_id = true;
                            }
                        } else {
                            return Err(ExchangeError::new(error_message(row.get("sMsg"))?));
                        }
                    }
                    Ok(batch_ids)
                })
            }
        };
        match result {
            Ok(batch_ids) => ids.extend(batch_ids),
            Err(error) => {
                emit(&error.message)?;
                return Err(ExchangeError::new(error.message));
            }
        }
    }
    if refused_id {
        let message = "OKX order id must be a string";
        emit(message)?;
        return Err(ExchangeError::new(message));
    }
    Ok(ids)
}

pub async fn place_order_batch(
    adapter: &dyn RestAdapter,
    payloads: &[JsJsonValue],
    emit: &dyn Fn(&str) -> Result<(), ExchangeError>,
) -> Result<Vec<String>, ExchangeError> {
    // Drop stops this caller; no shared latch, task or socket is owned here.
    place_native(adapter, payloads, false, emit).await
}

pub async fn place_algo_order_batch(
    adapter: &dyn RestAdapter,
    payloads: &[JsJsonValue],
    emit: &dyn Fn(&str) -> Result<(), ExchangeError>,
) -> Result<Vec<String>, ExchangeError> {
    place_native(adapter, payloads, true, emit).await
}

fn first_lookup_row(
    data: Option<&Value>,
) -> Result<Option<std::borrow::Cow<'_, Value>>, safe_cex_num::JsCoercionError> {
    use std::borrow::Cow;
    Ok(match data {
        Some(Value::Array(rows)) => rows.first().map(Cow::Borrowed),
        Some(Value::Object(rows)) => rows.get("0").map(Cow::Borrowed),
        Some(Value::String(text)) => text.encode_utf16().next().map(|unit| {
            char::from_u32(u32::from(unit)).map(|value| Cow::Owned(Value::String(value.to_string())))
                .ok_or(safe_cex_num::JsCoercionError {
                    name: "RepresentationError",
                    message: "OKX lookup data indexing materializes a lone UTF-16 unit outside the shared String surface",
                })
        }).transpose()?,
        _ => None,
    })
}

// JSON-admitted own fields for the source object spread. The state override
// travels separately when its inherited native referent is not a JSON value.
fn spread_lookup_row(row: &Value) -> Result<Value, safe_cex_num::JsCoercionError> {
    let entries: safe_cex_types::object_map::JsObjectMap<Value> = match row {
        Value::Object(object) => object.iter().map(|(key, value)| (key.clone(), value.clone())).collect(),
        Value::Array(values) => values.iter().enumerate().map(|(i, value)| (i.to_string(), value.clone())).collect(),
        Value::String(value) => value.encode_utf16().enumerate().map(|(i, unit)| {
            char::from_u32(u32::from(unit)).map(|value| (i.to_string(), Value::String(value.to_string())))
                .ok_or(safe_cex_num::JsCoercionError {
                    name: "RepresentationError",
                    message: "OKX lookup string spread materializes a lone UTF-16 unit outside the shared String surface",
                })
        }).collect::<Result<_, _>>()?,
        _ => Default::default(),
    };
    Ok(Value::Object(entries.into_iter().collect()))
}

/// The mapper must apply the separate native state override on every state read;
/// an inherited referent cannot be substituted into the JSON row itself.
pub async fn find_order_by_client_order_id(
    adapter: &dyn RestAdapter,
    orders: &[Order],
    market: Option<&Market>,
    symbol: &str,
    client_order_id: &str,
    // The override is a source value: inherited hits must reach the mapper as
    // TableHit identity, never a made-up "live" JSON string.
    map_orders: &dyn Fn(&Value, TableHit<&'static str>) -> Result<Vec<Order>, RestFailure>,
) -> Result<ClientOrderLookup, RestFailure> {
    if let Some(order) = orders.iter().find(|order| {
        order.symbol == symbol && order_carries_client_order_id(order, client_order_id)
    }) {
        return Ok(ClientOrderLookup::Found(Box::new(order.clone())));
    }
    let Some(market) = market else {
        return Ok(ClientOrderLookup::NotFound { definitive: false });
    };
    let request = RestRequest {
        method: "get".into(),
        url: endpoint("ORDER_DETAIL"),
        params: Some(json!({"instId":market.id,"clOrdId":client_order_id})),
        ..Default::default()
    };
    let result: Result<ClientOrderLookup, RestFailure> = async {
        let response = adapter
            .execute(request.clone())
            .await
            .and_then(|response| crate::api::check_response(response, request.clone()))?;
        let body = response_data(&response);
        if body.is_null() {
            return Err(RestFailure {
                code: None,
                message: "Cannot read properties of null (reading 'data')".into(),
                transport_error: None,
                response: None,
                request: Box::new(request.clone()),
                context: Some(Box::new(json!({"name":"TypeError"}))),
            });
        }
        let selected = first_lookup_row(body.get("data")).map_err(|e| RestFailure {
            code: None,
            message: e.message.into(),
            transport_error: None,
            response: None,
            request: Box::new(request.clone()),
            context: Some(Box::new(json!({"name":e.name}))),
        })?;
        let row = selected.as_deref().filter(|row| js_truthy(Some(row)));
        let Some(row) = row else {
            return Ok(ClientOrderLookup::NotFound { definitive: false });
        };
        let mut mapped_row = spread_lookup_row(row).map_err(|e| RestFailure {
            code: None,
            message: e.message.into(),
            transport_error: None,
            response: None,
            request: Box::new(request.clone()),
            context: Some(Box::new(json!({"name":e.name}))),
        })?;
        let key = js_string(row.get("state")).map_err(|e| RestFailure {
            code: None,
            message: e.message.into(),
            transport_error: None,
            response: None,
            request: Box::new(request.clone()),
            context: Some(Box::new(json!({"name":e.name}))),
        })?;
        let live_table = [("live", "live"), ("partially_filled", "partially_filled")];
        let live_state = match js_table_lookup(&live_table, &key) {
            TableHit::Mapped(state) => TableHit::Mapped(*state),
            TableHit::Inherited(member) => TableHit::Inherited(member),
            TableHit::Missing => TableHit::Mapped("live"),
        };
        if let (Some(object), TableHit::Mapped(state)) = (mapped_row.as_object_mut(), live_state) {
            object.insert("state".into(), state.into());
        }
        // Do not refuse before the supplied mapper: its empty result and its
        // original failure both take precedence over a typed status refusal.
        let Some(mut order) = map_orders(&mapped_row, live_state)?.into_iter().next() else {
            return Ok(ClientOrderLookup::NotFound { definitive: false });
        };
        let status_table = [
            ("filled", OrderStatus::Closed),
            ("canceled", OrderStatus::Canceled),
            ("mmp_canceled", OrderStatus::Canceled),
        ];
        match js_table_lookup(&status_table, &key) {
            TableHit::Mapped(status) => order.status = (*status).into(),
            TableHit::Inherited(_) => {
                return Err(RestFailure {
                    code: None,
                    message:
                        "OKX inherited lookup status cannot cross the closed typed output boundary"
                            .into(),
                    transport_error: None,
                    response: None,
                    request: Box::new(request.clone()),
                    context: Some(Box::new(json!({"name":"RepresentationError"}))),
                });
            }
            TableHit::Missing => {}
        }
        Ok(ClientOrderLookup::Found(Box::new(order)))
    }
    .await;
    match result {
        Err(error)
            if error.response.as_deref().is_some_and(|response| {
                response_data(response).get("code").and_then(Value::as_str) == Some("51603")
            }) =>
        {
            Ok(ClientOrderLookup::NotFound { definitive: true })
        }
        result => result,
    }
}
