//! Hosted CLI routing. The account runtime owns scheduling, I/O and disposal.
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};

#[cfg(not(target_arch = "wasm32"))]
pub mod native;

pub const MAX_COMMAND_BYTES: usize = 16 * 1024;
pub const MAX_STEPS: usize = 128;
pub const MAX_PARALLEL: usize = 32;

/// The runtime projects its engine store into the existing TS store shape.
/// `call` routes through the server's engine-backed callMethod translator.
pub trait Host {
    fn snapshot(&self) -> Value;
    fn call(&mut self, token: u64, method: &str, args: Value) -> Result<(), Value>;
    fn emit(&mut self, op: &str, data: Value);
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
}
struct Chain {
    symbol: String,
    req_id: Option<Value>,
    steps: VecDeque<String>,
    last: Option<Value>,
}
struct Pending {
    chain: Option<u64>,
    req_id: Option<Value>,
    success: String,
}
enum Evaluation {
    Done(Value),
    Call {
        method: &'static str,
        args: Value,
        success: String,
    },
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
        let result = match result {
            Ok(_) => {
                log(host, &pending.req_id, &pending.success);
                json!({"handled":true,"success":true})
            }
            Err(error) => {
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Command failed");
                log(
                    host,
                    &pending.req_id,
                    &format!("Error: Command failed: {message}"),
                );
                json!({"handled":true,"success":false,"error":message,"errorReported":true,"errorDetails":error})
            }
        };
        if let Some(id) = pending.chain {
            if let Some(chain) = self.chains.get_mut(&id) {
                chain.last = Some(result);
            }
            self.drive(id, host);
        }
    }

    /// The owning socket's close disposes its scope; issued writes are not resent.
    pub fn dispose(&mut self) {
        self.disposed = true;
        self.pending.clear();
        self.chains.clear();
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
            let action = evaluate(&chain.symbol, text, &host.snapshot(), &chain.req_id, host);
            match action {
                Evaluation::Done(result) => {
                    if !parallel {
                        chain.last = Some(result);
                    }
                    self.chains.insert(id, chain);
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
    rows(store, "markets").iter().find_map(|m| {
        let s = m.get("symbol")?.as_str()?;
        (s == symbol || s == symbol.to_uppercase()).then(|| s.into())
    })
}

fn evaluate(
    symbol: &str,
    text: &str,
    store: &Value,
    req_id: &Option<Value>,
    host: &mut impl Host,
) -> Evaluation {
    let args = positional(text);
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
        "cancel" | "x" => cancel(symbol, &args, store),
        "leverage" | "lev" => {
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
    let target = args
        .last()
        .and_then(|s| resolve(store, s))
        .unwrap_or_else(|| symbol.into());
    let head = args.get(1).map(String::as_str).unwrap_or("");
    let sub = args.get(2).map(String::as_str).unwrap_or("");
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
