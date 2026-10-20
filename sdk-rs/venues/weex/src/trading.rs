//! Contract V3 writes. A command is planned completely before its first write;
//! subsequent steps run only after the previous acknowledgement. Writes are
//! never retried, including when the transport cannot determine their outcome.
use crate::{Pending, Task, WeexFutures};
use safe_cex_engine::model::{
    MarginType, Order, OrderExtra, OrderIdentityKind, OrderIdentityReceipt, OrderSide, OrderStatus,
    OrderType, PlaceOrderOpts, StopTriggerType, TimeInForce,
};
use safe_cex_engine::{Command, CommandId, Cx, LogLevel, Method, Response, num};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Default)]
pub(crate) struct State {
    jobs: HashMap<CommandId, Job>,
    brackets: HashMap<String, Brackets>,
}
#[derive(Clone, Copy)]
struct Brackets {
    tp: Option<StopTriggerType>,
    sl: Option<StopTriggerType>,
}
struct Job {
    steps: VecDeque<Step>,
    ids: Vec<String>,
    error: Option<String>,
    continue_on_error: bool,
    refresh: bool,
    hedged: Option<bool>,
}
pub(crate) struct Request {
    command: CommandId,
    call: Call,
    revision: u64,
}
struct ReplacementGuard {
    id: String,
    amount: f64,
    filled: f64,
}
struct Step {
    replacement: Option<ReplacementGuard>,
    method: Method,
    path: &'static str,
    query: String,
    body: Option<String>,
    call: Call,
}
enum Call {
    Place {
        orders: Vec<Order>,
        batch: bool,
        protective: bool,
        brackets: Option<Brackets>,
    },
    Cancel {
        before: Vec<Order>,
        native: Vec<String>,
        all: bool,
    },
    Amend {
        before: Box<Order>,
        price: f64,
    },
    Leverage {
        symbol: String,
        leverage: f64,
    },
    Margin {
        symbol: String,
        margin: MarginType,
    },
    Mode {
        symbol: String,
        hedged: bool,
    },
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Regular {
    symbol: String,
    side: &'static str,
    position_side: &'static str,
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    time_in_force: Option<&'static str>,
    quantity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<String>,
    new_client_order_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reduce_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tp_trigger_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sl_trigger_price: Option<String>,
    #[serde(rename = "TpWorkingType", skip_serializing_if = "Option::is_none")]
    tp_working_type: Option<&'static str>,
    #[serde(rename = "SlWorkingType", skip_serializing_if = "Option::is_none")]
    sl_working_type: Option<&'static str>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Conditional {
    symbol: String,
    side: &'static str,
    position_side: &'static str,
    #[serde(rename = "type")]
    kind: &'static str,
    quantity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<String>,
    trigger_price: String,
    client_algo_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    preset_take_profit_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preset_stop_loss_price: Option<String>,
    #[serde(rename = "TpWorkingType", skip_serializing_if = "Option::is_none")]
    tp_working_type: Option<&'static str>,
    #[serde(rename = "SlWorkingType", skip_serializing_if = "Option::is_none")]
    sl_working_type: Option<&'static str>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Protective {
    symbol: String,
    client_algo_id: String,
    plan_type: &'static str,
    trigger_price: String,
    execute_price: &'static str,
    quantity: String,
    position_side: &'static str,
    trigger_price_type: &'static str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Batch {
    batch_orders: Vec<Regular>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CancelBatch {
    order_id_list: Vec<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AmendProtective {
    order_id: String,
    trigger_price: String,
    execute_price: String,
    trigger_price_type: &'static str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Leverage {
    symbol: String,
    margin_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    cross_leverage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    isolated_long_leverage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    isolated_short_leverage: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Margin {
    symbol: String,
    margin_type: &'static str,
    separated_type: &'static str,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Id {
    Text(String),
    Number(u64),
}
impl Id {
    fn id(&self) -> Result<String, String> {
        let text = match self {
            Self::Text(s) => s.clone(),
            Self::Number(n) => n.to_string(),
        };
        native_id(&text)?;
        Ok(text)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResultRow {
    #[serde(default)]
    order_id: Option<Id>,
    #[serde(default)]
    client_order_id: Option<String>,
    success: bool,
    #[serde(default)]
    error_code: String,
    #[serde(default)]
    error_message: String,
}
impl ResultRow {
    fn error(&self) -> String {
        if !self.error_message.is_empty() {
            self.error_message.clone()
        } else if !self.error_code.is_empty() {
            self.error_code.clone()
        } else {
            "WEEX order rejected".into()
        }
    }
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Rows {
    Many(Vec<ResultRow>),
    One(ResultRow),
}
impl Rows {
    fn into_vec(self) -> Vec<ResultRow> {
        match self {
            Self::Many(v) => v,
            Self::One(v) => vec![v],
        }
    }
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Code {
    Text(String),
    Number(i64),
}
impl Code {
    fn success(&self) -> bool {
        match self {
            Self::Text(s) => s == "200" || s == "0",
            Self::Number(n) => *n == 200 || *n == 0,
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetadataResult {
    code: Option<Code>,
    success: Option<bool>,
    #[serde(default)]
    msg: String,
    symbol: Option<String>,
    margin_type: Option<String>,
    cross_leverage: Option<String>,
    isolated_long_leverage: Option<String>,
    isolated_short_leverage: Option<String>,
}

fn json(value: &impl Serialize) -> String {
    serde_json::to_string(value).expect("typed finite request")
}
fn native_id(id: &str) -> Result<u64, String> {
    if id.starts_with('0') || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err("WEEX requires an actual numeric order ID".into());
    }
    id.parse::<u64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| "WEEX invalid order ID".into())
}
fn valid_client_id(id: &str) -> bool {
    (1..=36).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".:/_-".contains(&b))
}
fn mint() -> Result<String, String> {
    safe_cex_crypto::uuid::uuid().map(|id| format!("ts{id}"))
}
fn working(t: StopTriggerType) -> Result<&'static str, String> {
    match t {
        StopTriggerType::MarkPrice => Ok("MARK_PRICE"),
        StopTriggerType::LastPrice => Ok("CONTRACT_PRICE"),
        StopTriggerType::IndexPrice => Err("WEEX index-price triggers are not supported".into()),
    }
}
fn margin_text(m: MarginType) -> &'static str {
    if m == MarginType::Isolated {
        "ISOLATED"
    } else {
        "CROSSED"
    }
}
fn mode_text(hedged: bool) -> &'static str {
    if hedged { "SEPARATED" } else { "COMBINED" }
}
fn algo(o: &Order) -> bool {
    matches!(
        o.kind,
        OrderType::Conditional
            | OrderType::StopLoss
            | OrderType::TakeProfit
            | OrderType::TrailingStopLoss
    ) || o.id.contains("__")
}
fn plain(o: &PlaceOrderOpts) -> bool {
    matches!(o.kind, OrderType::Limit | OrderType::Market)
        && o.trigger_price.is_none()
        && o.stop_loss.is_none()
        && o.take_profit.is_none()
}
fn batchable(o: &PlaceOrderOpts) -> bool {
    plain(o)
        && o.reduce_only != Some(true)
        && o.close != Some(true)
        && o.tp_trigger_type.is_none()
        && o.sl_trigger_type.is_none()
}
fn rounded(value: f64, step: f64) -> Result<f64, String> {
    let v = num::adjust(value, step);
    if value.is_finite() && value > 0.0 && v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        Err("WEEX price/quantity must be finite and positive after rounding".into())
    }
}
fn price(value: Option<f64>, step: f64) -> Result<Option<String>, String> {
    value.map(|v| rounded(v, step).map(num::format)).transpose()
}
fn reject(cx: &mut Cx<'_, Pending>, command: CommandId, error: String) {
    cx.log(LogLevel::Error, &error);
    cx.done(command, Err(error));
}
fn post(path: &'static str, body: String, call: Call) -> Step {
    Step {
        replacement: None,
        method: Method::Post,
        path,
        query: String::new(),
        body: Some(body),
        call,
    }
}

impl WeexFutures {
    fn replacement_brackets(
        &self,
        cx: &Cx<'_, Pending>,
        order: &Order,
    ) -> Result<Brackets, String> {
        if let Some(types) = self.trading.brackets.get(&order.id) {
            return Ok(*types);
        }
        let source = |kind| {
            cx.store
                .orders()
                .iter()
                .find(|child| {
                    child.parent_id.as_deref() == Some(order.id.as_str()) && child.kind == kind
                })
                .and_then(|child| child.trigger_type)
        };
        let tp = order
            .take_profit
            .map(|_| {
                source(OrderType::TakeProfit)
                    .ok_or("WEEX cannot preserve imported take-profit trigger type")
            })
            .transpose()?;
        let sl = order
            .stop_loss
            .map(|_| {
                source(OrderType::StopLoss)
                    .ok_or("WEEX cannot preserve imported stop-loss trigger type")
            })
            .transpose()?;
        Ok(Brackets { tp, sl })
    }
    fn plan_orders(
        &self,
        cx: &Cx<'_, Pending>,
        orders: Vec<PlaceOrderOpts>,
    ) -> Result<VecDeque<Step>, String> {
        if orders.iter().any(|order| {
            order.bybit_bbo.is_some() || order.bybit_slippage_tolerance_ticks.is_some()
        }) {
            return Err("Bybit BBO and TickSize controls are not supported by WEEX".into());
        }
        // Validate every caller ID (including duplicates) before constructing a
        // job. No earlier row can escape when a later row is unhonorable.
        let mut seen = HashSet::new();
        for o in &orders {
            if let Some(id) = &o.client_order_id {
                if !plain(o) {
                    return Err("WEEX caller clientOrderId requires one regular order without brackets or triggers".into());
                }
                if !valid_client_id(id) {
                    return Err("WEEX clientOrderId must match ^[.A-Z:/a-z0-9_-]{1,36}$".into());
                }
                if !seen.insert(id.clone()) {
                    return Err("WEEX duplicate clientOrderId in batch".into());
                }
            }
        }
        let mut steps = VecDeque::new();
        let mut regular = Vec::new();
        let mut optimistic = Vec::new();
        for o in orders {
            let m = cx
                .store
                .market(&o.symbol)
                .ok_or_else(|| format!("Market {} not found", o.symbol))?;
            let full_protective =
                matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit) && o.amount == 0.0;
            let amount = if full_protective {
                0.0
            } else {
                rounded(o.amount, m.precision.amount)?
            };
            if m.limits.amount.max > 0.0 && amount > m.limits.amount.max {
                return Err(
                    "WEEX order exceeds market maximum size; a caller ID cannot span orders".into(),
                );
            }
            if !full_protective && amount < m.limits.amount.min {
                return Err("WEEX order quantity below market minimum".into());
            }
            let p = price(o.price, m.precision.price)?;
            let tp = price(o.take_profit, m.precision.price)?;
            let sl = price(o.stop_loss, m.precision.price)?;
            let tp_type = o.tp_trigger_type.unwrap_or(StopTriggerType::LastPrice);
            let sl_type = o.sl_trigger_type.unwrap_or(StopTriggerType::MarkPrice);
            let tp_working = tp.as_ref().map(|_| working(tp_type)).transpose()?;
            let sl_working = sl.as_ref().map(|_| working(sl_type)).transpose()?;
            let side = if o.side == OrderSide::Buy {
                "BUY"
            } else {
                "SELL"
            };
            let position_side = if (o.side == OrderSide::Buy) != o.reduce_only.unwrap_or(false) {
                "LONG"
            } else {
                "SHORT"
            };
            let id = match &o.client_order_id {
                Some(id) => id.clone(),
                None => mint()?,
            };
            if !seen.insert(format!("native:{id}")) {
                return Err("WEEX duplicate generated clientOrderId".into());
            }
            let trigger = if o.kind == OrderType::Conditional {
                price(o.trigger_price, m.precision.price)?
            } else {
                None
            };
            let display_price = if o.kind == OrderType::Conditional {
                trigger.as_ref().map(|p| num::parse(p)).unwrap_or(0.0)
            } else {
                p.as_ref().map(|p| num::parse(p)).unwrap_or(0.0)
            };
            let bracket_types =
                (o.take_profit.is_some() || o.stop_loss.is_some()).then_some(Brackets {
                    tp: o.tp_trigger_type,
                    sl: o.sl_trigger_type,
                });
            let row = Order {
                id: format!("__weex_creating_{id}"),
                parent_id: None,
                status: OrderStatus::Creating,
                symbol: o.symbol.clone(),
                kind: o.kind,
                side: o.side,
                price: display_price,
                amount,
                filled: 0.0,
                remaining: amount,
                reduce_only: o.reduce_only.unwrap_or(false),
                close: o.close,
                client_order_id: Some(id.clone()),
                time_in_force: o.time_in_force,
                last_update: cx.now,
                trigger_type: if o.kind == OrderType::TakeProfit {
                    Some(tp_type)
                } else if o.kind == OrderType::StopLoss {
                    Some(sl_type)
                } else {
                    o.trigger_type
                },
                take_profit: tp.as_ref().map(|s| num::parse(s)),
                stop_loss: sl.as_ref().map(|s| num::parse(s)),
                extra: (o.kind == OrderType::Conditional).then_some(OrderExtra {
                    conditional_order_price: p.as_ref().map(|s| num::parse(s)),
                    ..OrderExtra::default()
                }),
            };
            if !batchable(&o) {
                flush(&mut steps, &mut regular, &mut optimistic);
            }
            match o.kind {
                OrderType::Limit | OrderType::Market => {
                    if o.trigger_price.is_some() {
                        return Err("WEEX regular orders cannot carry a triggerPrice".into());
                    }
                    if o.kind == OrderType::Limit && p.is_none() {
                        return Err("WEEX limit price is required".into());
                    }
                    let payload = Regular {
                        symbol: o.symbol.clone(),
                        side,
                        position_side,
                        kind: if o.kind == OrderType::Limit {
                            "LIMIT"
                        } else {
                            "MARKET"
                        },
                        time_in_force: (o.kind == OrderType::Limit).then_some(
                            match o.time_in_force.unwrap_or(TimeInForce::GoodTillCancel) {
                                TimeInForce::GoodTillCancel => "GTC",
                                TimeInForce::ImmediateOrCancel => "IOC",
                                TimeInForce::FillOrKill => "FOK",
                                TimeInForce::PostOnly => "POST_ONLY",
                            },
                        ),
                        quantity: num::format(amount),
                        price: if o.kind == OrderType::Limit { p } else { None },
                        new_client_order_id: id,
                        reduce_only: o.reduce_only.filter(|v| *v),
                        tp_trigger_price: tp,
                        sl_trigger_price: sl,
                        tp_working_type: tp_working,
                        sl_working_type: sl_working,
                    };
                    if batchable(&o) {
                        regular.push(payload);
                        optimistic.push(row);
                        if regular.len() == 5 {
                            flush(&mut steps, &mut regular, &mut optimistic);
                        }
                    } else {
                        steps.push_back(post(
                            "/capi/v3/order",
                            json(&payload),
                            Call::Place {
                                orders: vec![row],
                                batch: false,
                                protective: false,
                                brackets: bracket_types,
                            },
                        ));
                    }
                }
                OrderType::Conditional => {
                    if o.reduce_only == Some(true) || o.close == Some(true) {
                        return Err("WEEX opening conditional cannot close a position".into());
                    }
                    let payload = Conditional {
                        symbol: o.symbol,
                        side,
                        position_side,
                        kind: if p.is_some() { "STOP" } else { "STOP_MARKET" },
                        quantity: num::format(amount),
                        price: p,
                        trigger_price: trigger
                            .ok_or("WEEX opening conditional requires triggerPrice")?,
                        client_algo_id: id,
                        preset_take_profit_price: tp,
                        preset_stop_loss_price: sl,
                        tp_working_type: tp_working,
                        sl_working_type: sl_working,
                    };
                    steps.push_back(post(
                        "/capi/v3/algoOrder",
                        json(&payload),
                        Call::Place {
                            orders: vec![row],
                            batch: false,
                            protective: false,
                            brackets: bracket_types,
                        },
                    ));
                }
                OrderType::StopLoss | OrderType::TakeProfit => {
                    let payload = Protective {
                        symbol: o.symbol,
                        client_algo_id: id,
                        plan_type: if o.kind == OrderType::TakeProfit {
                            "TAKE_PROFIT"
                        } else {
                            "STOP_LOSS"
                        },
                        trigger_price: p.ok_or("WEEX protective order requires price")?,
                        execute_price: "0",
                        quantity: num::format(amount),
                        position_side,
                        trigger_price_type: working(if o.kind == OrderType::TakeProfit {
                            tp_type
                        } else {
                            sl_type
                        })?,
                    };
                    steps.push_back(post(
                        "/capi/v3/placeTpSlOrder",
                        json(&payload),
                        Call::Place {
                            orders: vec![row],
                            batch: false,
                            protective: true,
                            brackets: None,
                        },
                    ));
                }
                OrderType::TrailingStopLoss => {
                    return Err("WEEX trailing stop loss is not supported".into());
                }
            }
        }
        flush(&mut steps, &mut regular, &mut optimistic);
        Ok(steps)
    }

    fn plan_cancel(
        &self,
        cx: &Cx<'_, Pending>,
        ids: Vec<String>,
    ) -> Result<VecDeque<Step>, String> {
        let mut steps = VecDeque::new();
        let mut normal = Vec::new();
        let mut algo_orders = Vec::new();
        let mut seen = HashSet::new();
        for id in ids {
            if !seen.insert(id.clone()) {
                continue;
            }
            let order = cx.store.order(&id).cloned();
            let base = id.split("__").next().unwrap_or("");
            native_id(base)?;
            let base = base.to_owned();
            if order.as_ref().is_some_and(algo) || id.contains("__") {
                algo_orders.push((id, base.clone(), order));
            } else {
                normal.push((id, base.clone(), order));
            }
        }
        for chunk in normal.chunks(10) {
            let native: Vec<_> = chunk.iter().map(|(_, base, _)| base.clone()).collect();
            let body = CancelBatch {
                order_id_list: native.clone(),
            };
            steps.push_back(Step {
                replacement: None,
                method: Method::Delete,
                path: "/capi/v3/batchOrders",
                query: String::new(),
                body: Some(json(&body)),
                call: Call::Cancel {
                    before: chunk.iter().filter_map(|(_, _, o)| o.clone()).collect(),
                    native,
                    all: false,
                },
            });
        }
        // Synthetic TP/SL siblings share a native selector: cancel it once.
        let mut native_seen = HashSet::new();
        for (_, base, _) in &algo_orders {
            if !native_seen.insert(base.clone()) {
                continue;
            }
            steps.push_back(Step {
                replacement: None,
                method: Method::Delete,
                path: "/capi/v3/algoOrder",
                query: format!("orderId={base}"),
                body: None,
                call: Call::Cancel {
                    before: cx
                        .store
                        .orders()
                        .iter()
                        .filter(|o| o.id.split("__").next() == Some(base.as_str()))
                        .cloned()
                        .collect(),
                    native: vec![base.clone()],
                    all: false,
                },
            });
        }
        Ok(steps)
    }

    pub(crate) fn trade_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Command,
    ) -> Option<Command> {
        // Serialize settings admission: a later command must be planned from
        // acknowledged settings, never an older in-flight mode/margin cache.
        if let Command::SetLeverage { command: id, .. }
        | Command::SetMarginType { command: id, .. }
        | Command::SetPositionMode { command: id, .. } = &command
            && self.trading.jobs.values().any(|job| job.refresh)
        {
            if self.trading.jobs.contains_key(id) {
                cx.log(LogLevel::Error, "WEEX command ID is already in flight");
            } else {
                reject(
                    cx,
                    *id,
                    "WEEX account settings update is already in flight; wait for its result".into(),
                );
            }
            return None;
        }
        self.trading.brackets.retain(|id, _| {
            cx.store
                .order(id)
                .is_some_and(|o| !matches!(o.status, OrderStatus::Canceled | OrderStatus::Closed))
        });
        let mut continue_on_error = false;
        let mut refresh = false;
        let mut hedged = None;
        let (id, planned) = match command {
            Command::PlaceOrders { command, orders } => (command, self.plan_orders(cx, orders)),
            Command::CancelOrders { command, ids } => (command, self.plan_cancel(cx, ids)),
            Command::AmendOrder {
                command,
                id,
                price: new_price,
                amount,
            } => {
                let planned = (|| {
                    let o = cx.store.order(&id).ok_or("WEEX order not found")?.clone();
                    if matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled) {
                        return Err("WEEX cannot amend a terminal order".into());
                    }
                    if matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit) {
                        if amount.is_some() || new_price.is_none() {
                            return Err("WEEX TP/SL quantity amendment is not supported".into());
                        }
                        if id.contains("__") {
                            return Err(
                                "WEEX virtual TP/SL amendment requires the protective order ID"
                                    .into(),
                            );
                        }
                        native_id(&id)?;
                        let m = cx.store.market(&o.symbol).ok_or("WEEX market not found")?;
                        let price = rounded(new_price.unwrap(), m.precision.price)?;
                        let execute = o
                            .extra
                            .as_ref()
                            .and_then(|e| e.conditional_order_price)
                            .unwrap_or(0.0);
                        let payload = AmendProtective {
                            order_id: id,
                            trigger_price: num::format(price),
                            execute_price: num::format(execute),
                            trigger_price_type: working(o.trigger_type.unwrap_or(
                                if o.kind == OrderType::TakeProfit {
                                    StopTriggerType::LastPrice
                                } else {
                                    StopTriggerType::MarkPrice
                                },
                            ))?,
                        };
                        Ok(VecDeque::from([post(
                            "/capi/v3/modifyTpSlOrder",
                            json(&payload),
                            Call::Amend {
                                before: Box::new(o),
                                price,
                            },
                        )]))
                    } else {
                        if new_price.is_none() && amount.is_none() {
                            return Err("WEEX amendment requires price or amount".into());
                        }
                        if self.account.has_unreconciled_fills() {
                            return Err("WEEX fills are awaiting order reconciliation; wait before amending".into());
                        }
                        let conditional = o.kind == OrderType::Conditional;
                        let brackets = self.replacement_brackets(cx, &o)?;
                        let opts = PlaceOrderOpts {
                            symbol: o.symbol.clone(),
                            kind: o.kind,
                            side: o.side,
                            amount: amount.unwrap_or(o.remaining),
                            price: if conditional {
                                o.extra
                                    .as_ref()
                                    .and_then(|e| e.conditional_order_price)
                                    .filter(|v| *v > 0.0)
                            } else {
                                Some(new_price.unwrap_or(o.price)).filter(|v| *v > 0.0)
                            },
                            trigger_price: conditional.then_some(new_price.unwrap_or(o.price)),
                            stop_loss: o.stop_loss,
                            take_profit: o.take_profit,
                            reduce_only: Some(o.reduce_only),
                            time_in_force: o.time_in_force,
                            trigger_type: o.trigger_type,
                            tp_trigger_type: brackets.tp,
                            sl_trigger_type: brackets.sl,
                            client_order_id: None,
                            close: o.close,
                            stp_mode: None,
                            max_slippage_pct: None,
                            bybit_bbo: None,
                            bybit_slippage_tolerance_ticks: None,
                            implied_volatility: None,
                        };
                        // Replacement preflight runs before the destructive cancel.
                        let mut replacement = self.plan_orders(cx, vec![opts])?;
                        if let Some(step) = replacement.front_mut() {
                            step.replacement = Some(ReplacementGuard {
                                id: o.id.clone(),
                                amount: o.amount,
                                filled: o.filled,
                            });
                        }
                        let mut cancel = self.plan_cancel(cx, vec![o.id])?;
                        cancel.extend(replacement);
                        Ok(cancel)
                    }
                })();
                (command, planned)
            }
            Command::CancelSymbolOrders { command, symbol } => {
                continue_on_error = true;
                let planned = if cx.store.market(&symbol).is_none() {
                    Err("WEEX market not found".into())
                } else {
                    Ok([false, true]
                        .into_iter()
                        .map(|is_algo| Step {
                            replacement: None,
                            method: Method::Delete,
                            path: if is_algo {
                                "/capi/v3/algoOpenOrders"
                            } else {
                                "/capi/v3/allOpenOrders"
                            },
                            query: format!("symbol={symbol}"),
                            body: None,
                            call: Call::Cancel {
                                before: cx
                                    .store
                                    .orders()
                                    .iter()
                                    .filter(|o| {
                                        o.symbol == symbol
                                            && algo(o) == is_algo
                                            && !matches!(
                                                o.status,
                                                OrderStatus::Closed | OrderStatus::Canceled
                                            )
                                    })
                                    .cloned()
                                    .collect(),
                                native: vec![],
                                all: true,
                            },
                        })
                        .collect())
                };
                (command, planned)
            }
            Command::SetLeverage {
                command,
                symbol,
                leverage,
            } => {
                refresh = true;
                let planned = (|| {
                    let m = cx.store.market(&symbol).ok_or("WEEX market not found")?;
                    if !leverage.is_finite() || leverage <= 0.0 {
                        return Err("WEEX invalid leverage".into());
                    }
                    let leverage = leverage.clamp(m.limits.leverage.min, m.limits.leverage.max);
                    let margin = cx
                        .store
                        .setting(&symbol)
                        .and_then(|s| s.margin_type)
                        .ok_or("WEEX symbol settings not loaded")?;
                    let value = num::format(leverage);
                    let payload = Leverage {
                        symbol: symbol.clone(),
                        margin_type: margin_text(margin),
                        cross_leverage: (margin == MarginType::Cross).then_some(value.clone()),
                        isolated_long_leverage: (margin == MarginType::Isolated)
                            .then_some(value.clone()),
                        isolated_short_leverage: (margin == MarginType::Isolated).then_some(value),
                    };
                    Ok(VecDeque::from([post(
                        "/capi/v3/account/leverage",
                        json(&payload),
                        Call::Leverage { symbol, leverage },
                    )]))
                })();
                (command, planned)
            }
            Command::SetMarginType {
                command,
                symbol,
                isolated,
            } => {
                refresh = true;
                let margin = if isolated {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                };
                let planned = if cx.store.market(&symbol).is_none() {
                    Err("WEEX market not found".into())
                } else {
                    let payload = Margin {
                        symbol: symbol.clone(),
                        margin_type: margin_text(margin),
                        separated_type: mode_text(
                            self.account
                                .configs
                                .get(&symbol)
                                .map_or(cx.store.hedged(), |c| c.hedged),
                        ),
                    };
                    Ok(VecDeque::from([post(
                        "/capi/v3/account/marginType",
                        json(&payload),
                        Call::Margin { symbol, margin },
                    )]))
                };
                (command, planned)
            }
            Command::SetPositionMode {
                command,
                hedged: value,
            } => {
                refresh = true;
                hedged = Some(value);
                let symbols: HashSet<_> = cx
                    .store
                    .positions()
                    .iter()
                    .map(|p| p.symbol.clone())
                    .collect();
                let mut symbols: Vec<_> = if symbols.is_empty() {
                    cx.store
                        .markets()
                        .iter()
                        .map(|m| m.symbol.clone())
                        .collect()
                } else {
                    symbols.into_iter().collect()
                };
                symbols.sort();
                let planned = symbols
                    .into_iter()
                    .map(|symbol| {
                        let margin = cx
                            .store
                            .setting(&symbol)
                            .and_then(|s| s.margin_type)
                            .ok_or("WEEX symbol settings not loaded")?;
                        Ok(post(
                            "/capi/v3/account/marginType",
                            json(&Margin {
                                symbol: symbol.clone(),
                                margin_type: margin_text(margin),
                                separated_type: mode_text(value),
                            }),
                            Call::Mode {
                                symbol,
                                hedged: value,
                            },
                        ))
                    })
                    .collect::<Result<VecDeque<_>, String>>();
                (command, planned)
            }
            other => return Some(other),
        };
        let steps = match planned {
            Ok(v) => v,
            Err(error) => {
                reject(cx, id, error);
                return None;
            }
        };
        if !steps.is_empty() && self.credentials.is_none() {
            reject(cx, id, "WEEX private credentials required".into());
            return None;
        }
        if self.trading.jobs.contains_key(&id) {
            cx.log(LogLevel::Error, "WEEX command ID is already in flight");
            return None;
        }
        self.trading.jobs.insert(
            id,
            Job {
                steps,
                ids: Vec::new(),
                error: None,
                continue_on_error,
                refresh,
                hedged,
            },
        );
        self.trade_next(cx, id);
        None
    }

