//! Order mapping and request helpers from BackpackExchange.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::JsJsonValue;
use safe_cex_types::{
    Order, OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce, OrderType,
    PlaceOrderOpts,
};

// ts: BackpackExchange#canUseNativeBatch
pub fn can_use_native_batch<T, R, E>(
    opts: &T,
    ordinary: impl FnOnce(&T) -> Result<bool, E>,
    has_client_id: impl FnOnce(&T) -> Result<bool, E>,
    build: impl FnOnce(&T, f64) -> Result<R, E>,
) -> Result<bool, E> {
    if !ordinary(opts)? || has_client_id(opts)? {
        return Ok(false);
    }
    Ok(build(opts, 1.0).is_ok())
}

pub type BatchRequests<R> = std::rc::Rc<std::cell::RefCell<Vec<R>>>;

// ts: BackpackExchange#placeBatchPair
pub fn place_batch_pair<'a, T, R: 'a, D: 'a, P: 'a, E: 'a, F>(
    orders: &[T; 2],
    create_ids: impl FnOnce(usize) -> Result<Vec<f64>, E>,
    mut build: impl FnMut(&T, Option<f64>) -> Result<R, E>,
    post: impl FnOnce(&'static str, BatchRequests<R>) -> Result<F, E>,
    decode: impl FnOnce(D, BatchRequests<R>) -> Result<P, E> + 'a,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<P, E>> + 'a>>
where
    F: std::future::Future<Output = Result<D, E>> + 'a,
{
    let prepared: Result<(F, BatchRequests<R>), E> = (|| {
        let client_ids = create_ids(orders.len())?;
        let mut requests = Vec::with_capacity(orders.len());
        for (index, order) in orders.iter().enumerate() {
            requests.push(build(order, client_ids.get(index).copied())?);
        }
        let requests = std::rc::Rc::new(std::cell::RefCell::new(requests));
        let response = post(
            crate::constants::ENDPOINTS
                .iter()
                .find(|(name, _)| *name == "ORDERS")
                .expect("Backpack ORDERS endpoint")
                .1,
            requests.clone(),
        )?;
        Ok((response, requests))
    })();
    match prepared {
        Err(error) => Box::pin(std::future::ready(Err(error))),
        Ok((response, requests)) => Box::pin(async move { decode(response.await?, requests) }),
    }
}

// ts: BackpackExchange#mapOrder
pub fn map_order<E>(
    order: &crate::types::BackpackOrder,
    determine: impl FnOnce(&str, OrderSide, f64) -> Result<OrderType, E>,
    log: impl FnOnce(&str, &str) -> Result<(), E>,
) -> Result<Order, E> {
    use crate::types::{
        BackpackOrderOrderType, BackpackOrderSide, BackpackOrderStatus, BackpackOrderTimeInForce,
    };
    let side = if order.side == BackpackOrderSide::Bid {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    };
    let trigger_price = order
        .triggerPrice
        .as_deref()
        .filter(|price| !price.is_empty())
        .map(safe_cex_num::parse_float);
    let is_trigger_order = trigger_price.is_some();
    let order_type = if order.orderType == BackpackOrderOrderType::Market && is_trigger_order {
        determine(&order.symbol, side, trigger_price.expect("trigger exists"))?
    } else if order.orderType == BackpackOrderOrderType::Market {
        OrderType::Market
    } else {
        OrderType::Limit
    };
    let display_price = trigger_price.unwrap_or_else(|| safe_cex_num::parse_float(&order.price));
    let raw_amount = if is_trigger_order {
        order
            .triggerQuantity
            .as_deref()
            .filter(|quantity| !quantity.is_empty())
            .unwrap_or(&order.quantity)
    } else {
        &order.quantity
    };
    let parsed_amount = safe_cex_num::parse_float(raw_amount);
    let amount = if parsed_amount == 0.0 || parsed_amount.is_nan() {
        0.0
    } else {
        parsed_amount
    };
    if is_trigger_order && amount == 0.0 {
        log(
            &format!(
                "Warning: Trigger order {} has zero amount. quantity={}, triggerQuantity={}",
                order.id,
                order.quantity,
                order.triggerQuantity.as_deref().unwrap_or("undefined")
            ),
            "warning",
        )?;
    }
    let executed = order
        .executedQuantity
        .as_deref()
        .filter(|quantity| !quantity.is_empty())
        .unwrap_or("0");
    let status = match order.status {
        BackpackOrderStatus::New => "New",
        BackpackOrderStatus::PartiallyFilled => "PartiallyFilled",
        BackpackOrderStatus::Filled => "Filled",
        BackpackOrderStatus::Cancelled => "Cancelled",
        BackpackOrderStatus::Expired => "Expired",
        BackpackOrderStatus::TriggerPending => "TriggerPending",
    };
    let tif = match order.timeInForce {
        BackpackOrderTimeInForce::Gtc => "GTC",
        BackpackOrderTimeInForce::Ioc => "IOC",
        BackpackOrderTimeInForce::Fok => "FOK",
        BackpackOrderTimeInForce::Po => "PO",
    };
    Ok(Order {
        client_order_id: (None).into(),
        id: order.id.clone(),
        symbol: order.symbol.clone(),
        r#type: (order_type).into(),
        side: side.into(),
        price: (display_price).into(),
        amount,
        filled: safe_cex_num::parse_float(executed),
        remaining: amount - safe_cex_num::parse_float(executed),
        status: (map_order_status(status)).into(),
        last_update: order.createdAt,
        reduce_only: order.reduceOnly.unwrap_or(is_trigger_order).into(),
        time_in_force: (Some(map_backpack_time_in_force(tif))).into(),
        parent_id: (None).into(),
        close: (None).into(),
        info: (None).into(),
        trigger_type: (None).into(),
        tp_trigger_type: (None).into(),
        sl_trigger_type: (None).into(),
        take_profit: (None).into(),
        stop_loss: (None).into(),
        extra: (None).into(),
    })
}

fn valid_client_id(value: f64) -> bool {
    value.is_finite() && value.fract() == 0.0 && value > 0.0 && value <= f64::from(u32::MAX)
}

fn own<'a>(fields: &'a [(String, JsJsonValue)], name: &str) -> &'a JsJsonValue {
    fields
        .iter()
        .rev()
        .find(|(key, _)| key == name)
        .map_or(&JsJsonValue::Undefined, |(_, value)| value)
}

