//! Each command is a preflighted sequence of writes. A lost write response is
//! terminal for that job: it is never replayed.
use crate::account::Next;
use crate::sign::Params;
use crate::{Backpack, Pending, REST};
use safe_cex_engine::model::*;
use safe_cex_engine::{Command, CommandId, Cx, LogLevel, Method, Response, num};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeOrder {
    symbol: String,
    side: &'static str,
    order_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quantity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_quantity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_by: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_in_force: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    post_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reduce_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    take_profit_trigger_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    take_profit_trigger_by: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_loss_trigger_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_loss_trigger_by: Option<&'static str>,
}
impl NativeOrder {
    pub(crate) fn params(&self) -> Params {
        let mut p: Params = [
            ("symbol".into(), self.symbol.clone()),
            ("side".into(), self.side.into()),
            ("orderType".into(), self.order_type.into()),
        ]
        .into();
        macro_rules! field {
            ($key:literal,$value:expr) => {
                if let Some(value) = $value {
                    p.insert($key.into(), value.to_string());
                }
            };
        }
        field!("price", self.price.as_deref());
        field!("quantity", self.quantity.as_deref());
        field!("triggerPrice", self.trigger_price.as_deref());
        field!("triggerQuantity", self.trigger_quantity.as_deref());
        field!("triggerBy", self.trigger_by);
        field!("timeInForce", self.time_in_force);
        field!("postOnly", self.post_only);
        field!("reduceOnly", self.reduce_only);
        field!("clientId", self.client_id);
        field!(
            "takeProfitTriggerPrice",
            self.take_profit_trigger_price.as_deref()
        );
        field!("takeProfitTriggerBy", self.take_profit_trigger_by);
        field!(
            "stopLossTriggerPrice",
            self.stop_loss_trigger_price.as_deref()
        );
        field!("stopLossTriggerBy", self.stop_loss_trigger_by);
        p
    }
}
#[derive(Clone)]
pub enum Step {
    Place {
        order: Box<NativeOrder>,
        optional: bool,
    },
    Pair(Vec<NativeOrder>),
    Cancel {
        symbol: String,
        id: String,
        optional: bool,
    },
    CancelAll {
        symbol: String,
    },
    Leverage(f64),
}
pub struct Request {
    command: u64,
    step: Step,
    previous: Option<OrderStatus>,
    targets: Vec<(String, f64)>,
    leverage_revision: u64,
}
pub struct Repair {
    old_id: String,
    order: NativeOrder,
}
struct Job {
    steps: VecDeque<Step>,
    ids: Vec<String>,
    collect_ids: bool,
    internal: bool,
    replaces: Option<String>,
}
#[derive(Default)]
pub struct Trading {
    jobs: HashMap<u64, Job>,
    next_client: u32,
    cancels: HashMap<String, (usize, OrderStatus)>,
}

fn client_id(id: &str) -> Result<u32, String> {
    let n = id.parse::<u32>().map_err(|_| {
        "Backpack clientOrderId must be a canonical uint32 decimal string".to_owned()
    })?;
    if n == 0 || n.to_string() != id {
        return Err("Backpack clientOrderId must be a canonical uint32 decimal string".into());
    }
    Ok(n)
}
fn trigger(kind: Option<StopTriggerType>) -> &'static str {
    match kind.unwrap_or(StopTriggerType::MarkPrice) {
        StopTriggerType::MarkPrice => "MarkPrice",
        StopTriggerType::LastPrice => "LastPrice",
        StopTriggerType::IndexPrice => "IndexPrice",
    }
}
fn formatted(n: f64, step: f64) -> Result<String, String> {
    if !n.is_finite() || n <= 0.0 || !step.is_finite() || step <= 0.0 {
        return Err("Backpack order amount/price must be positive and finite".into());
    }
    let adjusted = num::adjust(n, step);
    if adjusted <= 0.0 {
        return Err("Backpack order rounds to zero".into());
    }
    Ok(format!("{:.*}", num::decimals(step) as usize, adjusted))
}
fn ordinary(o: &PlaceOrderOpts) -> bool {
    o.kind == OrderType::Limit
        && o.trigger_price.is_none()
        && o.stop_loss.is_none()
        && o.take_profit.is_none()
        && o.close != Some(true)
        && o.client_order_id.is_none()
}

