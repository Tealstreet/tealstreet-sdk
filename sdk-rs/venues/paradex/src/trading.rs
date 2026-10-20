use crate::{
    Pending,
    account::{Account, AmendTarget},
    sign::Credentials,
    wire::Number,
};
use safe_cex_engine::model::{OrderSide, OrderType, PlaceOrderOpts, TimeInForce};
use safe_cex_engine::num;
use safe_cex_engine::{Command, CommandId, Cx, LogLevel, Method, Response};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Serialize)]
struct Placement {
    market: String,
    side: &'static str,
    #[serde(rename = "type")]
    kind: &'static str,
    size: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_price: Option<String>,
    instruction: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    flags: Option<[&'static str; 1]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stp: Option<&'static str>,
    signature: String,
    signature_timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<String>,
}
#[derive(Clone)]
enum Kind {
    Place {
        expected: Vec<Option<String>>,
        child: bool,
        batch: bool,
    },
    Amend {
        id: String,
        target: Box<AmendTarget>,
    },
    Cancel(Vec<String>),
    Empty,
    Rails(String),
    Quotes(String),
    Leverage {
        symbol: String,
        leverage: f64,
    },
}
struct Operation {
    method: Method,
    path: String,
    body: Option<String>,
    kind: Kind,
}
struct Job {
    command: CommandId,
    preparing: Option<Preparation>,
    operations: VecDeque<Operation>,
    results: Vec<String>,
    errors: Vec<String>,
    attempt_all: bool,
    placement: bool,
}
struct Preparation {
    orders: Vec<PlaceOrderOpts>,
    symbols: VecDeque<String>,
    quotes: HashMap<String, Quote>,
    price_bands: HashMap<String, f64>,
}
#[derive(Clone, Copy)]
struct Quote {
    bid: Option<f64>,
    ask: Option<f64>,
    fair: Option<f64>,
}
pub struct Request {
    job: u64,
    kind: Kind,
}
impl Request {
    pub(crate) fn amend_id(&self) -> Option<&str> {
        if let Kind::Amend { id, .. } = &self.kind {
            Some(id)
        } else {
            None
        }
    }
}
#[derive(Default)]
pub struct Trading {
    next: u64,
    jobs: HashMap<u64, Job>,
    amend_locks: HashSet<String>,
}

fn encode(text: &str) -> String {
    let mut output = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            output.push(char::from(b));
        } else {
            use std::fmt::Write;
            write!(output, "%{b:02X}").unwrap();
        }
    }
    output
}
fn positive(v: f64) -> bool {
    v.is_finite() && v > 0.
}
fn side(side: OrderSide) -> &'static str {
    if side == OrderSide::Buy {
        "BUY"
    } else {
        "SELL"
    }
}
fn instruction(tif: Option<TimeInForce>) -> &'static str {
    match tif {
        Some(TimeInForce::ImmediateOrCancel | TimeInForce::FillOrKill) => "IOC",
        Some(TimeInForce::PostOnly) => "POST_ONLY",
        _ => "GTC",
    }
}
fn native_batch(o: &PlaceOrderOpts) -> bool {
    o.kind == OrderType::Limit
        && o.trigger_price.is_none()
        && o.stop_loss.is_none()
        && o.take_profit.is_none()
        && o.tp_trigger_type.is_none()
        && o.sl_trigger_type.is_none()
        && o.close != Some(true)
        && o.time_in_force != Some(TimeInForce::FillOrKill)
}
fn client_id() -> Result<String, String> {
    safe_cex_crypto::uuid::uuid().map(|id| format!("ts_{id}"))
}
fn prepare(
    o: &PlaceOrderOpts,
    signer: &Credentials,
    now: u64,
    child: bool,
    execution: Option<f64>,
) -> Result<Placement, String> {
    if !positive(o.amount) {
        return Err("Paradex order requires a finite positive amount".into());
    }
    let kind = if execution.is_some() {
        "LIMIT"
    } else {
        match o.kind {
            OrderType::Limit => "LIMIT",
            OrderType::Market => "MARKET",
            OrderType::Conditional => "STOP_LIMIT",
            OrderType::StopLoss => "STOP_MARKET",
            OrderType::TakeProfit => "TAKE_PROFIT_MARKET",
            OrderType::TrailingStopLoss => {
                return Err("Paradex trailing orders are unsupported".into());
            }
        }
    };
    if o.kind == OrderType::Conditional {
        if !o.price.is_some_and(positive) || !o.trigger_price.is_some_and(positive) {
            return Err(
                "Paradex opening trigger requires positive execution and trigger prices".into(),
            );
        }
        if o.reduce_only == Some(true) || o.stop_loss.is_some() || o.take_profit.is_some() {
            return Err(
                "Paradex opening trigger cannot be reduce-only or carry attached children".into(),
            );
        }
    }
    if o.kind == OrderType::Limit && !o.price.is_some_and(positive) {
        return Err("Paradex limit order requires a finite positive price".into());
    }
    let stop = matches!(o.kind, OrderType::StopLoss | OrderType::TakeProfit);
    let trigger = if o.kind == OrderType::Conditional {
        o.trigger_price
    } else if stop {
        o.price
    } else {
        o.trigger_price
    };
    if stop && !trigger.is_some_and(positive) {
        return Err("Paradex stop requires a finite positive trigger price".into());
    }
    let size = num::format(o.amount);
    let price = matches!(kind, "LIMIT" | "STOP_LIMIT")
        .then(|| num::format(execution.or(o.price).expect("validated execution price")));
    let signed = signer.order(now, &o.symbol, side(o.side), kind, &size, price.as_deref())?;
    Ok(Placement {
        market: o.symbol.clone(),
        side: side(o.side),
        kind,
        size,
        price,
        trigger_price: trigger.map(num::format),
        instruction: if execution.is_some() {
            "IOC"
        } else {
            instruction(o.time_in_force)
        },
        flags: (o.reduce_only == Some(true) || stop).then_some(["REDUCE_ONLY"]),
        stp: (!child).then_some("EXPIRE_TAKER"),
        signature: signed.signature,
        signature_timestamp: signed.signature_timestamp,
        client_id: if child {
            None
        } else {
            Some(o.client_order_id.clone().map_or_else(client_id, Ok)?)
        },
    })
}
fn validate_caller_ids(orders: &[PlaceOrderOpts], cx: &Cx<'_, Pending>) -> Result<(), String> {
    let mut seen = HashSet::new();
    for o in orders {
        let market = cx
            .store
            .market(&o.symbol)
            .ok_or_else(|| format!("unknown market {}", o.symbol))?;
        if let Some(id) = &o.client_order_id
            && (id.is_empty()
                || id.chars().count() > 64
                || id.contains(['\n', '\r'])
                || !seen.insert(id)
                || !matches!(o.kind, OrderType::Limit | OrderType::Market)
                || o.trigger_price.is_some()
                || o.stop_loss.is_some()
                || o.take_profit.is_some()
                || o.close == Some(true)
                || (market.limits.amount.max > 0. && o.amount > market.limits.amount.max))
        {
            return Err("Paradex cannot honor this caller client order ID exactly once".into());
        }
    }
    Ok(())
}
fn place_operations(
    orders: &[PlaceOrderOpts],
    cx: &Cx<'_, Pending>,
    account: &Account,
    quotes: &HashMap<String, Quote>,
    bands: &HashMap<String, f64>,
    require_quotes: bool,
) -> Result<VecDeque<Operation>, String> {
    if orders
        .iter()
        .any(|o| o.bybit_bbo.is_some() || o.bybit_slippage_tolerance_ticks.is_some())
    {
        return Err("Bybit BBO and TickSize controls are not supported by Paradex".into());
    }
    validate_caller_ids(orders, cx)?;
    let signer = account
        .credentials()
        .ok_or("Paradex signer is not initialized")?;
    let mut operations = VecDeque::new();
    let mut chunk = Vec::new();
    fn flush(
        chunk: &mut Vec<Placement>,
        operations: &mut VecDeque<Operation>,
    ) -> Result<(), String> {
        if chunk.is_empty() {
            return Ok(());
        }
        let expected = chunk.iter().map(|p| p.client_id.clone()).collect();
        let body = serde_json::to_string(&*chunk).map_err(|e| e.to_string())?;
        operations.push_back(Operation {
            method: Method::Post,
            path: "/orders/batch".into(),
            body: Some(body),
            kind: Kind::Place {
                expected,
                child: false,
                batch: true,
            },
        });
        chunk.clear();
        Ok(())
    }
    for o in orders {
        let execution = if capped(o) {
            if !o.max_slippage_pct.is_some_and(f64::is_finite)
                || !matches!(o.time_in_force, None | Some(TimeInForce::ImmediateOrCancel))
            {
                return Err(
                    "Paradex capped market orders require finite slippage and IOC time-in-force"
                        .into(),
                );
            }
            match quotes.get(&o.symbol) {
                Some(quote) => Some(capped_price(o, *quote, bands.get(&o.symbol).copied(), cx)?),
                None if require_quotes => {
                    return Err("Paradex capped market quote is unavailable".into());
                }
                None => None,
            }
        } else {
            None
        };
        if native_batch(o) {
            chunk.push(prepare(o, signer, cx.now as u64, false, execution)?);
            if chunk.len() == 10 {
                flush(&mut chunk, &mut operations)?;
            }
            continue;
        }
        flush(&mut chunk, &mut operations)?;
        let payload = prepare(o, signer, cx.now as u64, false, execution)?;
        let expected = vec![payload.client_id.clone()];
        operations.push_back(Operation {
            method: Method::Post,
            path: "/orders".into(),
            body: Some(serde_json::to_string(&payload).map_err(|e| e.to_string())?),
            kind: Kind::Place {
                expected,
                child: false,
                batch: false,
            },
        });
        let brackets_valid = o.price.is_none_or(|entry| match o.side {
            OrderSide::Buy => {
                o.take_profit.is_none_or(|p| p > entry) && o.stop_loss.is_none_or(|p| p < entry)
            }
            OrderSide::Sell => {
                o.take_profit.is_none_or(|p| p < entry) && o.stop_loss.is_none_or(|p| p > entry)
            }
        });
        if !brackets_valid {
            continue;
        }
        for (kind, price) in [
            (OrderType::TakeProfit, o.take_profit),
            (OrderType::StopLoss, o.stop_loss),
        ] {
            if let Some(price) = price {
                if !positive(price) {
                    return Err("Paradex bracket price must be finite and positive".into());
                }
                let mut child = o.clone();
                child.kind = kind;
                child.side = o.side.opposite();
                child.price = Some(price);
                child.trigger_price = None;
                child.client_order_id = None;
                child.take_profit = None;
                child.stop_loss = None;
                child.reduce_only = Some(true);
                child.time_in_force = None;
                let mut payload = prepare(&child, signer, cx.now as u64, true, None)?;
                if kind == OrderType::StopLoss {
                    payload.kind = "STOP_LOSS_MARKET";
                    let signed = signer.order(
                        cx.now as u64,
                        &o.symbol,
                        side(child.side),
                        payload.kind,
                        &payload.size,
                        None,
                    )?;
                    payload.signature = signed.signature;
                    payload.signature_timestamp = signed.signature_timestamp;
                }
                operations.push_back(Operation {
                    method: Method::Post,
                    path: "/orders".into(),
                    body: Some(serde_json::to_string(&payload).map_err(|e| e.to_string())?),
                    kind: Kind::Place {
                        expected: vec![None],
                        child: true,
                        batch: false,
                    },
                });
            }
        }
    }
    flush(&mut chunk, &mut operations)?;
    Ok(operations)
}

