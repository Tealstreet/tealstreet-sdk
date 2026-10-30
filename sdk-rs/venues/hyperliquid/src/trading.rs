//! Sequenced writes. A lost response reconciles account state; writes never retry.
mod custody;
mod schedule;
pub(crate) use custody::PACING;
use safe_cex_engine::setting_holds::SettingKey;

use crate::{
    Builder, Hyperliquid, Pending,
    account::{Placement, valid_cloid},
    sign,
};
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::Operation;
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Method, Outcome, Response, TimerId, VenueError, num,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

#[derive(Default)]
pub struct Trading {
    queue: VecDeque<Job>,
    active: Option<Job>,
    inflight: bool,
    builder_wait: Option<TimerId>,
    retry: Option<TimerId>,
    retry_step: usize,
    expected: HashSet<String>,
    uncertain_settings: HashSet<String>,
}
struct Job {
    custody: Option<custody::Custody>,
    command: CommandId,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    replacement: Option<String>,
    amend_source: Option<String>,
    amend_filled: bool,
    builder_checked: bool,
    /// A write took effect, so a later failure cannot mean nothing happened.
    effective: bool,
}
enum Step {
    ScheduleCancel {
        time_ms: Option<u64>,
    },
    Place(PlaceOrderOpts),
    Cancel(Vec<Order>),
    Leverage {
        symbol: String,
        leverage: Option<f64>,
        isolated: Option<bool>,
    },
}
pub struct Request {
    plan: safe_cex_engine::request_context::Plan,
    context: safe_cex_engine::request_context::Context,
    command: CommandId,
    effect: Effect,
    order_reply: OrderReply,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum OrderReply {
    Rows,
    Attached,
    FullPosition,
}
impl OrderReply {
    fn for_action(action: &Action, spot: bool) -> Self {
        let Action::Order {
            orders, grouping, ..
        } = action
        else {
            return Self::Rows;
        };
        if spot {
            return Self::Rows;
        }
        match (*grouping, orders.as_slice()) {
            ("positionTpsl", [order])
                if order.r && order.s == "0" && matches!(order.t, WireType::Trigger { .. }) =>
            {
                Self::FullPosition
            }
            ("normalTpsl", [parent, children @ ..])
                if !children.is_empty()
                    && matches!(parent.t, WireType::Limit { .. })
                    && children.iter().all(|child| {
                        child.r
                            && child.a == parent.a
                            && child.b != parent.b
                            && child.s == parent.s
                            && matches!(child.t, WireType::Trigger { .. })
                    }) =>
            {
                Self::Attached
            }
            _ => Self::Rows,
        }
    }
}
#[derive(Clone)]
enum Effect {
    ScheduleCancel,
    Place(Vec<Placement>),
    Cancel(Vec<Order>),
    Leverage {
        symbol: String,
        leverage: f64,
        isolated: bool,
    },
}
impl Effect {
    fn is_cancel(&self) -> bool {
        matches!(self, Self::Cancel(_))
    }
    fn is_schedule_control(&self) -> bool {
        matches!(self, Self::ScheduleCancel)
    }
}
#[derive(Clone, Serialize)]
struct OrderWire {
    a: u32,
    b: bool,
    p: String,
    s: String,
    r: bool,
    t: WireType,
    c: String,
}
#[derive(Clone, Serialize)]
#[serde(untagged)]
enum WireType {
    Limit { limit: LimitWire },
    Trigger { trigger: TriggerWire },
}
#[derive(Clone, Serialize)]
struct LimitWire {
    tif: &'static str,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TriggerWire {
    is_market: bool,
    trigger_px: String,
    tpsl: &'static str,
}
#[derive(Serialize)]
struct CancelWire {
    a: u32,
    o: u64,
}
#[derive(Serialize)]
#[serde(tag = "type")]
enum Action {
    #[serde(rename = "scheduleCancel")]
    ScheduleCancel {
        #[serde(skip_serializing_if = "Option::is_none")]
        time: Option<u64>,
    },
    #[serde(rename = "order")]
    Order {
        orders: Vec<OrderWire>,
        grouping: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        builder: Option<Builder>,
    },
    #[serde(rename = "cancel")]
    Cancel { cancels: Vec<CancelWire> },
    #[serde(rename = "updateLeverage")]
    Leverage {
        asset: u32,
        #[serde(rename = "isCross")]
        is_cross: bool,
        leverage: u32,
    },
}
#[derive(Deserialize)]
struct Reply<'a> {
    #[serde(borrow)]
    status: &'a str,
    #[serde(borrow)]
    response: &'a serde_json::value::RawValue,
}
#[derive(Deserialize)]
struct ReplyData {
    r#type: String,
    #[serde(default)]
    data: Option<Statuses>,
}
#[derive(Deserialize)]
struct Statuses {
    statuses: Vec<Status>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Status {
    Error { error: String },
    Resting { resting: Oid },
    Filled { filled: Oid },
    Text(String),
}
#[derive(Deserialize)]
struct Oid {
    oid: u64,
}
impl Hyperliquid {
    pub(crate) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if self.spot {
            match &command {
                Command::SetLeverage { leverage, .. } => {
                    if *leverage == 1.0 {
                        let _ = cx.done_local_setting("[]".into(), |_| {});
                    } else {
                        let error = not_sent("Hyperliquid spot supports 1x leverage only");
                        cx.refuse_command(Operation::Mutation, error.kind(ErrorKind::Unsupported));
                    }
                    return;
                }
                Command::SetMarginType { isolated, .. } => {
                    if !isolated {
                        let _ = cx.done_local_setting("[]".into(), |_| {});
                    } else {
                        let error = not_sent("Hyperliquid spot supports cross margin only");
                        cx.refuse_command(Operation::Mutation, error.kind(ErrorKind::Unsupported));
                    }
                    return;
                }
                _ => {}
            }
        }
        // TS: leverage and margin on a merged spot pair are silent no-ops (always 1x cross).
        if let Command::SetLeverage { symbol, .. } | Command::SetMarginType { symbol, .. } =
            &command
            && self.spot_market(cx, symbol)
        {
            let _ = cx.done_local_setting("[]".into(), |_| {});
            return;
        }
        let mut replacement = None;
        let mut amend_source = None;
        let (command, steps) = match command {
            Command::PlaceOrders { command, orders } => {
                if let Err(error) = self.preflight(cx, &orders) {
                    cx.refuse_command(Operation::Mutation, not_sent(error));
                    return;
                }
                (command, orders.into_iter().map(Step::Place).collect())
            }
            Command::CancelOrders { command, ids } => {
                let orders = ids
                    .iter()
                    .map(|id| {
                        cx.store.order(id).cloned().ok_or_else(|| {
                            format!("Order {id} not found; cancellation requires its market")
                        })
                    })
                    .collect::<Result<Vec<_>, String>>();
                let orders = match orders {
                    Ok(orders) => orders,
                    Err(error) => {
                        cx.refuse_command(Operation::Mutation, not_sent(error));
                        return;
                    }
                };
                (command, VecDeque::from([Step::Cancel(orders)]))
            }
            Command::CancelSymbolOrders { command, symbol } => (
                command,
                VecDeque::from([Step::Cancel(
                    cx.store
                        .orders()
                        .iter()
                        .filter(|o| o.symbol == symbol)
                        .cloned()
                        .collect(),
                )]),
            ),
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => {
                if self
                    .trading
                    .active
                    .iter()
                    .chain(self.trading.queue.iter())
                    .any(|job| job.amend_source.as_deref() == Some(id.as_str()))
                {
                    cx.refuse_command(
                        Operation::Mutation,
                        not_sent("Hyperliquid order already has an amendment pending"),
                    );
                    return;
                }
                let Some(old) = cx.store.order(&id).cloned() else {
                    cx.refuse_command(Operation::Mutation, not_sent("Order not found"));
                    return;
                };
                if !matches!(old.status, OrderStatus::Open | OrderStatus::Creating) {
                    cx.refuse_command(
                        Operation::Mutation,
                        not_sent("Hyperliquid order is not available for amendment"),
                    );
                    return;
                }
                let mut opts = PlaceOrderOpts {
                    symbol: old.symbol.clone(),
                    kind: old.kind,
                    side: old.side,
                    price: Some(price.unwrap_or(old.price)),
                    amount: amount.unwrap_or(old.amount),
                    reduce_only: Some(old.reduce_only),
                    time_in_force: old.time_in_force,
                    trigger_price: None,
                    stop_loss: None,
                    take_profit: None,
                    trigger_type: None,
                    tp_trigger_type: None,
                    sl_trigger_type: None,
                    client_order_id: None,
                    close: None,
                    stp_mode: None,
                    max_slippage_pct: None,
                    bybit_bbo: None,
                    bybit_slippage_tolerance_ticks: None,
                    implied_volatility: None,
                };
                if opts.kind == OrderType::TakeProfit
                    && let Some(t) = cx.store.ticker(&opts.symbol)
                {
                    let prices = [t.mark, t.last, t.index];
                    let prices = prices
                        .into_iter()
                        .filter(|v| v.is_finite() && *v > 0.0)
                        .collect::<Vec<_>>();
                    if let Some(reference) = prices.first()
                        && prices
                            .iter()
                            .all(|p| (p - reference).abs() / reference <= 0.25)
                    {
                        let above = opts.price.unwrap_or(0.0) > *reference;
                        if (opts.side == OrderSide::Sell && !above)
                            || (opts.side == OrderSide::Buy && above)
                        {
                            opts.kind = OrderType::StopLoss;
                        }
                    }
                }
                let cloid = match mint_cloid(cx) {
                    Ok(c) => c,
                    Err(e) => {
                        cx.refuse_command(Operation::Mutation, not_sent(e));
                        return;
                    }
                };
                opts.client_order_id = Some(cloid.clone());
                replacement = Some(cloid);
                if let Err(error) = self.preflight(cx, std::slice::from_ref(&opts)) {
                    cx.refuse_command(Operation::Mutation, not_sent(error));
                    return;
                }
                amend_source = Some(id);
                (
                    command,
                    VecDeque::from([Step::Cancel(vec![old]), Step::Place(opts)]),
                )
            }
            Command::SetLeverage {
                command,
                symbol,
                leverage,
                side: _,
            } => (
                command,
                VecDeque::from([Step::Leverage {
                    symbol,
                    leverage: Some(leverage),
                    isolated: None,
                }]),
            ),
            Command::SetMarginType {
                command,
                symbol,
                isolated,
            } => (
                command,
                VecDeque::from([Step::Leverage {
                    symbol,
                    leverage: None,
                    isolated: Some(isolated),
                }]),
            ),
            Command::SetPositionMode { hedged, .. } => {
                if hedged {
                    cx.refuse_command(
                        Operation::Mutation,
                        not_sent("Hyperliquid supports one-way positions only")
                            .kind(ErrorKind::Unsupported),
                    );
                } else {
                    let _ = cx.done_local_setting("[]".into(), |store| store.set_hedged(false));
                }
                return;
            }
            _ => return,
        };
        self.enqueue_trade(
            cx,
            Job {
                custody: None,
                command,
                steps,
                ids: Vec::new(),
                replacement,
                amend_source,
                amend_filled: false,
                builder_checked: false,
                effective: false,
            },
        );
    }
    fn preflight(&self, cx: &Cx<'_, Pending>, orders: &[PlaceOrderOpts]) -> Result<(), String> {
        let mut seen = HashSet::new();
        for o in orders {
            if o.bybit_bbo.is_some() || o.bybit_slippage_tolerance_ticks.is_some() {
                return Err(
                    "Bybit BBO and TickSize controls are not supported by Hyperliquid".into(),
                );
            }
            let m = cx
                .store
                .market(&o.symbol)
                .ok_or_else(|| format!("Market {} not found", o.symbol))?;
            if let Some(id) = o.client_order_id.as_deref() {
                if !valid_cloid(id) {
                    return Err("Hyperliquid client order ID must be 0x followed by 32 lowercase hex characters".into());
                }
                if !seen.insert(id) {
                    return Err("Duplicate Hyperliquid client order ID in batch".into());
                }
                if o.amount > m.limits.amount.max {
                    return Err(
                        "Caller client order ID cannot be split across native orders".into(),
                    );
                }
            }
            if self.spot && !matches!(o.kind, OrderType::Limit | OrderType::Market) {
                return Err("Hyperliquid spot order type is not supported".into());
            }
            if !matches!(
                o.kind,
                OrderType::Limit | OrderType::Market | OrderType::TakeProfit | OrderType::StopLoss
            ) {
                return Err("Hyperliquid order type is not supported".into());
            }
            if !o.amount.is_finite()
                || o.amount < 0.0
                || (o.amount == 0.0 && matches!(o.kind, OrderType::Limit | OrderType::Market))
            {
                return Err("Invalid Hyperliquid order amount".into());
            }
            if o.kind != OrderType::Market && o.price.is_none_or(|p| !p.is_finite() || p <= 0.0) {
                return Err("Invalid Hyperliquid order price".into());
            }
            if [o.stop_loss, o.take_profit]
                .into_iter()
                .flatten()
                .any(|p| !p.is_finite() || p < 0.0)
            {
                return Err("Invalid bracket trigger price".into());
            }
            if o.kind != OrderType::Limit {
                self.base_price(cx, &o.symbol)?;
            }
        }
        Ok(())
    }
    /// A spot pair, in the standalone spot venue or merged into the perp account.
    pub(crate) fn spot_market(&self, cx: &Cx<'_, Pending>, symbol: &str) -> bool {
        self.spot
            || cx
                .store
                .market(symbol)
                .is_some_and(|m| m.is_spot == Some(true))
    }
    fn base_price(&self, cx: &Cx<'_, Pending>, symbol: &str) -> Result<f64, String> {
        let t = cx.store.ticker(symbol).ok_or("Missing ticker price data")?;
        if self.spot {
            return (t.last.is_finite() && t.last > 0.0)
                .then_some(t.last)
                .ok_or_else(|| "Missing valid ticker price data".into());
        }
        let candidates = if self.spot_market(cx, symbol) {
            [t.last, t.mid_price.unwrap_or(0.0), t.mark, t.index]
        } else {
            [t.mark, t.last, t.mid_price.unwrap_or(0.0), t.index]
        };
        candidates
            .into_iter()
            .find(|p| p.is_finite() && *p > 0.0)
            .ok_or_else(|| "Missing valid ticker price data".into())
    }
    fn wires(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
    ) -> Result<(Vec<OrderWire>, &'static str), String> {
        let m = cx.store.market(&opts.symbol).ok_or("Market not found")?;
        let asset = *self
            .assets
            .get(&opts.symbol)
            .ok_or("Market asset index not found")?;
        let size = num::adjust(opts.amount, m.precision.amount);
        let trigger = matches!(opts.kind, OrderType::TakeProfit | OrderType::StopLoss);
        let positional = trigger
            && (opts.amount == 0.0
                || (opts.reduce_only == Some(true)
                    && cx.store.positions().iter().any(|p| {
                        p.symbol == opts.symbol
                            && p.contracts > 0.0
                            && size >= p.contracts
                            && ((p.side == PositionSide::Long && opts.side == OrderSide::Sell)
                                || (p.side == PositionSide::Short && opts.side == OrderSide::Buy))
                    })));
        let p = if opts.kind == OrderType::Limit {
            opts.price.unwrap_or(0.0)
        } else {
            self.base_price(cx, &opts.symbol)?
                * if opts.side == OrderSide::Buy {
                    1.05
                } else {
                    0.95
                }
        };
        let spot = self.spot || m.is_spot == Some(true);
        let price = price_wire(p, m.precision.amount, spot)?;
        let cloid = match &opts.client_order_id {
            Some(c) => c.clone(),
            None => mint_cloid(cx)?,
        };
        let t = if trigger {
            WireType::Trigger {
                trigger: TriggerWire {
                    is_market: true,
                    trigger_px: price_wire(opts.price.unwrap_or(0.0), m.precision.amount, spot)?,
                    tpsl: if opts.kind == OrderType::TakeProfit {
                        "tp"
                    } else {
                        "sl"
                    },
                },
            }
        } else {
            WireType::Limit {
                limit: LimitWire {
                    tif: if opts.time_in_force == Some(TimeInForce::PostOnly)
                        && opts.kind == OrderType::Limit
                    {
                        "Alo"
                    } else {
                        "Gtc"
                    },
                },
            }
        };
        let main = OrderWire {
            a: asset,
            b: opts.side == OrderSide::Buy,
            p: price,
            s: float_wire(if positional { 0.0 } else { size })?,
            r: !spot && opts.reduce_only.unwrap_or(false),
            t,
            c: cloid,
        };
        let mut wires = vec![main];
        if opts.kind == OrderType::Limit {
            for (kind, value) in [("sl", opts.stop_loss), ("tp", opts.take_profit)] {
                // Like TS, a zero bracket price means no bracket on every market.
                if let Some(trigger) = value.filter(|p| *p != 0.0) {
                    wires.push(OrderWire {
                        a: asset,
                        b: opts.side != OrderSide::Buy,
                        p: wires[0].p.clone(),
                        s: float_wire(size)?,
                        r: !spot,
                        t: WireType::Trigger {
                            trigger: TriggerWire {
                                is_market: true,
                                trigger_px: price_wire(trigger, m.precision.amount, spot)?,
                                tpsl: kind,
                            },
                        },
                        c: mint_cloid(cx)?,
                    });
                }
            }
        }
        let grouping = if positional {
            "positionTpsl"
        } else if wires.len() > 1 {
            "normalTpsl"
        } else {
            "na"
        };
        Ok((wires, grouping))
    }
    pub(crate) fn pump_trades(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading.inflight || self.trading.builder_wait.is_some() {
            return;
        }
        if self.trading.active.is_none() {
            self.trading.active = self.trading.queue.pop_front();
        }
        if self.trading.active.is_none() {
            return;
        }
        if !self.trade_entry_ready(cx) {
            return;
        }
        let job = self.trading.active.as_ref().unwrap();
        if job.steps.is_empty() {
            self.finish_trade(cx, Ok(()));
            return;
        }
        if job.amend_filled {
            self.finish_trade(cx, Err(not_sent("Hyperliquid amendment source was filled")));
            return;
        }
        if matches!(job.steps.front(), Some(Step::Cancel(_)))
            && let Some(source) = &job.amend_source
            && cx.store.order(source).is_none_or(|order| {
                !matches!(order.status, OrderStatus::Open | OrderStatus::Creating)
            })
        {
            self.finish_trade(
                cx,
                Err(not_sent("Hyperliquid amendment source is no longer open")),
            );
            return;
        }
        let needs_builder = matches!(job.steps.front(), Some(Step::Place(_)))
            && self.account.builder_approved.is_none()
            && !job.builder_checked;
        if needs_builder {
            if let Err(error) = self.park_trade_builder(cx) {
                self.finish_trade(cx, Err(error));
                return;
            }
            self.trading.active.as_mut().unwrap().builder_checked = true;
            self.check_builder(cx);
            self.trading.builder_wait = Some(cx.set_timer(8000.0));
            return;
        }

        let step = self
            .trading
            .active
            .as_mut()
            .unwrap()
            .steps
            .pop_front()
            .unwrap();
        let prepared = self.prepare_write(cx, step);
        let (action, effect) = match prepared {
            Ok(v) => v,
            Err(error) => {
                self.finish_trade(cx, Err(not_sent(error)));
                return;
            }
        };
        if matches!(&effect,Effect::Cancel(orders) if orders.is_empty()) {
            self.pump_trades(cx);
            return;
        }
        if let Err(error) = self.submit_trade(cx, &action, effect) {
            self.finish_trade(cx, Err(error));
        }
    }
    fn remember_trade_effect(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        _command: CommandId,
        effect: &Effect,
    ) {
        if let Effect::Leverage { symbol, .. } = effect {
            self.trading.uncertain_settings.insert(symbol.clone());
            self.acknowledge_settings(cx, symbol);
        }
        if let Effect::Cancel(orders) = effect {
            for order in orders {
                cx.store.set_order_status(&order.id, OrderStatus::Canceling);
            }
        }
        if let Effect::Place(records) = effect {
            for record in records {
                if let Some(c) = &record.order.client_order_id {
                    self.account.records.insert(c.clone(), record.clone());
                }
                if record.order.symbol.contains('|')
                    || (!record.order.reduce_only
                        && matches!(
                            record.order.kind,
                            OrderType::StopLoss | OrderType::TakeProfit
                        ))
                {
                    self.account.preplacements.push_back(record.clone());
                }
            }
        }
        while self.account.records.len() > 500 {
            if let Some(old) = self
                .account
                .records
                .iter()
                .min_by(|a, b| a.1.at.total_cmp(&b.1.at))
                .map(|(id, _)| id.clone())
            {
                self.account.records.remove(&old);
            }
        }
        while self.account.preplacements.len() > 500 {
            self.account.preplacements.pop_front();
        }
    }
    fn publish_trade_receipts(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        effect: &Effect,
    ) {
        if let Effect::Place(records) = effect {
            cx.command_progress(
                command,
                records
                    .iter()
                    .filter_map(|record| {
                        Some(OrderIdentityReceipt {
                            symbol: record.order.symbol.clone(),
                            kind: OrderIdentityKind::ClientOrderId,
                            identity: record.order.client_order_id.clone()?,
                        })
                    })
                    .collect(),
            );
        }
    }
    fn prepare_write(
        &self,
        cx: &mut Cx<'_, Pending>,
        step: Step,
    ) -> Result<(Action, Effect), String> {
        match step {
            Step::ScheduleCancel { time_ms } => {
                let time = schedule::deadline(time_ms.map(|time| time as f64), cx.now)?;
                Ok((Action::ScheduleCancel { time }, Effect::ScheduleCancel))
            }

            Step::Place(opts) => {
                let (wires, grouping) = self.wires(cx, &opts)?;
                let records = wires
                    .iter()
                    .map(|wire| {
                        let (kind, price, tif) = match &wire.t {
                            WireType::Limit { limit } => (
                                OrderType::Limit,
                                num::parse(&wire.p),
                                crate::account::tif(limit.tif),
                            ),
                            WireType::Trigger { trigger } => (
                                if trigger.tpsl == "tp" {
                                    OrderType::TakeProfit
                                } else {
                                    OrderType::StopLoss
                                },
                                num::parse(&trigger.trigger_px),
                                None,
                            ),
                        };
                        let amount = num::parse(&wire.s);
                        Placement {
                            at: cx.now,
                            order: Order {
                                id: String::new(),
                                parent_id: None,
                                status: OrderStatus::Creating,
                                symbol: opts.symbol.clone(),
                                kind,
                                side: if wire.b {
                                    OrderSide::Buy
                                } else {
                                    OrderSide::Sell
                                },
                                price,
                                amount,
                                filled: 0.0,
                                remaining: amount,
                                reduce_only: wire.r,
                                close: None,
                                client_order_id: Some(wire.c.clone()),
                                time_in_force: tif,
                                last_update: cx.now,
                                trigger_type: None,
                                take_profit: None,
                                stop_loss: None,
                                extra: None,
                            },
                        }
                    })
                    .collect::<Vec<_>>();
                let builder = self
                    .account
                    .builder_approved
                    .filter(|b| *b)
                    .map(|_| self.builder());
                Ok((
                    Action::Order {
                        orders: wires,
                        grouping,
                        builder,
                    },
                    Effect::Place(records),
                ))
            }
            Step::Cancel(orders) => {
                let cancels = orders
                    .iter()
                    .map(|o| {
                        Ok(CancelWire {
                            a: *self.assets.get(&o.symbol).ok_or("Unknown cancel market")?,
                            o: o.id.parse().map_err(|_| "Invalid Hyperliquid order ID")?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                Ok((Action::Cancel { cancels }, Effect::Cancel(orders)))
            }
            Step::Leverage {
                symbol,
                leverage,
                isolated,
            } => {
                if self.settings_uncertain(&symbol) {
                    return Err(
                        "Hyperliquid settings are uncertain; wait for account reconciliation"
                            .into(),
                    );
                }
                let market = cx.store.market(&symbol).ok_or("Market not found")?;
                // Frame-fed defaults come from the source's settings, never a local seed.
                if cx.frame_fed() && self.has_setting(market) && cx.store.setting(&symbol).is_none()
                {
                    return Err(
                        "Hyperliquid settings have not arrived from the data source yet".into(),
                    );
                }
                let isolated_only = self.isolated_only.contains(&symbol);
                // HIP-3 assets have no activeAssetData feed, so a flat one has no setting.
                let setting = cx.store.setting(&symbol).filter(|s| s.leverage > 0.0);
                let leverage = leverage.unwrap_or_else(|| {
                    setting.map_or(20f64.min(market.limits.leverage.max), |s| s.leverage)
                });
                let isolated = isolated.unwrap_or_else(|| {
                    setting.map_or(isolated_only || market.id.contains(':'), |s| {
                        s.margin_type == Some(MarginType::Isolated)
                    })
                });
                if isolated_only && !isolated {
                    return Err(format!(
                        "{symbol} supports isolated margin only on Hyperliquid"
                    ));
                }
                if !leverage.is_finite()
                    || leverage < 1.0
                    || leverage > market.limits.leverage.max
                    || leverage.fract() != 0.0
                {
                    return Err("Invalid Hyperliquid leverage".into());
                }
                let asset = *self.assets.get(&symbol).ok_or("Unknown leverage asset")?;
                Ok((
                    Action::Leverage {
                        asset,
                        is_cross: !isolated,
                        leverage: leverage as u32,
                    },
                    Effect::Leverage {
                        symbol,
                        leverage,
                        isolated,
                    },
                ))
            }
        }
    }
    pub(crate) fn builder_result(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(timer) = self.trading.builder_wait.take() {
            cx.clear_timer(timer);
            self.release_trade_builder(cx);
        }
    }
    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        result: Result<Response<'_>, &str>,
    ) {
        if !self.trade_valid(cx, &request) {
            return;
        }
        self.trading.inflight = false;
        if request.effect.is_schedule_control() {
            let outcome = schedule::reply(result);
            let reported = match &outcome {
                Ok(()) => self.report_trade_batch(cx, &request, &[], "", false),
                Err(error) => self.report_trade_failure(
                    cx,
                    &request,
                    &ReplyError {
                        message: error.message.clone(),
                        uncertain: error.outcome != Some(Outcome::Rejected),
                    },
                    error.venue_message.as_deref(),
                ),
            };
            if let Err(error) = reported {
                self.finish_trade(cx, Err(error.venue_error()));
                return;
            }
            self.finish_trade(cx, outcome);
            return;
        }

        let idempotent_cancel = self.trading.active.as_ref().unwrap().amend_source.is_none();
        let status = result.as_ref().ok().map(|r| r.status);
        let parsed = result.map_err(ReplyError::uncertain).and_then(|r| {
            if (200..300).contains(&r.status) {
                parse_reply(r.body)
            } else {
                Err(ReplyError::uncertain(format!(
                    "Hyperliquid HTTP {}: {}",
                    r.status, r.body
                )))
            }
        });
        let (kind, statuses) = match parsed {
            Ok(s) => s,
            Err(e) => {
                if idempotent_cancel
                    && request.effect.is_cancel()
                    && only_terminal_cancel(&e.message)
                {
                    let count = if let Effect::Cancel(orders) = &request.effect {
                        orders.len()
                    } else {
                        0
                    };
                    (
                        "cancel".into(),
                        (0..count).map(|_| Status::Text("success".into())).collect(),
                    )
                } else {
                    if let Err(error) = self.report_trade_failure(cx, &request, &e, None) {
                        self.finish_trade(cx, Err(error.venue_error()));
                        return;
                    }
                    self.restore_cancel(cx, &request.effect);
                    self.mark_settings_uncertain(cx, &request.effect);
                    self.schedule_orders(cx);
                    let error = VenueError::new(e.message);
                    let rejected =
                        |error: VenueError, kind| error.kind(kind).outcome(Outcome::Rejected);
                    let error = match status {
                        None => error.kind(ErrorKind::Network).outcome(Outcome::Unknown),
                        // A top-level `status: "err"` is Hyperliquid's explicit refusal.
                        Some(200..=299) if !e.uncertain => rejected(error, ErrorKind::Rejected),
                        Some(401) => rejected(error, ErrorKind::AuthInvalid),
                        Some(403) => rejected(error, ErrorKind::AuthPermission),
                        Some(429) => rejected(error, ErrorKind::RateLimited),
                        Some(400..=407 | 409..=499) => rejected(error, ErrorKind::Rejected),
                        _ => error.outcome(Outcome::Unknown),
                    };
                    self.finish_trade(cx, Err(error));
                    return;
                }
            }
        };
        let valid_shape = match &request.effect {
            Effect::ScheduleCancel => unreachable!(),
            Effect::Cancel(orders) => {
                kind == "cancel"
                    && statuses.len() == orders.len()
                    && statuses.iter().all(|s| {
                        matches!(s,Status::Text(t)if t=="success")
                            || matches!(s, Status::Error { .. })
                    })
            }
            Effect::Place(records) => {
                kind == "order"
                    && statuses.len() == records.len()
                    && (request.order_reply != OrderReply::Attached
                        || matches!(
                            statuses.first(),
                            Some(
                                Status::Resting { .. }
                                    | Status::Filled { .. }
                                    | Status::Error { .. }
                            )
                        ))
            }
            Effect::Leverage { .. } => kind == "default" && statuses.is_empty(),
        };
        if !valid_shape {
            let _ =
                cx.http_write_unproven(&request.context, "Invalid Hyperliquid command response");
            self.restore_cancel(cx, &request.effect);
            self.mark_settings_uncertain(cx, &request.effect);
            self.schedule_orders(cx);
            self.finish_trade(cx, Err(unproven("Invalid Hyperliquid command response")));
            return;
        }
        let errors = statuses
            .iter()
            .filter_map(|status| match status {
                Status::Error { error } => Some(error.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let accepted = statuses.iter().any(|status| match status {
            Status::Error { error } => {
                idempotent_cancel && kind == "cancel" && only_terminal_cancel(error)
            }
            _ => true,
        });
        let batch_error = if errors.is_empty()
            || (idempotent_cancel
                && request.effect.is_cancel()
                && errors.iter().all(|e| only_terminal_cancel(e)))
        {
            None
        } else {
            Some(
                VenueError::new(errors.join(", "))
                    .kind(ErrorKind::Rejected)
                    .outcome(if accepted {
                        Outcome::Unknown
                    } else {
                        Outcome::Rejected
                    }),
            )
        };
        if let Err(error) = self.report_trade_batch(
            cx,
            &request,
            &statuses,
            &errors.join(", "),
            idempotent_cancel,
        ) {
            self.finish_trade(cx, Err(error.venue_error()));
            return;
        }
        match request.effect {
            Effect::ScheduleCancel => unreachable!(),
            Effect::Place(records) => {
                self.trading.active.as_mut().unwrap().builder_checked = false;
                if statuses.is_empty() {
                    self.schedule_orders(cx);
                    self.finish_trade(cx, Err(unproven("Invalid response from Hyperliquid API")));
                    return;
                }
                for (index, status) in statuses.into_iter().enumerate() {
                    let (id, resting) = match status {
                        Status::Error { .. } => continue,
                        Status::Resting { resting } => (Some(resting.oid.to_string()), true),
                        Status::Filled { filled } => (Some(filled.oid.to_string()), false),
                        Status::Text(s) if s == "waitingForTrigger" || s == "waitingForFill" => {
                            (None, false)
                        }
                        _ => {
                            self.schedule_orders(cx);
                            self.finish_trade(
                                cx,
                                Err(unproven("Unknown Hyperliquid order status")),
                            );
                            return;
                        }
                    };
                    let Some(id) = id else {
                        self.schedule_orders(cx);
                        continue;
                    };
                    self.trading.active.as_mut().unwrap().ids.push(id.clone());
                    if let Some(mut record) = records.get(index).cloned() {
                        record.order.id = id.clone();
                        if self.spot {
                            self.apply_spot_ack_fills(&mut record.order);
                        }
                        if resting {
                            self.acknowledge_order_write(cx, &record.order.symbol);
                            if !record.order.symbol.contains('|') {
                                record.order.status = OrderStatus::Open;
                            }
                        }
                        if resting
                            && (!self.spot || record.order.remaining > 0.0)
                            && cx.store.order(&id).is_none()
                            && !self.account.suppressed.contains_key(&id)
                        {
                            cx.store.upsert_order(record.order.clone());
                        }
                        if record.order.symbol.contains('|') {
                            self.account.pending.insert(id.clone(), record);
                            self.trading.expected.insert(id);
                            if self.trading.retry.is_none() {
                                self.trading.retry_step = 0;
                                self.trading.retry = Some(cx.set_timer(300.0));
                            }
                        }
                    }
                }
            }
            Effect::Cancel(orders) => {
                for (o, status) in orders.into_iter().zip(statuses) {
                    if let Status::Error { error } = status
                        && !(idempotent_cancel && only_terminal_cancel(&error))
                    {
                        cx.store.update_order(&o.id, |live| {
                            if live.status == OrderStatus::Canceling {
                                live.status = o.status;
                            }
                        });
                        continue;
                    }
                    self.acknowledge_order_write(cx, &o.symbol);
                    self.account.suppressed.insert(o.id.clone(), cx.now);
                    cx.store.remove_orders(&[&o.id]);
                    self.account
                        .preplacements
                        .retain(|p| p.order.symbol != o.symbol || p.order.side != o.side);
                    self.account.pending.remove(&o.id);
                }
            }
            Effect::Leverage {
                symbol,
                leverage,
                isolated,
            } => {
                self.settings_reconciled(&symbol);
                self.acknowledge_settings(cx, &symbol);
                let margin = if isolated {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                };
                cx.store.update_setting(&symbol, |s| {
                    s.leverage = leverage;
                    s.margin_type = Some(margin);
                });
                cx.store.update_positions(|p| {
                    if p.symbol == symbol {
                        p.leverage = leverage;
                        p.margin_type = Some(margin);
                    }
                });
                // Frame-fed: hold the confirmed rails until the source re-reads them.
                cx.hold_setting(SettingKey::Leverage(symbol.clone()));
                cx.hold_setting(SettingKey::MarginType(symbol.clone()));
            }
        }
        if let Some(job) = self.trading.active.as_mut()
            && (batch_error.is_none() || accepted)
        {
            job.effective = true;
        }
        if let Some(error) = batch_error {
            self.schedule_orders(cx);
            self.finish_trade(cx, Err(error));
            return;
        }
        if self.spot {
            self.refresh_account(cx);
        }
        self.pump_trades(cx);
    }
    pub(crate) fn amend_source_filled(&mut self, id: &str) {
        for job in self
            .trading
            .active
            .iter_mut()
            .chain(self.trading.queue.iter_mut())
        {
            if job.amend_source.as_deref() == Some(id) {
                job.amend_filled = true;
            }
        }
    }
    fn mark_settings_uncertain(&mut self, cx: &mut Cx<'_, Pending>, effect: &Effect) {
        // Frame-fed settings come from the data source, which reconciles them.
        if let Effect::Leverage { symbol, .. } = effect
            && !cx.frame_fed()
        {
            self.trading.uncertain_settings.insert(symbol.clone());
            self.refresh_uncertain_settings(cx, symbol);
        }
    }
    pub(crate) fn reconcile_settings(&mut self, cx: &mut Cx<'_, Pending>) {
        for symbol in self
            .trading
            .uncertain_settings
            .iter()
            .cloned()
            .collect::<Vec<_>>()
        {
            self.refresh_uncertain_settings(cx, &symbol);
        }
    }
    pub(crate) fn settings_reconciled(&mut self, symbol: &str) {
        self.trading.uncertain_settings.remove(symbol);
    }
    pub(crate) fn settings_uncertain(&self, symbol: &str) -> bool {
        self.trading.uncertain_settings.contains(symbol)
    }
    fn restore_cancel(&self, cx: &mut Cx<'_, Pending>, effect: &Effect) {
        if let Effect::Cancel(orders) = effect {
            for o in orders {
                cx.store.update_order(&o.id, |live| {
                    if live.status == OrderStatus::Canceling {
                        live.status = o.status;
                    }
                });
            }
        }
    }
    fn finish_trade(&mut self, cx: &mut Cx<'_, Pending>, result: Result<(), VenueError>) {
        self.trading.inflight = false;
        if let Some(job) = self.trading.active.take() {
            if let Err(mut error) = result {
                error = error.partial_ids(&job.ids);
                if (job.effective || !job.ids.is_empty())
                    && matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent))
                {
                    error.outcome = Some(Outcome::Unknown);
                }
                self.finish_trade_owner(cx, &job, Err(error));
                self.pump_trades(cx);
                return;
            }
            let data = if let Some(client_order_id) = job.replacement.as_ref() {
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Replacement {
                    order_ids: Vec<String>,
                    client_order_id: String,
                }
                serde_json::to_string(&Replacement {
                    order_ids: job.ids.clone(),
                    client_order_id: client_order_id.clone(),
                })
                .unwrap()
            } else {
                serde_json::to_string(&job.ids).unwrap()
            };
            self.finish_trade_owner(cx, &job, Ok(data));
        }
        self.pump_trades(cx);
    }
    pub(crate) fn trade_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.trading.builder_wait == Some(timer) {
            self.trading.builder_wait = None;
            self.account.builder_approved = Some(false);
            self.release_trade_builder(cx);
        }
        if self.trading.retry == Some(timer) {
            self.trading.retry = None;
            self.trading.expected.retain(|id| {
                cx.store
                    .order(id)
                    .is_none_or(|o| o.status != OrderStatus::Open)
            });
            if !self.trading.expected.is_empty() {
                self.refresh_orders(cx);
                self.trading.retry_step += 1;
                if let Some(delay) =
                    [300.0, 700.0, 1500.0, 3000.0, 5000.0].get(self.trading.retry_step)
                {
                    self.trading.retry = Some(cx.set_timer(*delay));
                } else {
                    self.trading.expected.clear();
                }
            }
        }
    }
}
struct ReplyError {
    message: String,
    uncertain: bool,
}
impl ReplyError {
    fn uncertain(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            uncertain: true,
        }
    }
}
fn parse_reply(raw: &str) -> Result<(String, Vec<Status>), ReplyError> {
    let reply = serde_json::from_str::<Reply>(raw)
        .map_err(|_| ReplyError::uncertain("Invalid response from Hyperliquid API"))?;
    if reply.status == "err" {
        let message = serde_json::from_str::<String>(reply.response.get())
            .map_err(|_| ReplyError::uncertain("Invalid Hyperliquid rejection response"))?;
        return Err(ReplyError {
            message,
            uncertain: false,
        });
    }
    if reply.status != "ok" {
        return Err(ReplyError::uncertain("Invalid Hyperliquid response status"));
    }
    let data = serde_json::from_str::<ReplyData>(reply.response.get())
        .map_err(|_| ReplyError::uncertain("Invalid Hyperliquid response data"))?;
    Ok((
        data.r#type,
        data.data.map_or_else(Vec::new, |data| data.statuses),
    ))
}
fn only_terminal_cancel(error: &str) -> bool {
    let prefix = "Order was never placed, already canceled, or filled";
    error.strip_prefix(prefix).is_some_and(|s| {
        s.trim_start_matches('.').trim().is_empty()
            || s.trim_start_matches('.')
                .trim()
                .strip_prefix("asset=")
                .is_some_and(|s| s.bytes().all(|b| b.is_ascii_digit()))
    })
}
fn mint_cloid(cx: &Cx<'_, Pending>) -> Result<String, String> {
    let mut bytes = [0u8; 16];
    cx.fill_entropy(&mut bytes)?;
    Ok(format!("0x{}", safe_cex_crypto::hex(&bytes)))
}
fn float_wire(value: f64) -> Result<String, String> {
    if !value.is_finite() {
        return Err("Nonfinite order number".into());
    }
    let text = format!("{value:.8}");
    if (text.parse::<f64>().unwrap() - value).abs() >= 1e-9 {
        return Err("Order number exceeds wire precision".into());
    }
    let text = text.trim_end_matches('0').trim_end_matches('.');
    Ok(if text == "-0" {
        "0".into()
    } else {
        text.into()
    })
}
fn price_wire(price: f64, amount_step: f64, spot: bool) -> Result<String, String> {
    if !price.is_finite() || price <= 0.0 {
        return Err("Invalid order price".into());
    }
    let significant_step = 10f64.powi((price.log10().floor() as i32 - 4).min(0));
    let significant = num::adjust(price, significant_step);
    let decimals = (if spot { 8 } else { 6 }) - num::decimals(amount_step) as i32;
    float_wire(num::adjust(significant, 10f64.powi(-decimals)))
}

/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

/// A 2xx answer that does not prove what happened.
fn unproven(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::Unknown)
}

#[cfg(test)]
mod signing_tests {
    use super::*;
    #[test]
    fn schedule_encoding_hash_and_signature_match_independent_ts_vectors() {
        #[derive(serde::Deserialize)]
        struct Vector {
            mainnet: bool,
            vault: Option<String>,
            time: Option<u64>,
            nonce: u64,
            msgpack: String,
            hash: String,
            signature: SignatureVector,
        }
        #[derive(serde::Deserialize)]
        struct SignatureVector {
            r: String,
            s: String,
            v: u8,
        }
        let vectors: Vec<Vector> = serde_json::from_str(include_str!(
            "../../../../packages/safe-cex/src/__tests__/fixtures/hyperliquidScheduleCancel.json"
        ))
        .unwrap();
        for row in vectors {
            let action = Action::ScheduleCancel { time: row.time };
            let nonce = row.nonce;
            let vault = row.vault.as_deref();
            assert_eq!(
                format!(
                    "0x{}",
                    safe_cex_crypto::hex(&rmp_serde::to_vec_named(&action).unwrap())
                ),
                row.msgpack
            );
            assert_eq!(
                format!(
                    "0x{}",
                    safe_cex_crypto::hex(&sign::action_hash(&action, vault, nonce).unwrap())
                ),
                row.hash
            );
            let signature = sign::sign(
                &action,
                vault,
                nonce,
                row.mainnet,
                &format!("0x{}", "11".repeat(32)),
            )
            .unwrap();
            assert_eq!(signature.r, row.signature.r);
            assert_eq!(signature.s, row.signature.s);
            assert_eq!(signature.v, row.signature.v);
        }
    }

