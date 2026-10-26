use crate::{Coinbase, Pending, account::Editable};
use safe_cex_engine::Level;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Method, Outcome, Response, VenueError, num,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

const ROOT: &str = "/api/v3/brokerage/orders";
const MAX_JOBS: usize = 100;

pub struct Request {
    command: CommandId,
    step: Step,
}
enum Step {
    Place {
        payload: Create,
        order: Box<Order>,
        canceled: Option<String>,
        reducer: Reducer,
    },
    Cancel {
        ids: Vec<String>,
    },
    CancelSymbol {
        symbol: String,
    },
    Confirm {
        id: String,
        product: String,
    },
    Edit {
        payload: Edit,
        proof: Editable,
    },
}
struct Job {
    command: CommandId,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    collect: bool,
    /// A write took effect, so a later failure cannot mean nothing happened.
    effective: bool,
}
#[derive(Default)]
pub(crate) struct Trading {
    active: Option<Job>,
    queue: VecDeque<Job>,
    waiting: bool,
    reductions: HashMap<String, Reduction>,
    position_proofs: HashMap<(String, OrderSide), PositionProof>,
    blocked_reductions: HashMap<(String, OrderSide), u64>,
}
struct Reduction {
    symbol: String,
    side: OrderSide,
    amount: f64,
    native_id: Option<String>,
    accounted_fill: f64,
    /// Time and account revision of a write whose outcome was never reported.
    unresolved: Option<(f64, u64)>,
    /// The pool of a reduce-only placement; stored orders never report it.
    reducer: Option<Reducer>,
}
/// An unidentified placement is released after this long and one later order snapshot.
const UNRESOLVED_PLACEMENT_MS: f64 = 30_000.0;
struct PositionProof {
    native_position: f64,
    generation: u64,
}
#[derive(Serialize)]
struct Create {
    client_order_id: String,
    product_id: String,
    side: &'static str,
    leverage: &'static str,
    order_configuration: Configuration,
}
#[derive(Serialize)]
enum Configuration {
    #[serde(rename = "market_market_ioc")]
    Market(Size),
    #[serde(rename = "limit_limit_gtc")]
    Limit(Limit),
    #[serde(rename = "stop_limit_stop_limit_gtc")]
    Stop(Stop),
}
#[derive(Serialize)]
struct Size {
    base_size: String,
}
#[derive(Serialize)]
struct Limit {
    base_size: String,
    limit_price: String,
    post_only: bool,
}
#[derive(Serialize)]
struct Stop {
    base_size: String,
    limit_price: String,
    stop_price: String,
    stop_direction: &'static str,
}
#[derive(Serialize)]
struct Edit {
    order_id: String,
    price: String,
    size: String,
}
#[derive(Deserialize)]
struct CreateResponse {
    success: bool,
    success_response: Option<Created>,
    order_id: Option<String>,
    error_response: Option<CreateError>,
    failure_reason: Option<String>,
}
#[derive(Deserialize)]
struct Created {
    order_id: String,
    product_id: Option<String>,
    client_order_id: Option<String>,
    side: Option<String>,
}
#[derive(Deserialize)]
struct CreateError {
    error: Option<String>,
    new_order_failure_reason: Option<String>,
    preview_failure_reason: Option<String>,
    message: Option<String>,
    error_details: Option<String>,
}
#[derive(Deserialize)]
struct HttpError {
    error: Option<String>,
    message: Option<String>,
    error_details: Option<String>,
}
#[derive(Deserialize)]
struct Canceled {
    results: Vec<CancelResult>,
}
#[derive(Deserialize)]
struct CancelResult {
    order_id: String,
    success: bool,
    failure_reason: Option<String>,
}
#[derive(Deserialize)]
struct Edited {
    success: bool,
    errors: Vec<EditError>,
}
#[derive(Deserialize)]
struct EditError {
    edit_failure_reason: Option<String>,
    preview_failure_reason: Option<String>,
}
#[derive(Deserialize)]
struct Detail {
    order: DetailOrder,
}
#[derive(Deserialize)]
struct DetailOrder {
    order_id: String,
    product_id: String,
    status: String,
    filled_size: String,
}

fn command_id(command: &Command) -> Option<CommandId> {
    match command {
        Command::PlaceOrders { command, .. }
        | Command::CancelOrders { command, .. }
        | Command::AmendOrder { command, .. }
        | Command::CancelSymbolOrders { command, .. }
        | Command::SetLeverage { command, .. }
        | Command::SetMarginType { command, .. }
        | Command::SetPositionMode { command, .. }
        | Command::FetchLeverage { command, .. } => Some(*command),
        _ => None,
    }
}
fn side(side: OrderSide) -> &'static str {
    if side == OrderSide::Buy {
        "BUY"
    } else {
        "SELL"
    }
}
fn live(order: &Order) -> bool {
    matches!(
        order.status,
        OrderStatus::Open | OrderStatus::Creating | OrderStatus::Updating | OrderStatus::Canceling
    )
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 1024 && !id.chars().any(|c| c.is_whitespace() || c.is_control())
}
fn decimal(text: &str) -> Option<String> {
    let (whole, fraction) = text
        .split_once('.')
        .map_or((text, None), |(w, f)| (w, Some(f)));
    if whole.is_empty()
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || fraction.is_some_and(|f| f.is_empty() || !f.bytes().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    let whole = whole.trim_start_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole };
    let fraction = fraction.unwrap_or("").trim_end_matches('0');
    Some(if fraction.is_empty() {
        whole.into()
    } else {
        format!("{whole}.{fraction}")
    })
}
fn positive(value: f64) -> Result<String, String> {
    if !value.is_finite() || value <= 0.0 {
        return Err("Coinbase order price and size must be finite and positive".into());
    }
    let text = num::format(value);
    if decimal(&text).is_none() || text.parse::<f64>().ok() != Some(value) {
        return Err("Coinbase decimal cannot be represented exactly".into());
    }
    Ok(text)
}
fn encode<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap()
}

