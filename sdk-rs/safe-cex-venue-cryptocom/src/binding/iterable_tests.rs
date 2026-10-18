//! Real constructor, connected core socket and ported class methods over scripted IO.
use super::*;
use std::cell::Cell;
// Reuse the full transport fixture; these tests need only its socket paths.
#[allow(dead_code)]
#[path = "../../tests/support/binding_harness.rs"]
mod support;

fn connected() -> (support::Harness, Js) {
    let h = support::harness(support::options(), |_| {});
    let socket = h.exchange.rt.this.get("publicWebsocket").unwrap();
    socket.call_method("connectAndSubscribe", vec![]).unwrap();
    h.runtime.pump();
    assert!(socket.get("isConnected").unwrap().truthy());
    assert_eq!(h.runtime.sockets.borrow().len(), 1);
    (h, socket)
}
fn listen(socket: &Js) -> Js {
    socket
        .call_method(
            "listenOrderBook",
            vec![
                Js::str("BTCUSD-PERP"),
                Js::function("callback", |_, _| Ok(Js::Undefined)),
            ],
        )
        .unwrap()
}

#[test]
fn production_listener_spread_preserves_astral_and_lone_utf16_units() {
    let (_h, socket) = connected();
    let handlers = socket.get("messageHandlers").unwrap();
    handlers
        .define("book.BTCUSD-PERP", Js::utf16(&[65, 0xd83d, 0xde00, 0xd800]))
        .unwrap();
    let _cleanup = listen(&socket);
    let values = handlers
        .get("book.BTCUSD-PERP")
        .unwrap()
        .array_values()
        .unwrap();
    assert_eq!(
        values.len(),
        4,
        "source spread yields three code points and the parser"
    );
    for (value, expected) in values[..3]
        .iter()
        .zip([vec![65], vec![0xd83d, 0xde00], vec![0xd800]])
    {
        let Js::String(units) = value else {
            panic!("native string piece")
        };
        assert_eq!(
            &**units, expected,
            "string code-point iteration before JSON"
        );
    }
    assert!(values[3].is_callable());
}

#[test]
fn production_listener_spread_accepts_collection_iterator_objects() {
    for shape in ["set-values", "map-values"] {
        let (_h, socket) = connected();
        let first = Js::function("first", |_, _| Ok(Js::Undefined));
        let collection = if shape == "set-values" {
            let set = Js::new_set();
            set.call_method("add", vec![first.clone()]).unwrap();
            set
        } else {
            let map = Js::new_map();
            map.call_method("set", vec![Js::str("first"), first.clone()])
                .unwrap();
            map
        };
        let iterator = collection.call_method("values", vec![]).unwrap();
        let handlers = socket.get("messageHandlers").unwrap();
        handlers.define("book.BTCUSD-PERP", iterator).unwrap();
        let _cleanup = listen(&socket);
        let values = handlers
            .get("book.BTCUSD-PERP")
            .unwrap()
            .array_values()
            .unwrap();
        assert_eq!(values.len(), 2, "{shape}: retained listener plus parser");
        assert!(values[0].same_reference(&first));
        assert!(values[1].is_callable());
    }
}

#[test]
fn production_subscription_for_of_pulls_only_after_the_previous_body() {
    let (h, socket) = connected();
    let channels = Js::new_set();
    channels.call_method("add", vec![Js::str("a")]).unwrap();
    channels.call_method("add", vec![Js::str("b")]).unwrap();
    let iterator = channels.call_method("values", vec![]).unwrap();
    let next = iterator.get("next").unwrap();
    let subscriptions = socket.get("subscriptions").unwrap();
    let pulls = Rc::new(Cell::new(0));
    let body_seen = Rc::new(Cell::new(false));
    let count = pulls.clone();
    let observed = body_seen.clone();
    iterator
        .define(
            "next",
            Js::function("next", move |this, args| {
                let index = count.get();
                count.set(index + 1);
                if index == 1 {
                    let seen = subscriptions
                        .call_method("has", vec![Js::str("a")])?
                        .truthy();
                    observed.set(seen);
                    channels.call_method("delete", vec![Js::str("b")])?;
                    channels.call_method("add", vec![Js::str("c")])?;
                }
                next.call(this, args)
            }),
        )
        .unwrap();
    socket
        .call_method("registerAndSubscribe", vec![iterator])
        .unwrap();
    assert!(
        body_seen.get(),
        "source for-of registers a before pulling again"
    );
    assert_eq!(pulls.get(), 3);
    let calls = h.runtime.sockets.borrow();
    let sent: Vec<serde_json::Value> = calls[0]
        .1
        .sent
        .borrow()
        .iter()
        .map(|wire| serde_json::from_str(wire).unwrap())
        .collect();
    let subscribe = sent
        .iter()
        .find(|frame| frame["method"] == "subscribe")
        .unwrap();
    assert_eq!(
        subscribe["params"]["channels"],
        serde_json::json!(["a", "c"])
    );
}

