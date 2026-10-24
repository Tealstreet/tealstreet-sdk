use crate::{
    Orderly, Pending,
    wire::{Id, N, Rest, checked},
};
use safe_cex_engine::Level;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::{
    Command, CommandId, CommandWait, Cx, ErrorKind, Method, Outcome, Response, VenueError, num,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

fn reject_unsupported_order_controls(cx: &mut Cx<'_, Pending>) {
    match cx.command_wait() {
        Some(CommandWait::Remaining(_)) => {}
        Some(CommandWait::Expired) => {
            let _ = cx.reject_expired_command(Operation::Mutation);
            return;
        }
        None => {
            let _ = cx.reject_unstamped_command(Operation::Mutation);
            return;
        }
    }
    let Ok(context) = cx.admit_http_command(Policy {
        operation: Operation::Mutation,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }) else {
        return;
    };
    let _ = cx.fail_context(
        &context,
        not_sent("Bybit BBO and TickSize controls are not supported by Orderly")
            .kind(ErrorKind::Unsupported),
    );
}

pub struct Request {
    pub command: CommandId,
    step: Step,
}
impl Request {
    /// A read (FetchLeverage) has no write outcome to classify.
    pub(crate) fn is_read(&self) -> bool {
        self.step.method == Method::Get
    }
}
struct Step {
    method: Method,
    path: String,
    body: Option<String>,
    effect: Effect,
}
enum Effect {
    Place(Order),
    Pair {
        legs: usize,
        symbol: String,
        side: OrderSide,
        mode: MarginType,
    },
    Cancel {
        orders: Vec<Order>,
        parent: Option<String>,
    },
    Amend {
        id: String,
    },
    Leverage {
        symbol: String,
        mode: MarginType,
        value: f64,
    },
    Margin {
        symbol: String,
        mode: MarginType,
    },
    FetchLeverage {
        symbol: String,
        mode: MarginType,
    },
}
struct Job {
    command: CommandId,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    keep_going: bool,
    error: Option<VenueError>,
    collect: bool,
    total: usize,
    waiting: Option<Waiting>,
    scopes: HashSet<String>,
    superseded: bool,
    /// A write took effect, so a later failure cannot mean nothing happened.
    effective: bool,
}
#[derive(Default)]
pub struct Trading {
    active: Option<Job>,
    queue: VecDeque<Command>,
    detached: Vec<(Waiting, HashSet<String>)>,
}
fn mode(m: MarginType) -> &'static str {
    if m == MarginType::Isolated {
        "ISOLATED"
    } else {
        "CROSS"
    }
}
fn side_name(s: OrderSide) -> &'static str {
    if s == OrderSide::Buy { "BUY" } else { "SELL" }
}
fn trade_id(command: &Command) -> Option<CommandId> {
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
fn live(order: &Order) -> bool {
    matches!(
        order.status,
        OrderStatus::Open | OrderStatus::Creating | OrderStatus::Updating | OrderStatus::Canceling
    )
}
fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap()
}
fn client_id_valid(id: &str) -> bool {
    !id.is_empty()
        && !id.starts_with('-')
        && id.len() <= 36
        && id.is_ascii()
        && !id
            .chars()
            .any(|c| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
}
fn native_id(id: &str) -> Result<(), String> {
    if id.starts_with('0') || id.parse::<u64>().is_err() {
        Err("Orderly requires an exact positive decimal order ID".into())
    } else {
        Ok(())
    }
}
#[derive(Serialize)]
struct Entry<'a> {
    symbol: &'a str,
    order_type: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reduce_only: Option<bool>,
    side: &'a str,
    margin_mode: &'a str,
    order_tag: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_order_id: Option<&'a str>,
    order_quantity: f64,
    broker_id: &'a str,
}
#[derive(Serialize)]
struct Child {
    reduce_only: bool,
    algo_type: &'static str,
    r#type: &'static str,
    side: &'static str,
    trigger_price: String,
}
#[derive(Serialize)]
struct Pair<'a> {
    reduce_only: bool,
    symbol: &'a str,
    algo_type: &'static str,
    margin_mode: &'a str,
    child_orders: Vec<Child>,
    #[serde(rename = "brokerId")]
    broker_id: &'a str,
}

