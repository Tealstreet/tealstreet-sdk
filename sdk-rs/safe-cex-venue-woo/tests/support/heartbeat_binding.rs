use crate::binding_test_harness as fixture;
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[test]
fn heartbeat_members_match_source_order_and_retained_failures() {
    let mut results = Vec::new();
    for (channel, method) in [
        ("public", "ping"),
        ("private", "ping"),
        ("public", "handlePingEvent"),
    ] {
        for mode in [
            "success",
            "disposed",
            "send-fail",
            "clock-fail",
            "set-fail",
            "dynamic-send",
            "stringify-fail",
            "retained-payload",
        ] {
            let h = fixture::harness(ExchangeName::Woo, fixture::options(), |_| {});
            let socket = h
                .exchange
                .rt
                .this
                .get(if channel == "public" {
                    "publicWebsocket"
                } else {
                    "privateWebsocket"
                })
                .unwrap();
            let sentinel = Js::object(vec![
                ("units", Js::utf16(&[0xd800, 97, 0xdc00])),
                ("own", Js::Undefined),
            ]);
            let trace = Rc::new(RefCell::new(Vec::new()));
            let sent = Rc::new(RefCell::new(Vec::<Vec<u16>>::new()));
            let timestamp = Rc::new(Cell::new(false));
            let owner = socket.clone();
            let seen = trace.clone();
            socket
                .define_accessor(
                    "isDisposed",
                    Some(Rc::new(move |receiver| {
                        assert!(receiver.same_reference(&owner));
                        seen.borrow_mut().push("disposed".to_owned());
                        Ok(if mode == "disposed" {
                            Js::utf16(&[0xd800])
                        } else {
                            Js::Bool(false)
                        })
                    })),
                    None,
                )
                .unwrap();
            let seen = trace.clone();
            let error = sentinel.clone();
            h.exchange
                .rt
                .shared
                .globals
                .get("performance")
                .unwrap()
                .define(
                    "now",
                    Js::function("now", move |_, args| {
                        assert!(args.is_empty());
                        seen.borrow_mut().push("now".to_owned());
                        if mode == "clock-fail" {
                            return Err(Thrown::new(error.clone()));
                        }
                        Ok(Js::Number(-0.0))
                    }),
                )
                .unwrap();
            let seen = trace.clone();
            let error = sentinel.clone();
            h.exchange
                .rt
                .shared
                .globals
                .get("JSON")
                .unwrap()
                .define(
                    "stringify",
                    Js::function("stringify", move |_, args| {
                        assert_eq!(args.len(), 1);
                        assert!(args[0].get("event")?.strict_equals(&Js::str(
                            if method == "ping" { "ping" } else { "pong" }
                        )));
                        seen.borrow_mut().push("stringify".to_owned());
                        if mode == "stringify-fail" {
                            return Err(Thrown::new(error.clone()));
                        }
                        Ok(if mode == "retained-payload" {
                            Js::utf16(&[0xd800, 97, 0xdc00])
                        } else {
                            Js::str(if method == "ping" {
                                r#"{"event":"ping"}"#
                            } else {
                                r#"{"event":"pong"}"#
                            })
                        })
                    }),
                )
                .unwrap();
            let seen = trace.clone();
            let payloads = sent.clone();
            let owner = socket.clone();
            let error = sentinel.clone();
            let send = Js::function("send", move |receiver, args| {
                assert!(receiver.same_reference(&owner));
                assert_eq!(args.len(), 1);
                seen.borrow_mut().push("send".to_owned());
                payloads
                    .borrow_mut()
                    .push(args[0].as_str().unwrap().to_vec());
                if mode == "send-fail" {
                    return Err(Thrown::new(error.clone()));
                }
                Ok(Js::Undefined)
            });
            let current = Rc::new(RefCell::new(
                if mode == "dynamic-send" && method == "ping" {
                    Js::function("stale", |_, _| panic!("stale sender"))
                } else {
                    send.clone()
                },
            ));
            let sender = current.clone();
            let seen = trace.clone();
            let error = sentinel.clone();
            let recorded = timestamp.clone();
            socket
                .define_accessor(
                    "pingAt",
                    None,
                    Some(Rc::new(move |_, value| {
                        assert_eq!(value.as_f64().unwrap().to_bits(), (-0.0f64).to_bits());
                        seen.borrow_mut().push("set:pingAt".to_owned());
                        recorded.set(true);
                        if mode == "set-fail" {
                            return Err(Thrown::new(error.clone()));
                        }
                        if mode == "dynamic-send" {
                            *sender.borrow_mut() = send.clone();
                        }
                        Ok(())
                    })),
                )
                .unwrap();
            let member = if channel == "private" {
                "sendWithLog"
            } else {
                "send"
            };
            let seen = trace.clone();
            socket
                .define_accessor(
                    member,
                    Some(Rc::new(move |_| {
                        seen.borrow_mut().push(format!("get:{member}"));
                        Ok(current.borrow().clone())
                    })),
                    None,
                )
                .unwrap();
            let outcome = socket.get(method).unwrap().call(Js::object(vec![]), vec![]);
            let caught = match outcome {
                Ok(value) => {
                    assert!(value.is_undefined());
                    false
                }
                Err(error) => {
                    assert!(error.value.same_reference(&sentinel));
                    true
                }
            };
            assert!(sentinel.has_own("own"));
            assert!(sentinel.get("own").unwrap().is_undefined());
            assert_eq!(
                sentinel.get("units").unwrap().as_str().unwrap().to_vec(),
                vec![0xd800, 97, 0xdc00]
            );
            assert!(h.http.requests.borrow().is_empty());
            assert!(h.runtime.reports.borrow().is_empty());
            results.push(json!({"channel":channel,"method":method,"mode":mode,"trace":*trace.borrow(),"sent":*sent.borrow(),"caught":caught,"timestampSet":timestamp.get()}));
        }
    }
    assert_eq!(results.len(), 24);
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/socket-classes-oracle.cjs"
        ))
        .arg("heartbeat-audit")
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let expected: serde_json::Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(
        json!(results),
        expected,
        "constructed heartbeat source parity"
    );
}
