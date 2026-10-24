//! FIFO commands; all placement reads and validation precede the first write.
use crate::{
    Pending, Woo, native,
    wire::{Envelope, id, number},
};
use safe_cex_engine::model::*;
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Method, Outcome, Response, TimerId, VenueError, num,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone)]
enum Accepted {
    Regular(Option<OrderIdentityReceipt>),
    Algo,
    Ids(Vec<String>),
    CancelAll,
    Leverage {
        symbol: String,
        leverage: f64,
        revision: u64,
    },
    Mode {
        hedged: bool,
        revision: u64,
    },
}
#[derive(Clone)]
enum Step {
    Touch {
        symbol: String,
        side: OrderSide,
    },
    Build(Vec<PlaceOrderOpts>),
    Write {
        method: Method,
        path: String,
        params: Vec<(String, String)>,
        body: Option<String>,
        accepted: Accepted,
    },
    Leverage {
        symbol: String,
        expected: Option<f64>,
        revision: u64,
        attempt: u8,
    },
    Mode {
        hedged: bool,
        revision: u64,
        attempt: u8,
    },
}
pub struct Request {
    command: CommandId,
    step: Step,
}
struct Job {
    command: CommandId,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    touches: HashMap<(String, OrderSide), f64>,
    errors: Vec<String>,
    /// A failed cancel family's outcome is not known.
    errors_unknown: bool,
    /// A write took effect, so a later failure cannot mean nothing happened.
    effective: bool,
}
#[derive(Default)]
pub struct Trading {
    queue: VecDeque<Job>,
    active: Option<Job>,
    waiting: bool,
    timer: Option<TimerId>,
    next_write: f64,
    next_algo: f64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Algo<'a> {
    symbol: &'a str,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    kind: Option<&'static str>,
    algo_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quantity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_price_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    callback_value: Option<String>,
    reduce_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    position_side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    child_orders: Option<Vec<AlgoChild>>,
    broker_id: &'static str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AlgoChild {
    algo_type: &'static str,
    #[serde(rename = "type")]
    kind: &'static str,
    trigger_price_type: &'static str,
    side: &'static str,
    trigger_price: String,
    reduce_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    position_side: Option<&'static str>,
}
fn positive(n: f64) -> bool {
    n.is_finite() && n > 0.0
}
fn side(side: OrderSide) -> &'static str {
    if side == OrderSide::Buy {
        "BUY"
    } else {
        "SELL"
    }
}
fn algo(kind: OrderType) -> bool {
    matches!(
        kind,
        OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
    )
}
fn trigger(value: Option<StopTriggerType>) -> Result<&'static str, String> {
    match value.unwrap_or(StopTriggerType::MarkPrice) {
        StopTriggerType::MarkPrice => Ok("MARK_PRICE"),
        StopTriggerType::LastPrice => Ok("MARKET_PRICE"),
        StopTriggerType::IndexPrice => Err("WOO does not support index-price triggers".into()),
    }
}
fn native_id(id: &str) -> bool {
    !id.is_empty() && !id.starts_with('0') && id.bytes().all(|c| c.is_ascii_digit())
}
fn canonical_id(id: &str) -> bool {
    native_id(id) && id.parse::<u64>().is_ok_and(|n| n <= 9_007_199_254_740_991)
}