impl Trading {
    pub fn command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        account: &mut Account,
        command: Command,
        price_bands: &HashMap<String, f64>,
    ) -> bool {
        if let Command::PlaceOrders { command, orders } = &command
            && orders.iter().any(capped)
        {
            if let Err(error) =
                place_operations(orders, cx, account, &HashMap::new(), price_bands, false)
            {
                cx.done(*command, Err(error));
                return true;
            }
            let mut seen = HashSet::new();
            let symbols = orders
                .iter()
                .filter(|o| capped(o) && seen.insert(o.symbol.clone()))
                .map(|o| o.symbol.clone())
                .collect();
            self.next = self.next.wrapping_add(1);
            let job = self.next;
            self.jobs.insert(
                job,
                Job {
                    command: *command,
                    preparing: Some(Preparation {
                        orders: orders.clone(),
                        symbols,
                        quotes: HashMap::new(),
                        price_bands: price_bands.clone(),
                    }),
                    operations: VecDeque::new(),
                    results: vec![],
                    errors: vec![],
                    attempt_all: false,
                    placement: true,
                },
            );
            self.advance(cx, account, job);
            return true;
        }
        let (command, operations, attempt_all, placement) = match command {
            Command::PlaceOrders { command, orders } => (
                command,
                place_operations(&orders, cx, account, &HashMap::new(), price_bands, true),
                false,
                true,
            ),
            Command::CancelOrders { command, ids } => {
                let mut seen = HashSet::new();
                let result = if ids
                    .iter()
                    .any(|id| id.trim().is_empty() || !seen.insert(id))
                {
                    Err("Paradex cancellation requires unique nonempty IDs".into())
                } else {
                    let mut operations = VecDeque::new();
                    for chunk in ids.chunks(2) {
                        operations.push_back(Operation {
                            method: Method::Delete,
                            path: if chunk.len() == 2 {
                                "/orders/batch".into()
                            } else {
                                format!("/orders/{}", encode(&chunk[0]))
                            },
                            body: if chunk.len() == 2 {
                                #[derive(Serialize)]
                                struct Cancel<'a> {
                                    order_ids: &'a [String],
                                }
                                Some(serde_json::to_string(&Cancel { order_ids: chunk }).unwrap())
                            } else {
                                None
                            },
                            kind: Kind::Cancel(chunk.to_vec()),
                        });
                    }
                    Ok(operations)
                };
                (command, result, true, false)
            }
            Command::CancelSymbolOrders { command, symbol } => (
                command,
                cx.store
                    .market(&symbol)
                    .ok_or_else(|| "Paradex cancel-all requires a known nonempty market".to_owned())
                    .map(|market| {
                        VecDeque::from([Operation {
                            method: Method::Delete,
                            path: format!("/orders?market={}", encode(&market.id)),
                            body: None,
                            kind: Kind::Empty,
                        }])
                    }),
                false,
                false,
            ),
            Command::FetchLeverage { command, symbol } => (
                command,
                Ok(VecDeque::from([Operation {
                    method: Method::Get,
                    path: "/account/margin".into(),
                    body: None,
                    kind: Kind::Rails(symbol),
                }])),
                false,
                false,
            ),
            Command::SetLeverage {
                command,
                symbol,
                leverage,
            } => {
                let result = (|| {
                    if !leverage.is_finite() {
                        return Err("Paradex leverage must be finite".into());
                    }
                    let m = cx.store.market(&symbol).ok_or("unknown Paradex market")?;
                    let leverage = leverage
                        .floor()
                        .clamp(m.limits.leverage.min.max(1.), m.limits.leverage.max.max(1.));
                    #[derive(Serialize)]
                    struct Leverage {
                        leverage: f64,
                        margin_type: &'static str,
                    }
                    Ok(VecDeque::from([Operation {
                        method: Method::Post,
                        path: format!("/account/margin/{}", encode(&symbol)),
                        body: Some(
                            serde_json::to_string(&Leverage {
                                leverage,
                                margin_type: "CROSS",
                            })
                            .unwrap(),
                        ),
                        kind: Kind::Leverage { symbol, leverage },
                    }]))
                })();
                (command, result, false, false)
            }
            Command::SetMarginType {
                command, isolated, ..
            } => {
                cx.done(
                    command,
                    if isolated {
                        Err("Paradex isolated account routing is unsupported".into())
                    } else {
                        Ok("[]".into())
                    },
                );
                return true;
            }
            Command::SetPositionMode { command, hedged } => {
                cx.done(
                    command,
                    if hedged {
                        Err("Paradex supports one-way mode only".into())
                    } else {
                        Ok("[]".into())
                    },
                );
                return true;
            }
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => (
                command,
                self.amend(cx, account, &id, price, amount),
                false,
                true,
            ),
            _ => return false,
        };
        if account.credentials().is_none() {
            cx.done(
                command,
                Err("Paradex private account is not initialized".into()),
            );
            return true;
        }
        match operations {
            Ok(operations) => {
                self.next = self.next.wrapping_add(1);
                let job = self.next;
                self.jobs.insert(
                    job,
                    Job {
                        command,
                        preparing: None,
                        operations,
                        results: Vec::new(),
                        errors: Vec::new(),
                        attempt_all,
                        placement,
                    },
                );
                self.advance(cx, account, job);
            }
            Err(error) => cx.done(command, Err(error)),
        }
        true
    }
    fn advance(&mut self, cx: &mut Cx<'_, Pending>, account: &mut Account, job: u64) {
        if let Some(preparation) = self
            .jobs
            .get_mut(&job)
            .and_then(|state| state.preparing.as_mut())
        {
            if let Some(symbol) = preparation.symbols.pop_front() {
                cx.request(
                    Method::Get,
                    format!("{}/orderbook/{}?depth=5", account.rest(), encode(&symbol)),
                    Vec::new(),
                    None,
                    Pending::Trade(Request {
                        job,
                        kind: Kind::Quotes(symbol),
                    }),
                );
                return;
            }
            let preparation = self
                .jobs
                .get_mut(&job)
                .and_then(|state| state.preparing.take())
                .expect("pending plan");
            match place_operations(
                &preparation.orders,
                cx,
                account,
                &preparation.quotes,
                &preparation.price_bands,
                true,
            ) {
                Ok(operations) => {
                    self.jobs.get_mut(&job).expect("pending job").operations = operations
                }
                Err(error) => {
                    let state = self.jobs.remove(&job).expect("pending job");
                    cx.done(state.command, Err(error));
                    return;
                }
            }
        }
        let Some(state) = self.jobs.get_mut(&job) else {
            return;
        };
        if let Some(operation) = state.operations.pop_front() {
            if let Kind::Amend { id, target } = &operation.kind {
                if let Err(error) = account.begin_amend(id, (**target).clone()) {
                    let state = self.jobs.remove(&job).expect("pending amendment");
                    cx.done(state.command, Err(error));
                    return;
                }
                self.amend_locks.insert(id.clone());
            }
            account.bump_orders();
            account.bump_positions();
            account.request(
                cx,
                operation.method,
                operation.path,
                operation.body,
                None,
                Pending::Trade(Request {
                    job,
                    kind: operation.kind,
                }),
            );
        } else {
            let state = self.jobs.remove(&job).unwrap();
            let result = if state.errors.is_empty() {
                if state.placement {
                    serde_json::to_string(&state.results).map_err(|e| e.to_string())
                } else {
                    Ok("[]".into())
                }
            } else {
                Err(state.errors.join("; "))
            };
            cx.done(state.command, result);
        }
    }
    pub fn on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        account: &mut Account,
        request: Request,
        response: Response<'_>,
    ) {
        if let Kind::Quotes(symbol) = &request.kind {
            let result = crate::success(response).and_then(|()| decode_quote(response.body));
            match result {
                Ok(mut quote) => {
                    quote.fair = snapshot_fair(cx, symbol);
                    if let Some(preparation) = self
                        .jobs
                        .get_mut(&request.job)
                        .and_then(|state| state.preparing.as_mut())
                    {
                        preparation.quotes.insert(symbol.clone(), quote);
                    }
                    self.advance(cx, account, request.job);
                }
                Err(error) => self.complete(cx, account, request, Err(error), false),
            }
            return;
        }
        let rails = match &request.kind {
            Kind::Rails(symbol) => Some(symbol.clone()),
            _ => None,
        };
        let result = crate::success(response).and_then(|()| match &request.kind {
            Kind::Place {
                expected, batch, ..
            } => decode_placement(response.body, expected, *batch),
            Kind::Amend { id, .. } => {
                let row: Identity = serde_json::from_str(response.body)
                    .map_err(|_| "invalid Paradex amend response")?;
                if row.id != *id {
                    Err("Paradex amend did not confirm stable order ID".into())
                } else {
                    Ok(vec![id.clone()])
                }
            }
            Kind::Cancel(ids) if ids.len() == 2 => {
                decode_cancels(response.body, ids).map(|()| Vec::new())
            }
            Kind::Rails(symbol) => {
                #[derive(Deserialize)]
                struct Row {
                    market: String,
                    leverage: f64,
                }
                #[derive(Deserialize)]
                struct Margins {
                    configs: Vec<Row>,
                }
                let margins: Margins = serde_json::from_str(response.body)
                    .map_err(|_| "invalid Paradex margin settings")?;
                let leverage = margins
                    .configs
                    .iter()
                    .find(|m| m.market == *symbol)
                    .map(|m| m.leverage)
                    .filter(|v| positive(*v))
                    .unwrap_or(10.);
                account.set_leverage(cx, symbol, leverage);
                Ok(Vec::new())
            }
            Kind::Leverage { symbol, leverage } => {
                account.set_leverage(cx, symbol, *leverage);
                Ok(Vec::new())
            }
            _ => Ok(Vec::new()),
        });
        if result.is_ok() && matches!(request.kind, Kind::Amend { .. }) {
            account.amend_ack(cx, response.body);
        }
        if let Some(symbol) = rails
            && result.is_ok()
        {
            if let Some(job) = self.jobs.remove(&request.job) {
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Rails {
                    buy_leverage: f64,
                    sell_leverage: f64,
                }
                let leverage = cx.store.setting(&symbol).map_or(10., |s| s.leverage);
                cx.done(
                    job.command,
                    serde_json::to_string(&Rails {
                        buy_leverage: leverage,
                        sell_leverage: leverage,
                    })
                    .map_err(|e| e.to_string()),
                );
            }
        } else {
            let rejected = ((400..500).contains(&response.status)
                && !matches!(response.status, 408 | 429))
                || ((200..300).contains(&response.status) && crate::success(response).is_err());
            self.complete(cx, account, request, result, rejected);
        }
    }
    pub fn on_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        account: &mut Account,
        request: Request,
        error: &str,
    ) {
        self.complete(cx, account, request, Err(error.into()), false);
    }
    fn complete(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        account: &mut Account,
        request: Request,
        result: Result<Vec<String>, String>,
        rejected: bool,
    ) {
        if !matches!(request.kind, Kind::Quotes(_) | Kind::Rails(_)) {
            account.bump_orders();
            account.bump_positions();
        }
        if let Kind::Amend { id, .. } = &request.kind {
            self.amend_locks.remove(id);
            account.finish_amend(id, cx.now, rejected);
        }
        let Some(job) = self.jobs.get_mut(&request.job) else {
            return;
        };
        match result {
            Ok(ids) => job.results.extend(ids),
            Err(error) => {
                if matches!(request.kind, Kind::Place { child: true, .. }) {
                    cx.log(
                        LogLevel::Warning,
                        format!("Paradex TP/SL placement failed: {error}"),
                    );
                    while job
                        .operations
                        .front()
                        .is_some_and(|op| matches!(op.kind, Kind::Place { child: true, .. }))
                    {
                        job.operations.pop_front();
                    }
                } else {
                    job.errors.push(error);
                    if !job.attempt_all {
                        job.preparing = None;
                        job.operations.clear();
                    }
                }
            }
        }
        self.advance(cx, account, request.job);
    }
    fn amend(
        &self,
        cx: &Cx<'_, Pending>,
        account: &Account,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<VecDeque<Operation>, String> {
        if self.amend_locks.contains(id) {
            return Err("Paradex amendment is awaiting confirmation and reconciliation".into());
        }
        let raw = account
            .native_order(id)
            .ok_or("Paradex amend requires exactly reconciled native order")?;
        let order = cx
            .store
            .orders()
            .iter()
            .find(|o| o.id == id)
            .ok_or("Paradex amend requires open order")?;
        let limit = raw.kind == "LIMIT" && matches!(raw.status.as_str(), "NEW" | "OPEN");
        let trigger = is_trigger(&raw.kind) && raw.status == "UNTRIGGERED";
        if (!limit && !trigger)
            || !raw.on_behalf_of_account.is_empty()
            || raw.market != order.symbol
            || raw.side != side(order.side)
        {
            return Err("Paradex amend shape or isolated routing is unsupported".into());
        }
        let mapped = crate::account::map_order(raw)?;
        if mapped.kind != order.kind
            || mapped.reduce_only != order.reduce_only
            || (raw.reduce_only.is_none() && raw.flags.is_none())
            || (!matches!(
                raw.instruction.as_str(),
                "GTC" | "IOC" | "POST_ONLY" | "RPI"
            ) && raw.post_only.is_none()
                && !matches!(raw.time_in_force.as_str(), "GTT" | "FOK" | "IOC"))
        {
            return Err("Paradex amend immutable attributes are not reconciled".into());
        }
        let m = cx
            .store
            .market(&raw.market)
            .ok_or("unknown Paradex amend market")?;
        let size = raw
            .size
            .parse::<f64>()
            .map_err(|_| "invalid Paradex size")?;
        let remaining = raw
            .remaining_size
            .parse::<f64>()
            .map_err(|_| "invalid Paradex remaining size")?;
        if size <= 0.
            || remaining < 0.
            || remaining > size
            || size != order.amount
            || remaining != order.remaining
            || size - remaining != order.filled
        {
            return Err("Paradex amend size is not reconciled".into());
        }
        let raw_price = if trigger && raw.price.is_empty() {
            0.
        } else {
            raw.price
                .parse::<f64>()
                .map_err(|_| "invalid Paradex execution price")?
        };
        let raw_trigger = if trigger {
            raw.trigger_price
                .parse::<f64>()
                .map_err(|_| "invalid Paradex trigger price")?
        } else {
            0.
        };
        if !size.is_finite()
            || !remaining.is_finite()
            || !raw_price.is_finite()
            || raw_price < 0.
            || (limit && (raw_price <= 0. || raw_price != order.price))
            || (trigger && (!positive(raw_trigger) || raw_trigger != order.price))
        {
            return Err("Paradex amend existing prices or sizes are not reconciled".into());
        }
        let update_price = price;
        let size = num::adjust(amount.unwrap_or(size), m.precision.amount);
        let price = if limit {
            num::adjust(price.unwrap_or(raw_price), m.precision.price)
        } else {
            raw_price
        };
        let trigger_price =
            trigger.then(|| num::adjust(update_price.unwrap_or(raw_trigger), m.precision.price));
        if !positive(size)
            || size <= order.filled
            || !price.is_finite()
            || price < 0.
            || (limit && price <= 0.)
            || trigger_price.is_some_and(|p| !positive(p))
        {
            return Err("Paradex amendment size or price invalid".into());
        }
        let signed = account
            .credentials()
            .ok_or("Paradex signer is not initialized")?
            .modify(
                cx.now as u64,
                &raw.market,
                &raw.side,
                &raw.kind,
                &num::format(size),
                &num::format(price),
                id,
            )?;
        #[derive(Serialize)]
        struct Modify<'a> {
            id: &'a str,
            market: &'a str,
            side: &'a str,
            #[serde(rename = "type")]
            kind: &'a str,
            size: String,
            price: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            trigger_price: Option<String>,
            signature: String,
            signature_timestamp: u64,
        }
        Ok(VecDeque::from([Operation {
            method: Method::Put,
            path: format!("/orders/{}", encode(id)),
            body: Some(
                serde_json::to_string(&Modify {
                    id,
                    market: &raw.market,
                    side: &raw.side,
                    kind: &raw.kind,
                    size: num::format(size),
                    price: num::format(price),
                    trigger_price: trigger_price.map(num::format),
                    signature: signed.signature,
                    signature_timestamp: signed.signature_timestamp,
                })
                .unwrap(),
            ),
            kind: Kind::Amend {
                id: id.into(),
                target: Box::new(AmendTarget::new(
                    raw,
                    &num::format(size),
                    &num::format(price),
                    trigger_price.map(num::format).as_deref(),
                )?),
            },
        }]))
    }
}
fn capped(order: &PlaceOrderOpts) -> bool {
    order.kind == OrderType::Market && order.max_slippage_pct.is_some()
}
fn decode_quote(body: &str) -> Result<Quote, String> {
    #[derive(Deserialize)]
    struct Book {
        bids: Vec<[Number; 2]>,
        asks: Vec<[Number; 2]>,
    }
    let book: Book =
        serde_json::from_str(body).map_err(|_| "Invalid Paradex capped market orderbook")?;
    Ok(Quote {
        fair: None,
        bid: book
            .bids
            .first()
            .map(|level| level[0].0)
            .filter(|p| positive(*p)),
        ask: book
            .asks
            .first()
            .map(|level| level[0].0)
            .filter(|p| positive(*p)),
    })
}
fn round_price(price: f64, step: f64, up: bool) -> f64 {
    let step = if positive(step) { step } else { 0.000001 };
    let ratio = price / step;
    num::adjust(
        if up {
            (ratio - f64::EPSILON).ceil()
        } else {
            (ratio + f64::EPSILON).floor()
        } * step,
        step,
    )
}
fn snapshot_fair(cx: &Cx<'_, Pending>, symbol: &str) -> Option<f64> {
    let ticker = cx.store.ticker(symbol)?;
    let midpoint =
        (positive(ticker.bid) && positive(ticker.ask)).then_some((ticker.bid + ticker.ask) / 2.);
    [
        Some(ticker.mark),
        Some(ticker.index),
        ticker.mid_price,
        Some(ticker.last),
        midpoint,
    ]
    .into_iter()
    .flatten()
    .find(|p| positive(*p))
}
fn capped_price(
    order: &PlaceOrderOpts,
    quote: Quote,
    band: Option<f64>,
    cx: &Cx<'_, Pending>,
) -> Result<f64, String> {
    let buy = order.side == OrderSide::Buy;
    let touch = if buy { quote.ask } else { quote.bid }
        .filter(|p| positive(*p))
        .ok_or("Paradex capped market order needs a live touch price")?;
    let fair = quote.fair.unwrap_or(touch);
    let cap = order.max_slippage_pct.expect("capped market").max(0.) / 100.;
    let band_limit = band
        .filter(|b| positive(*b))
        .map(|width| fair * if buy { 1. + width } else { 1. - width });
    let mut raw = fair * if buy { 1. + cap } else { 1. - cap };
    if let Some(limit) = band_limit {
        raw = if buy { raw.min(limit) } else { raw.max(limit) };
    }
    let touch_inside =
        band_limit.is_none_or(|limit| if buy { touch <= limit } else { touch >= limit });
    if touch_inside && if buy { raw < touch } else { raw > touch } {
        raw = touch;
    }
    let step = cx
        .store
        .market(&order.symbol)
        .map_or(0.000001, |m| m.precision.price);
    let mut price = round_price(raw, step, buy);
    if let Some(limit) = band_limit {
        if buy && price > limit {
            price = round_price(limit, step, false);
        } else if !buy && price < limit {
            price = round_price(limit, step, true);
        }
    }
    if !positive(price) {
        return Err("Paradex capped market IOC price is invalid".into());
    }
    Ok(price)
}

