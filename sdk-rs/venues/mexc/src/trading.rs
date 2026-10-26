//! Commands are FIFO jobs; every accepted write advances exactly once.
use crate::{
    MexcFutures, Pending,
    account::{Rails, Scope},
    wire::*,
};
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Method, Outcome, Response, TimerId, VenueError, num,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

const SUBMIT: &str = "/api/v1/private/order/submit";
const BATCH: &str = "/api/v1/private/order/submit_batch";
const PLAN: &str = "/api/v1/private/planorder/place/v2";
const STOP: &str = "/api/v1/private/stoporder/place";

fn local_mutation_policy() -> Policy {
    Policy {
        operation: Operation::Mutation,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

pub struct Request {
    pub command: CommandId,
    pub call: Call,
    pub revision: u64,
}
#[derive(Clone)]
pub enum Call {
    Place {
        opts: PlaceOrderOpts,
        scope: Scope,
        amend: Option<AmendIntent>,
    },
    Batch {
        orders: Vec<PlaceOrderOpts>,
    },
    Cancel {
        ids: Vec<String>,
        regular: bool,
    },
    Leverage {
        symbol: String,
        position_type: u8,
        leverage: f64,
        isolated: bool,
        keep_leverage: bool,
        keep_margin: bool,
    },
    Mode {
        hedged: bool,
    },
    AmendRead {
        opts: PlaceOrderOpts,
        intent: AmendIntent,
        attempt: u8,
    },
}
#[derive(Clone)]
pub struct AmendIntent {
    id: String,
    amount: Option<f64>,
}
pub struct Step {
    receipts: Vec<OrderIdentityReceipt>,
    path: &'static str,
    body: String,
    call: Call,
}
pub struct Job {
    command: CommandId,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    gap: f64,
    total_orders: usize,
    failed_index: Option<usize>,
    /// A step took effect, so a later failure cannot mean nothing happened.
    effective: bool,
}
#[derive(Default)]
pub struct Trading {
    queue: VecDeque<Job>,
    active: Option<Job>,
    waiting: bool,
    pub timer: Option<TimerId>,
    next_submit: f64,
    batch_denied: bool,
    read_retry: Option<(TimerId, Request)>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Regular<'a> {
    symbol: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<String>,
    vol: f64,
    side: u8,
    #[serde(rename = "type")]
    kind: u8,
    open_type: u8,
    leverage: f64,
    external_oid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reduce_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stp_mode: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_loss_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    take_profit_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    loss_trend: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    profit_trend: Option<u8>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResultRow<'a> {
    #[serde(borrow, default, deserialize_with = "id")]
    order_id: Str<'a>,
    #[serde(borrow, default, deserialize_with = "id")]
    id: Str<'a>,
    #[serde(default, deserialize_with = "number")]
    error_code: f64,
    #[serde(borrow, default, alias = "message", alias = "msg")]
    error_msg: Str<'a>,
}
fn result_id(payload: &str) -> Result<String, String> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum IdResult<'a> {
        Row(#[serde(borrow)] ResultRow<'a>),
        Scalar(#[serde(borrow, deserialize_with = "id")] Str<'a>),
        Rows(#[serde(borrow)] Vec<ResultRow<'a>>),
    }
    let result: IdResult<'_> =
        serde_json::from_str(payload).map_err(|e| format!("MEXC placement: {e}"))?;
    let id = match result {
        IdResult::Scalar(s) => s.to_string(),
        IdResult::Row(r) => {
            if r.order_id.is_empty() {
                r.id.to_string()
            } else {
                r.order_id.to_string()
            }
        }
        IdResult::Rows(r) => r
            .first()
            .map(|r| {
                if r.id.is_empty() {
                    r.order_id.to_string()
                } else {
                    r.id.to_string()
                }
            })
            .unwrap_or_default(),
    };
    if id.is_empty() {
        Err("MEXC order placement returned no order id".into())
    } else {
        Ok(id)
    }
}
fn trend(t: Option<StopTriggerType>) -> u8 {
    match t {
        Some(StopTriggerType::MarkPrice) => 2,
        Some(StopTriggerType::IndexPrice) => 3,
        _ => 1,
    }
}
fn native_side(side: OrderSide, reduce: bool) -> u8 {
    match (side, reduce) {
        (OrderSide::Buy, false) => 1,
        (OrderSide::Buy, true) => 2,
        (OrderSide::Sell, false) => 3,
        (OrderSide::Sell, true) => 4,
    }
}
fn json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| e.to_string())
}

impl MexcFutures {
    fn order_rails(&self, symbol: &str) -> Rails {
        self.account.rails.get(symbol).copied().unwrap_or(Rails {
            long: 20.0,
            short: 20.0,
            isolated: false,
        })
    }
    fn leverage_for(&self, opts: &PlaceOrderOpts) -> f64 {
        let r = self.order_rails(&opts.symbol);
        if (opts.side == OrderSide::Buy) != opts.reduce_only.unwrap_or(false) {
            r.long
        } else {
            r.short
        }
    }
    fn regular_body(
        &mut self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
        batch: bool,
    ) -> Result<(String, OrderIdentityReceipt), String> {
        let m = cx
            .store
            .market(&opts.symbol)
            .ok_or_else(|| format!("Market {} not found", opts.symbol))?;
        let step = self.volume_units.get(&opts.symbol).copied().unwrap_or(1.0);
        let vol = num::adjust(opts.amount, step).max(step);
        let kind = if opts.kind == OrderType::Market {
            5
        } else {
            match opts.time_in_force {
                Some(TimeInForce::PostOnly) => 2,
                Some(TimeInForce::ImmediateOrCancel) => 3,
                Some(TimeInForce::FillOrKill) => 4,
                _ => 1,
            }
        };
        let payload = Regular {
            symbol: &m.id,
            price: if opts.kind == OrderType::Market {
                None
            } else {
                Some(num::format(num::adjust(
                    opts.price.unwrap_or(0.0),
                    m.precision.price,
                )))
            },
            vol,
            side: native_side(opts.side, opts.reduce_only.unwrap_or(false)),
            kind,
            open_type: if self.order_rails(&opts.symbol).isolated {
                1
            } else {
                2
            },
            leverage: num::clamp_leverage(
                self.leverage_for(opts),
                m.limits.leverage.min,
                m.limits.leverage.max,
            )?,
            external_oid: format!(
                "ts_{}",
                safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))?
            ),
            reduce_only: opts.reduce_only.filter(|v| *v),
            stp_mode: batch.then_some(0),
            stop_loss_price: opts
                .stop_loss
                .map(|v| num::format(num::adjust(v, m.precision.price))),
            take_profit_price: opts
                .take_profit
                .map(|v| num::format(num::adjust(v, m.precision.price))),
            loss_trend: opts
                .stop_loss
                .map(|_| trend(opts.sl_trigger_type.or(opts.trigger_type))),
            profit_trend: opts
                .take_profit
                .map(|_| trend(opts.tp_trigger_type.or(opts.trigger_type))),
        };
        let receipt = OrderIdentityReceipt {
            symbol: opts.symbol.clone(),
            kind: OrderIdentityKind::ClientOrderId,
            identity: payload.external_oid.clone(),
        };
        Ok((json(&payload)?, receipt))
    }
    fn placement(
        &mut self,
        cx: &Cx<'_, Pending>,
        mut opts: PlaceOrderOpts,
    ) -> Result<Step, String> {
        let m = cx
            .store
            .market(&opts.symbol)
            .ok_or_else(|| format!("Market {} not found", opts.symbol))?;
        if matches!(
            opts.kind,
            OrderType::Conditional | OrderType::TrailingStopLoss
        ) {
            return Err(
                "MEXC opening conditional and trailing-stop placement is not implemented".into(),
            );
        }
        let stop = matches!(opts.kind, OrderType::StopLoss | OrderType::TakeProfit);
        if !opts.amount.is_finite() || opts.amount < 0.0 || (!stop && opts.amount == 0.0) {
            return Err("MEXC order requires a positive native amount".into());
        }
        for value in [
            opts.price,
            opts.trigger_price,
            opts.stop_loss,
            opts.take_profit,
        ]
        .into_iter()
        .flatten()
        {
            if !value.is_finite() || value <= 0.0 {
                return Err("MEXC order requires a positive price".into());
            }
        }
        if !stop {
            if opts.kind == OrderType::Limit && opts.price.is_none() {
                return Err("MEXC limit order requires a positive price".into());
            }
            let (body, receipt) = self.regular_body(cx, &opts, false)?;
            return Ok(Step {
                receipts: vec![receipt],
                path: SUBMIT,
                body,
                call: Call::Place {
                    opts,
                    scope: Scope::Regular,
                    amend: None,
                },
            });
        }
        opts.reduce_only = Some(true);
        let trigger = opts
            .trigger_price
            .or(opts.price)
            .filter(|v| *v > 0.0)
            .ok_or_else(|| "MEXC stop requires a positive trigger price".to_owned())?;
        let trigger_price = num::format(num::adjust(trigger, m.precision.price));
        let target = if opts.side == OrderSide::Sell {
            PositionSide::Long
        } else {
            PositionSide::Short
        };
        if opts.amount == 0.0 {
            let p = cx
                .store
                .positions()
                .iter()
                .find(|p| p.symbol == opts.symbol && p.side == target && p.contracts > 0.0)
                .ok_or_else(|| "MEXC full-position stop requires an open position".to_owned())?;
            let position_id = self.account.native_ids.get(&p.id).ok_or_else(|| {
                "MEXC full-position stop requires a native position id".to_owned()
            })?;
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Stop<'a> {
                position_id: &'a str,
                vol: f64,
                loss_trend: u8,
                profit_trend: u8,
                #[serde(skip_serializing_if = "Option::is_none")]
                stop_loss_price: Option<String>,
                #[serde(skip_serializing_if = "Option::is_none")]
                take_profit_price: Option<String>,
                profit_loss_vol_type: &'static str,
                vol_type: u8,
                #[serde(skip_serializing_if = "Option::is_none")]
                take_profit_type: Option<u8>,
                #[serde(skip_serializing_if = "Option::is_none")]
                stop_loss_type: Option<u8>,
            }
            let take = opts.kind == OrderType::TakeProfit;
            let vol = num::adjust(
                p.contracts / m.contract_size.unwrap_or(1.0),
                self.volume_units.get(&opts.symbol).copied().unwrap_or(1.0),
            );
            if !vol.is_finite() || vol <= 0.0 {
                return Err("MEXC full-position stop requires a positive native volume".into());
            }
            let body = json(&Stop {
                position_id,
                vol,
                loss_trend: if take {
                    1
                } else {
                    trend(opts.sl_trigger_type.or(opts.trigger_type))
                },
                profit_trend: if take {
                    trend(opts.tp_trigger_type.or(opts.trigger_type))
                } else {
                    1
                },
                stop_loss_price: (!take).then(|| trigger_price.clone()),
                take_profit_price: take.then_some(trigger_price),
                profit_loss_vol_type: "SAME",
                vol_type: 2,
                take_profit_type: take.then_some(0),
                stop_loss_type: (!take).then_some(0),
            })?;
            return Ok(Step {
                receipts: Vec::new(),
                path: STOP,
                body,
                call: Call::Place {
                    opts,
                    scope: Scope::Stop,
                    amend: None,
                },
            });
        }
        let vol = num::adjust(
            opts.amount,
            self.volume_units.get(&opts.symbol).copied().unwrap_or(1.0),
        );
        if vol <= 0.0 || vol * m.contract_size.unwrap_or(1.0) > m.limits.amount.max {
            return Err("MEXC stop amount exceeds native volume limits".into());
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Plan<'a> {
            symbol: &'a str,
            vol: f64,
            leverage: f64,
            side: u8,
            open_type: u8,
            trigger_price: String,
            trigger_type: u8,
            execute_cycle: u8,
            order_type: u8,
            trend: u8,
            position_mode: u8,
            reduce_only: bool,
        }
        let take = opts.kind == OrderType::TakeProfit;
        let gte = (take && opts.side == OrderSide::Sell) || (!take && opts.side == OrderSide::Buy);
        let body = json(&Plan {
            symbol: &m.id,
            vol,
            leverage: num::clamp_leverage(
                self.leverage_for(&opts),
                m.limits.leverage.min,
                m.limits.leverage.max,
            )?,
            side: native_side(opts.side, true),
            open_type: if self.order_rails(&opts.symbol).isolated {
                1
            } else {
                2
            },
            trigger_price,
            trigger_type: if gte { 1 } else { 2 },
            execute_cycle: 2,
            order_type: 5,
            trend: trend(
                (if take {
                    opts.tp_trigger_type
                } else {
                    opts.sl_trigger_type
                })
                .or(opts.trigger_type),
            ),
            position_mode: if cx.store.hedged() { 1 } else { 2 },
            reduce_only: true,
        })?;
        Ok(Step {
            receipts: Vec::new(),
            path: PLAN,
            body,
            call: Call::Place {
                opts,
                scope: Scope::Plan,
                amend: None,
            },
        })
    }
    fn place_steps(
        &mut self,
        cx: &Cx<'_, Pending>,
        orders: Vec<PlaceOrderOpts>,
        allow_batch: bool,
    ) -> Result<VecDeque<Step>, String> {
        if orders.iter().any(|order| {
            order.bybit_bbo.is_some() || order.bybit_slippage_tolerance_ticks.is_some()
        }) {
            return Err("Bybit BBO and TickSize controls are not supported by MEXC".into());
        }
        if orders.iter().any(|o| o.client_order_id.is_some()) {
            return Err("mexc cannot honor a caller clientOrderId".into());
        }
        let steps: VecDeque<_> = orders
            .iter()
            .cloned()
            .map(|o| self.placement(cx, o))
            .collect::<Result<_, _>>()?;
        if allow_batch
            && !orders.is_empty()
            && !self.trading.batch_denied
            && orders
                .iter()
                .all(|o| matches!(o.kind, OrderType::Limit | OrderType::Market))
        {
            let mut batch = VecDeque::new();
            for orders in orders.chunks(50) {
                let (bodies, receipts): (Vec<_>, Vec<_>) = orders
                    .iter()
                    .map(|o| self.regular_body(cx, o, true))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .unzip();
                batch.push_back(Step {
                    receipts,
                    path: BATCH,
                    body: format!("[{}]", bodies.join(",")),
                    call: Call::Batch {
                        orders: orders.to_vec(),
                    },
                });
            }
            Ok(batch)
        } else {
            Ok(steps)
        }
    }
    fn cancel_steps(&self, cx: &Cx<'_, Pending>, ids: &[String]) -> Result<VecDeque<Step>, String> {
        let mut groups: [Vec<&Order>; 3] = Default::default();
        for id in ids {
            let o = cx
                .store
                .order(id)
                .ok_or_else(|| format!("MEXC order {id} not found"))?;
            let group = if self.account.stop_ids.contains(id) {
                0
            } else if self.account.plan_ids.contains(id) {
                1
            } else {
                2
            };
            groups[group].push(o);
        }
        let mut steps = VecDeque::new();
        for (group, orders) in groups.iter().enumerate() {
            for orders in orders.chunks(50) {
                let ids: Vec<_> = orders.iter().map(|o| o.id.clone()).collect();
                let (path, body) = match group {
                    0 => {
                        #[derive(Serialize)]
                        #[serde(rename_all = "camelCase")]
                        struct C<'a> {
                            stop_plan_order_id: &'a str,
                        }
                        (
                            "/api/v1/private/stoporder/cancel",
                            json(
                                &ids.iter()
                                    .map(|id| C {
                                        stop_plan_order_id: id,
                                    })
                                    .collect::<Vec<_>>(),
                            )?,
                        )
                    }
                    1 => {
                        #[derive(Serialize)]
                        #[serde(rename_all = "camelCase")]
                        struct C<'a> {
                            symbol: &'a str,
                            order_id: &'a str,
                        }
                        (
                            "/api/v1/private/planorder/cancel",
                            json(
                                &orders
                                    .iter()
                                    .map(|o| C {
                                        symbol: &cx.store.market(&o.symbol).unwrap().id,
                                        order_id: &o.id,
                                    })
                                    .collect::<Vec<_>>(),
                            )?,
                        )
                    }
                    _ => ("/api/v1/private/order/cancel", json(&ids)?),
                };
                steps.push_back(Step {
                    receipts: Vec::new(),
                    path,
                    body,
                    call: Call::Cancel {
                        ids,
                        regular: group == 2,
                    },
                });
            }
        }
        Ok(steps)
    }
    fn settings_steps(
        &self,
        cx: &Cx<'_, Pending>,
        symbol: &str,
        leverage: Option<f64>,
        isolated: Option<bool>,
        selected_side: Option<PositionSide>,
    ) -> Result<VecDeque<Step>, String> {
        let m = cx
            .store
            .market(symbol)
            .ok_or_else(|| format!("Market {symbol} not found"))?;
        let r = self.order_rails(symbol);
        let keep_leverage = leverage.is_none();
        let keep_margin = isolated.is_none();
        let mut steps = VecDeque::new();
        for position_type in [1, 2] {
            let side = if position_type == 1 {
                PositionSide::Long
            } else {
                PositionSide::Short
            };
            if selected_side.is_some_and(|selected| selected != side) {
                continue;
            }
            let leverage = num::clamp_leverage(
                leverage.unwrap_or(if position_type == 1 { r.long } else { r.short }),
                m.limits.leverage.min,
                m.limits.leverage.max,
            )?;
            let next_isolated = isolated.unwrap_or(r.isolated);
            let native = cx
                .store
                .positions()
                .iter()
                .find(|p| p.symbol == symbol && p.side == side && p.contracts > 0.0)
                .and_then(|p| self.account.native_ids.get(&p.id));
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct Change<'a> {
                #[serde(skip_serializing_if = "Option::is_none")]
                position_id: Option<&'a str>,
                #[serde(skip_serializing_if = "Option::is_none")]
                symbol: Option<&'a str>,
                #[serde(skip_serializing_if = "Option::is_none")]
                position_type: Option<u8>,
                leverage: f64,
                #[serde(skip_serializing_if = "Option::is_none")]
                open_type: Option<u8>,
            }
            let body = json(&Change {
                position_id: native.map(String::as_str),
                symbol: native.is_none().then_some(&m.id),
                position_type: native.is_none().then_some(position_type),
                leverage,
                open_type: if native.is_none() || isolated.is_some() {
                    Some(if next_isolated { 1 } else { 2 })
                } else {
                    None
                },
            })?;
            steps.push_back(Step {
                receipts: Vec::new(),
                path: "/api/v1/private/position/change_leverage",
                body,
                call: Call::Leverage {
                    symbol: symbol.into(),
                    position_type,
                    leverage,
                    isolated: next_isolated,
                    keep_leverage,
                    keep_margin,
                },
            });
        }
        Ok(steps)
    }
    pub fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let total_orders = match &command {
            Command::PlaceOrders { orders, .. } => orders.len(),
            Command::AmendOrder { .. } => 1,
            _ => 0,
        };
        let (command, steps, gap) = match command {
            Command::PlaceOrders { command, orders } => {
                let multiple = orders.len() > 1;
                let result = self.place_steps(cx, orders, multiple);
                (command, result, if multiple { 1500.0 } else { 450.0 })
            }
            Command::CancelOrders { command, ids } => (command, self.cancel_steps(cx, &ids), 0.0),
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => {
                let result = (|| {
                    let o = cx
                        .store
                        .order(&id)
                        .ok_or_else(|| format!("MEXC order {id} not found"))?;
                    if o.status != OrderStatus::Open {
                        return Err("MEXC only amends open orders".into());
                    }
                    let opts = PlaceOrderOpts {
                        symbol: o.symbol.clone(),
                        kind: o.kind,
                        side: o.side,
                        amount: amount.unwrap_or_else(|| {
                            o.extra.as_ref().and_then(|e| e.remaining_native).unwrap_or(
                                o.remaining
                                    / cx.store
                                        .market(&o.symbol)
                                        .and_then(|m| m.contract_size)
                                        .unwrap_or(1.0),
                            )
                        }),
                        price: price.or(Some(o.price)),
                        trigger_price: None,
                        reduce_only: Some(o.reduce_only),
                        time_in_force: o.time_in_force,
                        trigger_type: o.trigger_type,
                        tp_trigger_type: o.extra.as_ref().and_then(|e| e.tp_trigger_type),
                        sl_trigger_type: o.extra.as_ref().and_then(|e| e.sl_trigger_type),
                        stop_loss: o.stop_loss,
                        take_profit: o.take_profit,
                        client_order_id: None,
                        close: o.close,
                        stp_mode: None,
                        max_slippage_pct: None,
                        bybit_bbo: None,
                        bybit_slippage_tolerance_ticks: None,
                        implied_volatility: None,
                    };
                    let mut replacement = self.placement(cx, opts)?;
                    if let Call::Place { amend, .. } = &mut replacement.call {
                        *amend = Some(AmendIntent {
                            id: id.clone(),
                            amount,
                        });
                    }
                    let mut steps = self.cancel_steps(cx, &[id])?;
                    steps.push_back(replacement);
                    Ok(steps)
                })();
                (command, result, 450.0)
            }
            Command::CancelSymbolOrders { command, symbol } => {
                let result = (|| {
                    let m = cx
                        .store
                        .market(&symbol)
                        .ok_or_else(|| format!("Market {symbol} not found"))?;
                    let regular: Vec<_> = cx
                        .store
                        .orders()
                        .iter()
                        .filter(|o| {
                            o.symbol == symbol
                                && !self.account.stop_ids.contains(&o.id)
                                && !self.account.plan_ids.contains(&o.id)
                        })
                        .map(|o| o.id.clone())
                        .collect();
                    #[derive(Serialize)]
                    struct Cancel<'a> {
                        symbol: &'a str,
                    }
                    let mut steps = VecDeque::from([Step {
                        receipts: Vec::new(),
                        path: "/api/v1/private/order/cancel_all",
                        body: json(&Cancel { symbol: &m.id })?,
                        call: Call::Cancel {
                            ids: regular,
                            regular: false,
                        },
                    }]);
                    for (path, is_stop) in [
                        ("/api/v1/private/planorder/cancel_all", false),
                        ("/api/v1/private/stoporder/cancel_all", true),
                    ] {
                        let ids = cx
                            .store
                            .orders()
                            .iter()
                            .filter(|o| {
                                o.symbol == symbol
                                    && if is_stop {
                                        self.account.stop_ids.contains(&o.id)
                                    } else {
                                        self.account.plan_ids.contains(&o.id)
                                    }
                            })
                            .map(|o| o.id.clone())
                            .collect();
                        steps.push_back(Step {
                            receipts: Vec::new(),
                            path,
                            body: json(&Cancel { symbol: &m.id })?,
                            call: Call::Cancel {
                                ids,
                                regular: false,
                            },
                        });
                    }
                    Ok(steps)
                })();
                (command, result, 0.0)
            }
            Command::SetLeverage {
                command,
                symbol,
                leverage,
                side,
            } => (
                command,
                if leverage.is_finite() && leverage > 0.0 {
                    self.settings_steps(cx, &symbol, Some(leverage), None, side)
                } else {
                    Err("Invalid leverage".into())
                },
                0.0,
            ),
            Command::SetMarginType {
                command,
                symbol,
                isolated,
            } => (
                command,
                self.settings_steps(cx, &symbol, None, Some(isolated), None),
                0.0,
            ),
            Command::SetPositionMode {
                command,
                hedged,
                symbol: _,
            } => {
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Mode {
                    position_mode: u8,
                }
                (
                    command,
                    json(&Mode {
                        position_mode: if hedged { 1 } else { 2 },
                    })
                    .map(|body| {
                        VecDeque::from([Step {
                            receipts: Vec::new(),
                            path: "/api/v1/private/position/change_position_mode",
                            body,
                            call: Call::Mode { hedged },
                        }])
                    }),
                    0.0,
                )
            }
            _ => return,
        };
        if let Err(error) = &steps {
            let Ok(context) = cx.admit_http_command(local_mutation_policy()) else {
                return;
            };
            let _ = cx.command_error_context(&context, error);
            let _ = cx.fail_context(&context, not_sent(error.as_str()));
            return;
        }
        if self.credentials.is_none() {
            let Ok(context) = cx.admit_http_command(local_mutation_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                not_sent("MEXC trading needs an API key").kind(ErrorKind::AuthInvalid),
            );
            return;
        }
        if let Ok(steps) = steps {
            self.trading.queue.push_back(Job {
                command,
                steps,
                ids: vec![],
                gap,
                total_orders,
                failed_index: None,
                effective: false,
            });
            self.advance_trade(cx);
        }
    }
    fn advance_trade(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading.waiting || self.trading.timer.is_some() {
            return;
        }
        if self.trading.active.is_none() {
            self.trading.active = self.trading.queue.pop_front();
        }
        let Some(job) = self.trading.active.as_mut() else {
            return;
        };
        let Some(next) = job.steps.front() else {
            let job = self.trading.active.take().unwrap();
            {
                let result: Result<String, String> = json(&job.ids);
                match result {
                    Ok(data) => cx.done(job.command, data),
                    Err(error) => cx.fail(job.command, error),
                }
            };
            self.advance_trade(cx);
            return;
        };
        if matches!(next.path, SUBMIT | BATCH) && cx.now < self.trading.next_submit {
            self.trading.timer = Some(cx.set_timer(self.trading.next_submit - cx.now));
            return;
        }
        let job = self.trading.active.as_mut().unwrap();
        let mut step = job.steps.pop_front().unwrap();
        let command = job.command;
        let gap = job.gap;
        if let Call::Place {
            opts,
            amend: Some(intent),
            ..
        } = &step.call
            && matches!(opts.kind, OrderType::Limit | OrderType::Market)
        {
            if cx
                .store
                .order(&intent.id)
                .is_some_and(|o| o.status == OrderStatus::Closed)
            {
                self.trade_failed(
                    cx,
                    command,
                    not_sent("MEXC original order filled during amend"),
                );
                return;
            }
            self.read_amend(
                cx,
                Request {
                    command,
                    call: Call::AmendRead {
                        opts: opts.clone(),
                        intent: intent.clone(),
                        attempt: 0,
                    },
                    revision: self.account.revision,
                },
            );
            return;
        }
        if matches!(step.path, SUBMIT | BATCH) {
            self.trading.next_submit = cx.now + gap.max(450.0);
        }
        let refreshed=match step.call.clone() {
            Call::Place {mut opts,amend,..}=>{
                let amount=if let Some(amend)=amend {match cx.store.order(&amend.id) {
                    Some(o) if o.status!=OrderStatus::Closed => {opts.amount=amend.amount.unwrap_or_else(||o.extra.as_ref().and_then(|e|e.remaining_native).unwrap_or(o.remaining/cx.store.market(&o.symbol).and_then(|m|m.contract_size).unwrap_or(1.0)));Ok(())},
                    _=>Err("MEXC original order filled or disappeared during amend".to_owned()),
                }}else{Ok(())};
                amount.and_then(|_|self.placement(cx,opts))
            }
            Call::Batch {orders}=>orders.iter().map(|o|self.regular_body(cx,o,true)).collect::<Result<Vec<_>,_>>().map(|rows| {
                let (bodies, receipts): (Vec<_>, Vec<_>) = rows.into_iter().unzip();
                Step { receipts, path:BATCH,body:format!("[{}]",bodies.join(",")),call:Call::Batch{orders}}
            }),
            Call::Leverage {symbol,position_type,leverage,isolated,keep_leverage,keep_margin}=>self.settings_steps(cx,&symbol,(!keep_leverage).then_some(leverage),(!keep_margin).then_some(isolated),Some(if position_type==1{PositionSide::Long}else{PositionSide::Short})).and_then(|steps|steps.into_iter().find(|s|matches!(&s.call,Call::Leverage {position_type:p,..}if *p==position_type)).ok_or_else(||"MEXC leverage rail missing".into())),
            _=>Ok(step),
        };
        step = match refreshed {
            Ok(step) => step,
            Err(error) => {
                self.trade_failed(cx, command, not_sent(error));
                return;
            }
        };
        self.trading.waiting = true;
        cx.command_progress(command, step.receipts);
        self.signed(
            cx,
            Method::Post,
            step.path,
            step.body,
            false,
            Pending::Trade(Request {
                command,
                call: step.call,
                revision: self.account.revision,
            }),
        );
    }
    pub fn trade_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self
            .trading
            .read_retry
            .as_ref()
            .is_some_and(|(t, _)| *t == timer)
        {
            let (_, request) = self.trading.read_retry.take().unwrap();
            self.read_amend(cx, request);
            return;
        }

        if self.trading.timer == Some(timer) {
            self.trading.timer = None;
            self.advance_trade(cx);
        }
    }
    pub fn trade_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        mut error: VenueError,
    ) {
        self.trading.waiting = false;
        if let Some(job) = self.trading.active.take() {
            if (job.effective || !job.ids.is_empty())
                && matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent))
            {
                error.outcome = None;
            }
            let failed_index = job.failed_index.unwrap_or(job.ids.len());
            cx.fail(
                command,
                error.partial(&job.ids, job.total_orders, failed_index),
            );
        }
        self.advance_trade(cx);
    }
    fn read_amend(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        let Call::AmendRead { intent, .. } = &request.call else {
            return;
        };
        let path = format!("/api/v1/private/order/get/{}", intent.id);
        self.trading.waiting = true;
        self.signed(
            cx,
            Method::Get,
            &path,
            String::new(),
            false,
            Pending::Trade(request),
        );
    }
    fn amend_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let Call::AmendRead {
            mut opts,
            intent,
            attempt,
        } = request.call
        else {
            return;
        };
        let result = (|| {
            if response.status != 200 {
                return Err(format!(
                    "MEXC amend reconciliation HTTP {}",
                    response.status
                ));
            }
            let envelope = Envelope::checked(response.body)?;
            let raw: OrderRow<'_> =
                serde_json::from_str(envelope.payload()?).map_err(|e| e.to_string())?;
            let order = self
                .map_order(cx, &raw, Scope::Regular)
                .ok_or_else(|| "Invalid MEXC amend order read".to_owned())?;
            if order.id != intent.id || order.symbol != opts.symbol {
                return Err("MEXC amend read returned a different order".into());
            }
            if raw.vol <= 0.0 || raw.deal_vol < 0.0 || raw.deal_vol > raw.vol {
                return Err("Invalid MEXC amend volume; replacement withheld".into());
            }
            if cx
                .store
                .order(&intent.id)
                .is_some_and(|o| o.filled > order.filled)
            {
                return Err("Stale MEXC amend fill quantity; replacement withheld".into());
            }
            if order.status == OrderStatus::Open {
                if attempt >= 5 {
                    return Err(
                        "MEXC cancellation is not yet authoritative; replacement withheld".into(),
                    );
                }
                let retry = Request {
                    command: request.command,
                    call: Call::AmendRead {
                        opts: opts.clone(),
                        intent: intent.clone(),
                        attempt: attempt + 1,
                    },
                    revision: request.revision,
                };
                let timer = cx.set_timer(200.0);
                self.trading.read_retry = Some((timer, retry));
                return Ok(None);
            }
            if order.status == OrderStatus::Closed {
                if !cx
                    .store
                    .order(&intent.id)
                    .is_some_and(|o| o.status == OrderStatus::Closed && o.filled >= order.filled)
                {
                    self.account.revision += 1;
                    self.apply_order(cx, order, Scope::Regular, self.account.revision);
                }
                return Err("MEXC original order filled during amend".into());
            }
            if cx
                .store
                .order(&intent.id)
                .is_some_and(|o| o.status == OrderStatus::Closed)
            {
                return Err("MEXC original order filled during amend".into());
            }
            if raw.state != 4.0 {
                return Err(
                    "MEXC original order became invalid during amend; replacement withheld".into(),
                );
            }
            opts.take_profit = if raw.take_profit_price.is_some() {
                order.take_profit
            } else {
                order.take_profit.or(opts.take_profit)
            };
            opts.stop_loss = if raw.stop_loss_price.is_some() {
                order.stop_loss
            } else {
                order.stop_loss.or(opts.stop_loss)
            };
            if let Some(extra) = &order.extra {
                opts.tp_trigger_type = extra.tp_trigger_type.or(opts.tp_trigger_type);
                opts.sl_trigger_type = extra.sl_trigger_type.or(opts.sl_trigger_type);
            }
            opts.amount = intent.amount.unwrap_or_else(|| {
                order
                    .extra
                    .as_ref()
                    .and_then(|e| e.remaining_native)
                    .unwrap_or(0.0)
            });
            self.account.revision += 1;
            self.apply_order(cx, order, Scope::Regular, self.account.revision);
            self.placement(cx, opts).map(Some)
        })();
        match result {
            Ok(Some(step)) => {
                self.trading.active.as_mut().unwrap().steps.push_front(step);
                self.trading.waiting = false;
                self.advance_trade(cx);
            }
            Ok(None) => {}
            // A read before the replacement: nothing new was sent.
            Err(error) => self.trade_failed(cx, request.command, not_sent(error)),
        }
    }
    fn optimistic(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        opts: PlaceOrderOpts,
        id: String,
        scope: Scope,
    ) {
        // The stream may publish the accepted order before HTTP acknowledges it.
        // Reconciliation still follows every successful stop placement.
        if scope != Scope::Regular {
            self.schedule_stop_refresh(cx, &opts.symbol);
        }
        if self.account.terminal.contains_key(&id)
            || cx
                .store
                .order(&id)
                .is_some_and(|o| matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled))
        {
            return;
        }
        if cx.store.order(&id).is_some() {
            if scope == Scope::Regular {
                cx.store.update_order(&id, |o| {
                    let extra = o.extra.get_or_insert_with(OrderExtra::default);
                    if !extra.take_profit_known {
                        o.take_profit = o.take_profit.or(opts.take_profit);
                    }
                    if !extra.stop_loss_known {
                        o.stop_loss = o.stop_loss.or(opts.stop_loss);
                    }
                    extra.tp_trigger_type = extra
                        .tp_trigger_type
                        .or(opts.tp_trigger_type.or(opts.trigger_type));
                    extra.sl_trigger_type = extra
                        .sl_trigger_type
                        .or(opts.sl_trigger_type.or(opts.trigger_type));
                });
            }
            return;
        }
        if opts.kind == OrderType::Market {
            return;
        }
        let Some(m) = cx.store.market(&opts.symbol) else {
            return;
        };
        let size = m.contract_size.unwrap_or(1.0);
        let mut native = if scope == Scope::Stop {
            0.0
        } else {
            num::adjust(
                opts.amount,
                self.volume_units.get(&opts.symbol).copied().unwrap_or(1.0),
            )
        };
        if scope == Scope::Regular {
            native = native.max(self.volume_units.get(&opts.symbol).copied().unwrap_or(1.0));
        }
        let price = num::adjust(
            opts.trigger_price.or(opts.price).unwrap_or(0.0),
            m.precision.price,
        );
        let order = Order {
            id: id.clone(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: opts.symbol,
            kind: opts.kind,
            side: opts.side,
            price,
            amount: native * size,
            filled: 0.0,
            remaining: native * size,
            reduce_only: opts.reduce_only.unwrap_or(false) || scope != Scope::Regular,
            close: None,
            client_order_id: None,
            time_in_force: Some(opts.time_in_force.unwrap_or(TimeInForce::GoodTillCancel)),
            last_update: cx.now,
            trigger_type: match opts.kind {
                OrderType::StopLoss => opts.sl_trigger_type.or(opts.trigger_type),
                OrderType::TakeProfit => opts.tp_trigger_type.or(opts.trigger_type),
                _ => None,
            },
            take_profit: opts.take_profit,
            stop_loss: opts.stop_loss,
            extra: Some(OrderExtra {
                tp_trigger_type: opts.tp_trigger_type.or(opts.trigger_type),
                sl_trigger_type: opts.sl_trigger_type.or(opts.trigger_type),
                remaining_native: Some(native),
                ..Default::default()
            }),
        };
        self.account.revision += 1;
        self.account
            .order_updates
            .insert(id.clone(), self.account.revision);
        if scope == Scope::Stop {
            self.account.stop_ids.insert(id.clone());
        }
        if scope == Scope::Plan {
            self.account.plan_ids.insert(id.clone());
        }
        if scope != Scope::Regular {
            self.account.placed.insert(id, cx.now);
        }
        cx.store.upsert_order(order);
    }
    pub fn trade_response(
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
        if matches!(&request.call, Call::AmendRead { .. }) {
            self.amend_response(cx, request, response);
            return;
        }
        let envelope = serde_json::from_str::<Envelope<'_>>(response.body);
        if let (Call::Batch { orders }, Ok(e)) = (&request.call, &envelope)
            && (200..500).contains(&response.status)
            && e.success == Some(false)
            && (e.code == 511.0 || e.message.to_lowercase().contains("do not have access"))
        {
            self.trading.batch_denied = true;
            self.trading.waiting = false;
            let mut all = orders.clone();
            let job = self.trading.active.as_mut().unwrap();
            while let Some(step) = job.steps.pop_front() {
                if let Call::Batch { orders } = step.call {
                    all.extend(orders);
                } else {
                    job.steps.push_front(step);
                    break;
                }
            }
            match self.place_steps(cx, all, false) {
                Ok(mut steps) => {
                    steps.append(&mut self.trading.active.as_mut().unwrap().steps);
                    self.trading.active.as_mut().unwrap().steps = steps;
                    self.advance_trade(cx);
                }
                Err(error) => self.trade_failed(cx, request.command, not_sent(error)),
            }
            return;
        }
        if let Ok(e) = &envelope
            && e.code == 510.0
        {
            self.trading.next_submit = self.trading.next_submit.max(cx.now + 1500.0);
        }
        // Set when every failure is an explicit per-order venue refusal.
        let mut refused = false;
        let result = (|| {
            if response.status != 200 {
                return Err(format!("MEXC HTTP {}", response.status));
            }
            let e = Envelope::checked(response.body)?;
            match request.call {
                Call::Place { opts, scope, .. } => {
                    let id = result_id(e.payload()?)?;
                    self.optimistic(cx, opts, id.clone(), scope);
                    self.trading.active.as_mut().unwrap().ids.push(id);
                }
                Call::Batch { orders } => {
                    let rows: Vec<ResultRow<'_>> =
                        serde_json::from_str(e.payload()?).map_err(|e| e.to_string())?;
                    let mut unproven = rows.len() != orders.len();
                    let mut failures = if unproven {
                        vec!["incomplete results".to_owned()]
                    } else {
                        vec![]
                    };
                    let base = self.trading.active.as_ref().unwrap().ids.len();
                    for (index, (opts, r)) in orders.into_iter().zip(rows).enumerate() {
                        if r.error_code != 0.0 {
                            self.trading
                                .active
                                .as_mut()
                                .unwrap()
                                .failed_index
                                .get_or_insert(base + index);
                            failures.push(format!("{}: {}", r.error_code, r.error_msg));
                            continue;
                        }
                        let id = if r.order_id.is_empty() {
                            r.id.to_string()
                        } else {
                            r.order_id.to_string()
                        };
                        if id.is_empty() {
                            unproven = true;
                            failures.push("missing order id".into());
                            continue;
                        }
                        self.optimistic(cx, opts, id.clone(), Scope::Regular);
                        self.trading.active.as_mut().unwrap().ids.push(id);
                    }
                    if !failures.is_empty() {
                        refused = !unproven;
                        return Err(format!(
                            "MEXC batch order placement failed: {}",
                            failures.join(", ")
                        ));
                    }
                }
                Call::Cancel { ids, regular } => {
                    let rows = if regular {
                        serde_json::from_str::<Vec<ResultRow<'_>>>(e.payload()?)
                            .map_err(|e| e.to_string())?
                    } else {
                        vec![]
                    };
                    let mut errors = vec![];
                    let mut unproven = false;
                    for id in ids {
                        if regular && !rows.iter().any(|r| r.order_id == id || r.id == id) {
                            unproven = true;
                            errors.push(format!("{id}: missing cancel result"));
                            continue;
                        }
                        if let Some(row) = rows
                            .iter()
                            .find(|r| (r.order_id == id || r.id == id) && r.error_code != 0.0)
                        {
                            errors.push(format!("{id}: {}", row.error_msg));
                            continue;
                        }
                        self.account.revision += 1;
                        self.account
                            .order_updates
                            .insert(id.clone(), self.account.revision);
                        self.account
                            .terminal
                            .insert(id.clone(), (self.account.revision, cx.now));
                        self.account.placed.remove(&id);
                        if !cx
                            .store
                            .order(&id)
                            .is_some_and(|o| o.status == OrderStatus::Closed)
                        {
                            cx.store.set_order_status(&id, OrderStatus::Canceled);
                        }
                    }
                    if !errors.is_empty() {
                        refused = !unproven;
                        return Err(format!(
                            "MEXC order cancellation failed: {}",
                            errors.join(", ")
                        ));
                    }
                }
                Call::Leverage {
                    symbol,
                    position_type,
                    leverage,
                    isolated,
                    ..
                } => {
                    let target = if position_type == 1 {
                        PositionSide::Long
                    } else {
                        PositionSide::Short
                    };
                    if self
                        .account
                        .rail_revision
                        .get(&(symbol.clone(), target))
                        .is_some_and(|&v| v > request.revision)
                    {
                        return Ok(());
                    }
                    let isolated = if self
                        .account
                        .settings_revision
                        .get(&symbol)
                        .is_some_and(|&v| v > request.revision)
                    {
                        self.order_rails(&symbol).isolated
                    } else {
                        isolated
                    };
                    let r = self.account.rails.entry(symbol.clone()).or_default();
                    if position_type == 1 {
                        r.long = leverage;
                    } else {
                        r.short = leverage;
                    }
                    r.isolated = isolated;
                    let rails = *r;
                    self.account.revision += 1;
                    let revision = self.account.revision;
                    self.account
                        .rail_revision
                        .insert((symbol.clone(), target), revision);
                    self.account
                        .settings_revision
                        .insert(symbol.clone(), revision);
                    cx.store.update_setting(&symbol, |s| {
                        s.leverage = rails.long;
                        s.buy_leverage = Some(rails.long);
                        s.sell_leverage = Some(rails.short);
                        s.margin_type = Some(if isolated {
                            MarginType::Isolated
                        } else {
                            MarginType::Cross
                        });
                    });
                    let side = if position_type == 1 {
                        PositionSide::Long
                    } else {
                        PositionSide::Short
                    };
                    for p in cx
                        .store
                        .positions()
                        .iter()
                        .filter(|p| p.symbol == symbol && p.side == side)
                    {
                        self.account.position_updates.insert(p.id.clone(), revision);
                    }
                    cx.store.update_positions(|p| {
                        if p.symbol == symbol && p.side == side {
                            p.leverage = leverage;
                            p.margin_type = Some(if isolated {
                                MarginType::Isolated
                            } else {
                                MarginType::Cross
                            });
                        }
                    });
                }
                Call::AmendRead { .. } => unreachable!(),
                Call::Mode { hedged } => {
                    if self.account.mode_revision > request.revision {
                        return Ok(());
                    }
                    self.account.revision += 1;
                    self.account.mode_revision = self.account.revision;
                    cx.store.set_hedged(hedged);
                    self.refresh_account(cx, true);
                }
            }
            Ok(())
        })();
        self.trading.waiting = false;
        match result {
            Ok(()) => {
                if let Some(job) = self.trading.active.as_mut() {
                    job.effective = true;
                }
                self.advance_trade(cx);
            }
            Err(error) => {
                cx.command_error(request.command, &error);
                let error = write_failure(response.status, response.body, error, refused);
                self.trade_failed(cx, request.command, error);
            }
        }
    }
}

