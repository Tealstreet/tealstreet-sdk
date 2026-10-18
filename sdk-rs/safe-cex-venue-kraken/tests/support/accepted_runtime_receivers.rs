//! Live trading/tick receivers on the Scope of a real KrakenExchange.
use super::accepted_read_receivers::terminal::{harness, options};
use crate::{
    periodic_sync::TickHost,
    trading::{Client, Method, TradingHost},
};
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::value::{Js, Thrown, error_object},
    runtime::ExchangeError,
};
use safe_cex_types::ExchangeName;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[test]
fn iterable_uses_native_collection_protocol_and_retains_cells() {
    let h = harness(ExchangeName::Krakenspot, options(), |_| {});
    let rt = &h.exchange.rt;
    let cell = Js::object(vec![("text", Js::utf16(&[0xd800]))]);
    let set = Js::new_set();
    set.call_method("add", vec![cell.clone()]).unwrap();
    set.call_method("add", vec![Js::Number(f64::INFINITY)])
        .unwrap();
    let values = TradingHost::iterable(rt, &set, "txids").unwrap();
    assert_eq!(values.len(), 2);
    assert!(values[0].strict_equals(&cell));
    assert_eq!(values[1].as_f64(), Some(f64::INFINITY));
    let map = Js::new_map();
    map.call_method("set", vec![Js::str("id"), cell.clone()])
        .unwrap();
    let entries = TradingHost::iterable(rt, &map, "txids").unwrap();
    assert_eq!(entries.len(), 1);
    let pair = entries[0]
        .array_values()
        .expect("fixture array property reads")
        .unwrap();
    assert_eq!(pair[0].to_display(), "id");
    assert!(pair[1].strict_equals(&cell));
    let units = TradingHost::iterable(rt, &Js::utf16(&[0xd83d, 0xde00, 0xd800]), "txids").unwrap();
    assert_eq!(units.len(), 2);
    assert_eq!(units[0].as_str().unwrap().as_ref(), &[0xd83d, 0xde00]);
    assert_eq!(units[1].as_str().unwrap().as_ref(), &[0xd800]);
    let zeros = TradingHost::iterable(rt, &Js::array(vec![Js::Number(-0.0)]), "txids").unwrap();
    assert_eq!(zeros[0].as_f64().unwrap().to_bits(), (-0.0f64).to_bits());
    // A real built-in iterator is itself iterable; next is a live getter cached once.
    let iterator = Js::array(vec![]).iterator().unwrap();
    let gets = Rc::new(Cell::new(0));
    let calls = Rc::new(Cell::new(0));
    let getter_count = gets.clone();
    let next_count = calls.clone();
    let owner = iterator.clone();
    let yielded = cell.clone();
    iterator
        .define_accessor(
            "next",
            Some(Rc::new(move |_| {
                getter_count.set(getter_count.get() + 1);
                let count = next_count.clone();
                let owner = owner.clone();
                let yielded = yielded.clone();
                Ok(Js::function("next", move |this, args| {
                    assert!(this.strict_equals(&owner));
                    assert!(args.is_empty());
                    let index = count.replace(count.get() + 1);
                    Ok(Js::object(vec![
                        ("done", Js::Bool(index != 0)),
                        ("value", yielded.clone()),
                    ]))
                }))
            })),
            None,
        )
        .unwrap();
    let actual = TradingHost::iterable(rt, &iterator, "txids").unwrap();
    assert_eq!(gets.get(), 1);
    assert_eq!(calls.get(), 2);
    assert_eq!(actual.len(), 1);
    assert!(actual[0].strict_equals(&cell));
    let error = error_object("RangeError", "next getter failed");
    let original = error.clone();
    iterator
        .define_accessor(
            "next",
            Some(Rc::new(move |_| Err(Thrown::new(original.clone())))),
            None,
        )
        .unwrap();
    assert!(
        TradingHost::iterable(rt, &iterator, "txids")
            .unwrap_err()
            .value
            .strict_equals(&error)
    );
    for bad in [Js::Null, Js::Undefined, Js::Bool(true), Js::object(vec![])] {
        assert_eq!(
            TradingHost::iterable(rt, &bad, "txids")
                .unwrap_err()
                .message(),
            "txids is not iterable"
        );
        assert_eq!(
            TradingHost::iterable(rt, &bad, "extraIds")
                .unwrap_err()
                .message(),
            "Spread syntax requires ...iterable[Symbol.iterator] to be a function"
        );
    }
}

