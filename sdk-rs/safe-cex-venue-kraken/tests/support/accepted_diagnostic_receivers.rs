//! Native arguments at the terminal host of a real constructed KrakenExchange.
use super::accepted_read_receivers::terminal::{harness, options};
use crate::{ohlcv_polling::PollHost, rest_account::ReadHost};
use safe_cex_exchange::js_runtime::value::{Js, Thrown, error_object};
use safe_cex_types::ExchangeName;

#[test]
fn poll_console_preserves_two_arguments_and_native_error_identity() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let error = error_object("TypeError", "native diagnostic");
    error
        .define(
            "info",
            Js::object(vec![
                ("text", Js::utf16(&[0xd800])),
                ("value", Js::Number(f64::INFINITY)),
            ]),
        )
        .unwrap();
    PollHost::console_error(
        &h.exchange.rt,
        "[Kraken] Error polling OHLCV:",
        Thrown::new(error.clone()),
    )
    .unwrap();
    let calls = h.runtime.native_console.borrow();
    let (severity, args) = calls.last().unwrap();
    assert_eq!(severity, "error");
    assert_eq!(
        args.len(),
        2,
        "console.error(prefix, err) has two native arguments"
    );
    assert_eq!(args[0].to_display(), "[Kraken] Error polling OHLCV:");
    assert!(
        args[1].strict_equals(&error),
        "host receives the original native Error cell"
    );
    assert_eq!(
        args[1]
            .get("info")
            .unwrap()
            .get("text")
            .unwrap()
            .as_str()
            .unwrap()
            .as_ref(),
        &[0xd800]
    );
    assert_eq!(
        args[1].get("info").unwrap().get("value").unwrap().as_f64(),
        Some(f64::INFINITY)
    );
}

#[test]
fn read_trace_preserves_the_original_native_error_argument() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let error = error_object("RangeError", "native trace");
    error.define("number", Js::Number(-0.0)).unwrap();
    ReadHost::trace(&h.exchange.rt, &Thrown::new(error.clone())).unwrap();
    let calls = h.runtime.native_console.borrow();
    let (severity, args) = calls.last().unwrap();
    assert_eq!(severity, "trace");
    assert_eq!(args.len(), 1);
    assert!(
        args[0].strict_equals(&error),
        "console.trace(err) must retain its Error argument"
    );
    assert_eq!(
        args[0].get("number").unwrap().as_f64().unwrap().to_bits(),
        (-0.0f64).to_bits()
    );
}
