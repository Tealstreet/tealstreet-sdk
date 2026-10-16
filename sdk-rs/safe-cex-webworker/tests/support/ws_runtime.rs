use super::runtime::Runtime;
use safe_cex_exchange::{
    TransportProvider, TunnelHost,
    runtime::{ExchangeError, ExchangeFuture},
    websocket::{WebSocketConnectionOptions, WsRuntime, WsSocket},
};
use std::{rc::Rc, sync::Arc};
impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        panic!("statistics fixture must never dial transport")
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("statistics fixture must never dial a tunnel")
    }
    fn dial(
        &self,
        _: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        Box::pin(async { Err(ExchangeError::new("statistics fixture cannot dial")) })
    }
    fn headers_capable(&self) -> bool {
        false
    }
    fn enable_reconnecting(&self) -> bool {
        false
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}
