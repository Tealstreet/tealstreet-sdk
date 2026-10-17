//! Weak bridge from shared websocket lifecycle to a venue's embedded exchange core.
use crate::{
    error_reporting::ExchangeErrorInput,
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeFuture, LogSeverity},
    websocket::{ReopenResync, WsParent},
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::ExchangeOptions;
use serde_json::Value;
use std::rc::{Rc, Weak};

pub struct WsExchangeParent {
    venue: Weak<dyn Exchange>,
    options: crate::exchange::SharedExchangeOptions,
    name: String,
    store: Rc<dyn Store>,
    resync: Rc<ReopenResync>,
}
impl WsExchangeParent {
    pub fn new(venue: &Rc<dyn Exchange>) -> Rc<Self> {
        let base = venue.base();
        let weak = Rc::downgrade(venue);
        *base.venue_weak.borrow_mut() = Some(weak.clone());
        Rc::new(Self {
            venue: weak,
            options: base.options.clone(),
            name: serde_json::to_value(venue.name())
                .expect("exchange name")
                .as_str()
                .expect("name string")
                .into(),
            store: base.store.clone(),
            resync: base.reopen_resync.clone(),
        })
    }
}
impl WsParent for WsExchangeParent {
    fn name(&self) -> &str {
        &self.name
    }
    fn is_disposed(&self) -> bool {
        self.venue
            .upgrade()
            .is_none_or(|venue| venue.base().is_disposed())
    }
    fn options(&self) -> ExchangeOptions {
        self.options.snapshot()
    }
    fn store(&self) -> Rc<dyn Store> {
        self.store.clone()
    }
    fn is_private_socket(&self, candidate: &crate::websocket::BaseWebSocket) -> bool {
        self.venue.upgrade().is_some_and(|venue| {
            venue
                .base()
                .get_private_sockets()
                .iter()
                .any(|socket| std::ptr::eq(socket.as_ref(), candidate))
        })
    }
    fn resync_coordinator(&self) -> Rc<ReopenResync> {
        self.resync.clone()
    }
    fn fetch_and_set_private_snapshot(&self, retries: usize) -> ExchangeFuture<'static, ()> {
        let weak = self.venue.clone();
        Box::pin(async move {
            let Some(venue) = weak.upgrade() else {
                return Ok(());
            };
            if venue.base().is_disposed() {
                return Ok(());
            }
            venue.fetch_and_set_private_snapshot(retries as u32).await
        })
    }
    fn log(&self, message: &str, severity: LogSeverity) -> Result<(), ExchangeError> {
        if let Some(venue) = self.venue.upgrade() {
            venue.base().log(message, severity.as_str())?;
        }
        Ok(())
    }
    fn emit(&self, event: &str, message: &str) -> Result<(), ExchangeError> {
        if let Some(venue) = self.venue.upgrade() {
            venue
                .base()
                .emit(event, crate::js_runtime::value::Js::str(message))?;
        }
        Ok(())
    }
    fn sync_socket_health_to_store(&self) -> Result<(), ExchangeError> {
        if let Some(venue) = self.venue.upgrade() {
            venue.base().sync_socket_health_to_store()?;
        }
        Ok(())
    }
    fn update_latency(&self, latency: f64) -> Result<(), ExchangeError> {
        if !self.is_disposed() {
            self.store.update(
                StoreUpdate {
                    latency: Some(latency),
                    ..Default::default()
                },
                false,
            )?;
        }
        Ok(())
    }
    fn emit_ws_reconnected(&self) -> Result<(), ExchangeError> {
        if let Some(venue) = self.venue.upgrade() {
            venue.base().emit_ws_reconnected()?;
        }
        Ok(())
    }
    fn emit_and_log_error(
        &self,
        error: &str,
        message: Option<&str>,
        extra: Option<&Value>,
    ) -> Result<(), ExchangeError> {
        if let Some(venue) = self.venue.upgrade() {
            venue.base().emit_and_log_error(
                ExchangeErrorInput::Message(error.into()),
                message,
                extra,
            )?;
        }
        Ok(())
    }
}
