//! store.webworker.start, worker tables and module-owned perf fixtures.
//! Hosts supply callback installation, recording proxies and clock setters.
//! This module executes the actual protocol/init/map/cleanup body.
use crate::{
    exchange_worker::{ExchangeWorker, WorkerConstruction},
    exchange_worker_start::{WorkerBootstrapHost, decode_update, exchange_text},
    payload::{StoreWorkerOp, StoreWorkerPayload},
    perf_fixture::{PerfFixture, PerfFixtureHost},
    telegram::{TelegramHost, TelegramWorker},
    worker_ports::{WorkerConfigHandler, WorkerMessage, WorkerPort, WorkerPorts},
};
use indexmap::IndexMap;
use safe_cex_exchange::js_runtime::{
    library::in_operator,
    value::{Js, Thrown, to_typed},
};
use safe_cex_exchange::{
    eager::start_eager,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_types::ExchangeName;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
impl WorkerMessage for Js {
    fn is_truthy(&self) -> bool {
        self.truthy()
    }
}
pub type WorkerTable<H> = Rc<RefCell<IndexMap<String, Rc<ExchangeWorker<H>>>>>;
pub trait WorkerSessionHost:
    WorkerBootstrapHost + WorkerPort<Js, Error = ExchangeError> + PerfFixtureHost
{
    /// Return the cleanup that unregisters this listener at module teardown.
    fn install_message_listener(
        &self,
        listener: Rc<dyn Fn(Js) -> ExchangeFuture<'static, ()>>,
    ) -> Box<dyn FnOnce()>;
    /// Proxy equivalent: record before dispatch, preserving mutable port-key binding.
    fn worker_context(
        &self,
        shared: bool,
        port_key: Rc<RefCell<String>>,
        ports: Rc<WorkerPorts<Js, Self, Self>>,
        perf: Rc<PerfFixture<Self>>,
    ) -> Rc<Self>
    where
        Self: Sized;
    fn set_tick_interval_multipliers(&self, value: Js) -> Result<(), ExchangeError>;
    fn set_ws_diagnostics_enabled(&self, enabled: bool) -> Result<(), ExchangeError>;
    fn config_handlers(&self) -> Rc<RefCell<Vec<WorkerConfigHandler<Js>>>>;
    fn invoke_config_handler(
        &self,
        handler: WorkerConfigHandler<Js>,
        config: &Js,
    ) -> Result<(), ExchangeError>;
    fn sync_server_time(&self, time_ms: f64) -> Result<(), ExchangeError>;
    fn console(&self, level: &str, message: &str, data: Option<Js>) -> Result<(), ExchangeError>;
    /// Stop this module context's runtime, replay and timer work; shared calls are idempotent.
    fn shutdown_context(&self) -> Result<(), ExchangeError>;
    fn telegram_host(&self) -> Rc<dyn TelegramHost>;
}
pub struct WorkerModule<H: WorkerSessionHost> {
    pub exchange_workers: WorkerTable<H>,
    pub anonymous_exchange_workers: WorkerTable<H>,
    pub ports: Rc<WorkerPorts<Js, H, H>>,
    sessions: RefCell<Vec<Rc<WorkerSession<H>>>>,
    pub perf: Rc<PerfFixture<H>>,
}
impl<H: WorkerSessionHost> WorkerModule<H> {
    pub fn new(host: Rc<H>) -> Rc<Self> {
        Rc::new(Self {
            exchange_workers: Rc::default(),
            anonymous_exchange_workers: Rc::default(),
            perf: PerfFixture::new(&host),
            ports: WorkerPorts::new(host),
            sessions: RefCell::default(),
        })
    }
    pub fn exchange_workers(&self) -> WorkerTable<H> {
        self.exchange_workers.clone()
    }
    pub fn anonymous_exchange_workers(&self) -> WorkerTable<H> {
        self.anonymous_exchange_workers.clone()
    }
    // ts: start. Capture bindings and install exactly one real message listener.
    pub fn start(self: &Rc<Self>, ctx: Rc<H>, shared: bool) -> Rc<WorkerSession<H>> {
        let session = Rc::new(WorkerSession {
            module: Rc::downgrade(self),
            ctx: ctx.clone(),
            shared,
            account_name: RefCell::new(String::new()),
            exchange_name: RefCell::new(None),
            init_payload: RefCell::new(Js::Undefined),
            unregister: RefCell::new(None),
            port_key: Rc::new(RefCell::new(String::new())),
            has_logged_instance_mismatch: Cell::new(false),
        });
        let weak = Rc::downgrade(&session);
        let unregister = ctx.install_message_listener(Rc::new(move |payload| {
            if let Some(session) = weak.upgrade() {
                session.handle(payload)
            } else {
                Box::pin(async { Err(ExchangeError::new("Worker session owner released")) })
            }
        }));
        *session.unregister.borrow_mut() = Some(unregister);
        self.sessions.borrow_mut().push(session.clone());
        session
    }
}
impl<H: WorkerSessionHost> Drop for WorkerModule<H> {
    fn drop(&mut self) {
        self.perf.dispose();
        let sessions = std::mem::take(self.sessions.get_mut());
        for session in &sessions {
            if let Some(unregister) = session.unregister.borrow_mut().take() {
                unregister();
            }
            self.ports.cleanup_port_everywhere(&session.ctx);
        }
        let workers = std::mem::take(&mut *self.exchange_workers.borrow_mut())
            .into_values()
            .chain(std::mem::take(&mut *self.anonymous_exchange_workers.borrow_mut()).into_values())
            .collect::<Vec<_>>();
        for worker in workers {
            worker
                .dispose_with_error_handler(&mut |error| {
                    self.ports.host.runtime().report_exception(
                        &error,
                        &serde_json::json!({"source":"workerModuleTeardown"}).into(),
                    );
                    Ok(())
                })
                .expect("native context disposal error handler always continues");
            if let Some(exchange) = worker.exchange() {
                safe_cex_exchange::native_final_drop(exchange.base(), || Ok(()));
            }
        }
        for session in sessions {
            if let Err(error) = session.ctx.shutdown_context() {
                self.ports.host.runtime().report_exception(
                    &error,
                    &serde_json::json!({"source":"workerContextTeardown"}).into(),
                );
            }
        }
    }
}
pub struct WorkerSession<H: WorkerSessionHost> {
    pub module: Weak<WorkerModule<H>>,
    pub ctx: Rc<H>,
    pub shared: bool,
    pub account_name: RefCell<String>,
    pub exchange_name: RefCell<Option<String>>,
    pub init_payload: RefCell<Js>,
    unregister: RefCell<Option<Box<dyn FnOnce()>>>,
    pub port_key: Rc<RefCell<String>>,
    pub has_logged_instance_mismatch: Cell<bool>,
}
impl<H: WorkerSessionHost> WorkerSession<H> {
    fn module(&self) -> Result<Rc<WorkerModule<H>>, ExchangeError> {
        self.module
            .upgrade()
            .ok_or_else(|| ExchangeError::new("Worker module owner released"))
    }
    pub fn get_worker(&self) -> Option<Rc<ExchangeWorker<H>>> {
        let module = self.module.upgrade()?;
        let account = self.account_name.borrow().clone();
        if !account.is_empty() {
            module.exchange_workers.borrow().get(&account).cloned()
        } else {
            let name = self.exchange_name.borrow().clone()?;
            module
                .anonymous_exchange_workers
                .borrow()
                .get(&name)
                .cloned()
        }
    }
    fn delete_worker(&self) -> Result<(), ExchangeError> {
        let module = self.module()?;
        let account = self.account_name.borrow().clone();
        if !account.is_empty() {
            module.exchange_workers.borrow_mut().shift_remove(&account);
        } else if let Some(name) = self.exchange_name.borrow().clone() {
            module
                .anonymous_exchange_workers
                .borrow_mut()
                .shift_remove(&name);
        }
        Ok(())
    }
    fn original_post(&self, payload: Js) -> Result<(), ExchangeError> {
        if self.shared {
            let module = self.module()?;
            let key = self.port_key.borrow().clone();
            if !key.is_empty() {
                module
                    .ports
                    .broadcast_with_throttle(&key, Rc::new(payload), None);
            }
            Ok(())
        } else {
            self.ctx.post_message(&Rc::new(payload))
        }
    }
    fn replay_post(self: &Rc<Self>) -> Rc<dyn Fn(Js) -> Result<(), ExchangeError>> {
        let weak = Rc::downgrade(self);
        Rc::new(move |p| {
            weak.upgrade()
                .ok_or_else(|| ExchangeError::new("Session released during replay"))?
                .original_post(p)
        })
    }
    fn init(self: &Rc<Self>, payload: Js) -> ExchangeFuture<'static, ()> {
        let prefix: Result<
            Option<(Rc<ExchangeWorker<H>>, ExchangeFuture<'static, bool>)>,
            ExchangeError,
        > = (|| {
            let module = self.module()?;
            *self.init_payload.borrow_mut() = payload.clone();
            let data = payload.get("data")?;
            let account = data.get("accountName")?;
            *self.account_name.borrow_mut() = if account.is_nullish() {
                String::new()
            } else {
                to_typed(&account)?
            };
            let raw_name = data.get("exchangeName")?;
            *self.exchange_name.borrow_mut() = if raw_name.is_nullish() {
                None
            } else {
                Some(to_typed(&raw_name)?)
            };
            if module.perf.is_replaying() {
                self.ctx.post_payload(StoreWorkerPayload {
                    op: StoreWorkerOp::StartStatus,
                    instance_id: None,
                    data: Some(serde_json::Value::Bool(true)),
                })?;
                module.perf.start_replay(self.replay_post())?;
                return Ok(None);
            }
            let name: ExchangeName = to_typed(&raw_name)?;
            let account = self.account_name.borrow().clone();
            *self.port_key.borrow_mut() = if !account.is_empty() {
                format!("account:{account}")
            } else {
                format!("exchange:{}", exchange_text(&name))
            };
            // Source constructs the recording proxy even when shared reuse returns early.
            let mut wrapped = self.ctx.worker_context(
                false,
                self.port_key.clone(),
                module.ports.clone(),
                module.perf.clone(),
            );
            if self.shared {
                let key = self.port_key.borrow().clone();
                let list = {
                    let mut ports = module.ports.ports.borrow_mut();
                    ports.entry(key).or_default().clone()
                };
                if !list.borrow().iter().any(|p| Rc::ptr_eq(p, &self.ctx)) {
                    list.borrow_mut().push(self.ctx.clone());
                }
                if let Some(existing) = self.get_worker() {
                    if existing.exchange().is_some_and(|e| e.base().is_disposed()) {
                        self.delete_worker()?;
                    } else {
                        self.ctx.post_payload(StoreWorkerPayload {
                            op: StoreWorkerOp::StartStatus,
                            instance_id: None,
                            data: Some(serde_json::Value::Bool(true)),
                        })?;
                        existing.send_snapshot()?;
                        return Ok(None);
                    }
                }
                wrapped = self.ctx.worker_context(
                    true,
                    self.port_key.clone(),
                    module.ports.clone(),
                    module.perf.clone(),
                );
            }
            let decode = |key: &str| -> Result<Js, ExchangeError> { Ok(data.get(key)?) };
            let worker = ExchangeWorker::new(WorkerConstruction {
                ctx: wrapped,
                account_name: self.account_name.borrow().clone(),
                exchange_name: name.clone(),
                options: Rc::new(RefCell::new(to_typed(&decode("options")?)?)),
                settings: Rc::new(RefCell::new(to_typed(&decode("settings")?)?)),
                metadata: Rc::new(RefCell::new(to_typed(&decode("metadata")?)?)),
                session_key: {
                    let key = data.get("sessionKey")?;
                    if key.is_nullish() {
                        String::new()
                    } else {
                        to_typed(&key)?
                    }
                },
                instance_id: {
                    let id = payload.get("instanceId")?;
                    if id.is_undefined() {
                        None
                    } else {
                        Some(to_typed(&id)?)
                    }
                },
            });
            let account = self.account_name.borrow().clone();
            if !account.is_empty() {
                module
                    .exchange_workers
                    .borrow_mut()
                    .insert(account, worker.clone());
            } else {
                let name = self.exchange_name.borrow().clone().unwrap();
                module
                    .anonymous_exchange_workers
                    .borrow_mut()
                    .insert(name, worker.clone());
            }
            worker
                .store()
                .update(decode_update(decode("store")?)?, false)?;
            let starting = worker.start();
            Ok(Some((worker, starting)))
        })();
        let session = self.clone();
        Box::pin(async move {
            if let Some((worker, starting)) = prefix? {
                let started = starting.await?;
                if !started {
                    if session
                        .get_worker()
                        .is_some_and(|current| Rc::ptr_eq(&current, &worker))
                    {
                        session.delete_worker()?;
                    }
                    worker.dispose()?;
                }
            }
            Ok(())
        })
    }
    // Execute synchronous listener prefix now; returned future models its Promise.
    pub fn handle(self: &Rc<Self>, payload: Js) -> ExchangeFuture<'static, ()> {
        let original_payload = payload.clone();
        let prefix: Result<Option<ExchangeFuture<'static, ()>>, ExchangeError> = (|| {
            let module = self.module()?;
            let is_op = |name: &str| -> Result<bool, ExchangeError> {
                Ok(!payload.is_nullish() && payload.get("op")?.strict_equals(&Js::str(name)))
            };
            if is_op("__worker_ping__")? {
                let message = Js::object(vec![
                    ("op", Js::str("__worker_pong__")),
                    ("instanceId", payload.get("instanceId")?),
                    (
                        "data",
                        Js::object(vec![("ts", Js::Number(self.ctx.now_ms()))]),
                    ),
                ]);
                let _ = self.ctx.post_message(&Rc::new(message));
                return Ok(None);
            }
            if is_op("__workerConfig")? {
                let value = payload.get("data")?;
                let cfg = if value.is_nullish() {
                    Js::object(vec![])
                } else {
                    value
                };
                if in_operator(&cfg, "tickIntervalMultipliers")? {
                    self.ctx
                        .set_tick_interval_multipliers(cfg.get("tickIntervalMultipliers")?)?;
                }
                if in_operator(&cfg, "wsDiagnosticsEnabled")? {
                    self.ctx.set_ws_diagnostics_enabled(
                        cfg.get("wsDiagnosticsEnabled")?
                            .strict_equals(&Js::Bool(true)),
                    )?;
                }
                let handlers = self.ctx.config_handlers();
                let mut index = 0;
                loop {
                    let handler = handlers.borrow().get(index).cloned();
                    let Some(handler) = handler else { break };
                    if let Err(error) = self.ctx.invoke_config_handler(handler, &cfg) {
                        self.ctx.console(
                            "error",
                            "[store.webworker] __workerConfig host handler threw",
                            Some(Thrown::from(error).value),
                        )?;
                    }
                    index += 1;
                }
                if in_operator(&cfg, "perfFixture")? {
                    module.perf.configure(cfg.get("perfFixture")?)?;
                    if module.perf.is_replaying() {
                        module.perf.start_replay(self.replay_post())?;
                    }
                }
                return Ok(None);
            }
            if is_op("__perfFixtureDump")? {
                let _ = (|| -> Result<(), ExchangeError> {
                    let data = module.perf.take()?;
                    self.original_post(Js::object(vec![
                        ("op", Js::str("__perfFixtureDump")),
                        ("data", data),
                    ]))
                })();
                return Ok(None);
            }
            if is_op("sync-time")? {
                let data = payload.get("data")?;
                let time = if data.is_nullish() {
                    Js::Undefined
                } else {
                    data.get("timeMs")?
                };
                if let Js::Number(time) = time {
                    if time.is_finite() {
                        self.ctx.sync_server_time(time)?;
                    }
                }
                return Ok(None);
            }
            let worker = self.get_worker();
            if !self.shared {
                if let Some(worker) = &worker {
                    let actual = payload.get("instanceId")?;
                    if let Some(expected) = &worker.construction.instance_id {
                        if !expected.is_empty()
                            && actual.truthy()
                            && !actual.strict_equals(&Js::str(expected))
                        {
                            if !self.has_logged_instance_mismatch.replace(true) {
                                self.ctx.console("warn","[StoreWebWorker] Instance ID mismatch detected; dropping stale messages for this worker lifecycle",Some(Js::object(vec![("expectedInstanceId",Js::str(expected)),("receivedInstanceId",actual),("payloadOp",payload.get("op")?),("account",Js::str(&if self.account_name.borrow().is_empty(){"unknown".into()}else{self.account_name.borrow().clone()})),("exchange",Js::str(&self.exchange_name.borrow().clone().unwrap_or("unknown".into())))])))?;
                            }
                            return Ok(None);
                        }
                    }
                }
            }
            let op = payload.get("op")?;
            match op.as_str().map(|value| value.as_ref()) {
                Some(value)
                    if value == safe_cex_exchange::js_runtime::value::utf16("init").as_ref() =>
                {
                    return Ok(Some(self.init(payload)));
                }
                Some(value)
                    if value == safe_cex_exchange::js_runtime::value::utf16("dispose").as_ref() =>
                {
                    if self.shared {
                        module.ports.cleanup_port_everywhere(&self.ctx);
                        let key = self.port_key.borrow().clone();
                        let remaining = !key.is_empty()
                            && module
                                .ports
                                .ports
                                .borrow()
                                .get(&key)
                                .is_some_and(|p| !p.borrow().is_empty());
                        if !remaining {
                            if let Some(worker) = worker {
                                self.delete_worker()?;
                                worker.handle_payload(payload)?;
                            }
                        }
                    } else if let Some(worker) = worker {
                        self.delete_worker()?;
                        worker.handle_payload(payload)?;
                    }
                }
                Some(value)
                    if value == safe_cex_exchange::js_runtime::value::utf16("reinit").as_ref() =>
                {
                    let name = if self.account_name.borrow().is_empty() {
                        self.exchange_name
                            .borrow()
                            .as_ref()
                            .cloned()
                            .unwrap_or("undefined".into())
                    } else {
                        self.account_name.borrow().clone()
                    };
                    self.ctx.console(
                        "log",
                        &format!("[Worker] 🔄 reinit (force dispose) received for {name}"),
                        None,
                    )?;
                    if let Some(worker) = worker {
                        self.ctx.console(
                            "log",
                            "[Worker] 🗑️ Force disposing existing exchange",
                            None,
                        )?;
                        self.delete_worker()?;
                        worker.handle_payload(Js::object(vec![("op", Js::str("dispose"))]))?;
                    }
                    let key = self.port_key.borrow().clone();
                    if self.shared && !key.is_empty() {
                        let list = module
                            .ports
                            .ports
                            .borrow()
                            .get(&key)
                            .cloned()
                            .unwrap_or_default();
                        let mut i = 0;
                        loop {
                            let p = list.borrow().get(i).cloned();
                            let Some(p) = p else { break };
                            module.ports.cleanup_port_state(&p);
                            i += 1;
                        }
                        module.ports.ports.borrow_mut().insert(key, Rc::default());
                    }
                    self.ctx.console(
                        "log",
                        "[Worker] ✓ Exchange disposed, ready for fresh init",
                        None,
                    )?;
                }
                Some(value)
                    if value
                        == safe_cex_exchange::js_runtime::value::utf16("sendSnapshot").as_ref() =>
                {
                    if let Some(worker) = worker {
                        worker.send_snapshot()?;
                    }
                }
                _ => {
                    if let Some(worker) = worker {
                        if self.shared {
                            let data = payload.get("data")?;
                            let sub_id = if data.is_nullish() {
                                Js::Undefined
                            } else {
                                data.get("subId")?
                            };
                            if sub_id.truthy() {
                                let key = String::from_utf16_lossy(&sub_id.to_js_string()?);
                                module
                                    .ports
                                    .ports_by_sub_id
                                    .borrow_mut()
                                    .insert(key, self.ctx.clone());
                            }
                        }
                        worker.handle_payload(payload)?;
                    }
                }
            }
            Ok(None)
        })();
        let session = self.clone();
        start_eager(&self.ctx.runtime(), async move {
            if let Some(init) = prefix? {
                init.await?;
                let worker = session.get_worker();
                if let Some(worker) = worker {
                    if worker.exchange().is_some_and(|e| !e.base().is_disposed()) {
                        let token = original_payload.get("data")?.get("telegramBotToken")?;
                        let token: Option<String> =
                            if token.truthy() && !token.strict_equals(&Js::str("")) {
                                Some(to_typed(&token)?)
                            } else {
                                None
                            };
                        if let Some(token) = token {
                            let worker = worker.clone();
                            let telegram = TelegramWorker::new(
                                Rc::new(move |_| {
                                    worker.exchange().ok_or_else(|| {
                                        ExchangeError::new("Telegram exchange cleared")
                                    })
                                }),
                                token,
                            );
                            telegram.start(session.ctx.telegram_host());
                        }
                    }
                }
            }
            Ok(())
        })
    }
}
