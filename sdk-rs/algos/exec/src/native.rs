//! Adapter to the one shared account runtime. This module owns no scheduler.
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use safe_cex_algos::{AccountPort, AlgoCx, AlgoFactory};
use safe_cex_engine::command::decode_command;
use safe_cex_engine::engine::{Command, CommandId};
use safe_cex_engine::error::VenueError;
use serde_json::{Value, json};

use crate::{Exec, Host};

/// Register once in the server's account algo registry.
#[derive(Default)]
pub struct ExecFactory {
    parallel: Rc<Cell<usize>>,
    account: RefCell<Option<Weak<dyn AccountPort>>>,
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
}
struct ParallelReservation(Rc<Cell<usize>>);
impl Drop for ParallelReservation {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}
impl Host for Adapter {
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
        json!({"venue":store.venue,"markets":markets,"instrumentClassificationsReady":store.instrument_classifications.is_some(),
            "tickers":store.tickers.values().collect::<Vec<_>>(),"orders":store.orders,"positions":store.positions,"balance":store.balance,"socketHealth":{"privateInitialized":store.private_socket.initialized,"privateHealthy":store.private_socket.healthy}})
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
        let command = translate(method, &args, self.cx.port.next_command_id())
            .map_err(|error| json!({"message":error,"outcome":"notSent"}))?;
        let Some(exec) = self.exec.upgrade() else {
            return Err(json!({"message":"EXEC scope is disposed","outcome":"notSent"}));
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
            if adapter.cx.scope.is_disposed() {
                return;
            }
            let result = adapter
                .cx
                .port
                .command(command)
                .await
                .map(|s| serde_json::from_str(&s).unwrap_or(Value::String(s)))
                .map_err(serialize_error);
            drop(reservation);
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
pub fn translate(method: &str, args: &Value, id: CommandId) -> Result<Command, String> {
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
            return Ok(Command::CancelOrdersWithSymbols {
                command: id,
                orders: serde_json::from_value(Value::Array(orders?)).map_err(|e| e.to_string())?,
            });
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
            json!({"op":"amendOrder","command":id,"id":order_id,"price":input["update"]["price"]})
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
