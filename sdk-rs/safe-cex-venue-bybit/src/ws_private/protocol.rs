//! Concrete linear Bybit socket lifecycle. Message dispatch is still being wired.
use crate::exchange::BybitExchange;
use safe_cex_exchange::{
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeFuture},
    websocket::{BaseWebSocket, WsProtocol, WsRuntime},
};
use std::rc::{Rc, Weak};

#[derive(Clone)]
pub(crate) struct PrivateProtocolConfig {
    /// Invoke the clock's synchronous prefix before returning its async tail.
    pub sync_clock: Rc<dyn Fn() -> ExchangeFuture<'static, ()>>,
    pub now_ms: Rc<dyn Fn() -> f64>,
    pub account_name: Option<String>,
    pub relay_enabled: bool,
}

pub(crate) struct PrivateProtocol {
    pub parent: Weak<BybitExchange>,
    pub runtime: Rc<dyn WsRuntime>,
    pub config: PrivateProtocolConfig,
}
impl WsProtocol for PrivateProtocol {
    fn ping(&self, socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        super::ping(socket)
    }
    fn connect_and_subscribe(
        &self,
        socket: Rc<BaseWebSocket>,
        _: bool,
    ) -> Result<(), ExchangeError> {
        let Some(parent) = self.parent.upgrade() else {
            return Ok(());
        };
        super::connect_and_subscribe(
            &socket,
            self.runtime.as_ref(),
            parent.uses_bybit_demo_environment(),
            Some(parent.get_bybit_private_ws_url(
                if parent.base().options().testnet == Some(true) {
                    crate::exchange::Network::Testnet
                } else {
                    crate::exchange::Network::Livenet
                },
            )),
            parent.base().signers.payload_signer.is_some(),
            self.config.relay_enabled,
        )
    }
    fn on_open(&self, socket: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        self.open(socket)
    }
}
impl PrivateProtocol {
    pub(crate) fn open(&self, socket: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        let clock = self.config.sync_clock.clone();
        let now = self.config.now_ms.clone();
        let account_name = self.config.account_name.clone();
        let parent = self.parent.clone();
        let auth_socket = socket.clone();
        // The TS onOpen starts the sequence but does not return its promise.
        super::send_reconnect_sequence(
            &socket,
            self.runtime.clone(),
            move || clock(),
            move || {
                Box::pin(async move {
                    let parent = parent.upgrade().ok_or_else(|| {
                        ExchangeError::new("Bybit parent disposed before authentication")
                    })?;
                    super::local_auth::auth(&auth_socket, parent.base(), account_name, now()).await
                })
            },
            self.runtime.enable_reconnecting(),
            false,
        );
        Box::pin(async { Ok(()) })
    }
}
