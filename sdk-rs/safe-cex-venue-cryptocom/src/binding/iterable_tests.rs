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
    socket.define_accessor(
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
    );
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
                    );
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