impl Coinbase {
    fn prepare_create(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
        canceled: Option<String>,
    ) -> Result<Step, String> {
        let market = cx
            .store
            .market(&opts.symbol)
            .ok_or("Unknown Coinbase market")?;
        if !market.active || market.is_read_only == Some(true) {
            return Err("Coinbase market is not tradable".into());
        }
        if opts.stop_loss.is_some()
            || opts.take_profit.is_some()
            || opts.tp_trigger_type.is_some()
            || opts.sl_trigger_type.is_some()
            || opts.max_slippage_pct.is_some()
            || opts.bybit_bbo.is_some()
            || opts.bybit_slippage_tolerance_ticks.is_some()
            || opts.stp_mode.is_some()
        {
            return Err("Coinbase does not support these bracket or slippage instructions".into());
        }
        if opts
            .trigger_type
            .is_some_and(|t| t != StopTriggerType::LastPrice)
        {
            return Err("Coinbase supports only last-price stop triggers".into());
        }
        if !matches!(
            opts.kind,
            OrderType::Limit | OrderType::Market | OrderType::StopLoss
        ) {
            return Err("Unsupported Coinbase order type".into());
        }
        if opts.kind != OrderType::StopLoss
            && (opts.trigger_price.is_some() || opts.trigger_type.is_some())
        {
            return Err("Coinbase ordinary orders cannot carry trigger instructions".into());
        }
        let tif = match opts.kind {
            OrderType::Market => {
                // Coinbase market orders are always IOC; the CLI sends GTC.
                if !matches!(
                    opts.time_in_force,
                    None | Some(TimeInForce::ImmediateOrCancel | TimeInForce::GoodTillCancel)
                ) {
                    return Err("Coinbase market orders support IOC only".into());
                }
                TimeInForce::ImmediateOrCancel
            }
            OrderType::Limit => {
                if !matches!(
                    opts.time_in_force,
                    None | Some(TimeInForce::GoodTillCancel | TimeInForce::PostOnly)
                ) {
                    return Err("Coinbase limit orders support GTC or PostOnly only".into());
                }
                opts.time_in_force.unwrap_or(TimeInForce::GoodTillCancel)
            }
            _ => {
                if !matches!(opts.time_in_force, None | Some(TimeInForce::GoodTillCancel)) {
                    return Err("Coinbase stop-limit orders support GTC only".into());
                }
                TimeInForce::GoodTillCancel
            }
        };
        let amount = num::adjust(opts.amount, market.precision.amount);
        if !opts.amount.is_finite()
            || opts.amount <= 0.0
            || opts.amount > market.limits.amount.max
            || amount < market.limits.amount.min
            || amount > market.limits.amount.max
        {
            return Err("Coinbase size is outside market limits".into());
        }
        let size = positive(amount)?;
        let price = if opts.kind == OrderType::Market {
            0.0
        } else {
            let raw = if opts.kind == OrderType::StopLoss {
                if opts.trigger_price.is_some()
                    && opts.price.is_some()
                    && opts.trigger_price != opts.price
                {
                    return Err("Coinbase stop requires one trigger price".into());
                }
                opts.trigger_price.or(opts.price)
            } else {
                opts.price
            };
            let raw = raw.ok_or("Coinbase price is required")?;
            if !raw.is_finite() || raw <= 0.0 {
                return Err("Coinbase price must be positive".into());
            }
            num::adjust(raw, market.precision.price)
        };
        let client = if let Some(id) = &opts.client_order_id {
            if !valid_id(id)
                || id.chars().count() > 128
                || opts.kind == OrderType::StopLoss
                || opts.close == Some(true)
            {
                return Err(
                    "Coinbase caller ID requires one ordinary order and 1..128 non-whitespace characters".into(),
                );
            }
            id.clone()
        } else {
            format!(
                "tealstreet-{}",
                safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))?
            )
        };
        let config = match opts.kind {
            OrderType::Market => Configuration::Market(Size { base_size: size }),
            OrderType::Limit => Configuration::Limit(Limit {
                base_size: size,
                limit_price: positive(price)?,
                post_only: tif == TimeInForce::PostOnly,
            }),
            _ => {
                let up = opts.side == OrderSide::Buy;
                Configuration::Stop(Stop {
                    base_size: size,
                    stop_price: positive(price)?,
                    limit_price: positive(num::adjust(
                        price * if up { 1.01 } else { 0.99 },
                        market.precision.price,
                    ))?,
                    stop_direction: if up {
                        "STOP_DIRECTION_STOP_UP"
                    } else {
                        "STOP_DIRECTION_STOP_DOWN"
                    },
                })
            }
        };
        let scale = if market.is_expiring == Some(true) {
            market
                .contract_size
                .filter(|n| n.is_finite() && *n > 0.0)
                .ok_or("Invalid Coinbase CFM contract size")?
        } else {
            1.0
        };
        let displayed = amount * scale;
        if !displayed.is_finite() {
            return Err("Coinbase amount overflow".into());
        }
        Ok(Step::Place {
            payload: Create {
                client_order_id: client.clone(),
                product_id: market.id.clone(),
                side: side(opts.side),
                leverage: "10",
                order_configuration: config,
            },
            order: Box::new(Order {
                id: String::new(),
                parent_id: Some(client.clone()),
                client_order_id: Some(client),
                status: OrderStatus::Open,
                symbol: market.symbol.clone(),
                kind: opts.kind,
                side: opts.side,
                price,
                amount: displayed,
                filled: 0.0,
                remaining: displayed,
                // Spot has no position to reduce, so reduce-only spot is a plain order.
                reduce_only: (opts.reduce_only == Some(true) || opts.close == Some(true))
                    && market.is_spot != Some(true),
                close: None,
                time_in_force: Some(tif),
                last_update: cx.now,
                trigger_type: None,
                take_profit: None,
                stop_loss: None,
                extra: Some(OrderExtra {
                    remaining_native: Some(amount),
                    filled_in_placement_units: Some(0.0),
                    ..Default::default()
                }),
            }),
            canceled,
            reducer: reducer(opts.kind, opts.close == Some(true)),
        })
    }

    fn prepare_edit(
        &self,
        cx: &Cx<'_, Pending>,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<Step, String> {
        if price.is_some() == amount.is_some() {
            return Err("Coinbase native edit requires exactly one price or size update".into());
        }
        let proof = self
            .account
            .editable(id)
            .ok_or("Coinbase native edit requires reconciled zero-fill spot LIMIT provenance")?
            .clone();
        self.validate_edit(cx, id, &proof)?;
        let order = cx.store.order(id).unwrap();
        let market = cx.store.market(&order.symbol).unwrap();
        let next_price = num::adjust(price.unwrap_or(order.price), market.precision.price);
        let next_size = num::adjust(amount.unwrap_or(order.amount), market.precision.amount);
        if price.is_some_and(|n| !n.is_finite() || n <= 0.0)
            || amount.is_some_and(|n| !n.is_finite() || n <= 0.0)
            || next_size < market.limits.amount.min
            || next_size > market.limits.amount.max
        {
            return Err("Coinbase edit is outside market limits".into());
        }
        if next_price == order.price && next_size == order.amount {
            return Err("Coinbase edit must change price or size after rounding".into());
        }
        Ok(Step::Edit {
            payload: Edit {
                order_id: id.into(),
                price: positive(next_price)?,
                size: positive(next_size)?,
            },
            proof,
        })
    }

    fn validate_edit(
        &self,
        cx: &Cx<'_, Pending>,
        id: &str,
        proof: &Editable,
    ) -> Result<(), String> {
        let current = self
            .account
            .editable(id)
            .ok_or("Coinbase native edit provenance changed")?;
        let order = cx
            .store
            .order(id)
            .ok_or("Coinbase native edit order is unavailable")?;
        let market = cx
            .store
            .market(&order.symbol)
            .ok_or("Coinbase native edit market is unavailable")?;
        if current.native_product != proof.native_product
            || current.price != proof.price
            || current.size != proof.size
            || current.side != proof.side
            || order.status != OrderStatus::Open
            || order.kind != OrderType::Limit
            || order.side != proof.side
            || order.filled != 0.0
            || order.remaining != order.amount
            || order.reduce_only
            || order.close == Some(true)
            || order.stop_loss.is_some()
            || order.take_profit.is_some()
            || market.is_spot != Some(true)
            || market.is_expiring == Some(true)
            || market.contract_size != Some(1.0)
            || crate::public::ws_symbol(&market.id)
                != crate::public::ws_symbol(&proof.native_product)
            || num::adjust(order.price, market.precision.price) != order.price
            || num::adjust(order.amount, market.precision.amount) != order.amount
            || decimal(&proof.price) != decimal(&positive(order.price)?)
            || decimal(&proof.size) != decimal(&positive(order.amount)?)
        {
            return Err(
                "Coinbase native edit requires exact current zero-fill spot LIMIT provenance"
                    .into(),
            );
        }
        Ok(())
    }

    fn prepare_job(&self, cx: &Cx<'_, Pending>, command: Command) -> Result<Job, String> {
        let id = command_id(&command).unwrap();
        let mut job = Job {
            command: id,
            steps: VecDeque::new(),
            ids: vec![],
            collect: false,
            effective: false,
        };
        match command {
            Command::PlaceOrders { orders, .. } => {
                if orders.is_empty() || orders.len() > 100 {
                    return Err("Coinbase placement batch must contain 1 to 100 orders".into());
                }
                let mut ids = HashSet::new();
                for opts in &orders {
                    if let Some(id) = &opts.client_order_id
                        && !ids.insert(id)
                    {
                        return Err("Coinbase batch contains a duplicate caller ID".into());
                    }
                    job.steps.push_back(self.prepare_create(cx, opts, None)?);
                }
                job.collect = true;
            }
            Command::CancelOrders { ids, .. } => job.steps = cancel_steps(ids)?,
            Command::CancelSymbolOrders { symbol, .. } => {
                if cx.store.market(&symbol).is_none() {
                    return Err("Unknown Coinbase cancel market".into());
                }
                job.steps.push_back(Step::CancelSymbol { symbol });
            }
            Command::AmendOrder {
                id, price, amount, ..
            } => {
                if !valid_id(&id) {
                    return Err("Invalid Coinbase order ID".into());
                }
                if self.account.is_intx() {
                    let order = cx
                        .store
                        .order(&id)
                        .ok_or("Coinbase amend requires a reconciled order")?;
                    if order.status != OrderStatus::Open
                        || order.filled != 0.0
                        || order.close == Some(true)
                        || order.stop_loss.is_some()
                        || order.take_profit.is_some()
                        || price.is_none() && amount.is_none()
                    {
                        return Err(
                            "Coinbase INTX replacement requires an ordinary zero-fill open order"
                                .into(),
                        );
                    }
                    let opts = PlaceOrderOpts {
                        symbol: order.symbol.clone(),
                        kind: order.kind,
                        side: order.side,
                        amount: amount.unwrap_or(order.amount),
                        price: price.or(Some(order.price)),
                        trigger_price: None,
                        max_slippage_pct: None,
                        stop_loss: None,
                        take_profit: None,
                        reduce_only: Some(order.reduce_only),
                        time_in_force: order.time_in_force,
                        trigger_type: order.trigger_type,
                        tp_trigger_type: None,
                        sl_trigger_type: None,
                        client_order_id: None,
                        close: (self.stored_reducer(order) == Reducer::Close
                            && order.kind != OrderType::Market)
                            .then_some(true),
                        bybit_bbo: None,
                        bybit_slippage_tolerance_ticks: None,
                        implied_volatility: None,
                        stp_mode: None,
                    };
                    let replacement = self.prepare_create(cx, &opts, Some(id.clone()))?;
                    job.steps.push_back(Step::Cancel {
                        ids: vec![id.clone()],
                    });
                    job.steps.push_back(Step::Confirm {
                        id,
                        product: cx
                            .store
                            .market(&order.symbol)
                            .ok_or("Unknown Coinbase amend market")?
                            .id
                            .clone(),
                    });
                    job.steps.push_back(replacement);
                    job.collect = true;
                } else {
                    job.steps
                        .push_back(self.prepare_edit(cx, &id, price, amount)?);
                    job.collect = true;
                }
            }
            _ => unreachable!(),
        }
        let mut planned = HashMap::<(String, OrderSide, Reducer), f64>::new();
        let mut constrained = HashSet::new();
        let excluded: HashSet<_> = job
            .steps
            .iter()
            .filter_map(|step| match step {
                Step::Place { canceled, .. } => canceled.clone(),
                _ => None,
            })
            .collect();
        for step in self
            .trading
            .active
            .iter()
            .flat_map(|j| &j.steps)
            .chain(self.trading.queue.iter().flat_map(|j| &j.steps))
            .chain(&job.steps)
        {
            if let Step::Place { order, reducer, .. } = step {
                if order.reduce_only {
                    constrained.insert((order.symbol.clone(), order.side, *reducer));
                }
                let amount = order
                    .extra
                    .as_ref()
                    .and_then(|e| e.remaining_native)
                    .unwrap();
                for pool in [Reducer::Stop, Reducer::TakeProfit, Reducer::Close] {
                    if reserves(*reducer, pool) {
                        *planned
                            .entry((order.symbol.clone(), order.side, pool))
                            .or_default() += amount;
                    }
                }
            }
        }
        for ((symbol, side, class), amount) in planned {
            if constrained.contains(&(symbol.clone(), side, class))
                && amount > self.reduction_capacity(cx, &symbol, side, class, &excluded)?
            {
                return Err(
                    "Coinbase reduce-only orders exceed observed unreserved position size".into(),
                );
            }
        }
        Ok(job)
    }

    fn reduction_capacity(
        &self,
        cx: &Cx<'_, Pending>,
        symbol: &str,
        side: OrderSide,
        class: Reducer,
        excluded: &HashSet<String>,
    ) -> Result<f64, String> {
        if self
            .trading
            .blocked_reductions
            .contains_key(&(symbol.into(), side))
        {
            return Err(
                "Coinbase closing exposure requires an authoritative flat-position repair".into(),
            );
        }
        let market = cx
            .store
            .market(symbol)
            .ok_or("Unknown Coinbase reduce-only market")?;
        if market.is_spot == Some(true) || !cx.store.loaded().positions {
            return Err("Coinbase reduce-only needs a reconciled derivatives position".into());
        }
        let scale = market
            .contract_size
            .filter(|s| s.is_finite() && *s > 0.0)
            .ok_or("Invalid Coinbase contract size")?;
        let opposite = if side == OrderSide::Buy {
            PositionSide::Short
        } else {
            PositionSide::Long
        };
        let positions: Vec<_> = cx
            .store
            .positions()
            .iter()
            .filter(|p| p.symbol == symbol && p.side == opposite && p.contracts > 0.0)
            .collect();
        if positions.len() != 1 || !positions[0].contracts.is_finite() {
            return Err("Coinbase reduce-only needs exactly one opposite position".into());
        }
        // Stops, take-profits and closes reduce the same position as alternatives,
        // so each pool reserves its own remaining size (see `reserves`). Executed
        // and unidentified reductions below count against every pool.
        let mut reserved = 0.0;
        for order in cx.store.orders().iter().filter(|o| {
            o.symbol == symbol
                && o.side == side
                && reserves(self.stored_reducer(o), class)
                && live(o)
                && !excluded.contains(&o.id)
        }) {
            let amount = order
                .extra
                .as_ref()
                .and_then(|e| e.remaining_native)
                .unwrap_or(order.remaining / scale);
            if !amount.is_finite() || amount < 0.0 {
                return Err("Invalid Coinbase closing-order size".into());
            }
            reserved += amount;
        }
        for fence in self
            .trading
            .reductions
            .values()
            .filter(|r| r.symbol == symbol && r.side == side)
        {
            if fence
                .native_id
                .as_ref()
                .is_some_and(|id| excluded.contains(id))
            {
                continue;
            }
            if let Some(id) = fence.native_id.as_deref() {
                if let Some((filled, _, _)) = self.reduction_fill(cx, id) {
                    reserved += (filled - fence.accounted_fill).max(0.0);
                } else {
                    reserved += fence.amount;
                }
            } else {
                reserved += fence.amount;
            }
        }
        let capacity = positions[0].contracts / scale - reserved;
        if !capacity.is_finite() {
            return Err("Invalid Coinbase observed position size".into());
        }
        Ok(capacity.max(0.0))
    }

    fn stored_reducer(&self, order: &Order) -> Reducer {
        let placed = order
            .client_order_id
            .as_ref()
            .and_then(|c| self.trading.reductions.get(c))
            .and_then(|r| r.reducer);
        placed.unwrap_or_else(|| reducer(order.kind, order.close == Some(true)))
    }
    /// Coinbase has no native reduce-only, so resting reductions a close leaves
    /// uncovered can open a new position; name them rather than cancel them.
    fn warn_orphaned_reductions(&self, cx: &mut Cx<'_, Pending>, close: &Order, amount: f64) {
        let Some(position) = self.observed_position_native(cx, &close.symbol, close.side) else {
            return;
        };
        let left = (position - amount).max(0.0);
        for class in [Reducer::Stop, Reducer::TakeProfit] {
            let resting: Vec<_> = cx
                .store
                .orders()
                .iter()
                .filter(|o| {
                    o.symbol == close.symbol
                        && o.side == close.side
                        && live(o)
                        && o.client_order_id
                            .as_ref()
                            .and_then(|c| self.trading.reductions.get(c))
                            .is_some_and(|r| r.reducer == Some(class))
                })
                .collect();
            let reserved: f64 = resting
                .iter()
                .filter_map(|o| o.extra.as_ref().and_then(|e| e.remaining_native))
                .sum();
            if reserved > left {
                let ids: Vec<_> = resting.iter().map(|o| o.id.as_str()).collect();
                cx.notice(Level::Warn, code::ORDERS_REPLACED,
                    format!(
                        "Coinbase close of {} leaves reduce-only orders {} exceeding the remaining position; Coinbase cannot enforce reduce-only, so cancel them or they may open a new position",
                        close.symbol,
                        ids.join(", ")
                    ),
                );
            }
        }
    }
    fn observed_position_native(
        &self,
        cx: &Cx<'_, Pending>,
        symbol: &str,
        side: OrderSide,
    ) -> Option<f64> {
        let market = cx.store.market(symbol)?;
        let scale = market.contract_size.filter(|n| n.is_finite() && *n > 0.0)?;
        let opposite = if side == OrderSide::Sell {
            PositionSide::Long
        } else {
            PositionSide::Short
        };
        let amount = cx
            .store
            .positions()
            .iter()
            .filter(|p| p.symbol == symbol && p.side == opposite)
            .map(|p| p.contracts / scale)
            .sum::<f64>();
        (amount.is_finite() && amount >= 0.0).then_some(amount)
    }
    fn reduction_fill(&self, cx: &Cx<'_, Pending>, id: &str) -> Option<(f64, u64, bool)> {
        if let Some((version, filled)) = self.account.terminal_fill(id) {
            return Some((filled, version, true));
        }
        let order = cx.store.order(id)?;
        let market = cx.store.market(&order.symbol)?;
        let scale = market.contract_size.filter(|n| n.is_finite() && *n > 0.0)?;
        let filled = order
            .extra
            .as_ref()
            .and_then(|e| e.filled_in_placement_units)
            .unwrap_or(order.filled / scale);
        (filled.is_finite() && filled >= 0.0).then_some((
            filled,
            self.account.order_revision(id),
            false,
        ))
    }
    pub(crate) fn trading_observed_order(&mut self, cx: &Cx<'_, Pending>, order: &Order) {
        let Some(client) = order.client_order_id.as_deref() else {
            return;
        };
        let Some(reservation) = self.trading.reductions.get(client) else {
            return;
        };
        if reservation.symbol != order.symbol
            || reservation.side != order.side
            || reservation
                .native_id
                .as_deref()
                .is_some_and(|id| id != order.id)
            || !order
                .extra
                .as_ref()
                .and_then(|e| e.remaining_native.zip(e.filled_in_placement_units))
                .is_some_and(|(remaining, filled)| {
                    remaining + filled == reservation.amount
                        || order.status != OrderStatus::Open && filled <= reservation.amount
                })
        {
            return;
        }
        let external_key = format!("\0external:{}", order.id);
        if self
            .trading
            .reductions
            .get(&external_key)
            .is_some_and(|f| f.symbol != order.symbol || f.side != order.side)
        {
            return;
        }
        let external = self.trading.reductions.remove(&external_key);
        let reservation = self.trading.reductions.get_mut(client).unwrap();
        reservation.native_id = Some(order.id.clone());
        if let Some(external) = external {
            reservation.accounted_fill = reservation.accounted_fill.max(external.accounted_fill);
        }
        self.private_placement_finished(client);
        if cx
            .store
            .market(&order.symbol)
            .is_some_and(|m| m.is_spot == Some(true))
        {
            self.trading.reductions.remove(client);
        }
    }
    pub(crate) fn trading_order_amount(
        &self,
        client: &str,
        native_id: &str,
        symbol: &str,
        side: OrderSide,
    ) -> Option<f64> {
        self.trading
            .reductions
            .get(client)
            .filter(|r| {
                r.symbol == symbol
                    && r.side == side
                    && r.native_id.as_deref().is_none_or(|id| id == native_id)
            })
            .map(|r| r.amount)
    }
    pub(crate) fn trading_observed_fill(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        order: &Order,
        previous_base: f64,
    ) {
        if self
            .trading
            .reductions
            .values()
            .any(|f| f.native_id.as_deref() == Some(order.id.as_str()))
        {
            return;
        }
        let Some(m) = cx.store.market(&order.symbol) else {
            return;
        };
        if m.is_spot == Some(true) {
            return;
        }
        let Some(scale) = m.contract_size.filter(|s| s.is_finite() && *s > 0.0) else {
            return;
        };
        let Some(position) = self
            .observed_position_native(cx, &order.symbol, order.side)
            .filter(|n| *n > 0.0)
        else {
            return;
        };
        let key = (order.symbol.clone(), order.side);
        if self.trading.reductions.len() >= 4096 {
            self.trading
                .blocked_reductions
                .insert(key, self.account.order_revision(&order.id));
            return;
        }
        self.trading
            .position_proofs
            .entry(key)
            .or_insert(PositionProof {
                native_position: position,
                generation: self.account.position_generation(),
            });
        self.trading.reductions.insert(
            format!("\0external:{}", order.id),
            Reduction {
                symbol: order.symbol.clone(),
                side: order.side,
                amount: order.amount / scale,
                native_id: Some(order.id.clone()),
                accounted_fill: previous_base / scale,
                unresolved: None,
                reducer: None,
            },
        );
    }
    pub(crate) fn trading_positions(&mut self, cx: &Cx<'_, Pending>) {
        self.reconcile_reductions(cx);
    }
    fn reconcile_reductions(&mut self, cx: &Cx<'_, Pending>) {
        let committed = self.account.committed_orders_revision();
        let positions = self.account.position_read_revision();
        let expired = self
            .trading
            .reductions
            .iter()
            .filter(|(_, f)| {
                // A derivative fill must also be visible in a later position read.
                let spot = cx
                    .store
                    .market(&f.symbol)
                    .is_some_and(|m| m.is_spot == Some(true));
                f.native_id.is_none()
                    && f.unresolved.is_some_and(|(at, revision)| {
                        cx.now >= at + UNRESOLVED_PLACEMENT_MS
                            && committed >= revision
                            && (spot || positions.is_some_and(|read| read >= revision))
                    })
            })
            .map(|(client, _)| client.clone())
            .collect::<Vec<_>>();
        for client in expired {
            self.trading.reductions.remove(&client);
            self.private_placement_finished(&client);
        }
        let account = &self.account;
        self.trading.reductions.retain(|_, f| {
            !f.native_id
                .as_deref()
                .and_then(|id| account.terminal_fill(id))
                .is_some_and(|(_, filled)| {
                    filled == 0.0
                        || cx
                            .store
                            .market(&f.symbol)
                            .is_some_and(|m| m.is_spot == Some(true))
                })
        });
        let Some(read_revision) = self.account.position_read_revision() else {
            return;
        };
        let blocked = self
            .trading
            .blocked_reductions
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for key in blocked {
            if read_revision >= self.trading.blocked_reductions[&key]
                && self.observed_position_native(cx, &key.0, key.1) == Some(0.0)
            {
                self.trading.blocked_reductions.remove(&key);
            }
        }
        let generation = self.account.position_generation();
        let keys = self
            .trading
            .position_proofs
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            let proof = &self.trading.position_proofs[&key];
            if proof.generation == generation {
                continue;
            }
            let Some(position) = self.observed_position_native(cx, &key.0, key.1) else {
                continue;
            };
            let rows = self
                .trading
                .reductions
                .iter()
                .filter(|(_, f)| f.symbol == key.0 && f.side == key.1)
                .map(|(client, f)| {
                    (
                        client.clone(),
                        f.accounted_fill,
                        f.native_id
                            .as_deref()
                            .and_then(|id| self.reduction_fill(cx, id)),
                    )
                })
                .collect::<Vec<_>>();
            let mut executed = 0.0;
            let mut newest = 0;
            for (_, accounted, state) in &rows {
                if let Some((filled, version, terminal)) = state {
                    executed += (filled - accounted).max(0.0);
                    if *filled > *accounted || *terminal {
                        newest = newest.max(*version);
                    }
                }
            }
            if read_revision < newest {
                continue;
            }
            if executed > 0.0
                && position > 0.0
                && position > (proof.native_position - executed).max(0.0)
            {
                continue;
            }
            let mut remove = vec![];
            for (client, _, state) in rows {
                if let Some((filled, _, terminal)) = state {
                    if terminal {
                        remove.push(client);
                    } else if let Some(f) = self.trading.reductions.get_mut(&client) {
                        f.accounted_fill = filled;
                    }
                }
            }
            for client in remove {
                self.trading.reductions.remove(&client);
            }
            if let Some(proof) = self.trading.position_proofs.get_mut(&key) {
                proof.native_position = position;
                proof.generation = generation;
            }
        }
        self.trading.position_proofs.retain(|(symbol, side), _| {
            self.trading
                .reductions
                .values()
                .any(|f| &f.symbol == symbol && &f.side == side)
        });
    }

    pub(crate) fn trading_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Command,
    ) -> Option<Command> {
        if command_id(&command).is_none() {
            return Some(command);
        }
        match &command {
            Command::SetLeverage { .. }
            | Command::SetMarginType { .. }
            | Command::SetPositionMode { .. } => {
                crate::local_context::complete(
                    cx,
                    safe_cex_engine::request_context::Operation::Mutation,
                    Err(not_sent("Coinbase does not support account leverage, margin or position-mode mutations").kind(ErrorKind::Unsupported)),
                );
                return None;
            }
            Command::FetchLeverage { symbol, .. } => {
                if let Some(setting) = cx.store.setting(symbol) {
                    #[derive(Serialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Leverage {
                        buy_leverage: f64,
                        sell_leverage: f64,
                    }
                    crate::local_context::complete(
                        cx,
                        safe_cex_engine::request_context::Operation::Read,
                        Ok(encode(&Leverage {
                            buy_leverage: setting.leverage,
                            sell_leverage: setting.leverage,
                        })),
                    );
                } else {
                    crate::local_context::complete(
                        cx,
                        safe_cex_engine::request_context::Operation::Read,
                        Err(VenueError::new("Coinbase market settings are unavailable")),
                    );
                }
                return None;
            }
            _ => {}
        }
        if self.credentials.is_none()
            || self.options.public_data_only
            || self.account.kind.is_none()
            || !cx.store.loaded().markets
            || !cx.store.loaded().orders
        {
            crate::local_context::complete(
                cx,
                safe_cex_engine::request_context::Operation::Mutation,
                Err(not_sent("Coinbase account is not ready for trading")),
            );
            return None;
        }
        self.reconcile_reductions(cx);
        if self.trading.queue.len() + usize::from(self.trading.active.is_some()) >= MAX_JOBS {
            crate::local_context::complete(
                cx,
                safe_cex_engine::request_context::Operation::Mutation,
                Err(not_sent("Coinbase trading queue is full")),
            );
            return None;
        }
        match self.prepare_job(cx, command) {
            Ok(job) => {
                self.trading.queue.push_back(job);
                self.trading_next(cx);
            }
            Err(error) => crate::local_context::complete(
                cx,
                safe_cex_engine::request_context::Operation::Mutation,
                Err(not_sent(error)),
            ),
        }
        None
    }

    pub(crate) fn trading_before_dispatch(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &Request,
    ) -> Result<(), String> {
        self.reconcile_reductions(cx);
        match &request.step {
            Step::Edit { payload, proof } => {
                self.validate_edit(cx, &payload.order_id, proof)?;
                self.private_order_changed(&payload.order_id);
            }
            Step::Place {
                order,
                payload,
                canceled,
                reducer,
            } => {
                if let Some(id) = canceled
                    && !cx
                        .store
                        .order(id)
                        .is_some_and(|o| o.status == OrderStatus::Canceled && o.filled == 0.0)
                {
                    return Err(
                        "Coinbase replacement cancellation is no longer authoritative".into(),
                    );
                }
                let market = cx
                    .store
                    .market(&order.symbol)
                    .ok_or("Coinbase market is no longer available")?;
                if !market.active || self.options.public_data_only || self.account.kind.is_none() {
                    return Err("Coinbase account or market is no longer tradable".into());
                }
                if self.trading.reductions.len() >= 4096
                    || self
                        .trading
                        .reductions
                        .contains_key(&payload.client_order_id)
                {
                    return Err("Coinbase placement reservation is unavailable".into());
                }
                let amount = order
                    .extra
                    .as_ref()
                    .and_then(|e| e.remaining_native)
                    .unwrap();
                if order.reduce_only
                    && amount
                        > self.reduction_capacity(
                            cx,
                            &order.symbol,
                            order.side,
                            *reducer,
                            &HashSet::new(),
                        )?
                {
                    return Err("Coinbase reduce-only position changed before dispatch".into());
                }
                if order.reduce_only && *reducer == Reducer::Close {
                    self.warn_orphaned_reductions(cx, order, amount);
                }
                self.private_placement_started(&payload.client_order_id)?;
                let baseline = self
                    .observed_position_native(cx, &order.symbol, order.side)
                    .unwrap_or(0.0);
                self.trading
                    .position_proofs
                    .entry((order.symbol.clone(), order.side))
                    .or_insert(PositionProof {
                        native_position: baseline,
                        generation: self.account.position_generation(),
                    });
                self.trading.reductions.insert(
                    payload.client_order_id.clone(),
                    Reduction {
                        symbol: order.symbol.clone(),
                        side: order.side,
                        amount,
                        native_id: None,
                        accounted_fill: 0.0,
                        unresolved: None,
                        reducer: order.reduce_only.then_some(*reducer),
                    },
                );
            }
            Step::Cancel { ids } => {
                for id in ids {
                    self.private_order_changed(id);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn trading_next(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading.waiting {
            return;
        }
        if self.trading.active.is_none() {
            self.trading.active = self.trading.queue.pop_front();
        }
        let Some(job) = self.trading.active.as_mut() else {
            return;
        };
        let Some(step) = job.steps.pop_front() else {
            let job = self.trading.active.take().unwrap();
            cx.done(
                job.command,
                if job.collect {
                    encode(&job.ids)
                } else {
                    "null".into()
                },
            );
            self.trading_next(cx);
            return;
        };
        let command = job.command;
        if let Step::CancelSymbol { symbol } = step {
            if self
                .trading
                .reductions
                .values()
                .any(|f| f.symbol == symbol && f.native_id.is_none())
            {
                self.trading_finish_error(
                    cx,
                    command,
                    not_sent("Coinbase cannot enumerate an unresolved placement for cancel-all"),
                );
                return;
            }
            let mut ids = cx
                .store
                .orders()
                .iter()
                .filter(|o| o.symbol == symbol && live(o))
                .map(|o| o.id.clone())
                .collect::<HashSet<_>>();
            ids.extend(self.account.foreign_orders(&symbol).map(String::from));
            for f in self
                .trading
                .reductions
                .values()
                .filter(|f| f.symbol == symbol)
            {
                if let Some(id) = &f.native_id
                    && self.account.terminal_fill(id).is_none()
                {
                    ids.insert(id.clone());
                }
            }
            let mut ids = ids.into_iter().collect::<Vec<_>>();
            ids.sort();
            match cancel_steps(ids) {
                Ok(mut steps) => {
                    steps.append(&mut self.trading.active.as_mut().unwrap().steps);
                    self.trading.active.as_mut().unwrap().steps = steps;
                    self.trading_next(cx);
                }
                Err(e) => self.trading_finish_error(cx, command, not_sent(e)),
            }
            return;
        }
        let request = Request { command, step };
        let (method, path, body) = match &request.step {
            Step::Place { payload, .. } => (Method::Post, ROOT.into(), Some(encode(payload))),
            Step::Edit { payload, .. } => {
                (Method::Post, format!("{ROOT}/edit"), Some(encode(payload)))
            }
            Step::Cancel { ids } => {
                #[derive(Serialize)]
                struct Cancel<'a> {
                    order_ids: &'a [String],
                }
                (
                    Method::Post,
                    format!("{ROOT}/batch_cancel"),
                    Some(encode(&Cancel { order_ids: ids })),
                )
            }
            Step::CancelSymbol { .. } => unreachable!(),
            Step::Confirm { id, .. } => (
                Method::Get,
                format!("{ROOT}/historical/{}", safe_cex_engine::url::encode(id)),
                None,
            ),
        };
        let id = request.command;
        self.trading.waiting = true;
        if let Step::Place { payload, order, .. } = &request.step {
            cx.command_progress(
                id,
                vec![OrderIdentityReceipt {
                    symbol: order.symbol.clone(),
                    kind: OrderIdentityKind::ClientOrderId,
                    identity: payload.client_order_id.clone(),
                }],
            );
        }
        if let Err(error) = self.request(cx, method, path, body, None, Pending::Trading(request)) {
            self.trading_finish_error(cx, id, not_sent(error));
        } else if method == Method::Post {
            self.private_after_write(cx);
        }
    }

    fn trading_finish_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        error: impl Into<VenueError>,
    ) {
        if self
            .trading
            .active
            .as_ref()
            .is_some_and(|job| job.command == command)
        {
            let job = self.trading.active.take().expect("active job");
            self.trading.waiting = false;
            let mut error = error.into();
            if (job.effective || !job.ids.is_empty())
                && matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent))
            {
                error.outcome = None;
            }
            cx.fail(command, error);
            self.trading_next(cx);
        }
    }

    pub(crate) fn trading_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        self.private_after_write(cx);
        self.placement_unresolved(cx, &request.step);
        let error = unknown(format!(
            "Coinbase write outcome requires reconciliation: {error}"
        ));
        self.trading_finish_error(cx, request.command, error.kind(ErrorKind::Network));
    }

    /// Refused at dispatch (signing or a dispatch-time check): Coinbase received nothing.
    /// Reservation bookkeeping is kept as for a lost write; the key may belong to another request.
    pub(crate) fn trading_unsent(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        self.private_after_write(cx);
        self.placement_unresolved(cx, &request.step);
        let error = format!("Coinbase write outcome requires reconciliation: {error}");
        self.trading_finish_error(cx, request.command, not_sent(error));
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
        if !(200..300).contains(&response.status) {
            let error = http_error(response.status, response.body);
            // 408 and 5xx do not prove the write was rejected.
            if response.status == 408 || response.status >= 500 {
                self.private_after_write(cx);
                self.placement_unresolved(cx, &request.step);
                self.trading_finish_error(
                    cx,
                    request.command,
                    unknown(format!(
                        "Coinbase write outcome requires reconciliation: {error}"
                    )),
                );
                return;
            }
            if let Step::Place { payload, .. } = &request.step {
                self.trading.reductions.remove(&payload.client_order_id);
                self.private_placement_finished(&payload.client_order_id);
            }
            let error = VenueError::new(error);
            let rejected = |error: VenueError, kind| error.kind(kind).outcome(Outcome::Rejected);
            let error = match response.status {
                401 => rejected(error, ErrorKind::AuthInvalid),
                403 => rejected(error, ErrorKind::AuthPermission),
                429 => rejected(error, ErrorKind::RateLimited),
                // Any other 4xx (3xx answers prove nothing).
                400..=499 => rejected(error, ErrorKind::Rejected),
                _ => error.outcome(Outcome::Unknown),
            };
            // The cancellation proof is a read before the replacement write.
            let error = if matches!(request.step, Step::Confirm { .. }) {
                VenueError {
                    outcome: Some(Outcome::NotSent),
                    ..error
                }
            } else {
                error
            };
            self.trading_finish_error(cx, request.command, error);
            return;
        }
        let result = self.apply_trade_response(cx, &request.step, response.body);
        self.trading.waiting = false;
        match result {
            Ok(id) => {
                let job = self.trading.active.as_mut().unwrap();
                if !matches!(request.step, Step::Confirm { .. }) {
                    job.effective = true;
                }
                if let Some(id) = id {
                    job.ids.push(id);
                }
                self.trading_next(cx);
            }
            Err(error) => {
                self.private_after_write(cx);
                self.placement_unresolved(cx, &request.step);
                self.trading_finish_error(cx, request.command, error);
            }
        }
    }

    fn placement_unresolved(&mut self, cx: &Cx<'_, Pending>, step: &Step) {
        let revision = self.account.revision();
        if let Step::Place { payload, .. } = step
            && let Some(reservation) = self.trading.reductions.get_mut(&payload.client_order_id)
            && reservation.native_id.is_none()
        {
            reservation.unresolved = Some((cx.now, revision));
        }
    }

    fn apply_trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        step: &Step,
        body: &str,
    ) -> Result<Option<String>, VenueError> {
        match step {
            Step::Place { payload, order, .. } => {
                let result: CreateResponse = serde_json::from_str(body)
                    .map_err(|_| unknown("Malformed Coinbase placement response"))?;
                if !result.success {
                    self.trading.reductions.remove(&payload.client_order_id);
                    self.private_placement_finished(&payload.client_order_id);
                    // `success: false` is Coinbase's documented refusal: no order was created,
                    // except a duplicate client ID, which may name a live order.
                    let duplicate = result.error_response.as_ref().is_some_and(|e| {
                        e.new_order_failure_reason.as_deref() == Some("DUPLICATE_CLIENT_ORDER_ID")
                    });
                    let message = placement_rejection(result.error_response, result.failure_reason);
                    return Err(if duplicate {
                        VenueError::new(message)
                    } else {
                        rejected(message)
                    });
                }
                let id = if let Some(created) = result.success_response {
                    if created
                        .product_id
                        .as_ref()
                        .is_some_and(|p| p != &payload.product_id)
                        || created
                            .client_order_id
                            .as_ref()
                            .is_some_and(|id| id != &payload.client_order_id)
                        || created.side.as_ref().is_some_and(|s| s != payload.side)
                        || result
                            .order_id
                            .as_ref()
                            .is_some_and(|id| id != &created.order_id)
                    {
                        return Err(unknown("Coinbase placement response identity mismatch"));
                    }
                    created.order_id
                } else {
                    result
                        .order_id
                        .ok_or_else(|| unknown("Coinbase placement response omitted order ID"))?
                };
                if !valid_id(&id) {
                    return Err(unknown("Coinbase placement returned invalid order ID"));
                }
                let external_key = format!("\0external:{id}");
                if self
                    .trading
                    .reductions
                    .get(&external_key)
                    .is_some_and(|f| f.symbol != order.symbol || f.side != order.side)
                {
                    return Err(unknown(
                        "Coinbase placement outcome conflicts with stream identity",
                    ));
                }
                let external = self.trading.reductions.remove(&external_key);
                if let Some(reservation) = self.trading.reductions.get_mut(&payload.client_order_id)
                {
                    reservation.native_id = Some(id.clone());
                    if let Some(external) = external {
                        reservation.accounted_fill =
                            reservation.accounted_fill.max(external.accounted_fill);
                    }
                }
                self.private_base_order(cx, &id, order);
                self.private_placement_finished(&payload.client_order_id);
                // An earlier user-stream update or deduplicated client ID owns its state.
                if cx.store.order(&id).is_none() && self.account.terminal_fill(&id).is_none() {
                    let mut order = order.as_ref().clone();
                    order.id = id.clone();
                    order.last_update = cx.now;
                    self.private_order_changed(&id);
                    cx.store.upsert_order(order);
                }
                if cx
                    .store
                    .market(&order.symbol)
                    .is_some_and(|m| m.is_spot == Some(true))
                {
                    self.trading.reductions.remove(&payload.client_order_id);
                }
                Ok(Some(id))
            }
            Step::Cancel { ids } => {
                let result: Canceled = serde_json::from_str(body)
                    .map_err(|_| unknown("Malformed Coinbase cancel response"))?;
                let expected: HashSet<_> = ids.iter().map(String::as_str).collect();
                let mut seen = HashSet::new();
                if result.results.len() != ids.len()
                    || result.results.iter().any(|r| {
                        !expected.contains(r.order_id.as_str()) || !seen.insert(r.order_id.as_str())
                    })
                {
                    return Err(unknown(
                        "Coinbase cancel response IDs do not match requested IDs",
                    ));
                }
                let mut errors = vec![];
                for result in result.results {
                    if result.success {
                        self.account.forget_foreign(&result.order_id);
                        self.private_order_changed(&result.order_id);
                        if cx.store.order(&result.order_id).is_some_and(live) {
                            cx.store
                                .set_order_status(&result.order_id, OrderStatus::Canceling);
                        }
                    } else {
                        errors.push(format!(
                            "{}: {}",
                            result.order_id,
                            result
                                .failure_reason
                                .as_deref()
                                .unwrap_or("cancel rejected")
                        ));
                    }
                }
                if !errors.is_empty() {
                    // Per-order results; other orders in the batch may have been cancelled.
                    return Err(VenueError::new(errors.join(", ")));
                }
                Ok(None)
            }
            Step::Confirm { id, product } => {
                let detail: Detail = serde_json::from_str(body)
                    .map_err(|_| not_sent("Malformed Coinbase cancellation confirmation"))?;
                if detail.order.order_id != *id
                    || detail.order.product_id != *product
                    || detail.order.status != "CANCELLED"
                    || decimal(&detail.order.filled_size).as_deref() != Some("0")
                    || !cx
                        .store
                        .order(id)
                        .is_some_and(|o| o.filled == 0.0 && o.status != OrderStatus::Closed)
                {
                    return Err(not_sent(
                        "Coinbase replacement requires authoritative zero-fill cancellation",
                    ));
                }
                self.private_order_canceled(cx, id);
                Ok(None)
            }
            Step::CancelSymbol { .. } => unreachable!(),
            Step::Edit { payload, .. } => {
                let result: Edited = serde_json::from_str(body).map_err(|_| {
                    unknown("Coinbase edit response omitted documented success/errors")
                })?;
                if !result.success || !result.errors.is_empty() {
                    // Only `success: false` documents that the edit was not placed.
                    let refused = !result.success;
                    let reasons: Vec<_> = result
                        .errors
                        .into_iter()
                        .flat_map(|e| [e.edit_failure_reason, e.preview_failure_reason])
                        .flatten()
                        .collect();
                    let message = if reasons.is_empty() {
                        "Coinbase edit rejected".to_owned()
                    } else {
                        reasons.join(", ")
                    };
                    return Err(if refused {
                        rejected(message)
                    } else {
                        VenueError::new(message)
                    });
                }
                self.private_after_write(cx);
                Ok(Some(payload.order_id.clone()))
            }
        }
    }
}

