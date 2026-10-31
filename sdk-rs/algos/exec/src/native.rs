//! Adapter to the one shared account runtime. This module owns no scheduler.
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use safe_cex_algos::{AccountPort, AlgoCx, AlgoFactory};
use safe_cex_engine::command::decode_command;
use safe_cex_engine::engine::{Command, CommandId};
use safe_cex_engine::error::VenueError;
use safe_cex_engine::model::OrderStatus;
use serde_json::{Value, json};

use crate::{AlgoReply, AlgoRequest, Exec, Host};
use safe_cex_algo_twap::{TwapOptions, factory::TwapFactory};
use safe_cex_chaser::{ChaserBook, ChaserOptions};
mod nuke;
mod tasks;

/// Register once in the server's account algo registry.
#[derive(Default)]
pub struct ExecFactory {
    parallel: Rc<Cell<usize>>,
    nuking: Rc<Cell<bool>>,
    account: RefCell<Option<Weak<dyn AccountPort>>>,
    account_name: Option<String>,
    chasers: Option<Rc<ChaserBook>>,
    twap: Option<TwapFactory>,
    cancel_all: Option<CancelAllScopes>,
}

/// How the venue's cancel-all reaches every order, decided by the host that
/// knows the venue's capabilities; read when a nuke's final sweep runs.
pub enum CancelAllScope {
    /// One venue-wide cancel-all.
    Native,
    /// Each symbol with an open order in the store, plus these (symbols a
    /// placement was attempted on, whose order the store may not show yet).
    Symbols(Vec<String>),
}
pub type CancelAllScopes = Rc<dyn Fn() -> CancelAllScope>;

impl ExecFactory {
    /// The cancel-all a nuke ends with. Without it the sweep cancels each
    /// symbol with an open order in the store.
    pub fn with_cancel_all(mut self, scopes: CancelAllScopes) -> Self {
        self.cancel_all = Some(scopes);
        self
    }
}

impl ExecFactory {
    /// Inject the same account instances used by direct chaser/TWAP ops.
    pub fn with_algos(account_name: String, chasers: Rc<ChaserBook>, twap: TwapFactory) -> Self {
        Self {
            account_name: Some(account_name),
            chasers: Some(chasers),
            twap: Some(twap),
            ..Default::default()
        }
    }
}

impl ExecFactory {
    /// The exec nuke for a hosted `nuke()` call: `mode` is `all`, `positions` or
    /// `orders`. Shares exec's one-nuke-at-a-time guard; awaits and reports every leg.
    pub fn nuke(
        &self,
        mode: &str,
        cx: &AlgoCx,
    ) -> Result<impl std::future::Future<Output = Result<Value, Value>> + 'static, Value> {
        if cx.scope.is_disposed() {
            return Err(json!({"message":"EXEC scope is disposed","outcome":"notSent"}));
        }
        if self.nuking.replace(true) {
            return Err(
                json!({"message":"Nuke already in progress on this account","outcome":"notSent"}),
            );
        }
        let guard = nuke::NukeGuard(self.nuking.clone());
        let adapter = Adapter {
            cx: cx.clone(),
            exec: Weak::new(),
            parallel: self.parallel.clone(),
            nuking: self.nuking.clone(),
            account_name: self.account_name.clone(),
            chasers: self.chasers.clone(),
            twap: self.twap.clone(),
            cancel_all: self.cancel_all.clone(),
        };
        let mode = mode.to_owned();
        Ok(async move {
            let _guard = guard;
            adapter.run_nuke(&mode).await
        })
    }
}

impl AlgoFactory for ExecFactory {
    fn ops(&self) -> &'static [&'static str] {
        &["exec"]
    }

    fn handle(&self, op: &str, data: &str, cx: &AlgoCx) -> Result<(), String> {
        if op != "exec" {
            return Err("Unsupported exec operation".into());
        }
        if cx.scope.is_disposed() {
            return Err("EXEC scope is disposed".into());
        }
        let mut account = self.account.borrow_mut();
        if let Some(previous) = account.as_ref().and_then(Weak::upgrade) {
            if !Rc::ptr_eq(&previous, &cx.port) {
                return Err("ExecFactory must be account-scoped".into());
            }
        } else {
            *account = Some(Rc::downgrade(&cx.port));
        }
        drop(account);
        let payload: Value = serde_json::from_str(data).map_err(|_| "Invalid command payload.")?;
        let exec = Rc::new(RefCell::new(Exec::default()));
        let mut host = Adapter {
            cx: cx.clone(),
            exec: Rc::downgrade(&exec),
            parallel: self.parallel.clone(),
            nuking: self.nuking.clone(),
            account_name: self.account_name.clone(),
            chasers: self.chasers.clone(),
            twap: self.twap.clone(),
            cancel_all: self.cancel_all.clone(),
        };
        exec.borrow_mut().handle(&payload, &mut host);
        Ok(())
    }
}

