//! DXtrade writes have no transport retry. A placement batch is sequential;
//! cancellation waits for every result in each bounded group of ten.
use crate::{BreakoutProp, Operation, Pending, not_sent};
use safe_cex_engine::model::{
    Market, Order, OrderSide, OrderStatus, OrderType, PlaceOrderOpts, StopTriggerType,
};
use safe_cex_engine::request_context::Operation as RequestOperation;
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Method, OrderIdentityKind, OrderIdentityReceipt, Outcome,
    Store, VenueError, num,
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};

const MAX_ACKNOWLEDGED: usize = 10_000;
const CANCEL_BATCH: usize = 10;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreparedOrder {
    #[serde(rename = "type")]
    kind: &'static str,
    instrument: String,
    quantity: f64,
    side: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_price: Option<f64>,
    position_effect: &'static str,
    tif: &'static str,
}

fn positive(value: f64, label: &str) -> Result<f64, String> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(format!("Breakout Prop {label} must be finite and positive"))
    }
}

impl PreparedOrder {
    fn prepare(opts: &PlaceOrderOpts, market: &Market) -> Result<Self, String> {
        if opts.client_order_id.is_some() {
            return Err("Breakout Prop does not support caller clientOrderId".into());
        }
        if opts.bybit_bbo.is_some()
            || opts.bybit_slippage_tolerance_ticks.is_some()
            || opts.stp_mode.is_some()
            || opts.max_slippage_pct.is_some()
        {
            return Err("Breakout Prop does not support the requested venue order control".into());
        }
        if [
            opts.trigger_type,
            opts.sl_trigger_type,
            opts.tp_trigger_type,
        ]
        .into_iter()
        .flatten()
        .any(|trigger| trigger != StopTriggerType::LastPrice)
        {
            return Err("Breakout Prop supports only last-price stop triggers".into());
        }
        let unrepresentable = match opts.kind {
            OrderType::Limit | OrderType::Market => {
                opts.trigger_price.is_some()
                    || opts.stop_loss.is_some()
                    || opts.take_profit.is_some()
            }
            OrderType::StopLoss => opts.take_profit.is_some(),
            OrderType::TakeProfit => opts.stop_loss.is_some(),
            _ => false,
        };
        if unrepresentable || opts.close == Some(true) {
            return Err(
                "Breakout Prop cannot honor attached protective or close-position controls".into(),
            );
        }
        positive(market.precision.amount, "amount step")?;
        positive(market.precision.price, "price step")?;
        let quantity = if opts.amount == 0.0 && opts.reduce_only == Some(true) {
            // DXtrade accepts explicit zero quantity with CLOSE. This is not
            // the SDK's separate close-all control, which remains unsupported.
            0.0
        } else {
            positive(
                num::adjust(positive(opts.amount, "amount")?, market.precision.amount),
                "rounded amount",
            )?
        };
        let (kind, limit_price, stop_price) = match opts.kind {
            OrderType::Limit => {
                let price = opts
                    .price
                    .ok_or("Breakout Prop limit orders require a price")?;
                (
                    "LIMIT",
                    Some(positive(
                        num::adjust(positive(price, "price")?, market.precision.price),
                        "rounded price",
                    )?),
                    None,
                )
            }
            OrderType::Market => ("MARKET", None, None),
            OrderType::StopLoss | OrderType::TakeProfit => {
                let stop = opts
                    .trigger_price
                    .or(if opts.kind == OrderType::StopLoss {
                        opts.stop_loss
                    } else {
                        opts.take_profit
                    })
                    .ok_or("Breakout Prop stop orders require a trigger price")?;
                (
                    "STOP",
                    None,
                    Some(positive(
                        num::adjust(positive(stop, "trigger price")?, market.precision.price),
                        "rounded trigger price",
                    )?),
                )
            }
            _ => return Err("Breakout Prop does not support this order type".into()),
        };
        Ok(Self {
            kind,
            instrument: market.symbol.clone(),
            quantity,
            side: match opts.side {
                OrderSide::Buy => "BUY",
                OrderSide::Sell => "SELL",
            },
            limit_price,
            stop_price,
            position_effect: if opts.reduce_only == Some(true) {
                "CLOSE"
            } else {
                "OPEN"
            },
            // The TS adapter deliberately always uses GTC, including market orders.
            tif: "GTC",
        })
    }

