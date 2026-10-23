use crate::account::{clean, first, is_algo, is_hedged, margin, plan_type};
use crate::wire::{Envelope, Identifier, Number, SymbolConfig, UtaSettings};
use crate::{BitgetFutures, Pending, paths};
use safe_cex_engine::Level;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::{Credential, Operation, Policy, RetryPolicy};
use safe_cex_engine::{
    Command, CommandId, CredentialSlot, Cx, ErrorKind, Method, Outcome, Response, TimerId,
    VenueError, num,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

#[derive(Default)]
pub struct Trading {
    acknowledged: std::collections::HashMap<String, (String, Planned)>,
    queue: VecDeque<Command>,
    active: Option<Job>,
    metadata_at: std::collections::HashMap<String, f64>,
    retry: Option<(TimerId, Request)>,
    detect_wait: Option<TimerId>,
}

struct Job {
    id: CommandId,
    intent: Command,
    original: Option<Order>,
    metadata: VecDeque<String>,
    waiting_positions: bool,
    positions_checked: bool,
    planned: bool,
    outstanding: bool,
    calls: VecDeque<Planned>,
    fallback: Option<VecDeque<Planned>>,
    rollback: Option<Planned>,
    error: Option<VenueError>,
    ids: Vec<String>,
    /// A write took effect, so a later failure cannot mean nothing happened.
    effective: bool,
}

pub struct Request {
    command: CommandId,
    call: Planned,
    stream_revision: u64,
    signature_retry: bool,
    read_retries: u8,
}

#[derive(Clone)]
struct Planned {
    path: &'static str,
    payload: Payload,
    effect: Effect,
}

#[derive(Clone)]
enum Effect {
    Metadata(String),
    ResolveAmend {
        id: String,
        price: f64,
        amount: f64,
        client: String,
    },
    Place {
        expected: usize,
    },
    Cancel {
        orders: Vec<Order>,
        idempotent_missing: bool,
    },
    Amend {
        id: String,
        price: f64,
        amount: f64,
        client: String,
    },
    Leverage {
        symbol: String,
        leverage: f64,
        side: Option<PositionSide>,
    },
    Margin {
        symbol: String,
        margin: MarginType,
    },
    Mode(bool),
}

#[derive(Clone, Serialize)]
#[serde(untagged)]
enum Leverage {
    Number(f64),
    Text(String),
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CancelId {
    order_id: String,
}

// Native request fields; variants are selected by the planned endpoint.
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Payload {
    #[serde(skip_serializing_if = "Option::is_none")]
    trailing_stop_params: Option<Vec<TrailingParams>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_callback_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_trigger_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    product_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    margin_coin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    coin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    margin_mode: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_oid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_client_oid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    qty: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trade_side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hold_side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pos_side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    force: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_in_force: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reduce_only: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    callback_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tpsl_mode: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    take_profit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_loss: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_trigger_by: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_trigger_by: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_order_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_order_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preset_stop_surplus_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preset_stop_loss_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    leverage: Option<Leverage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pos_mode: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hold_mode: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_list: Option<Vec<Payload>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_id_list: Option<Vec<CancelId>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "type")]
    kind: Option<&'static str>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrailingParams {
    activation_price: String,
    activation_type: &'static str,
    trail_type: &'static str,
    trail_variance: String,
    pre_order_type: &'static str,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ReplyData<'a> {
    #[serde(default, borrow)]
    order_id: Identifier<'a>,
    #[serde(default, borrow)]
    client_oid: Identifier<'a>,
    #[serde(default, borrow)]
    success_list: Vec<ReplyItem<'a>>,
    #[serde(default)]
    failure_list: Vec<Failure>,
    #[serde(default, alias = "state", alias = "orderStatus")]
    status: String,
    #[serde(default, alias = "filledQty", alias = "cumExecQty")]
    base_volume: Number,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplyItem<'a> {
    #[serde(borrow)]
    order_id: Identifier<'a>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Failure {
    #[serde(default)]
    error_msg: String,
}
#[derive(Deserialize)]
struct Reply<'a> {
    code: Number,
    #[serde(default)]
    msg: String,
    #[serde(borrow)]
    data: Option<ReplyData<'a>>,
}

