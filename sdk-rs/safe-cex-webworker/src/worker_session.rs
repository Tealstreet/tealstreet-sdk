//! store.webworker.start and its two live worker tables. The native host owns
//! browser callback installation, proxy objects, perf-fixture imports and clock
//! setters. This module executes the actual protocol/init/map/cleanup body.
use crate::{
    exchange_worker::{ExchangeWorker, WorkerConstruction},
    exchange_worker_start::{WorkerBootstrapHost, decode_update, exchange_text},
    payload::{StoreWorkerOp, StoreWorkerPayload},
    telegram::{TelegramHost, TelegramWorker},
    worker_ports::{WorkerConfigHandler, WorkerMessage, WorkerPort, WorkerPorts},
};
use indexmap::IndexMap;
use safe_cex_exchange::{
    eager::start_eager,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
impl WorkerMessage for StoreWorkerPayload {
    fn is_truthy(&self) -> bool {
        true
    }
}
pub type WorkerTable<H> = Rc<RefCell<IndexMap<String, Rc<ExchangeWorker<H>>>>>;
pub trait WorkerSessionHost:
    WorkerBootstrapHost + WorkerPort<StoreWorkerPayload, Error = ExchangeError>
{
    fn install_message_listener(
        &self,
        listener: Rc<dyn Fn(StoreWorkerPayload) -> ExchangeFuture<'static, ()>>,
    );
    /// Proxy equivalent: record before dispatch, preserving mutable port-key binding.
    fn worker_context(
        &self,
        shared: bool,
        port_key: Rc<RefCell<String>>,
        ports: Rc<WorkerPorts<StoreWorkerPayload, Self, Self>>,
    ) -> Rc<Self>
    where
        Self: Sized;
    fn is_replaying_perf_fixture(&self) -> bool;
    fn start_replay(
        &self,
        post: Rc<dyn Fn(StoreWorkerPayload) -> Result<(), ExchangeError>>,
    ) -> Result<(), ExchangeError>;
    fn configure_perf_fixture(&self, value: Value) -> Result<(), ExchangeError>;
    fn take_perf_fixture(&self) -> Result<Value, ExchangeError>;
    fn set_tick_interval_multipliers(&self, value: Value) -> Result<(), ExchangeError>;
    fn set_ws_diagnostics_enabled(&self, enabled: bool) -> Result<(), ExchangeError>;
    fn config_handlers(&self) -> Rc<RefCell<Vec<WorkerConfigHandler<Value>>>>;
    fn invoke_config_handler(
        &self,
        handler: WorkerConfigHandler<Value>,
        config: &Value,
    ) -> Result<(), ExchangeError>;
    fn sync_server_time(&self, time_ms: f64) -> Result<(), ExchangeError>;
    fn console(&self, level: &str, message: &str, data: Option<Value>)
    -> Result<(), ExchangeError>;
    fn telegram_host(&self) -> Rc<dyn TelegramHost>;
}
pub struct WorkerModule<H: WorkerSessionHost> {
    pub exchange_workers: WorkerTable<H>,
    pub anonymous_exchange_workers: WorkerTable<H>,
    pub ports: Rc<WorkerPorts<StoreWorkerPayload, H, H>>,
}
impl<H: WorkerSessionHost> WorkerModule<H> {
    pub fn new(host: Rc<H>) -> Rc<Self> {
        Rc::new(Self {
            exchange_workers: Rc::default(),
            anonymous_exchange_workers: Rc::default(),
            ports: WorkerPorts::new(host),
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
            module: self.clone(),
            ctx: ctx.clone(),
            shared,
            account_name: RefCell::new(String::new()),
            exchange_name: RefCell::new(None),
            init_payload: RefCell::new(None),
            port_key: Rc::new(RefCell::new(String::new())),
            has_logged_instance_mismatch: Cell::new(false),
        });
        let weak = Rc::downgrade(&session);
        ctx.install_message_listener(Rc::new(move |payload| {
            if let Some(session) = weak.upgrade() {
                session.handle(payload)
            } else {
                Box::pin(async { Err(ExchangeError::new("Worker session owner released")) })
            }
        }));
        session
    }
}
pub struct WorkerSession<H: WorkerSessionHost> {
    pub module: Rc<WorkerModule<H>>,
    pub ctx: Rc<H>,
    pub shared: bool,
    pub account_name: RefCell<String>,
    pub exchange_name: RefCell<Option<ExchangeName>>,
    pub init_payload: RefCell<Option<StoreWorkerPayload>>,
    pub port_key: Rc<RefCell<String>>,
    pub has_logged_instance_mismatch: Cell<bool>,
}
impl<H: WorkerSessionHost> WorkerSession<H> {
    pub fn get_worker(&self) -> Option<Rc<ExchangeWorker<H>>> {
        let account = self.account_name.borrow().clone();
        if !account.is_empty() {
            self.module.exchange_workers.borrow().get(&account).cloned()
        } else {
            let name = self.exchange_name.borrow().clone()?;
            self.module
                .anonymous_exchange_workers
                .borrow()
                .get(&exchange_text(&name))
                .cloned()
        }
    }
    fn delete_worker(&self) {
        let account = self.account_name.borrow().clone();
        if !account.is_empty() {
            self.module
                .exchange_workers
                .borrow_mut()
                .shift_remove(&account);
        } else if let Some(name) = self.exchange_name.borrow().clone() {
            self.module
                .anonymous_exchange_workers
                .borrow_mut()
                .shift_remove(&exchange_text(&name));
        }
    }
    fn original_post(&self, payload: StoreWorkerPayload) -> Result<(), ExchangeError> {
        if self.shared {
            let key = self.port_key.borrow().clone();
            if !key.is_empty() {
                self.module
                    .ports
                    .broadcast_with_throttle(&key, Rc::new(payload), None);
            }
            Ok(())
        } else {
            self.ctx.post_payload(payload)
        }
    }
    fn replay_post(self: &Rc<Self>) -> Rc<dyn Fn(StoreWorkerPayload) -> Result<(), ExchangeError>> {
        let weak = Rc::downgrade(self);
        Rc::new(move |p| {
            weak.upgrade()
                .ok_or_else(|| ExchangeError::new("Session released during replay"))?
                .original_post(p)
        })
    }
    fn init(self: &Rc<Self>, payload: StoreWorkerPayload) -> ExchangeFuture<'static, ()> {
        let prefix: Result<
            Option<(Rc<ExchangeWorker<H>>, ExchangeFuture<'static, bool>)>,
            ExchangeError,
        > = (|| {
            *self.init_payload.borrow_mut() = Some(payload.clone());
            let data = payload
                .data
                .as_ref()
                .and_then(Value::as_object)
                .ok_or_else(|| ExchangeError::new("init data must be an object"))?;
            let account = data
                .get("accountName")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            *self.account_name.borrow_mut() = account;
            let name: ExchangeName =
                serde_json::from_value(data.get("exchangeName").cloned().unwrap_or(Value::Null))
                    .map_err(|e| ExchangeError::new(e.to_string()))?;
            *self.exchange_name.borrow_mut() = Some(name.clone());
            if self.ctx.is_replaying_perf_fixture() {
                self.ctx.post_payload(StoreWorkerPayload {
                    op: StoreWorkerOp::StartStatus,
                    instance_id: None,
                    data: Some(Value::Bool(true)),
                })?;
                self.ctx.start_replay(self.replay_post())?;
                return Ok(None);
            }
            let account = self.account_name.borrow().clone();
            *self.port_key.borrow_mut() = if !account.is_empty() {
                format!("account:{account}")
            } else {
                format!("exchange:{}", exchange_text(&name))
            };
            // Source constructs the recording proxy even when shared reuse returns early.
            let mut wrapped =
                self.ctx
                    .worker_context(false, self.port_key.clone(), self.module.ports.clone());
            if self.shared {
                let key = self.port_key.borrow().clone();
                let list = {
                    let mut ports = self.module.ports.ports.borrow_mut();
                    ports.entry(key).or_default().clone()
                };
                if !list.borrow().iter().any(|p| Rc::ptr_eq(p, &self.ctx)) {
                    list.borrow_mut().push(self.ctx.clone());
                }
                if let Some(existing) = self.get_worker() {
                    if existing.exchange().is_some_and(|e| e.base().is_disposed()) {
                        self.delete_worker();
                    } else {
                        self.ctx.post_payload(StoreWorkerPayload {
                            op: StoreWorkerOp::StartStatus,
                            instance_id: None,
                            data: Some(Value::Bool(true)),
                        })?;
                        existing.send_snapshot()?;
                        return Ok(None);
                    }
                }
                wrapped =
                    self.ctx
                        .worker_context(true, self.port_key.clone(), self.module.ports.clone());
            }
            let decode = |key: &str| {
                data.get(key)
                    .cloned()
                    .ok_or_else(|| ExchangeError::new(format!("Missing init {key}")))
            };
            let worker = ExchangeWorker::new(WorkerConstruction {
                ctx: wrapped,
                account_name: self.account_name.borrow().clone(),
                exchange_name: self.exchange_name.borrow().clone().unwrap(),
                options: Rc::new(RefCell::new(
                    serde_json::from_value(decode("options")?)
                        .map_err(|e| ExchangeError::new(e.to_string()))?,
                )),
                settings: Rc::new(RefCell::new(
                    serde_json::from_value(decode("settings")?)
                        .map_err(|e| ExchangeError::new(e.to_string()))?,
                )),
                metadata: Rc::new(RefCell::new(
                    serde_json::from_value(decode("metadata")?)
                        .map_err(|e| ExchangeError::new(e.to_string()))?,
                )),
                session_key: data
                    .get("sessionKey")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .into(),
                instance_id: payload.instance_id.clone(),
            });
            let account = self.account_name.borrow().clone();
            if !account.is_empty() {
                self.module
                    .exchange_workers
                    .borrow_mut()
                    .insert(account, worker.clone());
            } else {
                let name = self.exchange_name.borrow().clone().unwrap();
                self.module
                    .anonymous_exchange_workers
                    .borrow_mut()
                    .insert(exchange_text(&name), worker.clone());
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
                        session.delete_worker();
                    }
                    worker.dispose()?;
                }
            }
            Ok(())
        })
    }
    // Execute synchronous listener prefix now; returned future models its Promise.
    pub fn handle(self: &Rc<Self>, payload: StoreWorkerPayload) -> ExchangeFuture<'static, ()> {
        let original_payload = payload.clone();
        let prefix: Result<Option<ExchangeFuture<'static, ()>>, ExchangeError> = (|| {
            match payload.op {
                StoreWorkerOp::WorkerPing => {
                    let _ = self.ctx.post_payload(StoreWorkerPayload {
                        op: StoreWorkerOp::WorkerPong,
                        instance_id: payload.instance_id,
                        data: Some(json!({"ts":self.ctx.now_ms()})),
                    });
                    return Ok(None);
                }
                StoreWorkerOp::WorkerConfig => {
                    let cfg = payload.data.filter(|v| !v.is_null()).unwrap_or(json!({}));
                    let object = cfg
                        .as_object()
                        .ok_or_else(|| ExchangeError::new("Worker config must be an object"))?;
                    if let Some(value) = object.get("tickIntervalMultipliers") {
                        self.ctx.set_tick_interval_multipliers(value.clone())?;
                    }
                    if let Some(value) = object.get("wsDiagnosticsEnabled") {
                        self.ctx
                            .set_ws_diagnostics_enabled(value == &Value::Bool(true))?;
                    }
                    let handlers = self.ctx.config_handlers();
                    let mut index = 0;
                    loop {
                        let handler = handlers.borrow().get(index).cloned();
                        let Some(handler) = handler else { break };
                        if let Err(e) = self.ctx.invoke_config_handler(handler, &cfg) {
                            self.ctx.console(
                                "error",
                                "[store.webworker] __workerConfig host handler threw",
                                Some(json!({"name":e.name,"message":e.message})),
                            )?;
                        }
                        index += 1;
                    }
                    if let Some(value) = object.get("perfFixture") {
                        self.ctx.configure_perf_fixture(value.clone())?;
                        if self.ctx.is_replaying_perf_fixture() {
                            self.ctx.start_replay(self.replay_post())?;
                        }
                    }
                    return Ok(None);
                }
                StoreWorkerOp::PerfFixtureDump => {
                    let _ = (|| {
                        let data = self.ctx.take_perf_fixture()?;
                        self.original_post(StoreWorkerPayload {
                            op: StoreWorkerOp::PerfFixtureDump,
                            instance_id: None,
                            data: Some(data),
                        })
                    })();
                    return Ok(None);
                }
                StoreWorkerOp::SyncTime => {
                    if let Some(time) = payload
                        .data
                        .as_ref()
                        .and_then(|d| d.get("timeMs"))
                        .and_then(Value::as_f64)
                        .filter(|t| t.is_finite())
                    {
                        self.ctx.sync_server_time(time)?;
                    }
                    return Ok(None);
                }
                _ => {}
            }
            let worker = self.get_worker();
            if !self.shared {
                if let Some(worker) = &worker {
                    if let (Some(expected), Some(actual)) =
                        (&worker.construction.instance_id, &payload.instance_id)
                    {
                        if !expected.is_empty() && !actual.is_empty() && expected != actual {
                            if !self.has_logged_instance_mismatch.replace(true) {
                                self.ctx.console("warn","[StoreWebWorker] Instance ID mismatch detected; dropping stale messages for this worker lifecycle",Some(json!({"expectedInstanceId":expected,"receivedInstanceId":actual,"payloadOp":payload.op,"account":if self.account_name.borrow().is_empty(){"unknown".into()}else{self.account_name.borrow().clone()},"exchange":self.exchange_name.borrow().as_ref().map(exchange_text).unwrap_or("unknown".into())})))?;
                            }
                            return Ok(None);
                        }
                    }
                }
            }
            match payload.op {
                StoreWorkerOp::Init => return Ok(Some(self.init(payload))),
                StoreWorkerOp::Dispose => {
                    if self.shared {
                        self.module.ports.cleanup_port_everywhere(&self.ctx);
                        let key = self.port_key.borrow().clone();
                        let remaining = !key.is_empty()
                            && self
                                .module
                                .ports
                                .ports
                                .borrow()
                                .get(&key)
                                .is_some_and(|p| !p.borrow().is_empty());
                        if !remaining {
                            if let Some(worker) = worker {
                                self.delete_worker();
                                worker.handle_payload(payload)?;
                            }
                        }
                    } else if let Some(worker) = worker {
                        self.delete_worker();
                        worker.handle_payload(payload)?;
                    }
                }
                StoreWorkerOp::Reinit => {
                    let name = if self.account_name.borrow().is_empty() {
                        self.exchange_name
                            .borrow()
                            .as_ref()
                            .map(exchange_text)
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
                        self.delete_worker();
                        worker.handle_payload(StoreWorkerPayload {
                            op: StoreWorkerOp::Dispose,
                            instance_id: None,
                            data: None,
                        })?;
                    }
                    let key = self.port_key.borrow().clone();
                    if self.shared && !key.is_empty() {
                        let list = self
                            .module
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
                            self.module.ports.cleanup_port_state(&p);
                            i += 1;
                        }
                        self.module
                            .ports
                            .ports
                            .borrow_mut()
                            .insert(key, Rc::default());
                    }
                    self.ctx.console(
                        "log",
                        "[Worker] ✓ Exchange disposed, ready for fresh init",
                        None,
                    )?;
                }
                StoreWorkerOp::SendSnapshot => {
                    if let Some(worker) = worker {
                        worker.send_snapshot()?;
                    }
                }
                _ => {
                    if let Some(worker) = worker {
                        if self.shared {
                            if let Some(sub_id) = payload
                                .data
                                .as_ref()
                                .and_then(|d| d.get("subId"))
                                .and_then(Value::as_str)
                                .filter(|s| !s.is_empty())
                            {
                                self.module
                                    .ports
                                    .ports_by_sub_id
                                    .borrow_mut()
                                    .insert(sub_id.into(), self.ctx.clone());
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
                        let token = original_payload
                            .data
                            .as_ref()
                            .and_then(|d| d.get("telegramBotToken"))
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned);
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