#[derive(Clone)]
struct Adapter {
    cx: AlgoCx,
    exec: Weak<RefCell<Exec>>,
    parallel: Rc<Cell<usize>>,
    nuking: Rc<Cell<bool>>,
    account_name: Option<String>,
    chasers: Option<Rc<ChaserBook>>,
    twap: Option<TwapFactory>,
    cancel_all: Option<CancelAllScopes>,
}
struct ParallelReservation(Rc<Cell<usize>>);
impl Drop for ParallelReservation {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}
impl Host for Adapter {
    fn algo(&mut self, request: AlgoRequest) -> Result<AlgoReply, Value> {
        if self.cx.scope.is_disposed() {
            return Err(json!({"message":"EXEC scope is disposed","outcome":"notSent"}));
        }
        self.run_algo(request)
            .map_err(|message| json!({"message":message,"outcome":"notSent"}))
    }

    fn random(&mut self) -> Result<f64, String> {
        let mut bytes = [0u8; 8];
        getrandom::getrandom(&mut bytes).map_err(|_| "Random distribution unavailable")?;
        Ok((u64::from_le_bytes(bytes) >> 11) as f64 / (1u64 << 53) as f64)
    }

    fn parallel_in_flight(&self) -> usize {
        self.parallel.get()
    }
    fn snapshot(&self) -> Value {
        let store = self.cx.port.store();
        let markets: Vec<Value> = store.markets.values().map(|market| {
            let mut value = serde_json::to_value(market).expect("finite market");
            if let Some(labels) = store.instrument_classifications.as_ref().and_then(|map|map.get(&market.symbol)) {
                value["instrumentClassification"] = json!({"symbolType":labels.symbol_type,"underlyingTicker":labels.underlying_ticker});
            }
            value
        }).collect();
        // A venue's terminal rows stay in the store. Cancelling one answers "absent",
        // which fences the symbol, so `be` lost its replacement stop to that fence.
        let live_orders: Vec<_> = store
            .orders
            .iter()
            .filter(|order| !matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled))
            .collect();
        json!({"venue":store.venue,"accountName":self.account_name,"markets":markets,"instrumentClassificationsReady":store.instrument_classifications.is_some(),
            "tickers":store.tickers.values().collect::<Vec<_>>(),"orders":live_orders,"positions":store.positions,"balance":store.balance,"coins":store.coins,"loaded":store.loaded,"socketHealth":{"privateInitialized":store.private_socket.initialized,"privateHealthy":store.private_socket.healthy}})
    }
    fn call(&mut self, token: u64, method: &str, args: Value) -> Result<(), Value> {
        self.call_scoped(token, method, args, false)
    }
    fn call_scoped(
        &mut self,
        token: u64,
        method: &str,
        args: Value,
        _parallel: bool,
    ) -> Result<(), Value> {
        if self.cx.scope.is_disposed() {
            return Err(json!({"message":"EXEC scope is disposed","outcome":"notSent"}));
        }
        let nuke_mode = if method == "nuke" {
            Some(args[0].as_str().unwrap_or("all").to_string())
        } else {
            None
        };
        let nuke_guard = if nuke_mode.is_some() {
            if self.nuking.replace(true) {
                return Err(
                    json!({"message":"Nuke already in progress on this account","outcome":"notSent"}),
                );
            }
            Some(nuke::NukeGuard(self.nuking.clone()))
        } else {
            None
        };
        let wait = if method == "execWait" {
            let symbol = args[0]
                .as_str()
                .ok_or_else(|| json!({"message":"Invalid wait","outcome":"notSent"}))?;
            let tokens: Vec<String> = serde_json::from_value(args[1].clone())
                .map_err(|_| json!({"message":"Invalid wait","outcome":"notSent"}))?;
            Some(
                crate::waits::Wait::parse(symbol, &tokens)
                    .map_err(|message| json!({"message":message,"outcome":"notSent"}))?,
            )
        } else {
            None
        };
        let command = if wait.is_none() && nuke_mode.is_none() {
            Some(
                translate(
                    method,
                    &args,
                    self.cx.port.next_command_id(),
                    &self.cx.port.capabilities(),
                )
                .map_err(|error| json!({"message":error,"outcome":"notSent"}))?,
            )
        } else {
            None
        };
        let Some(exec) = self.exec.upgrade() else {
            return Err(json!({"message":"EXEC scope is disposed","outcome":"notSent"}));
        };
        let chart_query = if method == "fetchOHLCV" {
            args.as_array().and_then(|a| a.first()).cloned()
        } else {
            None
        };
        let mut adapter = self.clone();
        let reservation = {
            if self.parallel.get() >= crate::MAX_PARALLEL {
                return Err(
                    json!({"message":"EXEC command exceeds native resource limits","outcome":"notSent"}),
                );
            }
            self.parallel.set(self.parallel.get() + 1);
            ParallelReservation(self.parallel.clone())
        };
        self.cx.spawn(async move {
            let reservation = reservation;
            let nuke_guard = nuke_guard;
            if adapter.cx.scope.is_disposed() {
                return;
            }
            let result = if let Some(mode) = nuke_mode {
                adapter.run_nuke(&mode).await
            } else if let Some(wait) = wait {
                crate::waits::run(wait, &adapter.cx, || adapter.snapshot()).await
            } else {
                adapter
                    .cx
                    .port
                    .command(command.expect("engine call"))
                    .await
                    .map(|s| serde_json::from_str(&s).unwrap_or(Value::String(s)))
                    .map_err(serialize_error)
                    .and_then(|value| {
                        if let Some(query) = chart_query {
                            crate::chart::render(
                                &value,
                                &query,
                                &adapter.snapshot(),
                                adapter.cx.now_ms(),
                            )
                            .map(|summary| json!({"summary":summary}))
                            .map_err(|message| json!({"message":message,"outcome":"notSent"}))
                        } else {
                            Ok(value)
                        }
                    })
            };
            drop(reservation);
            drop(nuke_guard);
            if !adapter.cx.scope.is_disposed() {
                exec.borrow_mut().complete(token, result, &mut adapter);
            }
        });
        Ok(())
    }
    fn emit(&mut self, op: &str, data: Value) {
        if !self.cx.scope.is_disposed() {
            self.cx.port.emit(op, data.to_string());
        }
    }
}

