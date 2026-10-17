//! Wallet/WASM capabilities retain opaque handle and host-error identity.
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_num::JsJsonValue;
use std::{any::Any, cell::Cell, rc::Rc};

pub type ZkLinkHandle = Rc<dyn Any>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionKind {
    Contract,
}

pub trait ZkLinkHost {
    fn init(&self) -> ExchangeFuture<'_, ()>;
    fn builder(
        &self,
        kind: TransactionKind,
        args: Vec<JsJsonValue>,
    ) -> Result<ZkLinkHandle, ExchangeError>;
    fn new_transaction(
        &self,
        kind: TransactionKind,
        builder: ZkLinkHandle,
    ) -> Result<ZkLinkHandle, ExchangeError>;
    fn create_signed_contract(&self, tx: ZkLinkHandle) -> Result<JsJsonValue, ExchangeError>;
}

#[derive(Default)]
pub struct SdkInitialization {
    initialized: Cell<bool>,
}

impl SdkInitialization {
    // ts: initSdk. Keep one shared instance per host SDK module, not per exchange.
    pub async fn init_sdk(&self, host: &dyn ZkLinkHost) -> Result<(), ExchangeError> {
        if !self.initialized.get() {
            host.init().await?;
            self.initialized.set(true);
        }
        Ok(())
    }
}
