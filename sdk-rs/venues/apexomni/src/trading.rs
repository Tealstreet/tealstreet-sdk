//! Sequenced trading jobs; preflight and signatures complete before any placement.
use crate::{ApexOmni, Pending, sign};
use safe_cex_engine::model::*;
use safe_cex_engine::{Command, CommandId, Cx, LogLevel, Response, TimerId, num};
use serde::Deserialize;
use std::collections::{HashSet, VecDeque};

#[derive(Clone)]
enum Accepted {
    Place,
    Cancel(Vec<String>),
    AmendCancel,
    CancelSymbol(String),
    Leverage(String, f64),
}
enum Step {
    Build(Vec<PlaceOrderOpts>),
    Amend(String, Option<f64>, Option<f64>),
    Write(sign::Request, Accepted),
    Delay,
}
struct Job {
    command: CommandId,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    placements: bool,
}
pub struct Request {
    command: CommandId,
    accepted: Accepted,
}
struct Amendment {
    id: String,
    order: PlaceOrderOpts,
    total_amount: f64,
    baseline_filled: f64,
    filled: f64,
    fill_total: f64,
    fills: Vec<(f64, f64)>,
    reported: f64,
    reported_at: f64,
    overflow: bool,
    terminal: bool,
    default_amount: bool,
    started: f64,
    prepared: Vec<Step>,
}
impl Amendment {
    fn observe(&mut self) {
        let after_report = self
            .fills
            .iter()
            .filter(|(timestamp, _)| *timestamp > self.reported_at)
            .map(|(_, amount)| amount)
            .sum::<f64>();
        self.filled = self
            .filled
            .max(self.baseline_filled + self.fill_total)
            .max(self.reported + after_report);
    }
}
#[derive(Default)]
pub struct Trading {
    queue: VecDeque<Job>,
    active: Option<Job>,
    waiting: bool,
    timer: Option<TimerId>,
    amendment: Option<Amendment>,
}

fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}
fn side(side: OrderSide) -> &'static str {
    if side == OrderSide::Buy {
        "BUY"
    } else {
        "SELL"
    }
}
fn pair(key: &str, value: impl Into<String>) -> (String, String) {
    (key.into(), value.into())
}
fn mint_id(
    account: &str,
    now: f64,
    issued: &mut HashSet<String>,
    mut entropy: impl FnMut() -> Result<String, String>,
) -> Result<String, String> {
    for _ in 0..16 {
        let hash = safe_cex_crypto::sha256(entropy()?.as_bytes());
        let suffix = 100_000 + u32::from_be_bytes(hash[..4].try_into().unwrap()) % 900_000;
        let id = format!("apexomni-{account}-{}-{suffix}", num::format(now.floor()));
        if issued.insert(id.clone()) {
            return Ok(id);
        }
    }
    Err("ApeX Omni could not allocate a unique client order ID".into())
}
fn generated_id(account: &str, now: f64, issued: &mut HashSet<String>) -> Result<String, String> {
    mint_id(account, now, issued, safe_cex_crypto::uuid::uuid)
}
fn parent_id(id: &str) -> &str {
    id.strip_suffix("__stop_loss")
        .or_else(|| id.strip_suffix("__take_profit"))
        .unwrap_or(id)
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && !id.chars().any(|ch| ch.is_whitespace() || ch == '\u{feff}')
}

fn lots(order: &PlaceOrderOpts, market: &Market) -> Result<Vec<f64>, String> {
    let amount = num::adjust(order.amount, market.precision.amount);
    if !positive(order.amount) || !positive(amount) || amount < market.limits.amount.min {
        return Err("ApeX Omni order size is below the market minimum".into());
    }
    let maximum = market.limits.amount.max;
    if !positive(maximum) || !positive(market.precision.amount) {
        return Err("ApeX Omni market has invalid lot limits".into());
    }
    if amount <= maximum {
        return Ok(vec![amount]);
    }
    if order.stop_loss.is_some() || order.take_profit.is_some() {
        return Err("ApeX Omni cannot atomically attach TP/SL to split orders".into());
    }
    // Allocate whole ticks so splitting cannot create or lose a tradable unit.
    let ticks = (amount / market.precision.amount).round();
    let max_ticks = (maximum / market.precision.amount).floor();
    let min_ticks = (market.limits.amount.min / market.precision.amount)
        .ceil()
        .max(1.0);
    let count = (ticks / max_ticks).ceil();
    if !count.is_finite() || !(1.0..=1000.0).contains(&count) || ticks > 9_007_199_254_740_991.0 {
        return Err("ApeX Omni order requires too many lots".into());
    }
    let base = (ticks / count).floor();
    if base < min_ticks {
        return Err("ApeX Omni split size is below the market minimum".into());
    }
    let remainder = (ticks - base * count) as usize;
    Ok((0..count as usize)
        .map(|i| {
            num::adjust(
                (base + f64::from(i < remainder)) * market.precision.amount,
                market.precision.amount,
            )
        })
        .collect())
}

