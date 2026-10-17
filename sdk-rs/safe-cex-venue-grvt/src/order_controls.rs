//! GRVT order controls against configured, bound SDK methods.
use crate::{
    builder::GrvtBuilderUnauthorizedError,
    order_semantics::{get_grvt_trigger_metadata, symbol_to_instrument_id},
    types::GrvtTriggerKind,
};
use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_num::js_truthy;
use safe_cex_types::{
    Candle, OHLCVOptions, Order, OrderBook, OrderType, PlaceOrderOpts, PreparedOrderOpts,
    UpdateOrderOpts, UpdateOrderOptsUpdate,
};
use serde_json::{Value, json};
use std::future::Future;

fn cancellation_error<T>(
    result: Result<T, ExchangeError>,
    prefix: &str,
    emit_error: &mut impl FnMut(&str) -> Result<(), ExchangeError>,
) -> Result<T, ExchangeError> {
    match result {
        Ok(value) => Ok(value),
        Err(cause) => {
            let data = cause
                .response
                .as_ref()
                .and_then(|response| response.get("data"));
            let message = data
                .and_then(|data| data.get("message"))
                .filter(|v| js_truthy(Some(v)))
                .or_else(|| {
                    data.and_then(|data| data.get("m"))
                        .filter(|v| js_truthy(Some(v)))
                });
            let code = data
                .and_then(|data| data.get("code"))
                .filter(|v| js_truthy(Some(v)))
                .or_else(|| data.and_then(|data| data.get("c")));
            let stringify = |value| {
                safe_cex_types::js_coercion::js_string(value).map_err(|error| {
                    let mut native = ExchangeError::new(error.message);
                    native.name = error.name.into();
                    native
                })
            };
            let message = match message {
                Some(value) => stringify(Some(value))?,
                None => cause.message.clone(),
            };
            let full = if js_truthy(code) {
                format!("{message} (code: {})", stringify(code)?)
            } else {
                message
            };
            emit_error(&format!("{prefix}{full}"))?;
            Err(ExchangeError::new(full))
        }
    }
}

/// ts: GrvtExchange.cancelOrder. The configured SDK method is bound before endpoint await.
pub fn cancel_order<
    F: Future<Output = Result<Value, ExchangeError>>,
    G: Future<Output = Result<(), ExchangeError>>,
>(
    order: &Order,
    has_client: bool,
    account: impl FnOnce() -> Option<String>,
    real_order_id: Option<&str>,
    authenticate_endpoint: impl FnOnce() -> F,
    cancel: impl FnOnce(Value, Value) -> G,
    mut log: impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError>,
    mut emit_error: impl FnMut(&str) -> Result<(), ExchangeError>,
) -> impl Future<Output = Result<(), ExchangeError>> {
    let id = order.id.clone();
    let begin = (|| {
        if !has_client {
            return Err(ExchangeError::new("GRVT SDK client not initialized"));
        }
        let account = account()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| ExchangeError::new("Trading account ID required to cancel orders"))?;
        let real = real_order_id.filter(|id| !id.is_empty());
        let payload = match real {
            Some(real) => json!({"sub_account_id":account, "order_id":real}),
            None => json!({"sub_account_id":account, "client_order_id":id}),
        };
        log(
            &format!(
                "Cancelling order: storeId={id} realId={}",
                real.unwrap_or("none")
            ),
            LogSeverity::Info,
        )?;
        Ok((payload, authenticate_endpoint()))
    })();
    let begin = cancellation_error(begin, "Cancel failed: ", &mut emit_error);
    async move {
        let (payload, endpoint) = begin?;
        let result = async {
            cancel(payload, endpoint.await?).await?;
            log(
                &format!("Order {id} cancelled successfully"),
                LogSeverity::Info,
            )
        }
        .await;
        cancellation_error(result, "Cancel failed: ", &mut emit_error)
    }
}

fn cancel_scope<
    F: Future<Output = Result<Value, ExchangeError>>,
    G: Future<Output = Result<(), ExchangeError>>,