/// The initial callMethod subset; the engine decoder validates every command.
pub fn translate(
    method: &str,
    args: &Value,
    id: CommandId,
    capabilities: &safe_cex_engine::Capabilities,
) -> Result<Command, String> {
    let args = args.as_array().ok_or("Invalid method arguments")?;
    let symbol = || {
        args.first()
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or("Missing symbol")
    };
    let command = match method {
        "cancelSymbolOrders" => json!({"op":"cancelSymbolOrders","command":id,"symbol":symbol()?}),
        "cancelOrders" => {
            let orders = args
                .first()
                .and_then(Value::as_array)
                .ok_or("Invalid cancel orders")?;
            let orders: Result<Vec<Value>, String> = orders
                .iter()
                .map(|o| {
                    let id = o["id"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .ok_or("Missing cancel ID")?;
                    let symbol = o["symbol"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .ok_or("Missing cancel symbol")?;
                    Ok(json!({"id":id,"symbol":symbol}))
                })
                .collect();
            return Ok(safe_cex_algos::cancel_command(
                capabilities,
                id,
                serde_json::from_value(Value::Array(orders?)).map_err(|e| e.to_string())?,
            ));
        }
        "placeOrders" => {
            json!({"op":"placeOrders","command":id,"orders":args.first().filter(|v|v.is_array()).ok_or("Missing orders")?})
        }
        "placeOrder" => {
            json!({"op":"placeOrders","command":id,"orders":[args.first().ok_or("Missing order")?]})
        }
        "updateOrder" => {
            let input = args.first().ok_or("Missing order update")?;
            let order_id = input["order"]["id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("Missing order ID")?;
            let number = |field: &str| -> Result<Option<f64>, String> {
                match &input["update"][field] {
                    Value::Null => Ok(None),
                    value => value
                        .as_f64()
                        .filter(|n| n.is_finite())
                        .map(Some)
                        .ok_or_else(|| format!("Invalid update {field}")),
                }
            };
            let (price, amount) = (number("price")?, number("amount")?);
            if price.is_none() && amount.is_none() {
                return Err("Order amendment requires a price or amount".into());
            }
            let symbol = input["order"]["symbol"].as_str().unwrap_or_default();
            if capabilities.amend_with_symbol && symbol.is_empty() {
                return Err("Missing order symbol".into());
            }
            return Ok(safe_cex_algos::amend_command(
                capabilities,
                id,
                order_id.to_owned(),
                symbol.to_owned(),
                price,
                amount,
            ));
        }
        "fetchOHLCV" => {
            let query = args.first().ok_or("Missing candle query")?;
            json!({"op":"fetchCandles","command":id,"symbol":query["symbol"],"interval":query["interval"],"limit":query["limit"]})
        }
        "setLeverage" => {
            json!({"op":"setLeverage","command":id,"symbol":symbol()?,"leverage":args.get(1).ok_or("Missing leverage")?})
        }
        _ => return Err(format!("Unsupported exec method: {method}")),
    };
    decode_command(&command.to_string()).map_err(|e| e.to_string())
}

/// Match the browser host's structured command error, including known effects.
pub fn serialize_error(error: VenueError) -> Value {
    let mut data = serde_json::to_value(&error).expect("finite engine error fields");
    data["name"] = json!("Error");
    data.as_object_mut().unwrap().remove("partial");
    if let Some(partial) = error.partial {
        data["partialOrderIds"] = json!(partial.order_ids);
        if let Some(total) = partial.total {
            data["totalOrderCount"] = json!(total);
        }
        if let Some(index) = partial.failed_index {
            data["failedOrderIndex"] = json!(index);
        }
    }
    data
}

impl Adapter {
    fn run_algo(&self, request: AlgoRequest) -> Result<AlgoReply, String> {
        let mut reply = AlgoReply::default();
        match request {
            AlgoRequest::Tasks => return self.task_control(None),
            AlgoRequest::Kill(target) => return self.task_control(Some(&target)),
            AlgoRequest::ChaseStart { options, errors } => {
                reply.logs = errors;
                let book = self
                    .chasers
                    .as_ref()
                    .ok_or("Hosted chaser runtime unavailable")?;
                let mut ids = Vec::new();
                let mut symbols = Vec::new();
                for option in options {
                    let mut option: ChaserOptions =
                        serde_json::from_value(option).map_err(|_| "Invalid chase options")?;
                    option.account_name = self.account_name.clone();
                    option.client_id = None;
                    let symbol = option.symbol.clone();
                    match book.start(option) {
                        Ok(id) => {
                            ids.push(id);
                            symbols.push(symbol);
                        }
                        Err(error) => reply.logs.push(error),
                    }
                }
                if ids.is_empty() {
                    return Err(format!(
                        "No chasers started{}",
                        if reply.logs.is_empty() {
                            String::new()
                        } else {
                            format!(": {}", reply.logs.join("; "))
                        }
                    ));
                }
                let labels = ids
                    .iter()
                    .map(|id| &id[..id.len().min(8)])
                    .collect::<Vec<_>>()
                    .join(", ");
                reply.message = if ids.len() == 1 {
                    format!("Chaser started for {} [{labels}]", symbols.join(", "))
                } else {
                    format!(
                        "{} chasers started for {} [{labels}]",
                        ids.len(),
                        symbols.join(", ")
                    )
                };
                reply
                    .message
                    .push_str("\nUse \"watch chase\" to monitor progress, \"chasers\" to list all");
            }
            AlgoRequest::ChaseCancel { keep } => {
                let book = self
                    .chasers
                    .as_ref()
                    .ok_or("Hosted chaser runtime unavailable")?;
                if let Some(chase) = book
                    .active_chasers()
                    .into_iter()
                    .rev()
                    .find(|c| !c.is_stopped)
                {
                    if keep {
                        book.remove_keep_order(&chase.id);
                    } else {
                        book.remove(&chase.id);
                    }
                    reply.message = if keep {
                        format!(
                            "Stopped chaser [{}] — order left in book",
                            &chase.id[..chase.id.len().min(8)]
                        )
                    } else {
                        format!("Cancelled chaser [{}]", &chase.id[..chase.id.len().min(8)])
                    };
                } else {
                    reply.message = "No active chasers to cancel".into();
                }
            }
            AlgoRequest::ChasersList => {
                let book = self
                    .chasers
                    .as_ref()
                    .ok_or("Hosted chaser runtime unavailable")?;
                let all = book.active_chasers();
                reply.message = if all.is_empty() {
                    "No active or recent chasers".into()
                } else {
                    all.iter()
                        .map(|c| {
                            format!(
                                "[{}] {} {:?} {} attempts: {} {}",
                                c.id,
                                c.symbol,
                                c.side,
                                c.size,
                                c.attempts,
                                if c.is_stopped {
                                    &c.stop_reason
                                } else {
                                    "ACTIVE"
                                }
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
            }
            AlgoRequest::ChasersCancel(target) => {
                let book = self
                    .chasers
                    .as_ref()
                    .ok_or("Hosted chaser runtime unavailable")?;
                if target.is_empty() {
                    reply.message = "Usage: chasers cancel <id> or chasers cancel all".into();
                } else if target == "all" {
                    let active = book
                        .active_chasers()
                        .into_iter()
                        .filter(|c| !c.is_stopped)
                        .collect::<Vec<_>>();
                    for chase in &active {
                        book.remove(&chase.id);
                    }
                    reply.message = format!("Cancelled {} chaser(s)", active.len());
                } else if let Some(chase) = book
                    .active_chasers()
                    .into_iter()
                    .find(|c| c.id.starts_with(&target))
                {
                    if chase.is_stopped {
                        reply.message = format!("Chaser [{target}] is already stopped");
                    } else {
                        book.remove(&chase.id);
                        reply.message = format!(
                            "Cancelled chaser [{}] for {}",
                            &chase.id[..chase.id.len().min(8)],
                            chase.symbol
                        );
                    }
                } else {
                    reply.message = format!("Chaser not found: {target}");
                }
            }
            AlgoRequest::TwapStart(mut value) => {
                let factory = self
                    .twap
                    .as_ref()
                    .ok_or("Hosted TWAP runtime unavailable")?;
                value["accountName"] = json!(
                    self.account_name
                        .as_ref()
                        .ok_or("TWAP account unavailable")?
                );
                let options: TwapOptions =
                    serde_json::from_value(value).map_err(|_| "Invalid TWAP options")?;
                let symbol = options.symbol().to_string();
                let id = factory.start(options)?;
                reply.message = format!("Starting TWAP for {symbol} [{id}]");
            }
            AlgoRequest::TwapList => {
                let factory = self
                    .twap
                    .as_ref()
                    .ok_or("Hosted TWAP runtime unavailable")?;
                let rows = factory.active_twaps(
                    self.account_name
                        .as_deref()
                        .ok_or("TWAP account unavailable")?,
                );
                reply.message = if rows.is_empty() {
                    "No TWAP orders running".into()
                } else {
                    format!(
                        "Active TWAP Orders\n{}",
                        rows.iter()
                            .map(Value::to_string)
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                };
            }
            AlgoRequest::TwapStop(target) => {
                let factory = self
                    .twap
                    .as_ref()
                    .ok_or("Hosted TWAP runtime unavailable")?;
                let rows = factory.active_twaps(
                    self.account_name
                        .as_deref()
                        .ok_or("TWAP account unavailable")?,
                );
                let rows = rows
                    .into_iter()
                    .filter(|row| row["status"] != "completed")
                    .collect::<Vec<_>>();
                if target == "all" {
                    let mut count = 0;
                    for row in rows {
                        if let Some(id) = row["id"].as_str() {
                            count += usize::from(factory.stop(id));
                        }
                    }
                    reply.message = format!("Stopping {count} TWAP order(s)");
                } else {
                    let id = rows
                        .iter()
                        .filter_map(|row| row["id"].as_str())
                        .find(|id| id.starts_with(&target))
                        .ok_or_else(|| {
                            format!("No TWAP found with ID starting with \"{target}\"")
                        })?;
                    if !factory.stop(id) {
                        return Err(format!("TWAP [{id}] is already completed"));
                    }
                    reply.message = format!("Stopping TWAP [{id}]");
                }
            }
        }
        Ok(reply)
    }
}