    fn trade_next(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let Some(job) = self.trading.jobs.get_mut(&command) else {
            return;
        };
        if job.error.is_some() && !job.continue_on_error {
            job.steps.clear();
        }
        if let Some(step) = job.steps.pop_front() {
            if let Some(guard) = &step.replacement
                && !cx.store.order(&guard.id).is_some_and(|order| {
                    order.status == OrderStatus::Canceled
                        && order.amount == guard.amount
                        && order.filled == guard.filled
                })
            {
                job.error=Some("WEEX order changed while canceling; replacement not submitted; reconcile before retrying".into());
                self.refresh_account(cx);
                self.trade_next(cx, command);
                return;
            }
            if matches!(
                &step.call,
                Call::Place { .. } | Call::Cancel { .. } | Call::Amend { .. }
            ) {
                self.account.invalidate_orders();
            } else {
                self.account.invalidate_settings();
            }
            match &step.call {
                Call::Place { orders, .. } => {
                    for o in orders {
                        cx.store.upsert_order(o.clone());
                    }
                }
                Call::Cancel { before, .. } => {
                    for o in before {
                        cx.store.update_order(&o.id, |row| {
                            if !matches!(row.status, OrderStatus::Closed | OrderStatus::Canceled) {
                                row.status = OrderStatus::Canceling;
                            }
                        });
                    }
                }
                Call::Amend { before, .. } => {
                    cx.store.set_order_status(&before.id, OrderStatus::Updating);
                }
                _ => {}
            }
            if let Call::Place { orders, .. } = &step.call {
                cx.command_progress(
                    command,
                    orders
                        .iter()
                        .filter_map(|order| {
                            order
                                .client_order_id
                                .as_ref()
                                .map(|identity| OrderIdentityReceipt {
                                    symbol: order.symbol.clone(),
                                    kind: OrderIdentityKind::ClientOrderId,
                                    identity: identity.clone(),
                                })
                        })
                        .collect(),
                );
            }
            self.signed_request(
                cx,
                step.method,
                step.path,
                &step.query,
                step.body,
                Task::Trade(Request {
                    command,
                    call: step.call,
                    revision: self.account.order_revision(),
                }),
                None,
            );
        } else {
            let job = self.trading.jobs.remove(&command).unwrap();
            if job.refresh {
                self.refresh_account(cx);
            }
            if let Some(error) = job.error {
                reject(cx, command, error);
            } else {
                if let Some(hedged) = job.hedged {
                    cx.store.set_hedged(hedged);
                }
                cx.done(command, Ok(json(&job.ids)));
            }
        }
    }

