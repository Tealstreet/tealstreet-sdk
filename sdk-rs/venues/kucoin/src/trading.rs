//! Preflighted write plans; a lost response never resends a mutation.
use crate::wire::{Envelope, Number, not_sent, write_failure};
use crate::{Kucoin, Pending, api_symbol, encode};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::*;
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Method, Outcome, TimerId, VenueError, num,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone)]
enum Call {
    Place,
    PlaceAfterCancel { id: String, filled: f64 },
    Cancel { id: String, fallback: bool },
    ConfirmCancel(Box<Replacement>),
    Write,
    Leverage { symbol: String, leverage: f64 },
    Margin { symbol: String, isolated: bool },
    CheckMode { hedged: bool },
    Mode { hedged: bool },
}
#[derive(Clone)]
struct Replacement {
    id: String,
    options: PlaceOrderOpts,
    filled: f64,
    polls: u8,
    read_retries: u8,
}
#[derive(Clone)]
struct Step {
    receipt: Option<OrderIdentityReceipt>,
    method: Method,
    path: String,
    body: Option<String>,
    call: Call,
}
pub struct Request {
    command: CommandId,
    step: Step,
    retried: bool,
    position_open: bool,
    position_invalid: bool,
}
impl Request {
    pub(crate) fn credential_role(&self) -> safe_cex_engine::credential_selection::CredentialRole {
        use safe_cex_engine::credential_selection::CredentialRole;
        match self.step.call {
            Call::ConfirmCancel(_) | Call::CheckMode { .. } => CredentialRole::Read { scope: None },
            _ => CredentialRole::Mutation,
        }
    }
}
struct Job {
    input: Option<Command>,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    deadline: f64,
    error: Option<VenueError>,
    keep_going: bool,
    metadata_started: HashSet<String>,
    /// A write took effect, so a later failure cannot mean nothing happened.
    effective: bool,
}
impl Job {
    /// Keeps the first failure's text. An Unknown leg makes the whole command
    /// Unknown with its details; legs that disagree otherwise leave it unset.
    fn record(&mut self, error: VenueError) {
        match &mut self.error {
            None => self.error = Some(error),
            Some(first) if first.outcome == Some(Outcome::Unknown) => {}
            Some(first) if error.outcome == Some(Outcome::Unknown) => {
                first.outcome = error.outcome;
                first.kind = error.kind;
                first.venue_code = error.venue_code;
                first.venue_message = error.venue_message;
            }
            Some(first) if first.outcome != error.outcome => first.outcome = None,
            Some(_) => {}
        }
    }
}
#[derive(Default)]
pub struct Trading {
    jobs: HashMap<CommandId, Job>,
    queue: VecDeque<CommandId>,
    active: bool,
    timer: Option<TimerId>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Placement {
    symbol: String,
    #[serde(rename = "type")]
    kind: &'static str,
    side: &'static str,
    client_oid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    leverage: Option<f64>,
    reduce_only: bool,
    margin_mode: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    position_side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_in_force: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    post_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    close_order: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_price_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_stop_up_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_stop_down_price: Option<String>,
}

fn advanced(kind: OrderType) -> bool {
    matches!(kind, OrderType::StopLoss | OrderType::TakeProfit)
}
fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}
fn preflight(orders: &[PlaceOrderOpts]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for order in orders {
        if order.bybit_bbo.is_some() || order.bybit_slippage_tolerance_ticks.is_some() {
            return Err("Bybit BBO and TickSize controls are not supported by KuCoin".into());
        }
        if order.max_slippage_pct.is_some() {
            return Err("KuCoin protected market-order slippage is not supported".into());
        }
        if let Some(id) = &order.client_order_id {
            if id.is_empty()
                || id.len() > 40
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                return Err("KuCoin clientOrderId must match /^[A-Za-z0-9_-]{1,40}$/".into());
            }
            if !matches!(order.kind, OrderType::Limit | OrderType::Market)
                || order.trigger_price.is_some()
                || order.stop_loss.is_some()
                || order.take_profit.is_some()
            {
                return Err("KuCoin clientOrderId requires a plain Limit/Market order".into());
            }
            if !seen.insert(id) {
                return Err("KuCoin duplicate clientOrderId in batch".into());
            }
        }
        if !matches!(
            order.kind,
            OrderType::Limit | OrderType::Market | OrderType::StopLoss | OrderType::TakeProfit
        ) {
            return Err("Unsupported KuCoin order type".into());
        }
        if order.trigger_price.is_some() && !advanced(order.kind) {
            return Err("KuCoin opening trigger orders are not supported".into());
        }
        if !order.amount.is_finite()
            || order.amount < 0.0
            || (order.amount == 0.0 && !advanced(order.kind))
        {
            return Err(
                "KuCoin requires positive native lots (zero only for full-position stops)".into(),
            );
        }
        if order.kind == OrderType::Limit && !order.price.is_some_and(positive) {
            return Err("KuCoin limit price must be positive".into());
        }
        if advanced(order.kind) && !order.trigger_price.or(order.price).is_some_and(positive) {
            return Err("KuCoin stop price must be positive".into());
        }
        if advanced(order.kind) && (order.stop_loss.is_some() || order.take_profit.is_some()) {
            return Err("KuCoin standalone stops cannot carry attached brackets".into());
        }
        if order.stop_loss.is_some_and(|n| !positive(n))
            || order.take_profit.is_some_and(|n| !positive(n))
        {
            return Err("KuCoin attached stop prices must be positive".into());
        }
        if order.kind != OrderType::Limit && order.time_in_force == Some(TimeInForce::PostOnly) {
            return Err("KuCoin post-only requires a limit order".into());
        }
        if order.take_profit.is_some()
            && order.stop_loss.is_some()
            && order.tp_trigger_type.unwrap_or(StopTriggerType::LastPrice)
                != order.sl_trigger_type.unwrap_or(StopTriggerType::LastPrice)
        {
            return Err("KuCoin attached TP/SL must use the same trigger source".into());
        }
    }
    Ok(())
}
fn trigger(value: Option<StopTriggerType>) -> &'static str {
    match value {
        Some(StopTriggerType::MarkPrice) => "MP",
        Some(StopTriggerType::IndexPrice) => "IP",
        _ => "TP",
    }
}
fn post(path: &str, body: String, call: Call) -> Step {
    Step {
        receipt: None,
        method: Method::Post,
        path: path.into(),
        body: Some(body),
        call,
    }
}
fn cancel(id: &str, symbol: &str) -> Step {
    Step {
        receipt: None,
        method: Method::Delete,
        path: if symbol.is_empty() {
            format!("/api/v1/orders/{}", encode(id))
        } else {
            format!("/api/v1/orders/{}?symbol={}", encode(id), encode(symbol))
        },
        body: None,
        call: Call::Cancel {
            id: id.into(),
            fallback: false,
        },
    }
}
fn confirm_cancel(replacement: Replacement, symbol: &str) -> Step {
    Step {
        receipt: None,
        method: Method::Get,
        path: format!(
            "/api/v1/orders/{}?symbol={}",
            encode(&replacement.id),
            encode(symbol)
        ),
        body: None,
        call: Call::ConfirmCancel(Box::new(replacement)),
    }
}
fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("typed KuCoin request")
}

