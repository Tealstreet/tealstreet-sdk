//! Parent ownership bridge for the concrete Backpack sockets.
use crate::adapter::BackpackExchange;
use safe_cex_exchange::{
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeFuture, LogSeverity},
    websocket::{BaseWebSocket, ReopenResync, WsParent},
};
use safe_cex_store::Store;
use safe_cex_types::ExchangeOptions;
use std::rc::{Rc, Weak};
pub struct Parent {
    pub owner: Weak<BackpackExchange>,
    pub resync: Rc<ReopenResync>,
    pub store: Rc<dyn Store>,
    pub options: ExchangeOptions,
}
impl Parent {
    pub fn new(owner: &Rc<BackpackExchange>) -> Rc<Self> {
        Rc::new(Self {
            owner: Rc::downgrade(owner),
            resync: Rc::new(ReopenResync::default()),
            store: owner.base.store.clone(),
            options: owner.base.options(),
        })
    }
    fn live(&self) -> Result<Rc<BackpackExchange>, ExchangeError> {
        self.owner
            .upgrade()
            .ok_or_else(|| ExchangeError::new("Backpack parent was disposed"))
    }
}
impl WsParent for Parent {
    fn name(&self) -> &str {
        "backpack"
    }
    fn is_disposed(&self) -> bool {
        self.owner.upgrade().is_none_or(|o| o.base.is_disposed())
    }
    fn options(&self) -> ExchangeOptions {
        self.owner
            .upgrade()
            .map_or_else(|| self.options.clone(), |o| o.base.options())
    }
    fn store(&self) -> Rc<dyn Store> {
        self.store.clone()
    }
    fn is_private_socket(&self, s: &BaseWebSocket) -> bool {
        self.owner
            .upgrade()
            .and_then(|o| o.private_websocket.get().cloned())
            .is_some_and(|p| std::ptr::eq(p.base.as_ref(), s))
    }
    fn resync_coordinator(&self) -> Rc<ReopenResync> {
        self.resync.clone()
    }
    fn fetch_and_set_private_snapshot(&self, tries: usize) -> ExchangeFuture<'static, ()> {
        let owner = self.live();
        Box::pin(async move {
            let owner = owner?;
            owner.fetch_and_set_private_snapshot(tries as u32).await
        })
    }
    fn log(&self, message: &str, severity: LogSeverity) -> Result<(), ExchangeError> {
        self.live()?.base.log(message, severity.as_str())
    }
    fn emit(&self, event: &str, message: &str) -> Result<(), ExchangeError> {
        self.live()?.base.emit(
            event,
            safe_cex_exchange::js_runtime::value::Js::str(message),
        )
    }
    fn sync_socket_health_to_store(&self) -> Result<(), ExchangeError> {
        self.live()?.base.sync_socket_health_to_store()
    }
    fn update_latency(&self, n: f64) -> Result<(), ExchangeError> {
        self.live()?
            .base
            .store
            .update(
                safe_cex_store::StoreUpdate {
                    latency: Some(n),
                    ..Default::default()
                },
                false,
            )
            .map_err(Into::into)
    }
    fn emit_ws_reconnected(&self) -> Result<(), ExchangeError> {
        self.live()?.base.emit_ws_reconnected()
    }
    fn emit_and_log_error(
        &self,
        error: &str,
        message: Option<&str>,
        extra: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), ExchangeError> {
        self.live()?.base.emit_and_log_error(
            safe_cex_exchange::error_reporting::ExchangeErrorInput::Message(error.into()),
            message,
            extra,
        )
    }
}
