use crate::{native_exchange::Notify, native_value::JsData};
use napi::{Status, threadsafe_function::ThreadsafeFunctionCallMode};
use safe_cex::{js_runtime::value::Js, runtime::ExchangeError};
use safe_cex_registry::{
    native_account::{NativeAccount, NativeAccountFuture, native_account_call},
    native_value::NativeValue,
};
use safe_cex_webworker::{
    data_post::WorkerDataPayload,
    payload::StoreWorkerOp,
    worker_logging::{self, WorkerLogLevel},
};
use std::{
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
    cooldowns: Js,
}
impl NativeStartup {
    pub(crate) fn new(name: &str, account: Option<String>, on_event: Rc<Notify>) -> Self {
        Self {
            generation: watch::channel(0).0,
            name: name.into(),
            account,
            on_event,
            cooldowns: Js::new_map(),
        }
    }
    pub(crate) fn cancel(&self, account: &NativeAccount) {
        account.cancel_startup();
        self.generation
            .send_modify(|value| *value = value.wrapping_add(1));
    }
    fn post(&self, payload: WorkerDataPayload) -> Result<(), ExchangeError> {
        let op = payload.op;
        let data = payload.data;
        let account = self.account.as_deref().map_or(Js::Undefined, Js::str);
        match op {
            StoreWorkerOp::Error => data
                .get("1")?
                .get("sentryData")?
                .get("tags")?
                .define("account", account)?,
            StoreWorkerOp::Log => data.get("context")?.define("account", account)?,
            _ => unreachable!("native startup posts only worker logs"),
        }
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
    fn report_error(
        &self,
        account: &NativeAccount,
        error: &NativeValue,
    ) -> Result<(), ExchangeError> {
        let mut message: Vec<u16> = format!("Error starting exchange: {}", self.name)
            .encode_utf16()
            .collect();
        if let Some(NativeValue::String(detail)) = error.get("message")
            && !detail.is_empty()
        {
            message.extend(": ".encode_utf16());
            message.extend(detail);
        }
        let raw = error.to_js();
        let fields = Js::object(vec![]);
        for key in ["message", "name", "stack"] {
            fields.define(key, raw.get(key)?)?;
        }
        worker_logging::forward_error(
            &self.cooldowns,
            (
                account.exchange_name(),
                self.account.as_deref().unwrap_or(""),
            ),
            &Js::utf16(&message),
            &raw,
            Some(&Js::object(vec![(
                "sentryTags",
                Js::object(vec![("worker_error_phase", Js::str("startup"))]),
            )])),
            true,
            (
                || {
                    Ok(match SystemTime::now().duration_since(UNIX_EPOCH) {
                        Ok(time) => time.as_secs_f64() * 1000.0,
                        Err(error) => -error.duration().as_secs_f64() * 1000.0,
                    })
                },
                || Ok(fields),
                |payload| self.post(payload),
                |prefix, message, error, cause| {
                    eprintln!("{prefix} {message:?} {error:?} {cause}");
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
                &Js::str(&format!("Starting exchange in webworker: {}", self.name)),
                None,
                |payload| self.post(payload),
            )
            .map_err(|error| {
                NativeValue::object([
                    ("name", NativeValue::string(&error.name)),
                    ("message", NativeValue::string(&error.message)),
                ])
            })?;
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
                    let _ = finished.send(Err(NativeValue::object([
                        ("name", NativeValue::string("Error")),
                        (
                            "message",
                            NativeValue::string("Exchange start timed out after 30s"),
                        ),
                    ])));
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
                        Poll::Ready(Err(error)) => Poll::Ready(Err(NativeValue::object([
                            ("name", NativeValue::string("Error")),
                            ("message", NativeValue::string(&error.to_string())),
                        ]))),
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
                        .map_or(String::new(), |error| format!(" {error:?}"))
                );
                return Ok(JsData(NativeValue::Bool(false)));
            }
            match outcome {
                Ok(result) => Ok(JsData(result)),
                Err(error) => {
                    self.report_error(&account, &error).map_err(|error| {
                        NativeValue::object([
                            ("name", NativeValue::string(&error.name)),
                            ("message", NativeValue::string(&error.message)),
                        ])
                    })?;
                    Ok(JsData(NativeValue::Bool(false)))
                }
            }
        })
    }
}