impl Backpack {
    fn native(&self, cx: &Cx<'_, Pending>, o: &PlaceOrderOpts) -> Result<NativeOrder, String> {
        let m = cx
            .store
            .market(&o.symbol)
            .ok_or_else(|| format!("Market not found: {}", o.symbol))?;
        if !m.active || m.is_read_only == Some(true) {
            return Err("Backpack market is not placeable".into());
        }
        if matches!(o.kind, OrderType::TrailingStopLoss | OrderType::Conditional) {
            return Err("Backpack unsupported order type".into());
        }
        if let Some(id) = &o.client_order_id {
            client_id(id)?;
            if !matches!(o.kind, OrderType::Limit | OrderType::Market)
                || o.trigger_price.is_some()
                || o.stop_loss.is_some()
                || o.take_profit.is_some()
                || o.close == Some(true)
                || (m.limits.amount.max > 0.0 && o.amount > m.limits.amount.max)
            {
                return Err("Backpack clientOrderId requires one ordinary native order".into());
            }
        }
        let stop = matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit);
        let triggered = stop || o.trigger_price.is_some();
        let limit = o.kind == OrderType::Limit;
        let qty = formatted(o.amount, m.precision.amount)?;
        if o.amount < m.limits.amount.min
            || (m.limits.amount.max > 0.0 && o.amount > m.limits.amount.max)
        {
            return Err("Backpack order amount is outside market bounds".into());
        }
        let mut native = NativeOrder {
            symbol: m.id.clone(),
            side: if o.side == OrderSide::Buy {
                "Bid"
            } else {
                "Ask"
            },
            order_type: if limit { "Limit" } else { "Market" },
            price: None,
            quantity: None,
            trigger_price: None,
            trigger_quantity: None,
            trigger_by: None,
            time_in_force: None,
            post_only: None,
            reduce_only: (stop || o.reduce_only == Some(true)).then_some(true),
            client_id: o
                .client_order_id
                .as_ref()
                .map(|id| client_id(id))
                .transpose()?,
            take_profit_trigger_price: None,
            take_profit_trigger_by: None,
            stop_loss_trigger_price: None,
            stop_loss_trigger_by: None,
        };
        if limit {
            native.price = Some(formatted(
                o.price.ok_or("Backpack limit order needs price")?,
                m.precision.price,
            )?);
            native.post_only = Some(o.time_in_force == Some(TimeInForce::PostOnly));
            native.time_in_force = Some(match o.time_in_force {
                Some(TimeInForce::ImmediateOrCancel) => "IOC",
                Some(TimeInForce::FillOrKill) => "FOK",
                _ => "GTC",
            });
        }
        if triggered {
            native.trigger_price = Some(formatted(
                o.trigger_price
                    .or(o.price)
                    .ok_or("Backpack trigger order needs trigger price")?,
                m.precision.price,
            )?);
            native.trigger_quantity = Some(qty);
            native.trigger_by = Some(trigger(if o.kind == OrderType::TakeProfit {
                o.tp_trigger_type.or(o.trigger_type)
            } else {
                o.sl_trigger_type.or(o.trigger_type)
            }));
        } else {
            native.quantity = Some(qty);
        }
        if !limit {
            if let Some(price) = o.take_profit {
                let p = formatted(price, m.precision.price)?;
                native.take_profit_trigger_price = Some(p);
                native.take_profit_trigger_by = Some(trigger(o.tp_trigger_type));
            }
            if let Some(price) = o.stop_loss {
                let p = formatted(price, m.precision.price)?;
                native.stop_loss_trigger_price = Some(p);
                native.stop_loss_trigger_by = Some(trigger(o.sl_trigger_type));
            }
        }
        Ok(native)
    }
    fn placement_steps(
        &self,
        cx: &Cx<'_, Pending>,
        o: &PlaceOrderOpts,
    ) -> Result<Vec<Step>, String> {
        let main = self.native(cx, o)?;
        let mut steps = vec![Step::Place {
            order: Box::new(main.clone()),
            optional: false,
        }];
        if o.kind == OrderType::Limit {
            let m = cx.store.market(&o.symbol).unwrap();
            for (price, kind) in [
                (o.take_profit, o.tp_trigger_type),
                (o.stop_loss, o.sl_trigger_type),
            ] {
                if let Some(price) = price {
                    let mut order = main.clone();
                    order.side = if o.side == OrderSide::Buy {
                        "Ask"
                    } else {
                        "Bid"
                    };
                    order.order_type = "Market";
                    order.price = None;
                    order.quantity = None;
                    order.time_in_force = None;
                    order.post_only = None;
                    order.client_id = None;
                    order.reduce_only = Some(true);
                    order.trigger_price = Some(formatted(price, m.precision.price)?);
                    order.trigger_quantity = Some(formatted(o.amount, m.precision.amount)?);
                    order.trigger_by = Some(trigger(kind));
                    steps.push(Step::Place {
                        order: Box::new(order),
                        optional: true,
                    });
                }
            }
        }
        Ok(steps)
    }
    fn generated_client(
        &mut self,
        cx: &Cx<'_, Pending>,
        used: &HashSet<u32>,
    ) -> Result<u32, String> {
        if self.trading.next_client == 0 {
            let random = safe_cex_crypto::uuid::uuid()?;
            self.trading.next_client = u32::from_le_bytes(
                safe_cex_crypto::sha256(random.as_bytes())[..4]
                    .try_into()
                    .unwrap(),
            )
            .max(1);
        }
        for _ in 0..4096 {
            let id = self.trading.next_client;
            self.trading.next_client = self.trading.next_client.wrapping_add(1).max(1);
            if !used.contains(&id)
                && !cx.store.orders().iter().any(|o| {
                    o.client_order_id
                        .as_ref()
                        .is_some_and(|s| s == &id.to_string())
                })
            {
                return Ok(id);
            }
        }
        Err("Backpack cannot mint unique batch IDs".into())
    }
    fn prepare_place(
        &mut self,
        cx: &Cx<'_, Pending>,
        orders: Vec<PlaceOrderOpts>,
    ) -> Result<Vec<Step>, String> {
        let mut caller = HashSet::new();
        let mut prepared = Vec::new();
        for o in &orders {
            if let Some(id) = &o.client_order_id
                && !caller.insert(client_id(id)?)
            {
                return Err("Backpack duplicated caller clientOrderId".into());
            }
            prepared.push(self.placement_steps(cx, o)?);
        }
        let eligible = |o: &PlaceOrderOpts| {
            ordinary(o)
                && cx
                    .store
                    .market(&o.symbol)
                    .is_some_and(|m| m.limits.amount.max > 0.0 && o.amount <= m.limits.amount.max)
        };
        let mut steps = Vec::new();
        let mut i = 0;
        while i < orders.len() {
            if i + 1 < orders.len() && eligible(&orders[i]) && eligible(&orders[i + 1]) {
                let mut pair = vec![];
                for p in &prepared[i..i + 2] {
                    let Step::Place { order, .. } = &p[0] else {
                        unreachable!()
                    };
                    let mut order = (**order).clone();
                    let id = self.generated_client(cx, &caller)?;
                    caller.insert(id);
                    order.client_id = Some(id);
                    pair.push(order);
                }
                steps.push(Step::Pair(pair));
                i += 2;
            } else {
                steps.append(&mut prepared[i]);
                i += 1;
            }
        }
        Ok(steps)
    }
    pub(crate) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let (id, plan) = match command {
            Command::PlaceOrders { command, orders } => {
                (command, self.prepare_place(cx, orders).map(|s| (s, true)))
            }
            Command::CancelOrders { command, ids } => {
                let steps = ids
                    .into_iter()
                    .map(|id| {
                        let order = cx
                            .store
                            .order(&id)
                            .ok_or_else(|| format!("Order not found: {id}"))?;
                        Ok(Step::Cancel {
                            symbol: order.symbol.clone(),
                            id,
                            optional: true,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>();
                (command, steps.map(|s| (s, false)))
            }
            Command::CancelSymbolOrders { command, symbol } => (
                command,
                if cx.store.market(&symbol).is_some() {
                    Ok((vec![Step::CancelAll { symbol }], false))
                } else {
                    Err(format!("Market not found: {symbol}"))
                },
            ),
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => {
                let plan = (|| {
                    let old = cx
                        .store
                        .order(&id)
                        .ok_or_else(|| format!("Order not found: {id}"))?;
                    if matches!(old.status, OrderStatus::Closed | OrderStatus::Canceled) {
                        return Err("Backpack cannot amend a terminal order".into());
                    }
                    let o = PlaceOrderOpts {
                        symbol: old.symbol.clone(),
                        kind: old.kind,
                        side: old.side,
                        amount: amount.unwrap_or(old.amount),
                        price: Some(
                            old.extra
                                .as_ref()
                                .and_then(|e| e.conditional_order_price)
                                .unwrap_or(price.unwrap_or(old.price)),
                        ),
                        trigger_price: old
                            .extra
                            .as_ref()
                            .and_then(|e| e.conditional_order_price)
                            .map(|_| price.unwrap_or(old.price)),
                        stop_loss: None,
                        take_profit: None,
                        reduce_only: Some(old.reduce_only),
                        time_in_force: old.time_in_force,
                        trigger_type: old.trigger_type,
                        tp_trigger_type: None,
                        sl_trigger_type: None,
                        client_order_id: None,
                        close: None,
                        stp_mode: None,
                    };
                    let mut steps = self.placement_steps(cx, &o)?;
                    steps.insert(
                        0,
                        Step::Cancel {
                            symbol: old.symbol.clone(),
                            id,
                            optional: false,
                        },
                    );
                    Ok((steps, true))
                })();
                (command, plan)
            }
            Command::SetLeverage {
                command, leverage, ..
            } => (
                command,
                if leverage.is_finite() && leverage > 0.0 {
                    Ok((vec![Step::Leverage(leverage)], false))
                } else {
                    Err("Backpack leverage must be positive and finite".into())
                },
            ),
            Command::SetMarginType { command, .. } => {
                cx.done(command, Err("Backpack only supports cross margin".into()));
                return;
            }
            Command::SetPositionMode { command, .. } => {
                cx.done(command, Err("Backpack position mode is fixed".into()));
                return;
            }
            _ => return,
        };
        if self.account.credentials.is_none() {
            cx.done(id, Err("Backpack credentials unavailable".into()));
            return;
        }
        if self.trading.jobs.contains_key(&u64::from(id)) {
            cx.done(id, Err("Backpack command is already active".into()));
            return;
        }
        match plan {
            Ok((steps, collect_ids)) => {
                self.trading.jobs.insert(
                    u64::from(id),
                    Job {
                        steps: steps.into(),
                        ids: vec![],
                        collect_ids,
                        internal: false,
                        replaces: None,
                    },
                );
                self.run_trade(cx, u64::from(id));
            }
            Err(e) => cx.done(id, Err(e)),
        }
    }
    fn run_trade(&mut self, cx: &mut Cx<'_, Pending>, command: u64) {
        let Some(job) = self.trading.jobs.get_mut(&command) else {
            return;
        };
        let Some(step) = job.steps.pop_front() else {
            let job = self.trading.jobs.remove(&command).unwrap();
            if job.internal {
                return;
            }
            cx.done(
                command as CommandId,
                Ok(if job.collect_ids {
                    serde_json::to_string(&job.ids).unwrap()
                } else {
                    "[]".into()
                }),
            );
            return;
        };
        let mut previous = None;
        let mut targets = vec![];
        let (method, path, params, body) = match &step {
            Step::Place { order, .. } => (
                Method::Post,
                "/api/v1/order",
                order.params(),
                serde_json::to_string(order).unwrap(),
            ),
            Step::Pair(rows) => {
                let c = self.account.credentials.as_ref().unwrap();
                let headers = c.headers(
                    "orderExecute",
                    &rows.iter().map(NativeOrder::params).collect::<Vec<_>>(),
                    cx.now,
                );
                cx.request(
                    Method::Post,
                    format!("{REST}/api/v1/orders"),
                    headers,
                    Some(serde_json::to_string(rows).unwrap()),
                    Pending::Trade(Request {
                        command,
                        step,
                        previous,
                        targets,
                        leverage_revision: self.account.leverage_revision,
                    }),
                );
                return;
            }
            Step::Cancel { symbol, id, .. } => {
                self.account.orders_revision += 1;
                previous = cx.store.order(id).map(|o| o.status);
                if let Some(order) = cx.store.order(id) {
                    targets.push((id.clone(), self.order_stream_time(id)));
                    let state = self
                        .trading
                        .cancels
                        .entry(id.clone())
                        .or_insert((0, order.status));
                    state.0 += 1;
                    previous = Some(state.1);
                }
                if cx.store.order(id).is_some_and(|o| {
                    !matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled)
                }) {
                    cx.store.set_order_status(id, OrderStatus::Canceling);
                }
                let p: Params = [
                    ("symbol".into(), symbol.clone()),
                    ("orderId".into(), id.clone()),
                ]
                .into();
                (
                    Method::Delete,
                    "/api/v1/order",
                    p.clone(),
                    serde_json::to_string(&p).unwrap(),
                )
            }
            Step::CancelAll { symbol } => {
                self.account.orders_revision += 1;
                targets = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| {
                        o.symbol == *symbol
                            && !matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled)
                    })
                    .map(|o| (o.id.clone(), self.order_stream_time(&o.id)))
                    .collect();
                let p: Params = [("symbol".into(), symbol.clone())].into();
                (
                    Method::Delete,
                    "/api/v1/orders",
                    p.clone(),
                    serde_json::to_string(&p).unwrap(),
                )
            }
            Step::Leverage(leverage) => {
                self.account.leverage_revision += 1;
                let p: Params = [("leverageLimit".into(), num::format(*leverage))].into();
                (
                    Method::Patch,
                    "/api/v1/account",
                    p.clone(),
                    serde_json::to_string(&p).unwrap(),
                )
            }
        };
        let result = self.signed(
            cx,
            method,
            path,
            params,
            Some(body),
            Pending::Trade(Request {
                command,
                step,
                previous,
                targets,
                leverage_revision: self.account.leverage_revision,
            }),
            false,
        );
        if let Err(e) = result {
            self.trading.jobs.remove(&command);
            cx.done(command as CommandId, Err(e));
        }
    }
    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let result = (|| {
            if !(200..300).contains(&response.status) {
                return Err(format!(
                    "Backpack HTTP {}: {}",
                    response.status, response.body
                ));
            }
            // Some API errors are returned with HTTP 200.
            #[derive(Deserialize)]
            struct Failure<'a> {
                #[serde(borrow)]
                error: Option<&'a serde_json::value::RawValue>,
                #[serde(borrow)]
                code: Option<&'a serde_json::value::RawValue>,
            }
            if response.body.trim_start().starts_with('{')
                && let Ok(error) = serde_json::from_str::<Failure<'_>>(response.body)
                && (error.error.is_some() || error.code.is_some())
            {
                return Err(format!("Backpack write rejected: {}", response.body));
            }
            match &request.step {
                Step::Place { .. } => {
                    #[derive(Deserialize)]
                    struct Placed<'a> {
                        #[serde(borrow)]
                        id: &'a str,
                    }
                    let row: Placed<'_> =
                        serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                    if row.id.trim().is_empty() {
                        return Err("Backpack placement missing order ID".into());
                    }
                    self.account.orders_revision += 1;
                    if let Some(job) = self.trading.jobs.get_mut(&request.command) {
                        job.ids.push(row.id.into());
                        if let Some(old_id) = job.replaces.as_deref() {
                            let native: crate::wire::OrderRow<'_> =
                                serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                            let mapped = crate::account::order(native, cx.store);
                            cx.store.remove_orders(&[old_id]);
                            if cx.store.order(&mapped.id).is_none() {
                                cx.store.upsert_order(mapped);
                            }
                        }
                    }
                }
                Step::Pair(orders) => {
                    #[derive(Deserialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Row<'a> {
                        #[serde(borrow)]
                        operation: &'a str,
                        #[serde(borrow)]
                        id: &'a str,
                        client_id: u32,
                        #[serde(borrow)]
                        symbol: &'a str,
                        #[serde(borrow)]
                        side: &'a str,
                        #[serde(borrow)]
                        order_type: &'a str,
                    }
                    let rows: Vec<Row<'_>> =
                        serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                    if rows.len() != orders.len() {
                        return Err("Backpack batch response count mismatch".into());
                    }
                    let mut ids = HashMap::new();
                    let mut unique = HashSet::new();
                    for r in rows {
                        let expected = orders
                            .iter()
                            .find(|o| o.client_id == Some(r.client_id))
                            .ok_or("Backpack unknown batch clientId")?;
                        if r.operation != "Ok"
                            || r.id.trim().is_empty()
                            || expected.symbol != r.symbol
                            || expected.side != r.side
                            || expected.order_type != r.order_type
                            || ids.insert(r.client_id, r.id.to_owned()).is_some()
                            || !unique.insert(r.id)
                        {
                            return Err("Backpack invalid batch response identity".into());
                        }
                    }
                    if let Some(job) = self.trading.jobs.get_mut(&request.command) {
                        for o in orders {
                            job.ids.push(ids.remove(&o.client_id.unwrap()).unwrap());
                        }
                    }
                }
                Step::Cancel { id, .. } => {
                    self.account.orders_revision += 1;
                    self.finish_cancel(id);
                    let unchanged = request
                        .targets
                        .first()
                        .is_some_and(|(_, stamp)| *stamp == self.order_stream_time(id));
                    cx.store.update_order(id, |o| {
                        if unchanged
                            && !matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled)
                        {
                            o.status = OrderStatus::Canceled;
                        }
                    });
                }
                Step::CancelAll { .. } => {
                    self.account.orders_revision += 1;
                    for (id, stamp) in &request.targets {
                        let unchanged = self.order_stream_time(id) == *stamp;
                        cx.store.update_order(id, |o| {
                            if unchanged
                                && !matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled)
                            {
                                o.status = OrderStatus::Canceled;
                            }
                        });
                    }
                }
                Step::Leverage(leverage) => {
                    if request.leverage_revision == self.account.leverage_revision {
                        self.account.leverage_revision += 1;
                        self.account.leverage = *leverage;
                        self.account.positions_revision += 1;
                        self.leverage_settings(cx);
                        cx.store.update_positions(|p| p.leverage = *leverage);
                        self.positions_read(cx, Next::None);
                    }
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.run_trade(cx, request.command),
            Err(e) => self.trade_failed(cx, request, &e),
        }
    }
    pub(crate) fn trade_failed(&mut self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        if let Step::Cancel { id, .. } = &request.step
            && self.finish_cancel(id)
            && let Some(previous) = request.previous
        {
            let unchanged = request
                .targets
                .first()
                .is_some_and(|(_, stamp)| *stamp == self.order_stream_time(id));
            cx.store.update_order(id, |o| {
                if o.status == OrderStatus::Canceling && unchanged {
                    o.status = previous;
                }
            });
        }
        if matches!(
            request.step,
            Step::Place { optional: true, .. } | Step::Cancel { optional: true, .. }
        ) {
            cx.log(LogLevel::Warning, error);
            self.run_trade(cx, request.command);
        } else {
            let internal = self
                .trading
                .jobs
                .remove(&request.command)
                .is_some_and(|job| job.internal);
            if internal {
                cx.log(LogLevel::Warning, error);
            } else {
                cx.done(request.command as CommandId, Err(error.into()));
            }
        }
    }

    fn finish_cancel(&mut self, id: &str) -> bool {
        let Some(state) = self.trading.cancels.get_mut(id) else {
            return false;
        };
        state.0 -= 1;
        if state.0 == 0 {
            self.trading.cancels.remove(id);
            true
        } else {
            false
        }
    }
}

