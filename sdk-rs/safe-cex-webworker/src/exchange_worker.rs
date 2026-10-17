//! ExchangeWorker construction, store delivery and teardown. Startup/registry,
//! dispatch and event-listener installation are separate pending source bodies.
use crate::{
    chaser_manager::{ChaserOrderManager, ChaserSupervisor},
    payload::{StoreWorkerOp, StoreWorkerPayload, StoreWorkerStorePayload},
    settings::{AppSettingsWorkerType, ExchangeMetadataWorkerType},
    store_payload::StorePayloadPreparer,
    worker_ports::WorkerPortTimers,
};
use safe_cex_exchange::{exchange::Exchange, runtime::ExchangeError};
use safe_cex_store::{Store, store::DefaultStore};
use safe_cex_types::{ExchangeName, ExchangeOptions, StoreData};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub type StoreFrame = Rc<RefCell<StoreData>>;
pub type StartupCancellation = Rc<dyn Fn() -> Result<(), ExchangeError>>;
pub trait WorkerPostprocessor {
    fn process(&self, data: StoreFrame) -> Result<(), ExchangeError>;
}
pub trait WorkerHandler {
    fn dispose(&self) -> Result<(), ExchangeError>;
}
pub type ProcessorList = Rc<RefCell<Vec<Rc<dyn WorkerPostprocessor>>>>;
pub type HandlerList = Rc<RefCell<Vec<Rc<dyn WorkerHandler>>>>;
/// Typed store envelope avoids serializing native nonfinite values into JSON.
pub struct WorkerStoreMessage {
    pub op: StoreWorkerOp,
    pub data: StoreWorkerStorePayload,
}
pub trait ExchangeWorkerHost: WorkerPortTimers + 'static {
    fn post_store(&self, message: WorkerStoreMessage) -> Result<(), ExchangeError>;
    /// Pure, stable bindings to the real native manager and unresolved heavy callees.
    fn chaser_manager(&self) -> Rc<ChaserOrderManager>;
    fn chaser_supervisor(&self) -> Rc<dyn ChaserSupervisor>;
    fn report_timer_error(&self, error: &ExchangeError);
}
pub trait StoreUpdateThrottleHost: WorkerPortTimers + 'static {
    fn report_store_timer_error(&self, error: &ExchangeError);
}
impl<H: ExchangeWorkerHost> StoreUpdateThrottleHost for H {
    fn report_store_timer_error(&self, error: &ExchangeError) {
        self.report_timer_error(error);
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerHandlerKind {
    OHLCV,
    OrderBook,
    Chaser,
    Twap,
    Dev,
    Command,
    UpdatePositions,
    CallMethod,
    UiState,
}
pub struct WorkerConstruction<H> {
    pub ctx: Rc<H>,
    pub account_name: String,
    pub exchange_name: ExchangeName,
    pub options: Rc<RefCell<ExchangeOptions>>,
    pub settings: Rc<RefCell<AppSettingsWorkerType>>,
    pub metadata: Rc<RefCell<ExchangeMetadataWorkerType>>,
    pub session_key: String,
    pub instance_id: Option<String>,
}
struct ThrottleState<T> {
    timer: Option<T>,
    last_args: Option<StoreFrame>,
    last_call: Option<f64>,
    last_invoke: f64,
}
/// lodash throttle with default leading=true, trailing=true, maxWait=wait.
/// Its callback reaches the real worker store_update through Weak, not a second
/// keyed throttle. Binary64 timer delays and wall-clock rollback are preserved.
pub struct StoreUpdateThrottle<H: StoreUpdateThrottleHost> {
    host: Rc<H>,
    pub wait_ms: f64,
    state: RefCell<ThrottleState<H::Timer>>,
    callback: Rc<dyn Fn(StoreFrame) -> Result<(), ExchangeError>>,
}
impl<H: StoreUpdateThrottleHost> StoreUpdateThrottle<H> {
    pub fn new(
        host: Rc<H>,
        wait: f64,
        callback: Rc<dyn Fn(StoreFrame) -> Result<(), ExchangeError>>,
    ) -> Rc<Self> {
        Rc::new(Self {
            host,
            wait_ms: if wait.is_nan() || wait == 0.0 {
                0.0
            } else {
                wait
            },
            state: RefCell::new(ThrottleState {
                timer: None,
                last_args: None,
                last_call: None,
                last_invoke: 0.0,
            }),
            callback,
        })
    }
    fn should_invoke(&self, time: f64) -> bool {
        let s = self.state.borrow();
        s.last_call.is_none_or(|last| {
            time - last >= self.wait_ms || time - last < 0.0 || time - s.last_invoke >= self.wait_ms
        })
    }
    fn set_timer(self: &Rc<Self>, delay: f64) -> H::Timer {
        let weak = Rc::downgrade(self);
        self.host.set_timeout(
            Rc::new(move || {
                if let Some(throttle) = weak.upgrade() {
                    if let Err(error) = throttle.timer_expired() {
                        throttle.host.report_store_timer_error(&error);
                    }
                }
            }),
            delay,
        )
    }
    fn invoke(&self, time: f64) -> Result<(), ExchangeError> {
        let args = {
            let mut s = self.state.borrow_mut();
            let args = s.last_args.take();
            s.last_invoke = time;
            args
        };
        let args = args.ok_or_else(|| {
            ExchangeError::new("Store update arguments cleared before invocation")
        })?;
        (self.callback)(args)
    }
    fn timer_expired(self: &Rc<Self>) -> Result<(), ExchangeError> {
        let time = self.host.now_ms();
        if self.should_invoke(time) {
            self.state.borrow_mut().timer = None;
            let has_args = self.state.borrow().last_args.is_some();
            if has_args {
                return self.invoke(time);
            }
            self.state.borrow_mut().last_args = None;
        } else {
            let remaining = {
                let s = self.state.borrow();
                safe_cex_num::math_min(
                    self.wait_ms - (time - s.last_call.unwrap()),
                    self.wait_ms - (time - s.last_invoke),
                )
            };
            let timer = self.set_timer(remaining);
            self.state.borrow_mut().timer = Some(timer);
        }
        Ok(())
    }
    pub fn call(self: &Rc<Self>, data: StoreFrame) -> Result<(), ExchangeError> {
        let time = self.host.now_ms();
        let invoking = self.should_invoke(time);
        {
            let mut s = self.state.borrow_mut();
            s.last_args = Some(data);
            s.last_call = Some(time);
        }
        let timer = self.state.borrow().timer.clone();
        if invoking {
            if let Some(timer) = timer {
                self.host.clear_timeout(&timer);
                let timer = self.set_timer(self.wait_ms);
                self.state.borrow_mut().timer = Some(timer);
                return self.invoke(time);
            }
            self.state.borrow_mut().last_invoke = time;
            let timer = self.set_timer(self.wait_ms);
            self.state.borrow_mut().timer = Some(timer);
            return self.invoke(time);
        }
        if timer.is_none() {
            let timer = self.set_timer(self.wait_ms);
            self.state.borrow_mut().timer = Some(timer);
        }
        Ok(())
    }
    pub fn cancel(&self) {
        let timer = self.state.borrow().timer.clone();
        if let Some(timer) = timer {
            self.host.clear_timeout(&timer);
        }
        let mut s = self.state.borrow_mut();
        s.last_invoke = 0.0;
        s.last_args = None;
        s.last_call = None;
        s.timer = None;
    }
}
impl<H: StoreUpdateThrottleHost> Drop for StoreUpdateThrottle<H> {
    fn drop(&mut self) {
        if let Some(timer) = self.state.get_mut().timer.take() {
            self.host.clear_timeout(&timer);
        }
    }
}

pub struct ExchangeWorker<H: ExchangeWorkerHost> {
    pub construction: WorkerConstruction<H>,
    store: Rc<dyn Store>,
    pub exchange_handle: RefCell<Option<Rc<dyn Exchange>>>,
    pub throttled_store_update: Rc<StoreUpdateThrottle<H>>,
    pub cancel_startup: RefCell<Option<StartupCancellation>>,
    pub startup_token: Cell<f64>,
    pub message_buffer: Rc<RefCell<Vec<StoreWorkerPayload>>>,
    pub handler_kinds: [WorkerHandlerKind; 9],
    pub handlers: RefCell<HandlerList>,
    pub postprocessors: RefCell<ProcessorList>,
    pub exchange_dependent_postprocessors: RefCell<ProcessorList>,
    preparer: RefCell<StorePayloadPreparer>,
    pub sentry_report_cooldowns: RefCell<std::collections::HashMap<String, (f64, f64)>>,
}
impl<H: ExchangeWorkerHost> ExchangeWorker<H> {
    // ts: constructor. Retain input identity, construct fresh real store and throttle.
    pub fn new(construction: WorkerConstruction<H>) -> Rc<Self> {
        let store: Rc<dyn Store> = Rc::new(DefaultStore::new());
        let multiplier = construction
            .options
            .borrow()
            .tuning
            .as_ref()
            .and_then(|t| t.data_update_throttle_multiplier)
            .filter(|m| *m != 0.0 && !m.is_nan())
            .unwrap_or(1.0);
        let wait = 250.0 * multiplier;
        Rc::new_cyclic(|weak: &std::rc::Weak<Self>| {
            let target = weak.clone();
            let callback = Rc::new(move |data| {
                if let Some(worker) = target.upgrade() {
                    worker.store_update(data, None)
                } else {
                    Ok(())
                }
            });
            Self {
                throttled_store_update: StoreUpdateThrottle::new(
                    construction.ctx.clone(),
                    wait,
                    callback,
                ),
                construction,
                store,
                exchange_handle: RefCell::new(None),
                cancel_startup: RefCell::new(None),
                startup_token: Cell::new(0.0),
                message_buffer: Rc::default(),
                handler_kinds: [
                    WorkerHandlerKind::OHLCV,
                    WorkerHandlerKind::OrderBook,
                    WorkerHandlerKind::Chaser,
                    WorkerHandlerKind::Twap,
                    WorkerHandlerKind::Dev,
                    WorkerHandlerKind::Command,
                    WorkerHandlerKind::UpdatePositions,
                    WorkerHandlerKind::CallMethod,
                    WorkerHandlerKind::UiState,
                ],
                handlers: RefCell::new(Rc::default()),
                postprocessors: RefCell::new(Rc::default()),
                exchange_dependent_postprocessors: RefCell::new(Rc::default()),
                preparer: RefCell::default(),
                sentry_report_cooldowns: RefCell::default(),
            }
        })
    }
    // ts: get store / get exchange. No snapshot or conversion of opaque handles.
    pub fn store(&self) -> Rc<dyn Store> {
        self.store.clone()
    }
    pub fn exchange(&self) -> Option<Rc<dyn Exchange>> {
        self.exchange_handle.borrow().clone()
    }

    // ts: _storeUpdate. Read each current list on every loop iteration.
    pub fn store_update(
        &self,
        data: StoreFrame,
        force_full_store: Option<bool>,
    ) -> Result<(), ExchangeError> {
        let mut index = 0;
        loop {
            let list = self.exchange_dependent_postprocessors.borrow().clone();
            let processor = list.borrow().get(index).cloned();
            let Some(processor) = processor else {
                break;
            };
            processor.process(data.clone())?;
            index += 1;
        }
        index = 0;
        loop {
            let list = self.postprocessors.borrow().clone();
            let processor = list.borrow().get(index).cloned();
            let Some(processor) = processor else {
                break;
            };
            processor.process(data.clone())?;
            index += 1;
        }
        let markets_version = self.store.get_markets_version();
        let tickers_version = self.store.get_tickers_version();
        let hosted = self.construction.options.borrow().hosted_mode == Some(true);
        let prepared = self.preparer.borrow_mut().prepare(
            &data.borrow(),
            markets_version,
            tickers_version,
            hosted,
            force_full_store,
        );
        self.construction.ctx.post_store(WorkerStoreMessage {
            op: StoreWorkerOp::Store,
            data: prepared,
        })
    }
    // ts: sendSnapshot. Cancel before disposed check; no-exchange still sends.
    pub fn send_snapshot(&self) -> Result<(), ExchangeError> {
        self.throttled_store_update.cancel();
        if self
            .exchange()
            .is_some_and(|exchange| exchange.base().is_disposed())
        {
            return Ok(());
        }
        self.store_update(Rc::new(RefCell::new(self.store.serialize())), Some(true))
    }
    // ts: dispose. Re-read live exchange after cancellation and chaser cleanup.
    pub fn dispose(&self) -> Result<(), ExchangeError> {
        self.startup_token.set(self.startup_token.get() + 1.0);
        let cancel = self.cancel_startup.borrow().clone();
        if let Some(cancel) = cancel {
            cancel()?;
        }
        *self.cancel_startup.borrow_mut() = None;
        if let Some(exchange) = self.exchange() {
            self.construction.ctx.chaser_manager().cleanup_exchange(
                exchange,
                None,
                self.construction.ctx.chaser_supervisor(),
            )?;
        }
        if let Some(exchange) = self.exchange() {
            exchange.dispose()?;
        }
        self.clear_runtime_pipeline()
    }
    // ts: _clearRuntimePipeline. forEach retains original array and initial length.
    pub fn clear_runtime_pipeline(&self) -> Result<(), ExchangeError> {
        self.throttled_store_update.cancel();
        let handlers = self.handlers.borrow().clone();
        let length = handlers.borrow().len();
        for index in 0..length {
            let handler = handlers.borrow().get(index).cloned();
            if let Some(handler) = handler {
                handler.dispose()?;
            }
        }
        *self.handlers.borrow_mut() = Rc::default();
        *self.postprocessors.borrow_mut() = Rc::default();
        *self.exchange_dependent_postprocessors.borrow_mut() = Rc::default();
        Ok(())
    }
}
