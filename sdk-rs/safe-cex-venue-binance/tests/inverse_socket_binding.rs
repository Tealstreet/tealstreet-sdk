//! Actual public venue constructor must install both maintained socket classes.
#[path = "support/inverse_runtime.rs"]
mod support;
use safe_cex_exchange::socket_lifecycle::SocketSlot;
#[test]
fn public_socket_is_assembled_by_real_constructor() {
    let h = support::fixture(true, false);
    assert!(
        h.exchange.base().get_socket(SocketSlot::Public).is_some(),
        "real inverse constructor must install maintained public socket"
    );
}
#[test]
fn private_socket_is_assembled_by_real_constructor() {
    let h = support::fixture(false, true);
    assert!(
        h.exchange.base().get_socket(SocketSlot::Private).is_some(),
        "real inverse constructor must install maintained private socket"
    );
}
