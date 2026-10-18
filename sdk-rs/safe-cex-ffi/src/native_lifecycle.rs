use crate::{native_exchange::Notify, native_value::JsData};
use napi::{Status, threadsafe_function::ThreadsafeFunctionCallMode};
use safe_cex::{js_runtime::value::Js, runtime::ExchangeError};
use safe_cex_registry::{
    native_account::{NativeAccount, NativeAccountFuture, native_account_call},
    native_value::NativeValue,
};
use safe_cex_webworker::{
    payload::{StoreWorkerOp, StoreWorkerPayload},
    worker_logging::{self, WorkerLogLevel},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::HashMap,
    future::{Future, poll_fn},
    pin::pin,
    rc::Rc,
    task::Poll,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{oneshot, watch};

pub(crate) struct NativeStartup {
    generation: watch::Sender<u64>,
    name: String,
    account: Option<String>,
    on_event: Rc<Notify>,
    cooldowns: RefCell<HashMap<String, (f64, f64)>>,
}
impl NativeStartup {
    pub(crate) fn new(name: &str, account: Option<String>, on_event: Rc<Notify>) -> Self {
        Self {
            generation: watch::channel(0).0,
            name: name.into(),
            account,
            on_event,
            cooldowns: RefCell::default(),
        }
    }
    pub(crate) fn cancel(&self, account: &NativeAccount) {
        account.cancel_startup();
        self.generation
            .send_modify(|value| *value = value.wrapping_add(1));
    }
    fn post(&self, payload: StoreWorkerPayload) -> Result<(), ExchangeError> {
        let op = payload.op;
        let data = NativeValue::from(payload.data.unwrap_or(Value::Null)).to_js();
        let account = self.account.as_deref().map_or(Js::Undefined, Js::str);
        let prepare = || {
            match op {
                StoreWorkerOp::Error => {
                    let args = data.array_values().expect("worker error args");
                    let sentry = args[1].get("sentryData")?;
                    sentry.get("tags")?.define("account", account)?;
                    let error = sentry.get("error")?;
                    if !error.has_own("stack") {
                        error.define("stack", Js::Undefined)?;
                    }
                }
                StoreWorkerOp::Log => data.get("context")?.define("account", account)?,
                _ => unreachable!("native startup posts only worker logs"),
            }
            Ok::<_, safe_cex::js_runtime::value::Thrown>(())
        };
        prepare().map_err(|error| error.to_exchange_error())?;
        let event = match op {
            StoreWorkerOp::Error => "error",
            StoreWorkerOp::Log => "log",
            _ => unreachable!("native startup posts only worker logs"),
        };
        let status = self.on_event.call(
            JsData(NativeValue::object([
                ("event", NativeValue::string(event)),
                ("data", NativeValue::from_js(&data)?),
            ])),
            ThreadsafeFunctionCallMode::NonBlocking,
        );
        if status != Status::Ok {
            return Err(ExchangeError::new(format!(
                "native startup log delivery failed: {status:?}"
            )));
        }
        Ok(())
    }
    fn report_error(&self, account: &NativeAccount, error: &Value) -> Result<(), ExchangeError> {
        let detail = error.get("message").and_then(Value::as_str).unwrap_or("");
        let suffix = if detail.is_empty() {
            String::new()
        } else {
            format!(": {detail}")
        };
        let message = format!("Error starting exchange: {}{suffix}", self.name);
        worker_logging::forward_error(
            &self.cooldowns,
            (
                account.exchange_name(),
                self.account.as_deref().unwrap_or(""),
            ),
            &message,
            error,
            Some(&json!({"sentryTags":{"worker_error_phase":"startup"}})),
            true,
            (
                || {
                    Ok(match SystemTime::now().duration_since(UNIX_EPOCH) {
                        Ok(time) => time.as_secs_f64() * 1000.0,
                        Err(error) => -error.duration().as_secs_f64() * 1000.0,
                    })
                },
                || {
                    let mut fields = json!({"name":error["name"],"message":error["message"]});
                    if let Some(stack) = error.get("stack") {
                        fields["stack"] = stack.clone();
                    }
                    Ok(fields)
                },
                |payload| self.post(payload),
                |prefix, message, error, cause| {
                    eprintln!("{prefix} {message} {error} {cause}");
                    Ok(())
                },
            ),
        )
    }
    pub(crate) fn start(self: Rc<Self>, account: Rc<NativeAccount>) -> NativeAccountFuture<JsData> {
        let mut generation = self.generation.subscribe();
        let token = *generation.borrow();
        Box::pin(async move {
            if token != *generation.borrow() {
                return Ok(JsData(NativeValue::Bool(false)));
            }
            worker_logging::forward_log(
                account.exchange_name(),
                self.account.as_deref().unwrap_or(""),
                WorkerLogLevel::Info,
                &format!("Starting exchange in webworker: {}", self.name),
                None,
                |payload| self.post(payload),
            )
            .map_err(|error| json!({"name":error.name,"message":error.message}))?;
            let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
            let (finished, result) = oneshot::channel();
            let finished = Rc::new(RefCell::new(Some(finished)));
            let completed = finished.clone();
            let starting_account = account.clone();
            // Promise.race leaves the started venue operation running after a lost race.
            tokio::task::spawn_local(async move {
                let result = native_account_call(starting_account, "start".into(), vec![]).await;
                if let Some(finished) = completed.borrow_mut().take() {
                    let _ = finished.send(result);
                }
            });
            let timed_out_account = account.clone();
            let timeout = tokio::task::spawn_local(async move {
                tokio::time::sleep_until(deadline).await;
                if let Some(finished) = finished.borrow_mut().take() {
                    timed_out_account.cancel_startup();
                    let _ = finished.send(Err(
                        json!({"name":"Error","message":"Exchange start timed out after 30s"}),
                    ));
                }
            });
            let mut result = pin!(result);
            let outcome = {
                let mut changed = pin!(generation.changed());
                poll_fn(|context| {
                    if changed.as_mut().poll(context).is_ready() {
                        return Poll::Ready(Ok(NativeValue::Bool(false)));
                    }
                    match result.as_mut().poll(context) {
                        Poll::Ready(Ok(result)) => Poll::Ready(result),
                        Poll::Ready(Err(error)) => {
                            Poll::Ready(Err(json!({"name":"Error","message":error.to_string()})))
                        }
                        Poll::Pending => Poll::Pending,
                    }
                })
                .await
            };
            timeout.abort();
            if token != *generation.borrow() {
                eprintln!(
                    "Error starting exchange (disposed, suppressing): {}{}",
                    self.name,
                    outcome
                        .as_ref()
                        .err()
                        .map_or(String::new(), |error| format!(" {error}"))
                );
                return Ok(JsData(NativeValue::Bool(false)));
            }
            match outcome {
                Ok(result) => Ok(JsData(result)),
                Err(error) => {
                    self.report_error(&account, &error)
                        .map_err(|error| json!({"name":error.name,"message":error.message}))?;
                    Ok(JsData(NativeValue::Bool(false)))
                }
            }
        })
    }
}