/// The write reached Coinbase but its result is unproven; reconcile before resending.
/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}
/// A documented refusal: nothing was created or changed.
fn rejected(message: impl Into<String>) -> VenueError {
    VenueError::new(message)
        .kind(ErrorKind::Rejected)
        .outcome(Outcome::Rejected)
}
fn unknown(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::Unknown)
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Reducer {
    Stop,
    TakeProfit,
    Close,
}
/// A close is never held back by resting take-profits or stops, but a new
/// take-profit still counts resting closes so it cannot outlive the position.
fn reserves(order: Reducer, pool: Reducer) -> bool {
    order == pool || pool == Reducer::TakeProfit && order == Reducer::Close
}
fn reducer(kind: OrderType, close: bool) -> Reducer {
    match kind {
        OrderType::StopLoss => Reducer::Stop,
        OrderType::Market => Reducer::Close,
        _ if close => Reducer::Close,
        _ => Reducer::TakeProfit,
    }
}
fn reason(code: Option<String>) -> Option<String> {
    code.filter(|c| !c.is_empty() && !c.starts_with("UNKNOWN_"))
}
fn describe(prefix: &str, message: Option<String>, code: Option<String>) -> String {
    let message = message.filter(|m| !m.is_empty());
    let text = match (message, reason(code)) {
        (Some(m), Some(c)) if m != c => format!("{prefix}: {m} ({c})"),
        (Some(m), _) => format!("{prefix}: {m}"),
        (None, Some(c)) => format!("{prefix}: {c}"),
        (None, None) => prefix.into(),
    };
    text.chars().take(512).collect()
}
fn placement_rejection(error: Option<CreateError>, failure_reason: Option<String>) -> String {
    let Some(e) = error else {
        return describe("Coinbase placement rejected", None, failure_reason);
    };
    let code = reason(e.error)
        .or(reason(e.new_order_failure_reason))
        .or(reason(e.preview_failure_reason))
        .or(reason(failure_reason));
    describe(
        "Coinbase placement rejected",
        e.message.filter(|m| !m.is_empty()).or(e.error_details),
        code,
    )
}
fn http_error(status: u16, body: &str) -> String {
    let prefix = format!("Coinbase HTTP {status}");
    match serde_json::from_str::<HttpError>(body) {
        Ok(e) => describe(
            &prefix,
            e.message.filter(|m| !m.is_empty()).or(e.error_details),
            e.error,
        ),
        Err(_) => prefix,
    }
}
fn cancel_steps(ids: Vec<String>) -> Result<VecDeque<Step>, String> {
    if ids.len() > 10_000 {
        return Err("Coinbase cancel batch exceeds 10000 IDs".into());
    }
    let mut seen = HashSet::new();
    if ids
        .iter()
        .any(|id| !valid_id(id) || !seen.insert(id.as_str()))
    {
        return Err("Coinbase cancellation IDs must be valid and unique".into());
    }
    Ok(ids
        .chunks(100)
        .map(|ids| Step::Cancel { ids: ids.to_vec() })
        .collect())
}