fn position_side(hedged: bool, side: OrderSide, reduce: bool) -> Option<&'static str> {
    hedged.then_some(if (side == OrderSide::Buy) ^ reduce {
        "LONG"
    } else {
        "SHORT"
    })
}
fn write(
    method: Method,
    path: impl Into<String>,
    params: Vec<(String, String)>,
    body: Option<String>,
    accepted: Accepted,
) -> Step {
    Step::Write {
        method,
        path: path.into(),
        params,
        body,
        accepted,
    }
}
fn json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| e.to_string())
}
fn quantities(amount: f64, market: &Market) -> Result<Vec<f64>, String> {
    let step = market.precision.amount;
    if !positive(amount) || !positive(step) || !positive(market.limits.amount.max) {
        return Err("Invalid WOO quantity or lot limits".into());
    }
    let ticks = (amount / step).round();
    let max = (market.limits.amount.max / step).floor();
    let min = (market.limits.amount.min / step).ceil().max(1.0);
    if ticks > 9_007_199_254_740_991.0 || ticks < min || max < min {
        return Err("WOO quantity is outside lot limits".into());
    }
    let count = (ticks / max).ceil() as usize;
    if count == 0 || count > 1000 {
        return Err("WOO order requires too many native lots".into());
    }
    let base = (ticks / count as f64).floor();
    if base < min {
        return Err("WOO split leaves a below-minimum lot".into());
    }
    let remainder = (ticks - base * count as f64) as usize;
    Ok((0..count)
        .map(|i| num::adjust((base + if i < remainder { 1.0 } else { 0.0 }) * step, step))
        .collect())
}
fn fair(t: &Ticker) -> Option<f64> {
    [Some(t.mark), Some(t.index), t.mid_price, Some(t.last)]
        .into_iter()
        .flatten()
        .find(|v| positive(*v))
}
fn crosses(side: OrderSide, price: f64, touch: f64) -> bool {
    if side == OrderSide::Buy {
        price >= touch
    } else {
        price <= touch
    }
}
fn market_price(
    t: &Ticker,
    side: OrderSide,
    slippage: f64,
    range: f64,
    touch: Option<f64>,
) -> Option<f64> {
    let quote = touch.or_else(|| {
        [if side == OrderSide::Buy { t.ask } else { t.bid }]
            .into_iter()
            .find(|v| positive(*v))
    });
    let direction = if side == OrderSide::Buy { 1.0 } else { -1.0 };
    let factor = slippage.clamp(0.0, range.min(100.0)) / 100.0;
    if let Some(fair) = fair(t) {
        let price = fair * (1.0 + direction * factor);
        if let Some(quote) = quote {
            let within = if side == OrderSide::Buy {
                quote <= fair * (1.0 + range / 100.0)
            } else {
                quote >= fair * (1.0 - range / 100.0)
            };
            if within && !crosses(side, price, quote) {
                return Some(quote);
            }
        }
        return Some(price);
    }
    quote
        .or_else(|| {
            [if side == OrderSide::Buy { t.bid } else { t.ask }]
                .into_iter()
                .find(|v| positive(*v))
        })
        .map(|v| v * (1.0 + direction * factor))
}
impl Woo {
    fn broker(&self) -> &'static str {
        if self.options.testnet {
            "e708a644-9ce5-46c6-b50c-74b15f62d8ca"
        } else {
            "ab82cb09-cfec-4473-80a3-b740779d0644"
        }
    }
    fn validate_places(
        &self,
        cx: &Cx<'_, Pending>,
        orders: &[PlaceOrderOpts],
    ) -> Result<(), String> {
        let mut ids = HashSet::new();
        for o in orders {
            if o.bybit_bbo.is_some() || o.bybit_slippage_tolerance_ticks.is_some() {
                return Err("Bybit BBO and TickSize controls are not supported by WOO".into());
            }
            let m = cx
                .store
                .market(&o.symbol)
                .ok_or_else(|| format!("Market {} not found", o.symbol))?;
            if o.kind == OrderType::Conditional {
                return Err(
                    "WOO opening conditional orders are not implemented by the TS venue".into(),
                );
            }
            if !o.amount.is_finite() || o.amount < 0.0 {
                return Err("Invalid WOO quantity".into());
            }
            if o.price.is_some_and(|price| {
                !positive(price) || !positive(num::adjust(price, m.precision.price))
            }) {
                return Err("Invalid WOO price".into());
            }
            if o.max_slippage_pct
                .is_some_and(|v| !v.is_finite() || v < 0.0)
            {
                return Err("Invalid WOO slippage".into());
            }
            for value in [o.stop_loss, o.take_profit].into_iter().flatten() {
                if !value.is_finite()
                    || value < 0.0
                    || value > 0.0 && !positive(num::adjust(value, m.precision.price))
                {
                    return Err("Invalid WOO bracket price".into());
                }
            }
            trigger(o.sl_trigger_type.or(o.trigger_type))?;
            trigger(o.tp_trigger_type.or(o.trigger_type))?;
            if o.kind != OrderType::Market && o.price.is_none() {
                return Err("Price is required for WOO orders".into());
            }
            let lots = if !algo(o.kind) || o.amount > 0.0 && o.kind != OrderType::TrailingStopLoss {
                quantities(o.amount, m)?.len()
            } else {
                1
            };
            if let Some(id) = &o.client_order_id {
                if !canonical_id(id) {
                    return Err(
                        "WOO clientOrderId must be a canonical positive decimal integer within the JavaScript safe-integer range".into(),
                    );
                }
                if algo(o.kind)
                    || lots != 1
                    || o.stop_loss.is_some_and(|v| v > 0.0)
                    || o.take_profit.is_some_and(|v| v > 0.0)
                {
                    return Err(
                        "WOO client order ID requires exactly one plain native order".into(),
                    );
                }
                if !ids.insert(id) {
                    return Err("Duplicate WOO client order ID in batch".into());
                }
            }
            if o.kind == OrderType::Market
                || o.kind == OrderType::TrailingStopLoss
                || o.time_in_force == Some(TimeInForce::PostOnly)
            {
                let t = cx
                    .store
                    .ticker(&o.symbol)
                    .ok_or_else(|| format!("Ticker {} not found", o.symbol))?;
                if o.kind == OrderType::TrailingStopLoss {
                    let pside = if o.side == OrderSide::Buy {
                        PositionSide::Short
                    } else {
                        PositionSide::Long
                    };
                    if !cx
                        .store
                        .positions()
                        .iter()
                        .any(|p| p.symbol == o.symbol && p.side == pside && positive(p.contracts))
                    {
                        return Err(
                            "WOO trailing stop requires an open closing-side position".into()
                        );
                    }
                    if !positive(t.last)
                        || !positive(num::adjust(
                            (t.last - o.price.unwrap()).abs(),
                            m.precision.price,
                        ))
                    {
                        return Err("WOO trailing callback distance must be positive".into());
                    }
                }
                if o.time_in_force == Some(TimeInForce::PostOnly)
                    && o.kind == OrderType::Limit
                    && fair(t).is_some_and(|v| {
                        crosses(o.side, num::adjust(o.price.unwrap(), m.precision.price), v)
                    })
                {
                    return Err("WOO post-only order would cross current fair price".into());
                }
            }
        }
        Ok(())
    }
    fn positional(
        &self,
        cx: &Cx<'_, Pending>,
        o: &PlaceOrderOpts,
        sl: Option<f64>,
        tp: Option<f64>,
        sl_trigger: Option<StopTriggerType>,
        tp_trigger: Option<StopTriggerType>,
    ) -> Result<Step, String> {
        let m = cx.store.market(&o.symbol).unwrap();
        let symbol = native(m);
        let pside = position_side(cx.store.hedged(), o.side, true);
        let mut children = Vec::new();
        for (kind, price, source) in [
            ("STOP_LOSS", sl, sl_trigger),
            ("TAKE_PROFIT", tp, tp_trigger),
        ] {
            if let Some(price) = price.filter(|v| *v > 0.0) {
                children.push(AlgoChild {
                    algo_type: kind,
                    kind: "CLOSE_POSITION",
                    trigger_price_type: trigger(source)?,
                    side: side(o.side),
                    trigger_price: num::format(num::adjust(price, m.precision.price)),
                    reduce_only: true,
                    position_side: pside,
                });
            }
        }
        if children.is_empty() {
            return Err("WOO positional protection requires a trigger".into());
        }
        let body = json(&Algo {
            symbol: &symbol,
            kind: None,
            algo_type: "POSITIONAL_TP_SL",
            side: None,
            quantity: None,
            trigger_price: None,
            trigger_price_type: None,
            callback_value: None,
            reduce_only: true,
            position_side: pside,
            child_orders: Some(children),
            broker_id: self.broker(),
        })?;
        Ok(write(
            Method::Post,
            "/v3/algo/order",
            vec![],
            Some(body),
            Accepted::Algo,
        ))
    }
    fn build_places(
        &self,
        cx: &Cx<'_, Pending>,
        orders: Vec<PlaceOrderOpts>,
        touches: &HashMap<(String, OrderSide), f64>,
    ) -> Result<VecDeque<Step>, String> {
        self.validate_places(cx, &orders)?;
        let mut steps = VecDeque::new();
        for o in orders {
            let m = cx.store.market(&o.symbol).unwrap();
            let symbol = native(m);
            let pside = position_side(cx.store.hedged(), o.side, o.reduce_only.unwrap_or(false));
            if !algo(o.kind) {
                let mut price = o.price.map(|p| num::adjust(p, m.precision.price));
                let kind = if o.kind == OrderType::Market {
                    let t = cx.store.ticker(&o.symbol).unwrap();
                    let range = self.price_range(&o.symbol).unwrap_or(0.01) * 100.0;
                    price = market_price(
                        t,
                        o.side,
                        o.max_slippage_pct.unwrap_or(0.9),
                        range,
                        touches.get(&(o.symbol.clone(), o.side)).copied(),
                    )
                    .map(|p| num::adjust(p, m.precision.price));
                    if price.is_none_or(|p| !positive(p)) {
                        return Err("WOO market IOC has no valid reference price".into());
                    }
                    "IOC"
                } else if o.time_in_force == Some(TimeInForce::PostOnly) {
                    if touches
                        .get(&(o.symbol.clone(), o.side))
                        .is_some_and(|p| crosses(o.side, price.unwrap(), *p))
                    {
                        return Err("WOO post-only order would cross current touch".into());
                    }
                    "POST_ONLY"
                } else {
                    "LIMIT"
                };
                for quantity in quantities(o.amount, m)? {
                    let mut params = vec![
                        ("symbol".into(), symbol.clone()),
                        ("order_type".into(), kind.into()),
                        ("side".into(), side(o.side).into()),
                        ("order_quantity".into(), num::format(quantity)),
                        ("broker_id".into(), self.broker().into()),
                    ];
                    if let Some(p) = price {
                        params.push(("order_price".into(), num::format(p)));
                    }
                    if let Some(v) = o.reduce_only {
                        params.push(("reduce_only".into(), v.to_string()));
                    }
                    if let Some(v) = pside {
                        params.push(("position_side".into(), v.into()));
                    }
                    if let Some(v) = &o.client_order_id {
                        params.push(("client_order_id".into(), v.clone()));
                    }
                    steps.push_back(write(
                        Method::Post,
                        "/v1/order",
                        params,
                        None,
                        Accepted::Regular(o.client_order_id.as_ref().map(|identity| {
                            OrderIdentityReceipt {
                                symbol: o.symbol.clone(),
                                kind: OrderIdentityKind::ClientOrderId,
                                identity: identity.clone(),
                            }
                        })),
                    ));
                }
                if o.stop_loss.is_some_and(|v| v > 0.0) || o.take_profit.is_some_and(|v| v > 0.0) {
                    let mut closing = o.clone();
                    closing.side = o.side.opposite();
                    steps.push_back(self.positional(
                        cx,
                        &closing,
                        o.stop_loss,
                        o.take_profit,
                        o.sl_trigger_type.or(o.trigger_type),
                        o.tp_trigger_type.or(o.trigger_type),
                    )?);
                }
            } else if o.kind == OrderType::TrailingStopLoss {
                let t = cx.store.ticker(&o.symbol).unwrap();
                let closing = if o.side == OrderSide::Buy {
                    PositionSide::Short
                } else {
                    PositionSide::Long
                };
                let p = cx
                    .store
                    .positions()
                    .iter()
                    .find(|p| p.symbol == o.symbol && p.side == closing && positive(p.contracts))
                    .unwrap();
                let body = json(&Algo {
                    symbol: &symbol,
                    kind: Some("MARKET"),
                    algo_type: "TRAILING_STOP",
                    side: Some(side(o.side)),
                    quantity: Some(num::format(p.contracts)),
                    trigger_price: None,
                    trigger_price_type: Some(trigger(
                        o.sl_trigger_type.or(o.tp_trigger_type).or(o.trigger_type),
                    )?),
                    callback_value: Some(num::format(num::adjust(
                        (t.last - o.price.unwrap()).abs(),
                        m.precision.price,
                    ))),
                    reduce_only: true,
                    position_side: position_side(cx.store.hedged(), o.side, true),
                    child_orders: None,
                    broker_id: self.broker(),
                })?;
                steps.push_back(write(
                    Method::Post,
                    "/v3/algo/order",
                    vec![],
                    Some(body),
                    Accepted::Algo,
                ));
            } else if o.amount > 0.0 {
                for quantity in quantities(o.amount, m)? {
                    let body = json(&Algo {
                        symbol: &symbol,
                        kind: Some("MARKET"),
                        algo_type: "STOP",
                        side: Some(side(o.side)),
                        quantity: Some(num::format(quantity)),
                        trigger_price: Some(num::format(num::adjust(
                            o.price.unwrap(),
                            m.precision.price,
                        ))),
                        trigger_price_type: Some(trigger(if o.kind == OrderType::StopLoss {
                            o.sl_trigger_type.or(o.trigger_type)
                        } else {
                            o.tp_trigger_type.or(o.trigger_type)
                        })?),
                        callback_value: None,
                        reduce_only: true,
                        position_side: position_side(cx.store.hedged(), o.side, true),
                        child_orders: None,
                        broker_id: self.broker(),
                    })?;
                    steps.push_back(write(
                        Method::Post,
                        "/v3/algo/order",
                        vec![],
                        Some(body),
                        Accepted::Algo,
                    ));
                }
            } else {
                let mut sl = if o.kind == OrderType::StopLoss {
                    o.price
                } else {
                    None
                };
                let mut tp = if o.kind == OrderType::TakeProfit {
                    o.price
                } else {
                    None
                };
                let mut sl_trigger = o.sl_trigger_type.or(o.trigger_type);
                let mut tp_trigger = o.tp_trigger_type.or(o.trigger_type);
                if let Some(sibling) = cx.store.orders().iter().find(|p| {
                    p.symbol == o.symbol
                        && p.side == o.side
                        && p.status == OrderStatus::Open
                        && self.account_algo_positional(&p.id)
                        && matches!(
                            (o.kind, p.kind),
                            (OrderType::StopLoss, OrderType::TakeProfit)
                                | (OrderType::TakeProfit, OrderType::StopLoss)
                        )
                }) {
                    if sibling.kind == OrderType::StopLoss {
                        sl = Some(sibling.price);
                        sl_trigger = sibling
                            .extra
                            .as_ref()
                            .and_then(|e| e.sl_trigger_type)
                            .or(sibling.trigger_type);
                    } else {
                        tp = Some(sibling.price);
                        tp_trigger = sibling
                            .extra
                            .as_ref()
                            .and_then(|e| e.tp_trigger_type)
                            .or(sibling.trigger_type);
                    }
                    steps.push_back(write(
                        Method::Delete,
                        format!(
                            "/v3/algo/order/{}",
                            sibling.parent_id.as_ref().unwrap_or(&sibling.id)
                        ),
                        vec![],
                        None,
                        Accepted::Ids(vec![]),
                    ));
                }
                steps.push_back(self.positional(cx, &o, sl, tp, sl_trigger, tp_trigger)?);
            }
        }
        Ok(steps)
    }
    fn plan_amend(
        &self,
        cx: &Cx<'_, Pending>,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<VecDeque<Step>, String> {
        if price.is_none() && amount.is_none()
            || price.is_some_and(|v| !positive(v))
            || amount.is_some_and(|v| !positive(v))
        {
            return Err("WOO amendment requires a positive price or amount".into());
        }
        let order = cx.store.order(id).ok_or("WOO order not found")?;
        if order.status != OrderStatus::Open {
            return Err("WOO amendment requires an open order".into());
        }
        let m = cx
            .store
            .market(&order.symbol)
            .ok_or("WOO market not found")?;
        if price.is_some_and(|v| !positive(num::adjust(v, m.precision.price))) {
            return Err("WOO amendment price rounds to zero".into());
        }
        if let Some(value) = amount
            && (quantities(value, m)?.len() != 1
                || num::adjust(value, m.precision.amount) < order.filled)
        {
            return Err("WOO amendment quantity is outside native lot limits".into());
        }
        if order.kind == OrderType::TrailingStopLoss {
            let opts = PlaceOrderOpts {
                symbol: order.symbol.clone(),
                kind: order.kind,
                side: order.side,
                amount: amount.unwrap_or(order.amount),
                price: Some(price.unwrap_or(order.price)),
                trigger_price: None,
                stop_loss: None,
                take_profit: None,
                reduce_only: Some(true),
                time_in_force: None,
                trigger_type: order.trigger_type,
                tp_trigger_type: None,
                sl_trigger_type: order.extra.as_ref().and_then(|e| e.sl_trigger_type),
                client_order_id: None,
                close: None,
                max_slippage_pct: None,
                stp_mode: None,
                bybit_bbo: None,
                bybit_slippage_tolerance_ticks: None,
                implied_volatility: None,
            };
            self.validate_places(cx, std::slice::from_ref(&opts))?;
            let mut steps = VecDeque::from([write(
                Method::Delete,
                format!(
                    "/v3/algo/order/{}",
                    order.parent_id.as_deref().unwrap_or(id)
                ),
                vec![],
                None,
                Accepted::Ids(vec![]),
            )]);
            steps.push_back(Step::Build(vec![opts]));
            return Ok(steps);
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Amendment<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            order_id: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            price: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            quantity: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            trigger_price: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            child_orders: Option<Vec<ChildEdit<'a>>>,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ChildEdit<'a> {
            algo_order_id: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            trigger_price: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            quantity: Option<String>,
        }
        let mut body = Amendment {
            order_id: None,
            price: None,
            quantity: None,
            trigger_price: None,
            child_orders: None,
        };
        let path = if !algo(order.kind) {
            let quantity = amount.unwrap_or(order.amount);
            if quantity < order.filled || quantities(quantity, m)?.len() != 1 {
                return Err("Invalid WOO amendment quantity".into());
            }
            body.order_id = Some(id);
            body.quantity = Some(num::format(num::adjust(quantity, m.precision.amount)));
            body.price = price.map(|v| num::format(num::adjust(v, m.precision.price)));
            format!("/v3/order/{id}")
        } else {
            let p = price.map(|v| num::format(num::adjust(v, m.precision.price)));
            let quantity = amount.map(|v| num::format(num::adjust(v, m.precision.amount)));
            if self.account_algo_positional(id) {
                body.child_orders = Some(vec![ChildEdit {
                    algo_order_id: id,
                    trigger_price: p,
                    quantity,
                }]);
            } else {
                body.trigger_price = p;
                body.quantity = quantity;
            }
            format!(
                "/v3/algo/order/{}",
                order.parent_id.as_deref().unwrap_or(id)
            )
        };
        Ok(VecDeque::from([write(
            Method::Put,
            path,
            vec![],
            Some(json(&body)?),
            Accepted::Ids(vec![id.into()]),
        )]))
    }
    fn plan_cancel(
        &self,
        cx: &Cx<'_, Pending>,
        ids: Vec<String>,
    ) -> Result<VecDeque<Step>, String> {
        let mut steps = VecDeque::new();
        let mut seen = HashSet::new();
        for id in ids {
            let order = cx
                .store
                .order(&id)
                .ok_or_else(|| format!("WOO order {id} not found"))?;
            if algo(order.kind) {
                let parent = order.parent_id.as_ref().unwrap_or(&id);
                if seen.insert(format!("a{parent}")) {
                    steps.push_back(write(
                        Method::Delete,
                        format!("/v3/algo/order/{parent}"),
                        vec![],
                        None,
                        Accepted::Ids(vec![]),
                    ));
                }
            } else if seen.insert(format!("r{id}")) {
                let m = cx
                    .store
                    .market(&order.symbol)
                    .ok_or("WOO market not found")?;
                steps.push_back(write(
                    Method::Delete,
                    "/v1/order",
                    vec![("order_id".into(), id), ("symbol".into(), native(m))],
                    None,
                    Accepted::Ids(vec![]),
                ));
            }
        }
        Ok(steps)
    }
    pub(crate) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let (command, planned) = match command {
            Command::PlaceOrders { command, orders } => {
                let planned = self.validate_places(cx, &orders).map(|()| {
                    let mut steps = VecDeque::new();
                    let mut seen = HashSet::new();
                    for o in &orders {
                        if (o.kind == OrderType::Market
                            || o.kind == OrderType::Limit
                                && o.time_in_force == Some(TimeInForce::PostOnly))
                            && seen.insert((o.symbol.clone(), o.side))
                        {
                            steps.push_back(Step::Touch {
                                symbol: o.symbol.clone(),
                                side: o.side,
                            });
                        }
                    }
                    steps.push_back(Step::Build(orders));
                    steps
                });
                (command, planned)
            }
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => (command, self.plan_amend(cx, &id, price, amount)),
            Command::CancelOrders { command, ids } => (command, self.plan_cancel(cx, ids)),
            Command::CancelSymbolOrders { command, symbol } => {
                let planned = if symbol.is_empty() {
                    Ok(VecDeque::from([
                        write(
                            Method::Delete,
                            "/v3/orders/pending",
                            vec![],
                            None,
                            Accepted::CancelAll,
                        ),
                        write(
                            Method::Delete,
                            "/v3/algo/orders/pending",
                            vec![],
                            None,
                            Accepted::CancelAll,
                        ),
                    ]))
                } else {
                    cx.store
                        .market(&symbol)
                        .ok_or_else(|| "WOO market not found".to_string())
                        .map(|m| {
                            VecDeque::from([write(
                                Method::Delete,
                                format!("/v3/merge/orders/pending/{}", native(m)),
                                vec![],
                                None,
                                Accepted::Ids(vec![]),
                            )])
                        })
                };
                (command, planned)
            }
            Command::SetLeverage {
                command,
                symbol,
                leverage,
                side: _,
            } => {
                let planned = cx
                    .store
                    .market(&symbol)
                    .ok_or_else(|| "WOO market not found".to_string())
                    .and_then(|m| {
                        if !positive(leverage)
                            || leverage.fract() != 0.0
                            || leverage < m.limits.leverage.min
                            || leverage > m.limits.leverage.max
                        {
                            return Err("WOO leverage is outside market limits".into());
                        }
                        let revision = self.account_revision(&symbol);
                        Ok(VecDeque::from([write(
                            Method::Post,
                            "/v1/client/futures_leverage",
                            vec![
                                ("symbol".into(), native(m)),
                                ("margin_mode".into(), "CROSS".into()),
                                ("position_side".into(), "BOTH".into()),
                                ("leverage".into(), num::format(leverage)),
                            ],
                            None,
                            Accepted::Leverage {
                                symbol: symbol.clone(),
                                leverage,
                                revision,
                            },
                        )]))
                    });
                (command, planned)
            }
            Command::FetchLeverage { command, symbol } => {
                let planned = if cx.store.market(&symbol).is_some() {
                    Ok(VecDeque::from([Step::Leverage {
                        revision: self.account_revision(&symbol),
                        symbol,
                        expected: None,
                        attempt: 0,
                    }]))
                } else {
                    Err("WOO market not found".into())
                };
                (command, planned)
            }
            Command::SetPositionMode {
                command,
                hedged,
                symbol: _,
            } => {
                let revision = self.account_revision("$mode");
                (
                    command,
                    Ok(VecDeque::from([write(
                        Method::Post,
                        "/v1/client/position_mode",
                        vec![(
                            "position_mode".into(),
                            if hedged { "HEDGE_MODE" } else { "ONE_WAY" }.into(),
                        )],
                        None,
                        Accepted::Mode { hedged, revision },
                    )])),
                )
            }
            Command::SetMarginType {
                command,
                symbol,
                isolated,
            } => {
                let planned = if isolated {
                    Err("WOO isolated margin mutation is not implemented by the TS venue".into())
                } else if cx.store.market(&symbol).is_none() {
                    Err("WOO market not found".into())
                } else {
                    Ok(VecDeque::new())
                };
                (command, planned)
            }
            _ => return,
        };
        if self.options.public_data_only
            || self.options.key.is_none()
            || self.options.secret.is_none()
        {
            cx.fail(
                command,
                not_sent("WOO trading requires credentials").kind(ErrorKind::AuthInvalid),
            );
            return;
        }
        match planned {
            Ok(steps) => {
                self.trading.queue.push_back(Job {
                    command,
                    steps,
                    ids: Vec::new(),
                    touches: HashMap::new(),
                    errors: Vec::new(),
                    errors_unknown: false,
                    effective: false,
                });
                self.trade_pump(cx);
            }
            Err(error) => cx.fail(command, not_sent(error)),
        }
    }
    fn trade_pump(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading.waiting || self.trading.timer.is_some() {
            return;
        }
        loop {
            if self.trading.active.is_none() {
                self.trading.active = self.trading.queue.pop_front();
            }
            let Some(job) = &mut self.trading.active else {
                return;
            };
            let Some(step) = job.steps.pop_front() else {
                let job = self.trading.active.take().unwrap();
                if job.errors.is_empty() {
                    cx.done(job.command, json(&job.ids).unwrap());
                } else {
                    // The cancel families are independent; only an unknown one is worth stating.
                    let error = VenueError::new(job.errors.join("; "));
                    let error = if job.errors_unknown {
                        error.outcome(Outcome::Unknown)
                    } else {
                        error
                    };
                    cx.fail(job.command, error);
                }
                continue;
            };
            let command = job.command;
            match step {
                Step::Build(orders) => {
                    let touches =
                        std::mem::take(&mut self.trading.active.as_mut().unwrap().touches);
                    match self.build_places(cx, orders, &touches) {
                        Ok(steps) => self.trading.active.as_mut().unwrap().steps.extend(steps),
                        Err(error) => {
                            self.trade_abort(cx, not_sent(error));
                        }
                    }
                    continue;
                }
                Step::Touch { ref symbol, .. } => {
                    let Some(m) = cx.store.market(symbol) else {
                        self.trade_abort(cx, not_sent("WOO market disappeared"));
                        continue;
                    };
                    cx.request(
                        Method::Get,
                        format!("{}/v1/public/orderbook/{}", self.rest, native(m)),
                        vec![],
                        None,
                        Pending::Trade(Request { command, step }),
                    );
                }
                Step::Write {
                    method,
                    ref path,
                    ref params,
                    ref body,
                    ..
                } => {
                    let is_algo = method == Method::Post && path == "/v3/algo/order";
                    let ready = self.trading.next_write.max(if is_algo {
                        self.trading.next_algo
                    } else {
                        0.0
                    });
                    if cx.now < ready {
                        self.trading.active.as_mut().unwrap().steps.push_front(step);
                        self.trading.timer = Some(cx.set_timer(ready - cx.now));
                        return;
                    }
                    let mut outbound = step.clone();
                    if let Step::Write { accepted, .. } = &mut outbound {
                        match accepted {
                            Accepted::Leverage {
                                symbol, revision, ..
                            } => *revision = self.account_config_write_started(symbol),
                            Accepted::Mode { revision, .. } => {
                                *revision = self.account_config_write_started("$mode")
                            }
                            _ => (),
                        }
                    }
                    if let Step::Write {
                        accepted: Accepted::Regular(Some(receipt)),
                        ..
                    } = &outbound
                    {
                        cx.command_progress(command, vec![receipt.clone()]);
                    }
                    let result = self.signed(
                        cx,
                        method,
                        path,
                        params.clone(),
                        body.clone(),
                        Pending::Trade(Request {
                            command,
                            step: outbound.clone(),
                        }),
                    );
                    if let Err(error) = result {
                        self.trade_write_failed(cx, &outbound, not_sent(error));
                        continue;
                    }
                    self.trading.next_write =
                        cx.now + if method == Method::Put { 200.0 } else { 100.0 };
                    if is_algo {
                        self.trading.next_algo = cx.now + 500.0;
                    }
                }
                Step::Leverage { ref symbol, .. } => {
                    let Some(m) = cx.store.market(symbol) else {
                        self.trade_abort(cx, not_sent("WOO market disappeared"));
                        continue;
                    };
                    let result = self.signed(
                        cx,
                        Method::Get,
                        "/v1/client/futures_leverage",
                        vec![
                            ("symbol".into(), native(m)),
                            ("margin_mode".into(), "CROSS".into()),
                            ("position_mode".into(), "ONE_WAY".into()),
                        ],
                        None,
                        Pending::Trade(Request {
                            command,
                            step: step.clone(),
                        }),
                    );
                    if let Err(error) = result {
                        self.trade_abort(cx, not_sent(error));
                        continue;
                    }
                }
                Step::Mode { .. } => {
                    let result = self.signed(
                        cx,
                        Method::Get,
                        "/v3/accountinfo",
                        vec![],
                        None,
                        Pending::Trade(Request {
                            command,
                            step: step.clone(),
                        }),
                    );
                    if let Err(error) = result {
                        self.trade_abort(cx, not_sent(error));
                        continue;
                    }
                }
            }
            self.trading.waiting = true;
            return;
        }
    }
    fn trade_abort(&mut self, cx: &mut Cx<'_, Pending>, mut error: VenueError) {
        if let Some(job) = self.trading.active.take() {
            if !job.ids.is_empty() {
                error.message = format!(
                    "{}; accepted native IDs: {}",
                    error.message,
                    json(&job.ids).unwrap()
                );
            }
            if (job.effective || !job.ids.is_empty())
                && matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent))
            {
                error.outcome = None;
            }
            cx.fail(job.command, error);
        }
        self.trading.waiting = false;
    }
    fn trade_write_failed(&mut self, cx: &mut Cx<'_, Pending>, step: &Step, error: VenueError) {
        if let Step::Write {
            accepted: Accepted::CancelAll,
            path,
            ..
        } = step
        {
            // The two cancel families are independent; neither ambiguous write is retried.
            let job = self.trading.active.as_mut().unwrap();
            job.errors_unknown |= error.outcome == Some(Outcome::Unknown);
            job.errors.push(format!("{path}: {}", error.message));
        } else {
            self.trade_config_finished(step);
            self.trade_abort(cx, error);
        }
    }
    /// Retries a config read, including confirmation after an accepted leverage write.
    fn trade_retry_read(&mut self, cx: &mut Cx<'_, Pending>, step: Step, error: &str) {
        let original = step.clone();
        let retry = match step {
            Step::Leverage {
                symbol,
                expected,
                revision,
                attempt,
            } if attempt < 2 => Some(Step::Leverage {
                symbol,
                expected,
                revision,
                attempt: attempt + 1,
            }),
            Step::Mode {
                hedged,
                revision,
                attempt,
            } if attempt < 2 => Some(Step::Mode {
                hedged,
                revision,
                attempt: attempt + 1,
            }),
            _ => None,
        };
        if let Some(step) = retry {
            self.trading.active.as_mut().unwrap().steps.push_front(step);
            self.trading.timer = Some(cx.set_timer(250.0));
        } else {
            self.trade_config_finished(&original);
            let error = if matches!(
                original,
                Step::Leverage {
                    expected: Some(_),
                    ..
                }
            ) {
                VenueError::new(format!(
                    "WOO leverage write was accepted, but readback remains unconfirmed: {error}"
                ))
                .outcome(Outcome::Unknown)
            } else {
                not_sent(error)
            };
            self.trade_abort(cx, error);
            self.trade_pump(cx);
        }
    }
    fn trade_config_finished(&mut self, step: &Step) {
        match step {
            Step::Write {
                accepted:
                    Accepted::Leverage {
                        symbol, revision, ..
                    },
                ..
            }
            | Step::Leverage {
                symbol, revision, ..
            } => self.account_config_write_finished(symbol, *revision),
            Step::Write {
                accepted: Accepted::Mode { revision, .. },
                ..
            }
            | Step::Mode { revision, .. } => self.account_config_write_finished("$mode", *revision),
            _ => (),
        }
    }
    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if !self
            .trading
            .active
            .as_ref()
            .is_some_and(|j| j.command == request.command)
        {
            return;
        }
        self.trading.waiting = false;
        if let Step::Touch { ref symbol, side } = request.step {
            #[derive(Deserialize)]
            struct Touch {
                #[serde(default)]
                asks: Vec<TouchLevel>,
                #[serde(default)]
                bids: Vec<TouchLevel>,
            }
            #[derive(Deserialize)]
            struct TouchLevel {
                #[serde(deserialize_with = "number")]
                price: f64,
            }
            if response.status < 400
                && let Ok(t) = serde_json::from_str::<Touch>(response.body)
                && let Some(p) = if side == OrderSide::Buy {
                    t.asks.first()
                } else {
                    t.bids.first()
                }
                && positive(p.price)
            {
                self.trading
                    .active
                    .as_mut()
                    .unwrap()
                    .touches
                    .insert((symbol.clone(), side), p.price);
            }
            self.trade_pump(cx);
            return;
        }
        let checked = Envelope::checked(response.body).and_then(|e| {
            if response.status >= 400 {
                Err(format!("WOO HTTP {}: {}", response.status, e.message))
            } else {
                Ok(e)
            }
        });
        let envelope = match checked {
            Ok(e) => e,
            Err(error) => {
                if let Step::Write {
                    accepted: Accepted::Mode { hedged, revision },
                    ..
                } = request.step
                    && error
                        .to_lowercase()
                        .contains("position mode remains unchanged")
                {
                    self.trading
                        .active
                        .as_mut()
                        .unwrap()
                        .steps
                        .push_front(Step::Mode {
                            hedged,
                            revision,
                            attempt: 0,
                        });
                    self.trade_pump(cx);
                    return;
                }
                if matches!(request.step, Step::Mode { .. } | Step::Leverage { .. }) {
                    self.trade_retry_read(cx, request.step, &error);
                } else {
                    let error = answer_failure(&response, error);
                    self.trade_write_failed(cx, &request.step, error);
                    self.trade_pump(cx);
                }
                return;
            }
        };
        let outcome: Result<(), String> = (|| {
            match &request.step {
                Step::Write { accepted, .. } => {
                    #[derive(Deserialize)]
                    struct Ack {
                        success: Option<bool>,
                    }
                    if let Some(data) = envelope.data
                        && serde_json::from_str::<Ack>(data.get())
                            .is_ok_and(|a| a.success == Some(false))
                    {
                        return Err("WOO mutation was not accepted".into());
                    }
                    match accepted {
                        Accepted::Regular(_) => {
                            #[derive(Deserialize)]
                            struct Placed {
                                #[serde(deserialize_with = "id")]
                                order_id: String,
                            }
                            let result: Placed =
                                serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                            if !native_id(&result.order_id) {
                                return Err("WOO placement returned an invalid native ID".into());
                            }
                            self.trading
                                .active
                                .as_mut()
                                .unwrap()
                                .ids
                                .push(result.order_id);
                        }
                        Accepted::Algo => {
                            #[derive(Deserialize)]
                            struct Rows {
                                rows: Vec<Row>,
                            }
                            #[derive(Deserialize)]
                            #[serde(rename_all = "camelCase")]
                            struct Row {
                                #[serde(deserialize_with = "id")]
                                order_id: String,
                            }
                            let rows: Rows = serde_json::from_str(envelope.payload()?)
                                .map_err(|e| e.to_string())?;
                            if rows.rows.is_empty()
                                || rows.rows.iter().any(|r| !native_id(&r.order_id))
                            {
                                return Err(
                                    "WOO algo placement returned no valid native IDs".into()
                                );
                            }
                            self.trading
                                .active
                                .as_mut()
                                .unwrap()
                                .ids
                                .extend(rows.rows.into_iter().map(|r| r.order_id));
                        }
                        Accepted::CancelAll => (),
                        Accepted::Ids(ids) => self
                            .trading
                            .active
                            .as_mut()
                            .unwrap()
                            .ids
                            .extend(ids.iter().cloned()),
                        Accepted::Leverage {
                            symbol,
                            leverage,
                            revision,
                        } => {
                            self.trading
                                .active
                                .as_mut()
                                .unwrap()
                                .steps
                                .push_front(Step::Leverage {
                                    symbol: symbol.clone(),
                                    expected: Some(*leverage),
                                    revision: *revision,
                                    attempt: 0,
                                })
                        }
                        Accepted::Mode { hedged, revision } => self
                            .trading
                            .active
                            .as_mut()
                            .unwrap()
                            .steps
                            .push_front(Step::Mode {
                                hedged: *hedged,
                                revision: *revision,
                                attempt: 0,
                            }),
                    }
                    if !matches!(accepted, Accepted::Leverage { .. } | Accepted::Mode { .. }) {
                        cx.store.invalidate_order_snapshots();
                        self.account_refresh(cx);
                    }
                }
                Step::Leverage {
                    symbol,
                    expected,
                    revision,
                    ..
                } => {
                    #[derive(Deserialize)]
                    struct Leverage {
                        symbol: String,
                        margin_mode: String,
                        position_mode: String,
                        details: Vec<Detail>,
                    }
                    #[derive(Deserialize)]
                    struct Detail {
                        position_side: String,
                        #[serde(deserialize_with = "number")]
                        leverage: f64,
                    }
                    let data: Leverage =
                        serde_json::from_str(envelope.payload()?).map_err(|e| e.to_string())?;
                    let m = cx.store.market(symbol).unwrap();
                    if data.symbol != native(m)
                        || data.margin_mode != "CROSS"
                        || data.position_mode != "ONE_WAY"
                        || data.details.len() != 1
                        || data.details[0].position_side != "BOTH"
                        || !positive(data.details[0].leverage)
                        || data.details[0].leverage.fract() != 0.0
                        || expected.is_some_and(|v| v != data.details[0].leverage)
                    {
                        return Err(
                            "WOO leverage readback did not match the requested scope/value".into(),
                        );
                    }
                    self.account_leverage_applied(cx, symbol, data.details[0].leverage, *revision);
                }
                Step::Mode {
                    hedged, revision, ..
                } => {
                    #[derive(Deserialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Account {
                        account_mode: String,
                        position_mode: String,
                    }
                    let data: Account =
                        serde_json::from_str(envelope.payload()?).map_err(|e| e.to_string())?;
                    if data.account_mode != "FUTURES"
                        || data.position_mode != if *hedged { "HEDGE_MODE" } else { "ONE_WAY" }
                    {
                        return Err(
                            "WOO position mode readback did not match the requested mode".into(),
                        );
                    }
                    self.account_mode_applied(cx, *hedged, *revision);
                    self.account_refresh(cx);
                }
                _ => (),
            }
            Ok(())
        })();
        match outcome {
            Ok(()) => {
                if matches!(request.step, Step::Write { .. })
                    && let Some(job) = self.trading.active.as_mut()
                {
                    job.effective = true;
                }
            }
            Err(error) => {
                if matches!(request.step, Step::Mode { .. } | Step::Leverage { .. }) {
                    self.trade_retry_read(cx, request.step, &error);
                    return;
                }
                // An explicit `success: false` ack is a refusal without a code table.
                let error = if error == "WOO mutation was not accepted" {
                    VenueError::new(error)
                } else {
                    VenueError::new(error).outcome(Outcome::Unknown)
                };
                self.trade_write_failed(cx, &request.step, error);
            }
        }
        self.trade_pump(cx);
    }
    pub(crate) fn trade_failed(&mut self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        if !self
            .trading
            .active
            .as_ref()
            .is_some_and(|j| j.command == request.command)
        {
            return;
        }
        self.trading.waiting = false;
        match request.step {
            Step::Touch { .. } => self.trade_pump(cx),
            Step::Mode { .. } | Step::Leverage { .. } => {
                self.trade_retry_read(cx, request.step, error)
            }
            _ => {
                let error = VenueError::new(error)
                    .kind(ErrorKind::Network)
                    .outcome(Outcome::Unknown);
                self.trade_write_failed(cx, &request.step, error);
                self.trade_pump(cx);
            }
        }
    }
    pub(crate) fn trade_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.trading.timer == Some(timer) {
            self.trading.timer = None;
            self.trade_pump(cx);
            true
        } else {
            false
        }
    }
}