fn validate(orders: &[PlaceOrderOpts], cx: &Cx<'_, Pending>) -> Result<(), String> {
    let mut ids = HashSet::new();
    for order in orders {
        if order.stp_mode.is_some() {
            return Err("ApeX Omni does not support per-order self-trade prevention".into());
        }
        if order.bybit_bbo.is_some() || order.bybit_slippage_tolerance_ticks.is_some() {
            return Err("Bybit BBO and TickSize controls are not supported by ApeX Omni".into());
        }
        let market = cx
            .store
            .market(&order.symbol)
            .ok_or_else(|| format!("ApeX Omni market {} not found", order.symbol))?;
        if let Some(id) = &order.client_order_id {
            if !valid_id(id) {
                return Err(
                    "ApeX Omni client order ID must be non-empty without whitespace".into(),
                );
            }
            if !ids.insert(id) {
                return Err("ApeX Omni batch contains duplicate client order IDs".into());
            }
            if !matches!(order.kind, OrderType::Limit | OrderType::Market)
                || order.trigger_price.is_some()
                || order.stop_loss.is_some()
                || order.take_profit.is_some()
                || order.amount > market.limits.amount.max
            {
                return Err(
                    "ApeX Omni caller client order ID requires one plain Limit/Market order".into(),
                );
            }
        }
        if matches!(
            order.kind,
            OrderType::Conditional | OrderType::TrailingStopLoss
        ) {
            return Err(
                "ApeX Omni does not support opening conditional or trailing-stop orders".into(),
            );
        }
        if order.kind == OrderType::Limit && !order.price.is_some_and(positive) {
            return Err("ApeX Omni limit order requires a positive price".into());
        }
        if matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit)
            && !order.trigger_price.or(order.price).is_some_and(positive)
        {
            return Err("ApeX Omni trigger order requires a positive trigger price".into());
        }
        if [
            order.price,
            order.trigger_price,
            order.stop_loss,
            order.take_profit,
        ]
        .into_iter()
        .flatten()
        .any(|v| !positive(v))
        {
            return Err("ApeX Omni order prices must be finite and positive".into());
        }
        lots(order, market)?;
    }
    Ok(())
}

fn replacement(
    cx: &Cx<'_, Pending>,
    id: &str,
    price: Option<f64>,
    amount: Option<f64>,
) -> Result<PlaceOrderOpts, String> {
    let target = cx
        .store
        .order(id)
        .ok_or_else(|| format!("ApeX Omni order {id} not found"))?;
    let base_id = parent_id(id);
    let parent = cx
        .store
        .order(base_id)
        .ok_or_else(|| format!("ApeX Omni parent order {base_id} not found"))?;
    if !matches!(parent.status, OrderStatus::Open | OrderStatus::Creating)
        || parent.remaining <= 0.0
    {
        return Err("ApeX Omni cannot amend a terminal order".into());
    }
    let tp = cx.store.order(&format!("{base_id}__take_profit"));
    let sl = cx.store.order(&format!("{base_id}__stop_loss"));
    let mut order = PlaceOrderOpts {
        symbol: parent.symbol.clone(),
        kind: parent.kind,
        side: parent.side,
        amount: amount.unwrap_or(parent.remaining),
        price: Some(parent.price),
        trigger_price: None,
        max_slippage_pct: None,
        stop_loss: sl.map(|o| o.price).or(parent.stop_loss),
        take_profit: tp.map(|o| o.price).or(parent.take_profit),
        reduce_only: Some(parent.reduce_only),
        time_in_force: parent.time_in_force,
        trigger_type: parent.trigger_type,
        tp_trigger_type: tp
            .and_then(|o| o.trigger_type)
            .or(parent.extra.as_ref().and_then(|e| e.tp_trigger_type)),
        sl_trigger_type: sl
            .and_then(|o| o.trigger_type)
            .or(parent.extra.as_ref().and_then(|e| e.sl_trigger_type)),
        client_order_id: None,
        close: parent.close,
        stp_mode: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
    };
    if let Some(price) = price {
        if target.parent_id.is_some() || id != base_id {
            if id.ends_with("__take_profit") {
                order.take_profit = Some(price);
            } else {
                order.stop_loss = Some(price);
            }
        } else {
            order.price = Some(price);
        }
    }
    Ok(order)
}

