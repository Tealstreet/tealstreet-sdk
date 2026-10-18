//! The actual Scope stored by a constructed KrakenExchange, with terminal HTTP only.
#[allow(dead_code)]
#[path = "binding_harness.rs"]
pub(super) mod terminal;

use crate::rest_account::ReadHost;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use safe_cex_types::ExchangeName;
use std::rc::Rc;
use terminal::{harness, options};

#[test]
fn keyed_preserves_native_cells_and_key_precedence() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let rt = &h.exchange.rt;
    let cell = Js::object(vec![("raw", Js::utf16(&[0xd800]))]);
    let row = Js::object(vec![("source_key", cell.clone()), ("sourceKey", Js::Null)]);
    assert!(rt.keyed(&row, "Source Key").unwrap().strict_equals(&cell));
    assert!(matches!(&*rt.keyed(&row, "sourceKey").unwrap(), Js::Null));
    row.define("sourceKey", Js::Undefined).unwrap();
    assert!(rt.keyed(&row, "sourceKey").unwrap().is_undefined());
    assert!(rt.keyed(&row, "absent").unwrap().is_undefined());
    row.define("negativeZero", Js::Number(-0.0)).unwrap();
    assert_eq!(
        rt.keyed(&row, "negativeZero")
            .unwrap()
            .as_f64()
            .unwrap()
            .to_bits(),
        (-0.0f64).to_bits()
    );
    row.define("overflow", Js::Number(f64::INFINITY)).unwrap();
    assert_eq!(
        rt.keyed(&row, "overflow").unwrap().as_f64(),
        Some(f64::INFINITY)
    );
    let inherited = Js::object(vec![("sourceKey", Js::Null)]);
    inherited
        .set("__proto__", Js::object(vec![("source_key", cell.clone())]))
        .unwrap();
    assert!(
        rt.keyed(&inherited, "Source Key")
            .unwrap()
            .strict_equals(&cell),
        "inherited snake key must precede own camel key"
    );
    assert!(
        rt.keyed(&Js::Number(1.0), "key").is_err(),
        "live get-key uses in and refuses primitive receivers"
    );
    let throwing = Js::object(vec![]);
    throwing
        .define_accessor(
            "sourceKey",
            Some(Rc::new(|_| Err(Thrown::error("Error", "getter failed")))),
            None,
        )
        .unwrap();
    assert_eq!(
        rt.keyed(&throwing, "sourceKey").unwrap_err().message(),
        "getter failed"
    );
}

#[test]
fn starts_with_uses_the_native_string_method() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let rt = &h.exchange.rt;
    assert!(
        rt.starts_with(&Js::utf16(&[b'X' as u16, 0xd800]), "X")
            .unwrap()
    );
    assert!(!rt.starts_with(&Js::str("PXBTUSD"), "X").unwrap());
    assert!(rt.starts_with(&Js::str(""), "").unwrap());
    assert!(rt.starts_with(&Js::Number(1.0), "1").is_err());
    assert!(rt.starts_with(&Js::Null, "").is_err());
}

#[test]
fn includes_uses_the_native_string_method() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let rt = &h.exchange.rt;
    assert!(
        rt.includes(&Js::utf16(&[0xd800, b'X' as u16]), "X")
            .unwrap()
    );
    assert!(!rt.includes(&Js::str("PXBTUSD"), "usd").unwrap());
    assert!(rt.includes(&Js::str(""), "").unwrap());
    assert!(rt.includes(&Js::Number(1.0), "1").is_err());
    assert!(rt.includes(&Js::Undefined, "").is_err());
}

