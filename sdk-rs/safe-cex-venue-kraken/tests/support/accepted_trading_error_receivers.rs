//! The existing trading-error hosts of an actual constructed KrakenExchange.
use super::accepted_read_receivers::terminal::{harness, options};
use crate::trading::ErrorHost;
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::value::{Js, Thrown, error_object},
};
use safe_cex_types::ExchangeName;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn method_value(name: &str, result: Js) -> (Js, Rc<Cell<usize>>, Rc<RefCell<Vec<Js>>>) {
    let row = Js::object(vec![]);
    let gets = Rc::new(Cell::new(0));
    let receivers = Rc::new(RefCell::new(vec![]));
    let count = gets.clone();
    let seen = receivers.clone();
    row.define_accessor(
        name,
        Some(Rc::new(move |_| {
            count.set(count.get() + 1);
            let result = result.clone();
            let seen = seen.clone();
            Ok(Js::function("source member", move |this, args| {
                assert!(args.is_empty());
                seen.borrow_mut().push(this);
                Ok(result.clone())
            }))
        })),
        None,
    )
    .unwrap();
    (row, gets, receivers)
}

#[test]
fn lower_method_calls_one_live_member_and_preserves_receiver_and_result() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let rt = &h.exchange.rt;
    assert_eq!(
        ErrorHost::lower_method(rt, &Js::utf16(&[65, 0xd800, 90]), "errorMessage")
            .unwrap()
            .as_str()
            .unwrap()
            .as_ref(),
        &[97, 0xd800, 122]
    );
    let result = Js::object(vec![("native", Js::Number(f64::INFINITY))]);
    let (row, gets, receivers) = method_value("toLowerCase", result.clone());
    let actual = ErrorHost::lower_method(rt, &row, "errorMessage").unwrap();
    assert!(actual.strict_equals(&result));
    assert_eq!(gets.get(), 1);
    assert_eq!(receivers.borrow().len(), 1);
    assert!(receivers.borrow()[0].strict_equals(&row));
    assert!(
        ErrorHost::lower_method(rt, &Js::Number(1.0), "errorMessage")
            .unwrap_err()
            .message()
            .contains("errorMessage.toLowerCase is not a function")
    );
    let error = error_object("Error", "lower getter failed");
    let thrown = error.clone();
    let bad = Js::object(vec![]);
    bad.define_accessor(
        "toLowerCase",
        Some(Rc::new(move |_| Err(Thrown::new(thrown.clone())))),
        None,
    )
    .unwrap();
    assert!(
        ErrorHost::lower_method(rt, &bad, "errorMessage")
            .unwrap_err()
            .value
            .strict_equals(&error)
    );
}

#[test]
fn upper_method_calls_one_live_member_and_preserves_receiver_and_result() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let rt = &h.exchange.rt;
    assert_eq!(
        ErrorHost::upper_method(rt, &Js::utf16(&[97, 0xd800, 122]), "symbol")
            .unwrap()
            .as_str()
            .unwrap()
            .as_ref(),
        &[65, 0xd800, 90]
    );
    let (row, gets, receivers) = method_value("toUpperCase", Js::Number(-0.0));
    let actual = ErrorHost::upper_method(rt, &row, "symbol").unwrap();
    assert_eq!(actual.as_f64().unwrap().to_bits(), (-0.0f64).to_bits());
    assert_eq!(gets.get(), 1);
    assert_eq!(receivers.borrow().len(), 1);
    assert!(receivers.borrow()[0].strict_equals(&row));
    assert!(
        ErrorHost::upper_method(rt, &Js::Bool(false), "symbol")
            .unwrap_err()
            .message()
            .contains("symbol.toUpperCase is not a function")
    );
    let error = error_object("Error", "upper getter failed");
    let thrown = error.clone();
    let bad = Js::object(vec![]);
    bad.define_accessor(
        "toUpperCase",
        Some(Rc::new(move |_| Err(Thrown::new(thrown.clone())))),
        None,
    )
    .unwrap();
    assert!(
        ErrorHost::upper_method(rt, &bad, "symbol")
            .unwrap_err()
            .value
            .strict_equals(&error)
    );
}

#[test]
fn emit_and_log_reaches_real_base_console_and_event_with_context() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let rt = &h.exchange.rt;
    let events = Rc::new(RefCell::new(vec![]));
    let seen = events.clone();
    h.exchange.base().on_args(
        "error",
        Rc::new(move |_, args| {
            seen.borrow_mut().push(args.to_vec());
            Ok(())
        }),
    );
    let error = error_object("TypeError", "native error");
    ErrorHost::emit_and_log(rt, Rc::new(error.clone()), Some("source context")).unwrap();
    let calls = h.runtime.native_console.borrow();
    let (_, args) = calls
        .iter()
        .find(|(severity, args)| {
            severity == "error" && args.len() == 2 && args[0].to_display() == "exchange error"
        })
        .unwrap();
    assert!(args[1].strict_equals(&error));
    assert_eq!(events.borrow().len(), 1);
    assert_eq!(events.borrow()[0][0].to_display(), "source context");
    drop(calls);
    ErrorHost::emit_and_log(rt, Rc::new(Js::str("plain message")), None).unwrap();
    assert_eq!(events.borrow().len(), 2);
    assert_eq!(
        events.borrow()[1][0].to_display(),
        "Exchange Error: plain message"
    );
}
