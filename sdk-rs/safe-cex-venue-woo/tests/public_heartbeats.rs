#[path = "support/ws.rs"]
mod support;

use safe_cex_exchange::websocket::BaseWebSocket;
use std::rc::Rc;
use support::*;

#[test]
fn pong_propagates_parent_latency_failure() {
    let runtime = Rc::new(Runtime::default());
    let parent = Parent::new();
    let socket = BaseWebSocket::new(1, parent.clone(), runtime.clone(), Rc::new(Protocol));
    let mut cause = safe_cex_exchange::runtime::ExchangeError::new("latency listener failed");
    cause.name = "LatencyNotificationError".into();
    cause.code = Some(safe_cex_exchange::js_runtime::value::Js::Number(732.0));
    *parent.hook_failure.borrow_mut() = Some(("latency", cause.clone()));
    runtime.monotonic.set(7.0);
    socket.mark_ping_sent();
    runtime.monotonic.set(23.0);
    assert_eq!(
        socket.handle_pong_event(5000),
        Err(cause),
        "pong retains latency failure identity"
    );
    assert_eq!(*parent.latencies.borrow(), vec![8.0]);
    runtime.discard_tasks();
}