impl BitgetFutures {
    pub(crate) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let observed: Vec<_> = self
            .trading
            .acknowledged
            .keys()
            .filter(|id| {
                cx.store
                    .orders()
                    .iter()
                    .any(|o| o.id == **id || o.parent_id.as_ref() == Some(*id))
                    || self.order_terminal(id)
            })
            .cloned()
            .collect();
        for id in observed {
            self.trading.acknowledged.remove(&id);
        }
        if command_id(&command).is_none() {
            return;
        }
        let validation = if !self.private_enabled() {
            Err("Bitget credentials are required".to_owned())
        } else {
            self.preflight(cx, &command)
        };
        if let Err(error) = validation {
            self.refuse_entry(cx, not_sent(error));
            return;
        }
        self.trading.queue.push_back(command);
        self.resume_trading(cx);
    }

    /// A write refused before I/O admits a terminal-only Mutation owner, so the
    /// engine caps it at NotSent. A rejected admission already failed the command.
    fn refuse_entry(&self, cx: &mut Cx<'_, Pending>, error: VenueError) {
        let credential = self
            .options
            .credentials
            .as_ref()
            .filter(|identity| identity.validate().is_ok())
            .map(|identity| Credential {
                slot: CredentialSlot::Primary,
                reference: identity.credential.clone(),
            });
        if let Ok(context) = cx.admit_http_command(Policy {
            operation: Operation::Mutation,
            credential,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }) {
            let _ = cx.fail_context(&context, error);
        }
    }

    fn preflight(&self, cx: &Cx<'_, Pending>, command: &Command) -> Result<(), String> {
        if let Command::PlaceOrders { orders, .. } = command {
            let mut ids = HashSet::new();
            for o in orders {
                if o.bybit_bbo.is_some() || o.bybit_slippage_tolerance_ticks.is_some() {
                    return Err("Bybit order controls are not supported by Bitget".into());
                }
                let market = cx
                    .store
                    .market(&o.symbol)
                    .ok_or_else(|| format!("Market {} not found", o.symbol))?;
                if o.kind == OrderType::Conditional
                    || (o.trigger_price.is_some() && !o.redundant_protective_trigger())
                {
                    return Err("Bitget opening conditional orders are unsupported".into());
                }
                if !o.amount.is_finite() || o.amount < 0.0 {
                    return Err("invalid Bitget order amount".into());
                }
                if o.kind != OrderType::Market
                    && !o.price.is_some_and(|v| {
                        v.is_finite()
                            && num::adjust(v, market.precision.price).is_finite()
                            && num::adjust(v, market.precision.price) > 0.0
                    })
                {
                    return Err("Bitget order requires a positive price".into());
                }
                for value in [o.stop_loss, o.take_profit].into_iter().flatten() {
                    if !value.is_finite() || value <= 0.0 {
                        return Err("invalid Bitget bracket price".into());
                    }
                }
                if let Some(id) = &o.client_order_id
                    && (id.is_empty()
                        || id.len() > 32
                        || !id
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b".:/_-".contains(&b))
                        || !ids.insert(id)
                        || is_algo(o.kind)
                        || num::adjust(o.amount, market.precision.amount) <= 0.0
                        || o.take_profit.is_some()
                        || o.stop_loss.is_some()
                        || (market.limits.amount.max > 0.0
                            && (o.amount > market.limits.amount.max
                                || num::adjust(o.amount, market.precision.amount)
                                    > floor_lot(
                                        market.limits.amount.max,
                                        market.precision.amount,
                                    ))))
                {
                    return Err(
                        "Bitget clientOrderId cannot be honored verbatim on one native order"
                            .into(),
                    );
                }
            }
        }
        Ok(())
    }

    pub(crate) fn resume_trading(&mut self, cx: &mut Cx<'_, Pending>) {
        if self
            .trading
            .active
            .as_ref()
            .is_some_and(|job| job.outstanding)
        {
            return;
        }
        let mut job = if let Some(job) = self.trading.active.take() {
            job
        } else if let Some(intent) = self.trading.queue.pop_front() {
            let metadata = symbols(cx, &intent)
                .into_iter()
                .filter(|symbol| {
                    cx.store.setting(symbol).is_none()
                        || (!cx
                            .store
                            .positions()
                            .iter()
                            .any(|p| &p.symbol == symbol && p.contracts > 0.0)
                            && !self
                                .trading
                                .metadata_at
                                .get(symbol)
                                .is_some_and(|at| cx.now - *at < 5_000.0))
                })
                .collect();
            Job {
                id: command_id(&intent).expect("trade command"),
                original: if let Command::AmendOrder { id, .. } = &intent {
                    cx.store.order(id).cloned()
                } else {
                    None
                },
                intent,
                metadata,
                waiting_positions: false,
                positions_checked: false,
                planned: false,
                outstanding: false,
                calls: VecDeque::new(),
                fallback: None,
                rollback: None,
                error: None,
                ids: vec![],
                effective: false,
            }
        } else {
            return;
        };
        if !self.account.detected {
            if self.trading.detect_wait.is_none() {
                self.trading.detect_wait = Some(cx.set_timer(10_000.0));
            }
            self.trading.active = Some(job);
            return;
        }
        if let Some(timer) = self.trading.detect_wait.take() {
            cx.clear_timer(timer);
        }
        if job.waiting_positions {
            if self.positions_in_flight() {
                self.trading.active = Some(job);
                return;
            }
            job.waiting_positions = false;
            job.positions_checked = true;
        }
        if let Some(symbol) = job.metadata.pop_front() {
            if cx.store.market(&symbol).is_none() {
                self.trade_fail(cx, &job, not_sent(format!("Market {symbol} not found")));
                self.resume_trading(cx);
                return;
            }
            let call = Planned {
                path: paths::ACCOUNT,
                payload: Payload {
                    symbol: Some(symbol.clone()),
                    ..Payload::default()
                },
                effect: Effect::Metadata(symbol),
            };
            self.issue_trade(cx, job, call, false, 0);
            return;
        }
        if !job.planned {
            if !job.positions_checked && needs_position_read(cx, &job.intent, self.account.uta) {
                self.refresh_positions(cx);
                if self.positions_in_flight() {
                    job.waiting_positions = true;
                    self.trading.active = Some(job);
                    return;
                }
                job.positions_checked = true;
            }
            if let Err(error) = self.plan_job(cx, &mut job) {
                self.trade_fail(cx, &job, not_sent(error));
                self.resume_trading(cx);
                return;
            }
            self.warn_stop_downgrade(cx, &job);
            job.planned = true;
        }
        if let Some(call) = job.calls.pop_front() {
            self.issue_trade(cx, job, call, false, 0);
        } else {
            match job.error.take() {
                Some(error) => self.trade_fail(cx, &job, error),
                None => cx.done(job.id, serde_json::to_string(&job.ids).expect("order ids")),
            }
            self.resume_trading(cx);
        }
    }

    fn issue_trade(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut job: Job,
        call: Planned,
        signature_retry: bool,
        read_retries: u8,
    ) {
        let read = matches!(
            call.effect,
            Effect::Metadata(_) | Effect::ResolveAmend { .. }
        );
        let method = if read { Method::Get } else { Method::Post };
        let path = match &call.effect {
            Effect::Metadata(symbol) => {
                if self.account.uta {
                    paths::SETTINGS.to_owned()
                } else {
                    let coin = match self.margin_coin(cx, symbol) {
                        Ok(coin) => coin,
                        Err(error) => {
                            self.trade_fail(cx, &job, not_sent(error));
                            self.resume_trading(cx);
                            return;
                        }
                    };
                    format!(
                        "{}?symbol={symbol}&productType={}&marginCoin={}",
                        paths::ACCOUNT,
                        self.product(),
                        coin
                    )
                }
            }
            Effect::ResolveAmend { client, .. } => format!(
                "{}?symbol={}&productType={}&clientOid={}",
                paths::ORDER_DETAIL,
                call.payload.symbol.as_deref().unwrap_or_default(),
                self.product(),
                crate::sign::encode(client)
            ),
            _ => call.path.to_owned(),
        };
        let body = (!read).then(|| serde_json::to_string(&call.payload).expect("Bitget payload"));
        if let Effect::Cancel { orders, .. } = &call.effect {
            for order in orders {
                cx.store.set_order_status(&order.id, OrderStatus::Canceling);
            }
        }
        job.outstanding = true;
        let command = job.id;
        self.trading.active = Some(job);
        cx.command_progress(command, call.identities());
        let pending = Pending::Trade(Box::new(Request {
            command,
            call,
            stream_revision: self.order_revision_clock(),
            signature_retry,
            read_retries,
        }));
        self.send(cx, method, &path, body, true, pending);
    }

    fn plan_job(&self, cx: &Cx<'_, Pending>, job: &mut Job) -> Result<(), String> {
        match &job.intent {
            Command::PlaceOrders { orders, .. } => {
                if self.trading.acknowledged.len() >= 4096 {
                    return Err(
                        "Bitget unobserved placement capacity exceeded; reconcile before placing"
                            .into(),
                    );
                }
                job.calls = self.place_calls(cx, orders)?.into();
            }
            Command::CancelOrders { ids, .. } => {
                let native = ids
                    .iter()
                    .map(|id| {
                        cx.store
                            .order(id)
                            .map(native_id)
                            .or_else(|| {
                                self.trading
                                    .acknowledged
                                    .contains_key(id)
                                    .then(|| id.clone())
                            })
                            .ok_or_else(|| {
                                format!(
                                    "Order {id} not found; cancellation requires a reconciled order"
                                )
                            })
                    })
                    .collect::<Result<HashSet<_>, String>>()?;
                let orders: Vec<_> = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| native.contains(&native_id(o)))
                    .cloned()
                    .collect();
                job.calls = self.cancel_calls(cx, &orders, true)?.into();
                for (_, call) in ids
                    .iter()
                    .filter_map(|id| self.trading.acknowledged.get(id))
                {
                    if call.path.is_empty() {
                        return Err(
                            "Bitget cancellation needs the acknowledged order's native identity"
                                .into(),
                        );
                    }
                    job.calls.push_back(call.clone());
                }
            }
            Command::CancelSymbolOrders { symbol, .. } => {
                let orders: Vec<_> = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.symbol == *symbol)
                    .cloned()
                    .collect();
                job.calls = self.cancel_calls(cx, &orders, true)?.into();
                for (_, call) in self
                    .trading
                    .acknowledged
                    .values()
                    .filter(|(s, _)| s == symbol)
                {
                    if call.path.is_empty() {
                        return Err(
                            "Bitget cancellation needs the acknowledged order's native identity"
                                .into(),
                        );
                    }
                    job.calls.push_back(call.clone());
                }
            }
            Command::AmendOrder {
                id, price, amount, ..
            } => {
                let (id, price, amount) = (id.clone(), *price, *amount);
                self.plan_amend(cx, job, &id, price, amount)?;
            }
            Command::SetLeverage {
                symbol,
                leverage,
                side,
                ..
            } => {
                let m = cx.store.market(symbol).ok_or("unknown market")?;
                if !leverage.is_finite() || *leverage <= 0.0 {
                    return Err("invalid leverage".into());
                }
                let min = leverage_limit(m.limits.leverage.min, 1.0);
                let max = leverage_limit(m.limits.leverage.max, 20.0);
                let leverage = num::clamp_leverage(*leverage, min, max)?;
                let isolated_hedge = cx.store.hedged()
                    && cx
                        .store
                        .setting(symbol)
                        .is_some_and(|s| s.margin_type == Some(MarginType::Isolated));
                let requested_side = side.unwrap_or(PositionSide::Long);
                let side = isolated_hedge.then_some(requested_side);
                let side_text = (isolated_hedge || (self.inverse() && cx.store.hedged()))
                    .then_some(requested_side)
                    .map(|side| match side {
                        PositionSide::Long => "long",
                        PositionSide::Short => "short",
                    });
                let mut p = self.classic(cx, symbol)?;
                p.leverage = Some(Leverage::Number(leverage));
                let path = if self.account.uta && isolated_hedge {
                    p = Payload {
                        category: Some(self.product().to_uppercase()),
                        symbol: Some(symbol.clone()),
                        coin: Some(self.coin().into()),
                        leverage: Some(Leverage::Text(num::format(leverage))),
                        pos_side: side_text,
                        ..Payload::default()
                    };
                    paths::UTA_SET_LEVERAGE
                } else {
                    p.hold_side = side_text;
                    paths::SET_LEVERAGE
                };
                job.calls.push_back(Planned {
                    path,
                    payload: p,
                    effect: Effect::Leverage {
                        symbol: symbol.clone(),
                        leverage,
                        side,
                    },
                });
            }
            Command::SetMarginType {
                symbol, isolated, ..
            } => {
                let margin = if *isolated {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                };
                let mut p = self.classic(cx, symbol)?;
                p.margin_mode = Some(margin_text(margin));
                let path = if self.account.uta {
                    p = Payload {
                        category: Some(self.product().to_uppercase()),
                        symbol: Some(symbol.clone()),
                        margin_mode: Some(margin_text(margin)),
                        leverage: Some(Leverage::Text(num::format(
                            cx.store.setting(symbol).map_or(1.0, |s| s.leverage),
                        ))),
                        ..Payload::default()
                    };
                    paths::UTA_SET_LEVERAGE
                } else {
                    paths::SET_MARGIN
                };
                job.calls.push_back(Planned {
                    path,
                    payload: p,
                    effect: Effect::Margin {
                        symbol: symbol.clone(),
                        margin,
                    },
                });
            }
            Command::SetPositionMode { hedged, .. } => {
                let mode = if *hedged {
                    "hedge_mode"
                } else {
                    "one_way_mode"
                };
                let payload = if self.account.uta {
                    Payload {
                        hold_mode: Some(mode),
                        ..Payload::default()
                    }
                } else {
                    Payload {
                        product_type: Some(self.product().into()),
                        pos_mode: Some(mode),
                        ..Payload::default()
                    }
                };
                job.calls.push_back(Planned {
                    path: if self.account.uta {
                        paths::UTA_SET_MODE
                    } else {
                        paths::SET_MODE
                    },
                    payload,
                    effect: Effect::Mode(*hedged),
                });
            }
            _ => return Err("unsupported Bitget trading command".into()),
        }
        Ok(())
    }

    fn warn_stop_downgrade(&self, cx: &mut Cx<'_, Pending>, job: &Job) {
        let orders = match &job.intent {
            Command::PlaceOrders { orders, .. } => orders.clone(),
            Command::AmendOrder { amount, .. } => job
                .original
                .as_ref()
                .map(|order| {
                    let mut opts = place_opts(order);
                    opts.amount = amount.unwrap_or(order.amount);
                    vec![opts]
                })
                .unwrap_or_default(),
            _ => vec![],
        };
        for order in orders {
            if order.amount <= 0.0
                || !matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit)
            {
                continue;
            }
            let no_sized_plan = self
                .plan_size(cx, &order)
                .is_ok_and(|(_, size)| size <= 0.0);
            let uta_fallback_size = cx
                .store
                .positions()
                .iter()
                .find(|position| {
                    position.symbol == order.symbol
                        && (!cx.store.hedged() || position.side == protected_position(order.side))
                        && position.contracts > 0.0
                })
                .map_or(0.0, |position| {
                    cx.store.market(&order.symbol).map_or(0.0, |market| {
                        num::adjust(position.contracts, market.precision.amount)
                    })
                });
            let downgraded = no_sized_plan
                && (!self.account.uta || uta_fallback_size <= 0.0)
                && job.calls.iter().any(|call| {
                    call.payload.symbol.as_deref() == Some(order.symbol.as_str())
                        && (matches!(
                            call.payload.plan_type.as_deref(),
                            Some("pos_loss" | "pos_profit")
                        ) || call.payload.tpsl_mode == Some("full"))
                });
            if downgraded {
                let positions = cx
                    .store
                    .positions()
                    .iter()
                    .filter(|position| position.symbol == order.symbol)
                    .map(|position| format!("{:?}:{}", position.side, position.contracts))
                    .collect::<Vec<_>>()
                    .join(", ");
                cx.notice(Level::Warn, code::COMMAND_UNSUPPORTED, format!(
                    "Sized {} stop became a full-position stop: no open {:?} position to size requested {} against, positions seen [{}]",
                    order.symbol, protected_position(order.side), order.amount,
                    if positions.is_empty() { "none" } else { &positions },
                ));
            }
        }
    }

    fn classic(&self, cx: &Cx<'_, Pending>, symbol: &str) -> Result<Payload, String> {
        Ok(Payload {
            symbol: Some(symbol.into()),
            product_type: Some(self.product().into()),
            margin_coin: Some(self.margin_coin(cx, symbol)?.into()),
            ..Payload::default()
        })
    }

    fn place_calls(
        &self,
        cx: &Cx<'_, Pending>,
        orders: &[PlaceOrderOpts],
    ) -> Result<Vec<Planned>, String> {
        let mut calls = vec![];
        let mut groups: Vec<(String, Vec<Payload>)> = vec![];
        for o in orders.iter().filter(|o| !is_algo(o.kind)) {
            let m = cx.store.market(&o.symbol).ok_or("unknown market")?;
            let total = num::adjust(o.amount, m.precision.amount);
            // TS drops an order that rounds below one lot. Explicit caller ids
            // were rejected in preflight because dropping cannot honor them.
            if total <= 0.0 {
                continue;
            }
            let max = if m.limits.amount.max > 0.0 && m.limits.amount.max.is_finite() {
                floor_lot(m.limits.amount.max, m.precision.amount)
            } else {
                total
            };
            if max <= 0.0 {
                return Err("Bitget maximum size is smaller than one lot".into());
            }
            let mut left = total;
            let count = (total / max).ceil() as usize;
            if o.client_order_id.is_some() && count > 1 {
                return Err("Bitget caller clientOrderId would split across native orders".into());
            }
            if count > 10_000 {
                return Err("Bitget order exceeds the split bound".into());
            }
            while left > 0.0 {
                let size = left.min(max);
                left = clean(left - size);
                let payload = self.normal_payload(cx, o, size)?;
                if self.account.uta {
                    calls.push(Planned {
                        path: paths::UTA_PLACE,
                        payload,
                        effect: Effect::Place { expected: 1 },
                    });
                } else if let Some((_, group)) =
                    groups.iter_mut().find(|(symbol, _)| symbol == &o.symbol)
                {
                    group.push(payload);
                } else {
                    groups.push((o.symbol.clone(), vec![payload]));
                }
            }
        }
        for (symbol, orders) in groups {
            for chunk in orders.chunks(50) {
                let mut payload = self.classic(cx, &symbol)?;
                payload.margin_mode = Some(margin_text(
                    cx.store
                        .setting(&symbol)
                        .and_then(|s| s.margin_type)
                        .unwrap_or(MarginType::Cross),
                ));
                payload.order_list = Some(chunk.to_vec());
                calls.push(Planned {
                    path: paths::BATCH_PLACE,
                    payload,
                    effect: Effect::Place {
                        expected: chunk.len(),
                    },
                });
            }
        }
        for o in orders.iter().filter(|o| is_algo(o.kind)) {
            let (path, payload) = if self.account.uta {
                (paths::UTA_STRATEGY, self.strategy_payload(cx, o)?)
            } else {
                (
                    if o.kind == OrderType::TrailingStopLoss {
                        paths::PLACE_TRAILING
                    } else {
                        paths::PLACE_TPSL
                    },
                    self.algo_payload(cx, o)?,
                )
            };
            calls.push(Planned {
                path,
                payload,
                effect: Effect::Place { expected: 1 },
            });
        }
        Ok(calls)
    }

    fn normal_payload(
        &self,
        cx: &Cx<'_, Pending>,
        o: &PlaceOrderOpts,
        amount: f64,
    ) -> Result<Payload, String> {
        let m = cx.store.market(&o.symbol).ok_or("unknown market")?;
        let reduce = o.reduce_only.unwrap_or(false);
        let hedge = cx.store.hedged();
        let mut p = Payload {
            client_oid: Some(o.client_order_id.clone().map_or_else(|| mint(cx), Ok)?),
            price: o
                .price
                .filter(|_| o.kind != OrderType::Market)
                .map(|v| number(v, m.precision.price)),
            side: Some(side_text(if hedge && reduce && !self.account.uta {
                o.side.opposite()
            } else {
                o.side
            })),
            order_type: Some(if o.kind == OrderType::Limit {
                "limit"
            } else {
                "market"
            }),
            ..Payload::default()
        };
        if self.account.uta {
            p.category = Some(self.product().to_uppercase());
            p.symbol = Some(o.symbol.clone());
            p.qty = Some(num::format(amount));
            p.pos_side = hedge.then_some(if reduce {
                protected_side(o.side)
            } else {
                opening_side(o.side)
            });
            p.margin_mode = Some(margin_text(
                cx.store
                    .setting(&o.symbol)
                    .and_then(|s| s.margin_type)
                    .unwrap_or(MarginType::Cross),
            ));
            p.reduce_only = (!hedge).then_some(if reduce { "yes" } else { "no" });
            p.time_in_force = (o.kind == OrderType::Limit).then_some(tif(o.time_in_force));
            p.take_profit = o.take_profit.map(|v| number(v, m.precision.price));
            p.stop_loss = o.stop_loss.map(|v| number(v, m.precision.price));
            p.tp_trigger_by = o.take_profit.map(|_| uta_trigger(o.tp_trigger_type));
            p.sl_trigger_by = o.stop_loss.map(|_| uta_trigger(o.sl_trigger_type));
            p.tp_order_type = o.take_profit.map(|_| "market");
            p.sl_order_type = o.stop_loss.map(|_| "market");
        } else {
            p.size = Some(num::format(amount));
            p.trade_side = hedge.then_some(if reduce { "close" } else { "open" });
            p.force = (o.kind == OrderType::Limit).then_some(tif(o.time_in_force));
            p.reduce_only = (!hedge).then_some(if reduce { "YES" } else { "NO" });
            p.preset_stop_surplus_price = o.take_profit.map(|v| number(v, m.precision.price));
            p.preset_stop_loss_price = o.stop_loss.map(|v| number(v, m.precision.price));
        }
        Ok(p)
    }

    fn plan_size(&self, cx: &Cx<'_, Pending>, o: &PlaceOrderOpts) -> Result<(String, f64), String> {
        let m = cx.store.market(&o.symbol).ok_or("unknown market")?;
        let is_tp = o.kind == OrderType::TakeProfit;
        let position = cx.store.positions().iter().find(|p| {
            p.symbol == o.symbol && p.side == protected_position(o.side) && p.contracts > 0.0
        });
        let capacity = position.map_or(0.0, |p| {
            floor_lot(
                if self.inverse() {
                    p.notional
                } else {
                    p.contracts
                },
                m.precision.amount,
            )
        });
        let requested = num::adjust(o.amount, m.precision.amount);
        let size = if o.amount == 0.0 || capacity < m.precision.amount {
            0.0
        } else {
            requested.min(capacity)
        };
        Ok((
            if size > 0.0 {
                if is_tp { "profit_plan" } else { "loss_plan" }
            } else if is_tp {
                "pos_profit"
            } else {
                "pos_loss"
            }
            .into(),
            size,
        ))
    }

    fn algo_payload(&self, cx: &Cx<'_, Pending>, o: &PlaceOrderOpts) -> Result<Payload, String> {
        let m = cx.store.market(&o.symbol).ok_or("unknown market")?;
        let (plan, size) = self.plan_size(cx, o)?;
        let mut p = self.classic(cx, &o.symbol)?;
        p.client_oid = Some(mint(cx)?);
        p.reduce_only = (!cx.store.hedged()).then_some("YES");
        p.trigger_price = o.price.map(|v| number(v, m.precision.price));
        p.trigger_type = Some(trigger(if o.kind == OrderType::TakeProfit {
            o.tp_trigger_type.or(o.trigger_type)
        } else {
            o.sl_trigger_type.or(o.trigger_type)
        }));
        if o.kind == OrderType::TrailingStopLoss {
            let (size, _, ratio) = self.trailing_values(cx, o, None)?;
            p.size = Some(num::format(size));
            p.plan_type = Some("track_plan".into());
            p.side = Some(side_text(if cx.store.hedged() {
                o.side.opposite()
            } else {
                o.side
            }));
            p.trade_side = cx.store.hedged().then_some("close");
            p.order_type = Some("market");
            p.margin_mode = Some(margin_text(
                cx.store
                    .setting(&o.symbol)
                    .and_then(|s| s.margin_type)
                    .unwrap_or(MarginType::Cross),
            ));
            p.callback_ratio = Some(num::format(ratio));
        } else {
            p.plan_type = Some(plan);
            p.size = (size > 0.0).then(|| num::format(size));
            p.hold_side = Some(if cx.store.hedged() {
                protected_side(o.side)
            } else if o.side == OrderSide::Sell {
                "buy"
            } else {
                "sell"
            });
        }
        Ok(p)
    }

    fn trailing_values(
        &self,
        cx: &Cx<'_, Pending>,
        o: &PlaceOrderOpts,
        retained: Option<(&str, f64)>,
    ) -> Result<(f64, f64, f64), String> {
        let m = cx.store.market(&o.symbol).ok_or("unknown market")?;
        let stop = o
            .price
            .filter(|p| p.is_finite() && *p > 0.0)
            .ok_or("Bitget trailing stop requires a positive trigger price")?;
        let size = num::adjust(o.amount, m.precision.amount);
        if !size.is_finite() || size <= 0.0 {
            return Err("Bitget trailing stop requires an explicit positive size".into());
        }
        if let Some((kind, variance)) = retained {
            if !variance.is_finite()
                || variance <= 0.0
                || !matches!(kind, "ratio" | "distance")
                || (!self.account.uta && (kind != "ratio" || variance > 10.0))
            {
                return Err(
                    "Bitget native trailing parameters are invalid; refresh before editing".into(),
                );
            }
            return Ok((size, variance, variance));
        }
        let ticker = cx
            .store
            .ticker(&o.symbol)
            .filter(|t| t.last.is_finite() && t.last > 0.0)
            .ok_or("Bitget trailing stop requires a positive ticker")?;
        let distance = num::adjust((ticker.last - stop).abs(), m.precision.price);
        if !distance.is_finite() || distance <= 0.0 {
            return Err("Bitget trailing distance must be at least one price tick".into());
        }
        let ratio = num::adjust(distance * 100.0 / ticker.last, 0.1);
        if !self.account.uta && (!ratio.is_finite() || ratio <= 0.0 || ratio > 10.0) {
            return Err(
                "Bitget trailing callback percentage must be greater than zero and at most 10"
                    .into(),
            );
        }
        Ok((size, distance, ratio))
    }

    fn strategy_payload(
        &self,
        cx: &Cx<'_, Pending>,
        o: &PlaceOrderOpts,
    ) -> Result<Payload, String> {
        self.strategy_payload_with_trailing(cx, o, None)
    }

    fn strategy_payload_with_trailing(
        &self,
        cx: &Cx<'_, Pending>,
        o: &PlaceOrderOpts,
        retained: Option<(&str, f64)>,
    ) -> Result<Payload, String> {
        if o.kind == OrderType::TrailingStopLoss {
            let (size, distance, _) = self.trailing_values(cx, o, retained)?;
            let m = cx.store.market(&o.symbol).ok_or("unknown market")?;
            return Ok(Payload {
                category: Some(self.product().to_uppercase()),
                symbol: Some(o.symbol.clone()),
                kind: Some("trailing_stop"),
                side: Some(side_text(o.side)),
                pos_side: cx.store.hedged().then_some(protected_side(o.side)),
                qty: Some(num::format(size)),
                reduce_only: (!cx.store.hedged()).then_some("yes"),
                margin_mode: Some(margin_text(
                    cx.store
                        .setting(&o.symbol)
                        .and_then(|s| s.margin_type)
                        .unwrap_or(MarginType::Cross),
                )),
                // UTA only accepts clientOid for tpsl. Distance avoids an
                // undocumented ratio unit conversion and keeps chart prices exact.
                trailing_stop_params: Some(vec![TrailingParams {
                    activation_price: number(
                        o.price.ok_or("trailing price is required")?,
                        m.precision.price,
                    ),
                    activation_type: uta_trigger(o.sl_trigger_type.or(o.trigger_type)),
                    trail_type: if retained.is_some_and(|(kind, _)| kind == "ratio") {
                        "ratio"
                    } else {
                        "distance"
                    },
                    trail_variance: num::format(distance),
                    pre_order_type: "market",
                }]),
                ..Payload::default()
            });
        }
        if !matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit) {
            return Err("unsupported Bitget UTA strategy type".into());
        }
        let m = cx.store.market(&o.symbol).ok_or("unknown market")?;
        let (_, mut size) = self.plan_size(cx, o)?;
        if size == 0.0 {
            size = cx
                .store
                .positions()
                .iter()
                .find(|p| {
                    p.symbol == o.symbol
                        && (!cx.store.hedged() || p.side == protected_position(o.side))
                        && p.contracts > 0.0
                })
                .map_or(0.0, |p| num::adjust(p.contracts, m.precision.amount));
        }
        let mut p = Payload {
            category: Some(self.product().to_uppercase()),
            symbol: Some(o.symbol.clone()),
            kind: Some("tpsl"),
            side: Some(side_text(o.side)),
            pos_side: cx.store.hedged().then_some(protected_side(o.side)),
            tpsl_mode: Some(if size > 0.0 { "partial" } else { "full" }),
            qty: (size > 0.0).then(|| num::format(size)),
            reduce_only: Some("yes"),
            client_oid: Some(mint(cx)?),
            ..Payload::default()
        };
        if o.kind == OrderType::TakeProfit {
            p.take_profit = o.price.map(|v| number(v, m.precision.price));
            p.tp_trigger_by = Some(uta_trigger(o.tp_trigger_type.or(o.trigger_type)));
            p.tp_order_type = Some("market");
        } else {
            p.stop_loss = o.price.map(|v| number(v, m.precision.price));
            p.sl_trigger_by = Some(uta_trigger(o.sl_trigger_type.or(o.trigger_type)));
            p.sl_order_type = Some("market");
        }
        Ok(p)
    }

    fn cancel_calls(
        &self,
        cx: &Cx<'_, Pending>,
        orders: &[Order],
        idempotent_missing: bool,
    ) -> Result<Vec<Planned>, String> {
        let mut calls = vec![];
        let mut normal: Vec<(String, Vec<Order>)> = vec![];
        let mut seen = HashSet::new();
        for order in orders {
            let id = native_id(order);
            let strategy = is_algo(order.kind)
                && (order.parent_id.is_none()
                    || self
                        .account
                        .plans
                        .get(&order.id)
                        .is_some_and(|p| p == "uta_tpsl"));
            if !seen.insert((strategy, id.clone())) {
                continue;
            }
            if self.account.uta {
                let payload = Payload {
                    category: (!strategy).then(|| self.product().to_uppercase()),
                    order_id: (!order
                        .client_order_id
                        .as_ref()
                        .is_some_and(|client| client == &id))
                    .then_some(id),
                    client_oid: order.client_order_id.clone(),
                    ..Payload::default()
                };
                calls.push(Planned {
                    path: if strategy {
                        paths::UTA_CANCEL_STRATEGY
                    } else {
                        paths::UTA_CANCEL
                    },
                    payload,
                    effect: Effect::Cancel {
                        orders: related(orders, order),
                        idempotent_missing: strategy && idempotent_missing,
                    },
                });
            } else if strategy {
                let mut payload = self.classic(cx, &order.symbol)?;
                if order.kind == OrderType::TrailingStopLoss {
                    payload.order_id_list = Some(vec![CancelId { order_id: id }]);
                } else {
                    payload.order_id = Some(id);
                }
                payload.plan_type = Some(
                    self.account
                        .plans
                        .get(&order.id)
                        .and_then(|v| plan_type(v))
                        .unwrap_or_else(|| fallback_plan(order))
                        .into(),
                );
                calls.push(Planned {
                    path: paths::CANCEL_PLAN,
                    payload,
                    effect: Effect::Cancel {
                        orders: vec![order.clone()],
                        idempotent_missing: false,
                    },
                });
            } else if let Some((_, rows)) = normal.iter_mut().find(|(s, _)| s == &order.symbol) {
                rows.push(order.clone());
            } else {
                normal.push((order.symbol.clone(), vec![order.clone()]));
            }
        }
        let mut normal_calls = vec![];
        for (symbol, rows) in normal {
            for chunk in rows.chunks(50) {
                let mut p = self.classic(cx, &symbol)?;
                p.order_id_list = Some(
                    chunk
                        .iter()
                        .map(|o| CancelId {
                            order_id: native_id(o),
                        })
                        .collect(),
                );
                normal_calls.push(Planned {
                    path: paths::BATCH_CANCEL,
                    payload: p,
                    effect: Effect::Cancel {
                        orders: chunk.iter().flat_map(|o| related(orders, o)).collect(),
                        idempotent_missing: false,
                    },
                });
            }
        }
        normal_calls.extend(calls);
        Ok(normal_calls)
    }

    fn plan_amend(
        &self,
        cx: &Cx<'_, Pending>,
        job: &mut Job,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<(), String> {
        let order = cx
            .store
            .order(id)
            .cloned()
            .ok_or("Bitget order not found")?;
        if matches!(
            order.status,
            OrderStatus::Closed | OrderStatus::Canceled | OrderStatus::Canceling
        ) {
            return Err("Bitget order is terminal or canceling; refresh before amending".into());
        }
        let m = cx.store.market(&order.symbol).ok_or("unknown market")?;
        let next_price = num::adjust(price.unwrap_or(order.price), m.precision.price);
        let next_amount = num::adjust(amount.unwrap_or(order.amount), m.precision.amount);
        if !next_price.is_finite()
            || next_price <= 0.0
            || !next_amount.is_finite()
            || next_amount < 0.0
        {
            return Err("invalid Bitget amendment".into());
        }
        let linked: Vec<_> = cx
            .store
            .orders()
            .iter()
            .filter(|o| native_id(o) == native_id(&order))
            .cloned()
            .collect();
        let standalone = is_algo(order.kind)
            && (order.parent_id.is_none()
                || self
                    .account
                    .plans
                    .get(&order.id)
                    .is_some_and(|p| p == "uta_tpsl"));
        if standalone && !self.account.uta && order.kind == OrderType::TrailingStopLoss {
            let mut opts = place_opts(&order);
            opts.price = Some(next_price);
            opts.amount = next_amount;
            let retained = if price.is_none() {
                Some(self.account.trailing.get(&order.id)
                    .map(|(kind, variance)| (kind.as_str(), *variance))
                    .ok_or("Bitget native trailing parameters are unavailable; refresh before editing")?)
            } else {
                None
            };
            let (_, _, ratio) = self.trailing_values(cx, &opts, retained)?;
            job.calls.push_back(Planned {
                path: "/api/v2/mix/order/modify-plan-order",
                payload: Payload {
                    product_type: Some(self.product().into()),
                    order_id: Some(native_id(&order)),
                    new_size: amount.map(|_| num::format(next_amount)),
                    new_trigger_price: price.map(|_| num::format(next_price)),
                    new_callback_ratio: Some(num::format(ratio)),
                    ..Payload::default()
                },
                effect: Effect::Place { expected: 1 },
            });
            return Ok(());
        }
        if standalone && !self.account.uta {
            let mut p = self.classic(cx, &order.symbol)?;
            p.order_id = Some(native_id(&order));
            p.trigger_price = price.map(|_| num::format(next_price));
            p.size = Some(if amount.is_some_and(|a| a > 0.0) {
                num::format(next_amount)
            } else if order.amount > 0.0 {
                number(order.amount, m.precision.amount)
            } else {
                String::new()
            });
            job.calls.push_back(Planned {
                path: paths::MODIFY_TPSL,
                payload: p,
                effect: Effect::Place { expected: 1 },
            });
            return Ok(());
        }
        let mut opts = place_opts(&order);
        opts.client_order_id = None;
        let attached = linked.iter().find(|o| o.kind == OrderType::Limit);
        if let Some(parent) = attached {
            opts = place_opts(parent);
            opts.client_order_id = None;
            opts.stop_loss = linked
                .iter()
                .find(|o| o.kind == OrderType::StopLoss)
                .map(|o| if o.id == id { next_price } else { o.price });
            opts.take_profit = linked
                .iter()
                .find(|o| o.kind == OrderType::TakeProfit)
                .map(|o| if o.id == id { next_price } else { o.price });
            opts.price = Some(if parent.id == id {
                next_price
            } else {
                parent.price
            });
            opts.amount = amount.unwrap_or(parent.amount);
        } else {
            opts.price = Some(next_price);
            opts.amount = next_amount;
        }
        let mut replacement = if standalone && self.account.uta {
            vec![]
        } else {
            self.place_calls(cx, &[opts.clone()])?
        };
        if standalone && self.account.uta {
            let retained = if order.kind == OrderType::TrailingStopLoss {
                Some(self.account.trailing.get(&order.id)
                    .map(|(kind, variance)| (kind.as_str(), *variance))
                    .ok_or("Bitget native trailing parameters are unavailable; refresh before editing")?)
            } else {
                None
            };
            let mut original =
                self.strategy_payload_with_trailing(cx, &place_opts(&order), retained)?;
            let mut next = self.strategy_payload_with_trailing(
                cx,
                &opts,
                if price.is_none() { retained } else { None },
            )?;
            if linked.len() > 1 {
                for related in &linked {
                    let original_price = number(related.price, m.precision.price);
                    let changed = if related.id == id {
                        number(next_price, m.precision.price)
                    } else {
                        original_price.clone()
                    };
                    if related.kind == OrderType::StopLoss {
                        original.stop_loss = Some(original_price);
                        next.stop_loss = Some(changed);
                        original.sl_order_type = Some("market");
                        next.sl_order_type = Some("market");
                        original.sl_trigger_by = Some(uta_trigger(related.trigger_type));
                        next.sl_trigger_by = Some(uta_trigger(related.trigger_type));
                    } else if related.kind == OrderType::TakeProfit {
                        original.take_profit = Some(original_price);
                        next.take_profit = Some(changed);
                        original.tp_order_type = Some("market");
                        next.tp_order_type = Some("market");
                        original.tp_trigger_by = Some(uta_trigger(related.trigger_type));
                        next.tp_trigger_by = Some(uta_trigger(related.trigger_type));
                    }
                }
            }
            replacement = vec![Planned {
                path: paths::UTA_STRATEGY,
                payload: next,
                effect: Effect::Place { expected: 1 },
            }];
            job.rollback = Some(Planned {
                path: paths::UTA_STRATEGY,
                payload: original,
                effect: Effect::Place { expected: 1 },
            });
        }
        let mut fallback: VecDeque<_> = self.cancel_calls(cx, &linked, false)?.into();
        fallback.extend(replacement);
        if !standalone
            && order.kind == OrderType::Limit
            && attached.is_some_and(|_| linked.len() == 1)
            && !self.account.uta
        {
            if next_amount <= 0.0 {
                return Err("Bitget amended size must be positive".into());
            }
            let client = mint(cx)?;
            let payload = Payload {
                symbol: Some(order.symbol.clone()),
                product_type: Some(self.product().into()),
                order_id: Some(order.id.clone()),
                new_client_oid: Some(client.clone()),
                new_price: Some(num::format(next_price)),
                new_size: Some(num::format(next_amount)),
                ..Payload::default()
            };
            job.calls.push_back(Planned {
                path: paths::MODIFY_ORDER,
                payload,
                effect: Effect::Amend {
                    id: order.id,
                    price: next_price,
                    amount: next_amount,
                    client,
                },
            });
            job.fallback = Some(fallback);
        } else {
            job.calls = fallback;
        }
        Ok(())
    }

    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Result<Response<'_>, &str>,
    ) {
        let Some(mut job) = self.trading.active.take() else {
            return;
        };
        if job.id != request.command {
            self.trading.active = Some(job);
            return;
        }
        job.outstanding = false;
        if response
            .as_ref()
            .ok()
            .and_then(|r| serde_json::from_str::<Envelope<serde::de::IgnoredAny>>(r.body).ok())
            .is_some_and(|r| r.code.0 == Some(40085.0))
            && self.promote_uta(cx)
        {
            if let Effect::Cancel { orders, .. } = &request.call.effect {
                for o in orders {
                    cx.store.update_order(&o.id, |row| {
                        if row.status == OrderStatus::Canceling {
                            row.status = o.status;
                        }
                    });
                }
            }
            if job.ids.is_empty() {
                job.calls.clear();
                job.planned = false;
                job.positions_checked = false;
                job.waiting_positions = false;
                job.metadata = symbols(cx, &job.intent).into();
                job.fallback = None;
                job.rollback = None;
                self.trading.metadata_at.clear();
                self.trading.active = Some(job);
                self.resume_trading(cx);
            } else {
                let error = VenueError::new(format!(
                    "Bitget switched to UTA after partial success; acknowledged IDs {:?}",
                    job.ids
                ));
                self.trade_fail(cx, &job, error);
            }
            return;
        }
        if matches!(
            request.call.effect,
            Effect::Metadata(_) | Effect::ResolveAmend { .. }
        ) && response.is_err()
            && request.read_retries < 3
        {
            self.retry_read(cx, job, request);
            return;
        }
        if let Effect::Metadata(symbol) = &request.call.effect {
            if self.account.uta {
                let parsed = response.map_err(str::to_owned).and_then(|r| {
                    if r.status != 200 {
                        return Err(format!("Bitget metadata HTTP {}", r.status));
                    }
                    serde_json::from_str::<Envelope<UtaSettings>>(r.body)
                        .map_err(|e| e.to_string())
                        .and_then(Envelope::checked)
                });
                match parsed {
                    Ok(settings) if !settings.hold_mode.is_empty() => {
                        self.apply_settings(cx, &settings);
                        if cx.store.setting(symbol).is_none() {
                            cx.store.update_setting(symbol, |s| {
                                s.leverage = 1.0;
                                s.margin_type = Some(MarginType::Cross);
                            });
                        }
                        self.trading.metadata_at.insert(symbol.clone(), cx.now);
                        self.trading.active = Some(job);
                        self.resume_trading(cx);
                    }
                    _ => {
                        let error = not_sent("Bitget UTA metadata could not be resolved");
                        self.trade_fail(cx, &job, error);
                        self.resume_trading(cx);
                    }
                }
                return;
            }
            let parsed = response.map_err(str::to_owned).and_then(|r| {
                if r.status != 200 {
                    return Err(format!("Bitget metadata HTTP {}", r.status));
                }
                serde_json::from_str::<Envelope<SymbolConfig>>(r.body)
                    .map_err(|e| e.to_string())
                    .and_then(Envelope::checked)
            });
            match parsed {
                Ok(row) if !row.pos_mode.is_empty() && !row.margin_mode.is_empty() => {
                    cx.store.set_hedged(is_hedged(&row.pos_mode));
                    let kind = margin(&row.margin_mode);
                    let leverage = if kind == MarginType::Cross {
                        row.crossed_margin_leverage.or(1.0)
                    } else {
                        first(&[row.isolated_long_lever, row.isolated_short_lever]).unwrap_or(1.0)
                    };
                    cx.store.update_setting(symbol, |s| {
                        s.leverage = leverage;
                        s.margin_type = Some(kind);
                        if kind == MarginType::Isolated && is_hedged(&row.pos_mode) {
                            if let Some(long) = row.isolated_long_lever.0 {
                                s.buy_leverage = Some(long);
                            }
                            if let Some(short) = row.isolated_short_lever.0 {
                                s.sell_leverage = Some(short);
                            }
                            // Legacy scalar settings represent the long rail.
                            s.leverage = s.buy_leverage.unwrap_or(leverage);
                        } else {
                            s.buy_leverage = Some(leverage);
                            s.sell_leverage = Some(leverage);
                        }
                    });
                    self.trading.metadata_at.insert(symbol.clone(), cx.now);
                    self.trading.active = Some(job);
                    self.resume_trading(cx);
                }
                _ => {
                    let error =
                        not_sent("Bitget position mode or margin type could not be resolved");
                    self.trade_fail(cx, &job, error);
                    self.resume_trading(cx);
                }
            }
            return;
        }
        let status = response.as_ref().ok().map(|r| r.status);
        let parsed = response.map_err(str::to_owned).and_then(|r| {
            serde_json::from_str::<Reply>(r.body)
                .map(|reply| (r.status, reply))
                .map_err(|e| e.to_string())
        });
        if let Effect::ResolveAmend { id, .. } = &request.call.effect {
            let ready = parsed.as_ref().is_ok_and(|(status, reply)| {
                *status == 200
                    && reply.code.0 == Some(0.0)
                    && reply.data.as_ref().is_some_and(|d| {
                        !d.order_id.text().is_empty() && d.order_id.text().as_ref() != id
                    })
            });
            if !ready && request.read_retries < 3 {
                self.retry_read(cx, job, request);
                return;
            }
        }
        let explicit_rejection = parsed.as_ref().is_ok_and(|(status, reply)| {
            *status < 500
                && *status != 408
                && !reply
                    .code
                    .0
                    .is_some_and(|code| [40010.0, 40725.0, 45001.0].contains(&code))
                && (*status >= 400 || reply.code.0.is_some_and(|code| code != 0.0))
        });
        if parsed
            .as_ref()
            .is_ok_and(|(_, reply)| reply.code.0 == Some(40009.0))
            && !request.signature_retry
        {
            self.issue_trade(cx, job, request.call, true, request.read_retries);
            return;
        }
        let mut error = match &parsed {
            Ok((200, reply)) if reply.code.0 == Some(0.0) => None,
            Ok((status, reply)) => Some(format!(
                "Bitget {}: {} (HTTP {status})",
                reply.code.get(),
                reply.msg
            )),
            Err(error) => Some(error.clone()),
        };
        if let Effect::Cancel { idempotent_missing: true, .. } = &request.call.effect
            && parsed.as_ref().is_ok_and(|(_, reply)| reply.msg == "Failed to stop the strategy because the corresponding strategy ID could not be found") { error = None; }
        if error.is_some()
            && matches!(request.call.effect, Effect::Amend { .. })
            && explicit_rejection
            && let Some(fallback) = job.fallback.take()
        {
            job.calls = fallback;
            self.trading.active = Some(job);
            self.resume_trading(cx);
            return;
        }
        let mut write_noted = false;
        // A 2xx success answer that does not account for every order.
        let mut unproven = false;
        if error.is_none()
            && let Effect::Cancel { orders, .. } = &request.call.effect
        {
            let data = parsed.as_ref().ok().and_then(|(_, r)| r.data.as_ref());
            let retained = orders
                .iter()
                .any(|o| self.trading.acknowledged.contains_key(&native_id(o)));
            let mut accepted: HashSet<_> = data.map_or_else(HashSet::new, |d| {
                d.success_list
                    .iter()
                    .map(|i| i.order_id.text().into_owned())
                    .collect()
            });
            if let Some(data) = data {
                let native = data.order_id.text();
                let client = data.client_oid.text();
                for order in orders {
                    if !native.is_empty() && native == native_id(order)
                        || !client.is_empty()
                            && request.call.payload.client_oid.as_deref() == Some(client.as_ref())
                    {
                        accepted.insert(native_id(order));
                    }
                }
            }
            for order in orders {
                if accepted.contains(&native_id(order)) {
                    self.trading.acknowledged.remove(&native_id(order));
                    self.mark_canceled(cx, &order.id);
                    if !write_noted {
                        self.note_write(cx);
                        write_noted = true;
                    }
                }
            }
            if data.is_some_and(|d| !d.failure_list.is_empty()) {
                error = Some(format!(
                    "Bitget cancel failure: {}",
                    data.expect("data")
                        .failure_list
                        .iter()
                        .map(|e| e.error_msg.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
            } else if (request.call.path == paths::BATCH_CANCEL || retained)
                && orders.iter().any(|o| !accepted.contains(&native_id(o)))
            {
                unproven = true;
                error = Some("Bitget batch cancel did not acknowledge every order".into());
            }
        }
        if error.is_none() {
            let data = parsed.as_ref().ok().and_then(|(_, r)| r.data.as_ref());
            if let Effect::Place { expected } = request.call.effect {
                let mut ids: Vec<_> = data.map_or_else(Vec::new, |d| {
                    d.success_list
                        .iter()
                        .map(|i| i.order_id.text().into_owned())
                        .filter(|s| !s.is_empty())
                        .collect()
                });
                if ids.is_empty()
                    && let Some(d) = data
                {
                    let native = d.order_id.text();
                    let client = d.client_oid.text();
                    let id = if !native.is_empty() { native } else { client };
                    if !id.is_empty() {
                        ids.push(id.into_owned());
                    }
                }
                if let Some(d) = data
                    && !d.failure_list.is_empty()
                {
                    error = Some(format!(
                        "Bitget batch failure after ids {ids:?}: {}",
                        d.failure_list
                            .iter()
                            .map(|e| e.error_msg.as_str())
                            .collect::<Vec<_>>()
                            .join("; ")
                    ));
                } else if ids.len() != expected {
                    unproven = true;
                    error = Some(format!(
                        "Bitget acknowledged {} of {expected} orders; ids {ids:?}",
                        ids.len()
                    ));
                }
                if !ids.is_empty() {
                    let p = &request.call.payload;
                    let symbol = p.symbol.clone().unwrap_or_default();
                    for id in &ids {
                        if cx
                            .store
                            .orders()
                            .iter()
                            .any(|o| o.id == *id || o.parent_id.as_ref() == Some(id))
                            || self.order_terminal(id)
                        {
                            continue;
                        }
                        let strategy = matches!(
                            request.call.path,
                            paths::PLACE_TPSL | paths::PLACE_TRAILING | paths::UTA_STRATEGY
                        );
                        let mut payload = if self.account.uta {
                            Payload {
                                category: (!strategy).then(|| self.product().to_uppercase()),
                                order_id: Some(id.clone()),
                                ..Payload::default()
                            }
                        } else {
                            let mut payload = Payload {
                                symbol: Some(symbol.clone()),
                                product_type: p.product_type.clone(),
                                margin_coin: p.margin_coin.clone(),
                                ..Payload::default()
                            };
                            if strategy {
                                payload.plan_type = p.plan_type.clone();
                                if request.call.path == paths::PLACE_TRAILING {
                                    payload.order_id_list = Some(vec![CancelId {
                                        order_id: id.clone(),
                                    }]);
                                } else {
                                    payload.order_id = Some(id.clone());
                                }
                            } else {
                                payload.order_id_list = Some(vec![CancelId {
                                    order_id: id.clone(),
                                }]);
                            }
                            payload
                        };
                        if p.client_oid.as_deref() == Some(id)
                            && data.is_some_and(|d| d.order_id.text().is_empty())
                        {
                            // Native identity was not returned: only UTA accepts a client-id cancel here.
                            if self.account.uta {
                                payload.order_id = None;
                                payload.client_oid = Some(id.clone());
                            }
                        }
                        let kind = if strategy {
                            if request.call.path == paths::PLACE_TRAILING {
                                OrderType::TrailingStopLoss
                            } else if p.plan_type.as_deref().is_some_and(|t| t.contains("profit")) {
                                OrderType::TakeProfit
                            } else {
                                OrderType::StopLoss
                            }
                        } else {
                            OrderType::Limit
                        };
                        let order = Order {
                            id: id.clone(),
                            symbol: symbol.clone(),
                            kind,
                            parent_id: None,
                            status: OrderStatus::Open,
                            side: OrderSide::Buy,
                            price: 0.0,
                            amount: 0.0,
                            filled: 0.0,
                            remaining: 0.0,
                            reduce_only: false,
                            close: None,
                            client_order_id: None,
                            time_in_force: None,
                            last_update: cx.now,
                            trigger_type: None,
                            take_profit: None,
                            stop_loss: None,
                            extra: None,
                        };
                        let unresolved = !self.account.uta
                            && p.client_oid.as_deref() == Some(id)
                            && data.is_some_and(|d| d.order_id.text().is_empty());
                        self.trading.acknowledged.insert(
                            id.clone(),
                            (
                                symbol.clone(),
                                Planned {
                                    path: if unresolved {
                                        ""
                                    } else if self.account.uta {
                                        if strategy {
                                            paths::UTA_CANCEL_STRATEGY
                                        } else {
                                            paths::UTA_CANCEL
                                        }
                                    } else if strategy {
                                        paths::CANCEL_PLAN
                                    } else {
                                        paths::BATCH_CANCEL
                                    },
                                    payload,
                                    effect: Effect::Cancel {
                                        orders: vec![order],
                                        idempotent_missing: true,
                                    },
                                },
                            ),
                        );
                    }
                    self.note_write(cx);
                    write_noted = true;
                }
                if error.is_none() {
                    let p = &request.call.payload;
                    let trailing = p
                        .callback_ratio
                        .as_ref()
                        .or(p.new_callback_ratio.as_ref())
                        .and_then(|v| v.parse().ok())
                        .map(|v| ("ratio".to_owned(), v))
                        .or_else(|| {
                            p.trailing_stop_params
                                .as_ref()
                                .and_then(|rows| rows.first())
                                .and_then(|v| {
                                    v.trail_variance
                                        .parse()
                                        .ok()
                                        .map(|variance| (v.trail_type.to_owned(), variance))
                                })
                        });
                    if let Some(trailing) = trailing {
                        for id in &ids {
                            if self.order_revision(id) <= request.stream_revision {
                                self.account.trailing.insert(id.clone(), trailing.clone());
                            }
                        }
                    }
                }
                job.ids.extend(ids);
            }
        }
        if let Some(error) = error {
            let error = match (&parsed, status) {
                (_, None) => VenueError::new(error)
                    .kind(ErrorKind::Network)
                    .outcome(Outcome::Unknown),
                _ if unproven => VenueError::new(error).outcome(Outcome::Unknown),
                (Ok((status, reply)), _) if explicit_rejection => {
                    let error = VenueError::new(error).kind(match status {
                        401 => ErrorKind::AuthInvalid,
                        403 => ErrorKind::AuthPermission,
                        429 => ErrorKind::RateLimited,
                        _ => ErrorKind::Rejected,
                    });
                    let error = match reply.code.0 {
                        Some(code) => error.venue_code(code.to_string()),
                        None => error,
                    };
                    error.outcome(Outcome::Rejected)
                }
                // Explicit per-order refusals in a 2xx answer, without a documented code table.
                (Ok((200, reply)), _) if reply.code.0 == Some(0.0) => VenueError::new(error),
                // 408, 5xx, malformed answers and Bitget's ambiguous codes.
                _ => VenueError::new(error).outcome(Outcome::Unknown),
            };
            if matches!(request.call.effect, Effect::Place { .. }) {
                self.account.orders_version = self.account.orders_version.wrapping_add(1);
            }
            if let Effect::Cancel { orders, .. } = &request.call.effect {
                for order in orders {
                    cx.store.update_order(&order.id, |current| {
                        if current.status == OrderStatus::Canceling {
                            current.status = order.status;
                        }
                    });
                }
            }
            if explicit_rejection
                && matches!(request.call.effect, Effect::Place { .. })
                && job.error.is_none()
                && let Some(rollback) = job.rollback.take()
            {
                job.error = Some(error);
                job.calls.clear();
                job.calls.push_back(rollback);
                self.trading.active = Some(job);
                self.resume_trading(cx);
                return;
            }
            let error = match job.error.take() {
                None => error,
                Some(original) => VenueError {
                    message: format!(
                        "{}; protection rollback failed: {}",
                        original.message, error.message
                    ),
                    // A lost or unproven rollback makes the result Unknown, with its kind.
                    kind: if error.outcome == Some(Outcome::Unknown) {
                        error.kind
                    } else {
                        original.kind
                    },
                    outcome: error.outcome.filter(|o| *o == Outcome::Unknown),
                    ..original
                },
            };
            self.trade_fail(cx, &job, error);
            self.refresh_orders(cx, false);
            self.resume_trading(cx);
            return;
        }
        match request.call.effect {
            Effect::Cancel { orders, .. } => {
                for order in orders {
                    self.mark_canceled(cx, &order.id);
                }
            }
            Effect::Amend {
                id,
                price,
                amount,
                client,
            } => {
                let next = parsed
                    .as_ref()
                    .ok()
                    .and_then(|(_, reply)| reply.data.as_ref())
                    .map(|d| d.order_id.text().into_owned())
                    .filter(|s| !s.is_empty() && s != &id);
                if let Some(next) = next {
                    self.commit_amend(
                        cx,
                        &id,
                        &next,
                        (price, amount, job.original.clone(), request.stream_revision),
                        &client,
                    );
                    job.ids.push(next);
                } else {
                    let symbol = request.call.payload.symbol.clone();
                    job.calls.push_front(Planned {
                        path: paths::ORDER_DETAIL,
                        payload: Payload {
                            symbol,
                            ..Payload::default()
                        },
                        effect: Effect::ResolveAmend {
                            id,
                            price,
                            amount,
                            client,
                        },
                    });
                }
            }
            Effect::ResolveAmend {
                id,
                price,
                amount,
                client,
            } => {
                let next = parsed
                    .as_ref()
                    .ok()
                    .and_then(|(_, reply)| reply.data.as_ref())
                    .map(|d| d.order_id.text().into_owned())
                    .filter(|s| !s.is_empty() && s != &id);
                if let Some(next) = next {
                    let newer_stream = self.order_revision(&next) > request.stream_revision;
                    self.commit_amend(
                        cx,
                        &id,
                        &next,
                        (price, amount, job.original.clone(), request.stream_revision),
                        &client,
                    );
                    if !self.order_terminal(&next)
                        && !newer_stream
                        && let Some(data) = parsed.as_ref().ok().and_then(|(_, r)| r.data.as_ref())
                    {
                        let inverse = self.inverse();
                        cx.store.update_order(&next, |o| {
                            if !matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled) {
                                if let Some(filled) = data.base_volume.0 {
                                    o.filled = filled;
                                    amend_remaining(o, inverse);
                                }
                                if let Some(status) = crate::account::order_status(&data.status) {
                                    o.status = status;
                                }
                            }
                        });
                        if crate::account::order_status(&data.status).is_some_and(|s| {
                            matches!(s, OrderStatus::Closed | OrderStatus::Canceled)
                        }) {
                            self.mark_canceled(cx, &next);
                        }
                    }
                    job.ids.push(next);
                } else {
                    let error = VenueError::new(
                        "Bitget amendment identity is not available; refresh before further edits",
                    );
                    self.trade_fail(cx, &job, error);
                    self.refresh_orders(cx, false);
                    self.resume_trading(cx);
                    return;
                }
            }
            Effect::Leverage {
                symbol,
                leverage,
                side,
            } => {
                self.account.config_at = Some(cx.now);
                cx.store.update_setting(&symbol, |s| match side {
                    Some(PositionSide::Long) => {
                        s.leverage = leverage;
                        s.buy_leverage = Some(leverage);
                    }
                    Some(PositionSide::Short) => s.sell_leverage = Some(leverage),
                    None => {
                        s.leverage = leverage;
                        s.buy_leverage = Some(leverage);
                        s.sell_leverage = Some(leverage);
                    }
                });
                cx.store.update_positions(|p| {
                    if p.symbol == symbol && side.is_none_or(|side| p.side == side) {
                        p.leverage = leverage;
                    }
                });
            }
            Effect::Margin { symbol, margin } => {
                self.account.config_at = Some(cx.now);
                cx.store
                    .update_setting(&symbol, |s| s.margin_type = Some(margin));
                cx.store.update_positions(|p| {
                    if p.symbol == symbol {
                        p.margin_type = Some(margin);
                    }
                });
            }
            Effect::Mode(hedged) => {
                self.account.config_at = Some(cx.now);
                cx.store.set_hedged(hedged);
                cx.store.update_positions(|p| {
                    p.position_mode = Some(if hedged {
                        PositionMode::Hedged
                    } else {
                        PositionMode::Oneway
                    })
                });
            }
            _ => {}
        }
        if !write_noted && request.call.path != paths::ORDER_DETAIL {
            self.note_write(cx);
        }
        if request.call.path != paths::ORDER_DETAIL {
            job.effective = true;
        }
        self.trading.active = Some(job);
        self.resume_trading(cx);
    }

    pub(crate) fn trade_sign_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        mut error: VenueError,
    ) {
        let Some(mut job) = self.trading.active.take() else {
            return;
        };
        if job.id != request.command {
            self.trading.active = Some(job);
            return;
        }
        job.outstanding = false;
        if let Effect::Cancel { orders, .. } = &request.call.effect {
            for order in orders {
                cx.store.update_order(&order.id, |current| {
                    if current.status == OrderStatus::Canceling {
                        current.status = order.status;
                    }
                });
            }
        }
        if request.signature_retry {
            // Earlier native attempt reached the venue; never call the command wholly unsent.
            error.outcome = Some(Outcome::Unknown);
        }
        if let Some(original) = job.error.take() {
            error = VenueError {
                message: format!(
                    "{}; protection rollback failed: {}",
                    original.message, error.message
                ),
                kind: if error.outcome == Some(Outcome::Unknown) {
                    error.kind
                } else {
                    original.kind
                },
                outcome: error.outcome.filter(|outcome| *outcome == Outcome::Unknown),
                ..original
            };
        }
        self.trade_fail(cx, &job, error);
        self.resume_trading(cx);
    }

    /// Fails the job; after a write took effect it is never Rejected or NotSent.
    fn trade_fail(&mut self, cx: &mut Cx<'_, Pending>, job: &Job, mut error: VenueError) {
        if (job.effective || !job.ids.is_empty())
            && matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent))
        {
            error.outcome = None;
        }
        cx.fail(job.id, error);
    }

    fn commit_amend(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        old: &str,
        next: &str,
        values: (f64, f64, Option<Order>, u64),
        client: &str,
    ) {
        let (price, amount, fallback, stream_revision) = values;
        let original = cx.store.order(old).cloned().or(fallback);
        let newer_stream = self.order_revision(old) > stream_revision;
        if old != next {
            self.mark_canceled(cx, old);
        }
        if self.order_terminal(next)
            || (old == next
                && original.as_ref().is_some_and(|o| {
                    matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled)
                }))
        {
            return;
        }
        if let Some(mut order) = original {
            if old != next && cx.store.order(next).is_some() {
                return;
            }
            // Old-ID events describe the canceled order, not its replacement.
            if old != next || !newer_stream {
                order.price = price;
                order.amount = amount;
                amend_remaining(&mut order, self.inverse());
            }
            order.id = next.to_owned();
            order.client_order_id = Some(client.to_owned());
            order.status = OrderStatus::Open;
            self.mark_opened(cx.now, next);
            cx.store.upsert_order(order);
        }
    }

    fn retry_read(&mut self, cx: &mut Cx<'_, Pending>, mut job: Job, mut request: Request) {
        request.read_retries += 1;
        job.outstanding = true;
        self.trading.active = Some(job);
        self.trading.retry = Some((
            cx.set_timer(250.0 * f64::from(request.read_retries)),
            request,
        ));
    }

    pub(crate) fn trade_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.trading.detect_wait == Some(timer) {
            self.trading.detect_wait = None;
            if let Some(job) = self.trading.active.take() {
                let error = not_sent("Bitget account detection did not complete");
                self.trade_fail(cx, &job, error);
            }
            while let Some(command) = self.trading.queue.pop_front() {
                if let Some(id) = command_id(&command) {
                    cx.fail(id, not_sent("Bitget account detection did not complete"));
                }
            }
            return;
        }
        if self
            .trading
            .retry
            .as_ref()
            .is_some_and(|(t, _)| *t == timer)
        {
            let (_, request) = self.trading.retry.take().expect("retry");
            if let Some(mut job) = self.trading.active.take() {
                job.outstanding = false;
                self.issue_trade(
                    cx,
                    job,
                    request.call,
                    request.signature_retry,
                    request.read_retries,
                );
            }
        }
    }
}