    // Official Python SDK test_l1_action_signing_order_with_cloid_matches.
    #[test]
    fn official_order_with_cloid_signature_pins_msgpack_key_order() {
        let action = Action::Order {
            orders: vec![OrderWire {
                a: 1,
                b: true,
                p: "100".into(),
                s: "100".into(),
                r: false,
                t: WireType::Limit {
                    limit: LimitWire { tif: "Gtc" },
                },
                c: "0x00000000000000000000000000000001".into(),
            }],
            grouping: "na",
            builder: None,
        };
        let s = sign::sign(
            &action,
            None,
            0,
            true,
            "0x0123456789012345678901234567890123456789012345678901234567890123",
        )
        .unwrap();
        assert_eq!(
            s.r,
            "0x041ae18e8239a56cacbc5dad94d45d0b747e5da11ad564077fcac71277a946e3"
        );
        assert_eq!(
            s.s,
            "0x3c61f667e747404fe7eea8f90ab0e76cc12ce60270438b2058324681a00116da"
        );
        assert_eq!(s.v, 27);
    }
    #[test]
    fn leverage_is_an_integer_in_signed_msgpack() {
        let action = Action::Leverage {
            asset: 0,
            is_cross: true,
            leverage: 20,
        };
        let mut expected = vec![0x84, 0xa4];
        expected.extend_from_slice(b"type");
        expected.push(0xae);
        expected.extend_from_slice(b"updateLeverage");
        expected.push(0xa5);
        expected.extend_from_slice(b"asset");
        expected.extend_from_slice(&[0, 0xa7]);
        expected.extend_from_slice(b"isCross");
        expected.extend_from_slice(&[0xc3, 0xa8]);
        expected.extend_from_slice(b"leverage");
        expected.push(20);
        assert_eq!(rmp_serde::to_vec_named(&action).unwrap(), expected);
    }

