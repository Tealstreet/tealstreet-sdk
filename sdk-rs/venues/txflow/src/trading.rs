//! TxFlow writes are serialized and never retried after an uncertain response.
//! An execution acknowledgement carries no order IDs: identity comes from reads/streams.
use crate::{Pending, TxFlow, sign};
use safe_cex_engine::model::*;
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
    last_write: Option<f64>,
    pace: Option<TimerId>,
    reconcile: Option<TimerId>,
    reconcile_count: usize,
}
struct Job {
    command: CommandId,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    waiting: Vec<(String, Option<String>)>,
    /// A write took effect, so a later failure cannot mean nothing happened.
    effective: bool,
}
struct Step {
    action: Action,
    effect: Effect,
}
pub struct Request {
    command: CommandId,
    effect: Effect,
    response_type: &'static str,
}
enum Effect {
    Place(Vec<(String, Option<String>)>),
    Cancel,
    Amend {
        id: String,
        price: Option<f64>,
        amount: Option<f64>,
    },
    Setting {
        symbol: String,
        leverage: Option<f64>,
        isolated: Option<bool>,
    },
    PositionMode(bool),
}
#[derive(Serialize)]
#[serde(untagged)]
enum Action {
    Order {
        grouping: &'static str,
        orders: Vec<OrderWire>,
    },
    Cancel {
        cancels: Vec<CancelWire>,
    },
    Modify {
        order: ModifyWire,
    },
    Leverage {
        asset: u32,
        leverage: i32,
    },
    Margin {
        asset: u32,
        #[serde(rename = "isCross")]
        is_cross: bool,
    },
    Mode {
        #[serde(rename = "isOneway")]
        is_oneway: bool,
    },
}
impl Action {
    fn kind(&self) -> (&'static str, &'static str) {
        match self {
            Self::Order { .. } => ("order", "PlaceOrder"),
            Self::Cancel { .. } => ("cancel", "CancelOrder"),
            Self::Modify { .. } => ("modify2", "Modify2"),
            Self::Leverage { .. } => ("updateLeverage", "UpdateLeverage"),
            Self::Margin { .. } => ("updateMarginMode", "UpdateMarginMode"),
            Self::Mode { .. } => ("updatePositionMode", "UpdatePositionMode"),
        }
    }
}
#[derive(Serialize)]
struct OrderWire {
    a: u32,
    b: bool,
    p: String,
    s: String,
    r: bool,
    t: LimitType,
    c: String,
}
#[derive(Serialize)]
struct LimitType {
    limit: Limit,
}
#[derive(Serialize)]
struct Limit {
    tif: &'static str,
}
#[derive(Serialize)]
struct CancelWire {
    a: u32,
    o: u64,
}
#[derive(Serialize)]
struct ModifyWire {
    o: u64,
    p: String,
    s: String,
}
#[derive(Serialize)]
struct TypedAction<'a> {
    r#type: &'static str,
    #[serde(flatten)]
    body: &'a Action,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<'a> {
    action: TypedAction<'a>,
    signature: sign::Signature,
    nonce: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    vault_address: Option<&'a str>,
    confirm: &'static str,
}