impl Backpack {
    pub(crate) fn trigger_repair(
        &self,
        cx: &Cx<'_, Pending>,
        row: &crate::wire::OrderRow<'_>,
    ) -> Result<Repair, String> {
        let market = cx
            .store
            .market(&row.symbol)
            .ok_or("Backpack repair market missing")?;
        let amount = row.trigger_quantity.as_ref().map_or(row.quantity, |n| n.0);
        let trigger = formatted(
            row.trigger_price
                .as_ref()
                .ok_or("Backpack repair trigger missing")?
                .0,
            market.precision.price,
        )?;
        let quantity = formatted(amount, market.precision.amount)?;
        let limit = row.order_type.eq_ignore_ascii_case("Limit");
        let price = if limit {
            Some(formatted(row.price, market.precision.price)?)
        } else {
            None
        };
        Ok(Repair {
            old_id: row.id.to_string(),
            order: NativeOrder {
                symbol: row.symbol.to_string(),
                side: if row.side == "Bid" { "Bid" } else { "Ask" },
                order_type: if limit { "Limit" } else { "Market" },
                price,
                trigger_price: Some(trigger),
                trigger_quantity: Some(quantity),
                trigger_by: Some("MarkPrice"),
                reduce_only: row.reduce_only,
                post_only: limit.then_some(row.post_only || row.time_in_force == "PO"),
                time_in_force: limit.then_some(match row.time_in_force.as_ref() {
                    "IOC" => "IOC",
                    "FOK" => "FOK",
                    _ => "GTC",
                }),
                ..Default::default()
            },
        })
    }
    pub(crate) fn start_repair(&mut self, cx: &mut Cx<'_, Pending>, repair: Repair) {
        let command = (u64::from(u32::MAX) + 1..u64::MAX)
            .rev()
            .find(|id| !self.trading.jobs.contains_key(id))
            .unwrap();
        let steps = vec![
            Step::Cancel {
                symbol: repair.order.symbol.clone(),
                id: repair.old_id.clone(),
                optional: false,
            },
            Step::Place {
                order: Box::new(repair.order),
                optional: false,
            },
        ];
        self.trading.jobs.insert(
            command,
            Job {
                steps: steps.into(),
                ids: vec![],
                collect_ids: false,
                internal: true,
                replaces: Some(repair.old_id),
            },
        );
        self.run_trade(cx, command);
    }
}
