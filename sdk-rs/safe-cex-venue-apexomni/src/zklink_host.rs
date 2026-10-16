//! Wallet/WASM capabilities retain opaque handle and host-error identity.
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_num::JsJsonValue;
use std::{any::Any, cell::Cell, rc::Rc};

pub type ZkLinkHandle = Rc<dyn Any>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionKind {
    Contract,
    Order,
    Transfer,
    Withdraw,
    ChangePubKey,
}

pub trait ZkLinkHost {
    fn init(&self) -> ExchangeFuture<'_, ()>;
    fn now_ms(&self) -> f64;
    fn pack_amount(&self, amount: &JsJsonValue) -> Result<JsJsonValue, ExchangeError>;
    fn pack_fee(&self, fee: &JsJsonValue) -> Result<JsJsonValue, ExchangeError>;
    fn pubkey_hash(&self) -> Result<JsJsonValue, ExchangeError>;
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
    fn create_signed_order(&self, tx: ZkLinkHandle) -> Result<JsJsonValue, ExchangeError>;
    fn sign_transfer(
        &self,
        tx: ZkLinkHandle,
        symbol: JsJsonValue,
    ) -> ExchangeFuture<'_, JsJsonValue>;
    fn sign_withdraw(
        &self,
        tx: ZkLinkHandle,
        symbol: JsJsonValue,
    ) -> ExchangeFuture<'_, JsJsonValue>;
    fn sign_change_pubkey(&self, tx: ZkLinkHandle, ecdsa: bool) -> ExchangeFuture<'_, JsJsonValue>;
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
