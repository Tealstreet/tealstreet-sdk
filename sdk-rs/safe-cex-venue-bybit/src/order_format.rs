//! Pure request formatting extracted from the exchange; callers supply fetched position mode.
use safe_cex_num::{adjust, js_number_string};
use safe_cex_types::{
    Market, OrderStopTriggerType as Trigger, OrderTimeInForce, OrderType, PlaceOrderOpts, Ticker,
};
use serde_json::{Map, Value, json};

pub fn get_bybit_trigger_by(opts: &PlaceOrderOpts) -> Option<&'static str> {
    match opts.r#type {
        OrderType::Conditional => Some(match opts.trigger_type {
            Some(Trigger::MarkPrice) => "MarkPrice",
            Some(Trigger::IndexPrice) => "IndexPrice",
            _ => "LastPrice",
        }),
        OrderType::StopLoss => Some(if opts.sl_trigger_type == Some(Trigger::LastPrice) {
            "LastPrice"
        } else {
            "MarkPrice"
        }),
        OrderType::TakeProfit => Some(if opts.tp_trigger_type == Some(Trigger::MarkPrice) {
            "MarkPrice"
        } else {
            "LastPrice"
        }),
        _ => None,
    }
}
pub fn get_bybit_trigger_direction_price(opts: &PlaceOrderOpts, ticker: &Ticker) -> f64 {
    match opts.r#type {
        OrderType::Conditional => match opts.trigger_type {
            Some(Trigger::MarkPrice) => ticker.mark,
            Some(Trigger::IndexPrice) => ticker.index.unwrap_or(0.0),
            _ => ticker.last,
        },
        OrderType::StopLoss => {
            if opts.sl_trigger_type == Some(Trigger::LastPrice) {
                ticker.last
            } else {
                ticker.mark
            }
        }
        // Source deliberately defaults direction to mark, although triggerBy defaults to last.
        OrderType::TakeProfit => {
            if opts.tp_trigger_type == Some(Trigger::LastPrice) {
                ticker.last
            } else {
                ticker.mark
            }
        }
        _ => ticker.last,
    }
}
fn truthy(value: f64) -> bool {
    value != 0. && !value.is_nan()
}
fn inverse(table: &Value, value: &Value) -> Option<String> {
    table
        .as_object()?
        .iter()
        .rev()
        .find_map(|(key, item)| (item == value).then(|| key.clone()))
}
/// Returns the request or the diagnostic for a refused format.
/// Source diagnostics match for owned typed market keys. Inherited JS object keys
/// are deliberately treated as missing markets (declared key-set divergence).
/// `position_idx` is the resolved getOrderPositionIdx result; conditional callers
/// must obtain it with reduceOnly=false, as the source does before this pure phase.
pub fn format_order_for_batch(
    opts: &PlaceOrderOpts,
    market: Option<&Market>,
    ticker: Option<&Ticker>,
    position_idx: f64,
) -> Result<Value, String> {
    format_order_body(opts, market, ticker, position_idx, false)
}

// ts: BybitInverseExchange.formatOrderForBatch — integer USD contract sizing,
// source min/max clamps and the dedicated conditional-order diagnostic.
pub(crate) fn format_inverse_order_for_batch(
    opts: &PlaceOrderOpts,
    market: Option<&Market>,
    ticker: Option<&Ticker>,
    position_idx: f64,
) -> Result<Value, String> {
    format_order_body(opts, market, ticker, position_idx, true)
}