fn command_id(command: &Command) -> Option<u32> {
    match command {
        Command::PlaceOrders { command, .. }
        | Command::CancelOrders { command, .. }
        | Command::CancelSymbolOrders { command, .. }
        | Command::AmendOrder { command, .. }
        | Command::SetLeverage { command, .. }
        | Command::SetMarginType { command, .. }
        | Command::SetPositionMode { command, .. } => Some(*command),
        _ => None,
    }
}
fn symbols(cx: &Cx<'_, Pending>, command: &Command) -> Vec<String> {
    let mut result = match command {
        Command::PlaceOrders { orders, .. } => orders.iter().map(|o| o.symbol.clone()).collect(),
        Command::AmendOrder { id, .. } => cx
            .store
            .order(id)
            .map(|o| vec![o.symbol.clone()])
            .unwrap_or_default(),
        Command::SetLeverage { symbol, .. } | Command::SetMarginType { symbol, .. } => {
            vec![symbol.clone()]
        }
        _ => vec![],
    };
    let mut seen = HashSet::new();
    result.retain(|s| seen.insert(s.clone()));
    result
}
fn needs_position_read(cx: &Cx<'_, Pending>, command: &Command, uta: bool) -> bool {
    if let Command::PlaceOrders { orders, .. } = command {
        orders.iter().any(|o| {
            is_algo(o.kind)
                && o.kind != OrderType::TrailingStopLoss
                && (o.amount > 0.0 || uta)
                && !cx.store.positions().iter().any(|p| {
                    p.symbol == o.symbol
                        && p.side == protected_position(o.side)
                        && p.contracts > 0.0
                })
        })
    } else if uta && let Command::AmendOrder { id, .. } = command {
        cx.store.order(id).is_some_and(|o| {
            is_algo(o.kind)
                && o.kind != OrderType::TrailingStopLoss
                && !cx.store.positions().iter().any(|p| {
                    p.symbol == o.symbol
                        && p.side == protected_position(o.side)
                        && p.contracts > 0.0
                })
        })
    } else {
        false
    }
}
fn mint(cx: &Cx<'_, Pending>) -> Result<String, String> {
    safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))
}
fn side_text(side: OrderSide) -> &'static str {
    if side == OrderSide::Buy {
        "buy"
    } else {
        "sell"
    }
}
fn opening_side(side: OrderSide) -> &'static str {
    if side == OrderSide::Buy {
        "long"
    } else {
        "short"
    }
}
fn protected_side(side: OrderSide) -> &'static str {
    if side == OrderSide::Buy {
        "short"
    } else {
        "long"
    }
}
fn protected_position(side: OrderSide) -> PositionSide {
    if side == OrderSide::Buy {
        PositionSide::Short
    } else {
        PositionSide::Long
    }
}
fn margin_text(margin: MarginType) -> &'static str {
    if margin == MarginType::Cross {
        "crossed"
    } else {
        "isolated"
    }
}
fn tif(value: Option<TimeInForce>) -> &'static str {
    match value.unwrap_or(TimeInForce::GoodTillCancel) {
        TimeInForce::GoodTillCancel => "gtc",
        TimeInForce::ImmediateOrCancel => "ioc",
        TimeInForce::FillOrKill => "fok",
        TimeInForce::PostOnly => "post_only",
    }
}
fn trigger(value: Option<StopTriggerType>) -> &'static str {
    if value == Some(StopTriggerType::LastPrice) {
        "fill_price"
    } else {
        "mark_price"
    }
}
fn uta_trigger(value: Option<StopTriggerType>) -> &'static str {
    if value == Some(StopTriggerType::LastPrice) {
        "market"
    } else {
        "mark"
    }
}
fn amend_remaining(order: &mut Order, inverse: bool) {
    let native = clean((order.amount - order.filled).max(0.0));
    order.remaining = if inverse && order.price > 0.0 {
        (native / order.price * 100_000_000.0).round() / 100_000_000.0
    } else {
        native
    };
    if inverse {
        order
            .extra
            .get_or_insert_with(OrderExtra::default)
            .remaining_native = Some(native);
    }
}
fn number(v: f64, step: f64) -> String {
    num::format(num::adjust(v, step))
}
fn floor_lot(v: f64, step: f64) -> f64 {
    if step > 0.0 {
        clean(((v / step) + 1e-10).floor() * step)
    } else {
        v
    }
}
fn native_id(order: &Order) -> String {
    order.parent_id.clone().unwrap_or_else(|| order.id.clone())
}
fn related(orders: &[Order], order: &Order) -> Vec<Order> {
    let id = native_id(order);
    orders
        .iter()
        .filter(|o| native_id(o) == id)
        .cloned()
        .collect()
}
fn fallback_plan(order: &Order) -> &'static str {
    match (order.kind, order.amount > 0.0) {
        (OrderType::TakeProfit, true) => "profit_plan",
        (OrderType::TakeProfit, false) => "pos_profit",
        (OrderType::TrailingStopLoss, _) => "track_plan",
        (_, true) => "loss_plan",
        _ => "pos_loss",
    }
}
fn place_opts(order: &Order) -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: order.symbol.clone(),
        kind: order.kind,
        side: order.side,
        amount: order.amount,
        price: Some(order.price),
        trigger_price: None,
        stop_loss: order.stop_loss,
        take_profit: order.take_profit,
        reduce_only: Some(order.reduce_only),
        time_in_force: order.time_in_force,
        trigger_type: order.trigger_type,
        tp_trigger_type: order.trigger_type,
        sl_trigger_type: order.trigger_type,
        client_order_id: None,
        close: order.close,
        stp_mode: None,
        max_slippage_pct: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
    }
}

