//! Remaining worker command handlers. Context and host bindings replace source
//! inheritance/globals; blocked manager functions remain required dependencies.
use crate::{
    chaser::ChaserOrderInstance,
    error_post::WorkerErrorPost,
    handler::BaseHandler,
    payload::{StoreWorkerOp, StoreWorkerPayload},
    settings::{AppSettingsWorkerType, ExchangeMetadataWorkerType},
    task_types::{ChaserOrderOptions, TwapOptions, TwapState, TwapStatus},
    twap_instance::{TwapHost, TwapInstance, UpdateStateCallback},
};
use safe_cex_exchange::{
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_types::ExchangeName;
use serde_json::Value;
use std::{
    cell::RefCell,
    rc::Rc,
    task::{Context, Poll, Waker},
};

/// Hosts retain full lifecycle instances consistently with TwapHost::manager(),
/// and release them on removal/teardown. Lookup must return the current record,
/// including replacement under the same ID. No default manager implementation
/// manufactures partial cancellations or optional fill reconciliation.
pub trait CommandHandlerHost: TwapHost + Sized + 'static {
    fn worker_error_post(&self) -> WorkerErrorPost;
    fn post_payload(&self, payload: StoreWorkerPayload) -> Result<(), ExchangeError>;
    fn cleanup_chaser_exchange(
        &self,
        exchange: Rc<dyn Exchange>,
        reason: &str,
    ) -> Result<(), ExchangeError>;
    fn add_twap(&self, instance: Rc<TwapInstance<Self>>) -> Result<(), ExchangeError>;
    fn get_twap(&self, id: &str) -> Option<Rc<TwapInstance<Self>>>;
}

/// Six inherited BaseHandler constructor inputs, retained as a live context.
/// Release all borrows before invoking handlers; JSON payloads remain owned.
pub struct HandlerContext<H> {
    pub account_name: RefCell<String>,
    pub exchange_name: ExchangeName,
    pub exchange: RefCell<Rc<dyn Exchange>>,
    pub ctx: Rc<H>,
    pub settings: Rc<RefCell<AppSettingsWorkerType>>,
    pub metadata: Rc<RefCell<ExchangeMetadataWorkerType>>,
}
impl<H> HandlerContext<H> {
    pub fn exchange(&self) -> Rc<dyn Exchange> {
        self.exchange.borrow().clone()
    }
}
fn error(error: impl std::fmt::Display) -> ExchangeError {
    ExchangeError::new(error.to_string())
}
fn data(payload: &StoreWorkerPayload) -> Result<&Value, ExchangeError> {
    payload
        .data
        .as_ref()
        .ok_or_else(|| error("Worker command has no data"))
}
fn id(payload: &StoreWorkerPayload) -> Result<&str, ExchangeError> {
    data(payload)?
        .as_str()
        .ok_or_else(|| error("Worker command ID must be a string"))
}
fn detached<H: CommandHandlerHost>(host: &H, future: ExchangeFuture<'static, ()>) {
    host.spawn(future);
}

pub struct ChaserOrderHandler<H: CommandHandlerHost> {
    pub context: Rc<HandlerContext<H>>,
    base: Rc<BaseHandler>,
}
impl<H: CommandHandlerHost> ChaserOrderHandler<H> {
    pub fn new(context: Rc<HandlerContext<H>>) -> Rc<Self> {
        Rc::new_cyclic(|weak: &std::rc::Weak<Self>| {
            let replay = weak.clone();
            let report = context.exchange();
            let base = BaseHandler::new(
                context.exchange(),
                "ChaserOrderHandler".into(),
                context.ctx.worker_error_post(),
                Rc::new(move |p| {
                    if let Some(h) = replay.upgrade() {
                        if let Err(e) = h.handle(&p) {
                            report.base().runtime.report_exception(
                                &e,
                                &serde_json::json!({"handler":"ChaserOrderHandler"}).into(),
                            );
                        }
                    }
                }),
            );
            Self { context, base }
        })
    }
    // ts: ChaserOrderHandler.handle.
    pub fn handle(&self, payload: &StoreWorkerPayload) -> Result<(), ExchangeError> {
        match payload.op {
            StoreWorkerOp::ChaserStart => {
                let mut options: ChaserOrderOptions =
                    serde_json::from_value(data(payload)?.clone()).map_err(error)?;
                if options
                    .account_name
                    .as_ref()
                    .is_none_or(|name| name.is_empty())
                {
                    options.account_name = Some(self.context.account_name.borrow().clone());
                }
                let instance = Rc::new(ChaserOrderInstance::new(
                    self.context.exchange(),
                    Rc::new(RefCell::new(options)),
                ));
                self.context.ctx.add_chaser(instance)?;
            }
            StoreWorkerOp::ChaserTerminate => self.context.ctx.remove_chaser(id(payload)?)?,
            StoreWorkerOp::ChaserTerminateAll => self.context.ctx.cleanup_chaser_exchange(
                self.context.exchange(),
                "Client disconnected while chase was active",
            )?,
            _ => {}
        }
        Ok(())
    }
    pub fn buffer_until_markets_loaded(&self, payload: StoreWorkerPayload) -> bool {
        self.base.buffer_until_markets_loaded(payload)
    }
    pub fn dispose(&self) {
        self.base.dispose();
    }
}
impl<H: CommandHandlerHost> Drop for ChaserOrderHandler<H> {
    fn drop(&mut self) {
        self.base.dispose();
    }
}