/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

/// A refused write answer, classified by HTTP status and WOO's documented
/// error codes. 408/5xx and malformed answers do not prove a refusal.
fn answer_failure(response: &Response<'_>, message: String) -> VenueError {
    #[derive(Deserialize)]
    struct Code {
        code: Option<i64>,
        success: Option<bool>,
    }
    let error = VenueError::new(message);
    let rejected = |error: VenueError, kind| error.kind(kind).outcome(Outcome::Rejected);
    let parsed = serde_json::from_str::<Code>(response.body).ok();
    let code = parsed.as_ref().and_then(|c| c.code);
    let error = match code {
        Some(code) => error.venue_code(code.to_string()),
        None => error,
    };
    match (response.status, code) {
        (408 | 500.., _) => error.outcome(Outcome::Unknown),
        (_, Some(-1001 | -1002)) | (401, _) => rejected(error, ErrorKind::AuthInvalid),
        (_, Some(-1003)) | (429, _) => rejected(error, ErrorKind::RateLimited),
        (403, _) => rejected(error, ErrorKind::AuthPermission),
        (_, Some(-1000 | -1011 | -1012)) => error.outcome(Outcome::Unknown),
        (_, Some(-1004 | -1005 | -1006 | -1008 | -1101 | -1102 | -1103)) => {
            rejected(error, ErrorKind::Rejected)
        }
        // A 2xx that is not an explicit `success: false` refusal proves nothing.
        _ if response.status < 400 && parsed.is_none_or(|c| c.success != Some(false)) => {
            error.outcome(Outcome::Unknown)
        }
        (400..=499, None) => rejected(error, ErrorKind::Rejected),
        _ => error,
    }
}