impl TxFlow {
    pub(crate) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let (id, built) = match command {
            Command::PlaceOrders { command, orders } => (command, self.place_steps(cx, &orders)),
            Command::CancelOrders { command, ids } => {
                let refs = ids
                    .into_iter()
                    .map(|id| {
                        cx.store
                            .order(&id)
                            .map(|o| CancelOrderRef {
                                id: id.clone(),
                                symbol: o.symbol.clone(),
                            })
                            .ok_or_else(|| "Unknown TxFlow order; provide its symbol".to_string())
                    })
                    .collect::<Result<Vec<_>, _>>();
                (command, refs.and_then(|refs| self.cancel_steps(cx, &refs)))
            }
            Command::CancelOrdersWithSymbols { command, orders } => {
                (command, self.cancel_steps(cx, &orders))
            }
            Command::CancelSymbolOrders { command, symbol } => {
                let refs = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.symbol == symbol)
                    .map(|o| CancelOrderRef {
                        id: o.id.clone(),
                        symbol: o.symbol.clone(),
                    })
                    .collect::<Vec<_>>();
                (command, self.cancel_steps(cx, &refs))
            }
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => (
                command,
                self.amend_step(cx, &id, price, amount)
                    .map(|s| VecDeque::from([s])),
            ),
            Command::SetLeverage {
                command,
                symbol,
                leverage,
                side: _,
            } => {
                let step = self
                    .assets
                    .get(&symbol)
                    .copied()
                    .ok_or_else(|| "Unknown TxFlow market".to_string())
                    .and_then(|asset| {
                        let market = cx.store.market(&symbol).ok_or("Unknown TxFlow market")?;
                        if !leverage.is_finite()
                            || leverage.fract() != 0.0
                            || leverage < 1.0
                            || leverage > market.limits.leverage.max
                            || leverage > i32::MAX as f64
                        {
                            return Err("Invalid TxFlow leverage".into());
                        }
                        if cx.store.hedged() {
                            return Err("TxFlow hedge leverage requires an explicit side".into());
                        }
                        Ok(VecDeque::from([Step {
                            action: Action::Leverage {
                                asset,
                                leverage: leverage as i32,
                            },
                            effect: Effect::Setting {
                                symbol: symbol.clone(),
                                leverage: Some(leverage),
                                isolated: None,
                            },
                        }]))
                    });
                (command, step)
            }
            Command::SetMarginType {
                command,
                symbol,
                isolated,
            } => (
                command,
                self.assets
                    .get(&symbol)
                    .copied()
                    .ok_or_else(|| "Unknown TxFlow market".to_string())
                    .map(|asset| {
                        VecDeque::from([Step {
                            action: Action::Margin {
                                asset,
                                is_cross: !isolated,
                            },
                            effect: Effect::Setting {
                                symbol: symbol.clone(),
                                leverage: None,
                                isolated: Some(isolated),
                            },
                        }])
                    }),
            ),
            Command::SetPositionMode {
                command,
                hedged,
                symbol: _,
            } => {
                let steps = if cx.store.positions().iter().any(|p| p.contracts != 0.0)
                    || !cx.store.orders().is_empty()
                {
                    Err("TxFlow position mode requires a flat account without orders".into())
                } else {
                    Ok(VecDeque::from([Step {
                        action: Action::Mode { is_oneway: !hedged },
                        effect: Effect::PositionMode(hedged),
                    }]))
                };
                (command, steps)
            }
            Command::FetchLeverage { command, symbol } => {
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Leverage {
                    buy_leverage: f64,
                    sell_leverage: f64,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    margin_type: Option<MarginType>,
                }
                let result =
                    self.confirmed_setting(cx, &symbol)
                        .filter(|s| s.leverage.is_finite() && s.leverage > 0.0)
                        .and_then(|s| {
                            let buy = s.buy_leverage.unwrap_or(s.leverage);
                            let sell = s.sell_leverage.unwrap_or(s.leverage);
                            (buy.is_finite() && buy > 0.0 && sell.is_finite() && sell > 0.0)
                                .then_some(Leverage {
                                    buy_leverage: buy,
                                    sell_leverage: sell,
                                    margin_type: s.margin_type,
                                })
                        })
                        .map(|leverage| serde_json::to_string(&leverage).unwrap())
                        .ok_or_else(|| {
                            format!("TxFlow account leverage is not confirmed for {symbol}")
                        });
                match result {
                    Ok(data) => cx.done(command, data),
                    Err(error) => cx.fail(command, VenueError::new(error)),
                }
                return;
            }
            Command::FetchAccount { command } => {
                cx.fail(command, not_sent("TxFlow account query completion is not implemented; use live account state").kind(ErrorKind::Unsupported));
                return;
            }
            Command::FetchOrderBook { command, .. } => {
                cx.fail(
                    command,
                    not_sent(
                        "TxFlow order book queries are not implemented; use the book subscription",
                    )
                    .kind(ErrorKind::Unsupported),
                );
                return;
            }
            Command::FetchStpMode { command } | Command::SetStpMode { command, .. } => {
                cx.fail(
                    command,
                    not_sent("TxFlow does not document configurable self-trade prevention")
                        .kind(ErrorKind::Unsupported),
                );
                return;
            }
            _ => return,
        };
        if !self.options.testnet || self.options.public_data_only || self.user().is_none() {
            // Only a missing account is an auth problem; mainnet or public-only is unsupported.
            let kind = if !self.options.testnet || self.options.public_data_only {
                ErrorKind::Unsupported
            } else {
                ErrorKind::AuthInvalid
            };
            cx.fail(
                id,
                not_sent("TxFlow trading requires an authenticated testnet account").kind(kind),
            );
            return;
        }
        match built {
            Err(error) => cx.fail(id, not_sent(error)),
            Ok(steps) => {
                self.trading.queue.push_back(Job {
                    command: id,
                    steps,
                    ids: vec![],
                    waiting: vec![],
                    effective: false,
                });
                self.pump_trades(cx);
            }
        }
    }
    fn place_steps(
        &self,
        cx: &Cx<'_, Pending>,
        orders: &[PlaceOrderOpts],
    ) -> Result<VecDeque<Step>, String> {
        // Apply the shared caller-ID contract to the entire command before
        // evaluating venue shape controls or minting any native identities.
        let mut caller_ids = HashSet::new();
        for opts in orders {
            let Some(id) = &opts.client_order_id else {
                continue;
            };
            if !matches!(opts.kind, OrderType::Limit | OrderType::Market)
                || opts.trigger_price.is_some()
                || opts.stop_loss.is_some()
                || opts.take_profit.is_some()
            {
                let kind = match opts.kind {
                    OrderType::Limit => "limit",
                    OrderType::Market => "market",
                    OrderType::Conditional => "conditional",
                    OrderType::StopLoss => "stop_market",
                    OrderType::TakeProfit => "take_profit_market",
                    OrderType::TrailingStopLoss => "trailing_stop_market",
                };
                return Err(format!(
                    "TxFlow clientOrderId cannot be honored for this order shape (type={kind}{}{}{}{}); caller client order IDs are supported only on plain Limit/Market orders",
                    if opts.stop_loss.is_some() {
                        " +stopLoss"
                    } else {
                        ""
                    },
                    if opts.take_profit.is_some() {
                        " +takeProfit"
                    } else {
                        ""
                    },
                    if opts.trigger_price.is_some() {
                        " +triggerPrice"
                    } else {
                        ""
                    },
                    if opts.close == Some(true) {
                        " +close"
                    } else {
                        ""
                    }
                ));
            }
            if !valid_cloid(id) {
                return Err("TxFlow clientOrderId must be nonempty, at most 64 UTF-8 bytes and not start with 0x".into());
            }
            if cx.store.market(&opts.symbol).is_some_and(|market| {
                market.limits.amount.max > 0.0
                    && num::adjust(opts.amount, market.precision.amount) > market.limits.amount.max
            }) {
                return Err("TxFlow order exceeds the market's max size for one order; a caller clientOrderId cannot span several orders".into());
            }
            if !caller_ids.insert(id) {
                return Err(format!(
                    "TxFlow clientOrderId {id} is duplicated in one batch"
                ));
            }
        }
        let mut ids = HashSet::new();
        let mut wires = Vec::with_capacity(orders.len());
        for opts in orders {
            let market = cx
                .store
                .market(&opts.symbol)
                .ok_or("Unknown TxFlow market")?;
            if !market.active {
                return Err("TxFlow market is halted".into());
            }
            if opts.stop_loss.is_some()
                || opts.take_profit.is_some()
                || opts.reduce_only == Some(true)
                || opts.close == Some(true)
                || !matches!(opts.kind, OrderType::Limit | OrderType::Market)
                || opts.trigger_price.is_some()
                || opts.trigger_type.is_some()
                || opts.tp_trigger_type.is_some()
                || opts.sl_trigger_type.is_some()
            {
                return Err("TxFlow protective, conditional and reduce-only semantics require testnet measurement".into());
            }
            if opts.stp_mode.is_some()
                || opts.bybit_bbo.is_some()
                || opts.bybit_slippage_tolerance_ticks.is_some()
            {
                return Err("TxFlow does not accept venue-specific execution controls".into());
            }
            if opts
                .max_slippage_pct
                .is_some_and(|v| !v.is_finite() || v != 5.0)
                || (opts.kind != OrderType::Market && opts.max_slippage_pct.is_some())
            {
                return Err("TxFlow market slippage is fixed at 5 percent".into());
            }
            let tif = match (opts.kind, opts.time_in_force) {
                (OrderType::Market, None | Some(TimeInForce::ImmediateOrCancel)) => {
                    "FrontendMarket"
                }
                (OrderType::Market, _) => {
                    return Err("TxFlow market orders use immediate execution".into());
                }
                (_, None | Some(TimeInForce::GoodTillCancel)) => "Gtc",
                (_, Some(TimeInForce::ImmediateOrCancel)) => "Ioc",
                (_, Some(TimeInForce::PostOnly)) => "Alo",
                (_, Some(TimeInForce::FillOrKill)) => {
                    return Err("TxFlow does not document fill-or-kill".into());
                }
            };
            let amount = checked_number(opts.amount, market.precision.amount)?;
            if let Some(instrument) = self.instruments.get(
                self.assets
                    .get(&opts.symbol)
                    .ok_or("Unknown TxFlow asset")?,
            ) {
                let max = if opts.kind == OrderType::Market {
                    instrument.max_market_order_size
                } else {
                    instrument.max_limit_order_size
                };
                if max.is_some_and(|max| opts.amount > max) {
                    return Err("TxFlow order size exceeds native order-type limit".into());
                }
                if instrument.use_notional_margin {
                    return Err(
                        "TxFlow notional-margin semantics require testnet measurement".into(),
                    );
                }
            }
            if opts.amount < market.limits.amount.min || opts.amount > market.limits.amount.max {
                return Err("TxFlow order size exceeds market limits".into());
            }
            let price = if opts.kind == OrderType::Market {
                if opts.price.is_some() {
                    return Err("TxFlow market orders do not accept a limit price".into());
                }
                "bbo".into()
            } else {
                checked_number(
                    opts.price.ok_or("TxFlow limit price is required")?,
                    market.precision.price,
                )?
            };
            let cloid = match &opts.client_order_id {
                Some(id) if valid_cloid(id) => id.clone(),
                Some(_) => return Err("TxFlow clientOrderId must be nonempty, at most 64 UTF-8 bytes and not start with 0x".into()),
                None => format!("ts_{}", safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))?),
            };
            if !ids.insert(cloid.clone()) || cx.store.orders().iter().any(|o| o.client_order_id.as_deref() == Some(&cloid)) || self.trading.active.iter().chain(self.trading.queue.iter()).any(|job| job.waiting.iter().any(|(id,_)| id == &cloid) || job.steps.iter().any(|step| matches!(&step.effect, Effect::Place(ids) if ids.iter().any(|(id,_)| id == &cloid)))) {
                return Err("Duplicate TxFlow clientOrderId".into());
            }
            wires.push(OrderWire {
                a: *self
                    .assets
                    .get(&opts.symbol)
                    .ok_or("Unknown TxFlow asset")?,
                b: opts.side == OrderSide::Buy,
                p: price,
                s: amount,
                r: false,
                t: LimitType {
                    limit: Limit { tif },
                },
                c: cloid,
            });
        }
        let mut steps = VecDeque::new();
        while !wires.is_empty() {
            let chunk = wires.drain(..wires.len().min(50)).collect::<Vec<_>>();
            let ids = chunk
                .iter()
                .map(|o| (o.c.clone(), self.account.order_ids.get(&o.c).cloned()))
                .collect();
            steps.push_back(Step {
                action: Action::Order {
                    grouping: "na",
                    orders: chunk,
                },
                effect: Effect::Place(ids),
            });
        }
        Ok(steps)
    }
    fn cancel_steps(
        &self,
        cx: &Cx<'_, Pending>,
        refs: &[CancelOrderRef],
    ) -> Result<VecDeque<Step>, String> {
        let mut wires = Vec::with_capacity(refs.len());
        for r in refs {
            if cx
                .store
                .order(&r.id)
                .is_some_and(|o| !matches!(o.kind, OrderType::Limit | OrderType::Market))
            {
                return Err("TxFlow grouped cancellation requires native grouping metadata".into());
            }
            wires.push(CancelWire {
                a: *self
                    .assets
                    .get(&r.symbol)
                    .ok_or("Unknown TxFlow cancel asset")?,
                o: r.id.parse().map_err(|_| "Invalid TxFlow order ID")?,
            });
        }
        let mut steps = VecDeque::new();
        while !wires.is_empty() {
            steps.push_back(Step {
                action: Action::Cancel {
                    cancels: wires.drain(..wires.len().min(200)).collect(),
                },
                effect: Effect::Cancel,
            });
        }
        Ok(steps)
    }
    fn amend_step(
        &self,
        cx: &Cx<'_, Pending>,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<Step, String> {
        let order = cx
            .store
            .order(id)
            .ok_or("TxFlow amend requires a known resting order")?;
        if order.status != OrderStatus::Open || order.kind != OrderType::Limit {
            return Err("TxFlow modify2 requires a resting limit order".into());
        }
        if order.take_profit.is_some()
            || order.stop_loss.is_some()
            || order
                .extra
                .as_ref()
                .is_none_or(|e| !e.take_profit_known || !e.stop_loss_known)
            || cx
                .store
                .orders()
                .iter()
                .any(|o| o.parent_id.as_deref() == Some(id))
        {
            return Err("TxFlow modify2 cannot clear unverified TP/SL children".into());
        }
        let market = cx
            .store
            .market(&order.symbol)
            .ok_or("Unknown TxFlow market")?;
        let total = amount.unwrap_or(order.amount);
        if !market.active || total < market.limits.amount.min || total > market.limits.amount.max {
            return Err("Invalid TxFlow amend size or halted market".into());
        }
        Ok(Step {
            action: Action::Modify {
                order: ModifyWire {
                    o: id.parse().map_err(|_| "Invalid TxFlow order ID")?,
                    p: checked_number(price.unwrap_or(order.price), market.precision.price)?,
                    s: checked_number(total, market.precision.amount)?,
                },
            },
            effect: Effect::Amend {
                id: id.into(),
                price,
                amount,
            },
        })
    }
    fn pump_trades(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading.inflight
            || self.trading.pace.is_some()
            || self
                .trading
                .active
                .as_ref()
                .is_some_and(|j| !j.waiting.is_empty())
        {
            return;
        }
        if self.trading.active.is_none() {
            self.trading.active = self.trading.queue.pop_front();
        }
        let Some(job) = self.trading.active.as_mut() else {
            return;
        };
        if job.steps.is_empty() {
            self.finish_trade(cx, Ok(()));
            return;
        }
        if let Some(last) = self.trading.last_write
            && cx.now - last < 110.0
        {
            self.trading.pace = Some(cx.set_timer((110.0 - (cx.now - last)).max(1.0)));
            return;
        }
        let mut step = job.steps.pop_front().unwrap();
        if let Effect::Amend { id, price, amount } = &step.effect {
            step = match self.amend_step(cx, id, *price, *amount) {
                Ok(step) => step,
                Err(error) => {
                    self.finish_trade(cx, Err(not_sent(error)));
                    return;
                }
            };
        }
        let Step { action, effect } = step;
        if matches!(&action, Action::Leverage { .. }) && cx.store.hedged() {
            self.finish_trade(
                cx,
                Err(not_sent("TxFlow hedge leverage requires an explicit side")),
            );
            return;
        }
        if matches!(&action, Action::Mode { .. })
            && (!cx.store.orders().is_empty()
                || cx.store.positions().iter().any(|p| p.contracts != 0.0))
        {
            self.finish_trade(
                cx,
                Err(not_sent(
                    "TxFlow position mode requires a flat account without orders",
                )),
            );
            return;
        }
        if let Action::Order { orders, .. } = &action
            && orders.iter().any(|o| {
                self.symbol(o.a)
                    .and_then(|symbol| cx.store.market(symbol))
                    .is_none_or(|m| !m.active)
            })
        {
            self.finish_trade(cx, Err(not_sent("TxFlow market is halted")));
            return;
        }
        let nonce = match safe_cex_venue_hyperliquid::sign::reserve_agent_nonce(
            cx,
            self.options.secret.as_deref().unwrap_or(""),
        ) {
            Ok(n) => n,
            Err(e) => {
                self.finish_trade(cx, Err(not_sent(e)));
                return;
            }
        };
        let signature = match sign::sign(
            &action,
            self.options.vault.as_deref(),
            nonce,
            self.options.secret.as_deref().unwrap_or(""),
        ) {
            Ok(s) => s,
            Err(e) => {
                self.finish_trade(cx, Err(not_sent(e)));
                return;
            }
        };
        let (kind, response_type) = action.kind();
        let body = serde_json::to_string(&Envelope {
            action: TypedAction {
                r#type: kind,
                body: &action,
            },
            signature,
            nonce,
            vault_address: self.options.vault.as_deref(),
            confirm: "executed",
        })
        .unwrap();
        let command = self.trading.active.as_ref().unwrap().command;
        if let Effect::Place(ids) = &effect {
            self.trading.active.as_mut().unwrap().waiting = ids.clone();
        }
        self.trading.inflight = true;
        self.trading.last_write = Some(cx.now);
        if let Action::Order { orders, .. } = &action {
            cx.command_progress(
                command,
                orders
                    .iter()
                    .map(|order| OrderIdentityReceipt {
                        // Active market validation above already established this asset.
                        symbol: self
                            .symbol(order.a)
                            .expect("validated order asset")
                            .to_owned(),
                        kind: OrderIdentityKind::ClientOrderId,
                        identity: order.c.clone(),
                    })
                    .collect(),
            );
        }
        cx.request(
            Method::Post,
            format!("{}/exchange", self.rest()),
            vec![
                ("Content-Type", "application/json".into()),
                ("x-api-version", "2026-08-16".into()),
            ],
            Some(body),
            Pending::Trade(Request {
                command,
                effect,
                response_type,
            }),
        );
    }
    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Result<Response<'_>, &str>,
    ) {
        if self
            .trading
            .active
            .as_ref()
            .is_none_or(|j| j.command != request.command)
        {
            return;
        }
        self.trading.inflight = false;
        // A transport or HTTP failure may follow execution.
        if let Err(mut error) = response
            .map_err(|error| unknown(error).kind(ErrorKind::Network))
            .and_then(|r| parse_reply(r, request.response_type))
        {
            self.acknowledge_write(cx);
            error.message = format!("{}; reconcile before any resubmission", error.message);
            self.finish_trade(cx, Err(error));
            return;
        }
        self.trading.active.as_mut().unwrap().effective = true;
        match request.effect {
            Effect::Place(cloids) => self.trading.active.as_mut().unwrap().waiting = cloids,
            Effect::Amend { id, .. } => self.trading.active.as_mut().unwrap().ids.push(id),
            Effect::PositionMode(hedged) => cx.store.set_hedged(hedged),
            Effect::Setting {
                symbol,
                leverage,
                isolated,
            } => {
                if let Some(mut setting) = self.confirmed_setting(cx, &symbol).copied() {
                    if let Some(leverage) = leverage {
                        setting.leverage = leverage;
                    }
                    if let Some(isolated) = isolated {
                        setting.margin_type = Some(if isolated {
                            MarginType::Isolated
                        } else {
                            MarginType::Cross
                        });
                    }
                    self.confirm_setting(cx, &symbol, &setting);
                }
            }
            Effect::Cancel => (),
        }
        self.acknowledge_write(cx);
        self.trading_reconcile(cx);
    }
    pub(crate) fn trading_reconcile(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading.inflight {
            return;
        }
        let Some(job) = self.trading.active.as_mut() else {
            return;
        };
        let resolved = job
            .waiting
            .iter()
            .map(|(cloid, previous_id)| {
                self.account
                    .order_ids
                    .get(cloid)
                    .cloned()
                    .or_else(|| {
                        cx.store
                            .orders()
                            .iter()
                            .find(|o| o.client_order_id.as_deref() == Some(cloid))
                            .map(|o| o.id.clone())
                    })
                    .filter(|id| Some(id) != previous_id.as_ref())
            })
            .collect::<Option<Vec<_>>>();
        if let Some(ids) = resolved {
            job.ids.extend(ids);
            job.waiting.clear();
        }
        if job.waiting.is_empty() {
            if let Some(timer) = self.trading.reconcile.take() {
                cx.clear_timer(timer);
            }
            self.trading.reconcile_count = 0;
            self.pump_trades(cx);
        } else if self.trading.reconcile.is_none() {
            self.trading.reconcile = Some(cx.set_timer(300.0));
        }
    }
    pub(crate) fn trade_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.trading.pace == Some(timer) {
            self.trading.pace = None;
            self.pump_trades(cx);
        }
        if self.trading.reconcile == Some(timer) {
            self.trading.reconcile = None;
            self.trading.reconcile_count += 1;
            if self.trading.reconcile_count >= 8 {
                self.finish_trade(cx, Err(unknown("TxFlow executed placement has unresolved identity; reconcile by clientOrderId before resubmitting")));
            } else {
                self.refresh_orders(cx);
                self.trading_reconcile(cx);
            }
        }
    }
    fn finish_trade(&mut self, cx: &mut Cx<'_, Pending>, result: Result<(), VenueError>) {
        self.trading.inflight = false;
        if let Some(timer) = self.trading.reconcile.take() {
            cx.clear_timer(timer);
        }
        self.trading.reconcile_count = 0;
        if let Some(job) = self.trading.active.take() {
            match result {
                Err(mut error) => {
                    if (job.effective || !job.ids.is_empty())
                        && matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent))
                    {
                        error.outcome = None;
                    }
                    cx.fail(job.command, error);
                }
                Ok(()) => cx.done(job.command, serde_json::to_string(&job.ids).unwrap()),
            }
        }
        self.pump_trades(cx);
    }
}
pub(crate) fn valid_cloid(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && !id.starts_with("0x")
}
fn checked_number(value: f64, step: f64) -> Result<String, String> {
    if !value.is_finite() || value <= 0.0 || !step.is_finite() || step <= 0.0 {
        return Err("Invalid TxFlow order number".into());
    }
    if (num::adjust(value, step) - value).abs() > step * 1e-7 {
        return Err("TxFlow order number does not match market precision".into());
    }
    let text = format!("{value:.12}");
    if text.parse::<f64>().ok() != Some(value) {
        return Err("TxFlow order number exceeds supported precision".into());
    }
    Ok(text.trim_end_matches('0').trim_end_matches('.').into())
}
#[derive(Deserialize)]
struct Reply {
    status: String,
    #[serde(default)]
    confirm: Option<String>,
    #[serde(default)]
    code: Option<i64>,
    #[serde(default)]
    data: Option<ReplyData>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    response: Option<String>,
}
#[derive(Deserialize)]
struct ReplyData {
    r#type: String,
    data: Statuses,
}
#[derive(Deserialize)]
struct Statuses {
    statuses: Vec<Status>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Status {
    Text(String),
    Error { error: String },
}
fn parse_reply(response: Response<'_>, expected: &str) -> Result<(), VenueError> {
    if response.status != 200 {
        return Err(unknown(format!(
            "TxFlow HTTP {} execution outcome is uncertain",
            response.status
        )));
    }
    let reply: Reply = serde_json::from_str(response.body)
        .map_err(|_| unknown("Invalid TxFlow execution response"))?;
    if reply.status == "err" {
        let message = reply
            .message
            .or(reply.response)
            .unwrap_or_else(|| "TxFlow action rejected".into());
        return Err(rejected(message));
    }
    if reply.status != "ok" || reply.code != Some(0) || reply.confirm.as_deref() != Some("executed")
    {
        return Err(unknown("TxFlow execution outcome is uncertain"));
    }
    let data = reply
        .data
        .ok_or_else(|| unknown("TxFlow queued response does not confirm execution"))?;
    if data.r#type != expected || data.data.statuses.len() != 1 {
        return Err(unknown("Invalid TxFlow whole-action acknowledgement"));
    }
    match data.data.statuses.into_iter().next().unwrap() {
        Status::Text(s) if s == "success" => Ok(()),
        Status::Error { error } => Err(rejected(error)),
        _ => Err(unknown("Unknown TxFlow execution status")),
    }
}

