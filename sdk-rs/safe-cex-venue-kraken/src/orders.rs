//! Futures order read at the existing parsed-data, clock and shared Store boundaries.
use crate::{helpers, types, wire};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{
    js_date_parse, js_string, js_time_clip, js_truthy,
    numeric_ingress::{JsNumberInput, js_value_as_f64},
    parse_float,
};
use safe_cex_store::Store;
use safe_cex_types::{Order, OrderSide, OrderStatus, OrderTimeInForce, OrderType};
use serde_json::Value;
use std::future::Future;

fn type_error(message: impl Into<String>) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = "TypeError".into();
    error
}

fn string(value: Option<&Value>) -> Result<String, ExchangeError> {
    js_string(value).map_err(|error| type_error(error.message))
}

fn nullish<'a>(row: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter()
        .find_map(|key| row.get(*key).filter(|value| !value.is_null()))
}

fn truthy<'a>(row: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter()
        .find_map(|key| row.get(*key).filter(|value| js_truthy(Some(value))))
}

fn number(value: Option<&Value>) -> Result<f64, ExchangeError> {
    value.map_or(Ok(0.0), |value| {
        string(Some(value)).map(|text| parse_float(&text))
    })
}

pub async fn fetch_orders<F: Future<Output = Result<Option<JsNumberInput>, ExchangeError>>>(
    store: &dyn Store,
    read: impl FnOnce(&'static str) -> Result<F, ExchangeError>,
    mut now: impl FnMut() -> Result<f64, ExchangeError>,
    mut local_to_utc: impl FnMut(f64) -> f64,
    trace: impl FnOnce(&ExchangeError) -> Result<(), ExchangeError>,
) -> Result<Vec<Order>, ExchangeError> {
    let outcome = async {
        let endpoint = types::ENDPOINTS
            .iter()
            .find(|(key, _)| *key == "OPEN_ORDERS")
            .expect("open orders endpoint")
            .1;
        let data = read(endpoint)?
            .await?
            .map(wire::decode)
            .transpose()
            .map_err(|error| ExchangeError::new(error.to_string()))?;
        let empty = Vec::new();
        let rows = match data
            .as_ref()
            .and_then(|data| data.get("openOrders"))
            .filter(|value| js_truthy(Some(value)))
        {
            None => &empty,
            Some(Value::Array(rows)) => rows,
            _ => return Err(type_error("list.map is not a function")),
        };
        let mut orders = Vec::with_capacity(rows.len());
        for row in rows {
            if row.is_null() {
                return Err(type_error(
                    "Cannot read properties of null (reading 'orderType')",
                ));
            }
            let typ = nullish(row, &["orderType", "type"]);
            let typ = if let Some(value) = typ {
                string(Some(value))?
            } else {
                "lmt".into()
            }
            .to_lowercase();
            let time_in_force = match typ.as_str() {
                "post" => OrderTimeInForce::PostOnly,
                "ioc" => OrderTimeInForce::ImmediateOrCancel,
                "fok" => OrderTimeInForce::FillOrKill,
                _ => OrderTimeInForce::GoodTillCancel,
            };
            let order_type = truthy(row, &["orderType", "type"]);
            let order_type = if let Some(value) = order_type {
                string(Some(value))?
            } else {
                "lmt".into()
            }
            .to_lowercase();
            let order_type = match order_type.as_str() {
                "stop" | "stp" => OrderType::StopLoss,
                "take_profit" => OrderType::TakeProfit,
                "trailing_stop" => OrderType::TrailingStopLoss,
                text if text.contains("lmt") || text == "limit" => OrderType::Limit,
                _ => OrderType::Market,
            };
            let id = truthy(row, &["order_id", "orderId"])
                .map(string_value)
                .transpose()?
                .unwrap_or_default();
            let symbol = truthy(row, &["instrument", "symbol"])
                .map(string_value)
                .transpose()?
                .unwrap_or_default();
            let direction = row.get("direction");
            let side = if let Some(direction) = direction {
                if js_value_as_f64(direction) == Some(0.0) || direction.as_str() == Some("buy") {
                    OrderSide::Buy
                } else {
                    OrderSide::Sell
                }
            } else {
                let side = truthy(row, &["side"]);
                if side
                    .map(string_value)
                    .transpose()?
                    .unwrap_or_else(|| "buy".into())
                    .to_lowercase()
                    == "buy"
                {
                    OrderSide::Buy
                } else {
                    OrderSide::Sell
                }
            };
            let triggered = matches!(
                order_type,
                OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
            );
            let price = number(nullish(
                row,
                if triggered {
                    &["stopPrice", "stop_price"]
                } else {
                    &["limitPrice", "limit_price", "price"]
                },
            ))?;
            let unfilled = number(nullish(row, &["unfilledSize", "qty", "quantity", "size"]))?;
            let filled = number(nullish(row, &["filledSize", "filled"]))?;
            let last_update =
                if let Some(value) = truthy(row, &["lastUpdateTime", "lastUpdateTimestamp"]) {
                    js_date_parse(&string(Some(value))?, &mut local_to_utc)
                } else {
                    let clipped = js_time_clip(now()?);
                    if clipped.is_nan() {
                        let mut error = ExchangeError::new("Invalid time value");
                        error.name = "RangeError".into();
                        return Err(error);
                    }
                    clipped
                };
            let trigger = if triggered {
                helpers::parse_trigger_signal(row.get("triggerSignal"))
                    .map_err(|error| type_error(error.message))?
            } else {
                None
            };
            orders.push(Order {
                client_order_id: (None).into(),
                id,
                parent_id: (None).into(),
                status: safe_cex_types::ModelField::Value(OrderStatus::Open),
                symbol,
                r#type: (order_type).into(),
                side: side.into(),
                price: price.into(),
                amount: unfilled + filled,
                filled,
                remaining: number(nullish(row, &["unfilledSize"]))?,
                reduce_only: (js_truthy(row.get("reduceOnly"))
                    || js_truthy(row.get("reduce_only")))
                .into(),
                close: (None).into(),
                info: (Some(row.clone().into())).into(),
                time_in_force: (Some(time_in_force)).into(),
                last_update,
                trigger_type: (None).into(),
                tp_trigger_type: if matches!(
                    order_type,
                    OrderType::TakeProfit | OrderType::TrailingStopLoss
                ) {
                    (trigger).into()
                } else {
                    (None).into()
                },
                sl_trigger_type: if order_type == OrderType::StopLoss {
                    (trigger).into()
                } else {
                    (None).into()
                },
                take_profit: (None).into(),
                stop_loss: (None).into(),
                extra: (None).into(),
            });
        }
        Ok(orders)
    }
    .await;
    match outcome {
        Ok(orders) => Ok(orders),
        Err(error) => {
            trace(&error)?;
            Ok(store.orders().to_vec())
        }
    }
}

fn string_value(value: &Value) -> Result<String, ExchangeError> {
    string(Some(value))
}