impl Planned {
    fn identities(&self) -> Vec<OrderIdentityReceipt> {
        let mut receipts = Vec::new();
        match self.effect {
            Effect::Place { .. } => self.payload.placement_identities(None, &mut receipts),
            Effect::Amend { .. } => {
                if let (Some(symbol), Some(identity)) =
                    (&self.payload.symbol, &self.payload.new_client_oid)
                {
                    receipts.push(OrderIdentityReceipt {
                        symbol: symbol.clone(),
                        kind: OrderIdentityKind::ClientOrderId,
                        identity: identity.clone(),
                    });
                }
            }
            _ => {}
        }
        receipts
    }
}
impl Payload {
    fn placement_identities(
        &self,
        outer_symbol: Option<&str>,
        receipts: &mut Vec<OrderIdentityReceipt>,
    ) {
        let symbol = self.symbol.as_deref().or(outer_symbol);
        if let (Some(symbol), Some(identity)) = (symbol, &self.client_oid) {
            receipts.push(OrderIdentityReceipt {
                symbol: symbol.into(),
                kind: OrderIdentityKind::ClientOrderId,
                identity: identity.clone(),
            });
        }
        if let Some(orders) = &self.order_list {
            for order in orders {
                order.placement_identities(symbol, receipts);
            }
        }
    }
}

fn leverage_limit(value: f64, fallback: f64) -> f64 {
    if value == 0.0 { fallback } else { value }
}

/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}