    /// Dispatch encodes msgpack from the action's JSON; it must equal rmp_serde's
    /// struct encoding for every wire shape, the official vectors included.
    #[test]
    fn dispatch_msgpack_from_json_equals_rmp_serde_for_every_action() {
        let mut actions = remote_actions();
        actions.push(Action::Order {
            orders: vec![OrderWire {
                a: 1,
                b: true,
                p: "100".into(),
                s: "100".into(),
                r: false,
                t: WireType::Limit {
                    limit: LimitWire { tif: "Gtc" },
                },
                c: "0x00000000000000000000000000000001".into(),
            }],
            grouping: "na",
            builder: None,
        });
        actions.push(Action::Leverage {
            asset: 0,
            is_cross: true,
            leverage: 20,
        });
        actions.push(Action::ScheduleCancel { time: None });
        for action in &actions {
            let json = serde_json::to_string(action).unwrap();
            assert_eq!(
                sign::statement::msgpack(&json).unwrap(),
                rmp_serde::to_vec_named(action).unwrap(),
                "{json}"
            );
        }
    }

    fn remote_actions() -> Vec<Action> {
        let order = |t: WireType, c: &str| OrderWire {
            a: 10_000,
            b: false,
            p: "0.000123".into(),
            s: "1234.5".into(),
            r: true,
            t,
            c: c.into(),
        };
        vec![
            Action::Order {
                orders: vec![
                    order(
                        WireType::Limit {
                            limit: LimitWire { tif: "Alo" },
                        },
                        "0x0000000000000000000000000000abcd",
                    ),
                    order(
                        WireType::Trigger {
                            trigger: TriggerWire {
                                is_market: true,
                                trigger_px: "99999.5".into(),
                                tpsl: "sl",
                            },
                        },
                        "0xffffffffffffffffffffffffffffffff",
                    ),
                ],
                grouping: "normalTpsl",
                builder: Some(crate::Builder {
                    b: "0x1719884eb866cb12b2287399b15f7db5e7d775ea".into(),
                    f: 25,
                }),
            },
            Action::Cancel {
                cancels: vec![
                    CancelWire { a: 0, o: 1 },
                    CancelWire {
                        a: 4_294_967_295,
                        o: u64::MAX,
                    },
                ],
            },
            Action::Leverage {
                asset: 300,
                is_cross: false,
                leverage: 3,
            },
            Action::ScheduleCancel {
                time: Some(1_700_000_000_000),
            },
            Action::ScheduleCancel { time: None },
        ]
    }