#[test]
fn send_uses_actual_clients_verbs_wire_and_native_response() {
    for (client, method, verb) in [
        (Client::Xhr, Method::Post, "POST"),
        (Client::Xhr, Method::Put, "PUT"),
        (Client::Unlimited, Method::Post, "POST"),
        (Client::Unlimited, Method::Put, "PUT"),
    ] {
        let response = Js::object(vec![
            ("native", Js::utf16(&[0xd800])),
            ("number", Js::Number(-0.0)),
        ]);
        let native = response.clone();
        let h = harness(ExchangeName::Kraken, options(), |http| {
            http.route_native(verb, "/receiver", 200.0, native);
        });
        let rt = &h.exchange.rt;
        for (name, label) in [("xhr", "limited"), ("unlimitedXHR", "unlimited")] {
            let interceptor = Js::function("observe real client", move |_, args| {
                let config = args[0].clone();
                config
                    .get("headers")?
                    .set("X-Receiver-Client", Js::str(label))?;
                Ok(config)
            });
            rt.this
                .get(name)
                .unwrap()
                .get("interceptors")
                .unwrap()
                .get("request")
                .unwrap()
                .call_method("use", vec![interceptor])
                .unwrap();
        }
        let body = Rc::new(Js::object(vec![
            ("symbol", Js::str("PI_XBTUSD")),
            ("size", Js::Number(2.5)),
        ]));
        let actual = h
            .drive(Box::pin(async {
                TradingHost::send(rt, false, client, method, "/receiver", body)
                    .map_err(|error| error.to_exchange_error())?
                    .await
                    .map_err(|error| error.to_exchange_error())
            }))
            .unwrap();
        assert!(actual.strict_equals(&response));
        assert_eq!(
            actual.get("native").unwrap().as_str().unwrap().as_ref(),
            &[0xd800]
        );
        assert_eq!(
            actual.get("number").unwrap().as_f64().unwrap().to_bits(),
            (-0.0f64).to_bits()
        );
        let requests = h.http.calls("/receiver");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, verb);
        assert_eq!(
            requests[0].headers["X-Receiver-Client"],
            match client {
                Client::Xhr => "limited",
                Client::Unlimited => "unlimited",
            }
        );
        let wire = requests[0].data.as_str().unwrap();
        assert!(wire.contains("symbol=PI_XBTUSD"), "{wire}");
        assert!(wire.contains("size=2.5"), "{wire}");
    }
}

#[test]
fn emit_tick_error_uses_message_or_fallback_through_real_base() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let events = Rc::new(RefCell::new(vec![]));
    let observed = events.clone();
    h.exchange.base().on_args(
        "error",
        Rc::new(move |_, args| {
            observed.borrow_mut().push(args.to_vec());
            Ok(())
        }),
    );
    for (message, expected) in [
        ("original message", "original message"),
        ("", "tick fallback"),
    ] {
        TickHost::emit_tick_error(&h.exchange.rt, ExchangeError::new(message), "tick fallback")
            .unwrap();
        let rows = events.borrow();
        assert_eq!(
            rows.last().unwrap()[0].to_display(),
            format!("Exchange Error: {expected}")
        );
        let console = h.runtime.native_console.borrow();
        let error = console
            .iter()
            .rev()
            .find(|(severity, _)| severity == "error")
            .unwrap();
        assert_eq!(error.1.len(), 2);
        assert_eq!(error.1[0].to_display(), "exchange error");
        assert_eq!(error.1[1].to_display(), expected);
    }
    assert_eq!(events.borrow().len(), 2);
}