impl Orderly {
    fn broker_id(&self) -> &'static str {
        if self.options.testnet {
            "e708a644-9ce5-46c6-b50c-74b15f62d8ca"
        } else {
            "ab82cb09-cfec-4473-80a3-b740779d0644"
        }
    }
    fn resolve_mode(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
        protective: bool,
    ) -> Result<MarginType, String> {
        if !protective && opts.reduce_only != Some(true) {
            return Ok(self.selected_mode(cx, &opts.symbol));
        }
        let closing = if opts.side == OrderSide::Buy {
            PositionSide::Short
        } else {
            PositionSide::Long
        };
        let rows: Vec<_> = cx
            .store
            .positions()
            .iter()
            .filter(|p| p.symbol == opts.symbol && p.side == closing && p.contracts > 0.0)
            .collect();
        if let Some(selected) = self.account.defaults.get(&opts.symbol)
            && rows
                .iter()
                .filter(|p| p.margin_type == Some(*selected))
                .count()
                == 1
        {
            return Ok(*selected);
        }
        if rows.len() > 1 {
            return Err("Orderly reduce-only order is ambiguous across Cross and Isolated positions; select a margin mode first".into());
        }
        Ok(rows
            .first()
            .and_then(|p| p.margin_type)
            .unwrap_or(self.selected_mode(cx, &opts.symbol)))
    }
    fn pair(
        &self,
        cx: &Cx<'_, Pending>,
        symbol: &str,
        side: OrderSide,
        mode_: MarginType,
        stop: Option<f64>,
        take: Option<f64>,
    ) -> Result<Step, String> {
        let market = cx.store.market(symbol).ok_or("Orderly unknown market")?;
        let mut children = vec![];
        for (kind, price) in [("STOP_LOSS", stop), ("TAKE_PROFIT", take)] {
            if let Some(price) = price {
                let adjusted = num::adjust(price, market.precision.price);
                if !price.is_finite() || adjusted <= 0.0 {
                    return Err("Orderly protective price must be finite and positive".into());
                }
                children.push(Child {
                    reduce_only: true,
                    algo_type: kind,
                    r#type: "CLOSE_POSITION",
                    side: side_name(side),
                    trigger_price: num::format(adjusted),
                });
            }
        }
        if children.is_empty() {
            return Err("Orderly positional order requires a protective price".into());
        }
        let legs = children.len();
        Ok(Step {
            method: Method::Post,
            path: "/v1/algo/order".into(),
            body: Some(json(&Pair {
                reduce_only: true,
                symbol: &self.native[symbol],
                algo_type: "POSITIONAL_TP_SL",
                margin_mode: mode(mode_),
                child_orders: children,
                broker_id: self.broker_id(),
            })),
            effect: Effect::Pair {
                legs,
                symbol: symbol.into(),
                side,
                mode: mode_,
            },
        })
    }
    fn replace_pair(
        &self,
        cx: &Cx<'_, Pending>,
        symbol: &str,
        side: OrderSide,
        mode_: MarginType,
        mut stop: Option<f64>,
        mut take: Option<f64>,
    ) -> Result<VecDeque<Step>, String> {
        let mut siblings = vec![];
        for o in cx.store.orders().iter().filter(|o| {
            o.symbol == symbol
                && o.side == side
                && live(o)
                && matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
        }) {
            let proof = self
                .account
                .proofs
                .get(&o.id)
                .ok_or("Orderly protective order lacks margin provenance")?;
            // Quantity TP_SL and other external algos are independent protections.
            // Only proven full-position legs participate in a parent rebuild.
            if !proof.positional_repair {
                if proof.close_compatible
                    || o.parent_id
                        .as_ref()
                        .is_some_and(|root| self.account.created_pairs.contains_key(root))
                {
                    return Err(
                        "Orderly positional parent lacks consistent full-position provenance"
                            .into(),
                    );
                }
                continue;
            }
            if o.parent_id
                .as_ref()
                .and_then(|root| self.account.created_pairs.get(root))
                .is_some_and(|(_, _, mode)| *mode != proof.mode)
            {
                return Err(
                    "Orderly protective margin provenance conflicts with accepted creation".into(),
                );
            }
            if proof.mode == mode_ {
                siblings.push(o.clone());
            }
        }
        let parents: HashSet<_> = siblings
            .iter()
            .filter_map(|o| o.parent_id.clone())
            .collect();
        if parents.len() > 1
            || siblings.len() > 2
            || siblings
                .iter()
                .any(|o| o.parent_id.is_none() || o.kind == OrderType::TrailingStopLoss)
        {
            return Err(
                "Orderly positional pair is ambiguous; cancel existing parents first".into(),
            );
        }
        if cx.store.orders().iter().any(|order| {
            live(order)
                && order
                    .parent_id
                    .as_ref()
                    .is_some_and(|parent| parents.contains(parent))
                && !siblings.iter().any(|sibling| sibling.id == order.id)
        }) {
            return Err(
                "Orderly cannot rebuild a parent containing unrelated protective siblings".into(),
            );
        }
        let mut seen = HashSet::new();
        for o in &siblings {
            let key = if o.kind == OrderType::StopLoss { 0 } else { 1 };
            if !seen.insert(key) {
                return Err("Orderly duplicate protective sibling".into());
            }
            if key == 0 && stop.is_none() {
                stop = Some(o.price);
            } else if key == 1 && take.is_none() {
                take = Some(o.price);
            }
        }
        let mut steps = VecDeque::new();
        if !siblings.is_empty() {
            steps.push_back(self.cancel(siblings, parents.into_iter().next())?);
        }
        steps.push_back(self.pair(cx, symbol, side, mode_, stop, take)?);
        Ok(steps)
    }
    fn cancel(&self, orders: Vec<Order>, parent: Option<String>) -> Result<Step, String> {
        let first = orders
            .first()
            .ok_or("Orderly missing cancellation identity")?;
        let id = parent.as_deref().unwrap_or(&first.id);
        native_id(id)?;
        let path = format!(
            "/v1/{}order?order_id={id}&symbol={}",
            if parent.is_some() { "algo/" } else { "" },
            self.native[&first.symbol]
        );
        Ok(Step {
            method: Method::Delete,
            path,
            body: None,
            effect: Effect::Cancel { orders, parent },
        })
    }
    fn validate_order(&self, cx: &Cx<'_, Pending>, o: &PlaceOrderOpts) -> Result<(), String> {
        let market = cx.store.market(&o.symbol).ok_or("Orderly unknown symbol")?;
        if (matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
            || o.stop_loss.is_some()
            || o.take_profit.is_some())
            && self
                .trading
                .detached
                .iter()
                .any(|(_, scopes)| scopes.contains(&o.symbol))
        {
            return Err(
                "Orderly protective state is still reconciling after an interrupted command".into(),
            );
        }
        if !market.active
            || !o.amount.is_finite()
            || o.amount <= 0.0
            || num::adjust(o.amount, market.precision.amount) < market.limits.amount.min
        {
            return Err("Orderly amount must meet the market minimum".into());
        }
        if matches!(o.kind, OrderType::TrailingStopLoss | OrderType::Conditional) {
            return Err(
                "Orderly opening triggers and trailing-stop placement are unsupported".into(),
            );
        }
        if o.trigger_price.is_some()
            && !(matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
                && o.trigger_price == o.price)
        {
            return Err("Orderly positional triggerPrice must equal price on SL/TP orders".into());
        }
        if o.close == Some(true) && matches!(o.kind, OrderType::Limit | OrderType::Market) {
            return Err("Orderly close flag is unsupported on ordinary orders".into());
        }
        for trigger in [o.trigger_type, o.tp_trigger_type, o.sl_trigger_type]
            .into_iter()
            .flatten()
        {
            if trigger != StopTriggerType::MarkPrice {
                return Err("Orderly only supports MarkPrice protective triggers".into());
            }
        }
        if o.kind != OrderType::Limit
            && o.time_in_force
                .is_some_and(|t| t != TimeInForce::GoodTillCancel)
        {
            return Err("Orderly time-in-force requires a limit order".into());
        }
        if matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
            && (o.stop_loss.is_some() || o.take_profit.is_some())
        {
            return Err("Orderly stop orders cannot carry another bracket".into());
        }
        for price in [
            if o.kind == OrderType::Market {
                None
            } else {
                o.price
            },
            o.stop_loss,
            o.take_profit,
        ]
        .into_iter()
        .flatten()
        {
            if !price.is_finite() || num::adjust(price, market.precision.price) <= 0.0 {
                return Err("Orderly price must be finite and positive".into());
            }
        }
        if o.kind != OrderType::Market && o.price.is_none() {
            return Err("Orderly price is required".into());
        }
        if let Some(id) = &o.client_order_id
            && (!client_id_valid(id)
                || !matches!(o.kind, OrderType::Limit | OrderType::Market)
                || o.stop_loss.is_some()
                || o.take_profit.is_some()
                || o.amount > market.limits.amount.max)
        {
            return Err("Orderly clientOrderId must be up to36 characters, cannot begin with -, and must identify one ordinary native order".into());
        }
        self.resolve_mode(
            cx,
            o,
            matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit),
        )?;
        Ok(())
    }
    fn plan_orders(
        &self,
        cx: &Cx<'_, Pending>,
        orders: Vec<PlaceOrderOpts>,
    ) -> Result<VecDeque<Step>, String> {
        let mut ids = HashSet::new();
        let mut positional = HashSet::new();
        for o in &orders {
            self.validate_order(cx, o)?;
            if let Some(id) = &o.client_order_id
                && !ids.insert(id.clone())
            {
                return Err("Orderly duplicate caller clientOrderId".into());
            }
        }
        let mut steps = VecDeque::new();
        for o in orders {
            let market = cx.store.market(&o.symbol).unwrap();
            let protective = matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit);
            let mode_ = self.resolve_mode(cx, &o, protective)?;
            if protective {
                if !positional.insert((o.symbol.clone(), mode_ == MarginType::Isolated)) {
                    return Err(
                        "Orderly batch cannot replace the same positional pair twice".into(),
                    );
                }
                steps.extend(self.replace_pair(
                    cx,
                    &o.symbol,
                    o.side,
                    mode_,
                    (o.kind == OrderType::StopLoss).then_some(o.price.unwrap()),
                    (o.kind == OrderType::TakeProfit).then_some(o.price.unwrap()),
                )?);
                continue;
            }
            let amount = num::adjust(o.amount, market.precision.amount);
            let units = (amount / market.precision.amount).round();
            let cap = (market.limits.amount.max / market.precision.amount).floor();
            if !units.is_finite()
                || !cap.is_finite()
                || cap < 1.0
                || units > 9007199254740991.0
                || (units / cap).ceil() > 100.0
            {
                return Err("Orderly order exceeds supported split bounds".into());
            }
            let mut remaining = units;
            let mut index = 0;
            while remaining > 0.0 {
                let lot = remaining.min(cap);
                let qty = num::adjust(lot * market.precision.amount, market.precision.amount);
                if qty < market.limits.amount.min {
                    return Err("Orderly split leaves a below-minimum remainder".into());
                }
                let price = (o.kind == OrderType::Limit)
                    .then(|| num::adjust(o.price.unwrap(), market.precision.price));
                let order_type = if o.kind == OrderType::Market {
                    "MARKET"
                } else {
                    match o.time_in_force.unwrap_or(TimeInForce::GoodTillCancel) {
                        TimeInForce::GoodTillCancel => "LIMIT",
                        TimeInForce::PostOnly => "POST_ONLY",
                        TimeInForce::ImmediateOrCancel => "IOC",
                        TimeInForce::FillOrKill => "FOK",
                    }
                };
                let body = json(&Entry {
                    symbol: &self.native[&o.symbol],
                    order_type,
                    order_price: price,
                    reduce_only: o.reduce_only,
                    side: side_name(o.side),
                    margin_mode: mode(mode_),
                    order_tag: self.options.broker.order_tag(),
                    client_order_id: o.client_order_id.as_deref(),
                    order_quantity: qty,
                    broker_id: self.broker_id(),
                });
                let order = Order {
                    id: format!("pending:{index}"),
                    parent_id: None,
                    status: OrderStatus::Creating,
                    symbol: o.symbol.clone(),
                    kind: o.kind,
                    side: o.side,
                    price: price.unwrap_or(0.0),
                    amount: qty,
                    filled: 0.0,
                    remaining: qty,
                    reduce_only: o.reduce_only.unwrap_or(false),
                    close: None,
                    client_order_id: o.client_order_id.clone(),
                    time_in_force: (o.kind == OrderType::Limit)
                        .then_some(o.time_in_force.unwrap_or(TimeInForce::GoodTillCancel)),
                    last_update: cx.now,
                    trigger_type: None,
                    take_profit: None,
                    stop_loss: None,
                    extra: None,
                };
                steps.push_back(Step {
                    method: Method::Post,
                    path: "/v1/order".into(),
                    body: Some(body),
                    effect: Effect::Place(order),
                });
                remaining -= lot;
                index += 1;
            }
            if o.stop_loss.is_some() || o.take_profit.is_some() {
                if !positional.insert((o.symbol.clone(), mode_ == MarginType::Isolated)) {
                    return Err(
                        "Orderly batch cannot replace the same positional pair twice".into(),
                    );
                }
                steps.extend(self.replace_pair(
                    cx,
                    &o.symbol,
                    o.side.opposite(),
                    mode_,
                    o.stop_loss,
                    o.take_profit,
                )?);
            }
        }
        Ok(steps)
    }
    fn plan_cancel(
        &self,
        cx: &Cx<'_, Pending>,
        ids: Vec<String>,
    ) -> Result<VecDeque<Step>, String> {
        let mut unique = HashSet::new();
        let ids: Vec<_> = ids
            .into_iter()
            .filter(|id| unique.insert(id.clone()))
            .collect();
        let requested: HashSet<_> = ids.iter().cloned().collect();
        let mut steps = VecDeque::new();
        let mut parents = HashSet::new();
        for id in &ids {
            let order = cx
                .store
                .orders()
                .iter()
                .find(|o| &o.id == id && live(o))
                .or_else(|| {
                    cx.store
                        .orders()
                        .iter()
                        .find(|o| o.parent_id.as_ref() == Some(id) && live(o))
                })
                .or_else(|| {
                    cx.store
                        .orders()
                        .iter()
                        .find(|o| &o.id == id || o.parent_id.as_ref() == Some(id))
                })
                .ok_or("Orderly cancel requires a reconciled order")?;
            if !live(order) {
                continue;
            }
            if let Some(parent) = &order.parent_id {
                if !parents.insert(parent.clone()) {
                    continue;
                }
                let siblings: Vec<_> = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| {
                        o.parent_id.as_ref() == Some(parent) && o.symbol == order.symbol && live(o)
                    })
                    .cloned()
                    .collect();
                let mode_ = self
                    .account
                    .proofs
                    .get(&order.id)
                    .ok_or("Orderly algo cancellation requires margin provenance")?
                    .mode;
                let survivors: Vec<_> = siblings
                    .iter()
                    .filter(|o| !requested.contains(parent) && !requested.contains(&o.id))
                    .collect();
                let mut stop = None;
                let mut take = None;
                for o in survivors {
                    if self
                        .account
                        .proofs
                        .get(&o.id)
                        .is_none_or(|p| p.mode != mode_ || !p.positional_repair)
                        || o.side != order.side
                    {
                        return Err("Orderly cannot safely recreate this sibling; cancel the whole parent instead".into());
                    }
                    match o.kind {
                        OrderType::StopLoss => {
                            if stop.replace(o.price).is_some() {
                                return Err("Orderly duplicate protective sibling".into());
                            }
                        }
                        OrderType::TakeProfit => {
                            if take.replace(o.price).is_some() {
                                return Err("Orderly duplicate protective sibling".into());
                            }
                        }
                        _ => return Err("Orderly cannot safely recreate this algo sibling".into()),
                    }
                }
                steps.push_back(self.cancel(siblings, Some(parent.clone()))?);
                if stop.is_some() || take.is_some() {
                    steps.push_back(self.pair(cx, &order.symbol, order.side, mode_, stop, take)?);
                }
            } else {
                steps.push_back(self.cancel(vec![order.clone()], None)?);
            }
        }
        Ok(steps)
    }
    fn plan_amend(
        &self,
        cx: &Cx<'_, Pending>,
        id: String,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<VecDeque<Step>, String> {
        let o = cx
            .store
            .orders()
            .iter()
            .find(|o| o.id == id)
            .ok_or("Orderly amend requires one reconciled order")?;
        if !live(o) || price.is_some() == amount.is_some() {
            return Err("Orderly amend requires one real price or quantity change".into());
        }
        native_id(&id)?;
        let market = cx.store.market(&o.symbol).unwrap();
        let mut steps = VecDeque::new();
        if let Some(parent) = &o.parent_id {
            native_id(parent)?;
            let p = price.ok_or("Orderly positional stops cannot amend quantity")?;
            if !matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
                || !p.is_finite()
                || p <= 0.0
                || num::adjust(p, market.precision.price) != p
                || p == o.price
            {
                return Err(
                    "Orderly algo amendment requires a new aligned protective price".into(),
                );
            }
            #[derive(Serialize)]
            struct Child<'a> {
                order_id: &'a str,
                trigger_price: f64,
            }
            #[derive(Serialize)]
            struct Edit<'a> {
                order_id: u64,
                child_orders: [Child<'a>; 1],
            }
            steps.push_back(Step {
                method: Method::Put,
                path: "/v1/algo/order".into(),
                body: Some(json(&Edit {
                    order_id: parent.parse().unwrap(),
                    child_orders: [Child {
                        order_id: &id,
                        trigger_price: p,
                    }],
                })),
                effect: Effect::Amend { id },
            });
            return Ok(steps);
        }
        let proof = self
            .account
            .proofs
            .get(&id)
            .ok_or("Orderly native amend requires complete REST provenance")?;
        if !proof.amend
            || o.kind != OrderType::Limit
            || o.status != OrderStatus::Open
            || o.filled != 0.0
            || o.remaining != o.amount
            || o.time_in_force != Some(TimeInForce::GoodTillCancel)
        {
            return Err("Orderly native amend requires a REST-proven pending zero-fill ordinary GTC LIMIT order".into());
        }
        let (value, old, step) = if let Some(p) = price {
            (p, o.price, market.precision.price)
        } else {
            (amount.unwrap(), o.amount, market.precision.amount)
        };
        if !value.is_finite()
            || value <= 0.0
            || value == old
            || num::adjust(value, step) != value
            || amount.is_some_and(|q| q < market.limits.amount.min || q > market.limits.amount.max)
        {
            return Err("Orderly amendment must be new, aligned and within market limits".into());
        }
        #[derive(Serialize)]
        struct Edit<'a> {
            order_id: &'a str,
            symbol: &'a str,
            order_type: &'a str,
            side: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            order_price: Option<f64>,
            #[serde(skip_serializing_if = "Option::is_none")]
            order_quantity: Option<f64>,
        }
        steps.push_back(Step {
            method: Method::Put,
            path: "/v1/order".into(),
            body: Some(json(&Edit {
                order_id: &id,
                symbol: &self.native[&o.symbol],
                order_type: "LIMIT",
                side: side_name(o.side),
                order_price: price,
                order_quantity: amount,
            })),
            effect: Effect::Amend { id },
        });
        Ok(steps)
    }
    fn plan_trade(&self, cx: &Cx<'_, Pending>, command: Command) -> Result<Job, String> {
        let id = trade_id(&command).unwrap();
        let scopes: HashSet<String> = match &command {
            Command::PlaceOrders { orders, .. } => {
                orders.iter().map(|o| o.symbol.clone()).collect()
            }
            Command::CancelOrders { ids, .. } => ids
                .iter()
                .filter_map(|id| {
                    cx.store
                        .orders()
                        .iter()
                        .find(|o| o.id == *id || o.parent_id.as_ref() == Some(id))
                        .map(|o| o.symbol.clone())
                })
                .collect(),
            Command::AmendOrder { id, .. } => cx
                .store
                .orders()
                .iter()
                .find(|o| o.id == *id)
                .map(|o| HashSet::from([o.symbol.clone()]))
                .unwrap_or_default(),
            Command::CancelSymbolOrders { symbol, .. }
            | Command::SetLeverage { symbol, .. }
            | Command::SetMarginType { symbol, .. }
            | Command::FetchLeverage { symbol, .. } => HashSet::from([symbol.clone()]),
            _ => HashSet::from([String::new()]),
        };

        let mut keep_going = false;
        let collect = matches!(
            command,
            Command::PlaceOrders { .. } | Command::AmendOrder { .. }
        );
        let steps = match command {
            Command::PlaceOrders { orders, .. } => self.plan_orders(cx, orders)?,
            Command::CancelOrders { ids, .. } => self.plan_cancel(cx, ids)?,
            Command::AmendOrder {
                id, price, amount, ..
            } => self.plan_amend(cx, id, price, amount)?,
            Command::CancelSymbolOrders { symbol, .. } => {
                if !symbol.is_empty() && !self.native.contains_key(&symbol) {
                    return Err("Orderly unknown cancel symbol".into());
                }
                keep_going = true;
                [false, true]
                    .into_iter()
                    .map(|algo| {
                        let orders = cx
                            .store
                            .orders()
                            .iter()
                            .filter(|o| {
                                (symbol.is_empty() || o.symbol == symbol)
                                    && o.parent_id.is_some() == algo
                                    && live(o)
                            })
                            .cloned()
                            .collect();
                        Step {
                            method: Method::Delete,
                            path: format!(
                                "/v1/{}orders{}",
                                if algo { "algo/" } else { "" },
                                if symbol.is_empty() {
                                    String::new()
                                } else {
                                    format!("?symbol={}", self.native[&symbol])
                                }
                            ),
                            body: None,
                            effect: Effect::Cancel {
                                orders,
                                parent: None,
                            },
                        }
                    })
                    .collect()
            }
            Command::SetLeverage {
                symbol, leverage, ..
            } => {
                if !leverage.is_finite() {
                    return Err("Orderly leverage must be finite".into());
                }
                let (lo, hi, mode_) = if symbol.is_empty() {
                    (1.0, 100.0, MarginType::Cross)
                } else {
                    let m = cx.store.market(&symbol).ok_or("Orderly unknown symbol")?;
                    (
                        m.limits.leverage.min,
                        m.limits.leverage.max,
                        self.selected_mode(cx, &symbol),
                    )
                };
                let value = num::round_half_up(num::clamp_leverage(leverage, lo, hi)?);
                #[derive(Serialize)]
                struct Set<'a> {
                    leverage: f64,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    symbol: Option<&'a str>,
                    margin_mode: &'a str,
                }
                VecDeque::from([Step {
                    method: Method::Post,
                    path: "/v1/client/leverages".into(),
                    body: Some(json(&Set {
                        leverage: value,
                        symbol: self.native.get(&symbol).map(String::as_str),
                        margin_mode: mode(mode_),
                    })),
                    effect: Effect::Leverage {
                        symbol,
                        mode: mode_,
                        value,
                    },
                }])
            }
            Command::SetMarginType {
                symbol, isolated, ..
            } => {
                let native = self
                    .native
                    .get(&symbol)
                    .ok_or("Orderly margin type requires a known symbol")?;
                let mode_ = if isolated {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                };
                #[derive(Serialize)]
                struct Set<'a> {
                    symbol: &'a str,
                    default_margin_mode: &'a str,
                }
                VecDeque::from([Step {
                    method: Method::Post,
                    path: "/v1/client/margin_mode".into(),
                    body: Some(json(&Set {
                        symbol: native,
                        default_margin_mode: mode(mode_),
                    })),
                    effect: Effect::Margin {
                        symbol,
                        mode: mode_,
                    },
                }])
            }
            Command::SetPositionMode { hedged, .. } => {
                if hedged {
                    return Err("Orderly position mode is fixed OneWay".into());
                }
                VecDeque::new()
            }
            Command::FetchLeverage { symbol, .. } => {
                let native = self
                    .native
                    .get(&symbol)
                    .ok_or("Orderly unknown leverage symbol")?;
                let mode_ = self.selected_mode(cx, &symbol);
                VecDeque::from([Step {
                    method: Method::Get,
                    path: format!(
                        "/v1/client/leverage?symbol={native}&margin_mode={}",
                        mode(mode_)
                    ),
                    body: None,
                    effect: Effect::FetchLeverage {
                        symbol,
                        mode: mode_,
                    },
                }])
            }
            _ => return Err("Orderly invalid trading command".into()),
        };
        if steps
            .iter()
            .any(|s| matches!(s.effect, Effect::Pair { .. }))
            && (self.trading.detached.len() >= 100
                || self
                    .trading
                    .detached
                    .iter()
                    .any(|(_, blocked)| scopes.iter().any(|s| blocked.contains(s))))
        {
            return Err(
                "Orderly protective state requires reconciliation before another write".into(),
            );
        }
        let total = steps.len();
        Ok(Job {
            command: id,
            steps,
            ids: vec![],
            keep_going,
            error: None,
            collect,
            total,
            waiting: None,
            scopes,
            superseded: false,
            effective: false,
        })
    }
    pub(crate) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let Some(id) = trade_id(&command) else {
            return;
        };
        if let Command::PlaceOrders { orders, .. } = &command
            && orders
                .iter()
                .any(|o| o.bybit_bbo.is_some() || o.bybit_slippage_tolerance_ticks.is_some())
        {
            reject_unsupported_order_controls(cx);
            return;
        }
        if self.account.credentials.is_none()
            || !cx.store.loaded().orders
            || !cx.store.loaded().positions
            || !cx.store.loaded().balance
        {
            cx.fail(id, not_sent("Orderly private account is not ready"));
            return;
        }
        if let Command::CancelSymbolOrders { symbol, .. } = &command {
            let mut keep = VecDeque::new();
            for queued in self.trading.queue.drain(..) {
                let affected = match &queued {
                    Command::PlaceOrders { orders, .. } => orders
                        .iter()
                        .any(|o| symbol.is_empty() || o.symbol == *symbol),
                    Command::CancelOrders { ids, .. } => ids.iter().any(|id| {
                        symbol.is_empty()
                            || cx
                                .store
                                .orders()
                                .iter()
                                .find(|o| o.id == *id || o.parent_id.as_ref() == Some(id))
                                .is_none_or(|o| o.symbol == *symbol)
                    }),
                    Command::AmendOrder { id, .. } => {
                        symbol.is_empty()
                            || cx
                                .store
                                .orders()
                                .iter()
                                .find(|o| o.id == *id)
                                .is_none_or(|o| o.symbol == *symbol)
                    }
                    _ => false,
                };
                if affected {
                    cx.fail(
                        trade_id(&queued).unwrap(),
                        not_sent(
                            "Orderly queued order mutation was superseded by bulk cancellation",
                        ),
                    );
                } else {
                    keep.push_back(queued);
                }
            }
            self.trading.queue = keep;
            if let Some(job) = self.trading.active.as_mut() {
                let affected =
                    symbol.is_empty() || job.scopes.contains(symbol) || job.scopes.contains("");
                let waiting = if affected && !job.keep_going {
                    job.superseded = true;
                    job.steps.clear();
                    // The current step may already be in flight, so nothing is proven.
                    job.error = Some(VenueError::new(
                        "Orderly remaining writes were superseded by bulk cancellation",
                    ));
                    job.waiting.take()
                } else {
                    None
                };
                self.trading.queue.push_front(command);
                if let Some(wait) = waiting {
                    cx.clear_timer(wait.timer);
                    let scopes = self.trading.active.as_ref().unwrap().scopes.clone();
                    self.trading.detached.push((wait, scopes));
                    self.trade_next(cx);
                }
                return;
            }
        }
        if self.trading.queue.len() >= 100 {
            cx.fail(id, not_sent("Orderly trading queue is full"));
            return;
        }
        if self.trading.active.is_some() {
            self.trading.queue.push_back(command);
            return;
        }
        match self.plan_trade(cx, command) {
            Ok(job) => {
                self.trading.active = Some(job);
                self.trade_next(cx);
            }
            Err(e) => {
                cx.command_error(id, &e);
                cx.fail(id, not_sent(e));
                self.trade_queued(cx);
            }
        }
    }
    fn trade_queued(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(command) = self.trading.queue.pop_front() {
            self.trade_command(cx, command);
        }
    }
    fn trade_next(&mut self, cx: &mut Cx<'_, Pending>) {
        let Some(job) = self.trading.active.as_mut() else {
            return;
        };
        let Some(mut step) = job.steps.pop_front() else {
            let job = self.trading.active.take().unwrap();
            if let Some(mut error) = job.error {
                if (job.effective || !job.ids.is_empty())
                    && matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent))
                {
                    error.outcome = None;
                }
                cx.fail(
                    job.command,
                    error.partial(&job.ids, job.total, job.ids.len()),
                );
            } else {
                cx.done(
                    job.command,
                    if job.collect {
                        json(&job.ids)
                    } else {
                        "[]".into()
                    },
                );
            }
            self.trade_queued(cx);
            return;
        };
        let command = job.command;
        match &mut step.effect {
            Effect::Place(order) => {
                order.id = format!("pending:{command}");
                cx.store.upsert_order(order.clone());
                self.account.order_revisions[0] += 1;
                self.account.positions_revision += 1;
            }
            Effect::Pair { .. } => {
                self.account.order_revisions[1] += 1;
            }
            Effect::Cancel { orders, .. } => {
                for o in orders {
                    cx.store.update_order(&o.id, |p| {
                        if live(p) {
                            p.status = OrderStatus::Canceling;
                        }
                    });
                }
                self.account.order_revisions[0] += 1;
                self.account.order_revisions[1] += 1;
            }
            Effect::Amend { id } => {
                cx.store
                    .update_order(id, |o| o.status = OrderStatus::Updating);
                self.account.order_revisions[0] += 1;
                self.account.order_revisions[1] += 1;
                self.account.proofs.remove(id);
            }
            Effect::Leverage { .. } | Effect::Margin { .. } => {
                self.account.settings_revision += 1;
                self.account.positions_revision += 1;
            }
            Effect::FetchLeverage { .. } => {}
        }
        let credentials = self.account.credentials.as_ref().unwrap();
        match credentials.headers(step.method, &step.path, step.body.as_deref(), cx.now) {
            Ok(headers) => {
                if let Effect::Place(order) = &step.effect
                    && let Some(identity) = &order.client_order_id
                {
                    cx.command_progress(
                        command,
                        vec![OrderIdentityReceipt {
                            symbol: order.symbol.clone(),
                            kind: OrderIdentityKind::ClientOrderId,
                            identity: identity.clone(),
                        }],
                    );
                }
                cx.request(
                    step.method,
                    format!("{}{}", self.rest(), step.path),
                    headers,
                    step.body.clone(),
                    Pending::Trade(Request { command, step }),
                );
            }
            Err(e) => self.trade_failed(cx, Request { command, step }, not_sent(e)),
        }
    }
    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let write = request.step.method != Method::Get;
        if write && (response.status == 408 || (500..600).contains(&response.status)) {
            let error = format!("Orderly HTTP {}: {}", response.status, response.body);
            self.trade_lost(
                cx,
                request,
                VenueError::new(error).outcome(Outcome::Unknown),
            );
            return;
        }
        // A successful envelope with an unusable ACK cannot prove a write was rejected.
        let unknown_ack = write
            && (200..300).contains(&response.status)
            && serde_json::from_str::<Rest<'_>>(response.body).map_or(true, |rest| rest.success);
        let result = (|| -> Result<(), String> {
            let rest = checked(response)?;
            let data = rest.data.map_or("{}", |d| d.get());
            match &request.step.effect {
                Effect::Place(order) => {
                    #[derive(Deserialize)]
                    struct Created {
                        order_id: Id,
                    }
                    let r: Created = serde_json::from_str(data).map_err(|e| e.to_string())?;
                    native_id(&r.order_id.0)?;
                    cx.store.remove_orders(&[&order.id]);
                    let mut placed = order.clone();
                    placed.id = r.order_id.0.clone();
                    placed.status = OrderStatus::Open;
                    if !self.order_terminal(&placed.id)
                        && !cx.store.orders().iter().any(|o| o.id == placed.id)
                    {
                        cx.store.upsert_order(placed);
                    }
                    self.account.order_revisions[0] += 1;
                    self.trading.active.as_mut().unwrap().ids.push(r.order_id.0);
                }
                Effect::Pair {
                    legs,
                    symbol,
                    side,
                    mode,
                } => {
                    #[derive(Deserialize)]
                    struct Row {
                        #[serde(rename = "orderId")]
                        id: Id,
                    }
                    #[derive(Deserialize)]
                    struct Created {
                        order_id: Option<Id>,
                        #[serde(default)]
                        rows: Vec<Row>,
                    }
                    let r: Created = serde_json::from_str(data).map_err(|e| e.to_string())?;
                    let parent = r
                        .order_id
                        .as_ref()
                        .map(|id| id.0.clone())
                        .unwrap_or_default();
                    let ids = if let Some(id) = r.order_id {
                        vec![id.0]
                    } else {
                        r.rows.into_iter().map(|r| r.id.0).collect()
                    };
                    if ids.is_empty() {
                        return Err("Orderly algo success omitted native order IDs".into());
                    }
                    for id in &ids {
                        native_id(id)?;
                    }
                    if !parent.is_empty() {
                        if self.account.created_pairs.len() >= 1000 {
                            self.account.created_pairs.clear();
                        }
                        self.account
                            .created_pairs
                            .insert(parent.clone(), (symbol.clone(), *side, *mode));
                        for order in cx.store.orders().iter().filter(|o| {
                            o.parent_id.as_ref() == Some(&parent)
                                && o.symbol == *symbol
                                && o.side == *side
                                && matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit)
                        }) {
                            if let Some(proof) = self.account.proofs.get_mut(&order.id)
                                && proof.close_compatible
                                && (!proof.mode_known || proof.mode == *mode)
                            {
                                proof.mode = *mode;
                                proof.mode_known = true;
                                proof.positional_repair = true;
                            }
                        }
                    }
                    self.trading
                        .active
                        .as_mut()
                        .unwrap()
                        .ids
                        .extend(ids.clone());
                    self.account.order_revisions[1] += 1;
                    if !self.pair_known(cx, &parent, &ids, *legs) {
                        let wait = Waiting {
                            parent,
                            ids,
                            legs: *legs,
                            present: true,
                            snapshot: self.account.algo_snapshots,
                            timer: cx.set_timer(250.0),
                            deadline: cx.now + 20000.0,
                        };
                        let job = self.trading.active.as_mut().unwrap();
                        if job.superseded {
                            cx.clear_timer(wait.timer);
                            self.trading.detached.push((wait, job.scopes.clone()));
                        } else {
                            job.waiting = Some(wait);
                        }
                        self.account_poll(cx, true);
                    }
                }
                Effect::Cancel { orders, parent } => {
                    #[derive(Deserialize)]
                    struct Ack<'a> {
                        #[serde(default, borrow)]
                        status: std::borrow::Cow<'a, str>,
                    }
                    let ack: Ack<'_> = serde_json::from_str(data).map_err(|e| e.to_string())?;
                    if !matches!(
                        ack.status.as_ref(),
                        "CANCEL_SENT" | "CANCEL_ALL_SENT" | "CANCELED" | "CANCELLED"
                    ) {
                        return Err(
                            "Orderly cancellation success omitted a valid native ACK".into()
                        );
                    }
                    self.account.order_revisions[0] += 1;
                    self.account.order_revisions[1] += 1;
                    if matches!(ack.status.as_ref(), "CANCELED" | "CANCELLED") {
                        if let Some(parent) = parent {
                            self.terminal_order(&format!("root:{parent}"));
                        }
                        for o in orders {
                            self.terminal_order(&o.id);
                            cx.store.update_order(&o.id, |p| {
                                if p.status != OrderStatus::Closed {
                                    p.status = OrderStatus::Canceled;
                                }
                            });
                        }
                    }
                    if let Some(parent) = parent
                        && self
                            .trading
                            .active
                            .as_ref()
                            .unwrap()
                            .steps
                            .front()
                            .is_some_and(|s| matches!(s.effect, Effect::Pair { .. }))
                        && !self.root_finished(cx, parent)
                    {
                        let wait = Waiting {
                            parent: parent.clone(),
                            ids: vec![],
                            legs: 0,
                            present: false,
                            snapshot: self.account.algo_snapshots,
                            timer: cx.set_timer(250.0),
                            deadline: cx.now + 20000.0,
                        };
                        self.trading.active.as_mut().unwrap().waiting = Some(wait);
                        self.account_poll(cx, true);
                    }
                }
                Effect::Amend { id } => {
                    #[derive(Deserialize)]
                    struct Ack<'a> {
                        #[serde(default, borrow)]
                        status: std::borrow::Cow<'a, str>,
                    }
                    if request.step.path == "/v1/order"
                        && serde_json::from_str::<Ack<'_>>(data)
                            .map_err(|e| e.to_string())?
                            .status
                            != "EDIT_SENT"
                    {
                        return Err("Orderly native amend requires EDIT_SENT".into());
                    }
                    cx.store.update_order(id, |o| {
                        if o.status == OrderStatus::Updating {
                            o.status = OrderStatus::Open;
                        }
                    });
                    self.account.order_revisions[0] += 1;
                    self.account.order_revisions[1] += 1;
                    self.trading.active.as_mut().unwrap().ids.push(id.clone());
                }
                Effect::Leverage {
                    symbol,
                    mode: mode_,
                    value,
                } => {
                    self.account.settings_revision += 1;
                    self.account.positions_revision += 1;
                    if symbol.is_empty() {
                        let previous = self.account.fallback_leverage;
                        self.account.fallback_leverage = *value;
                        for s in cx
                            .store
                            .markets()
                            .iter()
                            .map(|m| m.symbol.clone())
                            .collect::<Vec<_>>()
                        {
                            self.account
                                .leverage
                                .entry((s.clone(), true))
                                .or_insert(previous);
                            let m = cx.store.market(&s).unwrap();
                            let effective = match num::clamp_leverage(
                                *value,
                                m.limits.leverage.min,
                                m.limits.leverage.max,
                            ) {
                                Ok(effective) => effective,
                                Err(_) => {
                                    // The account write succeeded independently of market metadata.
                                    cx.notice(Level::Warn, code::SETTING_CAUTION, format!("Orderly invalid leverage limits for {s}; using confirmed account leverage"));
                                    *value
                                }
                            };
                            self.account.leverage.insert((s.clone(), false), effective);
                            self.sync_setting(cx, &s);
                        }
                    } else {
                        self.account
                            .leverage
                            .insert((symbol.clone(), *mode_ == MarginType::Isolated), *value);
                        self.sync_setting(cx, symbol);
                    }
                    cx.store.update_positions(|p| {
                        if (symbol.is_empty() || p.symbol == *symbol)
                            && p.margin_type == Some(*mode_)
                        {
                            p.leverage = self
                                .account
                                .leverage
                                .get(&(p.symbol.clone(), *mode_ == MarginType::Isolated))
                                .copied()
                                .unwrap_or(*value);
                        }
                    });
                }
                Effect::Margin {
                    symbol,
                    mode: mode_,
                } => {
                    self.account.settings_revision += 1;
                    self.account.defaults.insert(symbol.clone(), *mode_);
                    self.sync_setting(cx, symbol);
                }
                Effect::FetchLeverage {
                    symbol,
                    mode: mode_,
                } => {
                    #[derive(Deserialize)]
                    struct Leverage {
                        #[serde(alias = "leverage")]
                        max_leverage: N,
                    }
                    let value = serde_json::from_str::<Leverage>(data)
                        .map_err(|e| e.to_string())?
                        .max_leverage
                        .0;
                    if value <= 0.0 {
                        return Err("Orderly invalid symbol leverage".into());
                    }
                    self.account
                        .leverage
                        .insert((symbol.clone(), *mode_ == MarginType::Isolated), value);
                    self.sync_setting(cx, symbol);
                    #[derive(Serialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Rails {
                        buy_leverage: f64,
                        sell_leverage: f64,
                    }
                    let job = self.trading.active.take().unwrap();
                    cx.done(
                        job.command,
                        json(&Rails {
                            buy_leverage: value,
                            sell_leverage: value,
                        }),
                    );
                    self.trade_queued(cx);
                    return Ok(());
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            if unknown_ack {
                self.trade_lost(cx, request, VenueError::new(e).outcome(Outcome::Unknown));
            } else {
                let mut error = answer_failure(response, e);
                if !write {
                    // A read has no write outcome to classify.
                    error.outcome = None;
                    self.trade_failed(cx, request, error);
                } else if error.outcome == Some(Outcome::Unknown) {
                    // Server-side codes may follow execution: reconcile like a lost write.
                    self.trade_lost(cx, request, error);
                } else {
                    self.trade_failed(cx, request, error);
                }
            }
            return;
        }
        if write
            && let Some(job) = self
                .trading
                .active
                .as_mut()
                .filter(|j| j.command == request.command)
        {
            job.effective = true;
        }
        if self
            .trading
            .active
            .as_ref()
            .is_some_and(|j| j.command == request.command && j.waiting.is_none())
        {
            self.trade_next(cx);
        }
    }
    pub(crate) fn trade_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        self.finish_failed_trade(cx, request, error, false);
    }
    pub(crate) fn trade_lost(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        self.finish_failed_trade(cx, request, error, true);
        self.account_after_uncertain_write(cx);
    }
    fn finish_failed_trade(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
        uncertain: bool,
    ) {
        match request.step.effect {
            Effect::Place(order) if !uncertain => cx.store.remove_orders(&[&order.id]),
            Effect::Place(order) => {
                self.account.uncertain_placements.insert(order.id);
            }
            Effect::Cancel { orders, .. } if !uncertain => {
                for original in orders {
                    cx.store.update_order(&original.id, |o| {
                        if o.status == OrderStatus::Canceling {
                            o.status = original.status;
                        }
                    });
                }
            }
            Effect::Pair { legs, .. } => {
                if let Some(job) = self
                    .trading
                    .active
                    .as_ref()
                    .filter(|j| j.command == request.command)
                {
                    // A lost response cannot prove the pair was rejected.
                    let timer = cx.set_timer(250.0);
                    cx.clear_timer(timer);
                    self.trading.detached.push((
                        Waiting {
                            parent: String::new(),
                            ids: vec![],
                            legs,
                            present: true,
                            snapshot: self.account.algo_snapshots,
                            timer,
                            deadline: cx.now,
                        },
                        job.scopes.clone(),
                    ));
                    self.account.order_revisions[1] += 1;
                    self.account_poll(cx, true);
                }
            }
            Effect::Amend { id } if !uncertain => {
                cx.store.update_order(&id, |o| {
                    if o.status == OrderStatus::Updating {
                        o.status = OrderStatus::Open;
                    }
                });
            }
            _ => {}
        }
        cx.command_error(request.command, &error.message);
        let Some(job) = self
            .trading
            .active
            .as_mut()
            .filter(|j| j.command == request.command)
        else {
            return;
        };
        // The first failure names the error; any lost leg makes the outcome unknown.
        let lost = uncertain || error.outcome == Some(Outcome::Unknown);
        let first = job.error.get_or_insert(error);
        if lost {
            first.outcome = Some(Outcome::Unknown);
        }
        // Bulk cancellation's two families are independent writes, not replacements.
        if !job.keep_going {
            job.steps.clear();
        }
        self.trade_next(cx);
    }
    fn root_finished(&self, _cx: &Cx<'_, Pending>, parent: &str) -> bool {
        self.root_terminal(parent)
    }
    fn pair_known(&self, cx: &Cx<'_, Pending>, parent: &str, ids: &[String], legs: usize) -> bool {
        if !parent.is_empty() {
            return self.root_terminal(parent)
                || cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.parent_id.as_deref() == Some(parent))
                    .count()
                    >= legs;
        }
        ids.len() >= legs
            && ids.iter().all(|id| {
                self.order_terminal(id)
                    || cx
                        .store
                        .orders()
                        .iter()
                        .any(|o| o.id == *id && o.parent_id.is_some())
            })
    }
    fn waiting_satisfied(&self, cx: &Cx<'_, Pending>, w: &Waiting, detached: bool) -> bool {
        if w.present && w.parent.is_empty() && w.ids.is_empty() {
            return detached && self.account.algo_snapshots > w.snapshot;
        }
        if w.present {
            self.pair_known(cx, &w.parent, &w.ids, w.legs)
                || (detached
                    && self.account.algo_snapshots > w.snapshot
                    && !cx.store.orders().iter().any(|o| {
                        live(o)
                            && (w.ids.contains(&o.id)
                                || o.parent_id.as_ref().is_some_and(|p| w.ids.contains(p)))
                    }))
        } else {
            self.root_terminal(&w.parent)
                || (self.account.algo_snapshots > w.snapshot
                    && !cx
                        .store
                        .orders()
                        .iter()
                        .any(|o| o.parent_id.as_deref() == Some(&w.parent) && live(o)))
        }
    }
    fn wait_satisfied(&self, cx: &Cx<'_, Pending>) -> bool {
        self.trading
            .active
            .as_ref()
            .and_then(|j| j.waiting.as_ref())
            .is_some_and(|w| self.waiting_satisfied(cx, w, false))
    }
    pub(crate) fn trade_reconcile(&mut self, cx: &mut Cx<'_, Pending>) {
        let detached = std::mem::take(&mut self.trading.detached);
        self.trading.detached = detached
            .into_iter()
            .filter(|(w, _)| !self.waiting_satisfied(cx, w, true))
            .collect();
        if self.wait_satisfied(cx) {
            let wait = self
                .trading
                .active
                .as_mut()
                .unwrap()
                .waiting
                .take()
                .unwrap();
            cx.clear_timer(wait.timer);
            self.trade_next(cx);
        }
    }
    pub(crate) fn trade_timer(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        timer: safe_cex_engine::TimerId,
    ) -> bool {
        let Some(wait) = self
            .trading
            .active
            .as_ref()
            .and_then(|j| j.waiting.as_ref())
            .filter(|w| w.timer == timer)
        else {
            return false;
        };
        if cx.now >= wait.deadline {
            let job = self.trading.active.as_mut().unwrap();
            let wait = job.waiting.take().unwrap();
            self.trading.detached.push((wait, job.scopes.clone()));
            job.steps.clear();
            job.error = Some(
                VenueError::new(
                    "Orderly protective state is unconfirmed; no subsequent write was sent",
                )
                .outcome(Outcome::Unknown),
            );
            self.trade_next(cx);
        } else if self.wait_satisfied(cx) {
            self.trade_reconcile(cx);
        } else {
            self.trading
                .active
                .as_mut()
                .unwrap()
                .waiting
                .as_mut()
                .unwrap()
                .timer = cx.set_timer(250.0);
        }
        true
    }
}
struct Waiting {
    parent: String,
    ids: Vec<String>,
    legs: usize,
    present: bool,
    snapshot: u64,
    timer: safe_cex_engine::TimerId,
    deadline: f64,
}