    /// A remote device re-derives exactly the digest local signing signs, so a
    /// deterministic signature and the body built from it are byte-identical.
    #[test]
    fn remote_statement_derives_the_local_digest_for_every_issued_action() {
        let vault = "0x1719884eb866cb12b2287399b15f7db5e7d775ea";
        for action in remote_actions() {
            let json = serde_json::value::to_raw_value(&action).unwrap();
            for (vault, mainnet) in [
                (None, true),
                (None, false),
                (Some(vault), true),
                (Some(vault), false),
            ] {
                for nonce in [0, 1_700_000_000_123, u64::MAX] {
                    let statement = sign::statement::encode(&json, nonce, vault, mainnet).unwrap();
                    let local = sign::agent_digest(
                        sign::action_hash(&action, vault, nonce).unwrap(),
                        mainnet,
                    );
                    let account = sign::statement::Account { mainnet, vault };
                    assert_eq!(sign::statement::derive(&statement, &account), Ok(local));
                    // A device on the other network or vault never signs it.
                    let other = sign::statement::Account {
                        mainnet: !mainnet,
                        vault,
                    };
                    assert!(sign::statement::derive(&statement, &other).is_err());
                    let other = sign::statement::Account {
                        mainnet,
                        vault: if vault.is_some() {
                            None
                        } else {
                            Some(vault_other())
                        },
                    };
                    assert!(sign::statement::derive(&statement, &other).is_err());
                }
            }
        }
    }
    fn vault_other() -> &'static str {
        "0x0000000000000000000000000000000000000001"
    }

