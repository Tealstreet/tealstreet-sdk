//! Hosted CLI routing. The account runtime owns scheduling, I/O and disposal.
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};

mod algo_commands;
mod capabilities;
mod chart;
mod configuration;
mod display;
pub use algo_commands::{AlgoReply, AlgoRequest};
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
mod order_tools;
mod placement;
mod protection;
mod swarm;
mod waits;

pub const MAX_COMMAND_BYTES: usize = 16 * 1024;
pub const MAX_STEPS: usize = 128;
pub const MAX_PARALLEL: usize = 32;

/// The runtime projects its engine store into the existing TS store shape.
/// `call` routes through the server's engine-backed callMethod translator.
pub trait Host {
    fn snapshot(&self) -> Value;
    fn call(&mut self, token: u64, method: &str, args: Value) -> Result<(), Value>;
    fn emit(&mut self, op: &str, data: Value);
    fn algo(&mut self, _request: AlgoRequest) -> Result<AlgoReply, Value> {
        Err(json!({"message":"Hosted algo runtime unavailable","outcome":"notSent"}))
    }
    fn random(&mut self) -> Result<f64, String> {
        Err("Random distribution unavailable".into())
    }
    fn parallel_in_flight(&self) -> usize {
        0
    }
    fn call_scoped(
        &mut self,
        token: u64,
        method: &str,
        args: Value,
        _parallel: bool,
    ) -> Result<(), Value> {
        self.call(token, method, args)
    }
}

#[derive(Default)]
pub struct Exec {
    next: u64,
    chains: HashMap<u64, Chain>,
    pending: HashMap<u64, Pending>,
    disposed: bool,
    swarms: HashMap<u64, Swarm>,
}
struct Chain {
    wait_barrier: bool,
    symbol: String,
    req_id: Option<Value>,
    steps: VecDeque<String>,
    last: Option<Value>,
}
struct Pending {
    chain: Option<u64>,
    req_id: Option<Value>,
    success: String,
    actions: VecDeque<Action>,
    attempt_all: bool,
    order_ids: Option<Vec<String>>,
    parallel: bool,
    succeeded: usize,
    failed: usize,
    failure_summary: Option<String>,
    group: Option<(u64, usize)>,
    current_place: bool,
    native_summary: bool,
    failure_context: Option<String>,
}
enum Evaluation {
    Done(Value),
    Text {
        message: String,
        result: Value,
    },
    Place(VecDeque<placement::Placement>),
    Swarm {
        orders: Vec<Value>,
        symbol: String,
        side: String,
    },
    Batch {
        orders: Vec<Value>,
        success: String,
    },
    Sequence {
        actions: VecDeque<Action>,
        success: String,
        attempt_all: bool,
        failure_summary: Option<String>,
    },
    Call {
        method: &'static str,
        args: Value,
        success: String,
    },
}

