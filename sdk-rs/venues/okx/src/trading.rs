//! Commands are preflighted once, then executed as ordered, non-retried writes.
use crate::{Okx, Pending, wire};
use safe_cex_engine::model::*;
use safe_cex_engine::{Command, CommandId, Cx, LogLevel, Method, Response, num};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

const BROKER: &str = "e847386590ce4dBC";

pub struct Request {
    command: CommandId,
}
struct Step {
    path: &'static str,
    body: String,
    expected: usize,
    collect_ids: bool,
}
#[derive(Default)]
pub struct TradingState {
    jobs: HashMap<CommandId, Job>,
}
struct Job {
    steps: VecDeque<Step>,
    ids: Vec<String>,
    finish: Finish,
    optimistic: Vec<(String, OrderStatus)>,
    continue_cancel_errors: bool,
}
enum Finish {
    Ids,
    Amend(String),
    Empty,
    Leverage(String, f64),
    Mode(bool),
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OrderPayload {
    inst_id: String,
    td_mode: &'static str,
    tag: &'static str,
    side: OrderSide,
    ord_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pos_side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cl_ord_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    algo_cl_ord_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sz: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reduce_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    close_fraction: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_px_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_trigger_px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_ord_px: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_trigger_px_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_trigger_px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_ord_px: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_trigger_px_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_px: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    callback_ratio: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    callback_spread: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Cancel<'a> {
    inst_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    ord_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    algo_id: Option<&'a str>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ack<'a> {
    #[serde(default)]
    ord_id: &'a str,
    #[serde(default)]
    algo_id: &'a str,
    #[serde(default)]
    s_code: &'a str,
    #[serde(default)]
    s_msg: &'a str,
}

impl Okx {
    pub(crate) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let (id, result) = match command {
            Command::PlaceOrders { command, orders } => (command, self.plan_place(cx, orders)),
            Command::CancelOrders { command, ids } => (command, self.plan_cancel(cx, ids)),
            Command::CancelSymbolOrders { command, symbol } => {
                let ids = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| {
                        o.symbol == symbol
                            && matches!(
                                o.status,
                                OrderStatus::Open | OrderStatus::Creating | OrderStatus::Updating
                            )
                    })
                    .map(|o| o.id.clone())
                    .collect();
                (command, self.plan_cancel(cx, ids))
            }
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => (command, self.plan_amend(cx, &id, price, amount)),
            Command::SetMarginType { command, .. } => {
                cx.done(command, Err("Only cross mode is supported".into()));
                return;
            }
            Command::SetLeverage {
                command,
                symbol,
                leverage,
                side: _,
            } => {
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Leverage<'a> {
                    inst_id: &'a str,
                    lever: String,
                    mgn_mode: &'static str,
                }
                let result = (|| {
                    let m = cx
                        .store
                        .market(&symbol)
                        .ok_or_else(|| format!("unknown market {symbol}"))?;
                    if !leverage.is_finite() {
                        return Err("leverage must be finite".into());
                    }
                    let leverage = num::clamp_leverage(
                        leverage,
                        m.limits.leverage.min,
                        m.limits.leverage.max,
                    )?;
                    let step = step(
                        "/api/v5/account/set-leverage",
                        &Leverage {
                            inst_id: &m.id,
                            lever: num::format(leverage),
                            mgn_mode: "cross",
                        },
                        0,
                        false,
                    )?;
                    Ok(job(vec![step], Finish::Leverage(symbol, leverage)))
                })();
                (command, result)
            }
            Command::SetPositionMode {
                command,
                hedged,
                symbol: _,
            } => {
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Mode {
                    pos_mode: &'static str,
                }
                let result = if cx.store.positions().iter().any(|p| p.contracts > 0.0) {
                    Err("Please close all positions before switching position mode".into())
                } else {
                    step(
                        "/api/v5/account/set-position-mode",
                        &Mode {
                            pos_mode: if hedged {
                                "long_short_mode"
                            } else {
                                "net_mode"
                            },
                        },
                        0,
                        false,
                    )
                    .map(|s| job(vec![s], Finish::Mode(hedged)))
                };
                (command, result)
            }
            _ => return,
        };
        if self.credentials.is_none() || !self.account.ready {
            cx.done(id, Err("OKX account is not ready".into()));
            return;
        }
        if self.trading.jobs.contains_key(&id) {
            cx.done(id, Err("command is already running".into()));
            return;
        }
        match result {
            Ok(mut job) => {
                for (order, status) in &mut job.optimistic {
                    if let Some(current) = cx.store.order(order) {
                        *status = current.status;
                    }
                    cx.store.set_order_status(
                        order,
                        if matches!(job.finish, Finish::Amend(_)) {
                            OrderStatus::Updating
                        } else {
                            OrderStatus::Canceling
                        },
                    );
                }
                self.trading.jobs.insert(id, job);
                self.next_write(cx, id);
            }
            Err(error) => cx.done(id, Err(error)),
        }
    }

    fn plan_place(&self, cx: &Cx<'_, Pending>, orders: Vec<PlaceOrderOpts>) -> Result<Job, String> {
        if orders.iter().any(|order| {
            order.bybit_bbo.is_some() || order.bybit_slippage_tolerance_ticks.is_some()
        }) {
            return Err("Bybit execution controls are supported only on Bybit".into());
        }
        if orders
            .iter()
            .any(|order| order.implied_volatility.is_some())
        {
            return Err("Implied volatility is supported only for options".into());
        }
        let mut seen = HashSet::new();
        for o in &orders {
            let m = cx
                .store
                .market(&o.symbol)
                .ok_or_else(|| format!("Market {} not found on OKX", o.symbol))?;
            if let Some(id) = &o.client_order_id {
                if id.is_empty()
                    || id.len() > 32
                    || !id.bytes().all(|c| c.is_ascii_alphanumeric())
                    || !seen.insert(id)
                {
                    return Err(format!("Invalid or duplicated OKX client order ID {id}"));
                }
                if !matches!(o.kind, OrderType::Limit | OrderType::Market)
                    || o.trigger_price.is_some()
                    || o.stop_loss.is_some()
                    || o.take_profit.is_some()
                    || o.amount > m.limits.amount.max
                {
                    return Err(
                        "Caller client order ID requires exactly one plain native order".into(),
                    );
                }
            }
        }
        let mut regular = Vec::new();
        let mut brackets = Vec::new();
        let mut algos = Vec::new();
        for o in orders {
            if is_algo(o.kind) {
                algos.push(o);
                continue;
            }
            let m = cx.store.market(&o.symbol).expect("preflight market");
            if !m.limits.amount.max.is_finite()
                || m.limits.amount.max <= 0.0
                || m.precision.amount <= 0.0
            {
                return Err("invalid market size limits".into());
            }
            let amount = adjusted(o.amount, m.precision.amount, false)?;
            let max = lot_floor(m.limits.amount.max, m.precision.amount)?;
            if o.client_order_id.is_some() && amount > max {
                return Err(
                    "Caller client order ID requires exactly one plain native order".into(),
                );
            }
            if max <= 0.0 || amount / max > 10_000.0 {
                return Err("order exceeds safe split bounds".into());
            }
            let bracket =
                o.stop_loss.is_some_and(|v| v != 0.0) || o.take_profit.is_some_and(|v| v != 0.0);
            let mut remaining = amount;
            let mut count = 0;
            while remaining > 0.0 {
                let size = remaining.min(max);
                let payload = self.order_payload(cx, &o, size)?;
                count += 1;
                if bracket {
                    // Protect each accepted lot before another entry can fail.
                    brackets.push(step("/api/v5/trade/batch-orders", &[payload], 1, true)?);
                    for (kind, price) in [
                        (OrderType::StopLoss, o.stop_loss),
                        (OrderType::TakeProfit, o.take_profit),
                    ] {
                        if let Some(price) = price.filter(|v| *v != 0.0) {
                            let mut leg = o.clone();
                            leg.kind = kind;
                            leg.side = o.side.opposite();
                            leg.amount = size;
                            leg.price = Some(price);
                            leg.client_order_id = None;
                            leg.stop_loss = None;
                            leg.take_profit = None;
                            leg.reduce_only = Some(true);
                            let payload = self.order_payload(cx, &leg, size)?;
                            brackets.push(step("/api/v5/trade/order-algo", &payload, 1, true)?);
                        }
                    }
                } else {
                    regular.push(payload);
                }
                remaining = num::adjust(remaining - size, m.precision.amount);
            }
            if o.client_order_id.is_some() && count != 1 {
                return Err(
                    "Caller client order ID requires exactly one plain native order".into(),
                );
            }
        }
        algos.sort_by_key(|o| match o.kind {
            OrderType::StopLoss => 0,
            OrderType::TrailingStopLoss => 1,
            OrderType::TakeProfit => 2,
            _ => 3,
        });
        // Build every payload before issuing a write, including protective legs.
        let algos = algos
            .into_iter()
            .map(|o| self.order_payload(cx, &o, o.amount))
            .collect::<Result<Vec<_>, _>>()?;
        let mut steps = Vec::new();
        for batch in regular.chunks(20) {
            steps.push(step(
                "/api/v5/trade/batch-orders",
                batch,
                batch.len(),
                true,
            )?);
        }
        steps.extend(brackets);
        for payload in algos {
            steps.push(step("/api/v5/trade/order-algo", &payload, 1, true)?);
        }
        Ok(job(steps, Finish::Ids))
    }

    fn order_payload(
        &self,
        cx: &Cx<'_, Pending>,
        o: &PlaceOrderOpts,
        amount: f64,
    ) -> Result<OrderPayload, String> {
        let m = cx
            .store
            .market(&o.symbol)
            .ok_or_else(|| format!("unknown market {}", o.symbol))?;
        let algo = is_algo(o.kind);
        let size = adjusted(
            amount,
            m.precision.amount,
            algo && o.kind != OrderType::Conditional,
        )?;
        let protective = matches!(
            o.kind,
            OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
        );
        let closing =
            protective || (o.kind != OrderType::Conditional && o.reduce_only.unwrap_or(false));
        let pos_side = if cx.store.hedged() {
            Some(if (o.side == OrderSide::Buy) != closing {
                "long"
            } else {
                "short"
            })
        } else {
            None
        };
        let mut p = OrderPayload {
            inst_id: m.id.clone(),
            td_mode: "cross",
            tag: BROKER,
            side: o.side,
            ord_type: match o.kind {
                OrderType::Limit if o.time_in_force == Some(TimeInForce::PostOnly) => "post_only",
                OrderType::Limit => "limit",
                OrderType::Market => "market",
                OrderType::Conditional => "trigger",
                OrderType::TrailingStopLoss => "move_order_stop",
                _ => "conditional",
            },
            pos_side,
            ..empty_payload()
        };
        let generated = || {
            safe_cex_crypto::uuid::uuid()
                .map(|id| format!("{BROKER}{id}").chars().take(32).collect::<String>())
        };
        if algo {
            p.algo_cl_ord_id = Some(generated()?);
        } else {
            p.cl_ord_id = Some(match &o.client_order_id {
                Some(id) => id.clone(),
                None => generated()?,
            });
        }
        if !algo {
            p.sz = Some(num::format(size));
            p.reduce_only = o.reduce_only.filter(|b| *b);
            if o.kind == OrderType::Limit {
                p.px = Some(num::format(adjusted(
                    o.price.unwrap_or(0.0),
                    m.precision.price,
                    false,
                )?));
            }
        }
        match o.kind {
            OrderType::Conditional => {
                p.sz = Some(num::format(size));
                p.trigger_px = Some(num::format(adjusted(
                    o.trigger_price.unwrap_or(0.0),
                    m.precision.price,
                    false,
                )?));
                p.order_px = Some(if let Some(price) = o.price {
                    num::format(adjusted(price, m.precision.price, false)?)
                } else {
                    "-1".into()
                });
                p.trigger_px_type = Some(trigger_name(o.trigger_type));
            }
            OrderType::StopLoss | OrderType::TakeProfit => {
                if size > 0.0 {
                    p.sz = Some(num::format(size));
                } else {
                    p.close_fraction = Some("1");
                }
                p.reduce_only = Some(true);
                let price =
                    num::format(adjusted(o.price.unwrap_or(0.0), m.precision.price, false)?);
                if o.kind == OrderType::StopLoss {
                    p.sl_trigger_px = Some(price);
                    p.sl_ord_px = Some("-1");
                    p.sl_trigger_px_type = Some(trigger_name(
                        o.sl_trigger_type.or(Some(StopTriggerType::MarkPrice)),
                    ));
                } else {
                    p.tp_trigger_px = Some(price);
                    p.tp_ord_px = Some("-1");
                    p.tp_trigger_px_type = Some(trigger_name(o.tp_trigger_type));
                }
            }
            OrderType::TrailingStopLoss => {
                p.active_px = Some("");
                p.callback_ratio = Some("");
                p.callback_spread = Some(num::format(adjusted(
                    o.price.unwrap_or(0.0),
                    m.precision.price,
                    false,
                )?));
                p.close_fraction = Some("1");
            }
            _ => {}
        }
        Ok(p)
    }

    fn plan_cancel(&self, cx: &Cx<'_, Pending>, ids: Vec<String>) -> Result<Job, String> {
        let mut regular = Vec::new();
        let mut algos = Vec::new();
        let mut seen = HashSet::new();
        let mut optimistic = Vec::new();
        for id in &ids {
            let Some(o) = cx.store.order(id) else {
                continue;
            };
            let m = cx
                .store
                .market(&o.symbol)
                .ok_or_else(|| "unknown market".to_owned())?;
            let native = native_id(id);
            optimistic.push((id.clone(), o.status));
            if !seen.insert((m.id.as_str(), native)) {
                continue;
            }
            if is_algo(o.kind) {
                algos.push(Cancel {
                    inst_id: &m.id,
                    ord_id: None,
                    algo_id: Some(native),
                });
            } else {
                regular.push(Cancel {
                    inst_id: &m.id,
                    ord_id: Some(id),
                    algo_id: None,
                });
            }
        }
        let mut steps = Vec::new();
        for batch in regular.chunks(20) {
            steps.push(step(
                "/api/v5/trade/cancel-batch-orders",
                batch,
                batch.len(),
                false,
            )?);
        }
        for batch in algos.chunks(10) {
            steps.push(step(
                "/api/v5/trade/cancel-algos",
                batch,
                batch.len(),
                false,
            )?);
        }
        let mut result = job(steps, Finish::Empty);
        result.optimistic = optimistic;
        result.continue_cancel_errors = true;
        Ok(result)
    }

    fn plan_amend(
        &self,
        cx: &Cx<'_, Pending>,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<Job, String> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Amend<'a> {
            inst_id: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            ord_id: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            algo_id: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_sz: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_trigger_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_ord_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_trigger_px_type: Option<&'static str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_sl_trigger_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_sl_ord_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_sl_trigger_px_type: Option<&'static str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_tp_trigger_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_tp_ord_px: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            new_tp_trigger_px_type: Option<&'static str>,
        }
        let o = cx
            .store
            .order(id)
            .ok_or_else(|| format!("unknown order {id}"))?;
        if o.kind == OrderType::Market
            || !matches!(o.status, OrderStatus::Open | OrderStatus::Updating)
        {
            return Err("Only active limit or algo orders can be amended".into());
        }
        let m = cx
            .store
            .market(&o.symbol)
            .ok_or_else(|| "unknown market".to_owned())?;
        let oco = matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
            && cx.store.orders().iter().any(|other| {
                other.id != o.id
                    && other.symbol == o.symbol
                    && native_id(&other.id) == native_id(id)
                    && matches!(other.kind, OrderType::StopLoss | OrderType::TakeProfit)
            });
        if !oco
            && matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
            && o.extra
                .as_ref()
                .and_then(|e| e.conditional_order_price)
                .is_some_and(|p| p > 0.0)
        {
            return Err("Cannot amend a standalone protective limit order without preserving its execution price".into());
        }
        let mut a = Amend {
            inst_id: &m.id,
            ord_id: None,
            algo_id: None,
            new_px: None,
            new_sz: None,
            new_trigger_px: None,
            new_ord_px: None,
            new_trigger_px_type: None,
            new_sl_trigger_px: None,
            new_sl_ord_px: None,
            new_sl_trigger_px_type: None,
            new_tp_trigger_px: None,
            new_tp_ord_px: None,
            new_tp_trigger_px_type: None,
        };
        if let Some(amount) = amount.filter(|_| o.amount != 0.0) {
            a.new_sz = Some(num::format(adjusted(amount, m.precision.amount, false)?));
        }
        let mut result = if o.kind == OrderType::Limit {
            a.ord_id = Some(id);
            if let Some(price) = price {
                if !price.is_finite() || price <= 0.0 {
                    return Err("invalid order price".into());
                }
                a.new_px = Some(num::format(price));
            }
            job(
                vec![step("/api/v5/trade/amend-order", &a, 1, false)?],
                Finish::Amend(id.into()),
            )
        } else if o.kind == OrderType::Conditional {
            a.algo_id = Some(native_id(id));
            a.new_trigger_px = Some(num::format(adjusted(
                price.unwrap_or(o.price),
                m.precision.price,
                false,
            )?));
            let limit = o
                .extra
                .as_ref()
                .and_then(|e| e.conditional_order_price)
                .unwrap_or(-1.0);
            a.new_ord_px = Some(if limit > 0.0 {
                num::format(num::adjust(limit, m.precision.price))
            } else {
                "-1".into()
            });
            a.new_trigger_px_type = Some(trigger_name(o.trigger_type));
            job(
                vec![step("/api/v5/trade/amend-algos", &a, 1, false)?],
                Finish::Amend(id.into()),
            )
        } else if oco {
            a.algo_id = Some(native_id(id));
            let price = num::format(adjusted(
                price.unwrap_or(o.price),
                m.precision.price,
                false,
            )?);
            let order_price = o
                .extra
                .as_ref()
                .and_then(|e| e.conditional_order_price)
                .filter(|p| *p > 0.0)
                .map_or_else(
                    || "-1".into(),
                    |p| num::format(num::adjust(p, m.precision.price)),
                );
            if o.kind == OrderType::StopLoss {
                a.new_sl_trigger_px = Some(price);
                a.new_sl_ord_px = Some(order_price);
                a.new_sl_trigger_px_type = Some(trigger_name(o.trigger_type));
            } else {
                a.new_tp_trigger_px = Some(price);
                a.new_tp_ord_px = Some(order_price);
                a.new_tp_trigger_px_type = Some(trigger_name(o.trigger_type));
            }
            job(
                vec![step("/api/v5/trade/amend-algos", &a, 1, false)?],
                Finish::Amend(id.into()),
            )
        } else {
            let group: Vec<_> = cx
                .store
                .orders()
                .iter()
                .filter(|c| native_id(&c.id) == native_id(id))
                .map(|c| c.id.clone())
                .collect();
            let mut result = self.plan_cancel(cx, group)?;
            let replacement = self.plan_place(
                cx,
                vec![PlaceOrderOpts {
                    symbol: o.symbol.clone(),
                    kind: o.kind,
                    side: o.side,
                    amount: amount.unwrap_or(o.amount),
                    price: Some(price.unwrap_or(o.price)),
                    trigger_price: None,
                    stop_loss: None,
                    take_profit: None,
                    reduce_only: Some(o.reduce_only),
                    time_in_force: o.time_in_force,
                    trigger_type: o.trigger_type,
                    sl_trigger_type: o.trigger_type,
                    tp_trigger_type: o.trigger_type,
                    client_order_id: None,
                    close: o.close,
                    stp_mode: None,
                    max_slippage_pct: None,
                    bybit_bbo: None,
                    bybit_slippage_tolerance_ticks: None,
                    implied_volatility: None,
                }],
            )?;
            result.steps.extend(replacement.steps);
            result.continue_cancel_errors = false;
            result.finish = Finish::Ids;
            result
        };
        if result.optimistic.is_empty() {
            result.optimistic.push((id.into(), o.status));
        }
        Ok(result)
    }

    fn next_write(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let Some(job) = self.trading.jobs.get(&command) else {
            return;
        };
        if let Some(step) = job.steps.front() {
            let headers = self.headers(cx.now, Method::Post, step.path, &step.body, true);
            cx.request(
                Method::Post,
                format!("{}{}", self.rest(), step.path),
                headers,
                Some(step.body.clone()),
                Pending::Trade(Request { command }),
            );
            return;
        }
        let job = self.trading.jobs.remove(&command).expect("job");
        for (id, status) in job.optimistic {
            if cx
                .store
                .order(&id)
                .is_some_and(|o| matches!(o.status, OrderStatus::Updating | OrderStatus::Canceling))
            {
                cx.store.set_order_status(&id, status);
            }
        }
        match job.finish {
            Finish::Leverage(symbol, leverage) => {
                cx.store.update_setting(&symbol, |s| {
                    s.leverage = leverage;
                    s.margin_type = Some(MarginType::Cross);
                });
                self.account_after_settings(cx);
                cx.done(command, Ok("[]".into()));
            }
            Finish::Mode(hedged) => {
                cx.store.set_hedged(hedged);
                self.account_after_settings(cx);
                cx.done(command, Ok("[]".into()));
            }
            Finish::Amend(id) => cx.done(
                command,
                serde_json::to_string(&[id]).map_err(|e| e.to_string()),
            ),
            Finish::Ids => cx.done(
                command,
                serde_json::to_string(&job.ids).map_err(|e| e.to_string()),
            ),
            Finish::Empty => cx.done(command, Ok("[]".into())),
        }
    }

    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let Some(job) = self.trading.jobs.get_mut(&request.command) else {
            return;
        };
        let step = job.steps.front().expect("inflight step");
        if !(200..300).contains(&response.status) {
            self.trade_failed(
                cx,
                request,
                &format!("OKX HTTP {}: {}", response.status, response.body),
            );
            return;
        }
        let envelope = match serde_json::from_str::<wire::Envelope<'_>>(response.body) {
            Ok(envelope) => envelope,
            Err(e) => {
                self.trade_failed(cx, request, &e.to_string());
                return;
            }
        };
        let acks = match envelope
            .data
            .and_then(|data| serde_json::from_str::<Vec<Ack<'_>>>(data.get()).ok())
        {
            Some(acks) => acks,
            None => {
                self.trade_failed(cx, request, "Malformed OKX write response");
                return;
            }
        };
        let errors: Vec<_> = acks
            .iter()
            .filter(|a| (!a.s_code.is_empty() && a.s_code != "0") || !a.s_msg.is_empty())
            .map(|a| format!("{} (code {})", a.s_msg, a.s_code))
            .collect();
        if envelope.code != "0"
            && !(job.continue_cancel_errors && envelope.code == "2" && !errors.is_empty())
        {
            let error = if errors.is_empty() {
                format!("OKX {}: {}", envelope.code, envelope.msg)
            } else {
                errors.join("; ")
            };
            self.trade_failed(cx, request, &error);
            return;
        }
        if !errors.is_empty() {
            if !job.continue_cancel_errors {
                self.trade_failed(cx, request, &errors.join("; "));
                return;
            }
            for error in errors {
                cx.log(LogLevel::Error, error);
            }
        }
        if step.expected > 0 && acks.len() != step.expected {
            self.trade_failed(cx, request, "OKX write response row count mismatch");
            return;
        }
        if step.collect_ids {
            for ack in acks {
                let id = if ack.algo_id.is_empty() {
                    ack.ord_id
                } else {
                    ack.algo_id
                };
                if id.is_empty() {
                    self.trade_failed(cx, request, "OKX placement response missing order ID");
                    return;
                }
                job.ids.push(id.into());
            }
        }
        job.steps.pop_front();
        self.next_write(cx, request.command);
    }

    pub(crate) fn trade_failed(&mut self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        if let Some(job) = self.trading.jobs.remove(&request.command) {
            for (id, status) in job.optimistic {
                if cx.store.order(&id).is_some_and(|o| {
                    matches!(o.status, OrderStatus::Updating | OrderStatus::Canceling)
                }) {
                    cx.store.set_order_status(&id, status);
                }
            }
        }
        cx.done(request.command, Err(error.into()));
    }
}