    fn body(&self, order_code: &str) -> Result<String, String> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Payload<'a> {
            order_code: &'a str,
            #[serde(flatten)]
            order: &'a PreparedOrder,
        }
        serde_json::to_string(&Payload {
            order_code,
            order: self,
        })
        .map_err(|_| "Unable to encode DXtrade order".into())
    }
}

fn prepare_orders(store: &Store, orders: &[PlaceOrderOpts]) -> Result<Vec<PreparedOrder>, String> {
    // This check precedes market lookup and every other batch validation.
    if orders.iter().any(|order| order.client_order_id.is_some()) {
        return Err("Breakout Prop does not support caller clientOrderId".into());
    }
    orders
        .iter()
        .map(|opts| {
            let market = store.market(&opts.symbol).ok_or("Market not found")?;
            PreparedOrder::prepare(opts, market)
        })
        .collect()
}

#[derive(Clone)]
enum Step {
    Place {
        order: PreparedOrder,
        code: String,
    },
    Cancel {
        id: String,
        code: String,
        amend_baseline: Option<Box<Order>>,
    },
    ConfirmCanceled {
        native_id: String,
        code: String,
        expected: Box<Order>,
    },
}

#[derive(Clone)]
pub(crate) struct Request {
    pub command: CommandId,
    pub method: Method,
    pub path: String,
    pub body: Option<String>,
    step: Step,
}

impl Request {
    pub(crate) fn report_progress(&self, cx: &mut Cx<'_, Pending>) {
        if let Step::Place { order, code } = &self.step {
            cx.command_progress(
                self.command,
                vec![OrderIdentityReceipt {
                    symbol: order.instrument.clone(),
                    kind: OrderIdentityKind::OrderCode,
                    identity: code.clone(),
                }],
            );
        }
    }
}

#[derive(Default)]
struct Job {
    steps: VecDeque<Step>,
    in_flight: usize,
    cancel_batch: bool,
    error: Option<VenueError>,
    /// A step took effect, so a later failure cannot mean nothing happened.
    effective: bool,
    ids: Vec<String>,
    reserved_places: usize,
    cancel_targets: Vec<String>,
}

impl Job {
    /// Keeps the first failure's text; an Unknown leg makes the whole command Unknown.
    fn record(&mut self, error: VenueError) {
        match &mut self.error {
            None => self.error = Some(error),
            Some(first) if error.outcome == Some(Outcome::Unknown) => {
                first.outcome = Some(Outcome::Unknown);
                first.kind = error.kind;
            }
            Some(_) => {}
        }
    }
}

/// A 2xx answer that does not prove what happened.
fn unproven(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::Unknown)
}

#[derive(Default)]
pub(crate) struct TradingState {
    jobs: HashMap<CommandId, Job>,
    // Native numeric ACK IDs cannot be submitted as DXtrade order codes. Keep
    // their generated client codes so cancellation works before Store sees NEW.
    acknowledged: HashMap<String, Acknowledged>,
}

