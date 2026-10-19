//! Commands are preflighted as a whole, then executed as sequenced write jobs.
use crate::account::{Axis, NativeOrder};
use crate::sign::Params;
use crate::wire::{Num, Str, data};
use crate::{BitunixFutures, Pending};
use safe_cex_engine::model::*;
use safe_cex_engine::{Command, Cx, LogLevel, Method, Response, TimerId, num};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
const PLACE: &str = "/api/v1/futures/trade/place_order";
const BATCH: &str = "/api/v1/futures/trade/batch_order";
const TPSL: &str = "/api/v1/futures/tpsl/place_order";
const FULL_TPSL: &str = "/api/v1/futures/tpsl/position/place_order";
const CANCEL: &str = "/api/v1/futures/trade/cancel_orders";
const CANCEL_TPSL: &str = "/api/v1/futures/tpsl/cancel_order";
const AMEND: &str = "/api/v1/futures/trade/modify_order";
const AMEND_TPSL: &str = "/api/v1/futures/tpsl/modify_order";
const AMEND_FULL: &str = "/api/v1/futures/tpsl/position/modify_order";

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Payload {
    #[serde(skip_serializing_if = "Option::is_none")]
    symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    qty: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trade_side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    effect: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reduce_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_stop_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_order_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_order_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_qty: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_stop_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_order_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_order_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_qty: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Batch<'a> {
    symbol: &'a str,
    order_list: Vec<Payload>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Cancel<'a> {
    symbol: &'a str,
    order_list: Vec<Identity<'a>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Identity<'a> {
    order_id: &'a str,
}

pub struct Request {
    pub command: u32,
    call: Call,
}
enum Call {
    Place {
        clients: Vec<String>,
        batch: bool,
    },
    Cancel {
        ids: Vec<String>,
        require_ack: bool,
    },
    ReconcileReplacement {
        parent: String,
        id: String,
        leg: Option<&'static str>,
        payload: Box<Payload>,
    },
    Amend {
        id: String,
        parent: String,
        leg: Option<&'static str>,
        fallback: Option<Box<Payload>>,
    },
    Leverage {
        symbol: String,
        value: f64,
    },
    Margin {
        symbol: String,
        isolated: bool,
    },
    Mode {
        hedged: bool,
    },
}
struct Step {
    path: &'static str,
    body: String,
    call: Call,
    delay: f64,
}
#[derive(Default)]
struct Job {
    steps: VecDeque<Step>,
    ids: Vec<String>,
    optimistic: Vec<String>,
    restore: Vec<OrderStatus>,
    timer: Option<TimerId>,
    recover: Option<Vec<PlaceOrderOpts>>,
    created: Vec<String>,
}
struct Prepare {
    orders: Vec<PlaceOrderOpts>,
    stage: u8,
    retried: bool,
}
#[derive(Default)]
pub struct TradingState {
    jobs: HashMap<u32, Job>,
    preparing: HashMap<u32, Prepare>,
}
fn text<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).expect("finite request fields")
}
fn stop(t: Option<StopTriggerType>) -> &'static str {
    if t == Some(StopTriggerType::MarkPrice) {
        "MARK_PRICE"
    } else {
        "LAST_PRICE"
    }
}
fn effect(t: Option<TimeInForce>) -> &'static str {
    match t.unwrap_or(TimeInForce::GoodTillCancel) {
        TimeInForce::GoodTillCancel => "GTC",
        TimeInForce::ImmediateOrCancel => "IOC",
        TimeInForce::FillOrKill => "FOK",
        TimeInForce::PostOnly => "POST_ONLY",
    }
}
fn positive(v: f64, name: &str) -> Result<f64, String> {
    if v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        Err(format!("Bitunix {name} must be greater than zero"))
    }
}