/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

/// Classifies a failed write answer. 408/5xx and malformed or unproven 2xx
/// answers are Unknown; undocumented venue codes stay unset.
fn write_failure(status: u16, body: &str, message: String, refused: bool) -> VenueError {
    let error = VenueError::new(message);
    let rejected = |error: VenueError, kind| error.kind(kind).outcome(Outcome::Rejected);
    match status {
        200 => {}
        401 => return rejected(error, ErrorKind::AuthInvalid),
        403 => return rejected(error, ErrorKind::AuthPermission),
        429 => return rejected(error, ErrorKind::RateLimited),
        408 => return error.outcome(Outcome::Unknown),
        400..=499 => return rejected(error, ErrorKind::Rejected),
        _ => return error.outcome(Outcome::Unknown),
    }
    let Ok(envelope) = serde_json::from_str::<Envelope<'_>>(body) else {
        return error.outcome(Outcome::Unknown);
    };
    if envelope.success == Some(true) && envelope.code == 0.0 {
        return if refused {
            error
        } else {
            error.outcome(Outcome::Unknown)
        };
    }
    let mut error = error.venue_code(envelope.code.to_string());
    if !envelope.message.is_empty() {
        error = error.venue_message(envelope.message.into_owned());
    }
    match envelope.code as i64 {
        401 | 602 => rejected(error, ErrorKind::AuthInvalid),
        402 => rejected(error, ErrorKind::AuthExpired),
        406 | 701..=704 => rejected(error, ErrorKind::AuthPermission),
        510 => rejected(error, ErrorKind::RateLimited),
        600 | 1001..=1004 | 2001..=2009 | 2011 | 2013..=2015 => {
            rejected(error, ErrorKind::Rejected)
        }
        500 | 501 | 9999 => error.outcome(Outcome::Unknown),
        _ => error,
    }
}

impl MexcFutures {
    pub(crate) fn queue_trade(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        payload: String,
        pending: Pending,
    ) {
        use safe_cex_engine::credential_selection::{CredentialRole, Slots};
        let slots = Slots::new(
            self.credentials.clone(),
            self.options.identities.secondary.clone(),
            Default::default(),
        );
        let role = if method == Method::Get {
            CredentialRole::Read { scope: None }
        } else {
            CredentialRole::Mutation
        };
        let Ok((_, identity)) = slots.select(role) else {
            return;
        };
        let step = crate::sign::dispatch::Http::new(
            identity.clone(),
            method,
            path.into(),
            payload,
            self.options.recv_window,
        );
        let request = cx.request(
            method,
            step.url(),
            vec![("Content-Type", "application/json".into())],
            step.body(),
            pending,
        );
        cx.sign_http(request, Box::new(step));
    }
}