>(
    symbol: Option<&str>,
    has_client: bool,
    account: impl FnOnce() -> Option<String>,
    authenticate_endpoint: impl FnOnce() -> F,
    cancel: impl FnOnce(Value, Value) -> G,
    mut log: impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError>,
    mut emit_error: impl FnMut(&str) -> Result<(), ExchangeError>,
) -> impl Future<Output = Result<(), ExchangeError>> {
    let prefix = symbol.map_or_else(
        || "Cancel all failed: ".into(),
        |s| format!("Cancel {s} orders failed: "),
    );
    let success = symbol.map_or_else(
        || "All orders cancelled successfully".into(),
        |s| format!("All orders for {s} cancelled successfully"),
    );
    let begin = (|| {
        if !has_client {
            return Err(ExchangeError::new("GRVT SDK client not initialized"));
        }
        let account = account()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| ExchangeError::new("Trading account ID required to cancel orders"))?;
        let mut payload = json!({"sub_account_id":account});
        if let Some(symbol) = symbol {
            payload["instrument_id"] = json!(symbol_to_instrument_id(symbol));
        }
        Ok((payload, authenticate_endpoint()))
    })();
    let begin = cancellation_error(begin, &prefix, &mut emit_error);
    async move {
        let (payload, endpoint) = begin?;
        let result = async {
            cancel(payload, endpoint.await?).await?;
            log(&success, LogSeverity::Info)
        }
        .await;
        cancellation_error(result, &prefix, &mut emit_error)
    }
}

/// ts: GrvtExchange.cancelAllOrders.
pub fn cancel_all_orders<
    F: Future<Output = Result<Value, ExchangeError>>,
    G: Future<Output = Result<(), ExchangeError>>,
>(
    has_client: bool,
    account: impl FnOnce() -> Option<String>,
    authenticate_endpoint: impl FnOnce() -> F,
    cancel: impl FnOnce(Value, Value) -> G,
    log: impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError>,
    emit_error: impl FnMut(&str) -> Result<(), ExchangeError>,
) -> impl Future<Output = Result<(), ExchangeError>> {
    cancel_scope(
        None,
        has_client,
        account,
        authenticate_endpoint,
        cancel,
        log,
        emit_error,
    )
}

/// ts: GrvtExchange.cancelSymbolOrders.
pub fn cancel_symbol_orders<
    F: Future<Output = Result<Value, ExchangeError>>,
    G: Future<Output = Result<(), ExchangeError>>,
>(
    symbol: &str,
    has_client: bool,
    account: impl FnOnce() -> Option<String>,
    authenticate_endpoint: impl FnOnce() -> F,
    cancel: impl FnOnce(Value, Value) -> G,
    log: impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError>,
    emit_error: impl FnMut(&str) -> Result<(), ExchangeError>,
) -> impl Future<Output = Result<(), ExchangeError>> {
    cancel_scope(
        Some(symbol),
        has_client,
        account,
        authenticate_endpoint,
        cancel,
        log,
        emit_error,
    )
}

