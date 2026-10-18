//! Placement payload utilities. Native contracts remain native; price and lot axes stay separate.
use crate::order_services::ServiceError;
use crate::{
    modes::{PositionSideOptions, get_position_side},
    types::{BROKER_ID, REVERSE_ORDER_TYPE},
};
use safe_cex_num::{
    adjust, js_number_string, js_retained_number as js_number, js_retained_parse_float,
};
use safe_cex_store::Store;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{
    MarginType, Market, Order, OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce,
    OrderType, PreparedOrderOpts,
};
use serde_json::{Map, Value};
use std::{future::Future, pin::Pin};

type HostFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, ServiceError<E>>> + 'a>>;
type MarginReader<'a, E> = Box<dyn Fn(&str) -> HostFuture<'a, MarginType, E> + 'a>;
type ModeReader<'a, E> =
    Box<dyn Fn(&str) -> HostFuture<'a, Option<safe_cex_types::PositionMode>, E> + 'a>;
type ClientIdReader<'a, E> =
    Box<dyn Fn(&PreparedOrderOpts) -> Result<Option<String>, ServiceError<E>> + 'a>;
type SplitCheck<'a, E> = Box<dyn Fn(Option<&str>, usize) -> Result<(), ServiceError<E>> + 'a>;
type CancelAlgo<'a, E> = Box<dyn Fn(Vec<Order>) -> HostFuture<'a, (), E> + 'a>;
pub struct FormatHost<'a, E> {
    pub get_margin_type: MarginReader<'a, E>,
    pub get_position_mode: ModeReader<'a, E>,
    pub uuid: Box<dyn Fn() -> Result<String, ServiceError<E>> + 'a>,
    pub resolve_caller_client_order_id: ClientIdReader<'a, E>,
    pub assert_single_native_client_order_id: SplitCheck<'a, E>,
    pub cancel_algo_orders: CancelAlgo<'a, E>,
    pub broker_id_enabled: bool,
}
/// Ordinary own-property Record output; inspect these native values before serializing a request.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderPayload {
    pub fields: Map<String, Value>,
}
fn insert(map: &mut Map<String, Value>, key: &str, value: impl Into<Value>) {
    map.insert(key.into(), value.into());
}
fn number_truthy(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}
fn optional_truthy(value: Option<f64>) -> Option<f64> {
    value.filter(|value| number_truthy(*value))
}
fn margin(mode: MarginType) -> &'static str {
    if mode == MarginType::Isolated {
        "isolated"
    } else {
        "cross"
    }
}
fn side(side: safe_cex_types::ModelField<OrderSide>) -> Option<&'static str> {
    side.into_value().map(|side| match side {
        OrderSide::Buy => "buy",
        OrderSide::Sell => "sell",
    })
}
fn market<E>(opts: &PreparedOrderOpts, store: &dyn Store) -> Result<Market, ServiceError<E>> {
    store
        .markets_by_symbol()?
        .get(&opts.symbol)
        .cloned()
        .ok_or_else(|| ServiceError::Input(format!("Market {} not found on Blofin", opts.symbol)))
}
fn lot_value<E>(market: &Market) -> Result<Option<&Input>, ServiceError<E>> {
    let info = market.info.as_ref().ok_or_else(|| {
        ServiceError::Input("Cannot read properties of undefined (reading 'lotSize')".into())
    })?;
    if info.is_nullish() {
        return Err(ServiceError::Input(
            if info.is_null() {
                "Cannot read properties of null (reading 'lotSize')"
            } else {
                "Cannot read properties of undefined (reading 'lotSize')"
            }
            .into(),
        ));
    }
    Ok(info
        .get("lotSize")
        .filter(|value| value.is_truthy())
        .or_else(|| info.get("minSize")))
}
fn generated_id<E>(host: &FormatHost<'_, E>) -> Result<String, ServiceError<E>> {
    let uuid = (host.uuid)()?;
    // The maintained uuid helper returns0..21 ASCII alphanumerics, after stripping '-'/'_'.
    if uuid.len() > 21 || !uuid.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(ServiceError::Input(
            "UUID host returned a value outside the source uuid domain".into(),
        ));
    }
    Ok(format!("{BROKER_ID}{uuid}").chars().take(32).collect())
}
async fn position_side<E>(
    opts: &PreparedOrderOpts,
    host: &FormatHost<'_, E>,
) -> Result<&'static str, ServiceError<E>> {
    let options = PositionSideOptions {
        symbol: &opts.symbol,
        side: opts.side,
        order_type: opts.r#type,
        reduce_only: opts.reduce_only.unwrap_or(false),
    };
    get_position_side(&options, |symbol| (host.get_position_mode)(symbol)).await
}
fn trigger_type(value: Option<OrderStopTriggerType>) -> &'static str {
    match value {
        Some(OrderStopTriggerType::MarkPrice) => "mark",
        Some(OrderStopTriggerType::IndexPrice) => "index",
        _ => "last",
    }
}
fn order_type_key(value: safe_cex_types::ModelField<OrderType>) -> &'static str {
    match value.into_value() {
        Some(OrderType::Market) => "market",
        Some(OrderType::Limit) => "limit",
        Some(OrderType::Conditional) => "conditional",
        Some(OrderType::StopLoss) => "stop_market",
        Some(OrderType::TakeProfit) => "take_profit_market",
        Some(OrderType::TrailingStopLoss) => "trailing_stop_market",
        None => "undefined",
    }
}
pub async fn format_trigger_order<E>(
    opts: &PreparedOrderOpts,
    source: Option<&Input>,
    store: &dyn Store,
    host: &FormatHost<'_, E>,
) -> Result<OrderPayload, ServiceError<E>> {
    let market = market(opts, store)?;
    if opts.reduce_only == Some(true) || opts.close == Some(true) {
        return Err(ServiceError::Input(
            "BloFin opening trigger orders cannot be reduce-only or close-only".into(),
        ));
    }
    if opts
        .time_in_force
        .is_some_and(|tif| tif != OrderTimeInForce::GoodTillCancel)
    {
        return Err(ServiceError::Input(
            "BloFin opening trigger orders support only GoodTillCancel semantics".into(),
        ));
    }
    let lot = js_number(lot_value(&market)?).map_err(ServiceError::Coercion)?;
    let amount = adjust(opts.amount, lot);
    let trigger_price = optional_truthy(opts.trigger_price)
        .map_or(0.0, |price| adjust(price, market.precision.price));
    let order_price =
        optional_truthy(opts.price).map_or(-1.0, |price| adjust(price, market.precision.price));
    if !amount.is_finite() || amount <= 0.0 {
        return Err(ServiceError::Input(
            "BloFin opening trigger amount must be positive".into(),
        ));
    }
    if !trigger_price.is_finite() || trigger_price <= 0.0 {
        return Err(ServiceError::Input(format!(
            "Trigger price is required for conditional BloFin orders ({})",
            opts.symbol
        )));
    }
    if source.is_some_and(Input::is_truthy)
        && !source
            .and_then(|s| s.get("instId"))
            .is_some_and(|v| v.text_equals(&market.id))
    {
        return Err(ServiceError::Input(
            "Cannot replace a BloFin trigger whose instrument identity changed".into(),
        ));
    }
    let margin_mode = if source
        .and_then(|s| s.get("marginMode"))
        .is_some_and(|v| v.text_equals("cross"))
    {
        "cross"
    } else if source
        .and_then(|s| s.get("marginMode"))
        .is_some_and(|v| v.text_equals("isolated"))
    {
        "isolated"
    } else {
        margin((host.get_margin_type)(&market.symbol).await?)
    };
    let position_side = if source
        .and_then(|s| s.get("positionSide"))
        .is_some_and(|v| v.text_equals("net"))
    {
        "net"
    } else if source
        .and_then(|s| s.get("positionSide"))
        .is_some_and(|v| v.text_equals("long"))
    {
        "long"
    } else if source
        .and_then(|s| s.get("positionSide"))
        .is_some_and(|v| v.text_equals("short"))
    {
        "short"
    } else {
        position_side(opts, host).await?
    };
    let trigger_price_type = if source
        .and_then(|s| s.get("triggerPriceType"))
        .is_some_and(|v| v.text_equals("mark"))
    {
        "mark"
    } else if source
        .and_then(|s| s.get("triggerPriceType"))
        .is_some_and(|v| v.text_equals("index"))
    {
        "index"
    } else {
        "last"
    };
    let mut attached = Map::new();
    if let Some(price) = optional_truthy(opts.take_profit) {
        insert(
            &mut attached,
            "tpTriggerPrice",
            js_number_string(adjust(price, market.precision.price)),
        );
        insert(&mut attached, "tpOrderPrice", "-1");
        insert(
            &mut attached,
            "tpTriggerPriceType",
            trigger_type(opts.tp_trigger_type),
        );
    }
    if let Some(price) = optional_truthy(opts.stop_loss) {
        insert(
            &mut attached,
            "slTriggerPrice",
            js_number_string(adjust(price, market.precision.price)),
        );
        insert(&mut attached, "slOrderPrice", "-1");
        insert(
            &mut attached,
            "slTriggerPriceType",
            trigger_type(opts.sl_trigger_type),
        );
    }
    let mut req = Map::new();
    insert(&mut req, "instId", market.id);
    insert(&mut req, "marginMode", margin_mode);
    insert(&mut req, "positionSide", position_side);
    if let Some(side) = side(opts.side) {
        insert(&mut req, "side", side);
    }
    insert(&mut req, "size", js_number_string(amount));
    insert(&mut req, "clientOrderId", generated_id(host)?);
    insert(&mut req, "orderPrice", js_number_string(order_price));
    insert(&mut req, "orderType", "trigger");
    insert(&mut req, "triggerPrice", js_number_string(trigger_price));
    insert(&mut req, "triggerPriceType", trigger_price_type);
    if !attached.is_empty() {
        insert(
            &mut req,
            "attachAlgoOrders",
            Value::Array(vec![Value::Object(attached)]),
        );
    }
    if host.broker_id_enabled {
        insert(&mut req, "brokerId", BROKER_ID);
    }
    Ok(OrderPayload { fields: req })
}
pub async fn format_algo_order<E>(
    opts: &PreparedOrderOpts,
    store: &dyn Store,
    host: &FormatHost<'_, E>,
) -> Result<OrderPayload, ServiceError<E>> {
    let market = market(opts, store)?;
    let lot = js_retained_parse_float(lot_value(&market)?).map_err(ServiceError::Coercion)?;
    let amount = adjust(opts.amount, lot);
    let price = optional_truthy(opts.price).map(|price| adjust(price, market.precision.price));
    let position_side = position_side(opts, host).await?;
    let mut req = Map::new();
    insert(&mut req, "instId", market.id);
    insert(
        &mut req,
        "marginMode",
        margin((host.get_margin_type)(&market.symbol).await?),
    );
    insert(&mut req, "clientOrderId", generated_id(host)?);
    if host.broker_id_enabled {
        insert(&mut req, "brokerId", BROKER_ID);
    }
    if let Some(side) = side(opts.side) {
        insert(&mut req, "side", side);
    }
    insert(&mut req, "positionSide", position_side);
    insert(
        &mut req,
        "size",
        js_number_string(if amount == 0.0 { -1.0 } else { amount }),
    );
    if matches!(
        opts.r#type.into_value(),
        Some(OrderType::StopLoss | OrderType::TakeProfit)
    ) {
        insert(&mut req, "reduceOnly", "true");
    }
    let price_text = price.map_or_else(|| "null".into(), js_number_string);
    if let Some(price) = optional_truthy(opts.stop_loss) {
        insert(&mut req, "slTriggerPrice", js_number_string(price));
        insert(&mut req, "slOrderPrice", "-1");
    } else if opts.r#type == OrderType::StopLoss {
        insert(&mut req, "slTriggerPrice", price_text.clone());
        insert(&mut req, "slOrderPrice", "-1");
    }
    if let Some(price) = optional_truthy(opts.take_profit) {
        insert(&mut req, "tpTriggerPrice", js_number_string(price));
        insert(&mut req, "tpOrderPrice", "-1");
    } else if opts.r#type == OrderType::TakeProfit {
        insert(&mut req, "tpTriggerPrice", price_text);
        insert(&mut req, "tpOrderPrice", "-1");
    }
    if !number_truthy(opts.amount) {
        let opposite = if opts.r#type == OrderType::StopLoss {
            OrderType::TakeProfit
        } else {
            OrderType::StopLoss
        };
        let orders: Vec<_> = store
            .orders()?
            .iter()
            .filter(|o| {
                o.symbol == opts.symbol
                    && o.r#type == opposite
                    && o.side.value() == opts.side.value()
                    && o.status == OrderStatus::Open
                    && !number_truthy(o.amount)
            })
            .cloned()
            .collect();
        if let Some(first) = orders.first().cloned() {
            (host.cancel_algo_orders)(orders).await?;
            let field = if first.r#type == OrderType::StopLoss {
                "sl"
            } else {
                "tp"
            };
            insert(
                &mut req,
                &format!("{field}TriggerPrice"),
                first
                    .price
                    .into_value()
                    .map(js_number_string)
                    .unwrap_or_else(|| "undefined".into()),
            );
            insert(&mut req, &format!("{field}OrderPrice"), "-1");
        }
    }
    Ok(OrderPayload { fields: req })
}
pub async fn format_normal_order<E>(
    opts: &PreparedOrderOpts,
    store: &dyn Store,
    host: &FormatHost<'_, E>,
) -> Result<Vec<OrderPayload>, ServiceError<E>> {
    let market = market(opts, store)?;
    let max_size = market.limits.amount.max;
    let lot = js_retained_parse_float(lot_value(&market)?).map_err(ServiceError::Coercion)?;
    let amount = adjust(opts.amount, lot);
    let price = optional_truthy(opts.price).map(|price| adjust(price, market.precision.price));
    let margin_mode = (host.get_margin_type)(&market.symbol).await?;
    let caller_id = (host.resolve_caller_client_order_id)(opts)?;
    let mut req = Map::new();
    insert(&mut req, "instId", market.id);
    insert(
        &mut req,
        "clientOrderId",
        match &caller_id {
            Some(id) => id.clone(),
            None => generated_id(host)?,
        },
    );
    if let Some(side) = side(opts.side) {
        insert(&mut req, "side", side);
    }
    if opts.time_in_force == Some(OrderTimeInForce::PostOnly) && opts.r#type == OrderType::Limit {
        insert(&mut req, "orderType", "post_only");
    } else if let Some(kind) = REVERSE_ORDER_TYPE.get(order_type_key(opts.r#type)) {
        insert(&mut req, "orderType", kind.clone());
    }
    insert(&mut req, "size", js_number_string(amount));
    if opts.r#type == OrderType::Limit {
        insert(
            &mut req,
            "price",
            price.map_or_else(|| "null".into(), js_number_string),
        );
    }
    if opts.reduce_only == Some(true) {
        insert(&mut req, "reduceOnly", true);
    }
    insert(&mut req, "positionSide", position_side(opts, host).await?);
    insert(&mut req, "marginMode", margin(margin_mode));
    if host.broker_id_enabled {
        insert(&mut req, "brokerId", BROKER_ID);
    }
    let mut attached = Map::new();
    for (prefix, price) in [("sl", opts.stop_loss), ("tp", opts.take_profit)] {
        if let Some(price) = optional_truthy(price) {
            let field = format!("{prefix}TriggerPrice");
            let text = js_number_string(price);
            insert(&mut req, &field, text.clone());
            insert(&mut req, &format!("{prefix}OrderPrice"), "-1");
            insert(&mut attached, &field, text);
            insert(&mut attached, &format!("{prefix}OrderPrice"), "-1");
        }
    }
    if !attached.is_empty() {
        insert(
            &mut req,
            "attachAlgoOrders",
            Value::Array(vec![Value::Object(attached)]),
        );
    }
    if amount <= max_size {
        return Ok(vec![OrderPayload { fields: req }]);
    }
    let lots = (amount / max_size).ceil();
    let rest = adjust(amount % max_size, lot);
    let per_batch = adjust((amount - rest) / lots, lot);
    // lodash/times returns[] for NaN, nonpositive and >MAX_SAFE_INTEGER counts.
    let count = if lots.is_nan() || lots < 1.0 || lots > 9_007_199_254_740_991.0 {
        0
    } else {
        // CHOSEN allocation profile, rather than allocating unbounded hostile arrays.
        if lots > 10_000.0 {
            return Err(ServiceError::Input(
                "BloFin split exceeds the native10000-payload allocation profile".into(),
            ));
        }
        lots as usize
    };
    let total_count = count + usize::from(number_truthy(rest));
    if total_count > 10_000 {
        return Err(ServiceError::Input(
            "BloFin split exceeds the native10000-payload allocation profile".into(),
        ));
    }
    let mut payloads = Vec::with_capacity(total_count);
    for _ in 0..count {
        let mut fields = req.clone();
        insert(&mut fields, "size", js_number_string(per_batch));
        insert(&mut fields, "clientOrderId", generated_id(host)?);
        payloads.push(OrderPayload { fields });
    }
    if number_truthy(rest) {
        let mut fields = req;
        insert(&mut fields, "size", js_number_string(rest));
        insert(&mut fields, "clientOrderId", generated_id(host)?);
        payloads.push(OrderPayload { fields });
    }
    (host.assert_single_native_client_order_id)(caller_id.as_deref(), payloads.len())?;
    Ok(payloads)
}