impl ApexOmni {
    fn prepare_order(
        &self,
        cx: &Cx<'_, Pending>,
        order: &PlaceOrderOpts,
        issued: &mut HashSet<String>,
    ) -> Result<Vec<Step>, String> {
        let seed = self
            .options
            .omni_key_seed
            .as_deref()
            .ok_or("Invalid OmniKey seed")?;
        sign::validate_seed(seed)?;
        if !self.account.initialized {
            return Err("ApeX Omni account signing state is unavailable".into());
        }
        let market = cx
            .store
            .market(&order.symbol)
            .ok_or("ApeX Omni market not found")?;
        let config = self
            .contracts
            .get(&market.id)
            .or_else(|| self.contracts.get(&market.symbol))
            .ok_or("ApeX Omni contract signing config is unavailable")?;
        let tick = market.precision.price;
        let price = match order.kind {
            OrderType::Limit => num::adjust(order.price.unwrap_or(0.0), tick),
            OrderType::Market => {
                let mark = cx.store.ticker(&market.symbol).map_or(0.0, |t| t.mark);
                if !positive(mark) {
                    return Err("ApeX Omni market order requires a current mark price".into());
                }
                num::adjust(
                    mark * if order.side == OrderSide::Buy {
                        1.02
                    } else {
                        0.98
                    },
                    tick,
                )
            }
            _ => num::adjust(
                num::adjust(order.trigger_price.or(order.price).unwrap_or(0.0), tick)
                    * if order.side == OrderSide::Buy {
                        1.005
                    } else {
                        0.995
                    },
                tick,
            ),
        };
        if !positive(price) {
            return Err("ApeX Omni execution price rounds to zero".into());
        }
        let expiration =
            (((cx.now + 30.0 * 86_400_000.0) / 3_600_000.0).ceil() + 168.0) * 3_600_000.0;
        let taker_fee = self
            .account
            .taker_fee
            .parse::<f64>()
            .map_err(|_| "ApeX Omni invalid taker fee")?;
        if !taker_fee.is_finite() || taker_fee < 0.0 {
            return Err("ApeX Omni invalid taker fee".into());
        }
        let mut steps = Vec::new();
        for amount in lots(order, market)? {
            let id = order
                .client_order_id
                .clone()
                .map(Ok)
                .unwrap_or_else(|| generated_id(&self.account.account_id, cx.now, issued))?;
            let size = num::format(amount);
            let price_text = num::format(price);
            let signature = sign::contract_signature(
                seed,
                &id,
                &self.account.account_id,
                config.pair_id,
                &size,
                &price_text,
                &self.account.maker_fee,
                &self.account.taker_fee,
                order.side == OrderSide::Buy,
            )?;
            let mut fields = vec![
                pair("symbol", market.id.clone()),
                pair("side", side(order.side)),
                pair(
                    "type",
                    match order.kind {
                        OrderType::Limit => "LIMIT",
                        OrderType::Market => "MARKET",
                        OrderType::StopLoss => "STOP_LIMIT",
                        OrderType::TakeProfit => "TAKE_PROFIT_LIMIT",
                        _ => unreachable!(),
                    },
                ),
                pair("size", size.clone()),
                pair("price", price_text),
                pair(
                    "timeInForce",
                    match order.time_in_force.unwrap_or(TimeInForce::GoodTillCancel) {
                        TimeInForce::GoodTillCancel => "GOOD_TIL_CANCEL",
                        TimeInForce::ImmediateOrCancel => "IMMEDIATE_OR_CANCEL",
                        TimeInForce::FillOrKill => "FILL_OR_KILL",
                        TimeInForce::PostOnly => "POST_ONLY",
                    },
                ),
                pair("brokerId", "6587"),
                pair("clientId", id.clone()),
                pair("clientOrderId", id),
                pair("signature", signature),
                pair("expiration", num::format(expiration)),
                pair(
                    "limitFee",
                    sign::limit_fee(&num::format(price), &size, &self.account.taker_fee)?,
                ),
            ];
            // Placement's caller serializes once with falsy filtering before API auth.
            if order.reduce_only.unwrap_or(false) {
                fields.push(pair("reduceOnly", "true"));
            }
            if matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit) {
                let trigger = num::adjust(order.trigger_price.or(order.price).unwrap(), tick);
                fields.push(pair("triggerPrice", num::format(trigger)));
                let kind = if order.kind == OrderType::StopLoss {
                    order.sl_trigger_type
                } else {
                    order.tp_trigger_type
                };
                let trigger_type = if order.kind == OrderType::StopLoss {
                    if kind == Some(StopTriggerType::LastPrice) {
                        "MARKET"
                    } else {
                        "INDEX"
                    }
                } else if kind == Some(StopTriggerType::IndexPrice) {
                    "INDEX"
                } else {
                    "MARKET"
                };
                fields.push(pair("triggerPriceType", trigger_type));
            }
            if order.take_profit.is_some() || order.stop_loss.is_some() {
                fields.push(pair("isOpenTpslOrder", "true"));
            }
            for (prefix, trigger, trigger_type) in [
                ("tp", order.take_profit, order.tp_trigger_type),
                ("sl", order.stop_loss, order.sl_trigger_type),
            ] {
                let Some(trigger) = trigger else {
                    continue;
                };
                let trigger = num::adjust(trigger, tick);
                let opposite = order.side.opposite();
                let leg_price = num::adjust(
                    trigger
                        * if opposite == OrderSide::Buy {
                            1.005
                        } else {
                            0.995
                        },
                    tick,
                );
                if !positive(leg_price) || !positive(trigger) {
                    return Err("ApeX Omni bracket price rounds to zero".into());
                }
                let leg_id = generated_id(&self.account.account_id, cx.now, issued)?;
                let signature = sign::contract_signature(
                    seed,
                    &leg_id,
                    &self.account.account_id,
                    config.pair_id,
                    &size,
                    &num::format(leg_price),
                    &self.account.maker_fee,
                    &self.account.taker_fee,
                    opposite == OrderSide::Buy,
                )?;
                fields.push(pair(
                    if prefix == "tp" {
                        "isSetOpenTp"
                    } else {
                        "isSetOpenSl"
                    },
                    "true",
                ));
                let trigger_kind = if prefix == "tp" {
                    if trigger_type == Some(StopTriggerType::IndexPrice) {
                        "INDEX"
                    } else {
                        "MARKET"
                    }
                } else if trigger_type == Some(StopTriggerType::LastPrice) {
                    "MARKET"
                } else {
                    "INDEX"
                };
                for (key, value) in [
                    ("ClientOrderId", leg_id),
                    ("Expiration", num::format(expiration)),
                    (
                        "LimitFee",
                        sign::limit_fee(&num::format(leg_price), &size, &self.account.taker_fee)?,
                    ),
                    ("Price", num::format(leg_price)),
                    ("Side", side(opposite).into()),
                    ("Signature", signature),
                    ("Size", size.clone()),
                    ("TriggerPrice", num::format(trigger)),
                    ("TriggerPriceType", trigger_kind.into()),
                ] {
                    fields.push(pair(&format!("{prefix}{key}"), value));
                }
            }
            steps.push(Step::Write(
                sign::Request::raw("/v3/order", sign::query(&fields)),
                Accepted::Place,
            ));
        }
        Ok(steps)
    }

    pub(crate) fn trading_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Command,
    ) -> Result<(), Command> {
        let (command, steps, placements) = match command {
            Command::PlaceOrders { command, orders } => {
                if let Err(error) = validate(&orders, cx) {
                    cx.log(LogLevel::Error, &error);
                    cx.done(command, Err(error));
                    return Ok(());
                }
                (command, vec![Step::Build(orders)], true)
            }
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => {
                if price.is_none() && amount.is_none() {
                    cx.done(
                        command,
                        Err("ApeX Omni amendment requires a price or amount".into()),
                    );
                    return Ok(());
                }
                (command, vec![Step::Amend(id, price, amount)], true)
            }
            Command::CancelOrders { command, ids } => {
                let mut unique = HashSet::new();
                let mut steps = vec![];
                for id in ids {
                    let id = parent_id(&id).to_owned();
                    if !valid_id(&id) {
                        let error = "ApeX Omni cancel requires an order ID".to_owned();
                        cx.log(LogLevel::Error, &error);
                        cx.done(command, Err(error));
                        return Ok(());
                    }
                    if unique.insert(id.clone()) {
                        if unique.len() > 1 && (unique.len() - 1) % 10 == 0 {
                            steps.push(Step::Delay);
                        }
                        steps.push(Step::Write(
                            sign::Request::form("/v3/delete-order", vec![pair("id", id.clone())]),
                            Accepted::Cancel(vec![id]),
                        ));
                    }
                }
                (command, steps, false)
            }
            Command::CancelSymbolOrders { command, symbol } => {
                let Some(market) = cx.store.market(&symbol) else {
                    cx.done(command, Err(format!("ApeX Omni market {symbol} not found")));
                    return Ok(());
                };
                (
                    command,
                    vec![Step::Write(
                        sign::Request::form(
                            "/v3/delete-open-orders",
                            vec![pair("symbol", market.id.clone())],
                        ),
                        Accepted::CancelSymbol(symbol),
                    )],
                    false,
                )
            }
            Command::SetLeverage {
                command,
                symbol,
                leverage,
            } => {
                let Some(market) = cx.store.market(&symbol) else {
                    cx.done(command, Err(format!("ApeX Omni market {symbol} not found")));
                    return Ok(());
                };
                if !positive(leverage) {
                    cx.done(
                        command,
                        Err("ApeX Omni leverage must be finite and positive".into()),
                    );
                    return Ok(());
                }
                let leverage =
                    leverage.clamp(market.limits.leverage.min, market.limits.leverage.max);
                (
                    command,
                    vec![Step::Write(
                        sign::Request::multipart(
                            "/v3/set-initial-margin-rate",
                            vec![
                                pair("symbol", market.id.clone()),
                                pair("token", market.quote.clone()),
                                pair("initialMarginRate", format!("{:.5}", 1.0 / leverage)),
                            ],
                        ),
                        Accepted::Leverage(symbol, leverage),
                    )],
                    false,
                )
            }
            Command::SetMarginType {
                command, isolated, ..
            } => {
                cx.done(
                    command,
                    if isolated {
                        Err("ApeX Omni supports cross margin only".into())
                    } else {
                        Ok("null".into())
                    },
                );
                return Ok(());
            }
            Command::SetPositionMode { command, hedged } => {
                cx.done(
                    command,
                    if hedged {
                        Ok("null".into())
                    } else {
                        Err("ApeX Omni supports hedged position mode only".into())
                    },
                );
                return Ok(());
            }
            Command::FetchLeverage { command, symbol } => {
                let result = cx
                    .store
                    .setting(&symbol)
                    .map(|s| num::format(s.leverage))
                    .ok_or_else(|| "ApeX Omni leverage is unavailable".into());
                cx.done(command, result);
                return Ok(());
            }
            command => return Err(command),
        };
        if self.options.public_data_only
            || self.options.key.as_deref().is_none_or(str::is_empty)
            || self.options.secret.as_deref().is_none_or(str::is_empty)
            || self.options.passphrase.as_deref().is_none_or(str::is_empty)
        {
            let error = "ApeX Omni private commands require API credentials".to_owned();
            cx.log(LogLevel::Error, &error);
            cx.done(command, Err(error));
            return Ok(());
        }
        self.trading.queue.push_back(Job {
            command,
            steps: steps.into(),
            ids: vec![],
            placements,
        });
        self.drive_trading(cx);
        Ok(())
    }

    fn drive_trading(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading.waiting || self.trading.timer.is_some() {
            return;
        }
        loop {
            if self.trading.active.is_none() {
                self.trading.active = self.trading.queue.pop_front();
            }
            let Some(mut job) = self.trading.active.take() else {
                return;
            };
            let Some(step) = job.steps.pop_front() else {
                let result = if job.placements {
                    serde_json::to_string(&job.ids).unwrap()
                } else {
                    "[]".into()
                };
                cx.done(job.command, Ok(result));
                continue;
            };
            match step {
                Step::Amend(id, price, amount) => {
                    let result = replacement(cx, &id, price, amount).and_then(|order| {
                        validate(std::slice::from_ref(&order), cx)?;
                        let mut issued = cx
                            .store
                            .orders()
                            .iter()
                            .filter_map(|o| o.client_order_id.clone())
                            .collect();
                        let prepared = self.prepare_order(cx, &order, &mut issued)?;
                        let parent = cx.store.order(parent_id(&id)).unwrap();
                        Ok(Amendment {
                            id: parent.id.clone(),
                            order,
                            total_amount: parent.amount,
                            baseline_filled: parent.filled,
                            filled: parent.filled,
                            fill_total: 0.0,
                            fills: Vec::new(),
                            reported: parent.filled,
                            reported_at: cx.now,
                            overflow: false,
                            terminal: false,
                            default_amount: amount.is_none(),
                            started: cx.now,
                            prepared,
                        })
                    });
                    let amendment = match result {
                        Ok(amendment) => amendment,
                        Err(error) => {
                            cx.log(LogLevel::Error, &error);
                            cx.done(job.command, Err(error));
                            continue;
                        }
                    };
                    job.steps.push_front(Step::Write(
                        sign::Request::form(
                            "/v3/delete-order",
                            vec![pair("id", amendment.id.clone())],
                        ),
                        Accepted::AmendCancel,
                    ));
                    self.trading.amendment = Some(amendment);
                    self.trading.active = Some(job);
                }
                Step::Build(orders) => {
                    let mut prepared = Vec::new();
                    let mut issued: HashSet<_> = cx
                        .store
                        .orders()
                        .iter()
                        .filter_map(|o| o.client_order_id.clone())
                        .chain(orders.iter().filter_map(|o| o.client_order_id.clone()))
                        .collect();
                    let result = orders.iter().try_for_each(|o| {
                        self.prepare_order(cx, o, &mut issued)
                            .map(|steps| prepared.extend(steps))
                    });
                    if let Err(error) = result {
                        cx.log(LogLevel::Error, &error);
                        cx.done(job.command, Err(error));
                        continue;
                    }
                    for step in prepared.into_iter().rev() {
                        job.steps.push_front(step);
                    }
                    self.trading.active = Some(job);
                }
                Step::Write(request, accepted) => {
                    let command = job.command;
                    self.trading.active = Some(job);
                    self.trading.waiting = true;
                    if let Err(error) = self.signed(
                        cx,
                        request,
                        Pending::Trade(Request { command, accepted }),
                        None,
                    ) {
                        self.fail_trading(cx, &error);
                    }
                    return;
                }
                Step::Delay => {
                    self.trading.active = Some(job);
                    self.trading.timer = Some(cx.set_timer(1000.0));
                    return;
                }
            }
        }
    }

    fn fail_trading(&mut self, cx: &mut Cx<'_, Pending>, error: &str) {
        self.trading.waiting = false;
        self.trading.amendment = None;
        if let Some(job) = self.trading.active.take() {
            cx.log(LogLevel::Error, error);
            cx.done(job.command, Err(error.into()));
        }
        self.drive_trading(cx);
    }
    pub(crate) fn trading_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        _request: Request,
        error: &str,
    ) {
        self.fail_trading(cx, error);
    }
    pub(crate) fn trading_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.trading.timer == Some(timer) {
            self.trading.timer = None;
            self.drive_trading(cx);
            true
        } else {
            false
        }
    }
    pub(crate) fn trading_order_progress(
        &mut self,
        id: &str,
        filled: f64,
        terminal: bool,
        timestamp: f64,
    ) {
        if let Some(amendment) = self.trading.amendment.as_mut()
            && amendment.id == id
            && filled.is_finite()
            && filled >= 0.0
        {
            if filled > amendment.reported && timestamp.is_finite() {
                amendment.reported = filled;
                amendment.reported_at = timestamp;
            }
            amendment.filled = amendment.filled.max(filled);
            amendment.observe();
            amendment.terminal |= terminal;
        }
    }
    pub(crate) fn trading_fill_progress(&mut self, id: &str, amount: f64, timestamp: f64) {
        if let Some(amendment) = self.trading.amendment.as_mut()
            && amendment.id == id
            && positive(amount)
            && timestamp.is_finite()
            && timestamp >= amendment.started
        {
            if amendment.fills.len() == 4096 {
                amendment.overflow = true;
                return;
            }
            amendment.fill_total += amount;
            amendment.fills.push((timestamp, amount));
            // Only executions after a cumulative report's watermark add to it.
            amendment.observe();
        }
    }
    pub(crate) fn trading_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if !self
            .trading
            .active
            .as_ref()
            .is_some_and(|job| job.command == request.command)
        {
            return;
        }
        #[derive(Deserialize)]
        struct Envelope {
            data: Option<Data>,
        }
        #[derive(Deserialize)]
        struct Data {
            id: String,
        }
        match request.accepted {
            Accepted::Place => {
                let parsed = serde_json::from_str::<Envelope>(response.body)
                    .ok()
                    .and_then(|e| e.data)
                    .filter(|d| !d.id.is_empty());
                let Some(data) = parsed else {
                    self.fail_trading(cx, "ApeX Omni placement response has no order ID");
                    return;
                };
                if let Some(job) = self.trading.active.as_mut() {
                    job.ids.push(data.id);
                }
                self.heal_orders(cx);
            }
            Accepted::Cancel(ids) => self.note_cancel(cx, &ids),
            Accepted::AmendCancel => {
                let Some(mut amendment) = self.trading.amendment.take() else {
                    self.fail_trading(cx, "ApeX Omni amendment state is unavailable");
                    return;
                };
                if let Some(parent) = cx.store.order(&amendment.id) {
                    amendment.filled = amendment.filled.max(parent.filled);
                    amendment.terminal |=
                        parent.remaining <= 0.0 || parent.status == OrderStatus::Closed;
                }
                self.note_cancel(cx, &[amendment.id]);
                if amendment.overflow {
                    self.fail_trading(cx, "ApeX Omni amendment exceeded the observed fill limit");
                    return;
                }
                if amendment.default_amount {
                    amendment.order.amount -=
                        (amendment.filled - amendment.baseline_filled).max(0.0);
                }
                if !amendment.terminal
                    && amendment.filled < amendment.total_amount
                    && amendment.order.amount > 0.0
                {
                    let prepared = if amendment.default_amount
                        && amendment.filled > amendment.baseline_filled
                    {
                        validate(std::slice::from_ref(&amendment.order), cx).and_then(|()| {
                            let mut issued = cx
                                .store
                                .orders()
                                .iter()
                                .filter_map(|o| o.client_order_id.clone())
                                .collect();
                            self.prepare_order(cx, &amendment.order, &mut issued)
                        })
                    } else {
                        Ok(amendment.prepared)
                    };
                    match prepared {
                        Ok(prepared) => {
                            if let Some(job) = self.trading.active.as_mut() {
                                job.steps.extend(prepared);
                            }
                        }
                        Err(error) => {
                            self.fail_trading(cx, &error);
                            return;
                        }
                    }
                }
            }
            Accepted::CancelSymbol(symbol) => {
                let ids = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.symbol == symbol)
                    .map(|o| parent_id(&o.id).to_owned())
                    .collect::<Vec<_>>();
                self.note_cancel(cx, &ids);
            }
            Accepted::Leverage(symbol, leverage) => {
                self.account.note_leverage(&symbol, leverage, cx.now);
                if let Some(index) = cx.store.market_index(&symbol) {
                    let mut setting = cx.store.setting(&symbol).unwrap_or_default();
                    setting.leverage = leverage;
                    setting.buy_leverage = Some(leverage);
                    setting.sell_leverage = Some(leverage);
                    setting.margin_type = Some(MarginType::Cross);
                    cx.store.set_settings([(index, setting)]);
                }
                cx.store.update_positions(|position| {
                    if position.symbol == symbol {
                        position.leverage = leverage;
                    }
                });
                self.refresh_positions(cx);
            }
        }
        self.trading.waiting = false;
        self.drive_trading(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_ids_retry_entropy_collisions_before_any_write() {
        let mut issued = HashSet::new();
        let first = mint_id("account", 1.0, &mut issued, || Ok("first".into())).unwrap();
        let mut draws = ["first", "second"].into_iter();
        let second = mint_id("account", 1.0, &mut issued, || {
            Ok(draws.next().unwrap().into())
        })
        .unwrap();
        assert_ne!(first, second);
        assert_eq!(issued.len(), 2);
        assert!(mint_id("account", 1.0, &mut issued, || Ok("first".into())).is_err());
        assert_eq!(issued.len(), 2);
    }
}