#[test]
fn current_reads_the_live_exchange_and_store_slots() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let rt = &h.exchange.rt;
    for (slot, property) in [
        ("disposed", "isDisposed"),
        ("publicDataOnly", "isPublicDataOnly"),
    ] {
        let actual = rt.this.get(property).unwrap();
        assert!(rt.current(slot).unwrap().strict_equals(&actual));
    }
    let store = rt.this.get("store").unwrap();
    let actual = store.get("positions").unwrap();
    assert!(actual.is_array());
    // The shared Store facade currently returns snapshots; array identity belongs
    // to the separately owned Store cutover, not this receiver adapter.
    assert_eq!(
        rt.current("positions")
            .unwrap()
            .array_values()
            .unwrap()
            .len(),
        actual.array_values().unwrap().len()
    );
    store
        .call_method(
            "update",
            vec![Js::object(vec![("latency", Js::Number(42.5))])],
        )
        .unwrap();
    assert_eq!(rt.current("latency").unwrap().as_f64(), Some(42.5));
    assert!(rt.current("unknownSlot").unwrap().is_undefined());
    h.exchange.dispose().unwrap();
    assert!(matches!(&*rt.current("disposed").unwrap(), Js::Bool(true)));
    let mut public_options = options();
    public_options.public_data_only = Some(true);
    let public = harness(ExchangeName::Kraken, public_options, |_| {});
    assert!(matches!(
        &*public.exchange.rt.current("publicDataOnly").unwrap(),
        Js::Bool(true)
    ));
}

#[test]
fn preference_reads_real_maps_without_projecting_values() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let rt = &h.exchange.rt;
    for (cache, property) in [
        ("margin", "marginTypePreferences"),
        ("leverage", "leverageSettings"),
    ] {
        let map = rt.this.get(property).unwrap();
        let symbol = Js::utf16(&[b'X' as u16, 0xd800]);
        let cell = Js::object(vec![("raw", Js::Number(f64::NAN))]);
        map.call_method("set", vec![symbol.clone(), cell.clone()])
            .unwrap();
        assert!(rt.preference(cache, &symbol).unwrap().strict_equals(&cell));
        assert!(
            rt.preference(cache, &Js::str("absent"))
                .unwrap()
                .is_undefined()
        );
    }
}

#[test]
fn read_forwards_real_get_params_and_post_body_to_http() {
    let response = Js::object(vec![
        ("raw", Js::Number(f64::INFINITY)),
        ("text", Js::utf16(&[0xd800])),
    ]);
    let h = harness(ExchangeName::Kraken, options(), |http| {
        for (method, path) in [
            ("GET", "/v3/fills"),
            ("GET", "/v3/openpositions"),
            ("POST", "/receiver-read"),
        ] {
            http.route_native(method, path, 200.0, response.clone());
        }
    });
    let rt = &h.exchange.rt;
    for (spot, endpoint, post) in [
        (false, "/v3/fills", false),
        (false, "/v3/openpositions", false),
        (true, "/receiver-read", true),
    ] {
        let body = Js::object(vec![("marker", Js::str("native-read"))]);
        let pending = rt.read(spot, endpoint, post, Rc::new(body)).unwrap();
        let result = h
            .drive(Box::pin(async {
                pending.await.map_err(|e| e.to_exchange_error())
            }))
            .unwrap();
        assert!(result.strict_equals(&response));
        assert_eq!(result.get("raw").unwrap().as_f64(), Some(f64::INFINITY));
        assert_eq!(
            result.get("text").unwrap().as_str().unwrap().as_ref(),
            &[0xd800]
        );
        let calls = h.http.calls(endpoint);
        assert_eq!(calls.len(), 1);
        let request = &calls[0];
        assert_eq!(request.method, if post { "POST" } else { "GET" });
        if post {
            assert_eq!(request.data, serde_json::json!("marker=native-read"));
        } else if endpoint == "/v3/fills" {
            assert_eq!(request.url, "/v3/fills?marker=native-read");
            assert!(request.params.is_null());
        } else {
            assert!(request.params.is_null());
        }
    }
}

