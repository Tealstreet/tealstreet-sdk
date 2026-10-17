use crate::payload::{StoreWorkerOp, StoreWorkerPayload};
use safe_cex_exchange::{
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeFuture},
};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

pub type CommandExecutor =
    Rc<dyn Fn(Rc<dyn Exchange>, String, String, Option<String>) -> ExchangeFuture<'static, ()>>;
fn executor_slot<T>(f: impl FnOnce(&RefCell<Option<CommandExecutor>>) -> T) -> T {
    thread_local! { static EXECUTOR: RefCell<Option<CommandExecutor>> = RefCell::new(None); }
    EXECUTOR.with(f)
}

pub fn register_command_executor(executor: CommandExecutor) {
    executor_slot(|slot| *slot.borrow_mut() = Some(executor));
}
pub fn get_command_executor() -> Option<CommandExecutor> {
    executor_slot(|slot| slot.borrow().clone())
}
pub fn reset_command_executor() {
    executor_slot(|slot| *slot.borrow_mut() = None);
}

pub struct CommandHandler {
    exchange: Rc<dyn Exchange>,
}
impl CommandHandler {
    pub fn new(exchange: Rc<dyn Exchange>) -> Self {
        Self { exchange }
    }
    pub fn handle(&self, payload: StoreWorkerPayload) -> Result<(), ExchangeError> {
        if payload.op != StoreWorkerOp::Exec {
            return Ok(());
        }
        let data = payload.data.unwrap_or(Value::Null);
        // Source typeof treats arrays as objects too; primitives never reach the executor.
        if !data.is_object() && !data.is_array() {
            command_failure(
                self.exchange.as_ref(),
                None,
                false,
                "Invalid command payload.".into(),
            )?;
            return Ok(());
        }
        let req_id = data.get("reqId").and_then(Value::as_str).map(str::to_owned);
        let Some(executor) = get_command_executor() else {
            command_failure(
                self.exchange.as_ref(),
                req_id,
                false,
                "Command execution is not available in this environment.".into(),
            )?;
            return Ok(());
        };
        let exchange = self.exchange.clone();
        let symbol = data["symbol"].as_str().unwrap_or("").to_owned();
        let command = data["command"].as_str().unwrap_or("").to_owned();
        // The injected executor owns successful/handled-result events, including success:false.
        let future = executor(exchange.clone(), symbol, command, req_id.clone());
        self.exchange
            .base()
            .runtime
            .spawn_local(Box::pin(async move {
                if let Err(error) = future.await
                    && let Err(report_error) = command_failure(
                        exchange.as_ref(),
                        req_id,
                        true,
                        format!("Command failed: {}", error.message),
                    )
                {
                    exchange.base().runtime.report_exception(
                        &report_error,
                        &json!({"method":"CommandHandler.handle"}),
                    );
                }
            }));
        Ok(())
    }
}
fn command_failure(
    exchange: &dyn Exchange,
    req_id: Option<String>,
    handled: bool,
    message: String,
) -> Result<(), ExchangeError> {
    let mut log = json!({"message":message});
    if let Some(id) = &req_id {
        log["reqId"] = json!(id);
    }
    exchange.base().emit(
        "exec-log",
        safe_cex_exchange::js_runtime::value::from_json(&log),
    )?;
    if let Some(id) = req_id {
        exchange.base().emit(
            "exec-result",
            safe_cex_exchange::js_runtime::value::from_json(
                &json!({"reqId":id,"handled":handled,"success":false,"error":message}),
            ),
        )?;
    }
    Ok(())
}
