//! ExchangeWorker startup, live dispatch, constructor registry and event bridges.
//! Dynamic venue imports and heterogeneous constructors are explicit callable
//! dependencies; none of the worker method bodies is delegated to a success hook.
use crate::data_post::{WorkerDataPayload, WorkerDataPost};
use crate::{
    exchange_worker::{
        ExchangeWorker, ExchangeWorkerHost, WorkerConstruction, WorkerHandler, WorkerHandlerKind,
        WorkerPostprocessor,
    },
    payload::{StoreWorkerOp, StoreWorkerPayload},
    postprocessor_registry::PostprocessorRegistry,
    worker_logging::{self, WorkerLogLevel},
};
use safe_cex_exchange::js_runtime::value::Js;
use safe_cex_exchange::{
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, ExchangeOptions, StoreSubscriberError};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    task::Waker,
};

/// Install the source fill and variadic log bridge on a production exchange.
/// The post callback is the worker port; no event payload is interpreted as args.
pub fn install_exchange_event_bridge(
    base: &safe_cex_exchange::exchange::BaseExchange,
    post: WorkerDataPost,
) {
    let fill_post = post.clone();
    base.on(
        "fill",
        Rc::new(move |_, data| {
            fill_post.send(WorkerDataPayload {
                op: StoreWorkerOp::Fill,
                data: data.clone(),
            })
        }),
    );
    let logs = [
        ("error", StoreWorkerOp::Error),
        ("warning", StoreWorkerOp::Warning),
        ("log", StoreWorkerOp::Log),
        ("exec-log", StoreWorkerOp::ExecLog),
        ("exec-result", StoreWorkerOp::ExecResult),
        ("chaser:complete", StoreWorkerOp::ChaserComplete),
        ("grafana-info", StoreWorkerOp::GrafanaInfo),
        ("grafana-error", StoreWorkerOp::GrafanaError),
        ("grafana-pws", StoreWorkerOp::GrafanaPws),
        ("grafana-pws-sub", StoreWorkerOp::GrafanaPwsSub),
        ("grafana-http-request", StoreWorkerOp::GrafanaHttpRequest),
        ("grafana-http-response", StoreWorkerOp::GrafanaHttpResponse),
    ];
    for (name, op) in logs {
        let post = post.clone();
        base.on_args(
            name,
            Rc::new(move |_, args| {
                post.send(WorkerDataPayload {
                    op,
                    data: Js::array(args.to_vec()),
                })
            }),
        );
    }
}

pub struct ProcessorArguments {
    pub account_name: String,
    pub exchange_name: ExchangeName,
    pub options: Rc<RefCell<ExchangeOptions>>,
}
pub trait ProcessorConstructor {
    fn construct(
        &self,
        arguments: ProcessorArguments,
        exchange: Option<Rc<dyn Exchange>>,
    ) -> Result<Rc<dyn WorkerPostprocessor>, ExchangeError>;
}
pub type WorkerProcessorRegistry =
    PostprocessorRegistry<dyn ProcessorConstructor, dyn ProcessorConstructor>;