/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

/// An explicit refusal (`success: false` or a 4xx). Orderly's server-side and
/// duplicate-request codes do not prove nothing happened.
fn answer_failure(response: Response<'_>, message: String) -> VenueError {
    #[derive(Deserialize)]
    struct Code {
        code: Option<i64>,
    }
    let error = VenueError::new(message);
    let code = serde_json::from_str::<Code>(response.body)
        .ok()
        .and_then(|c| c.code);
    let error = match code {
        Some(code) => error.venue_code(code.to_string()),
        None => error,
    };
    let rejected = |error: VenueError, kind| error.kind(kind).outcome(Outcome::Rejected);
    match (response.status, code) {
        (_, Some(-1000 | -1011 | -1012)) => error.outcome(Outcome::Unknown),
        (_, Some(-1007)) => error,
        (401, _) | (_, Some(-1001 | -1002)) => rejected(error, ErrorKind::AuthInvalid),
        (403, _) => rejected(error, ErrorKind::AuthPermission),
        (429, _) | (_, Some(-1003)) => rejected(error, ErrorKind::RateLimited),
        (400..=499, _) | (_, Some(-1004 | -1005 | -1006 | -1008 | -1103..=-1101)) => {
            rejected(error, ErrorKind::Rejected)
        }
        // A 2xx `success: false` without a documented code.
        _ if serde_json::from_str::<crate::wire::Rest<'_>>(response.body).is_ok() => error,
        _ => error.outcome(Outcome::Unknown),
    }
}