fn empty_payload() -> OrderPayload {
    OrderPayload {
        inst_id: String::new(),
        td_mode: "cross",
        tag: BROKER,
        side: OrderSide::Buy,
        ord_type: "limit",
        pos_side: None,
        cl_ord_id: None,
        algo_cl_ord_id: None,
        sz: None,
        px: None,
        reduce_only: None,
        close_fraction: None,
        trigger_px: None,
        order_px: None,
        trigger_px_type: None,
        sl_trigger_px: None,
        sl_ord_px: None,
        sl_trigger_px_type: None,
        tp_trigger_px: None,
        tp_ord_px: None,
        tp_trigger_px_type: None,
        active_px: None,
        callback_ratio: None,
        callback_spread: None,
    }
}
fn step<T: Serialize + ?Sized>(
    path: &'static str,
    body: &T,
    expected: usize,
    collect_ids: bool,
) -> Result<Step, String> {
    Ok(Step {
        path,
        body: serde_json::to_string(body).map_err(|e| e.to_string())?,
        expected,
        collect_ids,
    })
}
fn job(steps: Vec<Step>, finish: Finish) -> Job {
    Job {
        steps: steps.into(),
        ids: Vec::new(),
        finish,
        optimistic: Vec::new(),
        continue_cancel_errors: false,
    }
}
fn is_algo(kind: OrderType) -> bool {
    !matches!(kind, OrderType::Limit | OrderType::Market)
}
fn native_id(id: &str) -> &str {
    id.strip_suffix("_sl")
        .or_else(|| id.strip_suffix("_tp"))
        .or_else(|| id.strip_suffix("_tsl"))
        .unwrap_or(id)
}
fn trigger_name(value: Option<StopTriggerType>) -> &'static str {
    match value {
        Some(StopTriggerType::MarkPrice) => "mark",
        Some(StopTriggerType::IndexPrice) => "index",
        _ => "last",
    }
}
fn adjusted(value: f64, precision: f64, allow_zero: bool) -> Result<f64, String> {
    if !value.is_finite() || value < 0.0 || !precision.is_finite() || precision <= 0.0 {
        return Err("order amount/price must be finite".into());
    }
    let result = num::adjust(value, precision);
    if result < 0.0 || (result == 0.0 && (!allow_zero || value > 0.0)) {
        Err("Order amount or price is too small".into())
    } else {
        Ok(result)
    }
}

// Integer decimal units avoid floor(0.3 / 0.1) losing an entire lot.
fn lot_floor(max: f64, step: f64) -> Result<f64, String> {
    let max_text = max.to_string();
    let step_text = step.to_string();
    let decimals = |text: &str| text.split_once('.').map_or(0, |(_, tail)| tail.len());
    let places = decimals(&max_text).max(decimals(&step_text));
    let units = |text: &str| -> Option<u128> {
        let value = text.replace('.', "").parse::<u128>().ok()?;
        value.checked_mul(10u128.checked_pow((places - decimals(text)).try_into().ok()?)?)
    };
    let max_units = units(&max_text).ok_or("invalid market size limits")?;
    let step_units = units(&step_text)
        .filter(|n| *n > 0)
        .ok_or("invalid market lot size")?;
    Ok(num::adjust((max_units / step_units) as f64 * step, step))
}
