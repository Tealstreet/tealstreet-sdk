use super::*;
use safe_cex_exchange::js_runtime::sockets::{base_websocket, core_of};
use std::cell::Cell;

#[test]
fn receive_property_bridges_lazy_initialization_core_frames_and_js_session_reset() {
    let (a, _venue) = assembled_venue(Ok(vec![]));
    a.runtime.hold_sleep.set(true);
    let socket = base_websocket(&a.context, a.this.clone());
    socket.set("lastReceived", Js::Number(17.0)).unwrap();
    let base = core_of(&a.context, &socket).unwrap().base().unwrap();
    assert_eq!(
        base.last_received(),
        17.0,
        "JS receive assignment before core initialization"
    );
    for bits in [
        0x8000_0000_0000_0000,
        0x7ff8_0000_0000_0042,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
        1,
        42.5_f64.to_bits(),
    ] {
        socket
            .set("lastReceived", Js::Number(f64::from_bits(bits)))
            .unwrap();
        assert_eq!(
            base.last_received().to_bits(),
            bits,
            "JS receive write reaches core"
        );
        base.set_last_received(f64::from_bits(bits));
        assert_eq!(
            socket
                .get("lastReceived")
                .unwrap()
                .to_number()
                .unwrap()
                .to_bits(),
            bits,
            "JS receive read observes core"
        );
    }
    a.runtime.wall_clock.set(10_000.0);
    socket.call_method("trackLastReceived", vec![]).unwrap();
    assert_eq!(
        socket.get("lastReceived").unwrap().to_number().unwrap(),
        10_000.0,
        "source frame timestamp visible in JS"
    );
    socket.set("lastReceived", Js::Number(0.0)).unwrap();
    assert!(
        base.last_message_timestamp().is_none(),
        "session reset clears native health timestamp"
    );
    a.runtime.wall_clock.set(11_000.0);
    socket.call_method("trackLastReceived", vec![]).unwrap();
    assert_eq!(base.last_message_timestamp(), Some(11_000.0));
    assert!(base.diagnostics().initialized);
}

#[test]
fn receive_timer_calls_the_derived_receiver_watchdog() {
    let (a, _venue) = assembled_venue(Ok(vec![]));
    a.runtime.wall_clock.set(1000.0);
    let socket = base_websocket(&a.context, a.this.clone());
    let calls = Rc::new(Cell::new(0));
    let expected = socket.clone();
    let observed = calls.clone();
    socket
        .set(
            "checkLastReceived",
            Js::function("checkLastReceived", move |this, _| {
                assert!(this.same_reference(&expected), "derived watchdog receiver");
                observed.set(observed.get() + 1);
                Ok(Js::Undefined)
            }),
        )
        .unwrap();
    let base = core_of(&a.context, &socket).unwrap().base().unwrap();
    socket.call_method("trackLastReceived", vec![]).unwrap();
    // The recording clock resolves this first receive timer; poll only its current jobs.
    let jobs = std::mem::take(&mut *a.runtime.jobs.borrow_mut());
    for mut job in jobs {
        if job
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
        {
            a.runtime.jobs.borrow_mut().push(job);
        }
    }
    assert_eq!(
        calls.get(),
        1,
        "receive timer dispatches derived checkLastReceived"
    );
    base.dispose();
}