fn client_id(value: &JsJsonValue) -> Option<f64> {
    match value {
        JsJsonValue::Number(value) if valid_client_id(*value) => Some(*value),
        _ => None,
    }
}

fn string_value(value: &JsJsonValue) -> Result<String, ExchangeError> {
    let text = if matches!(value, JsJsonValue::Undefined) {
        safe_cex_num::js_string(None)
    } else {
        safe_cex_num::js_string(Some(&crate::signing::string_input(value)))
    };
    text.map_err(|error| {
        let mut result = ExchangeError::new(error.message);
        result.name = error.name.into();
        result
    })
}

// ts: BackpackExchange#decodeBatchOrderResponse
pub fn decode_batch_order_response(
    data: &JsJsonValue,
    requests: &[crate::types::BackpackOrderRequest],
) -> Result<Vec<String>, ExchangeError> {
    let JsJsonValue::Array(rows) = data else {
        return Err(ExchangeError::new(format!(
            "Backpack batch response count mismatch: expected {}",
            requests.len()
        )));
    };
    if rows.len() != requests.len() {
        return Err(ExchangeError::new(format!(
            "Backpack batch response count mismatch: expected {}",
            requests.len()
        )));
    }
    let mut expected = Vec::new();
    for request in requests {
        let Some(id) = request.clientId.filter(|id| valid_client_id(*id)) else {
            return Err(ExchangeError::new(
                "Backpack batch request is missing a valid uint32 clientId",
            ));
        };
        if expected.iter().any(|(existing, _)| *existing == id) {
            return Err(ExchangeError::new(format!(
                "Duplicate Backpack batch request clientId: {}",
                safe_cex_num::js_number_string(id)
            )));
        }
        expected.push((id, request));
    }
    let mut exchange_ids: Vec<(f64, String)> = Vec::new();
    for row in rows {
        let JsJsonValue::Object(fields) = row else {
            return Err(ExchangeError::new("Malformed Backpack batch response row"));
        };
        let operation = own(fields, "operation");
        if operation != &JsJsonValue::String("Ok".into()) {
            return Err(ExchangeError::new(format!(
                "Backpack batch row failed with operation {}",
                string_value(operation)?
            )));
        }
        let Some(id) = client_id(own(fields, "clientId")) else {
            return Err(ExchangeError::new(
                "Backpack batch response row is missing a valid uint32 clientId",
            ));
        };
        let Some((_, request)) = expected.iter().find(|(expected_id, _)| *expected_id == id) else {
            return Err(ExchangeError::new(format!(
                "Unknown Backpack batch response clientId: {}",
                safe_cex_num::js_number_string(id)
            )));
        };
        if exchange_ids.iter().any(|(existing, _)| *existing == id) {
            return Err(ExchangeError::new(format!(
                "Duplicate Backpack batch response clientId: {}",
                safe_cex_num::js_number_string(id)
            )));
        }
        let side = match request.side {
            crate::types::BackpackOrderRequestSide::Bid => "Bid",
            crate::types::BackpackOrderRequestSide::Ask => "Ask",
        };
        let order_type = match request.orderType {
            crate::types::BackpackOrderRequestOrderType::Limit => "Limit",
            crate::types::BackpackOrderRequestOrderType::Market => "Market",
        };
        if own(fields, "symbol") != &JsJsonValue::String(request.symbol.clone())
            || own(fields, "side") != &JsJsonValue::String(side.into())
            || own(fields, "orderType") != &JsJsonValue::String(order_type.into())
        {
            return Err(ExchangeError::new(format!(
                "Backpack batch response identity mismatch for clientId {}",
                safe_cex_num::js_number_string(id)
            )));
        }
        let exchange_id = match own(fields, "id") {
            JsJsonValue::String(id) => safe_cex_num::js_trim(id),
            _ => "",
        };
        if exchange_id.is_empty() {
            return Err(ExchangeError::new(format!(
                "Backpack batch response clientId {} is missing an exchange order id",
                safe_cex_num::js_number_string(id)
            )));
        }
        if exchange_ids
            .iter()
            .any(|(_, existing)| existing == exchange_id)
        {
            return Err(ExchangeError::new(format!(
                "Duplicate Backpack batch exchange order id: {exchange_id}"
            )));
        }
        exchange_ids.push((id, exchange_id.to_owned()));
    }
    Ok(expected
        .iter()
        .map(|(id, _)| {
            exchange_ids
                .iter()
                .find(|(existing, _)| existing == id)
                .expect("equal cardinality and unique matched clientIds cover every request")
                .1
                .clone()
        })
        .collect())
}