enum Action {
    Buy(placement::Placement),
    Order(Value),
    Replacement(Value),
    Amend {
        order: Value,
        price: f64,
    },
    Close {
        position: Value,
        size: Option<String>,
    },
    Call(&'static str, Value),
}
impl Action {
    fn failure_context(&self) -> Option<String> {
        if let Self::Replacement(order) = self {
            Some(format!(
                "{}: stop-loss removed, replacement not confirmed — position unprotected; no stop is in place",
                order["symbol"].as_str().unwrap_or("unknown symbol")
            ))
        } else {
            None
        }
    }
    fn prepare(self, store: &Value) -> Result<(&'static str, Value), String> {
        match self {
            Self::Buy(spec) => Ok(("placeOrder", json!([spec.prepare(store)?]))),
            Self::Order(order) | Self::Replacement(order) => Ok((
                "placeOrder",
                json!([placement::prepare_order(order, store)?]),
            )),
            Self::Call(method, args) => Ok((method, args)),
            Self::Amend { order, price } => Ok((
                "updateOrder",
                json!([{"update":{"price":protection::amend_price(&order,price,store)?},"order":order}]),
            )),
            Self::Close { position, size } => Ok((
                "placeOrder",
                json!([protection::prepare_close(
                    &position,
                    size.as_deref(),
                    store
                )?]),
            )),
        }
    }
}

struct Swarm {
    chain: Option<u64>,
    req_id: Option<Value>,
    symbol: String,
    side: String,
    results: Vec<Option<Result<Value, Value>>>,
}

impl Exec {
    /// Start one existing `exec` payload. No callback is replayed after dispose.
    pub fn handle(&mut self, payload: &Value, host: &mut impl Host) {
        if self.disposed {
            return;
        }
        let req_id = payload.get("reqId").cloned();
        let Some(object) = payload.as_object() else {
            log(host, &req_id, "Invalid command payload.");
            reply(
                host,
                &req_id,
                json!({"handled":false,"success":false,"error":"Invalid command payload."}),
            );
            return;
        };
        let Some(command) = object.get("command").and_then(Value::as_str) else {
            let message = "Command failed: Invalid command payload.";
            log(host, &req_id, message);
            reply(
                host,
                &req_id,
                json!({"handled":true,"success":false,"error":message}),
            );
            return;
        };
        if command.len() > MAX_COMMAND_BYTES {
            resource_refusal(host, &req_id);
            return;
        }
        let steps: VecDeque<String> = command
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        let parallel = steps.iter().filter(|s| s.starts_with('@')).count();
        let in_flight = self
            .pending
            .values()
            .filter(|p| p.chain.is_none())
            .count()
            .max(host.parallel_in_flight());
        if steps.len() > MAX_STEPS || parallel + in_flight > MAX_PARALLEL {
            resource_refusal(host, &req_id);
            return;
        }
        let id = self.id();
        self.chains.insert(
            id,
            Chain {
                wait_barrier: false,
                symbol: object
                    .get("symbol")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .into(),
                req_id,
                steps,
                last: None,
            },
        );
        self.drive(id, host);
    }

    /// Complete exactly one dispatched command; duplicate/late replies are ignored.
    pub fn complete(&mut self, token: u64, result: Result<Value, Value>, host: &mut impl Host) {
        if self.disposed {
            return;
        }
        let Some(pending) = self.pending.remove(&token) else {
            return;
        };
        let mut pending = pending;
        let mut result = result;
        if pending.native_summary
            && let Ok(value) = &result
        {
            if let Some(summary) = value["summary"].as_str() {
                pending.success = summary.into();
            } else {
                result =
                    Err(json!({"message":"Invalid native command result","outcome":"unknown"}));
            }
        }
        if let Ok(value) = &result
            && pending.current_place
        {
            if let Some(returned) = value.as_array().filter(|a| a.iter().all(Value::is_string)) {
                if let Some(ids) = pending.order_ids.as_mut() {
                    ids.extend(returned.iter().map(|v| v.as_str().unwrap().to_string()));
                }
            } else {
                result = Err(json!({"message":"Invalid placement result","outcome":"unknown"}));
            }
        }
        if let Some((group, index)) = pending.group {
            self.complete_swarm(group, index, result, host);
            return;
        }
        if result.is_ok() {
            pending.succeeded += 1;
        } else {
            pending.failed += 1;
        }
        if result.is_ok() || pending.attempt_all {
            if let Err(error) = &result {
                log(
                    host,
                    &pending.req_id,
                    &format!(
                        "Error: Command failed: {}",
                        error["message"].as_str().unwrap_or("Command failed")
                    ),
                );
            }
            while let Some(action) = pending.actions.pop_front() {
                pending.failure_context = action.failure_context();
                match action.prepare(&host.snapshot()) {
                    Ok((method, args)) => {
                        self.issue(pending, method, args, host);
                        return;
                    }
                    Err(message) if pending.attempt_all => {
                        pending.failed += 1;
                        log(host, &pending.req_id, &format!("Error: {message}"))
                    }
                    Err(message) => {
                        result = Err(json!({"message":message,"outcome":"notSent"}));
                        break;
                    }
                }
            }
            if pending.attempt_all {
                result = Ok(Value::Null);
            }
        }
        let result = match result {
            Ok(_) => {
                let summary = if pending.failed > 0 {
                    pending.failure_summary.as_ref().unwrap_or(&pending.success)
                } else {
                    &pending.success
                };
                let message = summary
                    .replace(
                        "{ids}",
                        &pending
                            .order_ids
                            .as_ref()
                            .map(|ids| ids.join(","))
                            .unwrap_or_default(),
                    )
                    .replace("{ok}", &pending.succeeded.to_string())
                    .replace("{failed}", &pending.failed.to_string());
                if pending.native_summary {
                    log(host, &pending.req_id, &message);
                } else {
                    for line in message.split('\n') {
                        log(host, &pending.req_id, line);
                    }
                }
                let mut result = json!({"handled":true,"success":true});
                if let Some(ids) = pending.order_ids {
                    result["data"] = json!({"orderIds":ids});
                }
                result
            }
            Err(error) => {
                let original = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Command failed");
                let message = pending
                    .failure_context
                    .as_ref()
                    .map(|context| format!("{context}: {original}"))
                    .unwrap_or_else(|| original.into());
                log(
                    host,
                    &pending.req_id,
                    &format!("Error: Command failed: {message}"),
                );
                if pending.failure_context.is_some() {
                    host.emit("error", json!(message));
                }
                json!({"handled":true,"success":false,"error":message,"errorReported":true,"errorDetails":error})
            }
        };
        if let Some(id) = pending.chain {
            if let Some(chain) = self.chains.get_mut(&id) {
                let mut result = result;
                if chain.wait_barrier && result["success"] == false {
                    stop_wait_chain(chain, &mut result, host);
                }
                chain.last = Some(result);
            }
            self.drive(id, host);
        }
    }

