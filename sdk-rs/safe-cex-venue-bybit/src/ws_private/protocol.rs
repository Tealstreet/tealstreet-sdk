//! Concrete Bybit socket lifecycle and source private event delivery.
use crate::exchange::BybitExchange;
use safe_cex_exchange::{
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeFuture},
    websocket::{BaseWebSocket, WsProtocol, WsRuntime},
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

type FillTracker =
    Box<dyn FnMut(&str, f64, Option<f64>) -> safe_cex_store::incremental_fills::IncrementalFill>;

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
    pub(super) auth_retries: Cell<u32>,
    pub(super) fills: RefCell<FillTracker>,
}
impl WsProtocol for PrivateProtocol {
    fn dispose(&self) {
        if let Some((_, owner)) = crate::binding::hyro_socket::owner(self) {
            if let Err(error) = owner.call_method("resetSession", vec![]) {
                self.runtime.report_exception(
                    &error.to_exchange_error(),
                    &serde_json::json!({"operation":"HyroTrader private disposal"}),
                );
            }
        }
    }
    fn recovers_after_offline(&self) -> bool {
        !self.is_hyro()
    }
    fn resyncs_private_state_on_reopen(&self) -> Option<bool> {
        self.is_hyro().then_some(false)
    }
    fn recovery_complete(&self) -> bool {
        crate::binding::hyro_socket::owner(self).is_none_or(|(_, owner)| {
            owner
                .get("isRecoveryComplete")
                .is_ok_and(|value| value.truthy())
        })
    }
    fn owns_close_handling(&self) -> bool {
        self.is_hyro()
    }
    fn on_transport_close(
        &self,
        _: &Rc<BaseWebSocket>,
        code: Option<u16>,
        reason: &str,
    ) -> Result<(), ExchangeError> {
        if let Some((_, owner)) = crate::binding::hyro_socket::owner(self) {
            use crate::binding::value::Js;
            let event = Js::object(vec![
                (
                    "currentTarget",
                    owner.get("ws").map_err(|error| error.to_exchange_error())?,
                ),
                (
                    "code",
                    code.map_or(Js::Undefined, |code| Js::Number(f64::from(code))),
                ),
                ("reason", Js::str(reason)),
            ]);
            owner
                .call_method("onClose", vec![event])
                .map_err(|error| error.to_exchange_error())?;
        }
        Ok(())
    }
    fn ping(&self, socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        if let Some((_, owner)) = crate::binding::hyro_socket::owner(self) {
            owner
                .call_method("ping", vec![])
                .map_err(|error| error.to_exchange_error())?;
            return Ok(());
        }
        super::ping(socket)
    }
    fn connect_and_subscribe(
        &self,
        socket: Rc<BaseWebSocket>,
        _: bool,
    ) -> Result<(), ExchangeError> {
        if let Some((_, owner)) = crate::binding::hyro_socket::owner(self) {
            owner
                .call_method("connectAndSubscribe", vec![])
                .map_err(|error| error.to_exchange_error())?;
            return Ok(());
        }
        self.connect_base(socket)
    }
    fn on_message(
        &self,
        socket: &Rc<BaseWebSocket>,
        frame: &safe_cex_transport::socket::SocketFrame,
    ) -> Result<(), ExchangeError> {
        if let Some((_, owner)) = crate::binding::hyro_socket::owner(self) {
            use crate::binding::value::Js;
            let data = match frame {
                safe_cex_transport::socket::SocketFrame::Text(data) => Js::str(data),
                _ => Js::object_with_class("ArrayBuffer", vec![]),
            };
            let event = crate::binding::hyro_socket::event(&owner, data)?;
            owner
                .call_method("onMessage", vec![event])
                .map_err(|error| error.to_exchange_error())?;
            return Ok(());
        }
        self.message(socket, frame)
    }
    fn on_open(&self, socket: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        if let Some((_, owner)) = crate::binding::hyro_socket::owner(self) {
            if let Err(error) = owner.set(
                "ws",
                crate::binding::value::Js::object_with_class("WebSocket", vec![]),
            ) {
                return Box::pin(async move { Err(error.to_exchange_error()) });
            }
        }
        self.open(socket)
    }
}

impl PrivateProtocol {
    fn is_hyro(&self) -> bool {
        self.parent
            .upgrade()
            .is_some_and(|parent| parent.base().name == safe_cex_types::ExchangeName::Hyrotrader)
    }
    pub(crate) fn connect_base(&self, socket: Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
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
            )?),
            parent.base().signers.payload_signer.is_some(),
            self.config.relay_enabled,
        )
    }

    pub(crate) fn new(
        parent: Weak<BybitExchange>,
        runtime: Rc<dyn WsRuntime>,
        config: PrivateProtocolConfig,
    ) -> Self {
        Self {
            parent,
            runtime,
            config,
            auth_retries: Cell::new(0),
            fills: RefCell::new(Box::new(
                safe_cex_store::incremental_fills::create_incremental_fills(500.),
            )),
        }
    }
    pub(crate) fn open(&self, socket: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        if let Some((_, owner)) = crate::binding::hyro_socket::owner(self) {
            let result = (|| {
                owner.call_method("sendReconnectSequence", vec![])?;
                Ok(())
            })()
            .map_err(|error: crate::binding::value::Thrown| error.to_exchange_error());
            return Box::pin(async move { result });
        }
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