// ts: BackpackExchange#nextBatchClientId
pub fn next_batch_client_id(
    get_random_values: Option<impl FnOnce(&mut [u32; 1]) -> Result<[u32; 1], ExchangeError>>,
    random: impl FnOnce() -> Result<f64, ExchangeError>,
) -> Result<f64, ExchangeError> {
    if let Some(get_random_values) = get_random_values {
        let mut values = [0; 1];
        return get_random_values(&mut values).map(|returned| f64::from(returned[0]));
    }
    Ok((random()? * 0x1_0000_0000_u64 as f64).floor())
}

// ts: BackpackExchange#createBatchClientIds
pub fn create_batch_client_ids(
    count: f64,
    mut next: impl FnMut() -> Result<f64, ExchangeError>,
) -> Result<Vec<f64>, ExchangeError> {
    let mut ids = Vec::new();
    for _attempt in 0..64 {
        // JS `!(ids.length < count)`. count is a raw f64 and a NaN count is
        // part of this helper's contract: `!(len < NaN)` is true and breaks at
        // once, while clippy's `len >= NaN` is false and would run all 64.
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !((ids.len() as f64) < count) {
            break;
        }
        let client_id = next()?;
        if client_id.is_finite()
            && client_id.fract() == 0.0
            && client_id > 0.0
            && client_id <= f64::from(u32::MAX)
            && !ids.contains(&client_id)
        {
            ids.push(client_id);
        }
    }
    if ids.len() as f64 != count {
        return Err(ExchangeError::new(format!(
            "Unable to generate {} unique Backpack uint32 clientIds",
            safe_cex_num::js_number_string(count)
        )));
    }
    Ok(ids)
}

