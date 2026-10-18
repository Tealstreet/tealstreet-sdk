//! Candle no-ops reached through the actual constructed Futures public socket.
use super::accepted_read_receivers::terminal::{harness, options};
use crate::classes;
use safe_cex_exchange::js_runtime::value::Js;
use safe_cex_types::ExchangeName;
use std::{cell::Cell, process::Command, rc::Rc};

#[test]
fn futures_public_candle_members_keep_live_source_noop_contract() {
    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/stream-binding-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let count: usize = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(count, 8, "all actual source input cases executed");
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let rt = &h.exchange.rt;
    let socket = classes::futures_public(rt, Rc::new(rt.this.clone())).unwrap();
    let calls = Rc::new(Cell::new(0));
    let sink = calls.clone();
    let callback = Js::function("callback", move |_, _| {
        sink.set(sink.get() + 1);
        Ok(Js::Undefined)
    });
    for input in [
        Js::Undefined,
        Js::Null,
        Js::Bool(false),
        Js::Number(-0.0),
        Js::Number(f64::NAN),
        Js::utf16(&[0xd800]),
        Js::array(vec![]),
        Js::object(vec![]),
    ] {
        let first = socket
            .call_method("listenOHLCV", vec![input.clone(), callback.clone()])
            .unwrap();
        let second = socket
            .call_method("listenOHLCV", vec![input.clone(), callback.clone()])
            .unwrap();
        assert!(
            !first.strict_equals(&second),
            "source creates a fresh cleanup"
        );
        for cleanup in [&first, &first, &second] {
            assert!(
                cleanup.call(Js::Undefined, vec![]).unwrap().is_undefined(),
                "source cleanup returns undefined"
            );
        }
        assert!(
            socket
                .call_method("handleOHLC", vec![input])
                .unwrap()
                .is_undefined(),
            "source handler returns undefined"
        );
    }
    h.runtime.pump();
    assert_eq!(
        calls.get(),
        0,
        "unsupported Futures websocket candles never invoke callback"
    );
    assert!(
        h.runtime.sockets.borrow().is_empty(),
        "no physical connection"
    );
    assert!(h.http.requests.borrow().is_empty(), "no REST fallback");
}
