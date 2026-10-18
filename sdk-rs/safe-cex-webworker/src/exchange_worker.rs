//! ExchangeWorker construction, store delivery and teardown. Startup/registry,
//! dispatch and event-listener installation are separate pending source bodies.
use crate::chaser_manager::ChaserOrderManager;
use crate::chaser_manager::ChaserSupervisor;
use crate::payload::StoreWorkerOp;
use crate::payload::StoreWorkerPayload;
use crate::payload::StoreWorkerStorePayload;
use crate::settings::AppSettingsWorkerType;
use crate::settings::ExchangeMetadataWorkerType;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::store_delivery::StoreFrame;
use safe_cex_exchange::store_delivery::StoreUpdateThrottle;
use safe_cex_exchange::store_delivery::StoreUpdateThrottleHost;
use safe_cex_exchange::store_payload::StorePayloadPreparer;
use safe_cex_exchange::{exchange::Exchange, runtime::ExchangeError};
use safe_cex_store::Store;
use safe_cex_types::{ExchangeName, ExchangeOptions};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

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
pub trait ExchangeWorkerHost: StoreUpdateThrottleHost + 'static {
    fn post_store(&self, message: WorkerStoreMessage) -> Result<(), ExchangeError>;
    /// Pure, stable bindings to the real native manager and unresolved heavy callees.
    fn chaser_manager(&self) -> Rc<ChaserOrderManager>;
    fn chaser_supervisor(&self) -> Rc<dyn ChaserSupervisor>;
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
        self.store_update(Rc::new(RefCell::new(self.store.serialize()?)), Some(true))
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
