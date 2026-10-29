use super::{Adapter, serialize_error, translate};
use crate::{Host, protection, rows};
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
                    }
                    Err(error) => {
                        failures.push(named(error, "cancel", &symbol));
                    }
                }
            }
        }
        if !matches!(mode, "orders" | "ord") {
            let store = self.snapshot();
            let positions = rows(&store, "positions")
                .iter()
                .filter(|p| p["contracts"].as_f64().is_some_and(|v| v != 0.))
                .cloned()
                .collect::<Vec<_>>();
            for position in positions {
                if self.cx.scope.is_disposed() {
                    return Err(
                        json!({"message":"Nuke scope disposed","outcome":"unknown","partialOrderIds":ids}),
                    );
                }
                let result = match protection::prepare_close(&position, None, &self.snapshot()) {
                    Err(message) => Err(json!({"message":message,"outcome":"notSent"})),
                    Ok(order) => self.nuke_command("placeOrder", json!([order])).await,
                };
                match result {
                    Ok(value) => {
                        if let Some(real) =
                            value.as_array().filter(|a| a.iter().all(Value::is_string))
                        {
                            ids.extend(real.iter().cloned());
                            completed += 1;
                            closed += 1;
                        } else {
                            failures.push(named(
                                json!({"message":"Invalid placement result","outcome":"unknown"}),
                                "close",
                                position["symbol"].as_str().unwrap_or("unknown"),
                            ));
                        }
                    }
                    Err(error) => {
                        if let Some(partial) = error["partialOrderIds"].as_array() {
                            ids.extend(partial.iter().cloned());
                        }
                        failures.push(named(
                            error,
                            "close",
                            position["symbol"].as_str().unwrap_or("unknown"),
                        ));
                    }
                }
            }
        }
        if failures.is_empty() {
            Ok(
                json!({"summary":format!("Nuke completed: {closed} position close(s) accepted, {cancelled} symbol cancel(s) accepted")}),
            )
        } else {
            let unknown = failures.iter().any(|e| e["outcome"] == "unknown");
            let mut error = failures
                .iter()
                .find(|e| e["outcome"] == "unknown")
                .unwrap_or(&failures[0])
                .clone();
            error["message"] = json!(format!(
                "Nuke completed {completed} operation(s), {} failed: {}",
                failures.len(),
                failures
                    .iter()
                    .map(|e| e["message"].as_str().unwrap_or("command failed"))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
            error["partialOrderIds"] = json!(ids);
            error["failedOperationCount"] = json!(failures.len());
            error["failedLegs"] = json!(failures);
            error["acceptedCloseCount"] = json!(closed);
            error["cancelledSymbolCount"] = json!(cancelled);
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
    async fn nuke_command(&self, method: &str, args: Value) -> Result<Value, Value> {
        let command = translate(method, &args, self.cx.port.next_command_id())
            .map_err(|message| json!({"message":message,"outcome":"notSent"}))?;
        self.cx
            .port
            .command(command)
            .await
            .map_err(serialize_error)
            .map(|s| serde_json::from_str(&s).unwrap_or(Value::Null))
    }
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