impl BitunixFutures {
    fn reject(&self, cx: &mut Cx<'_, Pending>, command: u32, error: String) {
        cx.log(LogLevel::Error, &error);
        cx.done(command, Err(error));
    }
    fn preflight(&self, cx: &Cx<'_, Pending>, orders: &[PlaceOrderOpts]) -> Result<(), String> {
        let mut ids = HashSet::new();
        for o in orders {
            let m = cx
                .store
                .market(&o.symbol)
                .ok_or_else(|| format!("Market {} not found", o.symbol))?;
            if let Some(id) = &o.client_order_id {
                if id.is_empty() || id.chars().any(char::is_whitespace) || !ids.insert(id) {
                    return Err(
                        "Bitunix clientOrderId must be nonempty, unique and contain no whitespace"
                            .into(),
                    );
                }
                if !matches!(o.kind, OrderType::Limit | OrderType::Market)
                    || o.trigger_price.is_some()
                    || o.stop_loss.is_some()
                    || o.take_profit.is_some()
                    || o.amount > m.limits.amount.max
                {
                    return Err(
                        "Bitunix clientOrderId requires exactly one plain native order".into(),
                    );
                }
            }
            if matches!(o.kind, OrderType::TrailingStopLoss | OrderType::Conditional)
                || o.trigger_price.is_some()
            {
                return Err("Bitunix conditional and trailing orders are not supported".into());
            }
            if o.amount < 0.0 || !o.amount.is_finite() {
                return Err("Bitunix order amount is invalid".into());
            }
            if matches!(o.kind, OrderType::Limit | OrderType::Market) {
                positive(o.amount, "quantity")?;
            }
            if o.kind != OrderType::Market {
                positive(o.price.unwrap_or(0.0), "price")?;
            }
            for v in [o.take_profit, o.stop_loss].into_iter().flatten() {
                positive(v, "protection trigger")?;
            }
        }
        Ok(())
    }
    pub(crate) fn trading_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let id = match &command {
            Command::PlaceOrders { command, .. }
            | Command::CancelOrders { command, .. }
            | Command::CancelSymbolOrders { command, .. }
            | Command::AmendOrder { command, .. }
            | Command::SetLeverage { command, .. }
            | Command::SetMarginType { command, .. }
            | Command::SetPositionMode { command, .. } => *command,
            _ => return,
        };
        if self.credentials.is_none() {
            self.reject(cx, id, "Bitunix credentials required".into());
            return;
        }
        let (command, result) = match command {
            Command::PlaceOrders { command, orders } => {
                if let Err(e) = self.preflight(cx, &orders) {
                    self.reject(cx, command, e);
                    return;
                }
                self.start_place(cx, command, orders, false);
                return;
            }
            Command::CancelOrders { command, ids } => (command, self.plan_cancel(cx, &ids)),
            Command::CancelSymbolOrders { command, symbol } => {
                let ids: Vec<_> = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.symbol == symbol && o.status == OrderStatus::Open)
                    .map(|o| o.id.clone())
                    .collect();
                (command, self.plan_cancel(cx, &ids))
            }
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => (command, self.plan_amend(cx, &id, price, amount)),
            Command::SetLeverage {
                command,
                symbol,
                leverage,
            } => {
                let result = (|| {
                    let m = cx
                        .store
                        .market(&symbol)
                        .ok_or_else(|| format!("Market {symbol} not found"))?;
                    positive(leverage, "leverage")?;
                    let value = leverage.clamp(m.limits.leverage.min, m.limits.leverage.max);
                    #[derive(Serialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Body<'a> {
                        margin_coin: &'static str,
                        symbol: &'a str,
                        leverage: f64,
                    }
                    Ok(Job {
                        steps: VecDeque::from([Step {
                            path: "/api/v1/futures/account/change_leverage",
                            body: text(&Body {
                                margin_coin: "USDT",
                                symbol: &symbol,
                                leverage: value,
                            }),
                            call: Call::Leverage { symbol, value },
                            delay: 0.0,
                        }]),
                        ..Job::default()
                    })
                })();
                (command, result)
            }
            Command::SetMarginType {
                command,
                symbol,
                isolated,
            } => {
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Body<'a> {
                    symbol: &'a str,
                    margin_mode: &'static str,
                    margin_coin: &'static str,
                }
                let result = if cx.store.market(&symbol).is_none() {
                    Err(format!("Market {symbol} not found"))
                } else {
                    Ok(Job {
                        steps: VecDeque::from([Step {
                            path: "/api/v1/futures/account/change_margin_mode",
                            body: text(&Body {
                                symbol: &symbol,
                                margin_mode: if isolated { "ISOLATION" } else { "CROSS" },
                                margin_coin: "USDT",
                            }),
                            call: Call::Margin { symbol, isolated },
                            delay: 0.0,
                        }]),
                        ..Job::default()
                    })
                };
                (command, result)
            }
            Command::SetPositionMode { command, hedged } => {
                let result = if !cx.store.positions().is_empty() {
                    Err("Please close all positions before switching position mode".into())
                } else {
                    Ok(Job {
                        steps: VecDeque::from([Step {
                            path: "/api/v1/futures/account/change_position_mode",
                            body: format!(
                                r#"{{"positionMode":"{}"}}"#,
                                if hedged { "HEDGE" } else { "ONE_WAY" }
                            ),
                            call: Call::Mode { hedged },
                            delay: 0.0,
                        }]),
                        ..Job::default()
                    })
                };
                (command, result)
            }
            _ => return,
        };
        match result {
            Ok(job) => {
                self.trading.jobs.insert(command, job);
                self.advance(cx, command);
            }
            Err(e) => self.reject(cx, command, e),
        }
    }
    fn start_place(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: u32,
        orders: Vec<PlaceOrderOpts>,
        retried: bool,
    ) {
        let needs_read = orders.iter().any(|o| {
            let tpsl = matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit);
            (tpsl && o.amount == 0.0)
                || ((tpsl || o.reduce_only == Some(true))
                    && self.resolve_position(cx, o, tpsl).is_err())
        });
        if needs_read {
            self.trading.preparing.insert(
                command,
                Prepare {
                    orders,
                    stage: 0,
                    retried,
                },
            );
            self.read_axis_for(cx, Axis::Positions, Some(command));
            return;
        }
        self.place_prepared(cx, command, orders, retried);
    }
    pub(crate) fn prepared(&mut self, cx: &mut Cx<'_, Pending>, command: u32, success: bool) {
        let Some(mut p) = self.trading.preparing.remove(&command) else {
            return;
        };
        if !success {
            self.reject(
                cx,
                command,
                "Bitunix preflight reconciliation failed".into(),
            );
            return;
        }
        if p.stage < 2 {
            p.stage += 1;
            let axis = if p.stage == 1 {
                Axis::Orders
            } else {
                Axis::Tpsl
            };
            self.trading.preparing.insert(command, p);
            self.read_axis_for(cx, axis, Some(command));
        } else {
            self.place_prepared(cx, command, p.orders, p.retried);
        }
    }
    fn resolve_position(
        &self,
        cx: &Cx<'_, Pending>,
        o: &PlaceOrderOpts,
        tpsl: bool,
    ) -> Result<String, String> {
        let side = if o.side == OrderSide::Buy {
            PositionSide::Short
        } else {
            PositionSide::Long
        };
        let side_text = if side == PositionSide::Long {
            "long"
        } else {
            "short"
        };
        if !cx
            .store
            .positions()
            .iter()
            .any(|p| p.symbol == o.symbol && p.side == side && p.contracts > 0.0)
        {
            return Err(format!(
                "No {side_text} position open for {} - nothing to reduce",
                o.symbol
            ));
        }
        let key = format!("{}:{side_text}", o.symbol);
        let rest = self.account.position_ids.get(&key);
        let ws = self.account.ws_position_ids.get(&key);
        if tpsl { ws.or(rest) } else { rest.or(ws) }
            .cloned()
            .ok_or_else(|| format!("Missing positionId for {}", o.symbol))
    }
    fn payloads(
        &self,
        cx: &Cx<'_, Pending>,
        o: &PlaceOrderOpts,
    ) -> Result<Vec<(&'static str, Payload)>, String> {
        let m = cx
            .store
            .market(&o.symbol)
            .ok_or_else(|| format!("Market {} not found", o.symbol))?;
        let tpsl = matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit);
        let mut p = Payload {
            symbol: Some(o.symbol.clone()),
            ..Payload::default()
        };
        if tpsl {
            let position_id = self.resolve_position(cx, o, true)?;
            let full = o.amount == 0.0;
            p.position_id = Some(position_id.clone());
            let price = num::format(positive(
                num::adjust(o.price.unwrap_or(0.0), m.precision.price),
                "TP/SL price",
            )?);
            let qty = num::format(
                positive(num::adjust(o.amount, m.precision.amount), "TP/SL quantity")
                    .or_else(|e| if full { Ok(0.0) } else { Err(e) })?,
            );
            if o.kind == OrderType::TakeProfit {
                p.tp_price = Some(price);
                p.tp_stop_type = Some(stop(o.tp_trigger_type));
                if !full {
                    p.tp_qty = Some(qty);
                    p.tp_order_type = Some("MARKET".into());
                }
            } else {
                p.sl_price = Some(price);
                p.sl_stop_type = Some(stop(o.sl_trigger_type));
                if !full {
                    p.sl_qty = Some(qty);
                    p.sl_order_type = Some("MARKET".into());
                }
            }
            if full {
                for row in cx.store.orders().iter().filter(|r| {
                    r.symbol == o.symbol && r.status == OrderStatus::Open && r.kind != o.kind
                }) {
                    if self
                        .account
                        .native
                        .get(&row.id)
                        .is_some_and(|n| !n.attached && n.position_id == position_id)
                    {
                        if row.kind == OrderType::StopLoss {
                            p.sl_price = Some(num::format(row.price));
                            p.sl_stop_type = Some(stop(row.trigger_type));
                        } else if row.kind == OrderType::TakeProfit {
                            p.tp_price = Some(num::format(row.price));
                            p.tp_stop_type = Some(stop(row.trigger_type));
                        }
                    }
                }
            }
            return Ok(vec![(if full { FULL_TPSL } else { TPSL }, p)]);
        }
        let amount = positive(num::adjust(o.amount, m.precision.amount), "quantity")?;
        let reduce = o.reduce_only.unwrap_or(false);
        p.reduce_only = Some(reduce);
        p.side = Some(if o.side == OrderSide::Buy {
            "BUY"
        } else {
            "SELL"
        });
        p.trade_side = Some(if reduce { "CLOSE" } else { "OPEN" });
        if reduce {
            p.position_id = Some(self.resolve_position(cx, o, false)?);
        }
        p.order_type = Some(if o.kind == OrderType::Market {
            "MARKET"
        } else {
            "LIMIT"
        });
        p.effect = Some(effect(o.time_in_force));
        if o.kind != OrderType::Market {
            p.price = Some(num::format(positive(
                num::adjust(o.price.unwrap_or(0.0), m.precision.price),
                "price",
            )?));
        }
        if let Some(v) = o.take_profit {
            p.tp_price = Some(num::format(v));
            p.tp_stop_type = Some(stop(o.tp_trigger_type));
            p.tp_order_type = Some("MARKET".into());
        }
        if let Some(v) = o.stop_loss {
            p.sl_price = Some(num::format(v));
            p.sl_stop_type = Some(stop(o.sl_trigger_type));
            p.sl_order_type = Some("MARKET".into());
        }
        let max = m.limits.amount.max;
        if max <= 0.0 || !max.is_finite() {
            return Err("Bitunix market has no valid maximum quantity".into());
        }
        let lots = if amount > max {
            (amount / max).ceil() as usize
        } else {
            1
        };
        if lots > 10_000 {
            return Err("Bitunix order exceeds split limit".into());
        }
        let remainder = if amount > max {
            num::adjust(amount % max, m.precision.amount)
        } else {
            0.0
        };
        let lot = num::adjust((amount - remainder) / lots as f64, m.precision.amount);
        let mut sizes = vec![lot; lots];
        if remainder > 0.0 {
            sizes.push(remainder);
        }
        if o.client_order_id.is_some() && sizes.len() != 1 {
            return Err("Bitunix clientOrderId cannot be split".into());
        }
        sizes
            .into_iter()
            .map(|qty| {
                positive(qty, "split quantity")?;
                let mut payload = p.clone();
                payload.qty = Some(num::format(qty));
                payload.client_id = Some(if let Some(id) = &o.client_order_id {
                    id.clone()
                } else {
                    format!("ts_{}", safe_cex_crypto::uuid::uuid()?)
                });
                Ok((PLACE, payload))
            })
            .collect()
    }
    fn place_prepared(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: u32,
        orders: Vec<PlaceOrderOpts>,
        retried: bool,
    ) {
        let result = (|| {
            let mut plan: Vec<(&'static str, Payload)> = vec![];
            for o in &orders {
                for (path, p) in self.payloads(cx, o)? {
                    if path == FULL_TPSL
                        && let Some((_, existing)) = plan.iter_mut().find(|(kind, row)| {
                            *kind == FULL_TPSL
                                && row.symbol == p.symbol
                                && row.position_id == p.position_id
                        })
                    {
                        // Each payload carries an old sibling; merge only the requested leg.
                        if o.kind == OrderType::TakeProfit {
                            existing.tp_price = p.tp_price;
                            existing.tp_stop_type = p.tp_stop_type;
                        } else {
                            existing.sl_price = p.sl_price;
                            existing.sl_stop_type = p.sl_stop_type;
                        }
                    } else {
                        plan.push((path, p));
                    }
                }
            }
            let mut job = Job::default();
            if orders.len() == 1 && !retried {
                job.recover = Some(orders.clone());
            }
            for path in [FULL_TPSL, TPSL] {
                for (_, p) in plan.iter().filter(|(kind, _)| *kind == path) {
                    job.steps.push_back(Step {
                        path,
                        body: text(p),
                        call: Call::Place {
                            clients: vec![],
                            batch: false,
                        },
                        delay: 0.0,
                    });
                }
            }
            let mut grouped: Vec<(String, Vec<Payload>)> = vec![];
            for (_, p) in plan.into_iter().filter(|(path, _)| *path == PLACE) {
                let symbol = p.symbol.clone().unwrap();
                if let Some((_, rows)) = grouped.iter_mut().find(|(s, _)| *s == symbol) {
                    rows.push(p);
                } else {
                    grouped.push((symbol, vec![p]));
                }
            }
            for (symbol, payloads) in grouped {
                for (index, chunk) in payloads.chunks(5).enumerate() {
                    let clients: Vec<_> =
                        chunk.iter().filter_map(|p| p.client_id.clone()).collect();
                    for p in chunk {
                        if let Some(id) = &p.client_id {
                            self.account
                                .tifs
                                .insert(id.clone(), crate::account::tif(p.effect.unwrap_or("GTC")));
                            self.account
                                .reduces
                                .insert(id.clone(), p.reduce_only.unwrap_or(false));
                        }
                    }
                    let batch = chunk.len() > 1;
                    let body = if batch {
                        text(&Batch {
                            symbol: &symbol,
                            order_list: chunk
                                .iter()
                                .map(|p| {
                                    let mut p = p.clone();
                                    p.symbol = None;
                                    p
                                })
                                .collect(),
                        })
                    } else {
                        text(&chunk[0])
                    };
                    job.steps.push_back(Step {
                        path: if batch { BATCH } else { PLACE },
                        body,
                        call: Call::Place { clients, batch },
                        delay: if index > 0 { 1000.0 } else { 0.0 },
                    });
                }
            }
            if job.steps.len() != 1
                || !matches!(
                    job.steps.front().map(|s| &s.call),
                    Some(Call::Place { batch: false, .. })
                )
            {
                job.recover = None;
            }
            Ok::<_, String>(job)
        })();
        match result {
            Ok(mut job) => {
                for (index, order) in orders
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| matches!(o.kind, OrderType::Market | OrderType::Limit))
                {
                    let id = format!("creating:{command}:{index}");
                    cx.store.upsert_order(Order {
                        id: id.clone(),
                        parent_id: None,
                        status: OrderStatus::Creating,
                        symbol: order.symbol.clone(),
                        kind: order.kind,
                        side: order.side,
                        price: order.price.unwrap_or(0.0),
                        amount: order.amount,
                        filled: 0.0,
                        remaining: order.amount,
                        reduce_only: order.reduce_only.unwrap_or(false),
                        close: None,
                        client_order_id: order.client_order_id.clone(),
                        time_in_force: order.time_in_force,
                        last_update: cx.now,
                        trigger_type: None,
                        take_profit: order.take_profit,
                        stop_loss: order.stop_loss,
                        extra: None,
                    });
                    job.created.push(id);
                }
                self.account.generations[2] += 1;
                self.trading.jobs.insert(command, job);
                self.advance(cx, command);
            }
            Err(e) => self.reject(cx, command, e),
        }
    }
    fn plan_cancel(&self, cx: &Cx<'_, Pending>, ids: &[String]) -> Result<Job, String> {
        let mut job = Job::default();
        let mut regular: Vec<(String, Vec<String>)> = vec![];
        let mut seen = HashSet::new();
        let selected: HashSet<_> = ids.iter().map(String::as_str).collect();
        let mut replacements = vec![];
        for id in ids {
            let Some(o) = cx.store.order(id) else {
                continue;
            };
            let n = self
                .account
                .native
                .get(id)
                .ok_or_else(|| format!("Bitunix order {id} is not reconciled"))?;
            if !seen.insert(n.raw_id.clone()) {
                continue;
            }
            if n.attached {
                let parent = cx
                    .store
                    .order(&n.raw_id)
                    .ok_or("Bitunix attached parent missing")?;
                let keep_tp =
                    n.tp.is_some() && !selected.contains(format!("{}__tp", parent.id).as_str());
                let keep_sl =
                    n.sl.is_some() && !selected.contains(format!("{}__sl", parent.id).as_str());
                if !selected.contains(parent.id.as_str()) && (keep_tp || keep_sl) {
                    let mut metadata = n.clone();
                    if !keep_tp {
                        metadata.tp = None;
                        metadata.tp_execution.clear();
                    }
                    if !keep_sl {
                        metadata.sl = None;
                        metadata.sl_execution.clear();
                    }
                    if let Some(p) = self.replacement(cx, parent, &metadata)? {
                        replacements.push((parent.id.clone(), p));
                    }
                }
            }
            let standalone =
                matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit) && !n.attached;
            let affected: Vec<_> = cx
                .store
                .orders()
                .iter()
                .filter(|r| {
                    r.id == n.raw_id
                        || self
                            .account
                            .native
                            .get(&r.id)
                            .is_some_and(|m| m.raw_id == n.raw_id)
                })
                .map(|r| r.id.clone())
                .collect();
            if standalone {
                let p = Payload {
                    symbol: Some(o.symbol.clone()),
                    order_id: Some(n.raw_id.clone()),
                    ..Default::default()
                };
                job.steps.push_back(Step {
                    path: CANCEL_TPSL,
                    body: text(&p),
                    call: Call::Cancel {
                        ids: affected,
                        require_ack: false,
                    },
                    delay: 0.0,
                });
            } else {
                if let Some((_, rows)) = regular.iter_mut().find(|(s, _)| s == &o.symbol) {
                    rows.push(n.raw_id.clone());
                } else {
                    regular.push((o.symbol.clone(), vec![n.raw_id.clone()]));
                }
            }
            for id in affected_ids(cx, n) {
                if let Some(row) = cx.store.order(&id) {
                    job.optimistic.push(id);
                    job.restore.push(row.status);
                }
            }
        }
        for (symbol, ids) in regular {
            let affected: Vec<_> = ids
                .iter()
                .flat_map(|id| {
                    cx.store
                        .orders()
                        .iter()
                        .filter(|o| {
                            o.id == *id
                                || self
                                    .account
                                    .native
                                    .get(&o.id)
                                    .is_some_and(|n| n.raw_id == *id)
                        })
                        .map(|o| o.id.clone())
                })
                .collect();
            job.steps.push_back(Step {
                path: CANCEL,
                body: text(&Cancel {
                    symbol: &symbol,
                    order_list: ids.iter().map(|id| Identity { order_id: id }).collect(),
                }),
                call: Call::Cancel {
                    ids: affected,
                    require_ack: true,
                },
                delay: 0.0,
            });
        }
        for (parent, p) in replacements {
            job.steps.push_back(Step {
                path: "/api/v1/futures/trade/get_order_detail",
                body: String::new(),
                call: Call::ReconcileReplacement {
                    parent: parent.clone(),
                    id: parent,
                    leg: None,
                    payload: Box::new(p),
                },
                delay: 0.0,
            });
        }
        Ok(job)
    }
    fn replacement(
        &self,
        cx: &Cx<'_, Pending>,
        parent: &Order,
        n: &NativeOrder,
    ) -> Result<Option<Payload>, String> {
        if n.tp_execution == "LIMIT" || n.sl_execution == "LIMIT" {
            return Err("Bitunix cannot recreate attached LIMIT-execution protection".into());
        }
        let o = PlaceOrderOpts {
            symbol: parent.symbol.clone(),
            kind: parent.kind,
            side: parent.side,
            amount: parent.remaining,
            price: Some(parent.price),
            trigger_price: None,
            stop_loss: n.sl,
            take_profit: n.tp,
            reduce_only: Some(parent.reduce_only),
            time_in_force: parent.time_in_force,
            trigger_type: None,
            tp_trigger_type: n.tp_stop,
            sl_trigger_type: n.sl_stop,
            client_order_id: None,
            close: None,
        };
        Ok(self.payloads(cx, &o)?.into_iter().next().map(|(_, p)| p))
    }
    fn plan_amend(
        &self,
        cx: &Cx<'_, Pending>,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<Job, String> {
        let o = cx
            .store
            .order(id)
            .ok_or_else(|| format!("Bitunix order {id} not found"))?;
        if o.status != OrderStatus::Open {
            return Err("Bitunix only pending orders can be amended".into());
        }
        let n = self
            .account
            .native
            .get(id)
            .ok_or("Bitunix order identity not reconciled")?;
        let m = cx.store.market(&o.symbol).ok_or("Bitunix market missing")?;
        let attached = n.attached;
        let leg = if matches!(o.kind, OrderType::TakeProfit | OrderType::StopLoss) {
            Some(if o.kind == OrderType::TakeProfit {
                "tp"
            } else {
                "sl"
            })
        } else {
            None
        };
        let full = n.full_position || o.amount == 0.0;
        if (attached || full) && amount.is_some() {
            return Err(
                "Bitunix full-position or attached TP/SL quantity cannot be amended".into(),
            );
        }
        let mut p = Payload::default();
        let mut fallback = None;
        let path = if !attached && leg.is_some() {
            if full {
                if n.position_id.is_empty() {
                    return Err("Missing positionId for Bitunix full-position amend".into());
                }
                p.symbol = Some(o.symbol.clone());
                p.position_id = Some(n.position_id.clone());
            } else {
                p.order_id = Some(n.raw_id.clone());
            }
            for kind in [OrderType::TakeProfit, OrderType::StopLoss] {
                let row = if kind == o.kind {
                    Some(o)
                } else {
                    cx.store
                        .orders()
                        .iter()
                        .filter(|r| {
                            r.symbol == o.symbol && r.kind == kind && r.status == OrderStatus::Open
                        })
                        .find(|r| {
                            self.account
                                .native
                                .get(&r.id)
                                .is_some_and(|meta| !meta.attached && meta.raw_id == n.raw_id)
                        })
                        .or_else(|| {
                            cx.store
                                .orders()
                                .iter()
                                .filter(|r| {
                                    r.symbol == o.symbol
                                        && r.kind == kind
                                        && r.status == OrderStatus::Open
                                })
                                .find(|r| {
                                    self.account.native.get(&r.id).is_some_and(|meta| {
                                        !meta.attached
                                            && !n.position_id.is_empty()
                                            && meta.position_id == n.position_id
                                    })
                                })
                        })
                };
                if let Some(row) = row {
                    let meta = self.account.native.get(&row.id).unwrap();
                    let tp = kind == OrderType::TakeProfit;
                    let v = positive(
                        num::adjust(
                            if row.id == id {
                                price.unwrap_or(row.price)
                            } else {
                                row.price
                            },
                            m.precision.price,
                        ),
                        "TP/SL price",
                    )?;
                    let qty = positive(
                        num::adjust(
                            if row.id == id {
                                amount.unwrap_or(row.amount)
                            } else {
                                row.amount
                            },
                            m.precision.amount,
                        ),
                        "TP/SL quantity",
                    );
                    append_protection(
                        &mut p,
                        tp,
                        v,
                        row.trigger_type,
                        (
                            if tp {
                                &meta.tp_execution
                            } else {
                                &meta.sl_execution
                            },
                            if tp { meta.tp_limit } else { meta.sl_limit },
                        ),
                        if full { None } else { Some(qty?) },
                        !full,
                    )?;
                }
            }
            if full { AMEND_FULL } else { AMEND_TPSL }
        } else {
            let parent = if attached {
                cx.store
                    .order(&n.raw_id)
                    .ok_or("Bitunix attached parent not found")?
            } else {
                o
            };
            if parent.kind != OrderType::Limit {
                return Err("Bitunix native amend requires a pending LIMIT order".into());
            }
            p.symbol = Some(parent.symbol.clone());
            p.order_id = Some(parent.id.clone());
            p.qty = Some(num::format(positive(
                num::adjust(amount.unwrap_or(parent.amount), m.precision.amount),
                "quantity",
            )?));
            if num::parse(p.qty.as_deref().unwrap()) <= parent.filled {
                return Err("Bitunix amended quantity must exceed filled quantity".into());
            }
            p.price = Some(num::format(positive(
                num::adjust(
                    if attached {
                        parent.price
                    } else {
                        price.unwrap_or(parent.price)
                    },
                    m.precision.price,
                ),
                "price",
            )?));
            for (tp, trigger, stop_type, execution, limit) in [
                (true, n.tp, n.tp_stop, &n.tp_execution, n.tp_limit),
                (false, n.sl, n.sl_stop, &n.sl_execution, n.sl_limit),
            ] {
                let value = if attached && leg == Some(if tp { "tp" } else { "sl" }) {
                    price
                } else {
                    trigger
                };
                if let Some(value) = value {
                    append_protection(
                        &mut p,
                        tp,
                        positive(num::adjust(value, m.precision.price), "protection trigger")?,
                        stop_type,
                        (execution, limit),
                        None,
                        true,
                    )?;
                }
            }
            if attached {
                if price.is_none() {
                    return Err("Bitunix attached amend requires trigger price".into());
                }
                if let Ok(Some(mut replace)) = self.replacement(cx, parent, n) {
                    replace.tp_price = p.tp_price.clone();
                    replace.sl_price = p.sl_price.clone();
                    fallback = Some(Box::new(replace));
                }
            }
            AMEND
        };
        Ok(Job {
            steps: VecDeque::from([Step {
                path,
                body: text(&p),
                call: Call::Amend {
                    id: id.into(),
                    parent: n.raw_id.clone(),
                    leg: if attached { leg } else { None },
                    fallback,
                },
                delay: 0.0,
            }]),
            ..Job::default()
        })
    }
    fn advance(&mut self, cx: &mut Cx<'_, Pending>, command: u32) {
        let Some(job) = self.trading.jobs.get_mut(&command) else {
            return;
        };
        let Some(mut step) = job.steps.pop_front() else {
            let job = self.trading.jobs.remove(&command).unwrap();
            cx.store
                .remove_orders(&job.created.iter().map(String::as_str).collect::<Vec<_>>());
            cx.done(command, Ok(text(&job.ids)));
            return;
        };
        if step.delay > 0.0 {
            let delay = step.delay;
            step.delay = 0.0;
            job.steps.push_front(step);
            job.timer = Some(cx.set_timer(delay));
            return;
        }
        if let Call::Cancel { ids, .. } = &step.call {
            self.account.generations[2] += 1;
            for id in ids {
                if cx.store.order(id).is_some_and(|row| {
                    matches!(row.status, OrderStatus::Closed | OrderStatus::Canceled)
                }) {
                    continue;
                }
                if !job.optimistic.contains(id)
                    && let Some(row) = cx.store.order(id)
                {
                    job.optimistic.push(id.clone());
                    job.restore.push(row.status);
                }

                cx.store.set_order_status(id, OrderStatus::Canceling);
            }
        }
        if let Call::ReconcileReplacement { parent, .. } = &step.call {
            self.signed_request(
                cx,
                Method::Get,
                step.path,
                Params::default().str("orderId", parent),
                None,
                Pending::Trade(Request {
                    command,
                    call: step.call,
                }),
            );
            return;
        }
        self.signed_request(
            cx,
            Method::Post,
            step.path,
            Params::default(),
            Some(step.body),
            Pending::Trade(Request {
                command,
                call: step.call,
            }),
        );
    }
    pub(crate) fn trade_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        let command = self.trading.jobs.iter().find_map(|(id, j)| {
            if j.timer == Some(timer) {
                Some(*id)
            } else {
                None
            }
        });
        if let Some(command) = command {
            self.trading.jobs.get_mut(&command).unwrap().timer = None;
            self.advance(cx, command);
            true
        } else {
            false
        }
    }
    pub(crate) fn trade_failed(&mut self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        if let Some(job) = self.trading.jobs.remove(&request.command) {
            cx.store
                .remove_orders(&job.created.iter().map(String::as_str).collect::<Vec<_>>());
            for (id, status) in job.optimistic.iter().zip(job.restore) {
                if cx
                    .store
                    .order(id)
                    .is_some_and(|o| o.status == OrderStatus::Canceling)
                {
                    cx.store.set_order_status(id, status);
                }
            }
        }
        self.reject(cx, request.command, error.into());
        self.debounce(cx, Axis::Orders);
    }
    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let command = request.command;
        let body = match data(response.body) {
            Ok(b) if (200..300).contains(&response.status) => b,
            Err(error) => {
                if error
                    .to_ascii_lowercase()
                    .contains("position does not exist")
                    && matches!(request.call, Call::Place { batch: false, .. })
                    && let Some(orders) = self
                        .trading
                        .jobs
                        .get_mut(&command)
                        .and_then(|j| j.recover.take())
                {
                    if let Some(job) = self.trading.jobs.remove(&command) {
                        cx.store.remove_orders(
                            &job.created.iter().map(String::as_str).collect::<Vec<_>>(),
                        );
                    }
                    for o in &orders {
                        self.account
                            .ws_position_ids
                            .remove(&format!("{}:long", o.symbol));
                        self.account
                            .ws_position_ids
                            .remove(&format!("{}:short", o.symbol));
                    }
                    self.trading.preparing.insert(
                        command,
                        Prepare {
                            orders,
                            stage: 0,
                            retried: true,
                        },
                    );
                    self.read_axis_for(cx, Axis::Positions, Some(command));
                    return;
                }
                if error
                    .to_ascii_lowercase()
                    .contains("order does not support this operation")
                    && let Call::Amend {
                        fallback: Some(p),
                        parent,
                        id,
                        leg,
                        ..
                    } = &request.call
                {
                    let symbol = p.symbol.clone().unwrap();
                    let ids: Vec<_> = cx
                        .store
                        .orders()
                        .iter()
                        .filter(|o| {
                            o.id == *parent
                                || self
                                    .account
                                    .native
                                    .get(&o.id)
                                    .is_some_and(|n| n.raw_id == *parent)
                        })
                        .map(|o| o.id.clone())
                        .collect();
                    let cancel = Step {
                        path: CANCEL,
                        body: text(&Cancel {
                            symbol: &symbol,
                            order_list: vec![Identity { order_id: parent }],
                        }),
                        call: Call::Cancel {
                            ids,
                            require_ack: true,
                        },
                        delay: 0.0,
                    };
                    if let Some(job) = self.trading.jobs.get_mut(&command) {
                        job.steps.push_front(Step {
                            path: "/api/v1/futures/trade/get_order_detail",
                            body: String::new(),
                            call: Call::ReconcileReplacement {
                                id: id.clone(),
                                parent: parent.clone(),
                                leg: *leg,
                                payload: p.clone(),
                            },
                            delay: 0.0,
                        });
                        job.steps.push_front(cancel);
                        self.advance(cx, command);
                        return;
                    }
                }
                self.trade_failed(cx, request, &error);
                return;
            }
            _ => {
                self.trade_failed(cx, request, &format!("Bitunix HTTP {}", response.status));
                return;
            }
        };
        let result = (|| {
            match &request.call {
                Call::Place { clients, batch } => {
                    #[derive(Deserialize, Default)]
                    #[serde(rename_all = "camelCase")]
                    struct Placed<'a> {
                        #[serde(borrow, default, alias = "id")]
                        order_id: Str<'a>,
                        #[serde(borrow, default)]
                        client_id: Str<'a>,
                    }
                    #[derive(Deserialize)]
                    #[serde(rename_all = "camelCase")]
                    struct BatchResult<'a> {
                        #[serde(borrow, default)]
                        success_list: Vec<Placed<'a>>,
                        #[serde(borrow, default)]
                        failure_list: Vec<Failure<'a>>,
                    }
                    #[derive(Deserialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Failure<'a> {
                        #[serde(borrow, default)]
                        error_msg: Str<'a>,
                        #[serde(default)]
                        error_code: Num,
                    }
                    let mut ids = vec![];
                    if *batch {
                        let rows: BatchResult<'_> =
                            serde_json::from_str(body).map_err(|e| e.to_string())?;
                        for row in rows.success_list {
                            if !row.order_id.is_empty() {
                                ids.push(row.order_id.into_owned());
                            } else if !row.client_id.is_empty() {
                                ids.push(row.client_id.into_owned());
                            }
                        }
                        for f in rows.failure_list {
                            cx.log(
                                LogLevel::Error,
                                format!("{} ({})", f.error_msg, f.error_code.0),
                            );
                        }
                    } else {
                        let row: Placed<'_> =
                            serde_json::from_str(body).map_err(|e| e.to_string())?;
                        ids.push(if !row.order_id.is_empty() {
                            row.order_id.into_owned()
                        } else if !row.client_id.is_empty() {
                            row.client_id.into_owned()
                        } else {
                            clients
                                .first()
                                .cloned()
                                .ok_or("Bitunix place response has no order ID")?
                        });
                    }
                    if let Some(job) = self.trading.jobs.get_mut(&command) {
                        job.ids.extend(ids);
                    }
                }
                Call::ReconcileReplacement {
                    parent,
                    id,
                    leg,
                    payload,
                } => {
                    #[derive(Deserialize)]
                    #[serde(rename_all = "camelCase")]
                    struct FinalOrder<'a> {
                        #[serde(borrow)]
                        order_id: Str<'a>,
                        #[serde(borrow)]
                        symbol: Str<'a>,
                        #[serde(borrow, alias = "orderStatus")]
                        status: Str<'a>,
                        qty: crate::wire::StrictNum,
                        trade_qty: crate::wire::StrictNum,
                    }
                    let row: FinalOrder<'_> =
                        serde_json::from_str(body).map_err(|e| e.to_string())?;
                    if row.order_id != parent.as_str()
                        || row.symbol != payload.symbol.as_deref().unwrap_or("")
                    {
                        return Err("Bitunix canceled order identity is not reconciled".into());
                    }
                    let status = row.status.to_ascii_uppercase();
                    if !matches!(status.as_str(), "CANCELED" | "CANCELLED" | "FILLED") {
                        return Err("Bitunix cancel not final; replacement was not placed".into());
                    }
                    let m = cx
                        .store
                        .market(&row.symbol)
                        .ok_or("Bitunix market missing")?;
                    if status == "FILLED" {
                        return Ok(());
                    }
                    if row.qty.0 <= 0.0
                        || row.trade_qty.0 < 0.0
                        || row.trade_qty.0 > row.qty.0
                        || self
                            .account
                            .filled
                            .get(parent)
                            .is_some_and(|(filled, _)| row.trade_qty.0 < *filled)
                    {
                        return Err("Bitunix canceled fill total is stale or invalid".into());
                    }
                    let remaining = num::adjust(row.qty.0 - row.trade_qty.0, m.precision.amount);
                    if !remaining.is_finite() || remaining < 0.0 {
                        return Err("Bitunix canceled quantity is not reconciled".into());
                    }
                    if remaining > 0.0 {
                        let mut p = (**payload).clone();
                        p.qty = Some(num::format(remaining));
                        if let Some(job) = self.trading.jobs.get_mut(&command) {
                            job.steps.push_front(Step {
                                path: PLACE,
                                body: text(&p),
                                call: if leg.is_some() {
                                    Call::Amend {
                                        id: id.clone(),
                                        parent: parent.clone(),
                                        leg: *leg,
                                        fallback: None,
                                    }
                                } else {
                                    Call::Place {
                                        clients: p.client_id.into_iter().collect(),
                                        batch: false,
                                    }
                                },
                                delay: 0.0,
                            });
                        }
                    }
                }
                Call::Cancel { ids, require_ack } => {
                    #[derive(Deserialize, Default)]
                    #[serde(rename_all = "camelCase")]
                    struct Item<'a> {
                        #[serde(borrow, default, alias = "id")]
                        order_id: Str<'a>,
                        #[serde(borrow, default)]
                        error_msg: Str<'a>,
                    }
                    #[derive(Deserialize, Default)]
                    #[serde(rename_all = "camelCase")]
                    struct Canceled<'a> {
                        #[serde(borrow, default)]
                        success_list: Vec<Item<'a>>,
                        #[serde(borrow, default)]
                        failure_list: Vec<Item<'a>>,
                    }
                    let ack: Canceled<'_> = if body.trim() == "null" {
                        Canceled::default()
                    } else {
                        serde_json::from_str(body).map_err(|e| e.to_string())?
                    };
                    let failed = !ack.failure_list.is_empty();
                    let succeeded: HashSet<_> = ack
                        .success_list
                        .iter()
                        .map(|a| a.order_id.as_ref())
                        .collect();
                    for id in ids {
                        let raw = self
                            .account
                            .native
                            .get(id)
                            .map_or(id.as_str(), |n| n.raw_id.as_str())
                            .to_owned();
                        if (!*require_ack && !failed) || succeeded.contains(raw.as_str()) {
                            cx.store.update_order(id, |row| {
                                if !matches!(
                                    row.status,
                                    OrderStatus::Closed | OrderStatus::Canceled
                                ) {
                                    row.status = OrderStatus::Canceled;
                                }
                            });
                        }
                    }
                    if *require_ack
                        && ids.iter().any(|id| {
                            !succeeded.contains(
                                self.account
                                    .native
                                    .get(id)
                                    .map_or(id.as_str(), |n| n.raw_id.as_str()),
                            )
                        })
                    {
                        return Err("Bitunix cancellation not acknowledged for every order; replacement was not placed".into());
                    }
                    if failed {
                        return Err(ack
                            .failure_list
                            .iter()
                            .map(|f| {
                                if f.error_msg.is_empty() {
                                    format!("Bitunix cancel failed for {}", f.order_id)
                                } else {
                                    f.error_msg.to_string()
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("; "));
                    }
                }

                Call::Amend {
                    id, parent, leg, ..
                } => {
                    #[derive(Deserialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Ack<'a> {
                        #[serde(borrow, default, alias = "id")]
                        order_id: Str<'a>,
                    }
                    let ack: Ack<'_> = serde_json::from_str(body).map_err(|e| e.to_string())?;
                    let raw = if ack.order_id.is_empty() {
                        parent.as_str()
                    } else {
                        ack.order_id.as_ref()
                    };
                    let id = if let Some(leg) = leg {
                        format!("{raw}__{leg}")
                    } else if id == parent {
                        raw.to_owned()
                    } else {
                        id.clone()
                    };
                    if let Some(job) = self.trading.jobs.get_mut(&command) {
                        job.ids.push(id);
                    }
                }
                Call::Leverage { symbol, value } => {
                    self.account.generations[Axis::Positions.index()] += 1;
                    cx.store.update_setting(symbol, |s| s.leverage = *value);
                    cx.store.update_positions(|p| {
                        if p.symbol == *symbol {
                            p.leverage = *value;
                        }
                    });
                }
                Call::Margin { symbol, isolated } => {
                    self.account.generations[Axis::Positions.index()] += 1;
                    let margin = if *isolated {
                        MarginType::Isolated
                    } else {
                        MarginType::Cross
                    };
                    cx.store
                        .update_setting(symbol, |s| s.margin_type = Some(margin));
                    cx.store.update_positions(|p| {
                        if p.symbol == *symbol {
                            p.margin_type = Some(margin);
                        }
                    });
                }
                Call::Mode { hedged } => {
                    self.account.generations[Axis::Balance.index()] += 1;
                    self.account.generations[Axis::Positions.index()] += 1;
                    cx.store.set_hedged(*hedged);
                    self.debounce(cx, Axis::Positions);
                }
            }
            Ok::<_, String>(())
        })();
        if let Err(error) = result {
            self.trade_failed(cx, request, &error);
            return;
        }
        self.account.generations[2] += 1;
        self.debounce(cx, Axis::Orders);
        self.advance(cx, command);
    }
}
fn affected_ids(cx: &Cx<'_, Pending>, n: &NativeOrder) -> Vec<String> {
    cx.store
        .orders()
        .iter()
        .filter(|o| o.id == n.raw_id || o.id.starts_with(&format!("{}__", n.raw_id)))
        .map(|o| o.id.clone())
        .collect()
}
fn append_protection(
    p: &mut Payload,
    tp: bool,
    price: f64,
    trigger: Option<StopTriggerType>,
    execution: (&str, f64),
    qty: Option<f64>,
    with_execution: bool,
) -> Result<(), String> {
    let (execution, limit) = execution;
    let execution = if execution.is_empty() {
        "MARKET"
    } else {
        execution
    };
    if !matches!(execution, "MARKET" | "LIMIT") {
        return Err("Bitunix protection execution type is not reconciled".into());
    }
    let limit = if with_execution && execution == "LIMIT" {
        Some(num::format(positive(limit, "protection LIMIT price")?))
    } else {
        None
    };
    if tp {
        p.tp_price = Some(num::format(price));
        p.tp_stop_type = Some(stop(trigger));
        p.tp_qty = qty.map(num::format);
        if with_execution {
            p.tp_order_type = Some(execution.into());
            p.tp_order_price = limit;
        }
    } else {
        p.sl_price = Some(num::format(price));
        p.sl_stop_type = Some(stop(trigger));
        p.sl_qty = qty.map(num::format);
        if with_execution {
            p.sl_order_type = Some(execution.into());
            p.sl_order_price = limit;
        }
    }
    Ok(())
}