    #[test]
    fn remote_statement_refuses_fund_movement_and_unknown_shapes() {
        let account = sign::statement::Account {
            mainnet: true,
            vault: None,
        };
        let statement = |action: &str| {
            format!(r#"{{"kind":"l1","action":{action},"nonce":1,"isMainnet":true}}"#)
        };
        for action in [
            r#"{"type":"withdraw3","hyperliquidChain":"Mainnet","signatureChainId":"0xa4b1","amount":"1","time":1,"destination":"0x0000000000000000000000000000000000000001"}"#,
            r#"{"type":"usdSend","hyperliquidChain":"Mainnet","signatureChainId":"0xa4b1","amount":"1","time":1,"destination":"0x0000000000000000000000000000000000000001"}"#,
            r#"{"type":"approveAgent","agentAddress":"0x0000000000000000000000000000000000000001","nonce":1}"#,
            r#"{"type":"vaultTransfer","vaultAddress":"0x0000000000000000000000000000000000000001","isDeposit":false,"usd":1}"#,
            r#"{"type":"order","type":"cancel","cancels":[]}"#,
            r#"{"cancels":[]}"#,
            r#"["order"]"#,
        ] {
            assert!(
                sign::statement::derive(&statement(action), &account).is_err(),
                "{action}"
            );
        }
        let allowed = statement(r#"{"type":"cancel","cancels":[{"a":0,"o":1}]}"#);
        assert!(sign::statement::derive(&allowed, &account).is_ok());
        for refused in [
            allowed.replace(r#""kind":"l1""#, r#""kind":"user""#),
            allowed.replace(r#""nonce":1"#, r#""nonce":1,"expiresAfter":2"#),
            allowed.replace(r#""nonce":1"#, r#""nonce":1,"extra":true"#),
            allowed.replace(r#""nonce":1"#, r#""nonce":-1"#),
        ] {
            assert!(
                sign::statement::derive(&refused, &account).is_err(),
                "{refused}"
            );
        }
    }
}
