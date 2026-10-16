use safe_cex_exchange::{TransportProvider, TunnelHost, websocket::WsRuntime};
use std::{rc::Rc, sync::Arc};

fn provider(runtime: &dyn WsRuntime) -> Arc<dyn TransportProvider> {
    runtime.transport()
}
fn tunnel(runtime: &dyn WsRuntime) -> Rc<dyn TunnelHost> {
    runtime.tunnel_host()
}

#[test]
fn socket_host_signature_types_are_nameable_from_exchange_core() {
    // Compile the consumer boundary using only the crate defining WsRuntime.
    let _: fn(&dyn WsRuntime) -> Arc<dyn TransportProvider> = provider;
    let _: fn(&dyn WsRuntime) -> Rc<dyn TunnelHost> = tunnel;
}

fn remote_signing(
    exchange: &dyn safe_cex_exchange::exchange::Exchange,
    request: safe_cex_exchange::RemoteSignerRequest,
) -> safe_cex_exchange::runtime::ExchangeFuture<'_, safe_cex_exchange::RemoteSignerResponse> {
    exchange.sign_remote_request(request)
}

#[test]
fn signer_hook_types_are_nameable_from_exchange_core() {
    let _: for<'a> fn(
        &'a dyn safe_cex_exchange::exchange::Exchange,
        safe_cex_exchange::RemoteSignerRequest,
    ) -> safe_cex_exchange::runtime::ExchangeFuture<
        'a,
        safe_cex_exchange::RemoteSignerResponse,
    > = remote_signing;
}