/// ts: GrvtExchange.cancelOrders. Failures are logged before proceeding sequentially.
pub fn cancel_orders<'a, F: Future<Output = Result<(), ExchangeError>> + 'a>(
    orders: &'a [Order],
    mut cancel: impl FnMut(&Order) -> F + 'a,
    mut log: impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError> + 'a,
) -> impl Future<Output = Result<(), ExchangeError>> + 'a {
    let mut first = orders.first().map(&mut cancel);
    async move {
        for order in orders {
            let request = first.take().unwrap_or_else(|| cancel(order));
            if request.await.is_err() {
                log(
                    &format!("Failed to cancel order {}, continuing...", order.id),
                    LogSeverity::Warning,
                )?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum BatchOrderError {
    Exchange(ExchangeError),
    Builder(GrvtBuilderUnauthorizedError),
}

/// ts: GrvtExchange.placeOrders. Builder errors preserve their native variant and stop the batch.
pub fn place_orders<'a, F: Future<Output = Result<Vec<String>, BatchOrderError>> + 'a>(
    orders: &'a [PlaceOrderOpts],
    preflight: impl FnOnce(&[PlaceOrderOpts]) -> Result<(), ExchangeError>,
    mut place: impl FnMut(&PlaceOrderOpts) -> F + 'a,
    mut log: impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError> + 'a,
) -> impl Future<Output = Result<Vec<String>, BatchOrderError>> + 'a {
    let preflight = preflight(orders).map_err(BatchOrderError::Exchange);
    let mut first = if preflight.is_ok() {
        orders.first().map(&mut place)
    } else {
        None
    };
    async move {
        preflight?;
        let mut ids = Vec::new();
        for order in orders {
            let request = first.take().unwrap_or_else(|| place(order));
            match request.await {
                Ok(result) => ids.extend(result),
                Err(cause @ BatchOrderError::Builder(_)) => return Err(cause),
                Err(BatchOrderError::Exchange(_)) => log(
                    &format!("Failed to place order for {}, continuing...", order.symbol),
                    LogSeverity::Warning,
                )
                .map_err(BatchOrderError::Exchange)?,
            }
        }
        Ok(ids)
    }
}

fn replacement(opts: &UpdateOrderOpts) -> Result<PreparedOrderOpts, ExchangeError> {
    let order = &opts.order;
    let time_in_force = order.time_in_force.ok_or_else(|| {
        ExchangeError::new(format!(
            "Cannot safely update GRVT order {}: missing time-in-force",
            order.id
        ))
    })?;
    let (amount, price) = match &opts.update {
        UpdateOrderOptsUpdate::Variant0(update) => {
            (update.amount, update.price.or(order.price.into_value()))
        }
        UpdateOrderOptsUpdate::Variant1(update) => {
            (update.amount.unwrap_or(order.amount), Some(update.price))
        }
    };
    let mut new = PreparedOrderOpts {
        symbol: order.symbol.clone(),
        r#type: order.r#type.into_property(),
        side: order.side.into_property(),
        amount,
        price,
        reduce_only: order.reduce_only.into_property(),
        time_in_force: Some(time_in_force),
        trigger_price: None,
        stop_loss: None,
        take_profit: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: None,
        max_slippage_pct: None,
        close: None,
        implied_volatility: None,
    };
    if matches!(
        order.r#type,
        safe_cex_types::ModelField::Value(OrderType::StopLoss)
            | safe_cex_types::ModelField::Value(OrderType::TakeProfit)
    ) {
        let info = match order.info.as_ref() {
            None => return Err(trigger_info_error("undefined")),
            Some(Value::Null) => return Err(trigger_info_error("null")),
            Some(info) => info,
        };
        let trigger = info
            .as_object()
            .map(get_grvt_trigger_metadata)
            .transpose()
            .map_err(|cause| {
                let mut error = ExchangeError::new(cause.message);
                error.name = cause.name.into();
                error
            })?
            .flatten()
            .filter(|trigger| {
                trigger.r#type
                    == if order.r#type == OrderType::StopLoss {
                        GrvtTriggerKind::StopLoss
                    } else {
                        GrvtTriggerKind::TakeProfit
                    }
            })
            .ok_or_else(|| {
                ExchangeError::new(format!(
                    "Cannot safely update GRVT order {}: missing trigger metadata",
                    order.id
                ))
            })?;
        let trigger_price = trigger
            .trigger_price
            .filter(|price| price.is_finite() && *price > 0.0)
            .ok_or_else(|| {
                ExchangeError::new(format!(
                    "Cannot safely update GRVT order {}: missing trigger price",
                    order.id
                ))
            })?;
        let (Some(trigger_type), Some(close), Some(split)) = (
            trigger.trigger_type,
            trigger.close,
            trigger.is_split_position,
        ) else {
            return Err(ExchangeError::new(format!(
                "Cannot safely update GRVT order {}: incomplete trigger metadata",
                order.id
            )));
        };
        if split {
            return Err(ExchangeError::new(format!(
                "Cannot safely update GRVT order {}: split-position trigger is not supported",
                order.id
            )));
        }
        let price_update = match &opts.update {
            UpdateOrderOptsUpdate::Variant0(u) => u.price,
            UpdateOrderOptsUpdate::Variant1(u) => Some(u.price),
        };
        new.price = Some(price_update.unwrap_or(trigger_price));
        new.trigger_price = new.price;
        new.close = Some(close);
        if order.r#type == OrderType::StopLoss {
            new.sl_trigger_type = Some(trigger_type);
        } else {
            new.tp_trigger_type = Some(trigger_type);
        }
    }
    Ok(new)
}