#[test]
fn production_subscription_body_throw_closes_and_keeps_the_original_cause() {
    let (_h, socket) = connected();
    let channels = Js::new_set();
    channels.call_method("add", vec![Js::str("a")]).unwrap();
    let iterator = channels.call_method("values", vec![]).unwrap();
    let closes = Rc::new(Cell::new(0));
    let count = closes.clone();
    let expected_this = iterator.clone();
    iterator
        .define(
            "return",
            Js::function("return", move |this, _| {
                assert!(this.same_reference(&expected_this));
                count.set(count.get() + 1);
                Err(Thrown::error("Error", "close error"))
            }),
        )
        .unwrap();
    let subscriptions = socket.get("subscriptions").unwrap();
    let reads = Rc::new(Cell::new(0));
    let original = Js::object(vec![("marker", Js::str("body failure"))]);
    let failure = original.clone();
    socket
        .define_accessor(
            "subscriptions",
            Some(Rc::new(move |_| {
                let index = reads.get();
                reads.set(index + 1);
                if index > 0 {
                    Err(Thrown::new(failure.clone()))
                } else {
                    Ok(subscriptions.clone())
                }
            })),
            None,
        )
        .expect("fresh test accessor target");
    let error = socket
        .call_method("registerAndSubscribe", vec![iterator])
        .unwrap_err();
    assert!(
        error.value.same_reference(&original),
        "original body cause wins"
    );
    assert_eq!(closes.get(), 1, "source for-of performs IteratorClose");
}

#[test]
fn production_iterator_pair_closes_early_and_checks_return_shape() {
    use crate::{native_bindings::BoundHost, rest_reads::ReadHost};
    for valid_return in [true, false] {
        let h = support::harness(support::options(), |_| {});
        let host = BoundHost {
            ops: ops::binding_ops(Rc::downgrade(&h.exchange.rt.shared)),
            receiver: h.exchange.rt.this.clone(),
        };
        let iterator = Js::new_set().call_method("values", vec![]).unwrap();
        let pulls = Rc::new(Cell::new(0));
        let count = pulls.clone();
        iterator
            .define(
                "next",
                Js::function("next", move |_, _| {
                    count.set(count.get() + 1);
                    Ok(Js::object(vec![
                        ("done", Js::Bool(count.get() > 3)),
                        ("value", Js::Number(count.get() as f64)),
                    ]))
                }),
            )
            .unwrap();
        let closes = Rc::new(Cell::new(0));
        let count = closes.clone();
        iterator
            .define(
                "return",
                Js::function("return", move |_, _| {
                    count.set(count.get() + 1);
                    Ok(if valid_return {
                        Js::object(vec![])
                    } else {
                        Js::Number(7.0)
                    })
                }),
            )
            .unwrap();
        let result = host.iterator_pair(iterator, Ok);
        if valid_return {
            let (a, b) = result.unwrap();
            assert_eq!((a.as_f64(), b.as_f64()), (Some(1.0), Some(2.0)));
        } else {
            let error = result.unwrap_err();
            assert_eq!(error.value.get("name").unwrap().to_display(), "TypeError");
        }
        assert_eq!(
            pulls.get(),
            2,
            "pair binding must not eagerly exhaust its input"
        );
        assert_eq!(closes.get(), 1, "normal early binding closes exactly once");
    }
}