fn is_trigger(kind: &str) -> bool {
    matches!(
        kind,
        "STOP_LIMIT"
            | "STOP_MARKET"
            | "STOP_LOSS_LIMIT"
            | "STOP_LOSS_MARKET"
            | "TAKE_PROFIT_LIMIT"
            | "TAKE_PROFIT_MARKET"
    )
}
#[derive(Deserialize)]
struct Identity {
    id: String,
    #[serde(default)]
    client_id: Option<String>,
}
fn decode_placement(
    body: &str,
    expected: &[Option<String>],
    batch: bool,
) -> Result<Vec<String>, String> {
    #[derive(Deserialize)]
    struct Failure {
        #[serde(default)]
        error: String,
        #[serde(default)]
        message: String,
    }
    #[derive(Deserialize)]
    struct Batch {
        #[serde(default)]
        orders: Vec<Identity>,
        #[serde(default)]
        errors: Vec<Failure>,
    }
    let (rows, errors) = if batch {
        let response: Batch =
            serde_json::from_str(body).map_err(|_| "invalid Paradex batch placement envelope")?;
        (response.orders, response.errors)
    } else {
        (
            vec![
                serde_json::from_str::<Identity>(body)
                    .map_err(|_| "invalid Paradex placement response")?,
            ],
            Vec::new(),
        )
    };
    let mut found = HashMap::new();
    let mut seen_ids = HashSet::new();
    for row in rows {
        if row.id.trim().is_empty() || !seen_ids.insert(row.id.clone()) {
            return Err("Paradex placement missing or repeated canonical ID".into());
        }
        let key = if expected == [None] {
            None
        } else {
            row.client_id
        };
        if !expected.contains(&key) || found.insert(key, row.id).is_some() {
            return Err("Paradex placement unknown or repeated client identity".into());
        }
    }
    let missing: Vec<_> = expected
        .iter()
        .filter(|id| !found.contains_key(*id))
        .collect();
    if !errors.is_empty() {
        if errors.len() != missing.len()
            || errors
                .iter()
                .any(|e| e.error.is_empty() && e.message.is_empty())
        {
            return Err("Paradex batch success/error counts are inconsistent".into());
        }
        return Err(format!(
            "Paradex batch rejected {} unidentified client IDs: {}",
            missing.len(),
            errors
                .iter()
                .map(|e| format!("{}: {}", e.error, e.message))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    if !missing.is_empty() {
        return Err("Paradex placement missing client results".into());
    }
    Ok(expected
        .iter()
        .map(|id| found.remove(id).unwrap())
        .collect())
}
fn decode_cancels(body: &str, ids: &[String]) -> Result<(), String> {
    #[derive(Deserialize)]
    struct Row {
        id: Option<String>,
        status: String,
    }
    #[derive(Deserialize)]
    struct Batch {
        results: Vec<Row>,
    }
    let batch: Batch =
        serde_json::from_str(body).map_err(|_| "invalid Paradex cancellation response")?;
    let mut seen = HashSet::new();
    for row in batch.results {
        let id = row.id.ok_or("uncorrelated Paradex cancellation result")?;
        if !ids.contains(&id)
            || !seen.insert(id)
            || !matches!(
                row.status.as_str(),
                "QUEUED_FOR_CANCELLATION" | "ALREADY_CLOSED"
            )
        {
            return Err("Paradex batch cancellation unknown, duplicate or rejected ID".into());
        }
    }
    if seen.len() != ids.len() {
        return Err("Paradex batch cancellation missing IDs".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batch_results_are_correlated_by_client_identity_not_position() {
        let expected = vec![Some("first".into()), Some("second".into())];
        assert_eq!(
            decode_placement(
                r#"{"orders":[{"id":"2","client_id":"second"},{"id":"1","client_id":"first"}]}"#,
                &expected,
                true
            )
            .unwrap(),
            ["1", "2"]
        );
        for body in [
            r#"{"orders":[{"id":"1","client_id":"first"}]}"#,
            r#"{"orders":[{"id":"1","client_id":"first"},{"id":"2","client_id":"first"}]}"#,
            r#"{"orders":[{"id":"1","client_id":"first"},{"id":"1","client_id":"second"}]}"#,
            r#"{"orders":[{"id":"1","client_id":"other"},{"id":"2","client_id":"second"}]}"#,
            r#"{"orders":[{"id":"1","client_id":"first"}],"errors":[{}]}"#,
            r#"{"orders":[],"errors":[{"error":"REJECTED"}]}"#,
        ] {
            assert!(decode_placement(body, &expected, true).is_err(), "{body}");
        }
    }
    #[test]
    fn cancel_results_require_every_exact_id_and_successful_status() {
        let ids = vec!["first".into(), "second".into()];
        assert!(decode_cancels(r#"{"results":[{"id":"second","status":"ALREADY_CLOSED"},{"id":"first","status":"QUEUED_FOR_CANCELLATION"}]}"#, &ids).is_ok());
        for body in [
            r#"{"results":[{"id":"first","status":"QUEUED_FOR_CANCELLATION"}]}"#,
            r#"{"results":[{"id":"first","status":"NOT_FOUND"},{"id":"second","status":"ALREADY_CLOSED"}]}"#,
            r#"{"results":[{"status":"NOT_FOUND"},{"id":"second","status":"ALREADY_CLOSED"}]}"#,
            r#"{"results":[{"id":"first","status":"ALREADY_CLOSED"},{"id":"first","status":"ALREADY_CLOSED"}]}"#,
        ] {
            assert!(decode_cancels(body, &ids).is_err(), "{body}");
        }
    }
    #[test]
    fn placement_never_accepts_wrong_or_empty_identity() {
        let expected = vec![Some("caller".into())];
        assert!(decode_placement(r#"{"id":"1","client_id":"caller"}"#, &expected, false).is_ok());
        for body in [
            r#"{"id":"1"}"#,
            r#"{"id":"1","client_id":"another"}"#,
            r#"{"id":" ","client_id":"caller"}"#,
        ] {
            assert!(decode_placement(body, &expected, false).is_err());
        }
    }
}
