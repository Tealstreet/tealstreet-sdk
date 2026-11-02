use super::{Adapter, CancelAllScope, serialize_error, translate};
use crate::{Host, protection, rows};
use safe_cex_engine::command::decode_command;
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};
pub(super) struct NukeGuard(pub Rc<Cell<bool>>);
impl Drop for NukeGuard {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
impl Adapter {
    pub(super) async fn run_nuke(&self, mode: &str) -> Result<Value, Value> {
        let mut failures = Vec::new();
        let mut ids = Vec::new();
        let mut completed = 0;
        let mut closed = 0;
        let mut cancelled = 0;
        let mut swept = 0;
        let mut cancelled_symbols = Vec::new();
        let mut close_failures = Vec::new();
        // Running chasers and TWAPs would place again after the cancels below, so a
        // close-all would not close all: stop them first, waiting for their cancels.
        let stopped_twaps = self
            .twap
            .as_ref()
            .map(|twap| twap.stop_for_nuke())
            .unwrap_or_default();
        let stopped_chasers = match &self.chasers {
            Some(book) => book.stop_for_nuke(NUKE_STOP_REASON).await,
            None => Vec::new(),
        };
        if let Some(twap) = &self.twap {
            twap.placements_settled().await;
        }
        let stopped = json!({"stoppedChasers": stopped_chasers, "stoppedTwaps": stopped_twaps});
        // A browser writer runs the legs itself; the host only stops its algos.
        if mode == "algos" {
            return Ok(stopped);
        }
        if !matches!(mode, "positions" | "pos") {
            let store = self.snapshot();
            let mut symbols = Vec::new();
            for order in rows(&store, "orders") {
                if let Some(symbol) = order["symbol"].as_str().filter(|s| !s.is_empty())
                    && !symbols.iter().any(|s| s == symbol)
                {
                    symbols.push(symbol.to_string());
                }
            }
            for symbol in symbols {
                if self.cx.scope.is_disposed() {
                    return Err(
                        json!({"message":"Nuke scope disposed","outcome":"unknown","partialOrderIds":ids}),
                    );
                }
                match self
                    .nuke_command("cancelSymbolOrders", json!([symbol]))
                    .await
                {
                    Ok(_) => {
                        completed += 1;
                        cancelled += 1;
                        cancelled_symbols.push(symbol.clone());
                    }
                    Err(error) => {
                        failures.push(named(error, "cancel", &symbol));
                    }
                }
            }
        }
        if !matches!(mode, "orders" | "ord") {
            let fresh_read = self.cx.port.capabilities().fresh_positions;
            let mut skipped = Vec::new();
            let mut positions = open_positions(&self.snapshot());
            for attempt in 0..=NUKE_CLOSE_RETRIES {
                let mut failed = Vec::new();
                for position in positions {
                    if self.cx.scope.is_disposed() {
                        return Err(
                            json!({"message":"Nuke scope disposed","outcome":"unknown","partialOrderIds":ids}),
                        );
                    }
                    let symbol = position["symbol"].as_str().unwrap_or("unknown").to_owned();
                    let result = match protection::prepare_close(&position, None, &self.snapshot())
                    {
                        Err(message) => Err(json!({"message":message,"outcome":"notSent"})),
                        Ok(order) => self.nuke_command("placeOrder", json!([order])).await,
                    };
                    let failure = match result {
                        Ok(value) => {
                            if let Some(real) =
                                value.as_array().filter(|a| a.iter().all(Value::is_string))
                            {
                                ids.extend(real.iter().cloned());
                                completed += 1;
                                closed += 1;
                                continue;
                            }
                            json!({"message":"Invalid placement result","outcome":"unknown"})
                        }
                        Err(error) => {
                            if let Some(partial) = error["partialOrderIds"].as_array() {
                                ids.extend(partial.iter().cloned());
                            }
                            error
                        }
                    };
                    failed.push((position, named(failure, "close", &symbol)));
                }
                // A venue with the read closes again only on one issued after this round.
                if fresh_read {
                    for _ in 0..NUKE_FLAT_SETTLE_MS / NUKE_CLOSE_RETRY_MS {
                        if open_positions(&self.snapshot()).is_empty() {
                            break;
                        }
                        safe_cex_algos::executor::sleep_ms(NUKE_CLOSE_RETRY_MS).await;
                    }
                    if self.cx.scope.is_disposed() {
                        return Err(
                            json!({"message":"Nuke scope disposed","outcome":"unknown","partialOrderIds":ids}),
                        );
                    }
                    let read = self.read_fresh_positions().await;
                    let open = open_positions(&self.snapshot());
                    close_failures = failed
                        .into_iter()
                        .filter(|(position, _)| {
                            open.iter().any(|row| {
                                row["symbol"] == position["symbol"]
                                    && row["side"] == position["side"]
                            })
                        })
                        .map(|(_, failure)| failure)
                        .collect();
                    match read {
                        Ok(rows) => skipped = rows,
                        Err(error) => {
                            let cause = error["message"].as_str().unwrap_or("read failed");
                            let named = describe_open(&open);
                            let left = if named.is_empty() {
                                String::new()
                            } else {
                                format!("{named} may still be open: ")
                            };
                            failures.push(
                                json!({"message":format!("{NUKE_NOT_CONFIRMED}: {left}{cause}")}),
                            );
                            break;
                        }
                    }
                    if open.is_empty() {
                        break;
                    }
                    if attempt == NUKE_CLOSE_RETRIES {
                        if close_failures.is_empty() {
                            failures.push(json!({"message":format!("Nuke did not leave the account flat: {} still open", describe_open(&open))}));
                        }
                        break;
                    }
                    positions = open;
                    continue;
                }
                if failed.is_empty() {
                    close_failures.clear();
                    break;
                }
                if attempt < NUKE_CLOSE_RETRIES {
                    safe_cex_algos::executor::sleep_ms(NUKE_CLOSE_RETRY_MS).await;
                }
                // Retry only a failed close whose position still shows open; a
                // close that succeeded behind a lagging store is never resent.
                let open = open_positions(&self.snapshot());
                failed.retain(|(position, _)| {
                    open.iter().any(|row| {
                        row["symbol"] == position["symbol"] && row["side"] == position["side"]
                    })
                });
                positions = open
                    .into_iter()
                    .filter(|row| {
                        failed.iter().any(|(position, _)| {
                            row["symbol"] == position["symbol"] && row["side"] == position["side"]
                        })
                    })
                    .collect();
                close_failures = failed.into_iter().map(|(_, failure)| failure).collect();
                if positions.is_empty() {
                    break;
                }
            }
            failures.append(&mut close_failures);
            if !skipped.is_empty() {
                failures.push(json!({"message":format!("{NUKE_NOT_CONFIRMED}: {} on no listed market", skipped.join(", "))}));
            }
        }
        // The venue's cancel-all reaches orders the store does not track, as in the browser.
        if !matches!(mode, "positions" | "pos") {
            for (target, command) in self.sweep_commands(&cancelled_symbols) {
                if self.cx.scope.is_disposed() {
                    return Err(
                        json!({"message":"Nuke scope disposed","outcome":"unknown","partialOrderIds":ids}),
                    );
                }
                match self.run_command(command).await {
                    Ok(_) => swept += 1,
                    Err(error) => failures.push(named(error, "cancel-all", &target)),
                }
            }
        }
        let stopped_count = stopped_chasers.len() + stopped_twaps.len();
        if failures.is_empty() {
            let mut result = json!({"summary":format!("Nuke completed: {stopped_count} algo(s) stopped, {closed} position close(s) accepted, {cancelled} symbol cancel(s) and {swept} cancel-all sweep(s) accepted")});
            result["stoppedChasers"] = stopped["stoppedChasers"].clone();
            result["stoppedTwaps"] = stopped["stoppedTwaps"].clone();
            Ok(result)
        } else {
            let unknown = failures.iter().any(|e| e["outcome"] == "unknown");
            let mut error = failures
                .iter()
                .find(|e| e["outcome"] == "unknown")
                .unwrap_or(&failures[0])
                .clone();
            let store = self.snapshot();
            let open = rows(&store, "orders")
                .iter()
                .map(|o| {
                    format!(
                        "{} {}",
                        o["symbol"].as_str().unwrap_or(""),
                        o["id"].as_str().unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            error["message"] = json!(format!(
                "Nuke completed {completed} operation(s), {} failed: {}. Open orders: {}",
                failures.len(),
                failures
                    .iter()
                    .map(|e| e["message"].as_str().unwrap_or("command failed"))
                    .collect::<Vec<_>>()
                    .join("; "),
                if open.is_empty() { "none" } else { &open }
            ));
            error["partialOrderIds"] = json!(ids);
            error["failedOperationCount"] = json!(failures.len());
            error["failedLegs"] = json!(failures);
            error["acceptedCloseCount"] = json!(closed);
            error["cancelledSymbolCount"] = json!(cancelled);
            error["stoppedChasers"] = stopped["stoppedChasers"].clone();
            error["stoppedTwaps"] = stopped["stoppedTwaps"].clone();
            if unknown {
                error["outcome"] = json!("unknown");
            } else if completed > 0
                && let Some(object) = error.as_object_mut()
            {
                object.remove("outcome");
            }
            Err(error)
        }
    }
    /// The final cancel-all: one venue-wide cancel, or each symbol with an open
    /// order now plus the host's attempted-placement symbols, less those this nuke
    /// has already cancelled.
    fn sweep_commands(
        &self,
        cancelled: &[String],
    ) -> Vec<(String, safe_cex_engine::engine::Command)> {
        let scope = self
            .cancel_all
            .as_ref()
            .map_or(CancelAllScope::Symbols(Vec::new()), |scopes| scopes());
        let symbols = match scope {
            CancelAllScope::Native => vec![String::new()],
            CancelAllScope::Symbols(placed) => {
                let store = self.snapshot();
                let mut symbols: Vec<String> = rows(&store, "orders")
                    .iter()
                    .filter_map(|order| order["symbol"].as_str().filter(|s| !s.is_empty()))
                    .map(str::to_owned)
                    .chain(placed)
                    .collect();
                symbols.sort();
                symbols.dedup();
                symbols.retain(|symbol| !cancelled.contains(symbol));
                symbols
            }
        };
        symbols
            .into_iter()
            .filter_map(|symbol| {
                let command = json!({"op":"cancelSymbolOrders","command":self.cx.port.next_command_id(),"symbol":symbol});
                let command = decode_command(&command.to_string()).ok()?;
                let target = if symbol.is_empty() { "all symbols".into() } else { symbol };
                Some((target, command))
            })
            .collect()
    }
    async fn nuke_command(&self, method: &str, args: Value) -> Result<Value, Value> {
        let capabilities = self.cx.port.capabilities();
        let command = translate(method, &args, self.cx.port.next_command_id(), &capabilities)
            .map_err(|message| json!({"message":message,"outcome":"notSent"}))?;
        self.run_command(command).await
    }
    /// The symbols of open rows a read issued now could not keep.
    async fn read_fresh_positions(&self) -> Result<Vec<String>, Value> {
        let command = json!({"op":"readPositions","command":self.cx.port.next_command_id()});
        let command = decode_command(&command.to_string())
            .map_err(|error| json!({"message":error.to_string()}))?;
        let answer = self.run_command(command).await?;
        serde_json::from_value(answer)
            .map_err(|_| json!({"message":"Invalid positions read result"}))
    }
    async fn run_command(&self, command: safe_cex_engine::engine::Command) -> Result<Value, Value> {
        self.cx
            .port
            .command(command)
            .await
            .map_err(serialize_error)
            .map(|s| serde_json::from_str(&s).unwrap_or(Value::Null))
    }
}

const NUKE_STOP_REASON: &str = "Stopped by nuke";
const NUKE_CLOSE_RETRIES: u32 = 3;
const NUKE_CLOSE_RETRY_MS: u64 = 100;
const NUKE_FLAT_SETTLE_MS: u64 = 2_000;
const NUKE_NOT_CONFIRMED: &str = "Nuke could not confirm the account is flat";

fn open_positions(store: &Value) -> Vec<Value> {
    rows(store, "positions")
        .iter()
        .filter(|p| p["contracts"].as_f64().is_some_and(|v| v != 0.))
        .cloned()
        .collect()
}

fn describe_open(positions: &[Value]) -> String {
    positions
        .iter()
        .map(|p| {
            format!(
                "{} {} {}",
                p["symbol"].as_str().unwrap_or("unknown"),
                p["side"].as_str().unwrap_or(""),
                p["contracts"]
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn named(mut error: Value, phase: &str, symbol: &str) -> Value {
    error["message"] = json!(format!(
        "{symbol} {phase}: {}",
        error["message"].as_str().unwrap_or("command failed")
    ));
    error["symbol"] = json!(symbol);
    error["operation"] = json!(phase);
    error
}