#[test]
fn error_message_retains_truthy_values_and_uses_fallback_for_falsy_values() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let rt = &h.exchange.rt;
    for value in [
        Js::object(vec![]),
        Js::utf16(&[0xd800]),
        Js::Number(f64::INFINITY),
    ] {
        let error = Thrown::new(Js::object(vec![("message", value.clone())]));
        assert!(
            rt.error_message(&error, "fallback")
                .unwrap()
                .strict_equals(&value)
        );
    }
    for value in [
        Js::Null,
        Js::Undefined,
        Js::Bool(false),
        Js::str(""),
        Js::Number(-0.0),
        Js::Number(f64::NAN),
    ] {
        let error = Thrown::new(Js::object(vec![("message", value)]));
        assert_eq!(
            rt.error_message(&error, "fallback")
                .unwrap()
                .as_str()
                .unwrap()
                .as_ref(),
            &"fallback".encode_utf16().collect::<Vec<_>>()
        );
    }
    assert_eq!(
        rt.error_message(&Thrown::new(Js::Null), "fallback")
            .unwrap()
            .to_display(),
        "fallback"
    );
}

#[test]
fn fill_mapping_keeps_native_units_without_reading_unrelated_properties() {
    use safe_cex_exchange::js_runtime::value::from_retained_input;
    use safe_cex_types::retained_json::RetainedJsonInput;
    use std::cell::Cell;
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let input = RetainedJsonInput::parse_json(&r#"{"fill_id":"\ud800","symbol":"PF_XBTUSD","fillTime":"2023-11-14T22:13:19Z","side":"BUY","order_id":"\udc00","price":-0,"size":2.5,"fillType":"MAKER","realized_pnl":-0}"#.encode_utf16().collect::<Vec<_>>()).unwrap();
    let row = from_retained_input(input);
    let reads = Rc::new(Cell::new(0));
    let seen = reads.clone();
    row.define_accessor(
        "ignored",
        Some(Rc::new(move |_| {
            seen.set(seen.get() + 1);
            Err(Thrown::error("Error", "unread data must not execute"))
        })),
        None,
    )
    .unwrap();
    let mapped = h
        .exchange
        .rt
        .map_fills(Rc::new(Js::array(vec![row.clone()])))
        .unwrap();
    assert_eq!(mapped.len(), 1);
    assert_eq!(
        mapped[0]
            .get("id")
            .unwrap()
            .to_js_string()
            .unwrap()
            .as_ref(),
        &[0xd800]
    );
    assert_eq!(
        mapped[0]
            .get("orderId")
            .unwrap()
            .to_js_string()
            .unwrap()
            .as_ref(),
        &[0xdc00]
    );
    assert_eq!(
        mapped[0].get("price").unwrap().as_f64().unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(
        mapped[0]
            .get("realizedPnl")
            .unwrap()
            .as_f64()
            .unwrap()
            .to_bits(),
        (-0.0_f64).to_bits()
    );
    assert!(!mapped[0].has_own("commission"));
    assert_eq!(reads.get(), 0);
    // Number(BigInt) and String(Symbol) use the shared source builtins rather
    // than the stricter abstract ToNumber/ToString operations.
    let bigint = |text| {
        h.exchange
            .rt
            .global("BigInt")
            .unwrap()
            .call(Js::Undefined, vec![Js::str(text)])
            .unwrap()
    };
    row.set("price", bigint("9007199254740993")).unwrap();
    row.set("size", bigint("0")).unwrap();
    row.set("realized_pnl", bigint("-2")).unwrap();
    row.set("order_id", Js::SymbolIterator).unwrap();
    let mapped = h
        .exchange
        .rt
        .map_fills(Rc::new(Js::array(vec![row])))
        .unwrap();
    assert_eq!(
        mapped[0].get("price").unwrap().as_f64(),
        Some(9007199254740992.0)
    );
    assert_eq!(mapped[0].get("qty").unwrap().as_f64(), Some(0.0));
    assert_eq!(mapped[0].get("realizedPnl").unwrap().as_f64(), Some(-2.0));
    assert_eq!(
        mapped[0]
            .get("orderId")
            .unwrap()
            .to_js_string()
            .unwrap()
            .as_ref(),
        "Symbol(Symbol.iterator)"
            .encode_utf16()
            .collect::<Vec<_>>()
            .as_slice()
    );
    assert_eq!(reads.get(), 0);
}