struct Acknowledged {
    code: String,
    symbol: String,
    created_at: f64,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum NativeId {
    Number(u64),
    Text(String),
}
impl NativeId {
    fn text(self) -> Result<String, String> {
        match self {
            Self::Number(0) => Err("Invalid DXtrade orderId".into()),
            Self::Number(value) => Ok(value.to_string()),
            Self::Text(value)
                if value.bytes().all(|byte| byte.is_ascii_digit())
                    && value.bytes().any(|byte| byte != b'0') =>
            {
                Ok(value)
            }
            Self::Text(_) => Err("Invalid DXtrade orderId".into()),
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaceAck {
    order_id: NativeId,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct History<'a> {
    #[serde(borrow)]
    orders: Vec<HistoricalOrder<'a>>,
    #[serde(default, borrow)]
    next_page_transaction_time: Option<Cow<'a, str>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalOrder<'a> {
    #[serde(borrow)]
    account: Cow<'a, str>,
    order_id: NativeId,
    #[serde(borrow)]
    order_code: Cow<'a, str>,
    #[serde(default, borrow)]
    client_order_id: Option<Cow<'a, str>>,
    #[serde(borrow)]
    instrument: Cow<'a, str>,
    #[serde(borrow)]
    status: Cow<'a, str>,
    final_status: bool,
    #[serde(borrow)]
    legs: Vec<HistoricalLeg<'a>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalLeg<'a> {
    #[serde(borrow)]
    instrument: Cow<'a, str>,
    #[serde(borrow)]
    side: Cow<'a, str>,
    #[serde(borrow)]
    position_effect: Cow<'a, str>,
    quantity: f64,
    filled_quantity: f64,
    remaining_quantity: f64,
}

fn confirm_canceled(
    body: &str,
    account: &str,
    native_id: &str,
    code: &str,
    expected: &Order,
) -> Result<(), String> {
    let mut history: History<'_> =
        serde_json::from_str(body).map_err(|_| "Invalid DXtrade cancellation history")?;
    if history.orders.len() != 1
        || history
            .next_page_transaction_time
            .as_deref()
            .is_some_and(|next| !next.is_empty())
    {
        return Err("DXtrade cancellation history is missing or ambiguous".into());
    }
    let order = history.orders.remove(0);
    let matching_code = order.order_code == expected.id
        || order.order_code == code
        || order.client_order_id.as_deref() == Some(code)
        || expected
            .client_order_id
            .as_deref()
            .filter(|client| !client.is_empty())
            .is_some_and(|client| order.client_order_id.as_deref() == Some(client));
    if order.account != account
        || order.order_id.text()? != native_id
        || !matching_code
        || order.instrument != expected.symbol
        || !order.final_status
        || order.status != "CANCELED"
        || order.legs.len() != 1
    {
        return Err("DXtrade cancellation history does not confirm the original order".into());
    }
    let leg = &order.legs[0];
    let side = if expected.side == OrderSide::Buy {
        "BUY"
    } else {
        "SELL"
    };
    let effect = if expected.reduce_only {
        "CLOSE"
    } else {
        "OPEN"
    };
    if leg.instrument != expected.symbol
        || leg.side != side
        || leg.position_effect != effect
        || !leg.quantity.is_finite()
        || leg.quantity != expected.amount
        || !leg.filled_quantity.is_finite()
        || leg.filled_quantity != expected.filled
        || !leg.remaining_quantity.is_finite()
        || leg.remaining_quantity < 0.0
        || leg.remaining_quantity > leg.quantity
    {
        return Err(
            "DXtrade original order filled or changed; replacement was not submitted".into(),
        );
    }
    Ok(())
}

impl TradingState {
    /// Invoke only after a successfully accepted fresh account snapshot. Open
    /// orders retain routing; fast fills age out after a reconciliation grace.
    pub(crate) fn reconcile(&mut self, store: &Store, now: f64) {
        self.acknowledged.retain(|id, ack| {
            now < ack.created_at + 60_000.0
                || store.orders().iter().any(|order| {
                    order.status == OrderStatus::Open
                        && (order.id == *id
                            || order.id == ack.code
                            || order.client_order_id.as_deref() == Some(ack.code.as_str()))
                })
        });
    }
    fn reservations(&self) -> usize {
        self.jobs.values().map(|job| job.reserved_places).sum()
    }
    fn canonical_code<'a>(&'a self, store: &'a Store, id: &'a str) -> &'a str {
        store
            .order(id)
            .and_then(|order| order.client_order_id.as_deref())
            .filter(|client| !client.is_empty())
            .or_else(|| self.acknowledged.get(id).map(|ack| ack.code.as_str()))
            .unwrap_or(id)
    }
    fn cancellation_busy(&self, store: &Store, id: &str) -> bool {
        let code = self.canonical_code(store, id);
        self.jobs.values().any(|job| {
            job.cancel_targets
                .iter()
                .any(|target| target == id || self.canonical_code(store, target) == code)
        })
    }
    fn cancel_steps(&self, store: &Store, ids: Vec<String>) -> Result<VecDeque<Step>, String> {
        if ids.iter().any(|id| id.is_empty()) {
            return Err("Breakout Prop cancellation requires a nonempty order ID".into());
        }
        if ids.iter().any(|id| self.cancellation_busy(store, id)) {
            return Err(
                "Breakout Prop order cancellation or replacement is already in progress".into(),
            );
        }
        let mut seen = HashSet::new();
        Ok(ids
            .into_iter()
            .filter(|id| seen.insert(self.canonical_code(store, id).to_owned()))
            .map(|id| {
                let code = self
                    .acknowledged
                    .get(&id)
                    .map(|ack| ack.code.clone())
                    .unwrap_or_else(|| id.clone());
                Step::Cancel {
                    id,
                    code,
                    amend_baseline: None,
                }
            })
            .collect())
    }
}

impl BreakoutProp {
    pub(crate) fn on_trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) -> bool {
        let (id, job) = match command {
            Command::PlaceOrders { command, orders } => {
                // Caller identity must win even over public-only validation.
                let prepared = prepare_orders(cx.store, &orders);
                let result = prepared.and_then(|orders| {
                    if self.public_only() {
                        return Err("Breakout Prop trading requires a private account".into());
                    }
                    if self.trading.acknowledged.len() + self.trading.reservations() + orders.len()
                        > MAX_ACKNOWLEDGED
                    {
                        return Err("Breakout Prop pending order capacity exceeded".into());
                    }
                    let reserved_places = orders.len();
                    let steps = orders
                        .into_iter()
                        .map(|order| {
                            let code = format!(
                                "tealstreet-{}",
                                safe_cex_crypto::uuid::uuid_with_entropy(
                                    |bytes| cx.fill_entropy(bytes)
                                )?
                            );
                            Ok(Step::Place { order, code })
                        })
                        .collect::<Result<VecDeque<_>, String>>()?;
                    Ok(Job {
                        steps,
                        reserved_places,
                        ..Job::default()
                    })
                });
                (command, result)
            }
            Command::CancelOrders { command, ids } => (command, self.plan_cancel(cx.store, ids)),
            Command::CancelOrdersWithSymbols { command, orders } => (
                command,
                self.plan_cancel(cx.store, orders.into_iter().map(|order| order.id).collect()),
            ),
            Command::CancelSymbolOrders { command, symbol } => {
                let ids = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|order| {
                        (symbol.is_empty() || order.symbol == symbol)
                            && order.status == OrderStatus::Open
                    })
                    .map(|order| order.id.clone())
                    .chain(
                        self.trading
                            .acknowledged
                            .iter()
                            .filter(|(_, ack)| symbol.is_empty() || ack.symbol == symbol)
                            .map(|(id, _)| id.clone()),
                    )
                    .collect();
                (command, self.plan_cancel(cx.store, ids))
            }
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => (command, self.plan_amend(cx, &id, price, amount)),
            setting @ (Command::SetLeverage { .. }
            | Command::SetMarginType { .. }
            | Command::SetPositionMode { .. }
            | Command::SetStpMode { .. }
            | Command::FetchStpMode { .. }) => {
                let operation = if matches!(setting, Command::FetchStpMode { .. }) {
                    RequestOperation::Read
                } else {
                    RequestOperation::Mutation
                };
                crate::local_context::complete(
                    cx,
                    operation,
                    Err(not_sent("Breakout Prop uses fixed one-way cross accounts; this setting is unsupported")
                        .kind(ErrorKind::Unsupported)),
                );
                return true;
            }
            _ => return false,
        };
        match job {
            Ok(job) if self.trading.jobs.contains_key(&id) => {
                let _ = job;
                cx.fail(id, not_sent("Breakout Prop command is already in progress"));
            }
            Ok(job) => {
                self.trading.jobs.insert(id, job);
                self.advance_trade(cx, id);
            }
            Err(error) => {
                crate::local_context::complete(cx, RequestOperation::Mutation, Err(not_sent(error)))
            }
        }
        true
    }

    fn plan_cancel(&self, store: &Store, ids: Vec<String>) -> Result<Job, String> {
        if self.public_only() {
            return Err("Breakout Prop trading requires a private account".into());
        }
        let steps = self.trading.cancel_steps(store, ids)?;
        let cancel_targets = steps
            .iter()
            .filter_map(|step| match step {
                Step::Cancel { id, .. } => Some(self.trading.canonical_code(store, id).to_owned()),
                _ => None,
            })
            .collect();
        Ok(Job {
            steps,
            cancel_batch: true,
            cancel_targets,
            ..Job::default()
        })
    }

    fn plan_amend(
        &self,
        cx: &Cx<'_, Pending>,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<Job, String> {
        let store = &*cx.store;
        if self.public_only() {
            return Err("Breakout Prop trading requires a private account".into());
        }
        let order = store
            .order(id)
            .filter(|order| order.status == OrderStatus::Open)
            .ok_or("Breakout Prop amendment requires an open order")?;
        if self.trading.cancellation_busy(store, id) {
            return Err(
                "Breakout Prop order cancellation or replacement is already in progress".into(),
            );
        }
        let zero_close = order.reduce_only
            && order.amount == 0.0
            && order.filled == 0.0
            && order.remaining == 0.0;
        if !order.filled.is_finite()
            || order.filled != 0.0
            || !order.remaining.is_finite()
            || (order.remaining <= 0.0 && !zero_close)
        {
            return Err("Breakout Prop cannot safely replace a partially filled order".into());
        }
        let mut opts = PlaceOrderOpts {
            symbol: order.symbol.clone(),
            kind: order.kind,
            side: order.side,
            price: price.or(Some(order.price)),
            amount: amount.unwrap_or(order.amount),
            trigger_price: None,
            max_slippage_pct: None,
            stop_loss: order.stop_loss,
            take_profit: order.take_profit,
            reduce_only: Some(order.reduce_only),
            time_in_force: order.time_in_force,
            trigger_type: order.trigger_type,
            tp_trigger_type: None,
            sl_trigger_type: None,
            client_order_id: None,
            close: order.close,
            stp_mode: None,
            bybit_bbo: None,
            bybit_slippage_tolerance_ticks: None,
            implied_volatility: None,
        };
        if matches!(opts.kind, OrderType::StopLoss | OrderType::TakeProfit) {
            opts.trigger_price = opts.price;
        }
        let prepared = prepare_orders(store, &[opts])?
            .pop()
            .ok_or("Invalid replacement order")?;
        if self.trading.acknowledged.len() + self.trading.reservations() >= MAX_ACKNOWLEDGED {
            return Err("Breakout Prop pending order capacity exceeded".into());
        }
        let code = format!(
            "tealstreet-{}",
            safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))?
        );
        Ok(Job {
            steps: VecDeque::from([
                Step::Cancel {
                    id: id.into(),
                    code: self
                        .trading
                        .acknowledged
                        .get(id)
                        .map(|ack| ack.code.clone())
                        .unwrap_or_else(|| id.into()),
                    amend_baseline: Some(Box::new(order.clone())),
                },
                Step::Place {
                    order: prepared,
                    code,
                },
            ]),
            reserved_places: 1,
            cancel_targets: vec![self.trading.canonical_code(store, id).to_owned()],
            ..Job::default()
        })
    }