fn trigger_info_error(kind: &str) -> ExchangeError {
    let mut error = ExchangeError::new(format!(
        "Cannot read properties of {kind} (reading 'metadata')"
    ));
    error.name = "TypeError".into();
    error
}

/// ts: GrvtExchange.updateOrder. Validate the complete replacement before cancellation.
pub fn update_order<
    F: Future<Output = Result<(), ExchangeError>>,
    G: Future<Output = Result<Vec<String>, ExchangeError>>,
>(
    opts: &UpdateOrderOpts,
    cancel: impl FnOnce(&Order) -> F,
    place: impl FnOnce(PreparedOrderOpts) -> G,
    mut log: impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError>,
    mut emit_error: impl FnMut(&str) -> Result<(), ExchangeError>,
) -> impl Future<Output = Result<Vec<String>, ExchangeError>> {
    let id = opts.order.id.clone();
    let count = match &opts.update {
        UpdateOrderOptsUpdate::Variant0(u) => 1 + usize::from(u.price.is_some()),
        UpdateOrderOptsUpdate::Variant1(u) => 1 + usize::from(u.amount.is_some()),
    };
    let begin = (|| {
        log(
            &format!("Updating order {id} (fieldCount={count})"),
            LogSeverity::Info,
        )?;
        let new = replacement(opts)?;
        Ok((new, cancel(&opts.order)))
    })();
    let begin = cancellation_error(begin, "Update order failed: ", &mut emit_error);
    async move {
        let (new, request) = begin?;
        let result = async {
            request.await?;
            log(
                &format!("Order {id} cancelled for update"),
                LogSeverity::Debug,
            )?;
            let ids = place(new).await?;
            log(
                &format!(
                    "Order updated successfully: cancelled {id}, created {}",
                    ids.join(", ")
                ),
                LogSeverity::Info,
            )?;
            Ok(ids)
        }
        .await;
        cancellation_error(result, "Update order failed: ", &mut emit_error)
    }
}

/// ts: GrvtExchange.listenOHLCV. Preserve the delegate's callback and cleanup value.
pub fn listen_ohlcv<C, R>(
    opts: &OHLCVOptions,
    callback: C,
    listen: impl FnOnce(&OHLCVOptions, C) -> R,
) -> R
where
    C: FnMut(Candle),
{
    listen(opts, callback)
}

/// ts: GrvtExchange.listenOrderBook.
pub fn listen_order_book<C, R>(symbol: &str, callback: C, listen: impl FnOnce(&str, C) -> R) -> R
where
    C: FnMut(OrderBook),
{
    listen(symbol, callback)
}

#[cfg(test)]
mod accepted_contract_tests {
    #[test]
    fn accepted_trigger_info_error_factory() {
        for kind in ["undefined", "null", "café\n\0💠"] {
            let actual = super::trigger_info_error(kind);
            assert_eq!(actual.name, "TypeError", "native diagnostic name");
            assert_eq!(
                actual.message,
                format!("Cannot read properties of {kind} (reading 'metadata')"),
                "native diagnostic text"
            );
            assert_eq!(
                actual.metadata.as_ref(),
                &safe_cex_exchange::runtime::ExchangeErrorMetadata::default(),
                "fresh native metadata"
            );
        }
    }
}
