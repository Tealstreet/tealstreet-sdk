//! Futures order-book reads preserve failed categories independently.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_exchange::{error_reporting::ExchangeErrorInput, exchange::BaseExchange};
use safe_cex_num::js_truthy;
use safe_cex_store::virtual_clock::VirtualClock;
use safe_cex_transport::provider::{RestAdapter, RestRequest};
use safe_cex_types::{Order, OrderType};
use serde_json::{Value, json};

// ts: KucoinExchange.fetchNormalOrders
pub async fn fetch_normal_orders(
    base: &BaseExchange,
    xhr: &dyn RestAdapter,
    clock: &VirtualClock,
) -> Result<Option<Vec<Order>>, ExchangeError> {
    read_book(base, xhr, clock, false).await
}
// ts: KucoinExchange.fetchAlgoOrders
pub async fn fetch_algo_orders(
    base: &BaseExchange,
    xhr: &dyn RestAdapter,
    clock: &VirtualClock,
) -> Result<Option<Vec<Order>>, ExchangeError> {
    read_book(base, xhr, clock, true).await
}
// ts: KucoinExchange.fetchOrders
pub async fn fetch_orders(
    base: &BaseExchange,
    xhr: &dyn RestAdapter,
    clock: &VirtualClock,
) -> Result<Vec<Order>, ExchangeError> {
    // Source reads both books in order, then observes the CURRENT store. A
    // private stream may update the store while either request is pending.
    let normal = fetch_normal_orders(base, xhr, clock).await?;
    let algo = fetch_algo_orders(base, xhr, clock).await?;
    let is_stop = |order: &&Order| {
        matches!(
            order.r#type,
            OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
        )
    };
    let normal = normal.unwrap_or_else(|| {
        base.store
            .orders()
            .iter()
            .filter(|o| !is_stop(o))
            .cloned()
            .collect()
    });
    let algo = algo.unwrap_or_else(|| {
        base.store
            .orders()
            .iter()
            .filter(is_stop)
            .cloned()
            .collect()
    });
    Ok(crate::sizing::reconcile_fetched_order_remainders(
        normal.into_iter().chain(algo).collect(),
        base.store.as_ref(),
    ))
}
fn emit(
    base: &BaseExchange,
    input_message: Option<String>,
    inside_try: bool,
) -> Result<(), ExchangeError> {
    let emitted = base.emit_and_log_error(
        input_message.map_or(ExchangeErrorInput::Missing, ExchangeErrorInput::Message),
        None,
        None,
    );
    match emitted {
        Err(error) if inside_try => {
            let caught = message(
                error
                    .response
                    .as_ref()
                    .and_then(|r| r.get("data"))
                    .and_then(|d| d.get("msg")),
            )
            .unwrap_or_else(Some)
            .or(Some(error.message));
            base.emit_and_log_error(
                caught.map_or(ExchangeErrorInput::Missing, ExchangeErrorInput::Message),
                None,
                None,
            )
        }
        result => result,
    }
}

fn message(value: Option<&Value>) -> Result<Option<String>, String> {
    match value.filter(|v| js_truthy(Some(v))) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => {
            Err("KuCoin order error message requires the shared string representation".into())
        }
    }
}
async fn read_book(
    base: &BaseExchange,
    xhr: &dyn RestAdapter,
    clock: &VirtualClock,
    algo: bool,
) -> Result<Option<Vec<Order>>, ExchangeError> {
    let response = xhr
        .execute(RestRequest {
            method: "GET".into(),
            url: if algo {
                "/api/v1/stopOrders"
            } else {
                "/api/v1/orders"
            }
            .into(),
            params: Some(json!({"status":"active"})),
            ..Default::default()
        })
        .await;
    let mut inside_try = false;
    let result = match response {
        Err(error) => {
            let body = error
                .response
                .as_ref()
                .map(|r| r.body.json_value())
                .transpose();
            let message = match body {
                Ok(body) => message(body.as_ref().and_then(|b| b.get("msg")))
                    .unwrap_or_else(Some)
                    .or(Some(error.message)),
                Err(error) => Some(format!(
                    "KuCoin order response representation refusal at JSON/UTF-8 boundary: {error}"
                )),
            };
            Err(message)
        }
        Ok(response) => match response.body.json_value() {
            Err(error) => Err(Some(format!(
                "KuCoin order response representation refusal at JSON/UTF-8 boundary: {error}"
            ))),
            Ok(data) => {
                inside_try =
                    !data.is_null() && data.get("code").and_then(Value::as_str) != Some("200000");
                decode_book(base, clock, &data, algo).map_err(Some)
            }
        },
    };
    match result {
        Ok(orders) => Ok(Some(orders)),
        Err(message) => {
            emit(base, message, inside_try)?;
            Ok(None)
        }
    }
}
fn decode_book(
    base: &BaseExchange,
    clock: &VirtualClock,
    data: &Value,
    algo: bool,
) -> Result<Vec<Order>, String> {
    if data.is_null() {
        return Err("Cannot read properties of null (reading 'code')".into());
    }
    if data.get("code").and_then(Value::as_str) != Some("200000") {
        return Err(message(data.get("msg"))?.unwrap_or_else(|| {
            if algo {
                "Failed to fetch algo orders"
            } else {
                "Failed to fetch orders"
            }
            .into()
        }));
    }
    let rows = match data.get("data") {
        None => return Err("Cannot read properties of undefined (reading 'items')".into()),
        Some(Value::Null) => return Err("Cannot read properties of null (reading 'items')".into()),
        Some(data) => data.get("items").and_then(Value::as_array),
    };
    let Some(rows) = rows else {
        return Ok(Vec::new());
    };
    let mut orders = Vec::new();
    for row in rows {
        let row = row
            .as_object()
            .ok_or_else(|| "KuCoin order row requires an object representation".to_owned())?;
        let now_ms = if ["updateTime", "ts", "orderTime", "createdAt"]
            .iter()
            .any(|key| js_truthy(row.get(*key)))
        {
            0.0 // The mapper ignores this argument when source time is present.
        } else {
            clock.get_current_time_ms()
        };
        orders.extend(crate::mappers::map_order(row, base.store.as_ref(), now_ms)?);
    }
    Ok(orders)
}