#[derive(Debug, Clone, Copy)]
pub enum TriggerKind {
    TakeProfit,
    StopLoss,
}

// ts: BackpackExchange#getTriggerBy
pub fn get_trigger_by(
    kind: TriggerKind,
    explicit: Option<OrderStopTriggerType>,
    metadata: impl FnOnce(&str) -> (Option<OrderStopTriggerType>, Option<OrderStopTriggerType>),
) -> crate::types::BackpackOrderRequestTriggerBy {
    // Source looks up metadata even when the caller supplies an explicit type.
    let (take_profit, stop_loss) = metadata("backpack");
    let trigger = explicit.or(match kind {
        TriggerKind::TakeProfit => take_profit,
        TriggerKind::StopLoss => stop_loss,
    });
    match trigger {
        Some(OrderStopTriggerType::LastPrice) => {
            crate::types::BackpackOrderRequestTriggerBy::LastPrice
        }
        Some(OrderStopTriggerType::IndexPrice) => {
            crate::types::BackpackOrderRequestTriggerBy::IndexPrice
        }
        _ => crate::types::BackpackOrderRequestTriggerBy::MarkPrice,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TickerPrice<'a> {
    pub symbol: &'a str,
    pub last: Option<f64>,
    pub mark: Option<f64>,
}

// ts: BackpackExchange#determineTriggerOrderType
pub fn determine_trigger_order_type(
    tickers: &[TickerPrice<'_>],
    symbol: &str,
    side: OrderSide,
    trigger_price: f64,
) -> OrderType {
    let current_price = tickers
        .iter()
        .find(|ticker| ticker.symbol == symbol)
        .and_then(|ticker| ticker.last.or(ticker.mark));
    let Some(current_price) = current_price.filter(|price| *price != 0.0 && !price.is_nan()) else {
        return OrderType::StopLoss;
    };
    if side == OrderSide::Sell {
        if trigger_price < current_price {
            OrderType::StopLoss
        } else {
            OrderType::TakeProfit
        }
    } else if trigger_price > current_price {
        OrderType::StopLoss
    } else {
        OrderType::TakeProfit
    }
}

// ts: BackpackExchange#mapOrderStatus
pub fn map_order_status(status: &str) -> OrderStatus {
    match status {
        "New" | "PartiallyFilled" => OrderStatus::Open,
        "Filled" => OrderStatus::Closed,
        "Cancelled" | "Expired" => OrderStatus::Canceled,
        _ => OrderStatus::Open,
    }
}

// ts: BackpackExchange#mapTimeInForce
pub fn map_time_in_force(tif: Option<OrderTimeInForce>) -> &'static str {
    match tif {
        Some(OrderTimeInForce::ImmediateOrCancel) => "IOC",
        Some(OrderTimeInForce::FillOrKill) => "FOK",
        _ => "GTC",
    }
}

// ts: BackpackExchange#mapBackpackTimeInForce
pub fn map_backpack_time_in_force(tif: &str) -> OrderTimeInForce {
    match tif {
        "IOC" => OrderTimeInForce::ImmediateOrCancel,
        "FOK" => OrderTimeInForce::FillOrKill,
        _ => OrderTimeInForce::GoodTillCancel,
    }
}

// ts: BackpackExchange#isOrdinaryLimitOrder
pub fn is_ordinary_limit_order(opts: &PlaceOrderOpts) -> bool {
    opts.r#type == OrderType::Limit
        && opts.trigger_price.is_none()
        && opts.take_profit.is_none()
        && opts.stop_loss.is_none()
        && opts.tp_trigger_type.is_none()
        && opts.sl_trigger_type.is_none()
        && opts.close != Some(true)
}