pub struct TwapHandler<H: CommandHandlerHost> {
    pub context: Rc<HandlerContext<H>>,
    base: Rc<BaseHandler>,
}
impl<H: CommandHandlerHost> TwapHandler<H> {
    pub fn new(context: Rc<HandlerContext<H>>) -> Rc<Self> {
        Rc::new_cyclic(|weak: &std::rc::Weak<Self>| {
            let replay = weak.clone();
            let report = context.exchange();
            let base = BaseHandler::new(
                context.exchange(),
                "TwapHandler".into(),
                context.ctx.worker_error_post(),
                Rc::new(move |p| {
                    if let Some(h) = replay.upgrade() {
                        if let Err(e) = h.handle(&p) {
                            report.base().runtime.report_exception(
                                &e,
                                &serde_json::json!({"handler":"TwapHandler"}).into(),
                            );
                        }
                    }
                }),
            );
            Self { context, base }
        })
    }
    fn callback(&self) -> UpdateStateCallback<H> {
        let host = self.context.ctx.clone();
        Rc::new(move |_, _| {
            host.runtime().console("log", "[TWAP] twap state updated");
            Ok(())
        })
    }
    fn current(&self, id: &str) -> Option<Rc<TwapInstance<H>>> {
        let instance = self.context.ctx.get_twap(id)?;
        let record = self.context.ctx.manager().get_by_id(id)?;
        Rc::ptr_eq(&record, &instance.managed).then_some(instance)
    }
    // ts: TwapHandler.handle.
    pub fn handle(&self, payload: &StoreWorkerPayload) -> Result<(), ExchangeError> {
        let host = &self.context.ctx;
        match payload.op {
            StoreWorkerOp::TwapStart => {
                let options: TwapOptions =
                    serde_json::from_value(data(payload)?.clone()).map_err(error)?;
                let instance = TwapInstance::new(
                    self.context.exchange(),
                    host.clone(),
                    Rc::new(RefCell::new(options)),
                    self.callback(),
                    None,
                    None,
                )?;
                host.add_twap(instance.clone())?;
                let started = instance.start();
                detached(
                    host.as_ref(),
                    Box::pin(async move { started.await.map(|_| ()) }),
                );
            }
            StoreWorkerOp::TwapResume
            | StoreWorkerOp::TwapCleanup
            | StoreWorkerOp::TwapPause
            | StoreWorkerOp::TwapStop => {
                if let Some(instance) = self.current(id(payload)?) {
                    let future = match payload.op {
                        StoreWorkerOp::TwapResume => instance.resume(),
                        StoreWorkerOp::TwapCleanup => instance.cleanup(),
                        StoreWorkerOp::TwapPause => instance.pause(),
                        StoreWorkerOp::TwapStop => instance.terminate(),
                        _ => unreachable!(),
                    };
                    detached(host.as_ref(), future);
                }
            }
            StoreWorkerOp::TwapState => {
                let restored = (|| {
                    let values = data(payload)?
                        .as_array()
                        .ok_or_else(|| error("Persisted TWAP states must be an array"))?;
                    for value in values {
                        let id = value
                            .get("id")
                            .and_then(Value::as_str)
                            .ok_or_else(|| error("Persisted TWAP ID must be a string"))?;
                        // Duplicate IDs skip option/state decoding, like source own records.
                        if host.manager().has_id(id) {
                            continue;
                        }
                        let options: TwapOptions = serde_json::from_value(
                            value
                                .get("options")
                                .cloned()
                                .ok_or_else(|| error("Persisted TWAP has no options"))?,
                        )
                        .map_err(error)?;
                        let state: Option<Rc<RefCell<TwapState>>> = value
                            .get("state")
                            .map(|v| {
                                serde_json::from_value(v.clone()).map(|s| Rc::new(RefCell::new(s)))
                            })
                            .transpose()
                            .map_err(error)?;
                        let instance = TwapInstance::new(
                            self.context.exchange(),
                            host.clone(),
                            Rc::new(RefCell::new(options)),
                            self.callback(),
                            state.clone(),
                            Some(id.into()),
                        )?;
                        host.add_twap(instance.clone())?;
                        // Source checks the supplied state after add; add may replace instance.state.
                        let supplied = state.ok_or_else(|| error("Persisted TWAP has no state"))?;
                        if supplied.borrow().status == TwapStatus::Running {
                            let paused = instance.pause();
                            detached(host.as_ref(), paused);
                            let resumed = instance.resume();
                            detached(host.as_ref(), resumed);
                        }
                    }
                    Ok::<(), ExchangeError>(())
                })();
                if let Err(error) = restored {
                    host.runtime().console("error", &error.message);
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub fn buffer_until_markets_loaded(&self, payload: StoreWorkerPayload) -> bool {
        self.base.buffer_until_markets_loaded(payload)
    }
    pub fn dispose(&self) {
        self.base.dispose();
    }
}
impl<H: CommandHandlerHost> Drop for TwapHandler<H> {
    fn drop(&mut self) {
        self.base.dispose();
    }
}

pub struct UiStateHandler<H: CommandHandlerHost> {
    pub context: Rc<HandlerContext<H>>,
    base: Rc<BaseHandler>,
}
impl<H: CommandHandlerHost> UiStateHandler<H> {
    pub fn new(context: Rc<HandlerContext<H>>) -> Rc<Self> {
        Rc::new_cyclic(|weak: &std::rc::Weak<Self>| {
            let replay = weak.clone();
            let report = context.exchange();
            let base = BaseHandler::new(
                context.exchange(),
                "UiStateHandler".into(),
                context.ctx.worker_error_post(),
                Rc::new(move |p| {
                    if let Some(h) = replay.upgrade() {
                        if let Err(e) = h.handle(&p) {
                            report.base().runtime.report_exception(
                                &e,
                                &serde_json::json!({"handler":"UiStateHandler"}).into(),
                            );
                        }
                    }
                }),
            );
            Self { context, base }
        })
    }
    // ts: UiStateHandler.handle. Active-symbol throw prevents margin work.
    pub fn handle(&self, payload: &StoreWorkerPayload) -> Result<(), ExchangeError> {
        if payload.op != StoreWorkerOp::UiState {
            return Ok(());
        }
        crate::ui_state::handle_active_symbols_change(
            self.context.exchange().as_ref(),
            payload.data.as_ref(),
        )?;
        let enabled = self
            .context
            .metadata
            .borrow()
            .fetch_leverage_on_margin_symbol_change
            .unwrap_or(false);
        let symbol = payload
            .data
            .as_ref()
            .and_then(|d| d.get("activeMarginSymbol"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let exchange = self.context.exchange();
        let mut future: ExchangeFuture<'static, ()> = Box::pin(async move {
            crate::ui_state::handle_active_margin_symbol_change(
                exchange.as_ref(),
                enabled,
                symbol.as_deref(),
            )
            .await
        });
        // Execute the helper's prefix before return. Immediate native fulfillment can
        // settle now rather than at a JS microtask; pending futures are host-owned.
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(result) => result?,
            Poll::Pending => detached(self.context.ctx.as_ref(), future),
        }
        Ok(())
    }
    pub fn buffer_until_markets_loaded(&self, payload: StoreWorkerPayload) -> bool {
        self.base.buffer_until_markets_loaded(payload)
    }
    pub fn dispose(&self) {
        self.base.dispose();
    }
}
impl<H: CommandHandlerHost> Drop for UiStateHandler<H> {
    fn drop(&mut self) {
        self.base.dispose();
    }
}