impl Kucoin {
    fn format_placement(
        &mut self,
        cx: &Cx<'_, Pending>,
        order: &PlaceOrderOpts,
    ) -> Result<Step, String> {
        let market = cx
            .store
            .market(&order.symbol)
            .ok_or_else(|| format!("Market not found: {}", order.symbol))?;
        let setting = cx
            .store
            .setting(&order.symbol)
            .ok_or("KuCoin account settings unavailable")?;
        for price in [
            (order.kind == OrderType::Limit)
                .then_some(order.price)
                .flatten(),
            advanced(order.kind)
                .then_some(order.trigger_price.or(order.price))
                .flatten(),
            order.stop_loss,
            order.take_profit,
        ]
        .into_iter()
        .flatten()
        {
            if !positive(num::adjust(price, market.precision.price)) {
                return Err("KuCoin price rounds below the market tick".into());
            }
        }
        let amount = num::adjust(order.amount, market.precision.amount);
        if order.amount > 0.0
            && (!positive(amount)
                || amount < market.limits.amount.min
                || amount > market.limits.amount.max)
        {
            return Err("KuCoin native lots are outside market limits".into());
        }
        let is_stop = advanced(order.kind);
        let reduce = order.reduce_only.unwrap_or(is_stop);
        let closing = reduce || is_stop;
        let long = (order.side == OrderSide::Buy) != closing;
        let margin = setting
            .margin_type
            .ok_or("KuCoin margin mode unavailable")?;
        let leverage = if margin == MarginType::Isolated {
            cx.store
                .positions()
                .iter()
                .find(|p| {
                    p.symbol == order.symbol
                        && p.side
                            == if long {
                                PositionSide::Long
                            } else {
                                PositionSide::Short
                            }
                        && p.margin_type == Some(MarginType::Isolated)
                })
                .map(|p| p.leverage)
                .filter(|n| positive(*n))
                .unwrap_or(setting.leverage)
        } else {
            setting.leverage
        };
        let oid = match &order.client_order_id {
            Some(id) => id.clone(),
            None => safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))?,
        };
        let price = |n: f64| num::format(num::adjust(n, market.precision.price));
        let (up, down) = if order.side == OrderSide::Buy {
            (order.take_profit, order.stop_loss)
        } else {
            (order.stop_loss, order.take_profit)
        };
        let has_attached = up.is_some() || down.is_some();
        let payload = Placement {
            symbol: market.id.clone(),
            kind: if order.kind == OrderType::Market || is_stop {
                "market"
            } else {
                "limit"
            },
            side: if order.side == OrderSide::Buy {
                "buy"
            } else {
                "sell"
            },
            client_oid: oid,
            price: (!is_stop && order.kind == OrderType::Limit)
                .then(|| price(order.price.unwrap())),
            size: (order.amount > 0.0).then(|| num::format(amount)),
            leverage: (!is_stop).then_some(leverage.max(1.0)),
            reduce_only: reduce,
            margin_mode: match margin {
                MarginType::Cross => "CROSS",
                MarginType::Isolated => "ISOLATED",
            },
            position_side: cx
                .store
                .hedged()
                .then_some(if long { "LONG" } else { "SHORT" }),
            time_in_force: (!is_stop && order.kind == OrderType::Limit).then_some(
                match order.time_in_force {
                    Some(TimeInForce::ImmediateOrCancel) => "IOC",
                    Some(TimeInForce::FillOrKill) => "FOK",
                    _ => "GTC",
                },
            ),
            post_only: (order.time_in_force == Some(TimeInForce::PostOnly)).then_some(true),
            close_order: (is_stop && order.amount == 0.0).then_some(true),
            stop: is_stop.then_some(match (order.kind, order.side) {
                (OrderType::StopLoss, OrderSide::Buy)
                | (OrderType::TakeProfit, OrderSide::Sell) => "up",
                _ => "down",
            }),
            stop_price: is_stop.then(|| price(order.trigger_price.or(order.price).unwrap())),
            stop_price_type: if is_stop {
                Some(trigger(if order.kind == OrderType::StopLoss {
                    order.sl_trigger_type.or(order.trigger_type)
                } else {
                    order.tp_trigger_type.or(order.trigger_type)
                }))
            } else if has_attached {
                Some(trigger(order.tp_trigger_type.or(order.sl_trigger_type)))
            } else {
                None
            },
            trigger_stop_up_price: up.map(price),
            trigger_stop_down_price: down.map(price),
        };
        let receipt = OrderIdentityReceipt {
            symbol: order.symbol.clone(),
            kind: OrderIdentityKind::ClientOrderId,
            identity: payload.client_oid.clone(),
        };
        let mut step = post(
            if has_attached {
                "/api/v1/st-orders"
            } else {
                "/api/v1/orders"
            },
            json(&payload),
            Call::Place,
        );
        step.receipt = Some(receipt);
        Ok(step)
    }

    fn replacement(
        &self,
        cx: &Cx<'_, Pending>,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<(Order, PlaceOrderOpts), String> {
        let order = cx
            .store
            .order(id)
            .ok_or_else(|| format!("Order not found: {id}"))?;
        let parent_id = id.split("__").next().unwrap();
        let parent = cx.store.order(parent_id).unwrap_or(order);
        if id.contains("__") && parent.id == order.id {
            return Err("KuCoin attached stop parent unavailable".into());
        }
        if !self.account_order_complete(&parent.id) {
            return Err("KuCoin order metadata unavailable; wait for reconciliation".into());
        }
        let market = cx
            .store
            .market(&parent.symbol)
            .ok_or("KuCoin order market unavailable")?;
        let placement = self.account_placement(parent_id);
        let mut sl = parent
            .stop_loss
            .or(placement.and_then(|order| order.stop_loss));
        let mut tp = parent
            .take_profit
            .or(placement.and_then(|order| order.take_profit));
        let mut sl_trigger = parent
            .trigger_type
            .or(placement.and_then(|order| order.trigger_type));
        let mut tp_trigger = sl_trigger;
        for related in cx
            .store
            .orders()
            .iter()
            .filter(|o| o.id.starts_with(&format!("{parent_id}__")))
        {
            if related.id.ends_with("__stop_loss") {
                sl = Some(related.price);
                sl_trigger = related.trigger_type;
            }
            if related.id.ends_with("__take_profit") {
                tp = Some(related.price);
                tp_trigger = related.trigger_type;
            }
        }
        let virtual_stop = id.ends_with("__stop_loss") || id.ends_with("__take_profit");
        if id.ends_with("__stop_loss") {
            sl = price.or(sl);
        }
        if id.ends_with("__take_profit") {
            tp = price.or(tp);
        }
        if virtual_stop && amount.is_some() {
            return Err("KuCoin attached stop size is controlled by its parent".into());
        }
        let native = parent
            .extra
            .as_ref()
            .and_then(|e| e.remaining_native)
            .unwrap_or(parent.remaining / market.contract_size.unwrap_or(1.0));
        if !positive(native) && parent.close != Some(true) {
            return Err("KuCoin replacement has no remaining native lots".into());
        }
        let opts = PlaceOrderOpts {
            symbol: parent.symbol.clone(),
            kind: parent.kind,
            side: parent.side,
            amount: if virtual_stop {
                native
            } else {
                amount.unwrap_or(native)
            },
            price: if virtual_stop {
                Some(parent.price)
            } else {
                price.or(Some(parent.price))
            },
            trigger_price: None,
            max_slippage_pct: None,
            stp_mode: None,
            bybit_bbo: None,
            bybit_slippage_tolerance_ticks: None,
            implied_volatility: None,
            stop_loss: sl,
            take_profit: tp,
            reduce_only: Some(parent.reduce_only),
            time_in_force: parent.time_in_force,
            trigger_type: parent.trigger_type,
            tp_trigger_type: tp_trigger,
            sl_trigger_type: sl_trigger,
            client_order_id: None,
            close: parent.close,
        };
        preflight(std::slice::from_ref(&opts))?;
        Ok((parent.clone(), opts))
    }

    fn plan_trading(
        &mut self,
        cx: &Cx<'_, Pending>,
        input: Command,
    ) -> Result<VecDeque<Step>, String> {
        let mut steps = VecDeque::new();
        match input {
            Command::PlaceOrders { orders, .. } => {
                preflight(&orders)?;
                for order in orders {
                    steps.push_back(self.format_placement(cx, &order)?);
                }
            }
            Command::AmendOrder {
                id, price, amount, ..
            } => {
                let (parent, opts) = self.replacement(cx, &id, price, amount)?;
                self.format_placement(cx, &opts)?;
                steps.push_back(cancel(&parent.id, &api_symbol(cx.store, &parent.symbol)));
                let filled = parent
                    .extra
                    .as_ref()
                    .and_then(|extra| extra.filled_in_placement_units)
                    .unwrap_or(
                        parent.filled / crate::account::contract_size(cx.store, &parent.symbol),
                    );
                steps.push_back(confirm_cancel(
                    Replacement {
                        id: parent.id,
                        options: opts,
                        filled,
                        polls: 0,
                        read_retries: 0,
                    },
                    &api_symbol(cx.store, &parent.symbol),
                ));
            }
            Command::CancelOrders { ids, .. } => {
                let mut seen = HashSet::new();
                for id in ids {
                    let parent = id.split("__").next().unwrap();
                    if !seen.insert(parent.to_owned()) {
                        continue;
                    }
                    if parent.is_empty() {
                        return Err(
                            "KuCoin cancellation requires a nonempty native order ID".into()
                        );
                    }
                    let symbol = cx
                        .store
                        .order(&id)
                        .or_else(|| cx.store.order(parent))
                        .map(|order| api_symbol(cx.store, &order.symbol))
                        .unwrap_or_default();
                    steps.push_back(cancel(parent, &symbol));
                }
            }
            Command::CancelSymbolOrders { symbol, .. } => {
                steps.push_back(Step {
                    receipt: None,
                    method: Method::Delete,
                    path: if symbol.is_empty() {
                        "/api/v3/orders".into()
                    } else {
                        format!(
                            "/api/v3/orders?symbol={}",
                            encode(&api_symbol(cx.store, &symbol))
                        )
                    },
                    body: None,
                    call: Call::Write,
                });
                if symbol.is_empty() {
                    steps.push_back(Step {
                        receipt: None,
                        method: Method::Delete,
                        path: "/api/v1/stopOrders".into(),
                        body: None,
                        call: Call::Write,
                    });
                }
            }
            Command::SetLeverage {
                symbol, leverage, ..
            } => {
                let market = cx
                    .store
                    .market(&symbol)
                    .ok_or("KuCoin market unavailable")?;
                if !positive(leverage) {
                    return Err("KuCoin leverage must be positive".into());
                }
                if cx.store.setting(&symbol).and_then(|s| s.margin_type) != Some(MarginType::Cross)
                {
                    return Err("KuCoin only exposes cross-margin leverage changes".into());
                }
                let leverage = num::clamp_leverage(
                    leverage,
                    market.limits.leverage.min,
                    market.limits.leverage.max,
                )?;
                #[derive(Serialize)]
                struct Body<'a> {
                    symbol: &'a str,
                    leverage: String,
                }
                steps.push_back(post(
                    "/api/v2/changeCrossUserLeverage",
                    json(&Body {
                        symbol: &market.id,
                        leverage: num::format(leverage),
                    }),
                    Call::Leverage { symbol, leverage },
                ));
            }
            Command::SetMarginType {
                symbol, isolated, ..
            } => {
                if symbol.is_empty() {
                    return Err("Symbol is required".into());
                }
                let market = cx
                    .store
                    .market(&symbol)
                    .ok_or("KuCoin market unavailable")?;
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Body<'a> {
                    symbol: &'a str,
                    margin_mode: &'a str,
                }
                steps.push_back(post(
                    "/api/v2/position/changeMarginMode",
                    json(&Body {
                        symbol: &market.id,
                        margin_mode: if isolated { "ISOLATED" } else { "CROSS" },
                    }),
                    Call::Margin { symbol, isolated },
                ));
            }
            Command::SetPositionMode { hedged, .. } => steps.push_back(Step {
                receipt: None,
                method: Method::Get,
                path: "/api/v1/positions".into(),
                body: None,
                call: Call::CheckMode { hedged },
            }),
            _ => return Err("Unsupported KuCoin trading command".into()),
        }
        Ok(steps)
    }

    pub(crate) fn trading_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: CommandId,
        input: Command,
    ) {
        let validation = match &input {
            Command::PlaceOrders { orders, .. } => preflight(orders),
            Command::AmendOrder {
                id, price, amount, ..
            } => self.replacement(cx, id, *price, *amount).map(|_| ()),
            _ => Ok(()),
        };
        if let Err(error) = validation {
            let Ok(context) = cx.admit_http_command(crate::local_mutation_command_policy()) else {
                return;
            };
            let _ = cx.fail_context(&context, not_sent(error));
            return;
        }
        if self.credentials.is_none() {
            let Ok(context) = cx.admit_http_command(crate::local_mutation_command_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                not_sent("KuCoin credentials are required").kind(ErrorKind::AuthInvalid),
            );
            return;
        }
        self.trading.jobs.insert(
            id,
            Job {
                keep_going: matches!(&input, Command::CancelOrders { .. }),
                input: Some(input),
                steps: VecDeque::new(),
                ids: vec![],
                deadline: cx.now + 10_000.0,
                error: None,
                metadata_started: HashSet::new(),
                effective: false,
            },
        );
        self.trading.queue.push_back(id);
        self.drive_trading(cx);
    }

    fn metadata_symbols(&self, cx: &Cx<'_, Pending>, command: &Command) -> Vec<String> {
        match command {
            Command::PlaceOrders { orders, .. } => {
                orders.iter().map(|o| o.symbol.clone()).collect()
            }
            Command::AmendOrder { id, .. } => cx
                .store
                .order(id)
                .map(|o| vec![o.symbol.clone()])
                .unwrap_or_default(),
            Command::SetLeverage { symbol, .. } => vec![symbol.clone()],
            _ => vec![],
        }
    }
    fn drive_trading(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading.active || self.trading.timer.is_some() {
            return;
        }
        while let Some(command) = self.trading.queue.pop_front() {
            let Some(job) = self.trading.jobs.get(&command) else {
                continue;
            };
            if let Some(input) = &job.input {
                let symbols = self.metadata_symbols(cx, input);
                let waiting: HashSet<_> = symbols
                    .into_iter()
                    .filter(|s| !self.account_metadata_ready(cx, s))
                    .collect();
                if !waiting.is_empty() && cx.now < job.deadline {
                    for symbol in waiting {
                        if self
                            .trading
                            .jobs
                            .get_mut(&command)
                            .unwrap()
                            .metadata_started
                            .insert(symbol.clone())
                        {
                            self.account_metadata(cx, &symbol);
                        }
                    }
                    self.trading.queue.push_front(command);
                    self.trading.timer = Some(cx.set_timer(20.0));
                    return;
                }
                let input = self
                    .trading
                    .jobs
                    .get_mut(&command)
                    .unwrap()
                    .input
                    .take()
                    .unwrap();
                let plan = if waiting.is_empty() {
                    self.plan_trading(cx, input)
                } else {
                    Err("KuCoin account metadata unavailable".into())
                };
                let job = self.trading.jobs.get_mut(&command).unwrap();
                match plan {
                    Ok(steps) => job.steps = steps,
                    Err(error) => job.error = Some(not_sent(error)),
                }
            }
            let job = self.trading.jobs.get_mut(&command).unwrap();
            if let Some(step) = job.steps.pop_front() {
                self.trading.active = true;
                self.send_trade(
                    cx,
                    Request {
                        command,
                        step,
                        retried: false,
                        position_open: false,
                        position_invalid: false,
                    },
                );
                return;
            }
            let job = self.trading.jobs.remove(&command).unwrap();
            match job.error {
                None => cx.done(command, json(&job.ids)),
                Some(mut error) => {
                    if (job.effective || !job.ids.is_empty())
                        && matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent))
                    {
                        error.outcome = None;
                    }
                    cx.fail(command, error);
                }
            }
        }
    }

    pub(crate) fn trading_sign_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        let Some(job) = self.trading.jobs.get_mut(&request.command) else {
            return;
        };
        cx.command_error(request.command, &error.message);
        job.record(error);
        if !job.keep_going {
            job.steps.clear();
        }
        self.trading.active = false;
        self.trading.queue.push_front(request.command);
        self.trading.timer = Some(cx.set_timer(20.0));
    }

    pub(crate) fn trading_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.trading.timer != Some(timer) {
            return false;
        }
        self.trading.timer = None;
        self.drive_trading(cx);
        true
    }
    fn send_trade(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        if let Call::PlaceAfterCancel { id, filled } = &request.step.call
            && self.account_order_filled(id) > *filled
        {
            let error = "KuCoin newer fills invalidated cancellation proof";
            cx.command_error(request.command, error);
            let job = self.trading.jobs.get_mut(&request.command).unwrap();
            job.error = Some(not_sent(error));
            job.steps.clear();
            self.trading.active = false;
            self.trading.queue.push_front(request.command);
            self.trading.timer = Some(cx.set_timer(20.0));
            return;
        }
        let method = request.step.method;
        let path = request.step.path.clone();
        let body = request.step.body.clone();
        if let Some(receipt) = &request.step.receipt {
            cx.command_progress(request.command, vec![receipt.clone()]);
        }
        if matches!(request.step.call, Call::CheckMode { .. }) {
            self.signed_streamed(
                cx,
                method,
                &path,
                body,
                ArrayAt::EnvelopeField("data"),
                Pending::Trade(request),
            );
        } else {
            self.signed(cx, method, &path, body, Pending::Trade(request));
        }
    }
    pub(crate) fn trading_item(
        &mut self,
        _cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        #[derive(Deserialize)]
        struct PositionRow<'a> {
            #[serde(rename = "currentQty", borrow)]
            quantity: &'a serde_json::value::RawValue,
        }
        let parsed = (|| {
            let position: PositionRow = serde_json::from_str(item).ok()?;
            let text = position.quantity.get();
            let raw = if text.starts_with('"') {
                serde_json::from_str::<&str>(text).ok()?
            } else {
                text
            };
            raw.parse::<f64>().ok().filter(|n| n.is_finite())
        })();
        match parsed {
            Some(quantity) => request.position_open |= quantity != 0.0,
            None => request.position_invalid = true,
        }
    }
    pub(crate) fn trading_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        status: u16,
        body: &str,
    ) {
        let envelope = serde_json::from_str::<Envelope<'_>>(body);
        if let Ok(row) = &envelope
            && row.code == "400005"
            && !request.retried
        {
            request.retried = true;
            request.position_open = false;
            request.position_invalid = false;
            self.send_trade(cx, request);
            return;
        }
        if status == 404
            && let Call::Cancel {
                id,
                fallback: false,
            } = &request.step.call
        {
            let step = Step {
                receipt: None,
                method: Method::Delete,
                path: format!("/api/v1/stopOrders/{}", encode(id)),
                body: None,
                call: Call::Cancel {
                    id: id.clone(),
                    fallback: true,
                },
            };
            self.signed(
                cx,
                step.method,
                &step.path,
                None,
                Pending::Trade(Request {
                    command: request.command,
                    step: step.clone(),
                    retried: false,
                    position_open: false,
                    position_invalid: false,
                }),
            );
            return;
        }
        let mut delay = 20.0;
        let result = envelope
            .map_err(|_| format!("Invalid KuCoin response (HTTP {status})"))
            .and_then(|row| {
                if !(200..300).contains(&status) || row.code != "200000" {
                    return Err(format!(
                        "KuCoin API Error {}: {} (HTTP {status})",
                        row.code, row.msg
                    ));
                }
                match request.step.call.clone() {
                    Call::Place | Call::PlaceAfterCancel { .. } => {
                        #[derive(Deserialize)]
                        #[serde(rename_all = "camelCase")]
                        struct Created<'a> {
                            order_id: &'a str,
                        }
                        let created: Created<'_> = serde_json::from_str(
                            row.data.ok_or("Missing KuCoin order result")?.get(),
                        )
                        .map_err(|_| "Missing KuCoin order id")?;
                        if created.order_id.is_empty() {
                            return Err("Missing KuCoin order id".into());
                        }
                        if let Some(body) = request.step.body.as_deref() {
                            self.account_placed_order(cx, created.order_id, body);
                        }
                        self.trading
                            .jobs
                            .get_mut(&request.command)
                            .unwrap()
                            .ids
                            .push(created.order_id.into());
                    }
                    Call::ConfirmCancel(mut replacement) => {
                        #[derive(Deserialize)]
                        #[serde(rename_all = "camelCase")]
                        struct FinalOrder<'a> {
                            #[serde(default, alias = "orderId")]
                            id: &'a str,
                            #[serde(default)]
                            is_active: Option<bool>,
                            #[serde(default)]
                            cancel_exist: Option<bool>,
                            #[serde(default)]
                            status: &'a str,
                            #[serde(default)]
                            size: Option<Number>,
                            #[serde(default)]
                            filled_size: Option<Number>,
                            #[serde(default)]
                            deal_size: Option<Number>,
                        }
                        let final_order: FinalOrder = serde_json::from_str(
                            row.data.ok_or("Missing KuCoin cancellation proof")?.get(),
                        )
                        .map_err(|_| "Invalid KuCoin cancellation proof")?;
                        if final_order.id != replacement.id {
                            return Err("KuCoin cancellation proof has the wrong order id".into());
                        }
                        let canceled = final_order.is_active != Some(true)
                            && (matches!(final_order.status, "canceled" | "cancelled")
                                || final_order.is_active == Some(false)
                                    && final_order.cancel_exist == Some(true));
                        let filled = final_order
                            .filled_size
                            .or(final_order.deal_size)
                            .map(Number::get)
                            .ok_or("KuCoin cancellation proof missing final fills")?;
                        if filled
                            < replacement
                                .filled
                                .max(self.account_order_filled(&replacement.id))
                        {
                            return Err("KuCoin cancellation proof has stale fills".into());
                        }
                        let fully_filled = final_order.is_active == Some(false)
                            && final_order
                                .size
                                .is_some_and(|n| n.get() > 0.0 && filled >= n.get());
                        if !canceled && !fully_filled {
                            if replacement.polls >= 30 {
                                return Err("KuCoin cancellation could not be confirmed".into());
                            }
                            replacement.polls += 1;
                            replacement.read_retries = 0;
                            let symbol = api_symbol(cx.store, &replacement.options.symbol);
                            self.trading
                                .jobs
                                .get_mut(&request.command)
                                .unwrap()
                                .steps
                                .push_front(confirm_cancel(*replacement, &symbol));
                            delay = 100.0;
                            return Ok(());
                        }
                        let full_position_stop =
                            advanced(replacement.options.kind) && replacement.options.amount == 0.0;
                        if !full_position_stop {
                            replacement.options.amount -= filled - replacement.filled;
                        }
                        self.account_order_terminal(cx, &replacement.id);
                        if full_position_stop && canceled && !fully_filled
                            || !full_position_stop && replacement.options.amount > 0.0
                        {
                            let mut step = self.format_placement(cx, &replacement.options)?;
                            step.call = Call::PlaceAfterCancel {
                                id: replacement.id.clone(),
                                filled,
                            };
                            self.trading
                                .jobs
                                .get_mut(&request.command)
                                .unwrap()
                                .steps
                                .push_front(step);
                        }
                    }
                    Call::Leverage { symbol, leverage } => {
                        self.account_settings_changed();
                        cx.store.update_setting(&symbol, |s| s.leverage = leverage)
                    }
                    Call::Margin { symbol, isolated } => {
                        self.account_settings_changed();
                        cx.store.update_setting(&symbol, |s| {
                            s.margin_type = Some(if isolated {
                                MarginType::Isolated
                            } else {
                                MarginType::Cross
                            });
                            s.leverage = f64::NAN;
                        });
                    }
                    Call::Mode { hedged } => {
                        self.account_settings_changed();
                        self.account.mode_known = true;
                        cx.store.set_hedged(hedged);
                    }
                    Call::CheckMode { hedged } => {
                        if request.position_invalid {
                            return Err("Invalid KuCoin position quantity".into());
                        }
                        if request.position_open {
                            return Err("Cannot update position mode with open positions".into());
                        }
                        #[derive(Serialize)]
                        #[serde(rename_all = "camelCase")]
                        struct Mode {
                            position_mode: &'static str,
                        }
                        self.trading
                            .jobs
                            .get_mut(&request.command)
                            .unwrap()
                            .steps
                            .push_front(post(
                                "/api/v2/position/switchPositionMode",
                                json(&Mode {
                                    position_mode: if hedged { "1" } else { "0" },
                                }),
                                Call::Mode { hedged },
                            ));
                    }
                    _ => {}
                }
                Ok(())
            });
        let read = request.step.method == Method::Get;
        match &result {
            Ok(()) if !read => {
                self.trading
                    .jobs
                    .get_mut(&request.command)
                    .unwrap()
                    .effective = true;
            }
            _ => {}
        }
        if let Err(error) = result {
            if let Call::ConfirmCancel(mut replacement) = request.step.call
                && replacement.read_retries < 2
            {
                replacement.read_retries += 1;
                let symbol = api_symbol(cx.store, &replacement.options.symbol);
                self.trading
                    .jobs
                    .get_mut(&request.command)
                    .unwrap()
                    .steps
                    .push_front(confirm_cancel(*replacement, &symbol));
                self.trading.active = false;
                self.trading.queue.push_front(request.command);
                self.trading.timer = Some(cx.set_timer(100.0));
                return;
            }
            cx.command_error(request.command, &error);
            let mut error = write_failure(status, body, error);
            // Reads (cancel proof, position check) precede a write that was never sent.
            if read {
                error.outcome = Some(Outcome::NotSent);
            }
            let job = self.trading.jobs.get_mut(&request.command).unwrap();
            job.record(error);
            if !job.keep_going {
                job.steps.clear();
            }
        }
        self.trading.active = false;
        self.trading.queue.push_front(request.command);
        self.trading.timer = Some(cx.set_timer(delay));
    }
}