    fn issue(&mut self, mut pending: Pending, method: &str, args: Value, host: &mut impl Host) {
        let token = self.id();
        let parallel = pending.parallel;
        pending.current_place = matches!(method, "placeOrder" | "placeOrders");
        self.pending.insert(token, pending);
        if let Err(error) = host.call_scoped(token, method, args, parallel) {
            self.complete(token, Err(error), host);
        }
    }

    fn complete_swarm(
        &mut self,
        id: u64,
        index: usize,
        result: Result<Value, Value>,
        host: &mut impl Host,
    ) {
        let Some(group) = self.swarms.get_mut(&id) else {
            return;
        };
        if group.results[index].is_some() {
            return;
        }
        group.results[index] = Some(result);
        if group.results.iter().any(Option::is_none) {
            return;
        }
        let group = self.swarms.remove(&id).unwrap();
        let mut ids = Vec::new();
        let mut successes = 0;
        let mut failures = 0;
        let mut first_error = None;
        for result in group.results.into_iter().flatten() {
            match result {
                Ok(value) => {
                    successes += 1;
                    ids.extend(value.as_array().unwrap().iter().cloned());
                }
                Err(error) => {
                    failures += 1;
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        let result = if successes > 0 {
            log(
                host,
                &group.req_id,
                &if failures == 0 {
                    format!(
                        "Swarm placed: {successes} {} {} order(s)",
                        group.symbol, group.side
                    )
                } else {
                    format!("Swarm partial: {successes} placed, {failures} failed")
                },
            );
            json!({"handled":true,"success":true,"data":{"orderIds":ids}})
        } else {
            log(host, &group.req_id, "Swarm: all orders failed to place");
            json!({"handled":true,"success":false,"error":"Swarm: all orders failed to place","errorDetails":first_error})
        };
        if let Some(chain) = group.chain {
            if let Some(chain) = self.chains.get_mut(&chain) {
                chain.last = Some(result);
            }
            self.drive(chain, host);
        }
    }

    /// The owning socket's close disposes its scope; issued writes are not resent.
    pub fn dispose(&mut self) {
        self.disposed = true;
        self.pending.clear();
        self.chains.clear();
        self.swarms.clear();
    }

    fn id(&mut self) -> u64 {
        self.next = self
            .next
            .checked_add(1)
            .expect("exec token space exhausted");
        self.next
    }

    fn drive(&mut self, id: u64, host: &mut impl Host) {
        while let Some(mut chain) = self.chains.remove(&id) {
            let Some(text) = chain.steps.pop_front() else {
                reply(
                    host,
                    &chain.req_id,
                    chain.last.unwrap_or_else(|| json!({"handled":false})),
                );
                return;
            };
            let (parallel, text) = match text.strip_prefix('@') {
                Some(s) => (true, s),
                None => (false, text.as_str()),
            };
            if !parallel {
                chain.wait_barrier = matches!(
                    positional(text)
                        .first()
                        .and_then(|head| display::canonical(head)),
                    Some("wait" | "sleep")
                );
            }
            let action = evaluate(&chain.symbol, text, &host.snapshot(), &chain.req_id, host);
            match action {
                Evaluation::Text { message, result } => {
                    log(host, &chain.req_id, &message);
                    if !parallel {
                        chain.last = Some(result);
                    }
                    self.chains.insert(id, chain);
                }
                Evaluation::Done(mut result) => {
                    if !parallel {
                        if chain.wait_barrier && result["success"] == false {
                            stop_wait_chain(&mut chain, &mut result, host);
                        }
                        if chain.req_id.is_none()
                            && result["handled"] == true
                            && result["success"] == false
                            && result["errorReported"] != true
                        {
                            log(
                                host,
                                &chain.req_id,
                                result["error"].as_str().unwrap_or("Command failed"),
                            );
                        }
                        chain.last = Some(result);
                    }
                    self.chains.insert(id, chain);
                }
                Evaluation::Place(mut placements) => {
                    let first = placements
                        .pop_front()
                        .expect("placement parser returns at least one");
                    let order = first.prepare(&host.snapshot());
                    let pending = Pending {
                        chain: (!parallel).then_some(id),
                        req_id: chain.req_id.clone(),
                        success: if placements.is_empty() {
                            "Order placed".into()
                        } else {
                            "Orders placed".into()
                        },
                        actions: placements.into_iter().map(Action::Buy).collect(),
                        attempt_all: false,
                        order_ids: Some(Vec::new()),
                        parallel,
                        succeeded: 0,
                        failed: 0,
                        failure_summary: None,
                        group: None,
                        current_place: false,
                        native_summary: false,
                        failure_context: None,
                    };
                    self.chains.insert(id, chain);
                    match order {
                        Ok(order) => self.issue(pending, "placeOrder", json!([order]), host),
                        Err(message) => {
                            let token = self.id();
                            self.pending.insert(token, pending);
                            self.complete(
                                token,
                                Err(json!({"message":message,"outcome":"notSent"})),
                                host,
                            );
                        }
                    }
                    if !parallel {
                        return;
                    }
                }
                Evaluation::Swarm {
                    orders,
                    symbol,
                    side,
                } => {
                    let count = orders.len();
                    if count + self.pending.len().max(host.parallel_in_flight()) > MAX_PARALLEL {
                        if !parallel {
                            chain.last = Some(
                                json!({"handled":true,"success":false,"error":"EXEC command exceeds native resource limits"}),
                            );
                        }
                        log(
                            host,
                            &chain.req_id,
                            "EXEC command exceeds native resource limits",
                        );
                        self.chains.insert(id, chain);
                        continue;
                    }
                    let group = self.id();
                    let req_id = chain.req_id.clone();
                    self.swarms.insert(
                        group,
                        Swarm {
                            chain: (!parallel).then_some(id),
                            req_id: req_id.clone(),
                            symbol,
                            side,
                            results: vec![None; count],
                        },
                    );
                    self.chains.insert(id, chain);
                    for (index, order) in orders.into_iter().enumerate() {
                        let pending = Pending {
                            chain: None,
                            req_id: req_id.clone(),
                            success: String::new(),
                            actions: VecDeque::new(),
                            attempt_all: false,
                            order_ids: None,
                            parallel: true,
                            succeeded: 0,
                            failed: 0,
                            failure_summary: None,
                            group: Some((group, index)),
                            current_place: false,
                            native_summary: false,
                            failure_context: None,
                        };
                        self.issue(pending, "placeOrder", json!([order]), host);
                    }
                    if !parallel {
                        return;
                    }
                }
                Evaluation::Batch { orders, success } => {
                    let pending = Pending {
                        chain: (!parallel).then_some(id),
                        req_id: chain.req_id.clone(),
                        success,
                        actions: VecDeque::new(),
                        attempt_all: false,
                        order_ids: Some(Vec::new()),
                        parallel,
                        succeeded: 0,
                        failed: 0,
                        failure_summary: None,
                        group: None,
                        current_place: false,
                        native_summary: false,
                        failure_context: None,
                    };
                    self.chains.insert(id, chain);
                    self.issue(pending, "placeOrders", json!([orders]), host);
                    if !parallel {
                        return;
                    }
                }
                Evaluation::Sequence {
                    mut actions,
                    success,
                    attempt_all,
                    failure_summary,
                } => {
                    let Some(first) = actions.pop_front() else {
                        log(host, &chain.req_id, &success.replace("{ok}", "0"));
                        if !parallel {
                            chain.last = Some(json!({"handled":true,"success":true}));
                        }
                        self.chains.insert(id, chain);
                        continue;
                    };
                    let pending = Pending {
                        chain: (!parallel).then_some(id),
                        req_id: chain.req_id.clone(),
                        success,
                        actions,
                        attempt_all,
                        order_ids: None,
                        parallel,
                        succeeded: 0,
                        failed: 0,
                        failure_summary,
                        group: None,
                        current_place: false,
                        native_summary: false,
                        failure_context: None,
                    };
                    self.chains.insert(id, chain);
                    let mut pending = pending;
                    pending.failure_context = first.failure_context();
                    match first.prepare(&host.snapshot()) {
                        Ok((method, args)) => self.issue(pending, method, args, host),
                        Err(message) => {
                            let token = self.id();
                            self.pending.insert(token, pending);
                            self.complete(
                                token,
                                Err(json!({"message":message,"outcome":"notSent"})),
                                host,
                            );
                        }
                    }
                    if !parallel {
                        return;
                    }
                }
                Evaluation::Call {
                    method,
                    args,
                    success,
                } => {
                    let token = self.id();
                    self.pending.insert(
                        token,
                        Pending {
                            chain: (!parallel).then_some(id),
                            req_id: chain.req_id.clone(),
                            success,
                            actions: VecDeque::new(),
                            attempt_all: false,
                            order_ids: None,
                            parallel,
                            succeeded: 0,
                            failed: 0,
                            failure_summary: None,
                            group: None,
                            current_place: false,
                            native_summary: matches!(method, "nuke" | "fetchOHLCV"),
                            failure_context: None,
                        },
                    );
                    self.chains.insert(id, chain);
                    match host.call_scoped(token, method, args, parallel) {
                        Ok(()) if !parallel => return,
                        Ok(()) => {}
                        Err(error) => {
                            self.complete(token, Err(error), host);
                            if !parallel {
                                return;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn stop_wait_chain(chain: &mut Chain, result: &mut Value, host: &mut impl Host) {
    chain.steps.clear();
    let error = format!(
        "chain stopped: wait did not complete: {}",
        result["error"].as_str().unwrap_or("wait failed")
    );
    log(host, &chain.req_id, &error);
    result["error"] = json!(error);
    result["errorReported"] = json!(true);
}

fn reply(host: &mut impl Host, req_id: &Option<Value>, mut result: Value) {
    if let Some(id) = req_id {
        result["reqId"] = id.clone();
        host.emit("exec-result", result);
    }
}
fn log(host: &mut impl Host, req_id: &Option<Value>, message: &str) {
    let mut data = json!({"message":message});
    if let Some(id) = req_id {
        data["reqId"] = id.clone();
    }
    host.emit("exec-log", data);
}
fn done() -> Evaluation {
    Evaluation::Done(json!({"handled":true,"success":true}))
}
fn resource_refusal(host: &mut impl Host, req_id: &Option<Value>) {
    let message = "EXEC command exceeds native resource limits";
    log(host, req_id, message);
    reply(
        host,
        req_id,
        json!({"handled":true,"success":false,"error":message}),
    );
}
fn failed(message: String) -> Evaluation {
    Evaluation::Done(json!({"handled":true,"success":false,"error":message}))
}
fn rows<'a>(store: &'a Value, key: &str) -> &'a [Value] {
    store
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}
fn resolve(store: &Value, symbol: &str) -> Option<String> {
    let upper = symbol.trim().to_uppercase();
    let markets = rows(store, "markets");
    if let Some(m) = markets.iter().find(|m| {
        m["symbol"]
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case(&upper))
    }) {
        return m["symbol"].as_str().map(String::from);
    }
    let quote = if upper.ends_with("USDC") {
        "USDC"
    } else if upper.ends_with("USD") {
        "USD"
    } else {
        "USDT"
    };
    let base = markets
        .iter()
        .filter_map(|m| m["base"].as_str())
        .filter(|base| {
            !base.is_empty()
                && upper.strip_prefix(base).is_some_and(|rest| {
                    matches!(
                        rest,
                        "" | "USDT"
                            | "USDC"
                            | "USD"
                            | "PERP"
                            | "-PERP"
                            | "/USDT"
                            | "/USDC"
                            | "/USD"
                    )
                })
        })
        .max_by_key(|base| base.len())?;
    markets
        .iter()
        .find(|m| m["base"] == base && m["quote"] == quote)?
        .get("symbol")?
        .as_str()
        .map(String::from)
}

fn evaluate(
    symbol: &str,
    text: &str,
    store: &Value,
    req_id: &Option<Value>,
    host: &mut impl Host,
) -> Evaluation {
    let mut args = positional(text);
    if let Some(head) = args.first_mut() {
        *head = display::canonical(head)
            .map(str::to_string)
            .unwrap_or_else(|| head.to_ascii_lowercase());
    }
    if !args.is_empty()
        && let Some(result) = configuration::evaluate(symbol, &args, store)
    {
        return result;
    }
    if !args.is_empty()
        && let Some(result) = display::evaluate(symbol, &args, store, text)
    {
        return result;
    }
    if args.len() == 1
        && rows(store, "markets").iter().any(|m| {
            m["symbol"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(&args[0]))
        })
    {
        return display::evaluate(symbol, &["symbol".into(), args[0].clone()], store, text)
            .expect("symbol command");
    }
    let Some(head) = args.first().map(String::as_str) else {
        return Evaluation::Done(json!({"handled":true}));
    };
    match head {
        "echo" | "say" => {
            log(host, req_id, &args[1..].join(" "));
            done()
        }
        "last-price" | "lp" | "l" | "mark-price" | "mp" | "m" | "index-price" | "ip" | "i" => {
            let key = match head {
                "last-price" | "lp" | "l" => "last",
                "mark-price" | "mp" | "m" => "mark",
                _ => "index",
            };
            let symbols = args.get(1).map_or_else(
                || vec![symbol.to_string()],
                |list| list.split(',').filter_map(|s| resolve(store, s)).collect(),
            );
            let mut lines = Vec::new();
            for symbol in symbols {
                let price = rows(store, "tickers")
                    .iter()
                    .find(|t| t["symbol"] == symbol)
                    .and_then(|t| t[key].as_f64())
                    .filter(|p| *p != 0.);
                lines.push(match price {
                    Some(price) => format!(
                        "{}{} Price: {symbol} @ {price}",
                        key[..1].to_uppercase(),
                        &key[1..]
                    ),
                    None => format!("Ticker {symbol} was not found or is not loaded yet"),
                });
            }
            log(host, req_id, &lines.join("\n"));
            done()
        }
        "buy" | "b" | "sell" | "s" => match placement::parse(symbol, &args, store) {
            Ok(orders) => Evaluation::Place(orders),
            Err(error) => failed(error),
        },
        "sleep" | "wait" => waits::evaluate(symbol, &args),
        "chase" | "chasers" | "twap" | "tasks" | "kill" => {
            algo_commands::evaluate(symbol, &args, store, req_id, host)
        }
        "swarm" => match swarm::prepare(symbol, &args, store, host) {
            Ok((orders, symbol, side)) => Evaluation::Swarm {
                orders,
                symbol,
                side,
            },
            Err(error) => failed(error),
        },
        "scale" | "bump" => order_tools::evaluate(symbol, &args, store, text),
        "close" | "c" | "stop" | "be" | "breakeven" | "trail" | "move" | "mv" => {
            protection::evaluate(symbol, &args, store, text)
        }
        "chart" => chart::evaluate(symbol, &args, store),
        "nuke" => {
            let mode = args.get(1).map(String::as_str).unwrap_or("all");
            if !matches!(mode, "all" | "positions" | "pos" | "orders" | "ord") {
                return failed("Invalid syntax: nuke <positions|pos|orders|ord|all>".into());
            }
            Evaluation::Call {
                method: "nuke",
                args: json!([mode]),
                success: "Nuke completed".into(),
            }
        }
        "cancel" | "x" => cancel(symbol, &args, store),
        "leverage" | "lev" => {
            if let Some(tail) = args.get(2)
                && resolve(store, tail).is_none()
            {
                return failed(format!("Unknown symbol \"{tail}\""));
            }
            let target = args
                .last()
                .and_then(|s| resolve(store, s))
                .unwrap_or_else(|| symbol.into());
            let arg = args.get(1);
            if arg.is_none_or(|a| !a.chars().any(|c| c.is_ascii_digit())) {
                let position = rows(store, "positions").iter().find(|p| {
                    p["symbol"] == target && p["contracts"].as_f64().is_some_and(|n| n != 0.)
                });
                let message = position.and_then(|p|p["leverage"].as_f64()).map_or_else(|| format!("No open position on {target} — current per-symbol leverage isn't tracked in the store. Use 'lev <n>' to set."), |n|format!("{target}: {n}x"));
                log(host, req_id, &message);
                return done();
            }
            let arg = arg.unwrap();
            let leverage = arg.parse::<f64>().ok().filter(|n| n.is_finite() && *n > 0.);
            if !arg.chars().all(|c| c.is_ascii_digit() || c == '.') || leverage.is_none() {
                return failed(format!(
                    "Invalid leverage: '{arg}' — must be a positive number"
                ));
            }
            Evaluation::Call {
                method: "setLeverage",
                args: json!([target, leverage]),
                success: format!("{target} leverage set to {}x", leverage.unwrap()),
            }
        }
        _ => {
            log(
                host,
                req_id,
                &format!("Unknown command: {}", text.split(' ').next().unwrap_or("")),
            );
            Evaluation::Done(json!({"handled":false}))
        }
    }
}

fn cancel(symbol: &str, args: &[String], store: &Value) -> Evaluation {
    if let Some(result) = order_tools::advanced_cancel(symbol, args, store) {
        return result;
    }
    let target = args
        .iter()
        .skip(1)
        .rev()
        .filter(|s| {
            ![
                "buys", "sells", "buy", "sell", "limits", "stops", "ro", "xro",
            ]
            .contains(&s.as_str())
        })
        .find_map(|s| resolve(store, s))
        .unwrap_or_else(|| symbol.into());
    let head = args.get(1).map(String::as_str).unwrap_or("");
    let sub = args.get(2).map(String::as_str).unwrap_or("");
    if let Some(tail) = args.last().filter(|_| args.len() > 2)
        && !["buys", "sells", "limits", "stops", "ro", "xro"].contains(&tail.as_str())
        && resolve(store, tail).is_none()
    {
        return failed(format!("Unknown symbol \"{tail}\""));
    }
    if head.is_empty() || resolve(store, head).is_some() {
        return Evaluation::Call {
            method: "cancelSymbolOrders",
            args: json!([target]),
            success: format!("Cancelling all {target} orders"),
        };
    }
    let filters = ["buys", "sells", "limits", "stops", "ro", "xro"];
    let selected: Vec<Value> = if filters.contains(&head) {
        rows(store, "orders")
            .iter()
            .filter(|o| o["symbol"] == target)
            .filter(|o| match head {
                "buys" | "sells" => {
                    o["type"] == "limit"
                        && o["side"] == if head == "buys" { "buy" } else { "sell" }
                        && match sub {
                            "ro" => o["reduceOnly"] == true,
                            "xro" => o["reduceOnly"] != true,
                            _ => true,
                        }
                }
                "limits" => o["type"] == "limit",
                "stops" => o["type"] != "limit" && o["type"] != "market",
                "ro" | "xro" => {
                    o["type"] == "limit"
                        && o["reduceOnly"].as_bool().unwrap_or(false) == (head == "ro")
                }
                _ => false,
            })
            .cloned()
            .collect()
    } else {
        let ids: Vec<_> = head.split(',').filter(|s| !s.is_empty()).collect();
        if ids.is_empty()
            || ids.iter().any(|s| {
                s.len() < 3
                    || !s
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            })
        {
            return failed("Not implemented".into());
        }
        rows(store, "orders")
            .iter()
            .filter(|o| {
                o["status"] == "open"
                    && o["id"]
                        .as_str()
                        .is_some_and(|id| ids.iter().any(|suffix| id.ends_with(suffix)))
            })
            .cloned()
            .collect()
    };
    if selected.is_empty() && !filters.contains(&head) {
        return failed("Order not found".into());
    }
    let success = match head {
        "stops" => format!("Cancelling all {target} stops"),
        "limits" => format!("Cancelling {} {target} limit order(s)", selected.len()),
        "ro" | "xro" => format!(
            "Cancelling {} {target} {} order(s)",
            selected.len(),
            if head == "ro" {
                "reduce-only"
            } else {
                "non-reduce-only"
            }
        ),
        "buys" | "sells" => {
            let label = match sub {
                "ro" => format!("{head} reduce-only"),
                "xro" => format!("{head} non-reduce-only"),
                _ => head.into(),
            };
            format!("Cancelling {} {target} {label} order(s)", selected.len())
        }
        _ if selected.len() == 1 => format!(
            "Cancelling order {}",
            selected[0]["id"].as_str().unwrap_or("")
        ),
        _ => format!("Cancelling {} orders matching \"{head}\"", selected.len()),
    };
    Evaluation::Call {
        method: "cancelOrders",
        args: json!([selected]),
        success,
    }
}

/// Mirrors CLI parseArgs: quotes group words; flags/options are not positional.
pub fn positional(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    for c in input.chars() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            } else {
                word.push(c);
            }
        } else if c == '\'' || c == '"' {
            quote = Some(c);
        } else if c == ' ' || c == '\t' {
            if !word.is_empty() {
                tokens.push(std::mem::take(&mut word));
            }
        } else {
            word.push(c);
        }
    }
    if !word.is_empty() {
        tokens.push(word);
    }
    let mut result = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];
        if t.starts_with('-') && t.len() > 1 {
            if !t.contains('=') && tokens.get(i + 1).is_some_and(|next| !next.starts_with('-')) {
                i += 1;
            }
        } else {
            result.push(t.clone());
        }
        i += 1;
    }
    result
}