#[test]
fn production_subscription_step_failures_do_not_close_the_iterator() {
    for mode in ["next", "done", "value"] {
        let (_h, socket) = connected();
        let iterator = Js::new_set().call_method("values", vec![]).unwrap();
        let original = Js::object(vec![("marker", Js::str(mode))]);
        let failure = original.clone();
        iterator
            .define(
                "next",
                Js::function("next", move |_, _| {
                    if mode == "next" {
                        return Err(Thrown::new(failure.clone()));
                    }
                    let step = Js::object(vec![("done", Js::Bool(false)), ("value", Js::str("a"))]);
                    let failure = failure.clone();
                    step.define_accessor(
                        mode,
                        Some(Rc::new(move |_| Err(Thrown::new(failure.clone())))),
                        None,
                    )
                    .expect("fresh test accessor target");
                    Ok(step)
                }),
            )
            .unwrap();
        let closes = Rc::new(Cell::new(0));
        let count = closes.clone();
        iterator
            .define(
                "return",
                Js::function("return", move |_, _| {
                    count.set(count.get() + 1);
                    Ok(Js::object(vec![]))
                }),
            )
            .unwrap();
        let error = socket
            .call_method("registerAndSubscribe", vec![iterator])
            .unwrap_err();
        assert!(
            error.value.same_reference(&original),
            "{mode}: original step cause"
        );
        assert_eq!(closes.get(), 0, "{mode}: no IteratorClose before a body");
    }
}

#[test]
fn production_prepared_pair_captures_method_once_and_keeps_receiver_and_native_payload() {
    use crate::native_bindings::{BoundHost, Prepared};
    let h = support::harness(support::options(), |_| {});
    let host = BoundHost {
        ops: ops::binding_ops(Rc::downgrade(&h.exchange.rt.shared)),
        receiver: h.exchange.rt.this.clone(),
    };
    let receiver = Js::object(vec![("marker", Js::Number(1.0))]);
    let payload = Js::object(vec![
        ("undefined", Js::Undefined),
        ("text", Js::utf16(&[0xdc00])),
    ]);
    let bits = 0x7ff8_0000_0000_0042;
    payload
        .define_key(&[0xd800], Js::Number(f64::from_bits(bits)))
        .unwrap();
    let weak = match &receiver {
        Js::Object(data) => Rc::downgrade(data),
        _ => unreachable!(),
    };
    let calls = Rc::new(Cell::new(0));
    let invoked = calls.clone();
    let returned = payload.clone();
    let function = Js::function("original", move |this, args| {
        invoked.set(invoked.get() + 1);
        let expected = Js::Object(weak.upgrade().unwrap());
        assert!(this.same_reference(&expected));
        assert_eq!(
            this.get("marker")?.as_f64().unwrap().to_bits(),
            (-0.0_f64).to_bits()
        );
        assert!(args[0].same_reference(&returned));
        Ok(returned.clone())
    });
    let reads = Rc::new(Cell::new(0));
    let count = reads.clone();
    let captured = function.clone();
    receiver
        .define_accessor(
            "method",
            Some(Rc::new(move |_| {
                count.set(count.get() + 1);
                Ok(captured.clone())
            })),
            None,
        )
        .unwrap();
    let prepared: Prepared<Js> = host.prepare(receiver.clone(), "method").unwrap();
    assert!(prepared.0.same_reference(&function));
    assert!(prepared.1.same_reference(&receiver));
    receiver
        .define(
            "method",
            Js::function("replacement", |_, _| panic!("prepared function was reread")),
        )
        .unwrap();
    receiver.set("marker", Js::Number(-0.0)).unwrap();
    let value = host.call(prepared, vec![payload.clone()]).unwrap();
    assert!(value.same_reference(&payload));
    assert_eq!(
        value
            .get_key(&[0xd800])
            .unwrap()
            .as_f64()
            .unwrap()
            .to_bits(),
        bits
    );
    assert_eq!(
        value.get("text").unwrap().as_str().unwrap().as_ref(),
        &[0xdc00]
    );
    assert!(value.has_own("undefined"));
    assert!(value.get("undefined").unwrap().is_undefined());
    assert_eq!(reads.get(), 1);
    assert_eq!(calls.get(), 1);
}

#[test]
fn production_prepared_pair_keeps_original_getter_throw_before_call() {
    use crate::native_bindings::BoundHost;
    let h = support::harness(support::options(), |_| {});
    let host = BoundHost {
        ops: ops::binding_ops(Rc::downgrade(&h.exchange.rt.shared)),
        receiver: h.exchange.rt.this.clone(),
    };
    let cause = Js::object(vec![("code", Js::utf16(&[0xd800]))]);
    let returned = cause.clone();
    let receiver = Js::object(vec![]);
    receiver
        .define_accessor(
            "method",
            Some(Rc::new(move |_| Err(Thrown::new(returned.clone())))),
            None,
        )
        .unwrap();
    let continued = Cell::new(false);
    let error = host
        .prepare(receiver, "method")
        .and_then(|prepared| {
            continued.set(true);
            host.call(prepared, vec![])
        })
        .unwrap_err();
    assert!(error.value.same_reference(&cause));
    assert!(!continued.get());
}
