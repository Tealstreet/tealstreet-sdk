use super::*;
use safe_cex_exchange::js_runtime::sockets::{base_websocket, base_websocket_class, core_of};

#[test]
fn typed_socket_disposal_runs_source_overrides_once_and_preserves_super_order() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_socket_dispose_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 4);
    for row in rows {
        for typed in [false, true] {
            let (a, _venue) = assembled_venue(Ok(vec![]));
            let socket = base_websocket(&a.context, a.this.clone());
            let base = core_of(&a.context, &socket).unwrap().base().unwrap();
            a.this.set("publicWebsocket", socket.clone()).unwrap();
            let super_dispose = base_websocket_class(&a.context)
                .get("prototype")
                .unwrap()
                .get("dispose")
                .unwrap();
            let events = Rc::new(RefCell::new(Vec::<Value>::new()));
            let calls = Rc::new(std::cell::Cell::new(0));
            let side = row["side"].as_str().unwrap().to_owned();
            let expected_receiver = socket.clone();
            let (trace, count, core) = (events.clone(), calls.clone(), base.clone());
            socket
                .set(
                    "dispose",
                    Js::function("dispose", move |this, _| {
                        assert!(
                            this.same_reference(&expected_receiver),
                            "original source receiver"
                        );
                        count.set(count.get() + 1);
                        assert!(count.get() <= 2, "super disposal must not recurse");
                        if side == "public" {
                            trace
                                .borrow_mut()
                                .push(json!(["off", "activeTickerSymbols:changed"]));
                        } else {
                            for timer in ["orders", "positions", "balance"] {
                                trace.borrow_mut().push(json!(["clear", timer]));
                            }
                        }
                        super_dispose.call(this, vec![])?;
                        assert!(
                            core.is_force_closed(),
                            "super closes before the override returns"
                        );
                        Ok(Js::Undefined)
                    }),
                )
                .unwrap();
            let times = if row["repeated"].as_bool().unwrap() {
                2
            } else {
                1
            };
            for _ in 0..times {
                if typed {
                    a.base.dispose().unwrap();
                } else {
                    socket.call_method("dispose", vec![]).unwrap();
                }
            }
            assert_eq!(calls.get(), times, "one override per disposal request");
            assert_eq!(json!(*events.borrow()), row["events"]);
            assert_eq!(
                base.is_force_closed(),
                row["forceClosed"].as_bool().unwrap()
            );
            assert!(a.runtime.exceptions.borrow().is_empty());
        }
    }
}

#[test]
fn inherited_socket_dispose_and_throwing_override_still_close_and_reset_guard() {
    let (a, _venue) = assembled_venue(Ok(vec![]));
    let socket = base_websocket(&a.context, a.this.clone());
    let base = core_of(&a.context, &socket).unwrap().base().unwrap();
    base.dispose();
    assert!(
        base.is_force_closed(),
        "inherited dispose terminates without recursion"
    );
    let calls = Rc::new(std::cell::Cell::new(0));
    let count = calls.clone();
    socket
        .set(
            "dispose",
            Js::function("dispose", move |_, _| {
                count.set(count.get() + 1);
                Err(Thrown::type_error("synthetic cleanup failure"))
            }),
        )
        .unwrap();
    base.dispose();
    base.dispose();
    assert_eq!(calls.get(), 2, "an error must release the reentrancy guard");
    assert!(base.is_force_closed());
    let exceptions = a.runtime.exceptions.borrow();
    assert_eq!(exceptions.len(), 2);
    for (message, context) in exceptions.iter() {
        assert!(message.contains("synthetic cleanup failure"));
        assert_eq!(context, &json!({"operation":"dispose"}).into());
    }
}