    pub(crate) fn trade_failed(&mut self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        rollback(
            cx,
            &request.call,
            request.revision == self.account.order_revision(),
        );
        self.account.invalidate_orders();
        if let Some(job) = self.trading.jobs.get_mut(&request.command) {
            job.error.get_or_insert_with(|| error.into());
        }
        // Reconcile ambiguous outcomes through reads; never resend the write.
        self.refresh_account(cx);
        self.trade_next(cx, request.command);
    }

    pub(crate) fn trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if response.status >= 400 {
            self.trade_failed(
                cx,
                request,
                &format!("WEEX HTTP {}: {}", response.status, response.body),
            );
            return;
        }
        let owned = request.revision == self.account.order_revision();
        // A fill frame can revoke ownership before the cumulative order frame
        // changes cached quantities. Never replace from that stale amount.
        let replacement_lost = !owned
            && matches!(&request.call, Call::Cancel { .. })
            && self.trading.jobs.get(&request.command).is_some_and(|job| {
                job.steps
                    .front()
                    .is_some_and(|step| step.replacement.is_some())
            });
        let result = self.apply_trade_response(cx, &request.call, response.body, owned);
        self.account.invalidate_orders();
        if result.is_err() || replacement_lost {
            self.refresh_account(cx);
        }
        if matches!(
            &request.call,
            Call::Leverage { .. } | Call::Margin { .. } | Call::Mode { .. }
        ) {
            self.account.invalidate_settings();
        }
        if matches!(&request.call, Call::Cancel { .. }) {
            let terminal: Vec<_> = cx
                .store
                .orders()
                .iter()
                .filter(|o| o.status == OrderStatus::Canceled && o.last_update == cx.now)
                .map(|o| o.id.clone())
                .collect();
            for id in terminal {
                self.account.terminal(id, cx.now);
            }
        }
        if let Some(job) = self.trading.jobs.get_mut(&request.command) {
            if replacement_lost {
                job.error.get_or_insert_with(||"WEEX order changed while canceling; replacement not submitted; reconcile before retrying".into());
            }
            match result {
                Ok(ids) => job.ids.extend(ids),
                Err(error) => {
                    job.error.get_or_insert(error);
                }
            }
        }
        self.trade_next(cx, request.command);
    }

    fn apply_trade_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        call: &Call,
        body: &str,
        owned: bool,
    ) -> Result<Vec<String>, String> {
        let result = (|| match call {
            Call::Place {
                orders,
                batch,
                protective,
                brackets,
            } => apply_place(
                cx,
                &mut self.trading.brackets,
                *brackets,
                orders,
                *batch,
                *protective,
                body,
            ),
            Call::Cancel {
                before,
                native,
                all,
            } => apply_cancel(cx, before, native, *all, body, owned),
            Call::Amend { before, price } => {
                let response = serde_json::from_str::<ResultRow>(body)
                    .map_err(|_| "WEEX malformed TP/SL amendment response".to_owned())?;
                if !response.success {
                    Err(response.error())
                } else {
                    cx.store.update_order(&before.id, |o| {
                        if owned && o.status == OrderStatus::Updating {
                            o.status = before.status;
                            o.price = *price;
                            o.last_update = cx.now;
                        }
                    });
                    Ok(vec![before.id.clone()])
                }
            }
            Call::Leverage { symbol, leverage } => {
                metadata(body, Some(symbol))?;
                cx.store.update_setting(symbol, |s| s.leverage = *leverage);
                if let Some(config) = self.account.configs.get_mut(symbol) {
                    config.setting.leverage = *leverage;
                    if config.setting.margin_type == Some(MarginType::Isolated) {
                        config.isolated_long_leverage = *leverage;
                        config.isolated_short_leverage = *leverage;
                    }
                }
                Ok(vec![])
            }
            Call::Margin { symbol, margin } => {
                metadata(body, None)?;
                cx.store
                    .update_setting(symbol, |s| s.margin_type = Some(*margin));
                if let Some(config) = self.account.configs.get_mut(symbol) {
                    config.setting.margin_type = Some(*margin);
                }
                Ok(vec![])
            }
            Call::Mode { symbol, hedged } => {
                metadata(body, None)?;
                if let Some(config) = self.account.configs.get_mut(symbol) {
                    config.hedged = *hedged;
                }
                Ok(vec![])
            }
        })();
        if result.is_err() {
            rollback(cx, call, owned);
        }
        result
    }
}
fn flush(steps: &mut VecDeque<Step>, payloads: &mut Vec<Regular>, orders: &mut Vec<Order>) {
    if payloads.is_empty() {
        return;
    }
    let body = json(&Batch {
        batch_orders: std::mem::take(payloads),
    });
    steps.push_back(post(
        "/capi/v3/batchOrders",
        body,
        Call::Place {
            orders: std::mem::take(orders),
            batch: true,
            protective: false,
            brackets: None,
        },
    ));
}
fn rollback(cx: &mut Cx<'_, Pending>, call: &Call, owned: bool) {
    match call {
        Call::Place { orders, .. } => {
            let ids: Vec<_> = orders.iter().map(|o| o.id.as_str()).collect();
            cx.store.remove_orders(&ids);
        }
        Call::Cancel { before, .. } => {
            for o in before {
                cx.store.update_order(&o.id, |row| {
                    if owned && row.status == OrderStatus::Canceling {
                        row.status = o.status;
                    }
                });
            }
        }
        Call::Amend { before, .. } => {
            cx.store.update_order(&before.id, |row| {
                if owned && row.status == OrderStatus::Updating {
                    row.status = before.status;
                }
            });
        }
        _ => {}
    }
}
fn apply_place(
    cx: &mut Cx<'_, Pending>,
    provenance: &mut HashMap<String, Brackets>,
    brackets: Option<Brackets>,
    orders: &[Order],
    batch: bool,
    protective: bool,
    body: &str,
) -> Result<Vec<String>, String> {
    let decoded: Rows = serde_json::from_str(body).map_err(|_| "WEEX malformed order response")?;
    if batch && !matches!(&decoded, Rows::Many(_)) {
        return Err("WEEX batch response must be an array".into());
    }
    let rows = decoded.into_vec();
    if rows.len() != orders.len() {
        return Err("WEEX order response row count mismatch".into());
    }
    let mut correlated = HashMap::new();
    let mut native_ids = HashSet::new();
    for (i, row) in rows.iter().enumerate() {
        let client = if protective {
            orders[i].client_order_id.as_deref().unwrap()
        } else {
            row.client_order_id
                .as_deref()
                .ok_or("WEEX response missing echoed clientOrderId")?
        };
        if !orders
            .iter()
            .any(|o| o.client_order_id.as_deref() == Some(client))
            || correlated.insert(client, row).is_some()
        {
            return Err("WEEX response has unknown/duplicate clientOrderId".into());
        }
        if row.success {
            let id = row
                .order_id
                .as_ref()
                .ok_or("WEEX accepted order missing orderId")?
                .id()?;
            if !native_ids.insert(id) {
                return Err("WEEX response duplicated a native orderId".into());
            }
        }
    }
    let mut ids = Vec::new();
    let mut errors = Vec::new();
    for order in orders {
        let client = order.client_order_id.as_deref().unwrap();
        let row = correlated
            .get(client)
            .ok_or("WEEX response omitted a clientOrderId")?;
        cx.store.remove_orders(&[&order.id]);
        if row.success {
            let id = row.order_id.as_ref().unwrap().id()?;
            if cx.store.order(&id).is_some() {
                // An execution/private update can precede this REST ack; its
                // quantities and terminal state are authoritative.
                cx.store.update_order(&id, |o| {
                    o.client_order_id = order.client_order_id.clone();
                    o.time_in_force = order.time_in_force;
                    o.take_profit = order.take_profit;
                    o.stop_loss = order.stop_loss;
                });
            } else {
                let mut accepted = order.clone();
                accepted.id = id.clone();
                accepted.status = OrderStatus::Open;
                accepted.last_update = cx.now;
                cx.store.upsert_order(accepted);
            }
            if let Some(brackets) = brackets {
                provenance.insert(id.clone(), brackets);
            }
            ids.push(id);
        } else {
            errors.push(format!("{client}: {}", row.error()));
        }
    }
    if errors.is_empty() {
        Ok(ids)
    } else {
        Err(errors.join("; "))
    }
}
fn apply_cancel(
    cx: &mut Cx<'_, Pending>,
    before: &[Order],
    native: &[String],
    all: bool,
    body: &str,
    owned: bool,
) -> Result<Vec<String>, String> {
    let rows = serde_json::from_str::<Rows>(body)
        .map_err(|_| "WEEX malformed cancellation response")?
        .into_vec();
    let mut seen = HashSet::new();
    let mut results = Vec::new();
    for row in rows {
        let id = row
            .order_id
            .as_ref()
            .ok_or("WEEX cancellation missing orderId")?
            .id()?;
        if !seen.insert(id.clone()) || (!all && !native.contains(&id)) {
            return Err("WEEX cancellation returned unknown/duplicate orderId".into());
        }
        results.push((id, row));
    }
    if !all && seen.len() != native.len() {
        return Err("WEEX cancellation response row count mismatch".into());
    }
    let mut errors = Vec::new();
    for (id, row) in results {
        if row.success {
            // A private event may introduce an order after cancel-all was
            // dispatched. The accepted response names the authoritative IDs.
            let affected: Vec<_> = cx
                .store
                .orders()
                .iter()
                .filter(|o| o.id.split("__").next() == Some(id.as_str()))
                .map(|o| o.id.clone())
                .collect();
            for id in affected {
                cx.store.update_order(&id, |current| {
                    if current.status != OrderStatus::Closed {
                        current.status = OrderStatus::Canceled;
                        current.last_update = cx.now;
                    }
                });
            }
        } else {
            errors.push(format!("{id}: {}", row.error()));
        }
    }
    // A cancel-all response is an acknowledgement per native order, not an
    // instruction to mark every cached order canceled. Unreported rows await
    // reconciliation; explicitly rejected rows retain their original status.
    for o in before {
        cx.store.update_order(&o.id, |row| {
            if owned && row.status == OrderStatus::Canceling {
                row.status = o.status;
            }
        });
    }
    if errors.is_empty() {
        Ok(vec![])
    } else {
        Err(errors.join("; "))
    }
}
fn metadata(body: &str, symbol: Option<&String>) -> Result<(), String> {
    let row = serde_json::from_str::<MetadataResult>(body)
        .map_err(|_| "WEEX malformed settings response")?;
    if row.success == Some(false) || row.code.as_ref().is_some_and(|c| !c.success()) {
        return Err(if row.msg.is_empty() {
            "WEEX settings update rejected".into()
        } else {
            row.msg
        });
    }
    if row.success == Some(true) || row.code.as_ref().is_some_and(Code::success) {
        return Ok(());
    }
    if symbol.is_some_and(|s| row.symbol.as_ref() == Some(s))
        && matches!(row.margin_type.as_deref(), Some("CROSSED" | "ISOLATED"))
        && (row.cross_leverage.is_some()
            || row.isolated_long_leverage.is_some()
            || row.isolated_short_leverage.is_some())
    {
        return Ok(());
    }
    Err("WEEX settings response has no success acknowledgement".into())
}