    fn advance_trade(&mut self, cx: &mut Cx<'_, Pending>, id: CommandId) {
        let Some(job) = self.trading.jobs.get_mut(&id) else {
            return;
        };
        if job.in_flight > 0 {
            return;
        }
        if job.error.is_some() || job.steps.is_empty() {
            let job = self.trading.jobs.remove(&id).expect("job present");
            match job.error {
                None => cx.done(id, serde_json::to_string(&job.ids).unwrap_or_default()),
                Some(mut error) => {
                    if (job.effective || !job.ids.is_empty())
                        && matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent))
                    {
                        error.outcome = None;
                    }
                    cx.fail(id, error);
                }
            }
            return;
        }
        let limit = if job.cancel_batch { CANCEL_BATCH } else { 1 };
        let steps: Vec<_> = (0..limit).filter_map(|_| job.steps.pop_front()).collect();
        job.in_flight = steps.len();
        for step in steps {
            let Some(account) = self.account_code() else {
                self.on_trade_failed_step(cx, id, not_sent("DXtrade account is not initialized"));
                continue;
            };
            let base = format!("/accounts/{}/orders", safe_cex_engine::url::encode(account));
            let request = match &step {
                Step::Place { order, code } => match order.body(code) {
                    Ok(body) => Request {
                        command: id,
                        method: Method::Post,
                        path: base,
                        body: Some(body),
                        step,
                    },
                    Err(error) => {
                        self.on_trade_failed_step(cx, id, not_sent(error));
                        continue;
                    }
                },
                Step::Cancel { code, .. } => Request {
                    command: id,
                    method: Method::Delete,
                    path: format!("{base}/{}", safe_cex_engine::url::encode(code)),
                    body: None,
                    step,
                },
                Step::ConfirmCanceled { native_id, .. } => Request {
                    command: id,
                    method: Method::Get,
                    path: format!(
                        "{base}/history?with-order-id={}&limit=2",
                        safe_cex_engine::url::encode(native_id)
                    ),
                    body: None,
                    step,
                },
            };
            self.schedule(cx, Operation::Trade(request));
        }
    }

    pub(crate) fn on_trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        body: &str,
    ) {
        let command = request.command;
        let was_place = matches!(request.step, Step::Place { .. });
        let outcome = match request.step {
            Step::Place { code, order } => serde_json::from_str::<PlaceAck>(body)
                .map_err(|_| "Invalid DXtrade placement acknowledgement".to_owned())
                .and_then(|ack| ack.order_id.text())
                .map_err(unproven)
                .map(|id| {
                    self.trading.acknowledged.insert(
                        id.clone(),
                        Acknowledged {
                            code,
                            symbol: order.instrument,
                            created_at: cx.now,
                        },
                    );
                    self.order_revision = self.order_revision.wrapping_add(1);
                    if let Some(job) = self.trading.jobs.get_mut(&command) {
                        job.ids.push(id);
                        job.reserved_places = job.reserved_places.saturating_sub(1);
                    }
                }),
            Step::Cancel {
                id,
                code,
                amend_baseline,
            } => {
                let native_id = serde_json::from_str::<PlaceAck>(body)
                    .ok()
                    .and_then(|ack| ack.order_id.text().ok());
                if amend_baseline.is_some() && native_id.is_none() {
                    self.on_trade_failed_step(
                        cx,
                        command,
                        unproven(
                            "Invalid DXtrade cancel acknowledgement; replacement was not submitted",
                        ),
                    );
                    return;
                }
                // The explicit successful DELETE is the cancellation proof. A
                // network failure, unknown ID, or uncertain status never reaches here.
                let filled_during_cancel = amend_baseline.as_ref().is_some_and(|baseline| {
                    cx.store.order(&id).is_some_and(|order| {
                        order.filled > baseline.filled
                            || order.remaining < baseline.remaining
                            || order.status != OrderStatus::Open
                    })
                });
                let canonical = self.trading.canonical_code(cx.store, &id).to_owned();
                self.record_canceled(cx, &id, &code);
                if let Some(job) = self.trading.jobs.get_mut(&command) {
                    job.effective = true;
                }
                self.trading.acknowledged.retain(|ack_id, ack| {
                    ack_id != &id && ack.code != canonical && ack.code != code
                });
                let canceled: Vec<_> = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|order| {
                        order.id == id
                            || order.client_order_id.as_deref() == Some(canonical.as_str())
                    })
                    .filter(|order| order.status != OrderStatus::Closed)
                    .cloned()
                    .collect();
                for mut order in canceled {
                    order.status = OrderStatus::Canceled;
                    order.last_update = cx.now;
                    cx.store.upsert_order(order);
                }
                self.order_revision = self.order_revision.wrapping_add(1);
                if filled_during_cancel {
                    Err(not_sent(
                        "Breakout Prop order filled during cancellation; replacement was not submitted",
                    ))
                } else {
                    if let Some(expected) = amend_baseline
                        && let Some(job) = self.trading.jobs.get_mut(&command)
                    {
                        job.steps.push_front(Step::ConfirmCanceled {
                            native_id: native_id.expect("validated cancel ACK"),
                            code,
                            expected,
                        });
                    }
                    Ok(())
                }
            }
            Step::ConfirmCanceled {
                native_id,
                code,
                expected,
            } => confirm_canceled(
                body,
                self.account_code().unwrap_or_default(),
                &native_id,
                &code,
                &expected,
            )
            // A read before the replacement: nothing new was sent.
            .map_err(not_sent),
        };
        if was_place && outcome.is_err() {
            self.order_revision = self.order_revision.wrapping_add(1);
        }
        if let Some(job) = self.trading.jobs.get_mut(&command) {
            job.in_flight = job.in_flight.saturating_sub(1);
            match outcome {
                Ok(()) => job.effective = true,
                Err(error) => job.record(error),
            }
        }
        self.advance_trade(cx, command);
    }

    pub(crate) fn on_trade_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        if matches!(request.step, Step::Place { .. }) {
            self.order_revision = self.order_revision.wrapping_add(1);
        }
        self.on_trade_failed_step(cx, request.command, error);
    }
    fn on_trade_failed_step(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        error: VenueError,
    ) {
        if let Some(job) = self.trading.jobs.get_mut(&command) {
            job.in_flight = job.in_flight.saturating_sub(1);
            job.record(error);
        }
        self.advance_trade(cx, command);
    }
}

#[cfg(test)]
#[path = "../tests/support/trading_units.rs"]
mod tests;
