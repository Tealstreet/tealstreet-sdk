//! Preflighted command jobs. A lost write response is never retried.
use super::*;
use safe_cex_engine::model::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
const BROKER: &str = "b3cdedc0a20880ba";
#[derive(Default)]
pub(crate) struct Trading {
    queue: VecDeque<Command>,
    busy: bool,
    pause: Option<(TimerId, Job)>,
    joined: Option<Joined>,
}
struct Joined {
    job: Job,
    remaining: u8,
    error: Option<String>,
}
pub struct Request {
    job: Job,
    step: Step,
    joined: bool,
}
struct Job {
    command: CommandId,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    creating: Vec<Order>,
    affected: Vec<String>,
    replacement_targets: Vec<Target>,
    commit: Commit,
}
struct Step {
    receipts: Vec<OrderIdentityReceipt>,
    path: &'static str,
    body: String,
    effect: Effect,
    delay: f64,
}
enum Effect {
    Place { family: u8, count: usize },
    Cancel(Vec<Target>),
    Verify(Target),
    Setting,
}
#[derive(Default)]
enum Commit {
    #[default]
    None,
    Leverage {
        symbol: String,
        leverage: f64,
    },
    Margin(MarginType),
    Mode(bool),
}
#[derive(Clone)]
struct Target {
    id: String,
    inst_id: String,
    family: u8,
    store_ids: Vec<String>,
    filled: f64,
    filled_native: f64,
}
#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct Protection {
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_trigger_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_order_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_trigger_price_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_trigger_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_order_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_trigger_price_type: Option<String>,
}
impl Protection {
    fn merge_leg(&mut self, order: &Order, source: Option<&account::Source>) {
        let (execution, reference) = source.map_or((-1.0, "last"), |s| {
            if order.kind == OrderType::StopLoss {
                (s.sl_order_price, s.sl_type.as_str())
            } else {
                (s.tp_order_price, s.tp_type.as_str())
            }
        });
        let execution = if execution == 0.0 { -1.0 } else { execution };
        let reference = if reference.is_empty() {
            "last"
        } else {
            reference
        };
        if order.kind == OrderType::StopLoss {
            self.sl_trigger_price = Some(num::format(order.price));
            self.sl_order_price = Some(num::format(execution));
            self.sl_trigger_price_type = Some(reference.into());
        } else {
            self.tp_trigger_price = Some(num::format(order.price));
            self.tp_order_price = Some(num::format(execution));
            self.tp_trigger_price_type = Some(reference.into());
        }
    }
    fn retain_leg(&mut self, kind: OrderType) {
        if kind == OrderType::StopLoss {
            self.tp_trigger_price = None;
            self.tp_order_price = None;
            self.tp_trigger_price_type = None;
        } else {
            self.sl_trigger_price = None;
            self.sl_order_price = None;
            self.sl_trigger_price_type = None;
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(untagged)]
enum Reduce {
    Boolean(bool),
    Text(&'static str),
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Placement {
    inst_id: String,
    margin_mode: String,
    position_side: String,
    side: &'static str,
    size: String,
    client_order_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    broker_id: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reduce_only: Option<Reduce>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_price_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_price: Option<String>,
    #[serde(flatten)]
    protection: Protection,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    attach_algo_orders: Vec<Protection>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Cancel<'a> {
    inst_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tpsl_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    algo_id: Option<&'a str>,
}
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ResultRow<'a> {
    #[serde(borrow, default)]
    order_id: Str<'a>,
    #[serde(borrow, default)]
    tpsl_id: Str<'a>,
    #[serde(borrow, default)]
    algo_id: Str<'a>,
    #[serde(borrow, default)]
    code: Str<'a>,
    #[serde(borrow, default, deserialize_with = "nullable_text")]
    msg: Str<'a>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum ResultRows<'a> {
    Many(#[serde(borrow)] Vec<ResultRow<'a>>),
    One(#[serde(borrow)] ResultRow<'a>),
}
impl<'a> ResultRows<'a> {
    fn rows(self) -> Vec<ResultRow<'a>> {
        match self {
            Self::Many(v) => v,
            Self::One(v) => vec![v],
        }
    }
}
#[derive(Deserialize)]
struct Reply<'a> {
    #[serde(borrow)]
    code: Str<'a>,
    #[serde(borrow, default, deserialize_with = "nullable_text")]
    msg: Str<'a>,
    #[serde(borrow, default)]
    data: Option<ResultRows<'a>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrderDetail<'a> {
    #[serde(borrow)]
    inst_id: Str<'a>,
    #[serde(borrow)]
    order_id: Str<'a>,
    #[serde(borrow)]
    state: Str<'a>,
    #[serde(borrow)]
    filled_size: Str<'a>,
}
fn minted() -> Result<String, String> {
    Ok(format!("{BROKER}{}", safe_cex_crypto::uuid::uuid()?)
        .chars()
        .take(32)
        .collect())
}
fn trigger(value: Option<StopTriggerType>) -> &'static str {
    match value {
        Some(StopTriggerType::MarkPrice) => "mark",
        Some(StopTriggerType::IndexPrice) => "index",
        _ => "last",
    }
}
fn native_side(side: OrderSide) -> &'static str {
    if side == OrderSide::Buy {
        "buy"
    } else {
        "sell"
    }
}
fn margin(value: MarginType) -> &'static str {
    if value == MarginType::Isolated {
        "isolated"
    } else {
        "cross"
    }
}
fn positive(value: f64, label: &str) -> Result<(), String> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(format!("BloFin {label} must be positive"))
    }
}
impl Job {
    fn new(command: CommandId) -> Self {
        Self {
            command,
            steps: VecDeque::new(),
            ids: vec![],
            creating: vec![],
            affected: vec![],
            replacement_targets: vec![],
            commit: Commit::None,
        }
    }
}
impl Blofin {
    fn placement(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
        source: Option<&account::Source>,
    ) -> Result<Vec<(u8, Placement, Order)>, String> {
        let m = cx
            .store
            .market(&opts.symbol)
            .ok_or_else(|| format!("Unknown market {}", opts.symbol))?;
        if !self.account.mode_loaded {
            return Err("BloFin position mode is not loaded".into());
        }
        let margin_mode = cx
            .store
            .setting(&opts.symbol)
            .and_then(|s| s.margin_type)
            .or(self.account.margin)
            .ok_or("BloFin margin mode is not loaded")?;
        let protective = matches!(opts.kind, OrderType::StopLoss | OrderType::TakeProfit);
        if opts.time_in_force.is_some_and(|t| {
            t != TimeInForce::GoodTillCancel
                && (t != TimeInForce::PostOnly || opts.kind != OrderType::Limit)
        }) {
            return Err("Unsupported BloFin time in force".into());
        }
        let conditional = opts.kind == OrderType::Conditional;
        if opts.kind == OrderType::TrailingStopLoss {
            return Err("BloFin trailing stops are unsupported".into());
        }
        if conditional
            && (opts.reduce_only.unwrap_or(false)
                || opts.close.unwrap_or(false)
                || opts
                    .time_in_force
                    .is_some_and(|v| v != TimeInForce::GoodTillCancel))
        {
            return Err("Invalid BloFin opening trigger shape".into());
        }
        if !opts.amount.is_finite() || opts.amount < 0.0 || (!protective && opts.amount == 0.0) {
            return Err("BloFin amount must be positive".into());
        }
        let amount = num::adjust(opts.amount, m.precision.amount);
        if !protective || opts.amount > 0.0 {
            positive(amount, "amount")?;
        }
        let mut price = opts.price.map(|p| num::adjust(p, m.precision.price));
        if opts.kind == OrderType::Limit {
            positive(price.unwrap_or(0.0), "price")?;
        }
        if protective {
            positive(
                price
                    .or(if opts.kind == OrderType::StopLoss {
                        opts.stop_loss
                    } else {
                        opts.take_profit
                    })
                    .unwrap_or(0.0),
                "trigger price",
            )?;
        }
        if let Some(value) = opts.price {
            positive(value, "price")?;
        }
        for p in [opts.take_profit, opts.stop_loss, opts.trigger_price]
            .into_iter()
            .flatten()
        {
            positive(p, "trigger price")?;
        }
        if conditional {
            positive(
                num::adjust(opts.trigger_price.unwrap_or(0.0), m.precision.price),
                "trigger price",
            )?;
        }
        if let Some(id) = &opts.client_order_id
            && (id.is_empty()
                || id.len() > 32
                || !id.bytes().all(|c| c.is_ascii_alphanumeric())
                || !matches!(opts.kind, OrderType::Limit | OrderType::Market)
                || opts.trigger_price.is_some()
                || opts.stop_loss.is_some()
                || opts.take_profit.is_some()
                || opts.amount > m.limits.amount.max)
        {
            return Err("BloFin cannot honor caller clientOrderId on this order".into());
        }
        let reduce = protective || opts.reduce_only.unwrap_or(false);
        let side = if !cx.store.hedged() {
            "net"
        } else if (opts.side == OrderSide::Buy) ^ reduce {
            "long"
        } else {
            "short"
        };
        let tp = if opts.kind == OrderType::TakeProfit {
            price.or(opts.take_profit)
        } else {
            opts.take_profit
        };
        let sl = if opts.kind == OrderType::StopLoss {
            price.or(opts.stop_loss)
        } else {
            opts.stop_loss
        };
        let protection = Protection {
            tp_trigger_price: tp.map(num::format),
            tp_order_price: tp.map(|_| "-1".into()),
            tp_trigger_price_type: if conditional || protective {
                tp.map(|_| trigger(opts.tp_trigger_type).into())
            } else {
                None
            },
            sl_trigger_price: sl.map(num::format),
            sl_order_price: sl.map(|_| "-1".into()),
            sl_trigger_price_type: if conditional || protective {
                sl.map(|_| trigger(opts.sl_trigger_type).into())
            } else {
                None
            },
        };
        let mut payload = Placement {
            broker_id: self.options.broker_attribution.then_some(BROKER),
            inst_id: m.id.clone(),
            margin_mode: margin(margin_mode).into(),
            position_side: side.into(),
            side: native_side(opts.side),
            size: num::format(if protective && amount == 0.0 {
                -1.0
            } else {
                amount
            }),
            client_order_id: opts
                .client_order_id
                .clone()
                .map(Ok)
                .unwrap_or_else(minted)?,
            order_type: if protective {
                None
            } else if conditional {
                Some("trigger")
            } else if opts.kind == OrderType::Market {
                Some("market")
            } else if opts.time_in_force == Some(TimeInForce::PostOnly) {
                Some("post_only")
            } else {
                Some("limit")
            },
            price: if opts.kind == OrderType::Limit {
                price.take().map(num::format)
            } else {
                None
            },
            reduce_only: if protective {
                Some(Reduce::Text("true"))
            } else if reduce {
                Some(Reduce::Boolean(true))
            } else {
                None
            },
            trigger_price: if conditional {
                opts.trigger_price
                    .map(|p| num::format(num::adjust(p, m.precision.price)))
            } else {
                None
            },
            trigger_price_type: conditional.then(|| "last".into()),
            order_price: conditional.then(|| {
                num::format(
                    opts.price
                        .map(|p| num::adjust(p, m.precision.price))
                        .unwrap_or(-1.0),
                )
            }),
            protection: if conditional {
                Protection::default()
            } else {
                protection.clone()
            },
            attach_algo_orders: if !protective && (tp.is_some() || sl.is_some()) {
                vec![protection]
            } else {
                vec![]
            },
        };
        if let Some(source) = source {
            if source.inst_id != m.id
                || !matches!(source.margin_mode.as_str(), "cross" | "isolated")
                || !matches!(source.position_side.as_str(), "net" | "long" | "short")
            {
                if conditional {
                    return Err("BloFin replacement provenance is incomplete".into());
                }
            } else {
                payload.margin_mode = source.margin_mode.clone();
                payload.position_side = source.position_side.clone();
            }
            if !conditional {
                if payload.protection.tp_trigger_price.is_some() {
                    payload.protection.tp_order_price =
                        Some(num::format(if source.tp_order_price == 0.0 {
                            -1.0
                        } else {
                            source.tp_order_price
                        }));
                }
                if payload.protection.sl_trigger_price.is_some() {
                    payload.protection.sl_order_price =
                        Some(num::format(if source.sl_order_price == 0.0 {
                            -1.0
                        } else {
                            source.sl_order_price
                        }));
                }
                if payload.protection.tp_trigger_price.is_some()
                    && matches!(source.tp_type.as_str(), "last" | "mark" | "index")
                {
                    payload.protection.tp_trigger_price_type = Some(source.tp_type.clone());
                }
                if payload.protection.sl_trigger_price.is_some()
                    && matches!(source.sl_type.as_str(), "last" | "mark" | "index")
                {
                    payload.protection.sl_trigger_price_type = Some(source.sl_type.clone());
                }
            }
            if !conditional && !protective && (!payload.attach_algo_orders.is_empty()) {
                payload.attach_algo_orders = vec![payload.protection.clone()];
            }
            if conditional {
                if !matches!(
                    source.trigger_price_type.as_str(),
                    "last" | "mark" | "index"
                ) || !(source.order_price == -1.0 || source.order_price > 0.0)
                {
                    return Err("BloFin trigger replacement provenance is incomplete".into());
                }
                payload.trigger_price_type = Some(source.trigger_price_type.clone());
                payload.order_price = Some(num::format(source.order_price));
                if source.tp > 0.0 || source.sl > 0.0 {
                    payload.attach_algo_orders = vec![Protection {
                        tp_trigger_price: (source.tp > 0.0).then(|| num::format(source.tp)),
                        sl_trigger_price: (source.sl > 0.0).then(|| num::format(source.sl)),
                        tp_order_price: (source.tp > 0.0).then(|| {
                            num::format(if source.tp_order_price == 0.0 {
                                -1.0
                            } else {
                                source.tp_order_price
                            })
                        }),
                        sl_order_price: (source.sl > 0.0).then(|| {
                            num::format(if source.sl_order_price == 0.0 {
                                -1.0
                            } else {
                                source.sl_order_price
                            })
                        }),
                        tp_trigger_price_type: (source.tp > 0.0).then(|| {
                            if source.tp_type.is_empty() {
                                "last".into()
                            } else {
                                source.tp_type.clone()
                            }
                        }),
                        sl_trigger_price_type: (source.sl > 0.0).then(|| {
                            if source.sl_type.is_empty() {
                                "last".into()
                            } else {
                                source.sl_type.clone()
                            }
                        }),
                    }];
                }
            }
        }
        let family = if conditional {
            2
        } else if protective {
            1
        } else {
            0
        };
        let quantities = if family == 0 && amount > m.limits.amount.max {
            positive(m.limits.amount.max, "maximum size")?;
            let lots = (amount / m.limits.amount.max).ceil() as usize;
            if lots > 1000 {
                return Err("BloFin order requires too many splits".into());
            }
            let step = m.precision.amount;
            let units = (amount / step).round() as u64;
            let each = units / lots as u64;
            let remainder = units % lots as u64;
            (0..lots)
                .map(|i| (each + u64::from((i as u64) < remainder)) as f64 * step)
                .collect::<Vec<_>>()
        } else {
            vec![amount]
        };
        let mut result = Vec::new();
        for quantity in quantities {
            let mut p = payload.clone();
            if !result.is_empty() {
                p.client_order_id = minted()?;
            }
            p.size = num::format(if protective && quantity == 0.0 {
                -1.0
            } else {
                num::adjust(quantity, m.precision.amount)
            });
            let base_amount = quantity * m.contract_size.unwrap_or(1.0);
            result.push((
                family,
                p.clone(),
                Order {
                    id: format!("creating:{}", p.client_order_id),
                    parent_id: None,
                    status: OrderStatus::Creating,
                    symbol: opts.symbol.clone(),
                    kind: opts.kind,
                    side: opts.side,
                    price: if conditional {
                        opts.trigger_price.unwrap_or(0.0)
                    } else {
                        opts.price.unwrap_or(0.0)
                    },
                    amount: base_amount,
                    filled: 0.0,
                    remaining: base_amount,
                    reduce_only: reduce,
                    close: None,
                    client_order_id: Some(p.client_order_id),
                    time_in_force: opts.time_in_force,
                    last_update: cx.now,
                    trigger_type: opts.trigger_type,
                    take_profit: tp,
                    stop_loss: sl,
                    extra: None,
                },
            ));
        }
        Ok(result)
    }
    fn add_placements(job: &mut Job, rows: Vec<(u8, Placement, Order)>) {
        if !rows.is_empty() {
            job.replacement_targets = job
                .steps
                .iter()
                .filter_map(|step| {
                    if let Effect::Cancel(targets) = &step.effect {
                        Some(targets.clone())
                    } else {
                        None
                    }
                })
                .flatten()
                .collect();
            for target in job.replacement_targets.iter().filter(|t| t.family == 0) {
                job.steps.push_back(Step {
                    receipts: Vec::new(),
                    path: "/api/v1/trade/order-detail",
                    body: String::new(),
                    effect: Effect::Verify(target.clone()),
                    delay: 0.0,
                });
            }
        }
        let mut normal = Vec::new();
        let mut normal_receipts = Vec::new();
        let mut special = Vec::new();
        for (family, p, order) in rows {
            let receipt = OrderIdentityReceipt {
                symbol: order.symbol.clone(),
                kind: OrderIdentityKind::ClientOrderId,
                identity: p.client_order_id.clone(),
            };
            job.creating.push(order);
            if family == 0 {
                normal.push(p);
                normal_receipts.push(receipt);
            } else {
                special.push((family, p, receipt));
            }
        }
        // Keep caller order while respecting BloFin's homogeneous-batch rule.
        let mut start = 0;
        while start < normal.len() {
            let first = &normal[start];
            let count = normal[start..]
                .iter()
                .take(20)
                .take_while(|p| p.inst_id == first.inst_id && p.margin_mode == first.margin_mode)
                .count();
            let batch = &normal[start..start + count];
            let receipts = normal_receipts[start..start + count].to_vec();
            start += count;
            job.steps.push_back(Step {
                receipts,
                path: "/api/v1/trade/batch-orders",
                body: serde_json::to_string(batch).unwrap(),
                effect: Effect::Place {
                    family: 0,
                    count: batch.len(),
                },
                delay: 350.0,
            });
        }
        for (family, payload, receipt) in special {
            job.steps.push_back(Step {
                receipts: vec![receipt],
                path: if family == 1 {
                    "/api/v1/trade/order-tpsl"
                } else {
                    "/api/v1/trade/order-algo"
                },
                body: serde_json::to_string(&payload).unwrap(),
                effect: Effect::Place { family, count: 1 },
                delay: 350.0,
            });
        }
    }
    fn cancel_targets(&self, cx: &Cx<'_, Pending>, ids: &[String]) -> Result<Vec<Target>, String> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for id in ids {
            let order = cx.store.order(id).ok_or_else(|| {
                format!("Order {id} not found; cancellation requires a reconciled order")
            })?;
            if matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled) {
                continue;
            }
            let m = cx
                .store
                .market(&order.symbol)
                .ok_or("Unknown cancel market")?;
            let family = if order.kind == OrderType::Conditional {
                2
            } else if matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit) {
                1
            } else {
                0
            };
            let real = id.split("__").next().unwrap_or(id);
            let source = self.account.sources.get(id);
            if family == 2 && source.is_none_or(|s| s.algo_id != *id || s.inst_id != m.id) {
                return Err("BloFin conditional cancel requires authoritative provenance".into());
            }
            // Inline fallback legs share the normal parent's native cancel target.
            let family = if family == 1
                && source.is_some_and(|s| s.tpsl_id.is_empty() && !s.order_id.is_empty())
            {
                0
            } else {
                family
            };
            if seen.insert((family, real.to_string())) {
                let store_ids = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.id == real || o.id.split("__").next() == Some(real))
                    .map(|o| o.id.clone())
                    .collect();
                let native_order = cx.store.order(real).unwrap_or(order);
                let filled_native = native_order
                    .extra
                    .as_ref()
                    .and_then(|e| e.filled_in_placement_units)
                    .unwrap_or(native_order.filled / m.contract_size.unwrap_or(1.0));
                out.push(Target {
                    id: real.to_string(),
                    inst_id: m.id.clone(),
                    family,
                    store_ids,
                    filled: native_order.filled,
                    filled_native,
                });
            }
        }
        Ok(out)
    }
    fn add_cancels(job: &mut Job, targets: Vec<Target>) {
        for family in 0..3 {
            let rows: Vec<_> = targets
                .iter()
                .filter(|t| t.family == family)
                .cloned()
                .collect();
            for batch in rows.chunks(if family == 0 {
                20
            } else if family == 1 {
                10
            } else {
                1
            }) {
                let payload: Vec<_> = batch
                    .iter()
                    .map(|t| Cancel {
                        inst_id: &t.inst_id,
                        order_id: (family == 0).then_some(t.id.as_str()),
                        tpsl_id: (family == 1).then_some(t.id.as_str()),
                        algo_id: (family == 2).then_some(t.id.as_str()),
                    })
                    .collect();
                job.affected
                    .extend(batch.iter().flat_map(|t| t.store_ids.clone()));
                job.steps.push_back(Step {
                    receipts: Vec::new(),
                    path: match family {
                        0 => "/api/v1/trade/cancel-batch-orders",
                        1 => "/api/v1/trade/cancel-tpsl",
                        _ => "/api/v1/trade/cancel-algo",
                    },
                    body: if family == 2 {
                        serde_json::to_string(&payload[0]).unwrap()
                    } else {
                        serde_json::to_string(&payload).unwrap()
                    },
                    effect: Effect::Cancel(batch.to_vec()),
                    delay: if family == 1 { 1000.0 } else { 350.0 },
                });
            }
        }
    }
    fn replacement(
        &self,
        cx: &Cx<'_, Pending>,
        order: &Order,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<Vec<(u8, Placement, Order)>, String> {
        if matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled) {
            return Err("Cannot amend a terminal BloFin order".into());
        }
        if amount.is_some_and(|v| !v.is_finite() || v < 0.0) {
            return Err("BloFin replacement amount is invalid".into());
        }
        if let Some(price) = price {
            positive(price, "amend price")?;
        }
        let m = cx
            .store
            .market(&order.symbol)
            .ok_or("Unknown replacement market")?;
        if let Some(price) = price {
            positive(num::adjust(price, m.precision.price), "amend price")?;
        }
        let source = self.account.sources.get(&order.id);
        if matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit)
            && let Some(parent) = source
                .filter(|s| s.tpsl_id.is_empty() && !s.order_id.is_empty())
                .and_then(|s| cx.store.order(&s.order_id))
        {
            // Editing attached protection cannot resize its entry.
            let mut rows = self.replacement(cx, parent, None, None)?;
            for (_, p, _) in &mut rows {
                let value =
                    num::format(num::adjust(price.unwrap_or(order.price), m.precision.price));
                if order.kind == OrderType::StopLoss {
                    p.protection.sl_trigger_price = Some(value);
                    p.protection
                        .sl_order_price
                        .get_or_insert_with(|| "-1".into());
                } else {
                    p.protection.tp_trigger_price = Some(value);
                    p.protection
                        .tp_order_price
                        .get_or_insert_with(|| "-1".into());
                }
                p.attach_algo_orders = vec![p.protection.clone()];
            }
            return Ok(rows);
        }
        if order.kind == OrderType::Conditional && source.is_none() {
            return Err("BloFin trigger replacement requires provenance".into());
        }
        if let Some(price) = price {
            positive(num::adjust(price, m.precision.price), "amend price")?;
        }
        let qty = amount
            .or_else(|| order.extra.as_ref().and_then(|e| e.remaining_native))
            .unwrap_or(order.remaining / m.contract_size.unwrap_or(1.0));
        let raw = PlaceOrderOpts {
            symbol: order.symbol.clone(),
            kind: order.kind,
            side: order.side,
            amount: if amount.is_none()
                && order.amount <= 0.0
                && matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit)
            {
                0.0
            } else {
                qty
            },
            price: if order.kind == OrderType::Conditional {
                source
                    .filter(|s| s.order_price > 0.0)
                    .map(|s| s.order_price)
            } else {
                price.or(Some(order.price))
            },
            trigger_price: (order.kind == OrderType::Conditional)
                .then(|| price.unwrap_or(order.price)),
            stop_loss: source.filter(|s| s.sl > 0.0).map(|s| s.sl),
            take_profit: source.filter(|s| s.tp > 0.0).map(|s| s.tp),
            reduce_only: Some(order.reduce_only),
            time_in_force: order.time_in_force,
            trigger_type: order.trigger_type,
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
        self.placement(cx, &raw, source)
    }
    pub(crate) fn trading_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let (id, plan) = self.plan_trade(cx, command.clone());
        match plan {
            Ok(_) => {
                self.trading.queue.push_back(command);
                self.trade_next(cx);
            }
            Err(e) => cx.done(id, Err(e)),
        }
    }
    fn plan_trade(
        &self,
        cx: &Cx<'_, Pending>,
        command: Command,
    ) -> (CommandId, Result<Job, String>) {
        let command_id = match &command {
            Command::PlaceOrders { command, .. }
            | Command::CancelOrders { command, .. }
            | Command::AmendOrder { command, .. }
            | Command::CancelSymbolOrders { command, .. }
            | Command::SetLeverage { command, .. }
            | Command::SetMarginType { command, .. }
            | Command::SetPositionMode { command, .. } => *command,
            _ => return (0, Err("Unsupported trading command".into())),
        };
        let plan = (|| {
            if !self.signed() {
                return Err("BloFin trading requires API credentials".into());
            }
            if !self.account.mode_loaded || self.account.margin.is_none() {
                return Err("BloFin account settings are not loaded".into());
            }
            let mut job = Job::new(command_id);
            match command {
                Command::PlaceOrders { orders, .. } => {
                    if orders.iter().any(|order| {
                        order.bybit_bbo.is_some() || order.bybit_slippage_tolerance_ticks.is_some()
                    }) {
                        return Err(
                            "Bybit BBO and TickSize controls are not supported by BloFin".into(),
                        );
                    }
                    let mut seen = HashSet::new();
                    let mut rows = Vec::new();
                    for opts in &orders {
                        if let Some(id) = &opts.client_order_id
                            && !seen.insert(id)
                        {
                            return Err("Duplicate caller clientOrderId".into());
                        }
                        rows.extend(self.placement(cx, opts, None)?);
                    }
                    // Combine an existing opposite full-position leg before writing.
                    let mut cancel_ids = Vec::new();
                    for (family, p, order) in &mut rows {
                        if *family != 1 || order.amount != 0.0 {
                            continue;
                        }
                        for opposite in cx.store.orders().iter().filter(|o| {
                            !matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled)
                                && o.symbol == order.symbol
                                && o.side == order.side
                                && o.amount <= 0.0
                                && matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
                                && o.kind != order.kind
                        }) {
                            p.protection
                                .merge_leg(opposite, self.account.sources.get(&opposite.id));
                            cancel_ids.push(opposite.id.clone());
                        }
                    }
                    Self::add_cancels(&mut job, self.cancel_targets(cx, &cancel_ids)?);
                    Self::add_placements(&mut job, rows);
                }
                Command::CancelOrders { ids, .. } => {
                    let mut preserved = Vec::new();
                    let mut seen = HashSet::new();
                    for id in &ids {
                        if let Some(order) = cx.store.order(id)
                            && !matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled)
                            && order.amount <= 0.0
                            && matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit)
                        {
                            let real = id.split("__").next().unwrap_or(id);
                            for sibling in cx.store.orders().iter().filter(|o| {
                                !matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled)
                                    && o.id != *id
                                    && !ids.contains(&o.id)
                                    && o.id.split("__").next() == Some(real)
                                    && o.amount <= 0.0
                            }) {
                                if seen.insert(sibling.id.clone()) {
                                    let mut rows =
                                        self.replacement(cx, sibling, None, Some(0.0))?;
                                    for (_, p, _) in &mut rows {
                                        p.protection.retain_leg(sibling.kind);
                                        p.attach_algo_orders.clear();
                                    }
                                    preserved.extend(rows);
                                }
                            }
                        }
                    }
                    Self::add_cancels(&mut job, self.cancel_targets(cx, &ids)?);
                    Self::add_placements(&mut job, preserved);
                }
                Command::CancelSymbolOrders { symbol, .. } => {
                    let ids = cx
                        .store
                        .orders()
                        .iter()
                        .filter(|o| o.symbol == symbol && o.status == OrderStatus::Open)
                        .map(|o| o.id.clone())
                        .collect::<Vec<_>>();
                    Self::add_cancels(&mut job, self.cancel_targets(cx, &ids)?);
                }
                Command::AmendOrder {
                    id, price, amount, ..
                } => {
                    let order = cx.store.order(&id).ok_or("Unknown order to amend")?;
                    let mut rows = self.replacement(cx, order, price, amount)?;
                    let real = id.split("__").next().unwrap_or(&id);
                    // Preserve every other protection on a shared native target.
                    for sibling in cx.store.orders().iter().filter(|o| {
                        !matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled)
                            && o.id != id
                            && o.id.split("__").next() == Some(real)
                            && matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
                    }) {
                        for (family, p, _) in &mut rows {
                            if *family == 0 || *family == 2 {
                                let protection = if *family == 2 {
                                    if p.attach_algo_orders.is_empty() {
                                        p.attach_algo_orders.push(Protection::default());
                                    }
                                    &mut p.attach_algo_orders[0]
                                } else {
                                    &mut p.protection
                                };
                                protection
                                    .merge_leg(sibling, self.account.sources.get(&sibling.id));
                                if *family == 0 {
                                    p.attach_algo_orders = vec![p.protection.clone()];
                                }
                            } else if sibling.kind != order.kind {
                                p.protection
                                    .merge_leg(sibling, self.account.sources.get(&sibling.id));
                            }
                        }
                    }
                    Self::add_cancels(&mut job, self.cancel_targets(cx, &[id])?);
                    Self::add_placements(&mut job, rows);
                }
                Command::SetLeverage {
                    symbol, leverage, ..
                } => {
                    positive(leverage, "leverage")?;
                    let market = cx.store.market(&symbol).ok_or("Unknown leverage market")?;
                    let leverage =
                        leverage.clamp(market.limits.leverage.min, market.limits.leverage.max);
                    let mode = self.account.margin.unwrap();
                    #[derive(Serialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Leverage<'a> {
                        inst_id: &'a str,
                        leverage: String,
                        margin_mode: &'static str,
                        #[serde(skip_serializing_if = "Option::is_none")]
                        position_side: Option<&'static str>,
                    }
                    let sides = if mode == MarginType::Isolated && cx.store.hedged() {
                        vec![Some("long"), Some("short")]
                    } else {
                        vec![None]
                    };
                    for side in sides {
                        job.steps.push_back(Step {
                            receipts: Vec::new(),
                            path: "/api/v1/account/set-leverage",
                            body: serde_json::to_string(&Leverage {
                                inst_id: &market.id,
                                leverage: num::format(leverage),
                                margin_mode: margin(mode),
                                position_side: side,
                            })
                            .unwrap(),
                            effect: Effect::Setting,
                            delay: 0.0,
                        });
                    }
                    job.commit = Commit::Leverage { symbol, leverage };
                }
                Command::SetMarginType { isolated, .. } => {
                    let mode = if isolated {
                        MarginType::Isolated
                    } else {
                        MarginType::Cross
                    };
                    job.steps.push_back(Step {
                        receipts: Vec::new(),
                        path: "/api/v1/account/set-margin-mode",
                        body: format!("{{\"marginMode\":\"{}\"}}", margin(mode)),
                        effect: Effect::Setting,
                        delay: 0.0,
                    });
                    job.commit = Commit::Margin(mode);
                }
                Command::SetPositionMode { hedged, .. } => {
                    job.steps.push_back(Step {
                        receipts: Vec::new(),
                        path: "/api/v1/account/set-position-mode",
                        body: format!(
                            "{{\"positionMode\":\"{}\"}}",
                            if hedged {
                                "long_short_mode"
                            } else {
                                "net_mode"
                            }
                        ),
                        effect: Effect::Setting,
                        delay: 0.0,
                    });
                    job.commit = Commit::Mode(hedged);
                }
                _ => unreachable!(),
            }
            Ok(job)
        })();
        (command_id, plan)
    }
    fn trade_next(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading.busy {
            return;
        }
        if let Some(command) = self.trading.queue.pop_front() {
            let (id, plan) = self.plan_trade(cx, command);
            let job = match plan {
                Ok(job) => job,
                Err(e) => {
                    cx.done(id, Err(e));
                    self.trade_next(cx);
                    return;
                }
            };
            self.trading.busy = true;
            self.account.orders_revision += 1;
            for order in &job.creating {
                cx.store.upsert_order(order.clone());
            }
            for id in &job.affected {
                cx.store.set_order_status(id, OrderStatus::Canceling);
            }
            self.trade_step(cx, job);
        }
    }
    fn trade_step(&mut self, cx: &mut Cx<'_, Pending>, mut job: Job) {
        if matches!(&job.commit, Commit::Leverage { .. }) && job.steps.len() == 2 {
            let mut prepared = Vec::new();
            while let Some(step) = job.steps.pop_front() {
                match self.headers(step.path, Method::Post, cx.now, &step.body) {
                    Ok(headers) => prepared.push((step, headers)),
                    Err(e) => {
                        self.trade_failed(
                            cx,
                            Request {
                                job,
                                step,
                                joined: false,
                            },
                            &e,
                        );
                        return;
                    }
                }
            }
            let command = job.command;
            self.trading.joined = Some(Joined {
                job,
                remaining: 2,
                error: None,
            });
            for (step, headers) in prepared {
                cx.request(
                    Method::Post,
                    format!("{}{}", self.host(), step.path),
                    headers,
                    Some(step.body.clone()),
                    Pending::Trade(Request {
                        job: Job::new(command),
                        step,
                        joined: true,
                    }),
                );
            }
            return;
        }
        let Some(step) = job.steps.pop_front() else {
            self.cleanup_job(cx, &job);
            match &job.commit {
                Commit::None => {}
                Commit::Leverage { symbol, leverage } => {
                    self.account.settings_revision += 1;
                    self.account.positions_revision += 1;
                    cx.store.update_setting(symbol, |s| {
                        s.leverage = *leverage;
                        s.buy_leverage = Some(*leverage);
                        s.sell_leverage = Some(*leverage);
                    });
                    cx.store.update_positions(|p| {
                        if p.symbol == *symbol {
                            p.leverage = *leverage;
                        }
                    });
                }
                Commit::Margin(mode) => {
                    self.account.settings_revision += 1;
                    self.account.positions_revision += 1;
                    self.account.margin = Some(*mode);
                    let symbols = cx
                        .store
                        .markets()
                        .iter()
                        .map(|m| m.symbol.clone())
                        .collect::<Vec<_>>();
                    for symbol in symbols {
                        if !cx
                            .store
                            .positions()
                            .iter()
                            .any(|p| p.symbol == symbol && p.contracts > 0.0)
                        {
                            cx.store
                                .update_setting(&symbol, |s| s.margin_type = Some(*mode));
                        }
                    }
                    self.refresh_account(cx);
                }
                Commit::Mode(hedged) => {
                    self.account.settings_revision += 1;
                    self.account.positions_revision += 1;
                    cx.store.set_hedged(*hedged);
                }
            }
            cx.done(job.command, Ok(serde_json::to_string(&job.ids).unwrap()));
            self.trading.busy = false;
            self.trade_next(cx);
            return;
        };
        if matches!(step.effect, Effect::Place { .. } | Effect::Verify(_))
            && job.replacement_targets.iter().any(|target| {
                self.filled_native(&target.id) > target.filled_native
                    || cx.store.order(&target.id).is_some_and(|o| {
                        o.status == OrderStatus::Closed || o.filled > target.filled
                    })
            })
        {
            self.trade_failed(
                cx,
                Request {
                    job,
                    step,
                    joined: false,
                },
                "BloFin replacement aborted after execution progress",
            );
            return;
        }
        let (method, path) = if let Effect::Verify(target) = &step.effect {
            (
                Method::Get,
                format!(
                    "{}?instId={}&orderId={}",
                    step.path, target.inst_id, target.id
                ),
            )
        } else {
            (Method::Post, step.path.to_owned())
        };
        let headers = match self.headers(&path, method, cx.now, &step.body) {
            Ok(h) => h,
            Err(e) => {
                self.trade_failed(
                    cx,
                    Request {
                        job,
                        step,
                        joined: false,
                    },
                    &e,
                );
                return;
            }
        };
        cx.command_progress(job.command, step.receipts.clone());
        cx.request(
            method,
            format!("{}{}", self.host(), path),
            headers,
            (method == Method::Post).then(|| step.body.clone()),
            Pending::Trade(Request {
                job,
                step,
                joined: false,
            }),
        );
    }
    fn cleanup_job(&mut self, cx: &mut Cx<'_, Pending>, job: &Job) {
        let ids: Vec<_> = job.creating.iter().map(|o| o.id.as_str()).collect();
        cx.store.remove_orders(&ids);
        for id in &job.affected {
            if cx
                .store
                .order(id)
                .is_some_and(|o| o.status == OrderStatus::Canceling)
            {
                cx.store.set_order_status(id, OrderStatus::Open);
            }
        }
    }
    pub(crate) fn trade_failed(&mut self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        if request.joined {
            self.finish_join(cx, Some(error.to_owned()));
            return;
        }
        self.cleanup_job(cx, &request.job);
        cx.done(request.job.command, Err(error.to_owned()));
        self.trading.busy = false;
        self.trade_next(cx);
    }
    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) {
            self.trade_failed(
                cx,
                request,
                &format!("BloFin HTTP {}: {}", response.status, response.body),
            );
            return;
        }
        if let Effect::Verify(target) = &request.step.effect {
            let verified = check(&response).is_ok()
                && serde_json::from_str::<Data<OneOrMany<OrderDetail<'_>>>>(response.body)
                    .ok()
                    .is_some_and(|reply| {
                        let row = match &reply.data {
                            OneOrMany::One(row) => Some(row),
                            OneOrMany::Many(rows) if rows.len() == 1 => rows.first(),
                            _ => None,
                        };
                        row.is_some_and(|row| {
                            row.inst_id == target.inst_id
                                && row.order_id == target.id
                                && row.state == "canceled"
                                && row.filled_size.parse::<f64>().ok().is_some_and(|filled| {
                                    filled.is_finite()
                                        && filled >= 0.0
                                        && filled == target.filled_native
                                })
                        })
                    });
            if !verified {
                self.trade_failed(
                    cx,
                    request,
                    "BloFin replacement final order verification failed",
                );
            } else {
                self.trade_step(cx, request.job);
            }
            return;
        }
        let reply = match serde_json::from_str::<Reply<'_>>(response.body) {
            Ok(r) => r,
            Err(e) => {
                self.trade_failed(cx, request, &e.to_string());
                return;
            }
        };
        let rows = reply.data.map(ResultRows::rows).unwrap_or_default();
        let mut error = None;
        match &request.step.effect {
            Effect::Place { family, count } => {
                if reply.code != "0" || rows.len() != *count {
                    error = Some(format!("BloFin placement {}: {}", reply.code, reply.msg));
                }
                let mut seen = HashSet::new();
                for row in rows {
                    let id = match family {
                        0 => row.order_id,
                        1 => row.tpsl_id,
                        _ => row.algo_id,
                    };
                    let message = row.msg.trim().to_ascii_lowercase();
                    if id.is_empty()
                        || !seen.insert(id.to_string())
                        || (!row.code.is_empty() && row.code != "0")
                        || !matches!(message.as_str(), "" | "success" | "order placed")
                    {
                        error = Some(format!("BloFin placement rejected: {}", row.msg));
                    } else {
                        request.job.ids.push(id.into_owned());
                    }
                }
            }
            Effect::Cancel(targets) => {
                let mut seen = HashSet::new();
                if rows.len() != targets.len() {
                    error = Some("BloFin cancellation response count mismatch".into());
                }
                for row in rows {
                    let found = targets.iter().find(|t| match t.family {
                        0 => row.order_id == t.id,
                        1 => row.tpsl_id == t.id || row.algo_id == t.id,
                        _ => row.algo_id == t.id,
                    });
                    let Some(target) = found else {
                        error = Some("BloFin cancellation returned an unrelated identity".into());
                        continue;
                    };
                    if !seen.insert(target.id.clone()) {
                        error = Some("BloFin cancellation returned duplicate identities".into());
                        continue;
                    }
                    let gone = row.code == "1000"
                        || row
                            .msg
                            .contains("filled, triggered, canceled or does not exist");
                    if gone && !request.job.replacement_targets.is_empty() {
                        error = Some("BloFin replacement requires a confirmed cancellation".into());
                    }
                    if gone || (reply.code == "0" && (row.code.is_empty() || row.code == "0")) {
                        self.account.orders_revision += 1;
                        if target.family != 1 {
                            self.remember_terminal(&target.id);
                        }
                        for id in &target.store_ids {
                            self.remember_terminal(id);
                        }
                        let ids = target
                            .store_ids
                            .iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>();
                        if gone {
                            cx.store.remove_orders(&ids);
                        } else {
                            for id in &target.store_ids {
                                if !cx
                                    .store
                                    .order(id)
                                    .is_some_and(|o| o.status == OrderStatus::Closed)
                                {
                                    cx.store.set_order_status(id, OrderStatus::Canceled);
                                }
                            }
                        }
                        for id in &target.store_ids {
                            self.account.sources.remove(id);
                        }
                    } else {
                        error = Some(format!("BloFin cancellation rejected: {}", row.msg));
                    }
                }
            }
            Effect::Verify(_) => unreachable!(),
            Effect::Setting => {
                if reply.code != "0" || rows.iter().any(|r| !r.code.is_empty() && r.code != "0") {
                    error = Some(format!("BloFin setting rejected: {}", reply.msg));
                }
            }
        }
        if let Some(e) = error {
            self.trade_failed(cx, request, &e);
            return;
        }
        if request.joined {
            self.finish_join(cx, None);
            return;
        }
        if request.step.delay > 0.0 && !request.job.steps.is_empty() {
            self.trading.pause = Some((cx.set_timer(request.step.delay), request.job));
        } else {
            self.trade_step(cx, request.job);
        }
    }
    fn finish_join(&mut self, cx: &mut Cx<'_, Pending>, error: Option<String>) {
        let Some(joined) = &mut self.trading.joined else {
            return;
        };
        if error.is_some() {
            joined.error = error;
        }
        joined.remaining -= 1;
        if joined.remaining != 0 {
            return;
        }
        let joined = self.trading.joined.take().unwrap();
        if let Some(error) = joined.error {
            self.cleanup_job(cx, &joined.job);
            cx.done(joined.job.command, Err(error));
            self.trading.busy = false;
            self.refresh_account(cx);
            self.trade_next(cx);
        } else {
            self.trade_step(cx, joined.job);
        }
    }
    pub(crate) fn trading_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self
            .trading
            .pause
            .as_ref()
            .is_some_and(|(t, _)| *t == timer)
        {
            let (_, job) = self.trading.pause.take().unwrap();
            self.trade_step(cx, job);
            true
        } else {
            false
        }
    }
}
