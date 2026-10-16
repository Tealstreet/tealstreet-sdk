use super::*;
use safe_cex_exchange::{
    js_runtime::sockets,
    websocket::{WsEvent, WsSocket},
};
use std::cell::Cell;

#[derive(Default)]
struct CloseSocket(Cell<usize>);
impl WsSocket for CloseSocket {
    fn ready_state(&self) -> u8 {
        1
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        Box::pin(std::future::pending())
    }
    fn send(&self, _: &str) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn close(&self) {
        self.0.set(self.0.get() + 1);
    }
    fn terminate(&self) {
        panic!("raw close must not terminate")
    }
    fn reconnect(&self) {
        panic!("raw close must not reconnect")
    }
    fn is_reconnecting(&self) -> bool {
        false
    }
}

#[test]
fn raw_websocket_close_matches_source_and_preserves_force_closed_flag() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_raw_socket_close_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 16);
    for row in rows {
        let (a, _venue) = assembled_venue(Ok(vec![]));
        a.runtime.hold_sleep.set(true);
        let transport = Rc::new(CloseSocket::default());
        *a.runtime.dial_socket.borrow_mut() = Some(transport.clone());
        let socket = sockets::base_websocket(&a.context, a.this.clone());
        let base = sockets::core_of(&a.context, &socket)
            .unwrap()
            .base()
            .unwrap();
        if row["present"].as_bool().unwrap() {
            let ws = socket
                .call_method(
                    "createWebsocket",
                    vec![Js::str("wss://synthetic.invalid/raw-close")],
                )
                .unwrap();
            socket.set("ws", ws).unwrap();
            for _ in 0..4 {
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
            }
            assert_eq!(
                a.runtime.dials.borrow().len(),
                1,
                "raw_close::connected_fixture"
            );
            assert!(base.is_connected());
        }
        if row["initial"].as_bool().unwrap() {
            base.force_close();
        }
        transport.0.set(0);
        let operation = row["operation"].as_str().unwrap();
        if operation == "dispose" {
            // The source Crypto overrides have no super call. Their only shared
            // close operation must go through the actual returned JS ws handle.
            socket
                .set(
                    "dispose",
                    Js::function("dispose", |this, _| {
                        let ws = this.get("ws")?;
                        if ws.truthy() {
                            ws.call_method("close", vec![])?;
                        }
                        Ok(Js::Undefined)
                    }),
                )
                .unwrap();
        }
        let retained_ws = socket.get("ws").unwrap();
        for _ in 0..2 {
            if operation == "dispose" {
                socket.call_method("dispose", vec![]).unwrap();
            } else {
                base.force_close();
            }
        }
        assert_eq!(
            base.is_force_closed(),
            row["forceClosed"].as_bool().unwrap(),
            "raw_close::force_flag {row}"
        );
        assert_eq!(
            transport.0.get(),
            row["closes"].as_u64().unwrap() as usize,
            "raw_close::transport_calls {row}"
        );
        assert!(
            socket.get("ws").unwrap().strict_equals(&retained_ws),
            "raw_close::retained_handle"
        );
        assert!(a.runtime.exceptions.borrow().is_empty());
    }
}