fn format_order_body(
    opts: &PlaceOrderOpts,
    market: Option<&Market>,
    ticker: Option<&Ticker>,
    position_idx: f64,
    inverse_profile: bool,
) -> Result<Value, String> {
    let market = market.ok_or_else(|| format!("Market {} not found", opts.symbol))?;
    let price = opts
        .price
        .filter(|v| truthy(*v))
        .map(|v| adjust(v, market.precision.price));
    let trigger = opts
        .trigger_price
        .filter(|v| truthy(*v))
        .map(|v| adjust(v, market.precision.price));
    let mut direction = None;
    if let Some(trigger) = trigger.filter(|v| truthy(*v)) {
        let ticker = ticker.ok_or_else(|| {
            let field = match opts.r#type {
                OrderType::Conditional => match opts.trigger_type {Some(Trigger::MarkPrice)=>"mark",Some(Trigger::IndexPrice)=>"index",_=>"last"},
                OrderType::StopLoss if opts.sl_trigger_type != Some(Trigger::LastPrice) => "mark",
                OrderType::TakeProfit if opts.tp_trigger_type != Some(Trigger::LastPrice) => "mark",
                _ => "last",
            };
            format!("Failed to format order for batch: Cannot read properties of undefined (reading '{field}')")
        })?;
        direction = Some(
            if trigger > get_bybit_trigger_direction_price(opts, ticker) {
                "1"
            } else {
                "2"
            },
        );
    }
    let kind = match opts.r#type {
        OrderType::Conditional => {
            if !trigger.is_some_and(truthy) {
                return Err(format!(
                    "Failed to format order for batch: Trigger price is required for conditional Bybit{} orders ({})",
                    if inverse_profile { " inverse" } else { "" },
                    opts.symbol
                ));
            }
            if price.is_some_and(truthy) {
                OrderType::Limit
            } else {
                OrderType::Market
            }
        }
        OrderType::StopLoss | OrderType::TakeProfit if trigger.is_some_and(truthy) => {
            OrderType::Market
        }
        kind => kind,
    };
    let mut req = Map::new();
    req.insert("symbol".into(), json!(opts.symbol));
    if let Some(side) = inverse(&crate::types::ORDER_SIDE, &json!(opts.side)) {
        req.insert("side".into(), json!(side));
    }
    if let Some(kind) = inverse(&crate::types::ORDER_TYPE, &json!(kind)) {
        req.insert("orderType".into(), json!(kind));
    }
    let amount = if inverse_profile && market.is_inverse.unwrap_or(false) {
        let mut amount = safe_cex_num::math_round(opts.amount);
        let minimum = if truthy(market.limits.amount.min) {
            market.limits.amount.min
        } else {
            1.
        };
        if amount < minimum {
            amount = minimum;
        }
        let maximum = safe_cex_num::numeric_value_to_number(&market.limits.amount.max);
        if truthy(maximum) && amount > maximum {
            amount = maximum;
        }
        amount.floor()
    } else {
        adjust(opts.amount, market.precision.amount)
    };
    req.insert("qty".into(), json!(js_number_string(amount)));
    let limit = opts.r#type == OrderType::Limit
        || (opts.r#type == OrderType::Conditional && price.is_some_and(truthy));
    if limit {
        req.insert(
            "price".into(),
            json!(price.map(js_number_string).unwrap_or_else(|| "null".into())),
        );
    }
    for (key, value) in [
        ("stopLoss", opts.stop_loss),
        ("takeProfit", opts.take_profit),
    ] {
        if let Some(value) = value.filter(|v| truthy(*v)) {
            req.insert(
                key.into(),
                json!(js_number_string(adjust(value, market.precision.price))),
            );
        }
    }
    req.insert(
        "reduceOnly".into(),
        json!(opts.r#type != OrderType::Conditional && opts.reduce_only.unwrap_or(false)),
    );
    if opts.trigger_price.is_some_and(truthy) {
        req.insert(
            "triggerPrice".into(),
            json!(
                trigger
                    .map(js_number_string)
                    .unwrap_or_else(|| "null".into())
            ),
        );
    }
    if let Some(direction) = direction {
        req.insert("triggerDirection".into(), json!(direction));
    }
    if let Some(by) = get_bybit_trigger_by(opts) {
        req.insert("triggerBy".into(), json!(by));
    }
    if opts.stop_loss.is_some_and(truthy) {
        req.insert(
            "slTriggerBy".into(),
            json!(if opts.sl_trigger_type == Some(Trigger::LastPrice) {
                "LastPrice"
            } else {
                "MarkPrice"
            }),
        );
    }
    if opts.take_profit.is_some_and(truthy) {
        req.insert(
            "tpTriggerBy".into(),
            json!(if opts.tp_trigger_type == Some(Trigger::MarkPrice) {
                "MarkPrice"
            } else {
                "LastPrice"
            }),
        );
    }
    if limit {
        let tif = json!(
            opts.time_in_force
                .unwrap_or(OrderTimeInForce::GoodTillCancel)
        );
        req.insert(
            "timeInForce".into(),
            crate::types::ORDER_TIME_IN_FORCE_OUTBOUND[tif.as_str().unwrap()].clone(),
        );
    }
    req.insert(
        "closeOnTrigger".into(),
        json!(matches!(
            opts.r#type,
            OrderType::StopLoss | OrderType::TakeProfit
        )),
    );
    req.insert("positionIdx".into(), json!(position_idx));
    if let Some(id) = &opts.client_order_id {
        req.insert("orderLinkId".into(), json!(id));
    }
    Ok(Value::Object(req))
}
