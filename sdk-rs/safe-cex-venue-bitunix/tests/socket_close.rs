//! Bitunix sockets register a no-event `handleClose`/`handleError`, as in TS.
#[path = "support/venue_runtime.rs"]
#[allow(dead_code)]
mod support;
use safe_cex_exchange::{exchange::Exchange, websocket::WsEvent};
use serde_json::json;
use std::{
    cell::RefCell,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn drive<F: Future>(rt: &support::Runtime, f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..64 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return v;
        }
        rt.now.set(rt.now.get() + 1000.0);
        rt.pump();
    }
    panic!("scripted call must settle");
}

fn started() -> (
    Rc<safe_cex_venue_bitunix::adapter::BitunixVenue>,
    Rc<support::Runtime>,
    Rc<RefCell<Vec<String>>>,
    String,
) {
    let (venue, rt, _, wire) = support::fixture_with(true);
    for (path, data) in [
        ("/api/v1/futures/market/trading_pairs", json!([])),
        ("/api/v1/futures/market/tickers", json!([])),
    ] {
        wire.bodies
            .lock()
            .unwrap()
            .insert(path.into(), json!({"code":0,"data":data}));
    }
    let logs: Rc<RefCell<Vec<String>>> = Rc::default();
    let sink = logs.clone();
    venue.base().on(
        "log",
        Rc::new(move |_, value: &safe_cex_exchange::js_runtime::value::Js| {
            let value = safe_cex_exchange::js_runtime::value::to_json(value).unwrap();
            sink.borrow_mut()
                .push(value.as_str().unwrap_or_default().to_owned());
            Ok(())
        }),
    );
    rt.dial.set(true);
    drive(&rt, venue.start()).unwrap();
    for _ in 0..4 {
        rt.pump();
        rt.now.set(rt.now.get() + 100.0);
    }
    let url = rt
        .dialed
        .borrow()
        .first()
        .expect("a socket dials")
        .0
        .clone();
    (venue, rt, logs, url)
}

fn dials(rt: &support::Runtime, url: &str) -> usize {
    rt.dialed.borrow().iter().filter(|(u, _)| u == url).count()
}

fn latest(rt: &support::Runtime, url: &str) -> Rc<support::FakeSocket> {
    rt.dialed
        .borrow()
        .iter()
        .rev()
        .find(|(u, _)| u == url)
        .unwrap()
        .1
        .clone()
}

fn reconnects_within_3s(rt: &support::Runtime, url: &str, expected: usize) -> bool {
    for _ in 0..30 {
        rt.now.set(rt.now.get() + 100.0);
        rt.pump();
        if dials(rt, url) >= expected {
            return true;
        }
    }
    false
}

#[test]
fn abnormal_closes_reconnect_at_the_minimum_delay_without_a_failure_pause() {
    let (venue, rt, logs, url) = started();
    // TS retries every WS_MIN_RECONNECTION_DELAY; a coded close would back off
    // exponentially and pause 120 s after the 8th consecutive failure.
    for attempt in 1..=10 {
        latest(&rt, &url)
            .events
            .borrow_mut()
            .push_back(WsEvent::Close {
                code: Some(1006),
                reason: String::new(),
            });
        assert!(
            reconnects_within_3s(&rt, &url, attempt + 1),
            "close {attempt} must reconnect within 3 s: {:?}",
            logs.borrow()
        );
    }
    assert!(
        !logs.borrow().iter().any(|log| log.contains("(code: 1006")),
        "the transport code never reaches processClose: {:?}",
        logs.borrow()
    );
    venue.dispose().unwrap();
}

#[test]
fn a_socket_error_runs_handle_close() {
    let (venue, rt, logs, url) = started();
    latest(&rt, &url)
        .events
        .borrow_mut()
        .push_back(WsEvent::Error("boom".into()));
    assert!(
        reconnects_within_3s(&rt, &url, 2),
        "handleError must close and reconnect: {:?}",
        logs.borrow()
    );
    venue.dispose().unwrap();
}