/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

/// An explicit refusal of the whole single-status action.
fn rejected(message: impl Into<String>) -> VenueError {
    VenueError::new(message)
        .kind(ErrorKind::Rejected)
        .outcome(Outcome::Rejected)
}

/// The write may have executed; reconcile before resubmitting.
fn unknown(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn caller_ids_count_utf8_bytes_and_reserve_prefix() {
        assert!(valid_cloid(&"é".repeat(32)));
        assert!(!valid_cloid(&"é".repeat(33)));
        assert!(!valid_cloid("0xcaller"));
        assert!(!valid_cloid(""));
    }

    #[test]
    fn one_status_ack_applies_to_entire_action_and_never_yields_id() {
        let r = Response {
            status: 200,
            headers: "",
            body: r#"{"status":"ok","confirm":"executed","code":0,"data":{"type":"PlaceOrder","data":{"statuses":["success"]}}}"#,
        };
        assert!(parse_reply(r, "PlaceOrder").is_ok());
        assert!(parse_reply(r, "CancelOrder").is_err());
        assert!(parse_reply(Response { status: 202, ..r }, "PlaceOrder").is_err());
    }
    #[test]
    fn signed_order_starts_grouping_then_orders_without_type() {
        let a = Action::Order {
            grouping: "na",
            orders: vec![],
        };
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            r#"{"grouping":"na","orders":[]}"#
        );
        assert_eq!(
            serde_json::to_string(&TypedAction {
                r#type: "order",
                body: &a
            })
            .unwrap(),
            r#"{"type":"order","grouping":"na","orders":[]}"#
        );
    }
    #[test]
    fn precision_is_rejected_not_silently_rounded() {
        assert!(checked_number(1.234, 0.01).is_err());
        assert_eq!(checked_number(1.23, 0.01).unwrap(), "1.23");
    }
}
