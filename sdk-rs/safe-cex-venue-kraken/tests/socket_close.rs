//! The production Kraken socket's no-event `handleClose` owns transport closes, as in TS.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod support;
use safe_cex_exchange::js_runtime::value::Js;
use safe_cex_exchange::websocket::WsEvent;
use safe_cex_types::ExchangeName;
use std::{cell::RefCell, rc::Rc};
use support::{harness, options};

const PUBLIC: &str = "wss://futures.kraken.com/ws/v1";

#[test]
fn abnormal_closes_reconnect_at_the_minimum_delay_without_a_failure_pause() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let logs: Rc<RefCell<Vec<String>>> = Rc::default();
    let sink = logs.clone();
    h.venue().base().on(
        "log",
        Rc::new(move |_, value: &Js| {
            sink.borrow_mut().push(
                String::from_utf16(value.as_str().expect("source log message"))
                    .expect("fixture log UTF16"),
            );
            Ok(())
        }),
    );
    let _cleanup = h
        .venue()
        .listen_order_book("PF_XBTUSD".into(), Rc::new(|_| {}))
        .unwrap();
    h.runtime.pump();
    let dials = || {
        h.runtime
            .sockets
            .borrow()
            .iter()
            .filter(|(url, _)| url == PUBLIC)
            .count()
    };
    assert_eq!(dials(), 1);
    // TS retries every WS_MIN_RECONNECTION_DELAY indefinitely; a coded close would
    // back off exponentially and pause 120 s after the 8th consecutive failure.
    for attempt in 1..=10 {
        let socket = h
            .runtime
            .sockets
            .borrow()
            .iter()
            .rev()
            .find(|(url, _)| url == PUBLIC)
            .unwrap()
            .1
            .clone();
        socket.closed.set(true);
        socket.events.borrow_mut().push_back(WsEvent::Close {
            code: Some(1006),
            reason: String::new(),
        });
        for _ in 0..30 {
            h.runtime.advance(100.0);
            if dials() > attempt {
                break;
            }
        }
        assert_eq!(
            dials(),
            attempt + 1,
            "close {attempt} must reconnect within 3 s: {:?}",
            logs.borrow()
        );
    }
    let logs = logs.borrow();
    assert!(
        !logs.iter().any(|log| log.contains("(code: 1006")),
        "the transport code never reaches processClose: {logs:?}"
    );
    assert!(
        !logs.iter().any(|log| log.contains("already reconnecting")),
        "each close is processed once: {logs:?}"
    );
}