pub trait WorkerExchangeConstructor {
    fn construct(
        &self,
        name: ExchangeName,
        options: Rc<RefCell<ExchangeOptions>>,
        store: Rc<dyn Store>,
    ) -> Result<Rc<dyn Exchange>, ExchangeError>;
}
pub trait WorkerBootstrapHost: ExchangeWorkerHost {
    fn construct_clock_start(&self) -> ExchangeFuture<'static, ()>;
    fn load_exchange(
        &self,
        name: ExchangeName,
    ) -> Option<ExchangeFuture<'static, Rc<dyn WorkerExchangeConstructor>>>;
    fn processor_registry(&self) -> Rc<WorkerProcessorRegistry>;
    fn construct_handler(
        &self,
        kind: WorkerHandlerKind,
        construction: &WorkerConstruction<Self>,
        exchange: Rc<dyn Exchange>,
    ) -> Result<Rc<dyn WorkerHandler>, ExchangeError>
    where
        Self: Sized;
    /// Invoke the actual opaque handler created above, including extension handlers.
    fn invoke_handler(
        &self,
        handler: Rc<dyn WorkerHandler>,
        payload: StoreWorkerPayload,
    ) -> Result<(), ExchangeError>;
    fn post_payload(&self, payload: StoreWorkerPayload) -> Result<(), ExchangeError>;
    fn post_event(&self, payload: WorkerDataPayload) -> Result<(), ExchangeError>;
    /// Enqueue and drive the whole future; dropping it on a lost race is forbidden.
    fn construct_exchange_start(&self, exchange: Rc<dyn Exchange>) -> ExchangeFuture<'static, ()>;
    fn error_properties(&self, error: &ExchangeError) -> Value;
    fn spawn_start_task(&self, task: ExchangeFuture<'static, ()>);
    fn materialize_error(&self, error: &ExchangeError) -> Result<Value, ExchangeError>;
    fn console_error_fallback(
        &self,
        message: &str,
        error: &ExchangeError,
        post_error: ExchangeError,
    ) -> Result<(), ExchangeError>;
    fn console_suppressed_start_error(
        &self,
        name: ExchangeName,
        error: &ExchangeError,
    ) -> Result<(), ExchangeError>;
}
#[derive(Default)]
struct Signal {
    cancelled: Cell<bool>,
    race: RefCell<Option<Weak<StartRace>>>,
    waker: RefCell<Option<Waker>>,
}
impl Signal {
    fn wake(&self) {
        let w = self.waker.borrow_mut().take();
        if let Some(w) = w {
            w.wake();
        }
    }
}
#[derive(Default)]
struct StartRace {
    settled: Cell<bool>,
    result: RefCell<Option<Result<bool, ExchangeError>>>,
    waker: RefCell<Option<Waker>>,
}
impl StartRace {
    fn settle(&self, r: Result<bool, ExchangeError>) {
        let should = !self.settled.replace(true);
        if should {
            *self.result.borrow_mut() = Some(r);
            let w = self.waker.borrow_mut().take();
            if let Some(w) = w {
                w.wake();
            }
        }
    }
}
impl<H: WorkerBootstrapHost> ExchangeWorker<H> {
    pub fn log_message(
        &self,
        level: WorkerLogLevel,
        message: &str,
        context: Option<&serde_json::Map<String, Value>>,
    ) -> Result<(), ExchangeError> {
        worker_logging::forward_log(
            self.construction.exchange_name.clone(),
            &self.construction.account_name,
            level,
            message,
            context,
            |p| self.construction.ctx.post_payload(p),
        )
    }
    pub fn report_error(
        &self,
        message: &str,
        error: &ExchangeError,
        extra: Option<&Value>,
        sentry: bool,
    ) -> Result<(), ExchangeError> {
        let h = &self.construction.ctx;
        let raw = h.error_properties(error);
        worker_logging::forward_error(
            &self.sentry_report_cooldowns,
            (
                self.construction.exchange_name.clone(),
                &self.construction.account_name,
            ),
            message,
            &raw,
            extra,
            sentry,
            (
                || Ok(h.now_ms()),
                || h.materialize_error(error),
                |p| h.post_payload(p),
                |_, _, _, post_error| h.console_error_fallback(message, error, post_error),
            ),
        )
    }
    // ts: start. Cancellation installation and clock construction are synchronous.
    pub fn start(self: &Rc<Self>) -> ExchangeFuture<'static, bool> {
        let token = self.startup_token.get();
        let signal = Rc::new(Signal::default());
        let notify = signal.clone();
        *self.cancel_startup.borrow_mut() = Some(Rc::new(move || {
            notify.cancelled.set(true);
            let race = notify.race.borrow().as_ref().and_then(Weak::upgrade);
            if let Some(race) = race {
                race.settle(Ok(false));
            }
            notify.wake();
            Ok(())
        }));
        let clock = self.construction.ctx.construct_clock_start();
        let worker = self.clone();
        Box::pin(async move {
            let result: Result<bool, ExchangeError> = async {
                clock.await?;
                if token != worker.startup_token.get() {
                    return Ok(false);
                }
                let name = worker.construction.exchange_name.clone();
                let loader = worker
                    .construction
                    .ctx
                    .load_exchange(name.clone())
                    .ok_or_else(|| {
                        ExchangeError::new(format!("Unknown exchange: {}", exchange_text(&name)))
                    })?;
                let constructor = loader.await?;
                if token != worker.startup_token.get() {
                    return Ok(false);
                }
                let exchange = constructor.construct(
                    name.clone(),
                    worker.construction.options.clone(),
                    worker.store(),
                )?;
                *worker.exchange_handle.borrow_mut() = Some(exchange);
                if worker.construction.options.borrow().private_only == Some(true) {
                    worker
                        .exchange()
                        .unwrap()
                        .base()
                        .disable_public_websockets();
                }
                worker.initialize_processors_and_handlers()?;
                worker.setup_exchange_event_listeners()?;
                worker.log_message(
                    WorkerLogLevel::Info,
                    &format!("Starting exchange in webworker: {}", exchange_text(&name)),
                    None,
                )?;
                let race = Rc::new(StartRace::default());
                *signal.race.borrow_mut() = Some(Rc::downgrade(&race));
                if signal.cancelled.get() {
                    race.settle(Ok(false));
                }
                let completion = race.clone();
                let exchange = worker
                    .exchange()
                    .ok_or_else(|| ExchangeError::new("Exchange cleared before startup"))?;
                let starting = worker.construction.ctx.construct_exchange_start(exchange);
                worker
                    .construction
                    .ctx
                    .spawn_start_task(Box::pin(async move {
                        let r = starting.await;
                        completion.settle(r.map(|_| true));
                        Ok(())
                    }));
                let timeout = race.clone();
                worker.construction.ctx.set_timeout(
                    Rc::new(move || {
                        timeout.settle(Err(ExchangeError::new(
                            "Exchange start timed out after 30s",
                        )))
                    }),
                    30_000.,
                );
                let race_result = std::future::poll_fn(|cx| {
                    if let Some(r) = race.result.borrow_mut().take() {
                        return std::task::Poll::Ready(r);
                    }
                    if signal.cancelled.get() {
                        return std::task::Poll::Ready(Ok(false));
                    }
                    *race.waker.borrow_mut() = Some(cx.waker().clone());
                    *signal.waker.borrow_mut() = Some(cx.waker().clone());
                    std::task::Poll::Pending
                })
                .await?;
                if !race_result
                    || token != worker.startup_token.get()
                    || worker.exchange().is_some_and(|e| e.base().is_disposed())
                {
                    return Ok(false);
                }
                crate::worker_buffer::flush_buffered_messages(
                    &worker.message_buffer,
                    |m, c| worker.log_message(WorkerLogLevel::Info, m, c.as_object()),
                    |p| worker.handle_payload(p.clone()),
                    |e: &ExchangeError| Ok(e.js_error_string()),
                    |m, e, c| worker.report_error(m, &e, Some(c), true),
                )?;
                if token != worker.startup_token.get()
                    || worker.exchange().is_some_and(|e| e.base().is_disposed())
                {
                    return Ok(false);
                }
                worker.throttled_store_update.cancel();
                worker.store_update(
                    Rc::new(RefCell::new(worker.store().serialize()?)),
                    Some(true),
                )?;
                Ok(true)
            }
            .await;
            let result = match result {
                Ok(v) => Ok(v),
                Err(error) => {
                    if token != worker.startup_token.get()
                        || worker.exchange().is_some_and(|e| e.base().is_disposed())
                    {
                        worker
                            .construction
                            .ctx
                            .console_suppressed_start_error(
                                worker.construction.exchange_name.clone(),
                                &error,
                            )
                            .map(|_| false)
                    } else {
                        let detail = if error.message.is_empty() {
                            String::new()
                        } else {
                            format!(": {}", error.message)
                        };
                        worker
                            .report_error(
                                &format!(
                                    "Error starting exchange: {}{detail}",
                                    exchange_text(&worker.construction.exchange_name)
                                ),
                                &error,
                                Some(&json!({"sentryTags":{"worker_error_phase":"startup"}})),
                                true,
                            )
                            .map(|_| false)
                    }
                }
            };
            if token == worker.startup_token.get() {
                *worker.cancel_startup.borrow_mut() = None;
            }
            result
        })
    }
    // ts: handlePayload. Read live handler list again on every iteration.
    pub fn handle_payload(
        self: &Rc<Self>,
        payload: StoreWorkerPayload,
    ) -> Result<(), ExchangeError> {
        if self.exchange().is_none() {
            let count = self.message_buffer.borrow().len();
            if count < 1000 {
                self.message_buffer.borrow_mut().push(payload);
            } else {
                self.report_error(&format!("Message buffer full for {}, dropping message",exchange_text(&self.construction.exchange_name)),&ExchangeError::new("Message buffer overflow"),Some(&json!({"droppedPayload":payload.op,"maxBufferSize":1000,"bufferSize":count})),false)?;
            }
            return Ok(());
        }
        match payload.op {
            StoreWorkerOp::Store => {
                let update = decode_update(payload.data.unwrap_or(Value::Null))?;
                self.exchange()
                    .unwrap()
                    .base()
                    .store
                    .update(update, false)?;
            }
            StoreWorkerOp::MergeLoaded => {
                if payload
                    .data
                    .as_ref()
                    .is_some_and(|v| safe_cex_num::js_truthy(Some(v)))
                {
                    if let Some(e) = self.exchange() {
                        let mut loaded = serde_json::to_value(e.base().store.loaded()?.clone())
                            .map_err(|e| ExchangeError::new(e.to_string()))?;
                        if let (Some(a), Some(b)) = (
                            loaded.as_object_mut(),
                            payload.data.as_ref().and_then(Value::as_object),
                        ) {
                            a.extend(b.clone());
                        }
                        let loaded = serde_json::from_value(loaded)
                            .map_err(|e| ExchangeError::new(e.to_string()))?;
                        e.base().store.update(
                            StoreUpdate {
                                loaded: Some(loaded),
                                ..Default::default()
                            },
                            false,
                        )?;
                    }
                }
            }
            StoreWorkerOp::MergeMessageHandlerStats => {
                if payload
                    .data
                    .as_ref()
                    .is_some_and(|v| safe_cex_num::js_truthy(Some(v)))
                {
                    if let Some(e) = self.exchange() {
                        let mut stats = e
                            .base()
                            .store
                            .message_handler_stats()?
                            .map(|v| v.clone())
                            .unwrap_or(json!({}));
                        if !stats.is_object() {
                            stats = json!({});
                        }
                        if let Some(b) = payload.data.as_ref().and_then(Value::as_object) {
                            stats.as_object_mut().unwrap().extend(b.clone());
                        }
                        safe_cex_num::normalize_json_order(&mut stats);
                        e.base().store.update(
                            StoreUpdate {
                                message_handler_stats: Some(stats),
                                ..Default::default()
                            },
                            false,
                        )?;
                    }
                }
            }
            StoreWorkerOp::Dispose => self.dispose()?,
            StoreWorkerOp::Deactivate => {
                self.log_message(
                    WorkerLogLevel::Info,
                    &format!(
                        "Deactivating exchange: {}",
                        exchange_text(&self.construction.exchange_name)
                    ),
                    None,
                )?;
                self.deactivate()?;
            }
            StoreWorkerOp::SayHi => {}
            _ => {
                let mut i = 0;
                loop {
                    let list = self.handlers.borrow().clone();
                    let handler = list.borrow().get(i).cloned();
                    let Some(handler) = handler else { break };
                    self.construction
                        .ctx
                        .invoke_handler(handler, payload.clone())?;
                    i += 1;
                }
            }
        }
        Ok(())
    }
    fn deactivate(&self) -> Result<(), ExchangeError> {
        self.dispose()?;
        *self.exchange_handle.borrow_mut() = None;
        self.clear_runtime_pipeline()?;
        self.construction.ctx.post_payload(StoreWorkerPayload {
            op: StoreWorkerOp::DeactivateStatus,
            instance_id: None,
            data: Some(Value::Bool(true)),
        })
    }
    // ts: _initializeProcessorsAndHandlers. map captures list identity/initial length.
    pub fn initialize_processors_and_handlers(&self) -> Result<(), ExchangeError> {
        let registry = self.construction.ctx.processor_registry();
        let map = |classes: Rc<RefCell<Vec<Rc<dyn ProcessorConstructor>>>>, dependent: bool| {
            let len = classes.borrow().len();
            let mut out = vec![];
            for i in 0..len {
                let c = classes.borrow().get(i).cloned().ok_or_else(|| {
                    ExchangeError::new("Sparse processor registry is unsupported")
                })?;
                let arguments = ProcessorArguments {
                    account_name: self.construction.account_name.clone(),
                    exchange_name: self.construction.exchange_name.clone(),
                    options: self.construction.options.clone(),
                };
                out.push(c.construct(arguments, if dependent { self.exchange() } else { None })?);
            }
            Ok::<_, ExchangeError>(Rc::new(RefCell::new(out)))
        };
        if self.construction.options.borrow().hosted_mode != Some(true) {
            let list = map(registry.independent_postprocessors.clone(), false)?;
            *self.postprocessors.borrow_mut() = list;
        }
        let list = map(registry.exchange_dependent_postprocessors.clone(), true)?;
        *self.exchange_dependent_postprocessors.borrow_mut() = list;
        let mut handlers = vec![];
        for kind in self.handler_kinds {
            let exchange = self.exchange().ok_or_else(|| {
                ExchangeError::new("Exchange cleared during handler construction")
            })?;
            handlers.push(self.construction.ctx.construct_handler(
                kind,
                &self.construction,
                exchange,
            )?);
        }
        *self.handlers.borrow_mut() = Rc::new(RefCell::new(handlers));
        Ok(())
    }
    // ts: _setupExchangeEventListeners. Each registration re-reads the live exchange.
    pub fn setup_exchange_event_listeners(self: &Rc<Self>) -> Result<(), ExchangeError> {
        let worker = Rc::downgrade(self);
        let _unsubscribe = self
            .exchange()
            .ok_or_else(|| ExchangeError::new("No exchange for store listener"))?
            .base()
            .store
            .subscribe(Box::new(move |data: &safe_cex_types::StoreData, _| {
                if let Some(w) = worker.upgrade() {
                    w.throttled_store_update
                        .call(Rc::new(RefCell::new(data.clone())))
                        .map_err(StoreSubscriberError::from)?;
                }
                Ok(())
            }));
        let worker = Rc::downgrade(self);
        install_exchange_event_bridge(
            self.exchange()
                .ok_or_else(|| ExchangeError::new("No exchange for event bridge"))?
                .base(),
            WorkerDataPost::new(Rc::new(move |payload| {
                if let Some(w) = worker.upgrade() {
                    w.construction.ctx.post_event(payload)?;
                }
                Ok(())
            })),
        );
        let worker = Rc::downgrade(self);
        self.exchange()
            .ok_or_else(|| ExchangeError::new("No reconnect exchange"))?
            .base()
            .on(
                "ws-reconnected",
                Rc::new(move |_, _| {
                    if let Some(w) = worker.upgrade() {
                        w.construction.ctx.post_payload(StoreWorkerPayload {
                            op: StoreWorkerOp::WsReconnected,
                            instance_id: None,
                            data: None,
                        })?;
                    }
                    Ok(())
                }),
            );
        let worker = Rc::downgrade(self);
        self.exchange()
            .ok_or_else(|| ExchangeError::new("No deactivate exchange"))?
            .base()
            .on(
                "deactivate",
                Rc::new(move |_, _| {
                    if let Some(w) = worker.upgrade() {
                        w.log_message(
                            WorkerLogLevel::Info,
                            &format!(
                                "Exchange emitted deactivate event: {}",
                                exchange_text(&w.construction.exchange_name)
                            ),
                            None,
                        )?;
                        w.deactivate()?;
                    }
                    Ok(())
                }),
            );
        Ok(())
    }
}
pub fn decode_update(data: Value) -> Result<StoreUpdate, ExchangeError> {
    serde_json::from_value(data).map_err(|e| ExchangeError::new(e.to_string()))
}

pub fn exchange_text(name: &ExchangeName) -> String {
    serde_json::to_value(name)
        .expect("exchange enum serialization")
        .as_str()
        .expect("exchange enum string")
        .to_owned()
}
